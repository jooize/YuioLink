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
    document_full("YuioLink", html! {}, body, scripts)
}

/// As [`document`], but with extra `<head>` markup (e.g. OG tags). The masthead
/// `<h1>` is plain text, not a home link — clicking it on the create page would
/// discard whatever the user had typed, so it is no longer a navigation target.
fn document_full(title: &str, head_extra: Markup, body: Markup, scripts: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover";
                meta name="color-scheme" content="light dark";
                title { (title) }
                link rel="stylesheet" href="/static/app.css";
                (head_extra)
            }
            body {
                main.app-window {
                    header {
                        h1 {
                            "YuioLink"
                            // Invisible but selectable: dragging from the title into
                            // the tagline copies "YuioLink — Wieldy Ephemeral Link"
                            // (the tagline itself is select-none to avoid doubling).
                            span.select-only aria-hidden="true" { " — Wieldy Ephemeral Link" }
                        }
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
    url.split('#')
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .unwrap_or(url)
}

/// Split a shoutkey name into its alternating-case words: `runnyDUSK` -> `runny`,
/// `DUSK`. A boundary is any adjacent pair of ASCII letters whose case differs;
/// hyphens (the lone `yo-yo`) stay within their word.
fn name_words(name: &str) -> Vec<&str> {
    let b = name.as_bytes();
    let mut words = Vec::new();
    let mut start = 0;
    for i in 1..b.len() {
        let (p, c) = (b[i - 1], b[i]);
        if (p.is_ascii_lowercase() && c.is_ascii_uppercase())
            || (p.is_ascii_uppercase() && c.is_ascii_lowercase())
        {
            words.push(&name[start..i]);
            start = i;
        }
    }
    if start < b.len() {
        words.push(&name[start..]);
    }
    words
}

/// Render a shoutkey name with each word in an alternating colour, so a multi-word
/// name reads as separate words (`braveOTTER`). Mirrors the client's `nameSpans`.
fn highlight_name(name: &str) -> Markup {
    html! {
        @for (i, word) in name_words(name).into_iter().enumerate() {
            span class=(format!("nw nw-{}", i % 2)) { (word) }
        }
    }
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
    parse_sqlite_utc(expires_at)
        .map(|e| e - now_unix())
        .unwrap_or(0)
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
fn result_output(url: Option<&str>, meta: Markup, note: Option<&str>) -> Markup {
    html! {
        output.result #link-panel tabindex="-1" hidden[url.is_none()] {
            code.result-word #link-word { @if let Some(u) = url { (highlight_name(link_name(u))) } }
            code.result-url #link-element { @if let Some(u) = url { (u) } }
            // Shown when a public link got more than one word because the short
            // tiers are crowded; app.js fills this for the in-place result too.
            small.result-note #result-note hidden[note.is_none()] { @if let Some(n) = note { (n) } }
            div.result-foot {
                small.result-meta #link-expiry { (meta) }
                div.result-actions {
                    button.result-copy #copy-result type="button" hidden { "Copy" }
                }
            }
        }
    }
}

/// The landing page. Works without JavaScript (the form posts to `POST /` and a
/// result page comes back); `app.js` progressively enhances it with live type
/// detection, keyboard shortcuts, an in-place result, and copy.
pub fn index_page(max_ttl_secs: i64) -> Markup {
    let body = html! {
        p.tagline { "Wieldy Ephemeral Link" }

        // Split storage pill (top): left shows the status (and links to the list),
        // right is the local-persistence toggle in its own colour. app.js fills both.
        // Both start hidden: they are blank coloured pills until app.js fills
        // them (renderHistory un-hides), so the no-JS page never shows them empty.
        div.storage-pill {
            a.storage-status #storage-status href="#history" hidden {}
            button.storage-toggle #storage-toggle type="button" hidden {}
        }
        // Shown by app.js when the user turns local history off while links exist.
        p.storage-warning #storage-warning hidden {
            "Local history is off — these links will be gone when you close this page."
        }

        // The created link (latest), shown above the input. app.js fills it in place;
        // the no-JS path reloads to a result page.
        (result_output(None, html! {}, None))

        form #create-form method="post" action="/" {
            label.visually-hidden for="content" { "Link or text to share" }
            textarea #content.form-control name="content" rows="1"
                autocomplete="off" autocapitalize="off" spellcheck="false"
                placeholder="Link or text to share" autofocus {}

            div.split-btn {
                button #submit.btn.split-primary type="submit" { "Create Link" }
                // Dead without JS; app.js un-hides it when it wires the handler.
                button #clear.btn.split-clear type="button" hidden { "Clear" }
            }
            p.form-error #form-error role="alert" hidden {}

            fieldset.picker.type-picker {
                legend.visually-hidden { "Link Type" }
                div.segmented {
                    input.seg-radio #type-public type="radio" name="link_type" value="public" checked;
                    label.seg-label.dot.t-public for="type-public" { "Public" }
                    input.seg-radio #type-private type="radio" name="link_type" value="private";
                    label.seg-label.dot.t-private for="type-private" { "Private" }
                    input.seg-radio #type-once type="radio" name="link_type" value="once";
                    label.seg-label.dot.t-once for="type-once" { "One-Time" }
                }
                // One shared native disclosure under the picker. Only the selected
                // type's fragments show (CSS :has, so it works without JavaScript),
                // and the open state carries across type switches. The toggle word
                // is "Security" for all three types.
                details.note {
                    summary {
                        span.summary-txt {
                            span.for-public {
                                "Convenient link with 1 to 3 words. "
                                span.summary-sub { "Not private!" }
                            }
                            span.for-private {
                                "Private link with 4 words. "
                                span.summary-sub { "47-bit namespace." }
                            }
                            span.for-once {
                                "Single-use link with 4 words. "
                                span.summary-sub { "47-bit namespace." }
                            }
                        }
                        span.summary-toggle {
                            "Security"
                            svg.chev width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" {
                                path d="M2 3.5 L5 6.5 L8 3.5" fill="none" stroke="currentColor"
                                    stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" {}
                            }
                        }
                    }
                    div.details-body.for-public {
                        "Public names are short words from a public wordlist, so anyone can "
                        "run the whole list and turn up every public link. "
                        strong { "Ideal for convenience and easy sharing" }
                        " — never for anything secret. "
                        a href="/wordlist.txt" { "Browse the wordlist →" }
                    }
                    div.details-body.for-private {
                        "Link name is four random words from a 47-bit namespace — about "
                        "153 trillion possibilities — and nothing lists or indexes it, so "
                        "reaching the link means guessing its exact name within its "
                        "lifetime. "
                        strong { "The name is the secret" }
                        ", and it exists only until the link expires."
                    }
                    div.details-body.for-once {
                        strong { "Deleted from the server when revealed" }
                        ", and with the same security as Private links. Recipients first open "
                        "the link to a preview, then have a choice to reveal its destination "
                        "or content. The reveal immediately deletes destination and "
                        "content from the server."
                    }
                }
            }

            fieldset.picker #ttl-picker {
                legend { "Expires After" }
                // JS path (app.js un-hides and drives these): a big readout over a
                // stepped slider whose 17 stops are sensible durations from 1 minute
                // to 7 days. Tapping the readout opens the exact field below.
                button.ttl-readout #ttl-readout type="button" hidden
                    title="Set an exact expiry" {}
                input.ttl-slider #ttl-slider type="range" name="ttl_stop"
                    min="0" max="16" step="1" value="7" hidden
                    aria-label="Expires after";
                div.ttl-ticks #ttl-ticks hidden aria-hidden="true" {
                    span { "1m" } span { "10m" } span { "1h" } span { "1d" } span { "7d" }
                }
                // Exact expiry: the whole control without JavaScript; with it, the
                // escape hatch behind a readout tap. Left empty, the slider governs.
                div.custom-field #ttl-custom-field {
                    input #ttl-custom-value.custom-num name="ttl_custom" type="number"
                        min="1" step="1" inputmode="numeric" placeholder="1"
                        aria-label="Custom expiry amount";
                    div.segmented.unit-segmented {
                        input.seg-radio #ttl-unit-m type="radio" name="ttl_unit" value="m";
                        label.seg-label for="ttl-unit-m" { "minutes" }
                        input.seg-radio #ttl-unit-h type="radio" name="ttl_unit" value="h" checked;
                        label.seg-label for="ttl-unit-h" { "hours" }
                        input.seg-radio #ttl-unit-d type="radio" name="ttl_unit" value="d";
                        label.seg-label for="ttl-unit-d" { "days" }
                    }
                    small.custom-hint { "Up to " (humanize_duration(max_ttl_secs)) }
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
                div.history-head-actions {
                    // "Clear…" folds the two destructive actions away until asked for;
                    // app.js toggles it open to reveal Clear Expired / Clear All.
                    button.history-clear-open #history-clear-open type="button" { "Clear…" }
                    // Clear All sits leftmost so it never lands where "Clear…" was —
                    // the spot under the pointer belongs to the safe green action.
                    button.history-clear #history-clear type="button" hidden { "Clear All" }
                    button.history-clear-expired #history-clear-expired type="button" hidden { "Clear Expired" }
                }
            }
            div.history-body {
                ul.history-list #history-list {}
            }
        }

        footer {
            "A project by " a href="https://github.com/jooize" { "jooize" } " · "
            a href="https://github.com/jooize/YuioLink" { "Source on GitHub" }
        }
    };
    let scripts = html! { script src="/static/app.js" {} };
    document_full(
        "YuioLink — Wieldy Ephemeral Link",
        html! {
            meta name="description" content="Redirects and text snippets that always expire — never permanent, and every link shows where it leads before you go.";
        },
        body,
        scripts,
    )
}

/// The no-JS result page shown after `POST /` creates a link. "Open link" leads
/// to the link's own interstitial (the always-preview), not straight out.
pub fn result_page(
    url: &str,
    kind_label: &str,
    expires_at: &str,
    max_uses: Option<i64>,
    private: bool,
    words: usize,
) -> Markup {
    let meta = html! {
        (kind_label) " · expires " (expires_at) " UTC"
        @match max_uses {
            Some(1) => { " · one-time" }
            Some(max) => { " · max " (max) " uses" }
            None => {}
        }
    };
    // A public link is normally one word; more means the short tiers are crowded.
    let note = (max_uses.is_none() && !private && words > 1).then(|| {
        format!("Short names are in high demand right now, so this link uses {words} words.")
    });
    let body = html! {
        (result_output(Some(url), meta, note.as_deref()))
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
    // noindex: link pages must never end up in a search index — a public link
    // being crawlable would defeat "nothing indexes the name" for everyone.
    let head = html! {
        meta name="robots" content="noindex, nofollow";
        (interstitial_head(&i, one_time))
    };
    document_full("YuioLink", head, body, html! {})
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
        @if one_time {
            span.pv-caution.single { "If this page says the link is gone (410), someone already opened it." }
        } @else {
            span.pv-caution {
                "YuioLinks expire and are reused, so this name can carry different text later. "
                strong { "Revealing spends one view." }
            }
        }
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
    if one_time {
        "Opens Once"
    } else {
        "Limited Use"
    }
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

/// The token-gated revealed page. This is a one-time render: the destination or
/// content was just deleted from the server (see `db::reveal_and_redact`), so a
/// refresh or revisit won't show it again — the page says so up front.
pub fn revealed_page(r: RevealedView) -> Markup {
    let back = html! { p.back-link { a href="/" { "← Create New Link" } } };
    match r.target {
        RevealedTarget::Redirect { url, href } => {
            let body = html! {
                (back)
                (from_line(r.base_host, r.name))
                span.pv-arrow aria-hidden="true" { "↓" }
                (render_url(url))
                @if let Some(w) = idn_warning(url) { (idn_panel(w)) }
                a class=(GO_BTN) href=(href) rel="noopener noreferrer" { (continue_label(url)) }
                p.pv-revealed { "Deleted from the server on this view — refreshing won't bring it back." }
                p.pv-meta { "Expires in " (humanize_expires_in(r.expires_at)) }
                span.pv-caution.single { strong { "Always check the destination." } }
            };
            document(body, html! {})
        }
        RevealedTarget::Text(text) => {
            let body = html! {
                (back)
                p.pv-revealed { "Deleted from the server on this view — refreshing won't bring it back." }
                pre.text-body #text-body { (text) }
                // Dead without JS; text.js un-hides it when it wires the handler.
                button.btn.btn-block #copy-text type="button" hidden { "Copy" }
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
        // Dead without JS; text.js un-hides it when it wires the handler.
        button.btn.btn-block #copy-text type="button" hidden { "Copy" }
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
        a.btn.btn-block href="/" { "Create a New Link" }
    };
    document(body, html! {})
}

/// 404 Not Found: nothing here — expired, recycled, or never existed. Framed as
/// by-design, since every YuioLink is ephemeral.
pub fn not_found_page() -> Markup {
    let body = html! {
        p.error-code { "404" }
        p { "This link has expired or never existed — links on YuioLink are ephemeral." }
        a.btn.btn-block href="/" { "Create a New Link" }
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
