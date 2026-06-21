//! Route handlers, shared state, and embedded static assets.
//!
//! Three surfaces share one creation path ([`create_link`]):
//! - No-JS browser form: `POST /` -> a server-rendered result page.
//! - Terminal convenience: `POST /create` -> the short URL as text/JSON.
//! - Canonical REST API under `/api/v1`: versioned JSON, `201 + Location`, open
//!   CORS so a trusted third-party client can run against any backend.

use std::sync::Arc;

use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Json;
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
/// Cap on a link's view limit (one billion) — effectively unlimited, but bounded so a
/// request cannot ask for an absurd count. Mirrors the input's `max` on the page.
const MAX_USES: i64 = 1_000_000_000;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub base_url: Arc<str>,
    pub max_ttl_secs: i64,
    pub encryption_enabled: bool,
    pub api_base: Arc<str>,
}

// --------------------------------------------------------------------------
// Shared creation logic
// --------------------------------------------------------------------------

/// Why a create attempt failed: a client mistake (400) or our fault (500).
pub enum CreateError {
    BadRequest(String),
    Internal,
}

/// Validate the inputs and insert a link, shared by every creation surface.
///
/// `kind_choice` is the caller's explicit kind (`redirect`/`text`), or `auto`/
/// `None` to detect it. Trimming follows the rule "trim only a bare URL" — text
/// is stored verbatim (newlines and all); only a redirect target is trimmed and
/// normalized.
async fn create_link(
    state: &AppState,
    kind_choice: Option<&str>,
    raw_content: &str,
    ttl_seconds: i64,
    max_uses: Option<i64>,
    encrypted: bool,
    delete_token: Option<&str>,
) -> Result<db::InsertedLink, CreateError> {
    use CreateError::BadRequest;

    if encrypted && !state.encryption_enabled {
        return Err(BadRequest("Encryption is turned off on this server.".into()));
    }
    if raw_content.trim().is_empty() {
        return Err(BadRequest("Enter a link to redirect, or some text to share.".into()));
    }

    let kind = match kind_choice {
        None | Some("") | Some("auto") => detect_kind(raw_content),
        Some("redirect") => Kind::Redirect,
        Some("text") => Kind::Text,
        Some(_) => return Err(BadRequest("That is not a link type we recognize.".into())),
    };

    // Redirects are trimmed + normalized + scheme-checked (unless they are opaque
    // ciphertext); text is kept exactly as typed.
    let (content, content_type): (String, Option<&str>) = match kind {
        Kind::Redirect if encrypted => (raw_content.to_string(), None),
        Kind::Redirect => {
            let normalized = normalize_target(raw_content.trim());
            // Store the canonical (ASCII / IDNA-encoded) form so it is a valid
            // `Location` header value when the link resolves.
            let canonical = validate_redirect(&normalized, DEFAULT_ALLOWED_SCHEMES)
                .map_err(|e| BadRequest(e.to_string()))?;
            (canonical, None)
        }
        Kind::Text => (raw_content.to_string(), Some(ContentType::PlainText.as_str())),
    };

    if content.len() > MAX_CONTENT_BYTES {
        return Err(BadRequest("That is too large to share (the limit is 64 KB).".into()));
    }
    check_ttl(ttl_seconds, state.max_ttl_secs).map_err(BadRequest)?;
    if let Some(n) = max_uses {
        if n <= 0 {
            return Err(BadRequest("Enter a view limit of one or more.".into()));
        }
        if n > MAX_USES {
            return Err(BadRequest("The view limit can be at most 1,000,000,000.".into()));
        }
    }

    db::insert_link(
        &state.pool,
        NewLink {
            kind: kind.as_str(),
            content: &content,
            content_type,
            encrypted,
            ttl_seconds,
            max_uses,
            delete_token,
        },
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to insert link");
        CreateError::Internal
    })
}

// --------------------------------------------------------------------------
// Pages
// --------------------------------------------------------------------------

pub async fn index(State(state): State<AppState>) -> Html<String> {
    Html(views::index_page(state.encryption_enabled, &state.api_base, state.max_ttl_secs).into_string())
}

/// `POST /` — the no-JavaScript create path. A plain HTML form submits here and
/// gets a server-rendered result page. (With JS, `app.js` instead intercepts the
/// submit and uses the JSON API for an in-place result.) Always unencrypted: the
/// browser cannot seal without JS.
pub async fn form_create(State(state): State<AppState>, Form(form): Form<FormCreate>) -> Response {
    // Expiry: a preset ("600"/"3600"/"604800") or "custom" (a number + unit).
    let ttl_seconds = match form.ttl_seconds.as_deref() {
        Some("custom") => match parse_custom_ttl(form.ttl_custom.as_deref(), form.ttl_unit.as_deref())
        {
            Ok(secs) => secs,
            Err(msg) => return form_error(msg),
        },
        Some(s) => s.parse::<i64>().unwrap_or(DEFAULT_TTL_SECS),
        None => DEFAULT_TTL_SECS,
    };

    // Limit: unlimited (default), exactly 1, or a custom positive count. A "Specify"
    // left blank defaults to Once, matching the JS path.
    let max_uses = match form.limit.as_deref() {
        Some("1") => Some(1),
        Some("custom") => match form.limit_custom.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => match s.parse::<i64>() {
                Ok(n) => Some(n),
                Err(_) => return form_error("Enter the view limit as a whole number."),
            },
            _ => Some(1),
        },
        _ => None,
    };

    // No kind field: the server detects it (a URL is a redirect, else text).
    // No-JS form: no token issued (nowhere to keep it), so these links are not
    // API-deletable — fail closed.
    match create_link(&state, None, &form.content, ttl_seconds, max_uses, false, None).await {
        Ok(inserted) => {
            let url = format!("{}{}", state.base_url, inserted.name);
            let kind_label = match detect_kind(&form.content) {
                Kind::Redirect => "Redirect",
                Kind::Text => "Text",
            };
            Html(
                views::result_page(&url, kind_label, &inserted.expires_at, max_uses).into_string(),
            )
            .into_response()
        }
        Err(CreateError::BadRequest(msg)) => form_error(&msg),
        Err(CreateError::Internal) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(views::error_page(500, "Something went wrong.").into_string()),
        )
            .into_response(),
    }
}

/// Parse the no-JS "Custom" expiry (a number plus a minutes/hours/days unit) into
/// seconds. The accepted range is enforced afterward by [`check_ttl`].
fn parse_custom_ttl(value: Option<&str>, unit: Option<&str>) -> Result<i64, &'static str> {
    let n: i64 = value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("Enter a custom expiry.")?
        .parse()
        .map_err(|_| "Enter the expiry as a whole number.")?;
    let mult = match unit {
        Some("h") => 3600,
        Some("d") => 86400,
        _ => 60, // minutes (default)
    };
    Ok(n.saturating_mul(mult))
}

fn form_error(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Html(views::error_page(400, msg).into_string()),
    )
        .into_response()
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
/// auto-detected (so `--data-binary @file` becomes a Text link, verbatim).
pub async fn create_plain(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let parsed = parse_plain_body(&body);

    if parsed.content.trim().is_empty() {
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
                return (StatusCode::BAD_REQUEST, "That expiry is not valid. Try a value like 10m, 2h, or 3d.\n").into_response();
            }
        },
        None => DEFAULT_TTL_SECS,
    };

    let max_uses = match parsed.uses {
        Some(s) => match s.trim().parse::<i64>() {
            Ok(n) => Some(n),
            Err(_) => {
                return (StatusCode::BAD_REQUEST, "The view limit must be a whole number above zero.\n")
                    .into_response();
            }
        },
        None => None,
    };

    // Auto-detect kind (None); never encrypted.
    let inserted = match create_link(&state, None, parsed.content, ttl_seconds, max_uses, false, None).await
    {
        Ok(inserted) => inserted,
        Err(CreateError::BadRequest(msg)) => {
            return (StatusCode::BAD_REQUEST, format!("{msg}\n")).into_response();
        }
        Err(CreateError::Internal) => {
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
            delete_token: None,
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
/// pairs are consumed, so a redirect URL keeps its own `?a=1&b=2` query string as
/// long as `ttl`/`uses` come last (as `-d` appends them). The content body is not
/// trimmed here — text is kept verbatim; the redirect path trims it later.
fn parse_plain_body(body: &str) -> PlainBody<'_> {
    let mut rest = body;
    let mut ttl = None;
    let mut uses = None;

    loop {
        let trimmed = rest.trim_end();
        let Some(amp) = trimmed.rfind('&') else { break };
        let last = &trimmed[amp + 1..];
        if let Some(v) = last.strip_prefix("ttl=") {
            ttl = Some(v.trim());
        } else if let Some(v) = last.strip_prefix("uses=") {
            uses = Some(v.trim());
        } else {
            break;
        }
        rest = &trimmed[..amp];
    }

    let content = rest
        .strip_prefix("url=")
        .or_else(|| rest.strip_prefix("text="))
        .or_else(|| rest.strip_prefix("content="))
        .unwrap_or(rest);

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
    num.trim()
        .parse::<i64>()
        .ok()
        .filter(|&n| n >= 0)
        .map(|n| n * mult)
}

/// Reject a TTL outside `[MIN_TTL_SECS, max_ttl]`, phrased for humans in days/hours.
fn check_ttl(ttl_seconds: i64, max_ttl: i64) -> Result<(), String> {
    if ttl_seconds < MIN_TTL_SECS {
        Err(format!("Links must last at least {}.", views::humanize_duration(MIN_TTL_SECS)))
    } else if ttl_seconds > max_ttl {
        Err(format!("Links can last at most {}.", views::humanize_duration(max_ttl)))
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
// Form / REST request + response types
// --------------------------------------------------------------------------

/// The no-JS HTML form (`application/x-www-form-urlencoded`). The kind is detected
/// server-side, so there is no `kind` field.
#[derive(Deserialize)]
pub struct FormCreate {
    pub content: String,
    /// Expiry preset (`600`/`3600`/`604800`) or the sentinel `custom`.
    #[serde(default)]
    pub ttl_seconds: Option<String>,
    /// Custom-expiry amount (with [`Self::ttl_unit`]), used when `ttl_seconds` is `custom`.
    #[serde(default)]
    pub ttl_custom: Option<String>,
    /// Custom-expiry unit: `m`, `h`, or `d`.
    #[serde(default)]
    pub ttl_unit: Option<String>,
    /// Use limit: `unlimited`, `1`, or `custom`.
    #[serde(default)]
    pub limit: Option<String>,
    /// Custom use limit, used when `limit` is `custom`.
    #[serde(default)]
    pub limit_custom: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub kind: String,
    pub content: String,
    #[serde(default)]
    pub encrypted: bool,
    /// Lifetime in seconds; omitted -> [`DEFAULT_TTL_SECS`].
    #[serde(default)]
    pub ttl_seconds: Option<i64>,
    /// Burn-after-N-reads; omitted -> unlimited (within the TTL).
    #[serde(default)]
    pub max_uses: Option<i64>,
}
// Note: `content_type` is intentionally absent — minimal Text renders plaintext
// only. Rich Text (a later step, on a sandboxed origin) will reintroduce it with
// real handling. Unknown JSON fields are ignored, so older clients still work.

#[derive(Serialize)]
pub struct CreateResponse {
    pub name: String,
    pub url: String,
    pub expires_at: String,
    /// One-time secret that authorizes deleting this link (DELETE with
    /// `Authorization: Bearer <token>`). Returned only here; never stored
    /// anywhere the client doesn't put it. Absent when the link was made
    /// without a token (the `/create` convenience path).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_token: Option<String>,
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

impl From<CreateError> for ApiError {
    fn from(e: CreateError) -> Self {
        match e {
            CreateError::BadRequest(m) => ApiError::BadRequest(m),
            CreateError::Internal => ApiError::Internal,
        }
    }
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
/// `Location` header pointing at the new resource. This is the surface JS uses
/// for an in-place result (and the one a third-party client targets).
pub async fn api_create_link(
    State(state): State<AppState>,
    Json(req): Json<CreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if req.content.len() > MAX_CONTENT_BYTES {
        return Err(ApiError::BadRequest("That is too large to share (the limit is 64 KB).".into()));
    }

    let ttl_seconds = req.ttl_seconds.unwrap_or(DEFAULT_TTL_SECS);
    let delete_token = yuiolink_core::generate_token();
    let inserted = create_link(
        &state,
        Some(req.kind.as_str()),
        &req.content,
        ttl_seconds,
        req.max_uses,
        req.encrypted,
        Some(&delete_token),
    )
    .await?;

    let url = format!("{}{}", state.base_url, inserted.name);
    let location = format!("{}api/v1/links/{}", state.base_url, inserted.name);
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(CreateResponse {
            name: inserted.name,
            url,
            expires_at: inserted.expires_at,
            delete_token: Some(delete_token),
        }),
    ))
}

/// Pull a bearer token out of the `Authorization` header, if present.
fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// `DELETE /api/v1/links/:name` — delete a link, authorized by the per-link
/// secret from creation sent as `Authorization: Bearer <token>`. Returns
/// `204 No Content` on success. A missing/wrong token or unknown name both
/// return `404` so the endpoint reveals nothing about which links exist.
pub async fn api_delete_link(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let token = bearer_token(&headers).ok_or(ApiError::NotFound)?;
    let deleted = db::delete_link(&state.pool, &name, token).await.map_err(|e| {
        tracing::error!(error = %e, "failed to delete link");
        ApiError::Internal
    })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
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
    fn parse_plain_body_keeps_text_verbatim() {
        // A file dump keeps its internal newlines; only the trailing ttl is peeled.
        let p = parse_plain_body("just some\nnotes from a file\n&ttl=1d");
        assert_eq!(p.content, "just some\nnotes from a file\n");
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
