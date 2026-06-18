//! Route handlers, shared state, and embedded static assets.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use yuiolink_core::{ContentType, DEFAULT_ALLOWED_SCHEMES, validate_redirect};

use crate::error::AppError;
use crate::{db, views};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub base_url: Arc<str>,
}

// --------------------------------------------------------------------------
// Pages
// --------------------------------------------------------------------------

pub async fn index() -> Html<String> {
    Html(views::index_page().into_string())
}

/// POST `/` only happens without JavaScript — link creation needs the browser.
pub async fn js_required() -> Html<String> {
    Html(views::js_required_page().into_string())
}

pub async fn resolve(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, AppError> {
    let link = db::get_link(&state.pool, &name)
        .await
        .map_err(AppError::internal)?
        .ok_or(AppError::NotFound)?;

    // Best-effort; a failed counter must not break the redirect.
    let _ = db::bump_hits(&state.pool, &name).await;

    match link.kind.as_str() {
        "redirect" => {
            if link.encrypted {
                Ok(Html(views::encrypted_redirect_page(&link.content).into_string()).into_response())
            } else if validate_redirect(&link.content, DEFAULT_ALLOWED_SCHEMES).is_ok() {
                Ok(Redirect::to(&link.content).into_response())
            } else {
                // Stored an unexpected scheme somehow — refuse rather than reflect it.
                Err(AppError::NotFound)
            }
        }
        // Paste viewing arrives with the paste feature.
        _ => Err(AppError::NotFound),
    }
}

// --------------------------------------------------------------------------
// Terminal-friendly creation
// --------------------------------------------------------------------------

/// `curl yuio.link/create -d url=<url>` -> just the short URL as plain text
/// (or JSON when the client sends `Accept: application/json`).
///
/// POST, not GET: creating a link changes state, so it must not be a safe
/// method (RFC 9110); `GET`/`QUERY` are reserved for safe look-ups. Unencrypted
/// (a shell cannot do client-side crypto — that is the CLI's job); the scheme
/// allowlist applies; a bare host defaults to `https://`. The body is the URL,
/// with an optional `url=` prefix so both `-d url=X` and `-d X` work.
pub async fn create_plain(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let trimmed = body.trim();
    let target = trimmed.strip_prefix("url=").unwrap_or(trimmed).trim();

    if target.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "usage: curl -d url=<url> https://yuio.link/create\n",
        )
            .into_response();
    }

    let normalized = normalize_target(target);
    if let Err(e) = validate_redirect(&normalized, DEFAULT_ALLOWED_SCHEMES) {
        return (StatusCode::BAD_REQUEST, format!("{e}\n")).into_response();
    }

    let name = match db::insert_unique(&state.pool, "redirect", &normalized, None, false).await {
        Ok(name) => name,
        Err(e) => {
            tracing::error!(error = %e, "failed to insert link (plain)");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error\n").into_response();
        }
    };

    let url = format!("{}{}", state.base_url, name);

    let wants_json = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("application/json"));

    if wants_json {
        Json(CreateResponse { name, url }).into_response()
    } else {
        (
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("{url}\n"),
        )
            .into_response()
    }
}

/// True if `s` already starts with an explicit `scheme:` (RFC 3986 scheme
/// characters, with no `/` before the colon so a path colon does not count).
fn has_scheme(s: &str) -> bool {
    match s.find(':') {
        Some(i) if i > 0 => {
            let scheme = &s[..i];
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        }
        _ => false,
    }
}

fn normalize_target(s: &str) -> String {
    if has_scheme(s) {
        s.to_string()
    } else {
        format!("https://{s}")
    }
}

// --------------------------------------------------------------------------
// JSON API
// --------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateRequest {
    pub kind: String,
    pub content: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub encrypted: bool,
}

#[derive(Serialize)]
pub struct CreateResponse {
    pub name: String,
    pub url: String,
}

pub enum ApiError {
    BadRequest(String),
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            ),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

pub async fn api_create_link(
    State(state): State<AppState>,
    Json(req): Json<CreateRequest>,
) -> Result<Json<CreateResponse>, ApiError> {
    if req.content.trim().is_empty() {
        return Err(ApiError::BadRequest("content is required".into()));
    }

    match req.kind.as_str() {
        "redirect" | "paste" => {}
        _ => return Err(ApiError::BadRequest("kind must be 'redirect' or 'paste'".into())),
    }
    let kind = req.kind.as_str();

    // Plaintext redirects must use an allowlisted scheme (blocks javascript:, data:, ...).
    if kind == "redirect" && !req.encrypted {
        validate_redirect(&req.content, DEFAULT_ALLOWED_SCHEMES)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    }

    let content_type = if kind == "paste" {
        Some(ContentType::parse_or_default(req.content_type.as_deref().unwrap_or("")).as_str())
    } else {
        None
    };

    let name = db::insert_unique(&state.pool, kind, &req.content, content_type, req.encrypted)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to insert link");
            ApiError::Internal
        })?;

    let url = format!("{}{}", state.base_url, name);
    Ok(Json(CreateResponse { name, url }))
}

// --------------------------------------------------------------------------
// Static assets (embedded in the binary so the package is self-contained)
// --------------------------------------------------------------------------

macro_rules! static_asset {
    ($name:ident, $file:literal, $mime:literal) => {
        pub async fn $name() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, $mime)],
                include_str!(concat!("../static/", $file)),
            )
        }
    };
}

static_asset!(app_css, "app.css", "text/css; charset=utf-8");
static_asset!(crypto_js, "crypto.js", "text/javascript; charset=utf-8");
static_asset!(app_js, "app.js", "text/javascript; charset=utf-8");
static_asset!(redirect_js, "redirect.js", "text/javascript; charset=utf-8");
