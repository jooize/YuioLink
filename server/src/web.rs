//! Route handlers, shared state, and embedded static assets.
//!
//! Two surfaces share the same logic:
//! - Human/terminal convenience: `POST /create`, `GET /:name`, `GET /:name+`.
//! - Canonical REST API under `/api/v1`: versioned, JSON, `201 + Location`,
//!   open CORS so a trusted third-party client can run against yuio.link.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use yuiolink_core::{
    ContentType, DEFAULT_ALLOWED_SCHEMES, Kind, detect_kind, has_scheme, validate_redirect,
};

use crate::config::{DEFAULT_TTL_SECS, MIN_TTL_SECS};
use crate::db::{self, NewLink};
use crate::{error::AppError, views};

/// Cap on stored content (~64 KB) — enough for a long URL or a Text snippet,
/// small enough to keep a single ephemeral row cheap.
const MAX_CONTENT_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub base_url: Arc<str>,
    pub max_ttl_secs: i64,
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
    // A trailing "+" requests the preview/info page instead of resolving (the
    // bit.ly convention). A preview is safe: no redirect, no hit counted, and it
    // reads through `get_link_live` so expired/exhausted links 404.
    if let Some(base) = name.strip_suffix('+') {
        let d = db::get_link_live(&state.pool, base)
            .await
            .map_err(AppError::internal)?
            .ok_or(AppError::NotFound)?;
        let short_url = format!("{}{}", state.base_url, base);
        let target = (d.kind == "redirect" && !d.encrypted).then_some(d.content.as_str());
        let kind_label = if d.kind == "text" { "Text" } else { "Redirect" };
        let page = views::preview_page(views::Preview {
            short_url: &short_url,
            kind: kind_label,
            encrypted: d.encrypted,
            target,
            hits: d.hits,
            created_at: &d.created_at,
            expires_at: &d.expires_at,
            max_uses: d.max_uses,
        });
        return Ok(Html(page.into_string()).into_response());
    }

    // Resolving consumes a use (atomically) for both kinds; expired or exhausted
    // links 404 because the UPDATE matches no row.
    let d = db::consume_link(&state.pool, &name)
        .await
        .map_err(AppError::internal)?
        .ok_or(AppError::NotFound)?;

    match d.kind.as_str() {
        "redirect" => {
            if d.encrypted {
                Ok(Html(views::encrypted_redirect_page(&d.content).into_string()).into_response())
            } else if validate_redirect(&d.content, DEFAULT_ALLOWED_SCHEMES).is_ok() {
                Ok(Redirect::to(&d.content).into_response())
            } else {
                // Stored an unexpected scheme somehow — refuse rather than reflect it.
                Err(AppError::NotFound)
            }
        }
        "text" => {
            if d.encrypted {
                Ok(Html(views::encrypted_text_page(&d.content).into_string()).into_response())
            } else {
                // Rendered as an escaped <pre> (text/plain) — never live HTML.
                Ok(Html(views::text_view_page(&d.content).into_string()).into_response())
            }
        }
        _ => Err(AppError::NotFound),
    }
}

// --------------------------------------------------------------------------
// Terminal-friendly creation (convenience surface)
// --------------------------------------------------------------------------

/// `curl yuio.link/create -d url=<url>` -> just the short URL as plain text
/// (or JSON when the client sends `Accept: application/json`).
///
/// POST, not GET: creating a link changes state, so it must not be a safe
/// method (RFC 9110). Unencrypted (a shell cannot do client-side crypto — that
/// is the CLI's job). Optional trailing `ttl=`/`uses=` params tune the lifetime
/// and burn-after-read count; the rest of the body is the content, whose kind is
/// auto-detected (so `--data-binary @file` becomes a Text link).
pub async fn create_plain(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let parsed = parse_plain_body(&body);

    if parsed.content.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "usage: curl -d url=<url> [-d ttl=1d] [-d uses=1] https://yuio.link/create\n",
        )
            .into_response();
    }

    let ttl_seconds = match parsed.ttl {
        Some(s) => match parse_duration(s) {
            Some(secs) => secs,
            None => {
                return (StatusCode::BAD_REQUEST, "invalid ttl (try 10m, 2h, 3d)\n").into_response();
            }
        },
        None => DEFAULT_TTL_SECS,
    };
    if let Err(msg) = check_ttl(ttl_seconds, state.max_ttl_secs) {
        return (StatusCode::BAD_REQUEST, format!("{msg}\n")).into_response();
    }

    let max_uses = match parsed.uses {
        Some(s) => match s.trim().parse::<i64>() {
            Ok(n) if n > 0 => Some(n),
            _ => return (StatusCode::BAD_REQUEST, "uses must be a positive integer\n").into_response(),
        },
        None => None,
    };

    // Build the row from the detected kind. Redirects are normalized + scheme-checked.
    let (kind, content, content_type): (Kind, String, Option<&str>) = match detect_kind(parsed.content)
    {
        Kind::Redirect => {
            let normalized = normalize_target(parsed.content);
            if let Err(e) = validate_redirect(&normalized, DEFAULT_ALLOWED_SCHEMES) {
                return (StatusCode::BAD_REQUEST, format!("{e}\n")).into_response();
            }
            (Kind::Redirect, normalized, None)
        }
        Kind::Text => (
            Kind::Text,
            parsed.content.to_string(),
            Some(ContentType::PlainText.as_str()),
        ),
    };

    if content.len() > MAX_CONTENT_BYTES {
        return (StatusCode::BAD_REQUEST, "content too large\n").into_response();
    }

    let inserted = match db::insert_link(
        &state.pool,
        NewLink {
            kind: kind.as_str(),
            content: &content,
            content_type,
            encrypted: false,
            ttl_seconds,
            max_uses,
        },
    )
    .await
    {
        Ok(inserted) => inserted,
        Err(e) => {
            tracing::error!(error = %e, "failed to insert link (plain)");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error\n").into_response();
        }
    };

    let url = format!("{}{}", state.base_url, inserted.name);

    let wants_json = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("application/json"));

    if wants_json {
        Json(CreateResponse {
            name: inserted.name,
            url,
            expires_at: inserted.expires_at,
        })
        .into_response()
    } else {
        (
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("{url}\n"),
        )
            .into_response()
    }
}

struct PlainBody<'a> {
    content: &'a str,
    ttl: Option<&'a str>,
    uses: Option<&'a str>,
}

/// Pull optional trailing `&ttl=…` / `&uses=…` params off a `curl -d` body, then
/// strip a leading `url=`/`text=`/`content=` field name. Only *trailing* option
/// pairs are consumed, so a redirect URL keeps its own `?a=1&b=2` query string
/// as long as `ttl`/`uses` come last (as `-d` appends them).
fn parse_plain_body(body: &str) -> PlainBody<'_> {
    let mut rest = body.trim();
    let mut ttl = None;
    let mut uses = None;

    while let Some(amp) = rest.rfind('&') {
        let last = &rest[amp + 1..];
        if let Some(v) = last.strip_prefix("ttl=") {
            ttl = Some(v);
        } else if let Some(v) = last.strip_prefix("uses=") {
            uses = Some(v);
        } else {
            break;
        }
        rest = rest[..amp].trim_end();
    }

    let content = rest
        .strip_prefix("url=")
        .or_else(|| rest.strip_prefix("text="))
        .or_else(|| rest.strip_prefix("content="))
        .unwrap_or(rest)
        .trim();

    PlainBody { content, ttl, uses }
}

/// Parse a short duration like `60`, `10m`, `2h`, or `3d` into seconds.
fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    let (num, mult) = match s.chars().last()? {
        's' => (&s[..s.len() - 1], 1),
        'm' => (&s[..s.len() - 1], 60),
        'h' => (&s[..s.len() - 1], 3600),
        'd' => (&s[..s.len() - 1], 86400),
        c if c.is_ascii_digit() => (s, 1),
        _ => return None,
    };
    num.trim().parse::<i64>().ok().filter(|&n| n >= 0).map(|n| n * mult)
}

/// Reject a TTL outside `[MIN_TTL_SECS, max_ttl]`.
fn check_ttl(ttl_seconds: i64, max_ttl: i64) -> Result<(), String> {
    if ttl_seconds < MIN_TTL_SECS {
        Err(format!("ttl must be at least {MIN_TTL_SECS} seconds"))
    } else if ttl_seconds > max_ttl {
        Err(format!("ttl must be at most {max_ttl} seconds"))
    } else {
        Ok(())
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
// REST API (canonical, versioned, JSON)
// --------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateRequest {
    pub kind: String,
    pub content: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub encrypted: bool,
    /// Lifetime in seconds; omitted -> [`DEFAULT_TTL_SECS`].
    #[serde(default)]
    pub ttl_seconds: Option<i64>,
    /// Burn-after-N-reads; omitted -> unlimited (within the TTL).
    #[serde(default)]
    pub max_uses: Option<i64>,
}

#[derive(Serialize)]
pub struct CreateResponse {
    pub name: String,
    pub url: String,
    pub expires_at: String,
}

#[derive(Serialize)]
pub struct ApiLink {
    pub name: String,
    pub kind: String,
    pub url: String,
    pub encrypted: bool,
    /// The destination, only for unencrypted redirects (the server cannot read
    /// an encrypted target).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The body for Text links (plaintext, or `yl1.` ciphertext for out-of-band
    /// decryption). Reading it here does not count against `max_uses`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub hits: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    pub created_at: String,
    pub expires_at: String,
}

pub enum ApiError {
    NotFound,
    BadRequest(String),
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            ),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// `POST /api/v1/links` — create a link. Returns `201 Created` with a
/// `Location` header pointing at the new resource.
pub async fn api_create_link(
    State(state): State<AppState>,
    Json(req): Json<CreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if req.content.trim().is_empty() {
        return Err(ApiError::BadRequest("content is required".into()));
    }
    if req.content.len() > MAX_CONTENT_BYTES {
        return Err(ApiError::BadRequest("content too large".into()));
    }

    let kind = match req.kind.as_str() {
        "redirect" => Kind::Redirect,
        "text" => Kind::Text,
        _ => return Err(ApiError::BadRequest("kind must be 'redirect' or 'text'".into())),
    };

    // Plaintext redirects must use an allowlisted scheme (blocks javascript:, data:, ...).
    if kind == Kind::Redirect && !req.encrypted {
        validate_redirect(&req.content, DEFAULT_ALLOWED_SCHEMES)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    }

    let ttl_seconds = req.ttl_seconds.unwrap_or(DEFAULT_TTL_SECS);
    check_ttl(ttl_seconds, state.max_ttl_secs).map_err(ApiError::BadRequest)?;

    if let Some(n) = req.max_uses
        && n <= 0
    {
        return Err(ApiError::BadRequest("max_uses must be a positive integer".into()));
    }

    let content_type = match kind {
        // Stored for forward-compatibility; minimal Text only renders plaintext.
        Kind::Text => {
            Some(ContentType::parse_or_default(req.content_type.as_deref().unwrap_or("")).as_str())
        }
        Kind::Redirect => None,
    };

    let inserted = db::insert_link(
        &state.pool,
        NewLink {
            kind: kind.as_str(),
            content: &req.content,
            content_type,
            encrypted: req.encrypted,
            ttl_seconds,
            max_uses: req.max_uses,
        },
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to insert link");
        ApiError::Internal
    })?;

    let url = format!("{}{}", state.base_url, inserted.name);
    let location = format!("{}api/v1/links/{}", state.base_url, inserted.name);
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(CreateResponse {
            name: inserted.name,
            url,
            expires_at: inserted.expires_at,
        }),
    ))
}

/// `GET /api/v1/links/:name` — read a link (the REST "expand"). Safe and
/// idempotent: it does NOT count a hit or consume `max_uses`. The destination is
/// omitted for encrypted links; Text bodies are returned verbatim.
pub async fn api_get_link(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiLink>, ApiError> {
    let d = db::get_link_live(&state.pool, &name)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to read link");
            ApiError::Internal
        })?
        .ok_or(ApiError::NotFound)?;

    let (target, content) = match d.kind.as_str() {
        "redirect" => ((!d.encrypted).then(|| d.content.clone()), None),
        "text" => (None, Some(d.content.clone())),
        _ => (None, None),
    };

    Ok(Json(ApiLink {
        url: format!("{}{}", state.base_url, d.name),
        name: d.name,
        kind: d.kind,
        encrypted: d.encrypted,
        target,
        content,
        hits: d.hits,
        max_uses: d.max_uses,
        created_at: d.created_at,
        expires_at: d.expires_at,
    }))
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
static_asset!(text_js, "text.js", "text/javascript; charset=utf-8");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_body_extracts_trailing_options() {
        let p = parse_plain_body("url=https://example.com&ttl=15m");
        assert_eq!(p.content, "https://example.com");
        assert_eq!(p.ttl, Some("15m"));
        assert_eq!(p.uses, None);

        let p = parse_plain_body("url=https://example.com&ttl=1d&uses=1");
        assert_eq!(p.content, "https://example.com");
        assert_eq!(p.ttl, Some("1d"));
        assert_eq!(p.uses, Some("1"));
    }

    #[test]
    fn parse_plain_body_keeps_url_query_string() {
        // The URL's own &-query survives; only trailing ttl/uses are peeled.
        let p = parse_plain_body("url=https://x.com/?a=1&b=2&ttl=2h");
        assert_eq!(p.content, "https://x.com/?a=1&b=2");
        assert_eq!(p.ttl, Some("2h"));
    }

    #[test]
    fn parse_plain_body_treats_file_dump_as_content() {
        let p = parse_plain_body("just some\nnotes from a file&ttl=1d");
        assert_eq!(p.content, "just some\nnotes from a file");
        assert_eq!(p.ttl, Some("1d"));
    }

    #[test]
    fn parse_duration_units() {
        assert_eq!(parse_duration("60"), Some(60));
        assert_eq!(parse_duration("15m"), Some(900));
        assert_eq!(parse_duration("2h"), Some(7200));
        assert_eq!(parse_duration("3d"), Some(259200));
        assert_eq!(parse_duration("nope"), None);
        assert_eq!(parse_duration(""), None);
    }
}
