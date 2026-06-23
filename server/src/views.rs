//! HTML views, rendered with maud (escaped by default).
//!
//! The markup leans on semantic elements — `header`/`main`/`footer`, `fieldset`/
//! `legend` for the radio groups, `output` for created links, `code`/`pre` for
//! machine text — and reserves classes for genuinely styled components.

use std::time::{SystemTime, UNIX_EPOCH};

use maud::{DOCTYPE, Markup, html};

use crate::urlview::{IdnWarning, UrlView};

/// The shared page shell: head, the glass "app window", and the masthead.
fn document(body: Markup, scripts: Markup) -> Markup {
    document_full(html! {}, body, scripts)
}

/// As [`document`], but with extra `<head>` markup (e.g. OG tags). The masthead
/// `<h1>` links home so every page has a way back to create another link (the
/// per-page "Back to YuioLink" footer link is gone).
fn document_full(head_extra: Markup, body: Markup, scripts: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover";
                meta name="color-scheme" content="light dark";
                title { "YuioLink" }
                link rel="stylesheet" href="/static/app.css";
                (head_extra)
            }
            body {
                main.app-window {
                    header {
                        h1 { a href="/" { "YuioLink" } }
                    }
                    (body)
                }
                (scripts)
            }
        }
    }
}

/// The link name — the last path segment, minus any `#fragment` — shown as the hero.
fn link_name(url: &str) -> &str {
    url.split('#').next().unwrap_or(url).rsplit('/').next().unwrap_or(url)
}

/// The display host (no scheme, no trailing slash) of the public base URL, e.g.
/// `https://yuio.link/` -> `yuio.link`. Used for the interstitial source line.
pub fn host_from_base(base_url: &str) -> &str {
    base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
}

// --------------------------------------------------------------------------
// Time helpers (SQLite stores UTC "YYYY-MM-DD HH:MM:SS")
// --------------------------------------------------------------------------

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse SQLite's `datetime()` form ("YYYY-MM-DD HH:MM:SS", always UTC) to a Unix
/// timestamp. Uses Howard Hinnant's days-from-civil algorithm (proleptic
/// Gregorian) so it needs no date library.
fn parse_sqlite_utc(s: &str) -> Option<i64> {
    let (date, time) = s.trim().split_once(' ')?;
    let mut d = date.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;
    let mut t = time.split(':');
    let hour: i64 = t.next()?.parse().ok()?;
    let min: i64 = t.next()?.parse().ok()?;
    let sec: i64 = t.next().unwrap_or("0").parse().ok()?;

    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    let days = era * 146097 + doe - 719468; // since 1970-01-01
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

/// Seconds from now until `expires_at` (negative if already past).
fn seconds_until(expires_at: &str) -> i64 {
    parse_sqlite_utc(expires_at).map(|e| e - now_unix()).unwrap_or(0)
}

/// A coarse, friendly relative expiry like `6 days`, `5 hours`, `48 min`. The
/// view prepends "Expires in " / "frees up in ". Never shows seconds.
pub fn humanize_expires_in(expires_at: &str) -> String {
    let secs = seconds_until(expires_at).max(0);
    if secs < 60 {
        "less than a minute".to_string()
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else if secs < 86400 {
        let n = secs / 3600;
        format!("{n} hour{}", if n == 1 { "" } else { "s" })
    } else {
        let n = secs / 86400;
        format!("{n} day{}", if n == 1 { "" } else { "s" })
    }
}

/// An absolute date for share-card / OG copy, e.g. `Jun 29, 2026`.
pub fn format_card_date(expires_at: &str) -> String {
    let date = expires_at.split([' ', 'T']).next().unwrap_or(expires_at);
    let mut p = date.split('-');
    let year = p.next();
    let month = p.next().and_then(|m| m.parse::<usize>().ok());
    let day = p.next().and_then(|d| d.parse::<u32>().ok());
    match (year, month, day) {
        (Some(y), Some(m), Some(d)) if (1..=12).contains(&m) => {
            const MON: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            format!("{} {}, {}", MON[m - 1], d, y)
        }
        _ => date.to_string(),
    }
}

/// Humanize a TTL ceiling for display, e.g. 604800 -> "7 days". Also used by
/// `web::check_ttl` to phrase the out-of-range error in days/hours, not seconds.
pub fn humanize_duration(secs: i64) -> String {
    let (n, unit) = if secs % 86400 == 0 {
        (secs / 86400, "day")
    } else if secs % 3600 == 0 {
        (secs / 3600, "hour")
    } else {
        (secs / 60, "minute")
    };
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

// --------------------------------------------------------------------------
// Landing + created-link result
// --------------------------------------------------------------------------

/// The result `<output>` shown after a link is created (server-rendered on the
/// no-JS path, populated in place by `app.js` otherwise). The memorable word (the
/// link name) is the hero; the full URL sits small beneath it; a single meta line
/// carries kind, expiry, and any use limit.
fn result_output(url: Option<&str>, meta: Markup) -> Markup {
    html! {
        output.result #link-panel tabindex="-1" hidden[url.is_none()] {
            code.result-word #link-word { @if let Some(u) = url { (link_name(u)) } }
            code.result-url #link-element { @if let Some(u) = url { (u) } }
            div.result-foot {
                small.result-meta #link-expiry { (meta) }
                div.result-actions {
                    button.result-copy #copy-result type="button" hidden { "Copy" }
                }
            }
            small.result-note #result-note hidden {}
        }
    }
}

/// The landing page. Works without JavaScript (the form posts to `POST /` and a
/// result page comes back); `app.js` progressively enhances it with live type
/// detection, keyboard shortcuts, an in-place result, and copy.
pub fn index_page(max_ttl_secs: i64) -> Markup {
    let body = html! {
        p { "Wieldy Ephemeral Link" }

        // Split storage pill (top): left shows the status (and links to the list),
        // right is the local-persistence toggle in its own colour. app.js fills both.
        div.storage-pill {
            a.storage-status #storage-status href="#history" {}
            button.storage-toggle #storage-toggle type="button" {}
        }
        // Shown by app.js when the user turns local history off while links exist.
        p.storage-warning #storage-warning hidden {
            "Local history is off — these links will be gone when you close this page."
        }

        // The created link (latest), shown above the input. app.js fills it in place;
        // the no-JS path reloads to a result page.
        (result_output(None, html! {}))

        form #create-form method="post" action="/" {
            textarea #content.form-control name="content" rows="1"
                autocomplete="off" autocapitalize="off" spellcheck="false"
                placeholder="Paste a link to redirect, or type text to share" autofocus {}

            div.split-btn {
                button #submit.btn.split-primary type="submit" { "Create Link" }
                button #clear.btn.split-clear type="button" { "Clear" }
            }
            p.form-error #form-error role="alert" hidden {}

            fieldset.picker {
                legend { "Expires in" }
                div.segmented {
                    input.seg-radio #ttl-600 type="radio" name="ttl_seconds" value="600";
                    label.seg-label for="ttl-600" { "10 minutes" }
                    input.seg-radio #ttl-3600 type="radio" name="ttl_seconds" value="3600" checked;
                    label.seg-label for="ttl-3600" { "1 hour" }
                    input.seg-radio #ttl-604800 type="radio" name="ttl_seconds" value="604800";
                    label.seg-label for="ttl-604800" { "7 days" }
                    input.seg-radio #ttl-custom type="radio" name="ttl_seconds" value="custom";
                    label.seg-label for="ttl-custom" { "Specify" }
                }
                div.custom-field #ttl-custom-field {
                    input #ttl-custom-value.custom-num name="ttl_custom" type="number"
                        min="1" step="1" inputmode="numeric" placeholder="5";
                    div.segmented.unit-segmented {
                        input.seg-radio #ttl-unit-m type="radio" name="ttl_unit" value="m" checked;
                        label.seg-label for="ttl-unit-m" { "minutes" }
                        input.seg-radio #ttl-unit-h type="radio" name="ttl_unit" value="h";
                        label.seg-label for="ttl-unit-h" { "hours" }
                        input.seg-radio #ttl-unit-d type="radio" name="ttl_unit" value="d";
                        label.seg-label for="ttl-unit-d" { "days" }
                    }
                    small.custom-hint { "Up to " (humanize_duration(max_ttl_secs)) }
                }
            }

            fieldset.picker {
                legend { "Limit views to" }
                div.segmented {
                    input.seg-radio #limit-unlimited type="radio" name="limit" value="unlimited" checked;
                    label.seg-label for="limit-unlimited" {
                        span.infinity aria-label="Unlimited" { "∞" }
                    }
                    input.seg-radio #limit-1 type="radio" name="limit" value="1";
                    label.seg-label for="limit-1" { "Once" }
                    input.seg-radio #limit-custom type="radio" name="limit" value="custom";
                    label.seg-label for="limit-custom" { "Specify" }
                }
                div.custom-field #limit-custom-field {
                    input #limit-custom-value.custom-num name="limit_custom" type="number"
                        min="1" max="1000000000" step="1" inputmode="numeric" placeholder="Times";
                }
            }
        }

        // Created-link history (bottom). Kept in memory for the session unless the
        // user ticks "Save on this device", which opts into localStorage.
        section.history.collapsed #history hidden {
            div.history-head {
                button.history-toggle #history-toggle type="button" {
                    span.history-chevron aria-hidden="true" { "›" }
                    span.history-title { "Local History" }
                }
                button.history-clear #history-clear type="button" { "Clear" }
            }
            div.history-body {
                ul.history-list #history-list {}
                button.history-clear-expired #history-clear-expired type="button" hidden { "Clear Expired" }
            }
        }

        footer {
            "A project by " a href="https://github.com/jooize" { "jooize" } " · "
            a href="https://github.com/jooize/YuioLink" { "Source on GitHub" }
        }
    };
    let scripts = html! { script src="/static/app.js" {} };
    document(body, scripts)
}

/// The no-JS result page shown after `POST /` creates a link. "Open link" leads
/// to the link's own interstitial (the always-preview), not straight out.
pub fn result_page(url: &str, kind_label: &str, expires_at: &str, max_uses: Option<i64>) -> Markup {
    let meta = html! {
        (kind_label) " · expires " (expires_at) " UTC"
        @match max_uses {
            Some(1) => { " · one-time" }
            Some(max) => { " · max " (max) " uses" }
            None => {}
        }
    };
    let body = html! {
        (result_output(Some(url), meta))
        a.btn.btn-block href=(url) { "Open link" }
        p { a href="/" { "Create another" } }
    };
    let scripts = html! { script src="/static/app.js" {} };
    document(body, scripts)
}

// --------------------------------------------------------------------------
// Interstitial (always-preview)
// --------------------------------------------------------------------------

/// What the interstitial is gating.
pub enum Target<'a> {
    /// A redirect, with its destination already parsed for display.
    Redirect(&'a UrlView),
    /// A limited Text link — only its existence is shown until revealed.
    TextSnippet,
}

pub struct Interstitial<'a> {
    pub base_host: &'a str,
    pub name: &'a str,
    pub short_url: &'a str,
    pub expires_at: &'a str,
    pub max_uses: Option<i64>,
    pub target: Target<'a>,
}

/// The mandatory preview shown for `GET /:name`. Spends no use; consuming is a
/// separate POST. Unlimited redirects show the full syntax-highlighted URL and an
/// amber Continue; limited links show only the domain (or "A text snippet") and a
/// blue Reveal that spends the use.
pub fn interstitial_page(i: Interstitial) -> Markup {
    let one_time = i.max_uses == Some(1);
    let limited = i.max_uses.is_some();

    let body = html! {
        (from_line(i.base_host, i.name))
        span.pv-arrow aria-hidden="true" { "↓" }
        @match &i.target {
            Target::Redirect(url) if limited => (limited_redirect_block(&i, url, one_time)),
            Target::Redirect(url) => (unlimited_redirect_block(&i, url)),
            Target::TextSnippet => (text_snippet_block(&i, one_time)),
        }
    };
    document_full(interstitial_head(&i, one_time), body, html! {})
}

/// `<head>` Open Graph / theme-color tags so a shared link unfurls trustworthily.
fn interstitial_head(i: &Interstitial, one_time: bool) -> Markup {
    match &i.target {
        Target::Redirect(url) => {
            let domain = url.card_domain();
            let title = if one_time {
                format!("One-time link to {domain}")
            } else {
                format!("Redirect to {domain}")
            };
            let kind = if one_time { "Single-use" } else { "Ephemeral" };
            let desc = format!(
                "{kind} redirect that expires {} and may change after.",
                format_card_date(i.expires_at)
            );
            let card = format!("{}/card.png", i.short_url);
            html! {
                meta property="og:site_name" content="YuioLink";
                meta property="og:type" content="website";
                meta property="og:title" content=(title);
                meta property="og:description" content=(desc);
                meta property="og:url" content=(i.short_url);
                meta property="og:image" content=(card);
                meta property="og:image:width" content="1200";
                meta property="og:image:height" content="630";
                meta name="twitter:card" content="summary_large_image";
                meta name="twitter:title" content=(title);
                meta name="twitter:description" content=(desc);
                meta name="theme-color" content="#007aff";
            }
        }
        Target::TextSnippet => html! {
            meta property="og:site_name" content="YuioLink";
            meta property="og:title" content="Text snippet on YuioLink";
            meta property="og:description" content="An ephemeral text snippet shared via YuioLink.";
            meta name="theme-color" content="#007aff";
        },
    }
}

fn from_line(host: &str, name: &str) -> Markup {
    html! {
        span.pv-from { (host) "/" span.name { (name) } }
    }
}

fn unlimited_redirect_block(i: &Interstitial, url: &UrlView) -> Markup {
    html! {
        (render_url(url))
        @if let Some(w) = idn_warning(url) { (idn_panel(w)) }
        (consume_form(&format!("/{}/go", i.name), GO_BTN, &continue_label(url)))
        p.pv-meta { "Expires in " (humanize_expires_in(i.expires_at)) }
        span.pv-caution {
            "YuioLinks expire and are reused, so a link can point somewhere else later. "
            strong { "Always check the destination." }
        }
    }
}

fn limited_redirect_block(i: &Interstitial, url: &UrlView, one_time: bool) -> Markup {
    html! {
        (render_host_domain(url))
        (consume_form(&format!("/{}/reveal", i.name), REVEAL_BTN, "Reveal Destination"))
        div.pv-badge-wrap { span.pv-badge { (badge_text(one_time)) } }
        p.pv-meta { "Expires in " (humanize_expires_in(i.expires_at)) }
        @if one_time {
            span.pv-caution.single { "If this page says the link is gone (410), someone already opened it." }
        } @else {
            span.pv-caution {
                "A limited link shows only the domain until you reveal it. "
                strong { "Always check the destination." }
            }
        }
    }
}

fn text_snippet_block(i: &Interstitial, one_time: bool) -> Markup {
    html! {
        span.pv-host.plain { "A text snippet" }
        (consume_form(&format!("/{}/reveal", i.name), REVEAL_BTN, "Reveal Text"))
        div.pv-badge-wrap { span.pv-badge { (badge_text(one_time)) } }
        p.pv-meta { "Expires in " (humanize_expires_in(i.expires_at)) }
    }
}

/// Amber "Continue" (leave the site) and blue "Reveal" (stay, spend a use) button
/// class sets. Both submit a POST form (Post/Redirect/Get), so a link-unfurl
/// crawler — which only GETs — can never spend a use.
const GO_BTN: &str = "btn btn--go btn-block pv-btn";
const REVEAL_BTN: &str = "btn btn-block pv-btn";

fn consume_form(action: &str, btn_class: &str, label: &str) -> Markup {
    html! {
        form.pv-form method="post" action=(action) {
            button class=(btn_class) type="submit" { (label) }
        }
    }
}

fn badge_text(one_time: bool) -> &'static str {
    if one_time { "Opens Once" } else { "Limited Use" }
}

fn continue_label(url: &UrlView) -> String {
    // Never print the deceptive domain on the button; say "Continue Anyway".
    if url.is_deceptive() {
        "Continue Anyway".to_string()
    } else {
        format!("Continue to {}", url.card_domain())
    }
}

fn idn_warning(url: &UrlView) -> Option<&IdnWarning> {
    url.host.as_ref().and_then(|h| h.warning.as_ref())
}

/// The full destination URL, coloured by part: dim scheme/delimiters, the
/// registrable domain highlighted, path segments and query values distinguished.
fn render_url(url: &UrlView) -> Markup {
    html! {
        code.pv-url {
            span.sch { (url.scheme) }
            @match &url.host {
                Some(h) => {
                    span.pn { "://" }
                    @if !h.subdomain.is_empty() { span.sub { (h.subdomain) "." } }
                    span.reg { (h.registrable) }
                    (render_path(&url.path))
                    @if let Some(q) = &url.query { (render_query(q)) }
                    @if let Some(f) = &url.fragment { span.pn { "#" } span.seg { (f) } }
                }
                None => {
                    span.pn { ":" }
                    @if let Some(o) = &url.opaque { span.seg { (o) } }
                }
            }
        }
    }
}

fn render_path(path: &str) -> Markup {
    html! {
        @for part in path.split('/').skip(1) {
            span.pn { "/" }
            @if !part.is_empty() { span.seg { (part) } }
        }
    }
}

fn render_query(query: &str) -> Markup {
    html! {
        span.pn { "?" }
        @for (idx, pair) in query.split('&').enumerate() {
            @if idx > 0 { span.pn { "&" } }
            @match pair.split_once('=') {
                Some((k, v)) => { span.seg { (k) } span.pn { "=" } span.qv { (v) } }
                None => { span.seg { (pair) } }
            }
        }
    }
}

/// Domain-only host for a limited link's pre-reveal view.
fn render_host_domain(url: &UrlView) -> Markup {
    html! {
        @match &url.host {
            Some(h) => span.pv-host {
                @if !h.subdomain.is_empty() { span.sub { (h.subdomain) "." } }
                (h.registrable)
            },
            None => span.pv-host.plain { (url.card_domain()) },
        }
    }
}

fn idn_panel(w: &IdnWarning) -> Markup {
    html! {
        div.pv-idn {
            p {
                strong { "Lookalike domain." }
                " Domain uses special characters that can deceptively imitate another name."
            }
            div.rows {
                span.lbl { "displays as" } span.val { (w.displays_as) }
                span.lbl { "real address" } span.val { (w.real) }
            }
        }
    }
}

// --------------------------------------------------------------------------
// Revealed view (token-gated, after a use was spent)
// --------------------------------------------------------------------------

pub enum RevealedTarget<'a> {
    /// A redirect: show the full URL and a plain Continue link (going is free now,
    /// the use was spent at reveal). `href` is the canonical destination.
    Redirect { url: &'a UrlView, href: &'a str },
    /// The revealed text body.
    Text(&'a str),
}

pub struct RevealedView<'a> {
    pub base_host: &'a str,
    pub name: &'a str,
    pub expires_at: &'a str,
    pub target: RevealedTarget<'a>,
}

/// The token-gated revealed page: re-renderable without consuming again.
pub fn revealed_page(r: RevealedView) -> Markup {
    match r.target {
        RevealedTarget::Redirect { url, href } => {
            let body = html! {
                (from_line(r.base_host, r.name))
                span.pv-arrow aria-hidden="true" { "↓" }
                (render_url(url))
                @if let Some(w) = idn_warning(url) { (idn_panel(w)) }
                a class=(GO_BTN) href=(href) rel="noopener noreferrer" { (continue_label(url)) }
                p.pv-revealed { "Destination revealed — this used one view." }
                p.pv-meta { "Expires in " (humanize_expires_in(r.expires_at)) }
                span.pv-caution.single { strong { "Always check the destination." } }
            };
            document(body, html! {})
        }
        RevealedTarget::Text(text) => {
            let body = html! {
                p.pv-revealed { "Text revealed — this used one view." }
                pre.text-body #text-body { (text) }
                button.btn.btn-block #copy-text type="button" { "Copy" }
            };
            document(body, html! { script src="/static/text.js" {} })
        }
    }
}

/// A plaintext Text link, rendered immediately (unlimited text). The body is an
/// escaped `<pre>` — maud escapes it, so a `<script>` in the content shows as text
/// and never executes. We never emit it as live HTML.
pub fn text_view_page(text: &str) -> Markup {
    let body = html! {
        pre.text-body #text-body { (text) }
        button.btn.btn-block #copy-text type="button" { "Copy" }
    };
    document(body, html! { script src="/static/text.js" {} })
}

// --------------------------------------------------------------------------
// Tombstones + errors
// --------------------------------------------------------------------------

/// 410 Gone: the link was real but is now spent or withdrawn. Its name stays
/// reserved until expiry, so it cannot be silently repurposed in the meantime.
pub fn gone_page(expires_at: Option<&str>) -> Markup {
    let body = html! {
        p.error-code { "410" }
        p { "This link has been used or withdrawn." }
        @if let Some(exp) = expires_at {
            p.meta { "Its name stays reserved for " (humanize_expires_in(exp)) "." }
        }
        a.btn.btn-block href="/" { "Create a new link" }
    };
    document(body, html! {})
}

/// 404 Not Found: nothing here — expired, recycled, or never existed. Framed as
/// by-design, since every YuioLink is ephemeral.
pub fn not_found_page() -> Markup {
    let body = html! {
        p.error-code { "404" }
        p { "This link has expired or never existed — links on YuioLink are ephemeral." }
        a.btn.btn-block href="/" { "Create a new link" }
    };
    document(body, html! {})
}

/// Generic terse error page (used for 400 on the no-JS form and 500).
pub fn error_page(code: u16, message: &str) -> Markup {
    let body = html! {
        p.error-code { (code) }
        p { (message) }
        footer { a href="/" { "Back to YuioLink" } }
    };
    document(body, html! {})
}
