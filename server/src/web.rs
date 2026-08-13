//! Route handlers, shared state, and embedded static assets.
//!
//! Three surfaces share one creation path ([`create_link`]):
//! - No-JS browser form: `POST /` -> a server-rendered result page.
//! - Terminal convenience: `POST /create` -> the short URL as text/JSON.
//! - Canonical REST API under `/api/v0`: versioned JSON, `201 + Location`,
//!   same-origin (no open CORS).
//!
//! Resolution is the always-preview model: `GET /:name` renders an interstitial
//! (or, for unlimited text, the text) and spends no use; consuming is a separate
//! POST that 303-redirects (Post/Redirect/Get), so unfurl crawlers cannot burn a
//! link.

use std::borrow::Cow;
use std::collections::BTreeMap;
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
use crate::ratelimit::RateLimiter;
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
    /// Create-path rate limiter (per client IP). Creation only — resolution is
    /// never limited (see [`crate::ratelimit`]).
    pub limiter: Arc<RateLimiter>,
}

/// The message every over-limit create surface answers 429 with.
const RATE_LIMIT_MSG: &str = "You are creating links too quickly. Wait a moment and try again.";

/// Key a client for rate limiting. Behind the reverse proxy every TCP peer is
/// localhost, so the client is the *last* `X-Forwarded-For` entry — the one our
/// own proxy appended (earlier entries are attacker-controllable). No header
/// (tests, direct local hits) falls back to one shared bucket — fail closed.
fn client_key(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .map(|ip| ip.trim().to_string())
        .unwrap_or_default()
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
        .route("/healthz", get(healthz))
        .route("/static/app.css", get(app_css))
        .route("/static/app.js", get(app_js))
        .route("/static/text.js", get(text_js))
        .route("/static/preview.js", get(preview_js))
        .route("/wordlist.txt", get(wordlist_txt))
        .route("/robots.txt", get(robots_txt))
        // Any bare segment added here shadows `/{name}` and must also go in
        // `yuiolink_core::RESERVED_NAMES`, or a link issued under that word is
        // unreachable for its whole life (a GET lands on this route, not the link).
        .route("/help", get(help))
        .route("/colophon", get(colophon))
        .route("/stats", get(stats))
        .nest("/api/v0", api_routes())
        .route("/create", post(create_plain))
        .route("/{name}", get(resolve))
        .route("/{name}/go", post(go))
        .route("/{name}/reveal", post(reveal))
        .route("/{name}/card.png", get(card_image))
        .fallback(not_found_fallback)
        // Inside the router, not around it: every response the site can produce —
        // pages, assets, API JSON, errors — leaves through here with the same
        // headers, and the tests below exercise the real thing.
        .layer(axum::middleware::from_fn(crate::security::headers))
        .with_state(state)
}

/// The REST API. Same-origin only (no CORS): the page's own JS calls it, and the
/// "host your own browser frontend against yuio.link" rationale for open CORS was
/// dropped along with client-side encryption.
fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/links", post(api_create_link))
        .route("/links/{name}", get(api_get_link).delete(api_delete_link))
        .route("/openapi.yaml", get(openapi_yaml))
}

async fn not_found_fallback() -> AppError {
    AppError::NotFound
}

/// `GET /healthz` — deploy/update health probe. Touches the database so a failed
/// migration or unreadable file reads as unhealthy, not merely "process is up".
async fn healthz(State(state): State<AppState>) -> Response {
    match sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        Ok(_) => (StatusCode::OK, "ok\n").into_response(),
        Err(e) => AppError::internal(e).into_response(),
    }
}

// --------------------------------------------------------------------------
// Shared creation logic
// --------------------------------------------------------------------------

/// One thing wrong with one field of a create request. `field` names the JSON
/// field of the canonical API (`content`, `kind`, `ttl_seconds`, `max_uses`);
/// the other surfaces just show the messages. Borrowed for the fields the server
/// knows by name, owned for the ones a caller invents.
#[derive(Serialize)]
pub struct FieldError {
    pub field: Cow<'static, str>,
    pub message: String,
}

/// Join the messages of a batch of field errors into one human line/paragraph.
fn join_messages(errors: &[FieldError], sep: &str) -> String {
    errors
        .iter()
        .map(|e| e.message.as_str())
        .collect::<Vec<_>>()
        .join(sep)
}

/// Why a create attempt failed: client mistakes (400, all of them at once) or
/// our fault (500).
pub enum CreateError {
    BadRequest(Vec<FieldError>),
    Internal,
}

/// Validate the inputs and insert a link, shared by every creation surface.
///
/// Validation does not fail fast: every field is checked and all the errors come
/// back together, so a caller fixes one round-trip, not one mistake per round-trip.
/// `ttl_seconds` and `max_uses` arrive as `Result`s so a surface's own parse
/// failure ("2x" is not a duration) joins the same batch as the field checks.
///
/// `kind_choice` is the caller's explicit kind (`redirect`/`text`), or `auto`/
/// `None` to detect it. Trimming follows the rule "trim only a bare URL" — text
/// is stored verbatim (newlines and all); only a redirect target is trimmed and
/// normalized.
async fn create_link(
    state: &AppState,
    kind_choice: Option<&str>,
    raw_content: &str,
    ttl_seconds: Result<i64, String>,
    max_uses: Result<Option<i64>, String>,
    secret: bool,
    delete_token: Option<&str>,
) -> Result<db::InsertedLink, CreateError> {
    let mut errors: Vec<FieldError> = Vec::new();
    let mut fail = |field: &'static str, message: String| {
        errors.push(FieldError {
            field: Cow::Borrowed(field),
            message,
        });
    };

    // An unknown kind is its own error; detection still classifies the content so
    // the remaining checks run against something sensible.
    let kind = match kind_choice {
        None | Some("") | Some("auto") => detect_kind(raw_content),
        Some("redirect") => Kind::Redirect,
        Some("text") => Kind::Text,
        Some(_) => {
            fail("kind", "That is not a link type we recognize.".into());
            detect_kind(raw_content)
        }
    };

    // Content: empty, else (for redirects) trimmed + normalized + scheme-checked —
    // text is kept exactly as typed (newlines and all) — then the size cap. For a
    // redirect the canonical (ASCII / IDNA-encoded) form is stored so it is a
    // valid `Location` header value when the link resolves.
    let mut validated: Option<(String, Option<&str>)> = None;
    if raw_content.trim().is_empty() {
        fail(
            "content",
            "Enter a link to redirect, or some text to share.".into(),
        );
    } else {
        match kind {
            Kind::Redirect => {
                let normalized = normalize_target(raw_content.trim());
                match validate_redirect(&normalized, DEFAULT_ALLOWED_SCHEMES) {
                    Ok(canonical) => validated = Some((canonical, None)),
                    Err(e) => fail("content", e.to_string()),
                }
            }
            Kind::Text => {
                validated = Some((
                    raw_content.to_string(),
                    Some(ContentType::PlainText.as_str()),
                ));
            }
        }
    }
    if validated
        .as_ref()
        .is_some_and(|(c, _)| c.len() > MAX_CONTENT_BYTES)
    {
        validated = None;
        fail(
            "content",
            "That is too large to share (the limit is 64 KB).".into(),
        );
    }

    if let Err(msg) = ttl_seconds
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|&t| check_ttl(t, state.max_ttl_secs))
    {
        fail("ttl_seconds", msg);
    }

    // A link is either unlimited (no limit) or single-use. Storage keeps a general
    // remaining-uses counter, but creation only ever sets one view, so reject any
    // other count rather than silently coercing it (which would surprise a caller
    // who asked for, say, five and got a link that dies after one).
    match &max_uses {
        Ok(Some(n)) if *n != 1 => fail(
            "max_uses",
            "A link is either unlimited or single-use: set the view limit to 1, or leave it off."
                .into(),
        ),
        Ok(_) => {}
        Err(msg) => fail("max_uses", msg.clone()),
    }

    if !errors.is_empty() {
        return Err(CreateError::BadRequest(errors));
    }
    // No errors, so every piece validated: safe to unwrap the collected parts.
    let (content, content_type) = validated.expect("validated content");
    let ttl_seconds = ttl_seconds.expect("validated ttl");
    let max_uses = max_uses.expect("validated max_uses");

    let occupancy = occupancy_snapshot(&state.occupancy);
    db::insert_link(
        &state.pool,
        NewLink {
            kind: kind.as_str(),
            content: &content,
            content_type,
            ttl_seconds,
            max_uses,
            secret,
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
pub async fn form_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<FormCreate>,
) -> Response {
    if !state.limiter.allow(&client_key(&headers)) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Html(views::error_page(429, RATE_LIMIT_MSG).into_string()),
        )
            .into_response();
    }
    // Expiry: a filled exact field (number + unit) beats the slider stop; the
    // slider posts an index into TTL_STOPS. Legacy preset values still parse. A
    // parse failure is carried as an Err so it reports alongside the other
    // fields' errors rather than pre-empting them.
    let has_custom = form
        .ttl_custom
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let ttl_seconds = if has_custom || form.ttl_seconds.as_deref() == Some("custom") {
        parse_custom_ttl(form.ttl_custom.as_deref(), form.ttl_unit.as_deref())
            .map_err(str::to_string)
    } else if let Some(i) = form
        .ttl_stop
        .as_deref()
        .and_then(|s| s.parse::<usize>().ok())
    {
        Ok(TTL_STOPS.get(i).copied().unwrap_or(DEFAULT_TTL_SECS))
    } else {
        Ok(match form.ttl_seconds.as_deref() {
            Some(s) => s.parse::<i64>().unwrap_or(DEFAULT_TTL_SECS),
            None => DEFAULT_TTL_SECS,
        })
    };

    // One control picks the link's type: public (short, guessable, reusable),
    // secret (long unguessable, reusable), or once (long unguessable, single-use).
    let (max_uses, secret) = match form.link_type.as_deref() {
        Some("once") => (Some(1), false),
        Some("secret") => (None, true),
        _ => (None, false), // public (default)
    };

    // No kind field: the server detects it (a URL is a redirect, else text).
    // No-JS form: no token issued (nowhere to keep it), so these links are not
    // API-deletable — fail closed.
    // Captured before the Result is moved into create_link, so the "as Text"
    // offer can reuse the expiry the user actually chose.
    let redo_ttl = ttl_seconds.as_ref().copied().unwrap_or(DEFAULT_TTL_SECS);

    match create_link(
        &state,
        form.kind.as_deref(),
        &form.content,
        ttl_seconds,
        Ok(max_uses),
        secret,
        None,
    )
    .await
    {
        Ok(inserted) => {
            let url = format!("{}{}", state.base_url, inserted.name);
            let forced_text = form.kind.as_deref() == Some("text");
            let kind_label = match (forced_text, detect_kind(&form.content)) {
                (true, _) | (false, Kind::Text) => "Text",
                (false, Kind::Redirect) => "Redirect",
            };
            // The Text offer only makes sense after a Redirect: a link stored as
            // Text has no other kind to become, and neither does plain prose.
            let redo = (kind_label == "Redirect").then(|| views::ResultRedo {
                content: &form.content,
                ttl_seconds: redo_ttl,
                link_type: form.link_type.as_deref().unwrap_or("public"),
            });
            Html(
                views::result_page(
                    &url,
                    kind_label,
                    &inserted.expires_at,
                    max_uses,
                    secret,
                    inserted.words,
                    redo.as_ref(),
                )
                .into_string(),
            )
            .into_response()
        }
        Err(CreateError::BadRequest(errors)) => form_error(&errors),
        Err(CreateError::Internal) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(views::error_page(500, "Something went wrong.").into_string()),
        )
            .into_response(),
    }
}

/// The expiry slider's stop ladder — fine steps in the minutes range, coarser
/// through hours and days. Index 7 (1 hour) is the form's default; must match
/// `TTL_STOPS` in `app.js` and the slider's `min`/`max` in `views.rs`.
const TTL_STOPS: [i64; 17] = [
    60, 120, 300, 600, 900, 1800, 2700, 3600, 7200, 10800, 21600, 43200, 86400, 172800, 259200,
    432000, 604800,
];

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

/// The no-JS form's 400 page: every collected error, one line each.
fn form_error(errors: &[FieldError]) -> Response {
    let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
    (
        StatusCode::BAD_REQUEST,
        Html(views::error_page_list(400, &messages).into_string()),
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
    // `/:name` URL. That view redacts the content from the server on this first
    // render, so a second visit within the token's window reads as 410 Gone.
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
        // away. Nothing is spent: `uses` only gates a one-time link, and there
        // is no per-link view counter to bump. The aggregate tally still records
        // that a link resolved.
        ("text", false) => {
            db::bump(&state.pool, db::Stat::Opened).await;
            let base_host = views::host_from_base(&state.base_url);
            Html(views::text_view_page(base_host, &d.name, &d.content).into_string())
                .into_response()
        }
        // Redirects always preview; limited Text shows only that it exists.
        ("redirect", _) | ("text", true) => interstitial_response(&state, &d).await,
        _ => AppError::NotFound.into_response(),
    }
}

/// Render the interstitial for a live link without consuming it. The render is
/// tallied anonymously (`Stat::Previewed`) — day-granular and per-metric, which
/// is where the retired per-link counter's job now lives.
async fn interstitial_response(state: &AppState, d: &LinkDetail) -> Response {
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
    db::bump(&state.pool, db::Stat::Previewed).await;
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
/// to its token-gated revealed view. The use is spent here; the destination or
/// content itself is deleted from the server when the revealed GET actually
/// renders it, so refresh/back after that first render reads as 410 Gone.
pub async fn reveal(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    match db::get_link_live(&state.pool, &name).await {
        Ok(Some(d)) if d.max_uses.is_some() => {}
        Ok(Some(_)) => return AppError::NotFound.into_response(),
        Ok(None) => return tombstone_or_missing(&state, &name).await,
        Err(e) => return AppError::internal(e).into_response(),
    }
    match db::consume_link(&state.pool, &name).await {
        Ok(Some(d)) => {
            db::bump(&state.pool, db::Stat::Revealed).await;
            let t = token::mint(&state.secret, &d.name, now_unix() + token::TTL_SECS);
            // Carry the reveal capability in a short-lived, path-scoped cookie rather
            // than the URL, so the revealed page has a clean address and the token
            // never lands in browser history, referrers, or server logs. `Secure`
            // only when actually served over HTTPS, so local http dev still works.
            let secure = if state.base_url.starts_with("https") {
                "; Secure"
            } else {
                ""
            };
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

/// Render the revealed view for `name`, deleting its content from the server as
/// part of this same read. The caller (`resolve`) has already verified the
/// `yl_reveal` capability; this is the one render that actually shows the
/// destination or content, so a repeat visit (refresh, back-button, or a second
/// tab with the same cookie) finds it already redacted and reads as 410 Gone.
async fn revealed_view(state: &AppState, name: &str) -> Response {
    let d = match db::reveal_and_redact(&state.pool, name).await {
        Ok(Some(d)) => d,
        Ok(None) => return tombstone_or_missing(state, name).await,
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
                target: RevealedTarget::Redirect {
                    url: &url,
                    href: &d.content,
                },
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
    let domain = url.card_domain();
    // Date and clock, no "may change after" tail: the expiry is the fact worth
    // the space, and an hour-long link needs the minute to mean anything.
    let date = views::format_card_date(&d.expires_at);
    let foot = match views::format_card_time(&d.expires_at) {
        Some(time) => format!("expires {date} · {time}"),
        None => format!("expires {date}"),
    };

    // Rasterizing is synchronous CPU work; keep it off the async workers so a
    // burst of crawler card fetches cannot stall every other request.
    let png = tokio::task::spawn_blocking(move || {
        card::render_png(&card::Card {
            kicker,
            domain: &domain,
            foot: &foot,
        })
    })
    .await
    .ok()
    .flatten();

    match png {
        Some(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                // Immutable for the link's life; safe for crawlers to cache.
                (header::CACHE_CONTROL, "public, max-age=3600"),
                // The card exists to be shown by other sites, so it opts out of
                // the same-origin resource policy the rest of the site takes.
                (
                    header::HeaderName::from_static("cross-origin-resource-policy"),
                    "cross-origin",
                ),
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
    if !state.limiter.allow(&client_key(&headers)) {
        return (StatusCode::TOO_MANY_REQUESTS, format!("{RATE_LIMIT_MSG}\n")).into_response();
    }

    let parsed = parse_plain_body(&body);

    if parsed.content.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "usage: curl -d url=<url> [-d ttl=1d] [-d uses=1] https://yuio.link/create\n",
        )
            .into_response();
    }

    // Parse failures become Errs that report together with the field checks —
    // a bad ttl AND a bad url come back as two lines of one 400, not two round-trips.
    let ttl_seconds = match parsed.ttl {
        Some(s) => parse_duration(s).ok_or_else(|| {
            "That expiry is not valid. Try a value like 10m, 2h, or 3d.".to_string()
        }),
        None => Ok(DEFAULT_TTL_SECS),
    };

    let max_uses = match parsed.uses {
        Some(s) => s
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| "The view limit must be a whole number above zero.".to_string()),
        None => Ok(None),
    };

    // Auto-detect kind (None).
    let inserted = match create_link(
        &state,
        None,
        parsed.content,
        ttl_seconds,
        max_uses,
        false,
        None,
    )
    .await
    {
        Ok(inserted) => inserted,
        Err(CreateError::BadRequest(errors)) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("{}\n", join_messages(&errors, "\n")),
            )
                .into_response();
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
        // checked_mul: an absurd count must read as invalid, not wrap around
        // into some accidental in-range TTL.
        .and_then(|n| n.checked_mul(mult))
}

/// Reject a TTL outside `[MIN_TTL_SECS, max_ttl]`, phrased for humans in days/hours.
fn check_ttl(ttl_seconds: i64, max_ttl: i64) -> Result<(), String> {
    if ttl_seconds < MIN_TTL_SECS {
        Err(format!(
            "Links must last at least {}.",
            views::humanize_duration(MIN_TTL_SECS)
        ))
    } else if ttl_seconds > max_ttl {
        Err(format!(
            "Links can last at most {}.",
            views::humanize_duration(max_ttl)
        ))
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
    /// Legacy expiry preset (`600`/`3600`/`604800`) or the sentinel `custom`.
    #[serde(default)]
    pub ttl_seconds: Option<String>,
    /// Expiry slider stop — an index into [`TTL_STOPS`].
    #[serde(default)]
    pub ttl_stop: Option<String>,
    /// Custom-expiry amount (with [`Self::ttl_unit`]), used when `ttl_seconds` is `custom`.
    #[serde(default)]
    pub ttl_custom: Option<String>,
    /// Custom-expiry unit: `m`, `h`, or `d`.
    #[serde(default)]
    pub ttl_unit: Option<String>,
    /// Link type: `public` (default), `secret`, or `once`.
    #[serde(default)]
    pub link_type: Option<String>,
    /// Force the kind instead of letting the server detect it. Only the result
    /// page's "share the address as a text link instead" sends it; absent or `auto`
    /// means detection, which is what the form itself always does.
    #[serde(default)]
    pub kind: Option<String>,
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
    /// Request a secret (long, unguessable) name for an unlimited link. Ignored
    /// for single-use links, which always get the long name.
    #[serde(default)]
    pub secret: bool,
    /// Everything the API does not know, collected rather than ignored.
    ///
    /// Ignoring an unknown field is silent by nature, and silence is dangerous
    /// on exactly this endpoint: a caller who asks for an unguessable name with
    /// a field we drop — the old `private`, a typo like `secrit`, a hopeful
    /// `expires_at` — is handed a short, guessable link and told nothing. So
    /// every unrecognized field is named back in a `400`, which generalizes the
    /// one-off tombstone that used to guard `private` alone.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}
// Note: `content_type` is intentionally absent — minimal Text renders plaintext
// only. Rich Text (a later step, on a sandboxed origin) will reintroduce it with
// real handling.

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
    /// The destination, for redirect links. Absent for limited (single-use)
    /// links, whose payload is only disclosed by spending the use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The body for Text links. Reading it here does not count against
    /// `max_uses` — which is exactly why it is absent for limited links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Uses spent. Only ever 0 or 1: it exists to gate a one-time link, not to
    /// count views -- no per-link view counter exists anywhere in YuioLink.
    pub uses: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    pub created_at: String,
    pub expires_at: String,
}

pub enum ApiError {
    NotFound,
    BadRequest(Vec<FieldError>),
    TooManyRequests,
    Internal,
}

impl From<CreateError> for ApiError {
    fn from(e: CreateError) -> Self {
        match e {
            CreateError::BadRequest(errors) => ApiError::BadRequest(errors),
            CreateError::Internal => ApiError::Internal,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            // A 400 reports every collected problem: `error` stays the one-string
            // summary older clients read; `errors` carries the per-field breakdown.
            ApiError::BadRequest(errors) => {
                let body = serde_json::json!({
                    "error": join_messages(&errors, " "),
                    "errors": errors,
                });
                return (StatusCode::BAD_REQUEST, Json(body)).into_response();
            }
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found"),
            ApiError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, RATE_LIMIT_MSG),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// `GET /help` — the usage page. Static apart from the host it prints in the
/// `curl` examples, which comes from the configured base URL so a copied command
/// targets the instance the reader is actually on.
pub async fn help(State(state): State<AppState>) -> Response {
    Html(views::help_page(&state.base_url).into_string()).into_response()
}

/// `GET /colophon` — the licence and the third-party attributions. Fully static:
/// nothing on it depends on the instance, unlike `/help` and its `curl` examples.
pub async fn colophon() -> Response {
    Html(views::colophon_page().into_string()).into_response()
}

/// `GET /stats` — the public, aggregate-only counters. Reads three cheap queries
/// and renders them; a failure on any one degrades to zeroes rather than a 500,
/// since a broken counter is never worth an error page.
pub async fn stats(State(state): State<AppState>) -> Response {
    let live = db::live_count(&state.pool).await.unwrap_or(0);
    let totals = db::stat_totals(&state.pool).await.unwrap_or_default();
    let recent = db::stat_recent(&state.pool, 7).await.unwrap_or_default();

    // Fold the (day, metric, count) rows into one row per day: created (any type)
    // and opened. Days with no activity simply do not appear.
    let mut days: Vec<(String, i64, i64)> = Vec::new();
    for (day, metric, count) in recent {
        let row = match days.iter_mut().find(|(d, _, _)| *d == day) {
            Some(row) => row,
            None => {
                days.push((day, 0, 0));
                days.last_mut().expect("just pushed")
            }
        };
        match metric.as_str() {
            "created_public" | "created_secret" | "created_once" => row.1 += count,
            "opened" => row.2 += count,
            _ => {}
        }
    }

    Html(
        views::stats_page(&views::StatsView {
            live,
            totals: &totals,
            days: &days,
        })
        .into_string(),
    )
    .into_response()
}

/// `POST /api/v0/links` — create a link. Returns `201 Created` with a
/// `Location` header pointing at the new resource. This is the surface JS uses
/// for an in-place result (and the one a third-party client targets).
pub async fn api_create_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !state.limiter.allow(&client_key(&headers)) {
        return Err(ApiError::TooManyRequests);
    }

    // Named one per field, in the same batch shape as every other validation
    // error, so a caller fixes all of them in one round-trip.
    if !req.unknown.is_empty() {
        return Err(ApiError::BadRequest(
            req.unknown
                .keys()
                .map(|name| FieldError {
                    // The message names the field too: the `error` summary is
                    // these joined, and "not a field" twice over says nothing.
                    message: format!("`{name}` is not a field of the create API."),
                    field: Cow::Owned(name.clone()),
                })
                .collect(),
        ));
    }

    let ttl_seconds = req.ttl_seconds.unwrap_or(DEFAULT_TTL_SECS);
    let delete_token = yuiolink_core::generate_token();
    let inserted = create_link(
        &state,
        Some(req.kind.as_str()),
        &req.content,
        Ok(ttl_seconds),
        Ok(req.max_uses),
        req.secret,
        Some(&delete_token),
    )
    .await?;

    let url = format!("{}{}", state.base_url, inserted.name);
    let location = format!("{}api/v0/links/{}", state.base_url, inserted.name);
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

/// `DELETE /api/v0/links/:name` — withdraw a link, authorized by the per-link
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
    let deleted = db::delete_link(&state.pool, &name, token)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to delete link");
            ApiError::Internal
        })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

/// `GET /api/v0/links/:name` — read a link (the REST "expand"). Safe and
/// idempotent: it does NOT count a hit or consume `max_uses`. Because of that, a
/// **limited** (single-use) link answers with metadata only — returning its
/// destination or body here would let anyone who learns the name read a one-time
/// link repeatedly without spending the use, silently defeating the
/// burn-after-read tamper evidence the reveal flow exists to provide. Consuming
/// stays exclusive to `POST /:name/reveal`.
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

    let (target, content) = match (d.kind.as_str(), d.max_uses.is_some()) {
        (_, true) => (None, None),
        ("redirect", false) => (Some(d.content.clone()), None),
        ("text", false) => (None, Some(d.content.clone())),
        _ => (None, None),
    };

    Ok(Json(ApiLink {
        url: format!("{}{}", state.base_url, d.name),
        name: d.name,
        kind: d.kind,
        target,
        content,
        uses: d.uses,
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
                [
                    (header::CONTENT_TYPE, $mime),
                    // A year, and immutable. Safe only because every reference
                    // to these files carries `?v=<version>` (see `asset_url` in
                    // views): a deploy changes the query, which is a different
                    // cache key, so a client never has to be told to re-check.
                    // Without that versioning this would strand a stale asset
                    // in caches for a year.
                    (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
                ],
                include_str!(concat!("../static/", $file)),
            )
        }
    };
}

static_asset!(app_css, "app.css", "text/css; charset=utf-8");
static_asset!(app_js, "app.js", "text/javascript; charset=utf-8");
static_asset!(text_js, "text.js", "text/javascript; charset=utf-8");
static_asset!(preview_js, "preview.js", "text/javascript; charset=utf-8");

/// `GET /api/v0/openapi.yaml` — the API description, embedded from
/// `server/openapi.yaml` (inside the crate, so the container build context has
/// it) so the served spec always matches the built binary.
pub async fn openapi_yaml() -> impl IntoResponse {
    (
        [
            // RFC 9512 media type for YAML.
            (header::CONTENT_TYPE, "application/yaml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_str!("../openapi.yaml"),
    )
}

/// `GET /robots.txt` — allow crawling the landing page, static assets, and the
/// wordlist, but bar every link page: a crawled public link would end up in a
/// search index, defeating "nothing indexes the name" for everyone. The link
/// pages also carry a noindex meta as a belt-and-braces for crawlers that got
/// a URL from elsewhere.
pub async fn robots_txt() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        "User-agent: *\nAllow: /$\nAllow: /static/\nAllow: /wordlist.txt\nDisallow: /\n",
    )
}

/// `GET /wordlist.txt` — the curated wordlist behind every link name, one word
/// per line. The create page's Public note links here so "anyone can run the
/// whole list" is verifiable, not just asserted.
pub async fn wordlist_txt() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        yuiolink_core::words().join("\n") + "\n",
    )
}

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
        // Overflow must read as invalid, not wrap into an accidental TTL.
        assert_eq!(parse_duration("99999999999999999d"), None);
        assert_eq!(parse_duration("-1h"), None);
    }

    #[test]
    fn parse_custom_ttl_bounds_and_units() {
        assert_eq!(parse_custom_ttl(Some("5"), Some("m")), Ok(300));
        assert_eq!(parse_custom_ttl(Some("2"), Some("h")), Ok(7200));
        assert_eq!(parse_custom_ttl(Some("3"), Some("d")), Ok(259200));
        assert!(parse_custom_ttl(None, Some("m")).is_err());
        assert!(parse_custom_ttl(Some("x"), Some("m")).is_err());
        // Saturates instead of overflowing; check_ttl rejects it afterwards.
        assert_eq!(
            parse_custom_ttl(Some(&i64::MAX.to_string()), Some("d")),
            Ok(i64::MAX)
        );
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
            occupancy: Arc::new(std::array::from_fn(|_| {
                std::sync::atomic::AtomicU64::new(0)
            })),
            limiter: Arc::new(RateLimiter::new()),
        }
    }

    fn redirect(content: &str, max_uses: Option<i64>) -> NewLink<'_> {
        NewLink {
            kind: "redirect",
            content,
            content_type: None,
            ttl_seconds: 3600,
            max_uses,
            secret: false,
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
        Request::builder()
            .method("POST")
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    async fn uses(state: &AppState, name: &str) -> i64 {
        db::get_link_any(&state.pool, name)
            .await
            .unwrap()
            .unwrap()
            .uses
    }

    #[tokio::test]
    async fn unlimited_redirect_previews_then_consumes() {
        let st = test_state().await;
        let l = db::insert_link(
            &st.pool,
            redirect("https://example.com/x", None),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();

        // GET previews: 200 interstitial, no hit, full URL + amber Continue. A
        // crawler doing exactly this can never spend a use.
        let (s, _, body) = send(&st, get(&format!("/{}", l.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(
            body.contains("Continue to example.com"),
            "interstitial body: {body}"
        );
        assert_eq!(uses(&st, &l.name).await, 0);

        // POST /go consumes: 303 straight to the destination, hit counted.
        let (s, h, _) = send(&st, post(&format!("/{}/go", l.name))).await;
        assert_eq!(s, StatusCode::SEE_OTHER);
        assert_eq!(h.get("location").unwrap(), "https://example.com/x");
        assert_eq!(uses(&st, &l.name).await, 1);
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
        assert!(
            !body.contains("zzz-gated-path"),
            "path must be gated: {body}"
        );
        assert_eq!(uses(&st, &l.name).await, 0);

        // POST /reveal consumes once and 303s to the clean /:name URL, with the
        // capability token in a Set-Cookie header (not the URL).
        let (s, h, _) = send(&st, post(&format!("/{}/reveal", l.name))).await;
        assert_eq!(s, StatusCode::SEE_OTHER);
        let loc = h.get("location").unwrap().to_str().unwrap().to_string();
        assert_eq!(loc, format!("/{}", l.name));
        let set_cookie = h.get("set-cookie").unwrap().to_str().unwrap();
        assert!(set_cookie.starts_with("yl_reveal="));
        let cookie = set_cookie.split(';').next().unwrap().to_string();
        assert_eq!(uses(&st, &l.name).await, 1);

        // The revealed GET (carrying the cookie) shows the full URL and, in doing
        // so, deletes it from the server: it does NOT count a second hit, but a
        // repeat visit with the same cookie now finds the content already gone.
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
        assert_eq!(uses(&st, &l.name).await, 1);

        let (s, _, body) = send(&st, revealed_get(&cookie)).await;
        assert_eq!(s, StatusCode::GONE, "second render with the same cookie");
        assert!(!body.contains("zzz-gated-path"));
        assert_eq!(uses(&st, &l.name).await, 1);

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
        assert!(
            !body.contains("zzz-gated"),
            "forged cookie must not reveal: {body}"
        );
        assert_eq!(uses(&st, &l.name).await, 0);
    }

    #[tokio::test]
    async fn unlimited_text_opens_immediately_and_spends_nothing() {
        let st = test_state().await;
        let l = db::insert_link(
            &st.pool,
            NewLink {
                kind: "text",
                content: "hello plaintext",
                content_type: Some("text/plain"),
                ttl_seconds: 3600,
                max_uses: None,
                secret: false,
                delete_token: None,
            },
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        let (s, _, body) = send(&st, get(&format!("/{}", l.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("hello plaintext"));
        // `uses` gates a one-time link and nothing else, so rendering an
        // unlimited Text link leaves it at zero however often it is read.
        assert_eq!(uses(&st, &l.name).await, 0);
        send(&st, get(&format!("/{}", l.name))).await;
        assert_eq!(uses(&st, &l.name).await, 0);
    }

    #[tokio::test]
    async fn trailing_plus_and_any_case_resolve_to_canonical_preview() {
        let st = test_state().await;
        let l = db::insert_link(
            &st.pool,
            redirect("https://example.com/x", None),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();

        // A trailing "+" is accepted and behaves like the bare name (no use spent).
        let (s, _, body) = send(&st, get(&format!("/{}+", l.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("Continue to example.com"));
        assert_eq!(uses(&st, &l.name).await, 0);

        // Typing a different case resolves (NOCASE) and the preview shows the
        // canonical stored name, not what was typed.
        let typed = l.name.to_uppercase();
        assert_ne!(typed, l.name);
        let (s, _, body) = send(&st, get(&format!("/{typed}"))).await;
        assert_eq!(s, StatusCode::OK);
        // The heading renders the name one span per shoutkey word, so compare the
        // text rather than the markup.
        let inner = body
            .split_once(r#"<span class="pv-name">"#)
            .and_then(|(_, rest)| rest.split_once("</span></h1>"))
            .map(|(inner, _)| inner)
            .expect("the preview heading carries the link name");
        let shown: String = inner
            .split('<')
            .filter_map(|chunk| chunk.split_once('>'))
            .map(|(_, text)| text)
            .collect();
        assert_eq!(shown, l.name);
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
                .uri("/api/v0/links")
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
    async fn no_js_can_store_a_url_as_text_from_the_result_page() {
        let st = test_state().await;
        // A plain form post detects a Redirect and offers the Text alternative.
        let form = |extra: &str| {
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "content=https%3A%2F%2Fexample.com%2Fpath&ttl_stop=7{extra}"
                )))
                .unwrap()
        };
        let (s, _, body) = send(&st, form("")).await;
        assert_eq!(s, StatusCode::OK);
        assert!(
            body.contains("Share the address as a text link instead"),
            "redirect result should offer the Text alternative"
        );
        assert!(body.contains(r#"name="kind" value="text""#));

        // Following that offer stores the same URL as Text — the override the
        // Option key provides on desktop, reachable with no JavaScript at all.
        let (s, _, body) = send(&st, form("&kind=text")).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("Text"), "should report a Text link: {body}");
        // Not offered again: a Text link has no other kind to become.
        assert!(!body.contains("Share the address as a text link instead"));
    }

    /// Every bare segment routed at the root shadows `/{name}`, so it must also be
    /// in `RESERVED_NAMES` — otherwise a link issued under that word is a 405 for
    /// its whole life, which is how `/stats` shipped once already.
    ///
    /// The list is read out of this file's own source rather than restated here,
    /// so adding a route cannot silently skip the check: a new `.route("/x")`
    /// fails this test until `x` is reserved. Only the `router()` body counts —
    /// `api_routes()` is nested under `/api/v0` and shadows nothing.
    #[test]
    fn every_root_route_is_a_reserved_name() {
        let src = include_str!("web.rs");
        let router_body = src
            .split_once("fn api_routes(")
            .map(|(before, _)| before)
            .expect("api_routes marks the end of the root router");

        let mut checked = 0;
        for (marker, _) in [(".route(\"", 0), (".nest(\"", 0)] {
            for chunk in router_body.split(marker).skip(1) {
                let path = chunk.split('"').next().expect("a closing quote");
                // The root itself, and the `/{name}` family, shadow nothing.
                let Some(segment) = path.trim_start_matches('/').split('/').next() else {
                    continue;
                };
                if segment.is_empty() || segment.starts_with('{') {
                    continue;
                }
                // `/robots.txt` cannot collide, but `/robots` is what a reader
                // would try, and both are reserved — compare on the stem.
                let stem = segment.split('.').next().expect("a non-empty segment");
                assert!(
                    yuiolink_core::link::is_reserved_name(stem),
                    "route /{segment} shadows /{{name}}: add \"{stem}\" to RESERVED_NAMES"
                );
                checked += 1;
            }
        }
        // Guard the guard: a parse that silently matched nothing would pass.
        assert!(
            checked >= 8,
            "expected to find the root routes, saw {checked}"
        );
    }

    #[tokio::test]
    async fn colophon_credits_the_bundled_work() {
        let st = test_state().await;
        let (s, _, body) = send(&st, get("/colophon")).await;
        assert_eq!(s, StatusCode::OK);
        // CC-BY asks for the author, and the fonts carry their own terms — both
        // have to survive an edit to this page.
        for credit in ["Electronic Frontier Foundation", "CC-BY-3.0-US", "DejaVu"] {
            assert!(
                body.contains(credit),
                "colophon should credit {credit}: {body}"
            );
        }
        // The footer's licence link is the only route to this page.
        let (s, _, home) = send(&st, get("/")).await;
        assert_eq!(s, StatusCode::OK);
        assert!(home.contains("href=\"/colophon\""), "{home}");
    }

    #[tokio::test]
    async fn help_page_covers_the_types_and_prints_this_host_in_the_examples() {
        let st = test_state().await;
        let (s, _, body) = send(&st, get("/help")).await;
        assert_eq!(s, StatusCode::OK);
        for label in ["Public", "Secret", "One-Time"] {
            assert!(body.contains(label), "help should name {label}: {body}");
        }
        // The curl line must target the configured instance, not a hardcoded host.
        assert!(body.contains(&format!("{}create", st.base_url)), "{body}");
    }

    #[tokio::test]
    async fn stats_counts_creates_and_opens_but_no_identities() {
        let st = test_state().await;
        let l = db::insert_link(
            &st.pool,
            redirect("https://example.com/counted", None),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        // A preview must not count as an open — only spending a use does.
        let (s, _, _) = send(&st, get(&format!("/{}", l.name))).await;
        assert_eq!(s, StatusCode::OK);

        let (s, body) = {
            let (s, _, b) = send(&st, get("/stats")).await;
            (s, b)
        };
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("links created"), "page should render: {body}");
        // One create, no opens yet.
        assert!(body.contains("Statistics"));

        // Spending the use is what the counter is for.
        let req = Request::builder()
            .method("POST")
            .uri(format!("/{}/go", l.name))
            .body(Body::empty())
            .unwrap();
        let (s, _, _) = send(&st, req).await;
        assert!(s.is_redirection() || s == StatusCode::OK, "go returned {s}");

        let totals = db::stat_totals(&st.pool).await.unwrap();
        let get_total = |k: &str| {
            totals
                .iter()
                .find(|(m, _)| m == k)
                .map(|(_, n)| *n)
                .unwrap_or(0)
        };
        assert_eq!(get_total("created_public"), 1);
        assert_eq!(get_total("created_redirect"), 1);
        assert_eq!(get_total("opened"), 1, "preview must not have counted");
        // The preview render has its own metric — the only place a preview is
        // ever counted, since no per-link counter exists.
        assert_eq!(get_total("previewed"), 1);
        assert_eq!(get_total("revealed"), 0, "nothing was revealed here");

        // The table holds counts and nothing else — no column could carry a name,
        // a destination, or a visitor.
        let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('stats')")
            .fetch_all(&st.pool)
            .await
            .unwrap();
        assert_eq!(cols, vec!["day", "metric", "count"]);
    }

    #[tokio::test]
    async fn api_names_every_unknown_field_instead_of_ignoring_it() {
        let st = test_state().await;
        // `private` is the old name for `secret`, `secrit` is a typo: both would
        // otherwise be dropped and the caller handed a short, guessable name
        // while believing they had asked for an unguessable one.
        let req = Request::builder()
            .method("POST")
            .uri("/api/v0/links")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"kind":"redirect","content":"https://example.com","private":true,"secrit":true}"#,
            ))
            .unwrap();
        let (s, _, body) = send(&st, req).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert!(body.contains("private"), "should name the field: {body}");
        assert!(body.contains("secrit"), "and all of them at once: {body}");

        // The fields it does know still work.
        let req = Request::builder()
            .method("POST")
            .uri("/api/v0/links")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"kind":"redirect","content":"https://example.com","secret":true}"#,
            ))
            .unwrap();
        let (s, _, _) = send(&st, req).await;
        assert_eq!(s, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn titles_name_the_link_and_hide_secret_destinations() {
        let st = test_state().await;
        // Public: one word, nothing secret, so the tab may name the destination.
        let pubw = db::insert_link(
            &st.pool,
            redirect("https://example.com/deep/path", None),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        let (s, _, body) = send(&st, get(&format!("/{}", pubw.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(
            body.contains(&format!(
                "<title>YuioLink Redirect: {} → example.com</title>",
                pubw.name
            )),
            "{body}"
        );

        // One-time: four words, so the destination stays off the tab and out of
        // browser history.
        let once = db::insert_link(
            &st.pool,
            redirect("https://example.com/deep/path", Some(1)),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        let (s, _, body) = send(&st, get(&format!("/{}", once.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(
            body.contains(&format!("<title>YuioLink Redirect: {}</title>", once.name)),
            "{body}"
        );
    }

    #[tokio::test]
    async fn api_read_of_limited_link_omits_payload_and_spends_nothing() {
        let st = test_state().await;
        let l = db::insert_link(
            &st.pool,
            redirect("https://secret.example.com/zzz-gated-path", Some(1)),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();

        // The REST read must not disclose a one-time link's destination: doing so
        // would let anyone who knows the name read it repeatedly without spending
        // the use, defeating the burn-after-read tamper evidence.
        let (s, _, body) = send(&st, get(&format!("/api/v0/links/{}", l.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(
            !body.contains("zzz-gated-path"),
            "payload must be gated: {body}"
        );
        assert!(
            body.contains(r#""max_uses":1"#),
            "metadata still served: {body}"
        );
        assert_eq!(uses(&st, &l.name).await, 0);

        // An unlimited link still returns its target (the REST "expand").
        let u = db::insert_link(
            &st.pool,
            redirect("https://example.com/open", None),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        let (s, _, body) = send(&st, get(&format!("/api/v0/links/{}", u.name))).await;
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("https://example.com/open"));
    }

    #[tokio::test]
    async fn api_reports_every_validation_error_at_once() {
        let st = test_state().await;
        // Three things wrong in one request: an unknown kind, an over-long TTL,
        // and a multi-use limit. All three must come back together.
        let req = Request::builder()
            .method("POST")
            .uri("/api/v0/links")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"kind":"carrier-pigeon","content":"https://example.com","ttl_seconds":99999999,"max_uses":5}"#,
            ))
            .unwrap();
        let (s, _, body) = send(&st, req).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        // The summary string survives for older clients…
        assert!(v["error"].is_string());
        // …and the breakdown names each offending field.
        let fields: Vec<&str> = v["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["field"].as_str().unwrap())
            .collect();
        assert_eq!(fields, ["kind", "ttl_seconds", "max_uses"], "body: {body}");
    }

    #[tokio::test]
    async fn api_rejects_oversized_content() {
        let st = test_state().await;
        let big = "x".repeat(MAX_CONTENT_BYTES + 1);
        let req = Request::builder()
            .method("POST")
            .uri("/api/v0/links")
            .header("content-type", "application/json")
            .body(Body::from(format!(
                r#"{{"kind":"text","content":"{big}"}}"#
            )))
            .unwrap();
        let (s, _, _) = send(&st, req).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_is_rate_limited_per_client() {
        let st = test_state().await;
        let create = |ip: &str| {
            Request::builder()
                .method("POST")
                .uri("/api/v0/links")
                .header("content-type", "application/json")
                .header("x-forwarded-for", ip)
                .body(Body::from(
                    r#"{"kind":"redirect","content":"https://example.com"}"#.to_string(),
                ))
                .unwrap()
        };
        // The burst passes, the next create is a fast 429.
        for _ in 0..10 {
            let (s, _, _) = send(&st, create("203.0.113.7")).await;
            assert_eq!(s, StatusCode::CREATED);
        }
        let (s, _, body) = send(&st, create("203.0.113.7")).await;
        assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
        assert!(body.contains("too quickly"), "{body}");
        // A different client is unaffected; resolution is never limited.
        let (s, _, _) = send(&st, create("198.51.100.9")).await;
        assert_eq!(s, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn withdraw_via_api_then_gone() {
        let st = test_state().await;
        let l = db::insert_link(
            &st.pool,
            redirect("https://example.com", None),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();

        // A wrong (or missing) token reads as 404 — the endpoint reveals nothing.
        let bad = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v0/links/{}", l.name))
            .header("authorization", "Bearer wrong")
            .body(Body::empty())
            .unwrap();
        let (s, _, _) = send(&st, bad).await;
        assert_eq!(s, StatusCode::NOT_FOUND);

        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v0/links/{}", l.name))
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
        let l = db::insert_link(
            &st.pool,
            redirect("https://example.com/blog", None),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();

        // A crawler hitting the interstitial and the card never spends a use.
        send(&st, get(&format!("/{}", l.name))).await;
        let resp = router(st.clone())
            .oneshot(get(&format!("/{}/card.png", l.name)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");
        let png = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&png[1..4], b"PNG");
        assert_eq!(uses(&st, &l.name).await, 0);

        // Text links have no card.
        let t = db::insert_link(
            &st.pool,
            NewLink {
                kind: "text",
                content: "hi",
                content_type: Some("text/plain"),
                ttl_seconds: 3600,
                max_uses: None,
                secret: false,
                delete_token: None,
            },
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        let (s, _, _) = send(&st, get(&format!("/{}/card.png", t.name))).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    /// The policy is only worth having if the page it guards actually matches it:
    /// script is allowed by nonce alone, so every `<script>` the page ships has to
    /// carry the one this response minted, and no two responses may share it.
    #[tokio::test]
    async fn every_script_carries_this_response_nonce() {
        let st = test_state().await;
        let (s, headers, body) = send(&st, get("/")).await;
        assert_eq!(s, StatusCode::OK);

        let csp = headers
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let nonce = csp
            .split_once("'nonce-")
            .and_then(|(_, rest)| rest.split_once('\''))
            .expect("the policy carries a script nonce")
            .0;
        assert_eq!(body.matches("<script").count(), 2); // the pre-paint marker + app.js
        assert_eq!(body.matches(&format!("nonce=\"{nonce}\"")).count(), 2);

        // A nonce that repeated across responses would be a nonce an attacker can
        // read off one page and reuse on the next.
        let (_, again, _) = send(&st, get("/")).await;
        assert_ne!(again.get("content-security-policy").unwrap(), csp.as_str());

        // ...and a stored page is a repeated nonce by another route: one visitor's
        // would be served to everyone who followed.
        assert_eq!(headers.get("cache-control").unwrap(), "no-store");
        let (_, asset, _) = send(&st, get("/static/app.js")).await;
        assert!(
            asset
                .get("cache-control")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("immutable"),
            "only the pages go uncached — the versioned assets keep their year"
        );
    }

    /// Every response leaves through the same middleware, including the ones no
    /// handler renders (errors) and the ones that are not pages at all.
    #[tokio::test]
    async fn security_headers_are_on_every_response() {
        let st = test_state().await;
        for uri in ["/", "/static/app.js", "/api/v0/links/nope", "/nope"] {
            let (_, h, _) = send(&st, get(uri)).await;
            assert_eq!(h.get("x-frame-options").unwrap(), "DENY", "{uri}");
            assert_eq!(h.get("x-content-type-options").unwrap(), "nosniff", "{uri}");
            assert_eq!(h.get("referrer-policy").unwrap(), "no-referrer", "{uri}");
            assert_eq!(
                h.get("cross-origin-opener-policy").unwrap(),
                "same-origin",
                "{uri}"
            );
            assert_eq!(
                h.get("cross-origin-resource-policy").unwrap(),
                "same-origin",
                "{uri}"
            );
        }

        // The one exception: the share card is meant to be shown by other sites.
        let l = db::insert_link(
            &st.pool,
            redirect("https://example.com/blog", None),
            &db::EMPTY_OCCUPANCY,
        )
        .await
        .unwrap();
        let card = router(st.clone())
            .oneshot(get(&format!("/{}/card.png", l.name)))
            .await
            .unwrap();
        assert_eq!(
            card.headers().get("cross-origin-resource-policy").unwrap(),
            "cross-origin"
        );
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
