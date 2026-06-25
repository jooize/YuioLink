//! Route handlers, shared state, and embedded static assets.
//!
//! Three surfaces share one creation path ([`create_link`]):
//! - No-JS browser form: `POST /` -> a server-rendered result page.
//! - Terminal convenience: `POST /create` -> the short URL as text/JSON.
//! - Canonical REST API under `/api/v1`: versioned JSON, `201 + Location`,
//!   same-origin (no open CORS).
//!
//! Resolution is the always-preview model: `GET /:name` renders an interstitial
//! (or, for unlimited text, the text) and spends no use; consuming is a separate
//! POST that 303-redirects (Post/Redirect/Get), so unfurl crawlers cannot burn a
//! link.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use yuiolink_core::{
    ContentType, DEFAULT_ALLOWED_SCHEMES, Kind, detect_kind, has_scheme, validate_redirect,
};

use crate::config::{DEFAULT_TTL_SECS, MIN_TTL_SECS};
use crate::db::{self, LinkDetail, NewLink};
use crate::views::{self, Interstitial, RevealedTarget, RevealedView, Target};
use crate::{card, error::AppError, token, urlview};

/// Cap on stored content (~64 KB) — enough for a long URL or a Text snippet,
/// small enough to keep a single ephemeral row cheap.
const MAX_CONTENT_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub base_url: Arc<str>,
    pub max_ttl_secs: i64,
    /// Secret keying the HMAC reveal tokens (see [`crate::token`]).
    pub secret: Arc<[u8]>,
    /// Live name count per word-tier (1..=4 words), refreshed by the reaper. The
    /// create path reads it to choose the shortest available public name.
    pub occupancy: Arc<[AtomicU64; 4]>,
}

/// A point-in-time copy of the per-tier occupancy for one create.
fn occupancy_snapshot(occ: &[AtomicU64; 4]) -> db::Occupancy {
    std::array::from_fn(|i| occ[i].load(Ordering::Relaxed))
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Build the application router (without the trace layer, which `main` adds). The
/// always-preview model: `GET /:name` previews (no use spent); the POST endpoints
/// consume and 303 (Post/Redirect/Get).
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index).post(form_create))
        .route("/static/app.css", get(app_css))
        .route("/static/app.js", get(app_js))
        .route("/static/text.js", get(text_js))
        .nest("/api/v1", api_routes())
        .route("/create", post(create_plain))
        .route("/:name", get(resolve))
        .route("/:name/go", post(go))
        .route("/:name/reveal", post(reveal))
        .route("/:name/card.png", get(card_image))
        .fallback(not_found_fallback)
        .with_state(state)
}

/// The REST API. Same-origin only (no CORS): the page's own JS calls it, and the
/// "host your own browser frontend against yuio.link" rationale for open CORS was
/// dropped along with client-side encryption.
fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/links", post(api_create_link))
        .route("/links/:name", get(api_get_link).delete(api_delete_link))
}

async fn not_found_fallback() -> AppError {
    AppError::NotFound
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
    private: bool,
    delete_token: Option<&str>,
) -> Result<db::InsertedLink, CreateError> {
    use CreateError::BadRequest;

    if raw_content.trim().is_empty() {
        return Err(BadRequest("Enter a link to redirect, or some text to share.".into()));
    }

    let kind = match kind_choice {
        None | Some("") | Some("auto") => detect_kind(raw_content),
        Some("redirect") => Kind::Redirect,
        Some("text") => Kind::Text,
        Some(_) => return Err(BadRequest("That is not a link type we recognize.".into())),
    };

    // Redirects are trimmed + normalized + scheme-checked; text is kept exactly as
    // typed (newlines and all).
    let (content, content_type): (String, Option<&str>) = match kind {
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
    // A link is either unlimited (no limit) or single-use. Storage keeps a general
    // remaining-uses counter, but creation only ever sets one view, so reject any
    // other count rather than silently coercing it (which would surprise a caller
    // who asked for, say, five and got a link that dies after one).
    if let Some(n) = max_uses
        && n != 1
    {
        return Err(BadRequest(
            "A link is either unlimited or single-use: set the view limit to 1, or leave it off."
                .into(),
        ));
    }

    let occupancy = occupancy_snapshot(&state.occupancy);
    db::insert_link(
        &state.pool,
        NewLink {
            kind: kind.as_str(),
            content: &content,
            content_type,
            ttl_seconds,
            max_uses,
            private,
            delete_token,
        },
        &occupancy,
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
    Html(views::index_page(state.max_ttl_secs).into_string())
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

    // One control picks the link's type: public (short, guessable, reusable),
    // private (long unguessable, reusable), or once (long unguessable, single-use).
    let (max_uses, private) = match form.link_type.as_deref() {
        Some("once") => (Some(1), false),
        Some("private") => (None, true),
        _ => (None, false), // public (default)
    };

    // No kind field: the server detects it (a URL is a redirect, else text).
    // No-JS form: no token issued (nowhere to keep it), so these links are not
    // API-deletable — fail closed.
    match create_link(&state, None, &form.content, ttl_seconds, max_uses, private, None).await {
        Ok(inserted) => {
            let url = format!("{}{}", state.base_url, inserted.name);
            let kind_label = match detect_kind(&form.content) {
                Kind::Redirect => "Redirect",
                Kind::Text => "Text",
            };
            Html(
                views::result_page(
                    &url,
                    kind_label,
                    &inserted.expires_at,
                    max_uses,
                    private,
                    inserted.words,
                )
                .into_string(),
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

/// `GET /:name` — the mandatory preview. Spends **no** use. A live redirect (or
/// limited Text) renders the interstitial; unlimited Text renders immediately
/// (and counts a hit); a spent/withdrawn link is 410 Gone; an
/// expired/recycled/unknown name is 404.
///
/// A trailing `+` is accepted and ignored (the bit.ly "show me the preview"
/// convention): since every link already previews, `/:name+` just behaves like
/// `/:name`, so anyone reaching for `+` out of habit still lands here. Names are
/// alphanumeric words, so a `+` is never part of one.
pub async fn resolve(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Response {
    let name = name.strip_suffix('+').map(str::to_string).unwrap_or(name);
    // A visitor carrying a valid reveal capability (the `yl_reveal` cookie set when
    // they POSTed /:name/reveal) sees the revealed view right here at the clean
    // `/:name` URL — refresh/back safe, reading even a now-tombstoned row.
    if let Some(token) = reveal_cookie(&headers)
        && token::verify(&state.secret, &token, now_unix())
            .is_some_and(|n| n.eq_ignore_ascii_case(&name))
    {
        return revealed_view(&state, &name).await;
    }
    let live = match db::get_link_live(&state.pool, &name).await {
        Ok(v) => v,
        Err(e) => return AppError::internal(e).into_response(),
    };
    let Some(d) = live else {
        return tombstone_or_missing(&state, &name).await;
    };

    match (d.kind.as_str(), d.max_uses.is_some()) {
        // Unlimited Text has no external destination to vet — open it straight
        // away. This counts a hit (there is no use limit to gate).
        ("text", false) => {
            if let Err(e) = db::consume_link(&state.pool, &name).await {
                return AppError::internal(e).into_response();
            }
            Html(views::text_view_page(&d.content).into_string()).into_response()
        }
        // Redirects always preview; limited Text shows only that it exists.
        ("redirect", _) | ("text", true) => interstitial_response(&state, &d),
        _ => AppError::NotFound.into_response(),
    }
}

/// Render the interstitial for a live link without consuming it.
fn interstitial_response(state: &AppState, d: &LinkDetail) -> Response {
    let base_host = views::host_from_base(&state.base_url);
    let short_url = format!("{}{}", state.base_url, d.name);
    let markup = if d.kind == "redirect" {
        let url = urlview::parse(&d.content);
        views::interstitial_page(Interstitial {
            base_host,
            name: &d.name,
            short_url: &short_url,
            expires_at: &d.expires_at,
            max_uses: d.max_uses,
            target: Target::Redirect(&url),
        })
    } else {
        views::interstitial_page(Interstitial {
            base_host,
            name: &d.name,
            short_url: &short_url,
            expires_at: &d.expires_at,
            max_uses: d.max_uses,
            target: Target::TextSnippet,
        })
    };
    Html(markup.into_string()).into_response()
}

/// A name that is not live: a still-reserved tombstone (used-up or withdrawn) is
/// 410 Gone; an expired/recycled/unknown name is 404 Not Found.
async fn tombstone_or_missing(state: &AppState, name: &str) -> Response {
    match db::get_link_any(&state.pool, name).await {
        Ok(Some(d)) => (
            StatusCode::GONE,
            Html(views::gone_page(Some(&d.expires_at)).into_string()),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Html(views::not_found_page().into_string()),
        )
            .into_response(),
        Err(e) => AppError::internal(e).into_response(),
    }
}

/// `POST /:name/go` — consume an **unlimited** redirect and 303 to its
/// destination (Post/Redirect/Get keeps the back button clean). The link shape is
/// immutable, so we verify it before spending a hit: a non-matching shape returns
/// 404 without consuming.
pub async fn go(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    match db::get_link_live(&state.pool, &name).await {
        Ok(Some(d)) if d.kind == "redirect" && d.max_uses.is_none() => {}
        Ok(Some(_)) => return AppError::NotFound.into_response(),
        Ok(None) => return tombstone_or_missing(&state, &name).await,
        Err(e) => return AppError::internal(e).into_response(),
    }
    match db::consume_link(&state.pool, &name).await {
        Ok(Some(d)) if validate_redirect(&d.content, DEFAULT_ALLOWED_SCHEMES).is_ok() => {
            Redirect::to(&d.content).into_response()
        }
        // Stored an unexpected scheme somehow — refuse rather than reflect it.
        Ok(Some(_)) => AppError::NotFound.into_response(),
        // Died between the shape check and the consume.
        Ok(None) => tombstone_or_missing(&state, &name).await,
        Err(e) => AppError::internal(e).into_response(),
    }
}

/// `POST /:name/reveal` — consume a **limited** link (redirect or Text), then 303
/// to its token-gated revealed view. The use is spent here; the revealed GET only
/// re-renders, so refresh/back is safe.
pub async fn reveal(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    match db::get_link_live(&state.pool, &name).await {
        Ok(Some(d)) if d.max_uses.is_some() => {}
        Ok(Some(_)) => return AppError::NotFound.into_response(),
        Ok(None) => return tombstone_or_missing(&state, &name).await,
        Err(e) => return AppError::internal(e).into_response(),
    }
    match db::consume_link(&state.pool, &name).await {
        Ok(Some(d)) => {
            let t = token::mint(&state.secret, &d.name, now_unix() + token::TTL_SECS);
            // Carry the reveal capability in a short-lived, path-scoped cookie rather
            // than the URL, so the revealed page has a clean address and the token
            // never lands in browser history, referrers, or server logs. `Secure`
            // only when actually served over HTTPS, so local http dev still works.
            let secure = if state.base_url.starts_with("https") { "; Secure" } else { "" };
            let cookie = format!(
                "yl_reveal={t}; Path=/{}; Max-Age={}; HttpOnly; SameSite=Lax{secure}",
                d.name,
                token::TTL_SECS,
            );
            let mut resp = Redirect::to(&format!("/{}", d.name)).into_response();
            resp.headers_mut().append(
                header::SET_COOKIE,
                axum::http::HeaderValue::from_str(&cookie).expect("reveal cookie is ASCII"),
            );
            resp
        }
        Ok(None) => tombstone_or_missing(&state, &name).await,
        Err(e) => AppError::internal(e).into_response(),
    }
}

/// Pull the `yl_reveal` capability token out of the request `Cookie` header.
fn reveal_cookie(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';')
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == "yl_reveal")
        .map(|(_, v)| v.trim().to_string())
}

/// Render the revealed view for `name`, reading even a now-tombstoned row. The
/// caller (`resolve`) has already verified the `yl_reveal` capability, so this
/// re-renders without consuming — served at the clean `/:name` URL on a revealer's
/// revisit, refresh, or back-button.
async fn revealed_view(state: &AppState, name: &str) -> Response {
    let d = match db::get_link_any(&state.pool, name).await {
        Ok(Some(d)) => d,
        Ok(None) => return AppError::NotFound.into_response(),
        Err(e) => return AppError::internal(e).into_response(),
    };
    let base_host = views::host_from_base(&state.base_url);
    let markup = match d.kind.as_str() {
        "redirect" => {
            let url = urlview::parse(&d.content);
            views::revealed_page(RevealedView {
                base_host,
                name: &d.name,
                expires_at: &d.expires_at,
                target: RevealedTarget::Redirect { url: &url, href: &d.content },
            })
        }
        "text" => views::revealed_page(RevealedView {
            base_host,
            name: &d.name,
            expires_at: &d.expires_at,
            target: RevealedTarget::Text(&d.content),
        }),
        _ => return AppError::NotFound.into_response(),
    };
    Html(markup.into_string()).into_response()
}

/// `GET /:name/card.png` — the og:image share card for a live redirect. Spends no
/// use (crawlers fetch it). The card always shows the destination domain.
pub async fn card_image(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let d = match db::get_link_live(&state.pool, &name).await {
        Ok(Some(d)) if d.kind == "redirect" => d,
        // No card for non-redirects, or for spent/withdrawn/expired/unknown names.
        Ok(_) => return AppError::NotFound.into_response(),
        Err(e) => return AppError::internal(e).into_response(),
    };

    let url = urlview::parse(&d.content);
    let kicker = if d.max_uses == Some(1) {
        "One-time redirect"
    } else {
        "Ephemeral redirect"
    };
    let foot = format!(
        "expires {} · may change after",
        views::format_card_date(&d.expires_at)
    );

    match card::render_png(&card::Card {
        kicker,
        domain: &url.card_domain(),
        foot: &foot,
    }) {
        Some(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                // Immutable for the link's life; safe for crawlers to cache.
                (header::CACHE_CONTROL, "public, max-age=3600"),
            ],
            png,
        )
            .into_response(),
        None => AppError::internal("card render failed").into_response(),
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

    // Auto-detect kind (None).
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
            words: inserted.words,
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
    /// Link type: `public` (default), `private`, or `once`.
    #[serde(default)]
    pub link_type: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub kind: String,
    pub content: String,
    /// Lifetime in seconds; omitted -> [`DEFAULT_TTL_SECS`].
    #[serde(default)]
    pub ttl_seconds: Option<i64>,
    /// `1` makes the link single-use (burn after one view); omitted/null is
    /// unlimited within the TTL. Any other value is rejected — a link is either
    /// unlimited or single-use.
    #[serde(default)]
    pub max_uses: Option<i64>,
    /// Request a private (long, unguessable) name for an unlimited link. Ignored
    /// for single-use links, which always get the long name.
    #[serde(default)]
    pub private: bool,
}
// Note: `content_type` is intentionally absent — minimal Text renders plaintext
// only. Rich Text (a later step, on a sandboxed origin) will reintroduce it with
// real handling. Unknown JSON fields are ignored, so older clients still work.

#[derive(Serialize)]
pub struct CreateResponse {
    pub name: String,
    pub url: String,
    pub expires_at: String,
    /// Word count of the issued name. The page shows a note when a public link got
    /// more than one word because the short tiers are crowded.
    pub words: usize,
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
    /// The destination, for redirect links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The body for Text links. Reading it here does not count against `max_uses`.
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
        req.private,
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
            words: inserted.words,
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

/// `DELETE /api/v1/links/:name` — withdraw a link, authorized by the per-link
/// secret from creation sent as `Authorization: Bearer <token>`. Returns
/// `204 No Content` on success. Withdrawing does not free the name: it tombstones
/// the row (it then resolves as 410 Gone) and the name stays reserved until
/// expiry, so it cannot be silently repurposed. A missing/wrong token or unknown
/// name both return `404` so the endpoint reveals nothing about which links exist.
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
        "redirect" => (Some(d.content.clone()), None),
        "text" => (None, Some(d.content.clone())),
        _ => (None, None),
    };

    Ok(Json(ApiLink {
        url: format!("{}{}", state.base_url, d.name),
        name: d.name,
        kind: d.kind,
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
static_asset!(app_js, "app.js", "text/javascript; charset=utf-8");
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

    // ----------------------------------------------------------------------
    // HTTP-level flow tests (the always-preview model end to end)
    // ----------------------------------------------------------------------

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tower::ServiceExt;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    async fn test_state() -> AppState {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("yuiolink-web-{}-{n}.db", std::process::id()));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        AppState {
            pool: db::connect(path.to_str().unwrap()).await.unwrap(),
            base_url: Arc::from("http://yuio.test/"),
            max_ttl_secs: 604800,
            secret: Arc::from(b"test-secret".as_slice()),
            occupancy: Arc::new(std::array::from_fn(|_| std::sync::atomic::AtomicU64::new(0))),
        }
    }

    fn redirect(content: &str, max_uses: Option<i64>) -> NewLink<'_> {
        NewLink {
            kind: "redirect",
            content,
            content_type: None,
            ttl_seconds: 3600,
            max_uses,
            private: false,
            delete_token: Some("tok"),
        }
    }

    async fn send(state: &AppState, req: Request<Body>) -> (StatusCode, HeaderMap, String) {
        let resp = router(state.clone()).oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
    }

    fn get(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    fn post(uri: &str) -> Request<Body> {
        Request::builder().method("POST").uri(uri).body(Body::empty()).unwrap()
    }

    async fn hits(state: &AppState, name: &str) -> i64 {
        db::get_link_any(&state.pool, name).await.unwrap().unwrap().hits
    }

    #[tokio::test]
    async fn unlimited_redirect_previews_then_consumes() {
        let st = test_state().await;
        let l = db::insert_link(&st.pool, redirect("https://example.com/x", None), &db::EMPTY_OCCUPANCY).await.unwrap();

        // GET previews: 200 interstitial, no hit, full URL + amber Continue. A
        // crawler doing exactly this can never spend a use.
        let (s, _, body) = send(&st, get(&format!("/{}", l.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("Continue to example.com"), "interstitial body: {body}");
        assert_eq!(hits(&st, &l.name).await, 0);

        // POST /go consumes: 303 straight to the destination, hit counted.
        let (s, h, _) = send(&st, post(&format!("/{}/go", l.name))).await;
        assert_eq!(s, StatusCode::SEE_OTHER);
        assert_eq!(h.get("location").unwrap(), "https://example.com/x");
        assert_eq!(hits(&st, &l.name).await, 1);
    }

    #[tokio::test]
    async fn one_time_reveal_flow_then_gone() {
        let st = test_state().await;
        let l = db::insert_link(
            &st.pool,
            redirect("https://secret.example.com/zzz-gated-path", Some(1)),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();

        // GET previews domain-only: Reveal button, full path gated, no hit.
        let (s, _, body) = send(&st, get(&format!("/{}", l.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("Reveal Destination"));
        assert!(!body.contains("zzz-gated-path"), "path must be gated: {body}");
        assert_eq!(hits(&st, &l.name).await, 0);

        // POST /reveal consumes once and 303s to the clean /:name URL, with the
        // capability token in a Set-Cookie header (not the URL).
        let (s, h, _) = send(&st, post(&format!("/{}/reveal", l.name))).await;
        assert_eq!(s, StatusCode::SEE_OTHER);
        let loc = h.get("location").unwrap().to_str().unwrap().to_string();
        assert_eq!(loc, format!("/{}", l.name));
        let set_cookie = h.get("set-cookie").unwrap().to_str().unwrap();
        assert!(set_cookie.starts_with("yl_reveal="));
        let cookie = set_cookie.split(';').next().unwrap().to_string();
        assert_eq!(hits(&st, &l.name).await, 1);

        // The revealed GET (carrying the cookie) shows the full URL and does NOT
        // consume again.
        let revealed_get = |c: &str| {
            Request::builder()
                .uri(loc.as_str())
                .header("cookie", c)
                .body(Body::empty())
                .unwrap()
        };
        let (s, _, body) = send(&st, revealed_get(&cookie)).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("zzz-gated-path"), "revealed body: {body}");
        send(&st, revealed_get(&cookie)).await; // re-render is safe
        assert_eq!(hits(&st, &l.name).await, 1);

        // Without the cookie the link is spent: 410 Gone, content not shown.
        let (s, _, body) = send(&st, get(&format!("/{}", l.name))).await;
        assert_eq!(s, StatusCode::GONE);
        assert!(body.contains("410"));
        assert!(!body.contains("zzz-gated-path"));
    }

    #[tokio::test]
    async fn forged_reveal_cookie_does_not_reveal() {
        let st = test_state().await;
        let l = db::insert_link(
            &st.pool,
            redirect("https://example.com/zzz-gated", Some(1)),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        // A forged cookie fails the HMAC check, so /:name falls through to the
        // normal preview: 200, domain-only, the gated path NOT shown, no consume.
        let forged = Request::builder()
            .uri(format!("/{}", l.name))
            .header("cookie", "yl_reveal=forged.sig")
            .body(Body::empty())
            .unwrap();
        let (s, _, body) = send(&st, forged).await;
        assert_eq!(s, StatusCode::OK);
        assert!(!body.contains("zzz-gated"), "forged cookie must not reveal: {body}");
        assert_eq!(hits(&st, &l.name).await, 0);
    }

    #[tokio::test]
    async fn unlimited_text_opens_immediately_and_counts_hit() {
        let st = test_state().await;
        let l = db::insert_link(
            &st.pool,
            NewLink {
                kind: "text",
                content: "hello plaintext",
                content_type: Some("text/plain"),
                ttl_seconds: 3600,
                max_uses: None,
                private: false,
                delete_token: None,
            },
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        let (s, _, body) = send(&st, get(&format!("/{}", l.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("hello plaintext"));
        assert_eq!(hits(&st, &l.name).await, 1);
    }

    #[tokio::test]
    async fn trailing_plus_and_any_case_resolve_to_canonical_preview() {
        let st = test_state().await;
        let l = db::insert_link(&st.pool, redirect("https://example.com/x", None), &db::EMPTY_OCCUPANCY).await.unwrap();

        // A trailing "+" is accepted and behaves like the bare name (no use spent).
        let (s, _, body) = send(&st, get(&format!("/{}+", l.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("Continue to example.com"));
        assert_eq!(hits(&st, &l.name).await, 0);

        // Typing a different case resolves (NOCASE) and the preview shows the
        // canonical stored name, not what was typed.
        let typed = l.name.to_uppercase();
        assert_ne!(typed, l.name);
        let (s, _, body) = send(&st, get(&format!("/{typed}"))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains(&format!(r#"<span class="name">{}</span>"#, l.name)));
    }

    #[tokio::test]
    async fn unknown_name_is_404() {
        let st = test_state().await;
        let (s, _, body) = send(&st, get("/doesnotexist")).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert!(body.contains("404"));
    }

    #[tokio::test]
    async fn api_rejects_multi_use_but_allows_single_use() {
        let st = test_state().await;
        let create = |max: i64| {
            Request::builder()
                .method("POST")
                .uri("/api/v1/links")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"kind":"redirect","content":"https://example.com","max_uses":{max}}}"#
                )))
                .unwrap()
        };
        // N > 1 is refused with 400 — no silent coercion to single-use.
        let (s, _, _) = send(&st, create(5)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        // Single-use is accepted.
        let (s, _, _) = send(&st, create(1)).await;
        assert_eq!(s, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn withdraw_via_api_then_gone() {
        let st = test_state().await;
        let l = db::insert_link(&st.pool, redirect("https://example.com", None), &db::EMPTY_OCCUPANCY).await.unwrap();

        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/links/{}", l.name))
            .header("authorization", "Bearer tok")
            .body(Body::empty())
            .unwrap();
        let (s, _, _) = send(&st, req).await;
        assert_eq!(s, StatusCode::NO_CONTENT);

        let (s, _, body) = send(&st, get(&format!("/{}", l.name))).await;
        assert_eq!(s, StatusCode::GONE);
        assert!(body.contains("withdrawn"));
    }

    #[tokio::test]
    async fn card_png_renders_and_spends_no_use() {
        let st = test_state().await;
        let l = db::insert_link(&st.pool, redirect("https://example.com/blog", None), &db::EMPTY_OCCUPANCY).await.unwrap();

        // A crawler hitting the interstitial and the card never bumps hits.
        send(&st, get(&format!("/{}", l.name))).await;
        let resp = router(st.clone())
            .oneshot(get(&format!("/{}/card.png", l.name)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");
        let png = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&png[1..4], b"PNG");
        assert_eq!(hits(&st, &l.name).await, 0);

        // Text links have no card.
        let t = db::insert_link(
            &st.pool,
            NewLink {
                kind: "text",
                content: "hi",
                content_type: Some("text/plain"),
                ttl_seconds: 3600,
                max_uses: None,
                private: false,
                delete_token: None,
            },
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        let (s, _, _) = send(&st, get(&format!("/{}/card.png", t.name))).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn idn_lookalike_shows_warning_and_punycode() {
        let st = test_state().await;
        // аpple.com with a Cyrillic 'а' — a homograph attack.
        let host = idna::domain_to_ascii("аpple.com").unwrap();
        let l = db::insert_link(
            &st.pool,
            redirect(&format!("https://{host}/login"), None),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        let (s, _, body) = send(&st, get(&format!("/{}", l.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("Lookalike domain"));
        assert!(body.contains(&host), "punycode must be shown: {body}");
        assert!(body.contains("Continue Anyway"));
    }
}
