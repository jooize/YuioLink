//! HTML views, rendered with maud (escaped by default).
//!
//! The markup leans on semantic elements — `header`/`main`/`footer`, `fieldset`/
//! `legend` for the radio groups, `output` for created links, `code`/`pre` for
//! machine text — and reserves classes for genuinely styled components.

use std::time::{SystemTime, UNIX_EPOCH};

use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::phone;
use crate::urlview::{self, Hazard, IdnWarning, Piece, Role, Tier, UriView};

/// The crate version, shown in the footer and linked to its release tag.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The date the footer reports as "last updated", as `YYYY-MM-DD`. A constant
/// rather than a build timestamp on purpose: builds stay reproducible, and a
/// visitor wants to know when the site last changed, not when this binary was
/// compiled. Bump it alongside the workspace version.
const RELEASE_DATE: &str = "2026-08-20";

/// A static asset's URL, stamped with the release it belongs to and a
/// fingerprint of the embedded assets.
///
/// The stamp is what lets the handlers answer `immutable` with a year of
/// `max-age`: a deploy changes the query, which is a new cache key, so a client
/// picks up new CSS without ever revalidating the old. Unversioned URLs and a
/// long `max-age` are the combination that strands a stale stylesheet — and so
/// was the version alone: iterative deploys at one version served the old CSS
/// until a forced reload. The fingerprint changes when the bytes do, which is
/// the actual question a cache key answers.
fn asset_url(path: &str) -> String {
    format!("{path}?v={VERSION}-{stamp}", stamp = asset_stamp())
}

/// An FNV-1a fingerprint over every embedded static asset, computed once per
/// process. Not cryptographic — it only has to change when the bytes do. One
/// combined stamp for all four files keeps every page's references in step;
/// the files are small enough that over-busting three of them costs nothing.
fn asset_stamp() -> &'static str {
    static STAMP: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    STAMP.get_or_init(|| {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for file in [
            include_str!("../static/app.css"),
            include_str!("../static/app.js"),
            include_str!("../static/text.js"),
            include_str!("../static/preview.js"),
        ] {
            for byte in file.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        format!("{hash:016x}")
    })
}

/// The inline head script: everything that has to be true before the first paint.
///
/// It marks the document as scripted, so the stylesheet can draw the JS-on state
/// directly instead of app.js rearranging the page after the fact — folding the
/// exact-expiry field away from script was once the site's largest layout shift,
/// and as a CSS rule it costs nothing.
///
/// It then does the same for saved history. Those rows come from `localStorage`,
/// which the server cannot see, so the section sits between the form and the
/// footer and shoves both down the moment app.js renders it — the footer's whole
/// CLS. Reading the count here (and the open/closed choice with it) lets the CSS
/// reserve the rows' height at first paint; app.js fills the space it finds.
///
/// The keys and the shape are app.js's (`HISTORY_KEY` and friends); this is the
/// one other place that knows them, so they move together. A miscount is a
/// smaller shift, never a broken page: everything is inside one `try`, and app.js
/// rewrites `--history-rows` with the real number as soon as it renders.
const PRE_PAINT_JS: &str = "\
document.documentElement.classList.add('js');\
try{if(localStorage.getItem('yuiolink:history:persist')==='1'){\
var s=JSON.parse(localStorage.getItem('yuiolink:history')||'[]');\
var n=Array.isArray(s)?s.filter(function(e){return e&&e.tombstone!=='cleared'}).length:0;\
if(n){var d=document.documentElement;d.classList.add('has-history');\
d.style.setProperty('--history-rows',n);\
if(localStorage.getItem('yuiolink:history:open')==='0')d.classList.add('history-collapsed')}}}catch(e){}";

/// A `<script src>` for one of our own files, carrying this response's CSP nonce.
///
/// Every script tag on the site goes through here or through [`document_shell`]:
/// the policy allows script by nonce alone, so a tag written without one is a tag
/// that does not run.
fn script_tag(path: &str) -> Markup {
    html! { script src=(asset_url(path)) nonce=(crate::security::nonce().as_ref()) {} }
}

/// The shared page shell: head, the glass "app window", and the masthead.
///
/// Every page passes its own `<title>` — there is no bare "YuioLink" fallback,
/// because a tab or a history entry that says only the site name is the one place
/// the visitor cannot tell two of them apart. The masthead `<h1>` is plain text,
/// not a home link: clicking it on the create page would discard whatever the
/// user had typed, so it is no longer a navigation target.
fn document_shell(
    title: &str,
    head_extra: Markup,
    masthead: bool,
    centered: bool,
    body: Markup,
    scripts: Markup,
) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover";
                meta name="color-scheme" content="light dark";
                title { (title) }
                // The pre-paint marker (see `PRE_PAINT_JS`). Inline and in the
                // head on purpose: it must run before first paint, and being part
                // of this response it cannot fail on its own. If app.js itself
                // never arrives the slider still posts natively — only the typed
                // exact value is out of reach.
                script nonce=(crate::security::nonce().as_ref()) { (PreEscaped(PRE_PAINT_JS)) }
                link rel="stylesheet" href=(asset_url("/static/app.css"));
                (head_extra)
            }
            body {
                main.app-window {
                    // One wrapper around everything the page shows, so the phone
                    // sheet can centre its content with a single auto margin.
                    div.sheet-body.centered[centered] {
                        @if masthead {
                            header {
                                // Two-tone wordmark, matching the share card: the accent
                                // lives inside the name rather than in a separate mark.
                                h1 { "Yuio" span.wm-link { "Link" } }
                            }
                        }
                        (body)
                    }
                }
                (scripts)
            }
        }
    }
}

fn document_full(title: &str, head_extra: Markup, body: Markup, scripts: Markup) -> Markup {
    document_shell(title, head_extra, true, false, body, scripts)
}

/// A short page that gets the wordmark but is allowed to float to the middle of a
/// phone sheet: the tombstones and errors, which are a line or two of text.
fn document_short(title: &str, body: Markup, scripts: Markup) -> Markup {
    document_shell(title, html! {}, true, true, body, scripts)
}

/// A page about one link. It carries its own heading (`link_heading`) instead of
/// the wordmark, and centres on a phone.
fn document_link(title: &str, head_extra: Markup, body: Markup, scripts: Markup) -> Markup {
    document_shell(title, head_extra, false, true, body, scripts)
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
/// name reads as separate words (`braveOTTER`), grouped into two halves (`.nwg`).
/// Mirrors the client's `nameSpans`.
///
/// The halves are the only places a hero name may break, so a name too wide for
/// its line wraps between whole words — and a four-word name wraps two-and-two
/// instead of shedding its last word onto a line of its own. An odd count leans
/// the extra word onto the first line (`2 + 1`), which is where the eye starts.
fn highlight_name(name: &str) -> Markup {
    let words = name_words(name);
    let split = words.len().div_ceil(2);
    html! {
        @for (half, group) in [&words[..split], &words[split..]].into_iter().enumerate() {
            @if !group.is_empty() {
                span.nwg {
                    @for (i, word) in group.iter().enumerate() {
                        span class=(format!("nw nw-{}", (split * half + i) % 2)) { (word) }
                    }
                }
            }
        }
    }
}

/// The `<title>` for a page about one link: `YuioLink Redirect: line`.
///
/// Brand first, like every other page here, then the kind, then the name — so a
/// tab or a history entry says what the thing is before it says which one.
///
/// A **public** link also names its destination. Nothing about a public link is
/// secret (its name is one word from a list anyone can walk), and the domain is
/// already on the page, so the tab may as well be useful. A **secret** or
/// **one-time** name stops at the name: those are held by people who chose an
/// unguessable link, and a destination in a tab strip or a history entry outlives
/// the glance it was meant for.
fn link_title(kind: &str, name: &str, destination: Option<&str>) -> String {
    let public = name_words(name).len() < yuiolink_core::LIMITED_WORDS;
    match destination.filter(|_| public) {
        Some(domain) => format!("YuioLink {kind}: {name} → {domain}"),
        None => format!("YuioLink {kind}: {name}"),
    }
}

/// The down arrow between a preview's source line and its destination. An
/// inline SVG rather than a U+2193 character: the text glyph comes from the
/// OS UI font, so it is wide and substantial under SF Pro but thin and small
/// under Segoe UI, and the preview should look the same everywhere. The
/// geometry matches SF Pro's arrow — a stem into a 45-degree, round-capped
/// head — and inherits `currentColor` so the accent color still applies.
fn pv_arrow() -> Markup {
    html! {
        span.pv-arrow aria-hidden="true" {
            svg width="16" height="18" viewBox="0 0 16 18" fill="none" {
                path d="M8 1.5v15M2.25 10.75 8 16.5l5.75-5.75"
                    stroke="currentColor"
                    stroke-width="2.2"
                    stroke-linecap="round"
                    stroke-linejoin="round" {}
            }
        }
    }
}

/// The top-left way out to the site root: a circular glass chip in the
/// window's corner, mirroring the keyboard-help "?" chip on the right. It
/// holds a diagonal up-left arrow, not a back chevron, on purpose: many of
/// these pages are entered cold (a shared link, a bookmark), where "back"
/// would promise a return to a page the visitor has never seen — the arrow
/// points out and up to the root instead, and reads fine either way. The
/// wording lives on as the tooltip and accessible name.
fn home_chip(href: &str, label: &str) -> Markup {
    html! {
        a.home-chip href=(href) aria-label=(label) title=(label) {
            svg width="12" height="12" viewBox="0 0 13 13" fill="none" aria-hidden="true" {
                path d="M10.5 10.5 2.5 2.5M9 2.5H2.5V9"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round" {}
            }
        }
    }
}

/// The "this link leaves YuioLink" mark: a box with an arrow leaving its top-right
/// corner, the convention every OS and browser already uses for it. An inline SVG
/// rather than U+2197 or U+29C9, so it has one shape everywhere and takes the link's
/// own colour; `aria-hidden` because the destination is already in the link text.
fn external_mark() -> Markup {
    html! {
        svg.ext-mark width="10" height="10" viewBox="0 0 12 12" fill="none" aria-hidden="true" {
            // The box, open at the corner the arrow leaves through.
            path d="M7 1.5H10.5V5M10.5 1.5 6.75 5.25M9 7v3.5H1.5V3H5"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
                stroke-linejoin="round" {}
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

/// A coarse, friendly relative expiry like `7 days`, `5 hours`, `48 min`. The
/// view prepends "Expires in " / "frees up in ". Never shows seconds.
///
/// **Days round to the nearest**; minutes and hours still floor. Flooring days
/// made a seven-day link read "Expires in 6 days" from the moment it existed,
/// which reads as a mistake even though it is true, so a day-scale figure holds
/// the number that was asked for through the first half of the day and then
/// steps down. Minutes and hours keep flooring on purpose: near the end, saying
/// more time remains than there is would be the harmful direction of wrong.
/// (This is the server-rendered coarse line only — the live JS countdown in
/// `app.js` has its own set-value-grace scheme and is not touched by this.)
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
        // Halves up: 6 d 12 h reads "7 days", 6 d 11 h reads "6 days".
        let n = (secs + 86400 / 2) / 86400;
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

/// The clock part of a stored expiry, e.g. `14:30 UTC`. `expires_at` comes from
/// SQLite's `datetime('now', ...)`, which is always UTC, so the zone is a
/// constant rather than something to derive. `None` when the value carries no
/// usable time, leaving the caller to fall back to the date alone.
///
/// A card is the one place a link's expiry is read cold, hours or days after it
/// was shared and possibly out of a crawler's cache, so it states an absolute
/// instant — "in 3 hours" would be a lie the moment the card is cached. The
/// minute matters: a TTL can be as short as an hour, and a bare date would say
/// nothing at all about those.
pub fn format_card_time(expires_at: &str) -> Option<String> {
    let time = expires_at.split([' ', 'T']).nth(1)?;
    let mut p = time.split(':');
    let (h, m) = (p.next()?, p.next()?);
    let two_digits = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit());
    (two_digits(h) && two_digits(m)).then(|| format!("{h}:{m} UTC"))
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
                // Copy is the one thing almost everyone came here to do, so it is a
                // full-width button and stays a word, not a symbol. A real link to
                // the created URL (right-click gives Copy Link); app.js fills the
                // href and turns a left click into a copy.
                a.result-copy #copy-result hidden { "Copy" }
                // The secondary pair, in the glyphs and colours the history rows
                // already use. Preview opens this link's own interstitial, so the
                // creator sees exactly what a recipient will see.
                div.result-actions {
                    a.result-preview #preview-result href=[url] target="_blank"
                        rel="noopener noreferrer" hidden[url.is_none()]
                        title="Open this link's preview in a new tab" {
                        svg width="15" height="15" viewBox="0 0 13 13" fill="none" aria-hidden="true" {
                            path d="M2.5 10.5 10.5 2.5M4 2.5h6.5V9"
                                stroke="currentColor" stroke-width="1.8"
                                stroke-linecap="round" stroke-linejoin="round" {}
                        }
                        span { "Preview" }
                    }
                    // Withdrawing needs the creation token, which only the JavaScript
                    // path holds — so this ships hidden and app.js reveals it.
                    button.result-delete #delete-result type="button" hidden
                        title="Stop this link working" {
                        svg width="15" height="15" viewBox="0 0 14 14" fill="none" aria-hidden="true" {
                            path d="M2.6 3.9h8.8M5.6 3.9V2.7c0-.4.3-.7.7-.7h1.4c.4 0 .7.3.7.7v1.2M4 3.9l.5 7c0 .5.4.9.9.9h3.2c.5 0 .9-.4.9-.9l.5-7"
                                stroke="currentColor" stroke-width="1.4"
                                stroke-linecap="round" stroke-linejoin="round" {}
                        }
                        span { "Delete" }
                    }
                }
                // Filled by app.js when Delete is pressed: a prompt and the two ways
                // out. Deleting never frees the name — the row becomes a tombstone
                // and the name stays reserved until the link would have expired — so
                // the prompt promises only that the link stops working.
                div.result-confirm #result-confirm hidden {}
            }
        }
    }
}

/// The landing page. Works without JavaScript (the form posts to `POST /` and a
/// result page comes back); `app.js` progressively enhances it with live type
/// detection, keyboard shortcuts, an in-place result, and copy.
pub fn index_page(max_ttl_secs: i64) -> Markup {
    let body = html! {
        p { "Wieldy Ephemeral Links" }

        // Keyboard-shortcuts help: a quiet "?" in the window corner opening a
        // native <dialog>. The shortcuts only exist with JavaScript, so both ship
        // hidden and app.js un-hides the button (and fills in ⌘/Ctrl per platform).
        button.kbd-help #kbd-help type="button" hidden
            aria-label="Keyboard shortcuts" title="Keyboard shortcuts" { "?" }
        dialog.kbd-dialog #kbd-dialog aria-label="Keyboard shortcuts" {
            h2 { "Keyboard Shortcuts" }
            dl.kbd-list {
                dt { kbd { "Enter" } }
                dd { "Create the link (when the input is a link)" }
                dt { kbd { "Shift" } " " kbd { "Enter" } }
                dd { "Insert a new line" }
                dt { kbd.k-mod { "⌘" } " " kbd { "Enter" } }
                dd { "Create a text link" }
                dt { "Hold " kbd.k-alt { "⌥" } }
                dd { "Share a link as text instead" }
                dt { kbd.k-mod { "⌘" } " " kbd { "C" } }
                dd { "Copy the link you just created" }
                dt { kbd { "?" } }
                dd { "Show this overview" }
                dt { kbd { "Esc" } }
                dd { "Close this overview" }
            }
            form method="dialog" {
                button.btn.btn-block type="submit" { "Done" }
            }
        }

        // Split storage pill (top): left shows the status (and links to the list),
        // right is the local-persistence toggle in its own colour. app.js fills both.
        // Both start hidden: they are blank coloured pills until app.js fills
        // them (renderHistory un-hides), so the no-JS page never shows them empty.
        div.storage-pill {
            a.storage-status #storage-status href="#history" hidden {}
            button.storage-toggle #storage-toggle type="button" hidden {}
        }
        // Twin of the warning under the History heading: app.js shows whichever one
        // sits at the switch the user actually flipped, so the consequence appears
        // where the action happened rather than always at the bottom of the page.
        p.storage-warning.at-pill #storage-warning-pill hidden {
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
                // Dead without JS, so CSS hides it there (html:not(.js)).
                button #clear.btn.split-clear type="button" { "Clear" }
            }
            p.form-error #form-error role="alert" hidden {}

            fieldset.picker.type-picker {
                legend.visually-hidden { "Link Type" }
                div.segmented {
                    input.seg-radio #type-public type="radio" name="link_type" value="public" checked;
                    label.seg-label.dot.t-public for="type-public" { "Public" }
                    input.seg-radio #type-secret type="radio" name="link_type" value="secret";
                    label.seg-label.dot.t-secret for="type-secret" { "Secret" }
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
                                span.summary-sub { "Not secret!" }
                            }
                            span.for-secret {
                                "Secret link with 4 words. "
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
                        "Public links are short words from a public wordlist, so anyone "
                        "can list every possible link and view the "
                        "data/contents/destination. "
                        strong { "Ideal for convenience and easy sharing" }
                        ", but not for anything secret. "
                        a href="/wordlist.txt" { "Browse the wordlist →" }
                    }
                    div.details-body.for-secret {
                        strong.nowrap { "The name is the secret" }
                        ", and it is reachable only until the link expires. Our server "
                        "knows the data/contents/destination, but deletes it at the "
                        "point of expiry. "
                        // Same wording as One-Time's link: both go to /help#types,
                        // and a shared destination reads as one action, not two.
                        // Public's link stays distinct — it goes somewhere else
                        // (the wordlist), so it earns its own words.
                        a href="/help#types" { "Learn more →" }
                    }
                    div.details-body.for-once {
                        strong { "Deleted from our server when revealed" }
                        ", and with the same security as a secret link. Recipient is "
                        // Every link previews (docs/PREVIEW.md), but it only matters
                        // here: the preview is what keeps an unfurler or prefetch bot
                        // from burning the link before the recipient sees it.
                        "shown a concealed preview with a button to reveal. "
                        a href="/help#types" { "Learn more →" }
                    }
                }
            }

            fieldset.picker #ttl-picker {
                legend { "Expires after" }
                // JS path (CSS shows these, app.js drives them): a big readout over a
                // stepped slider whose 17 stops are sensible durations from 1 minute
                // to 7 days. Tapping the readout opens the exact field below.
                button.ttl-readout #ttl-readout type="button"
                    title="Set an exact expiry" {}
                // The slider and its ticks work without JavaScript too (the index
                // posts as ttl_stop); only the live readout needs app.js.
                input.ttl-slider #ttl-slider type="range" name="ttl_stop"
                    min="0" max="16" step="1" value="16"
                    aria-label="Expires after";
                // Labeled landmarks under the track, each a shortcut to its stop
                // (app.js wires the clicks; without it they are inert labels).
                div.ttl-ticks #ttl-ticks {
                    button.ttl-tick type="button" data-stop="0" { "1 min" }
                    button.ttl-tick type="button" data-stop="3" { "10 min" }
                    button.ttl-tick type="button" data-stop="7" { "1 hour" }
                    button.ttl-tick type="button" data-stop="12" { "1 day" }
                    button.ttl-tick type="button" data-stop="16" { "7 days" }
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
                    // role=status: screen readers announce the over-limit swap.
                    small.custom-hint role="status" { "Up to " (humanize_duration(max_ttl_secs)) }
                }
            }

        }

        // Created-link history (bottom). Kept in memory for the session unless the
        // user ticks "Save on this device", which opts into localStorage.
        //
        // Both of its states live on the root element, not here: `has-history`
        // shows the section at all (so a page with no script, or no links, never
        // shows an empty one), `history-collapsed` folds the list away. The
        // pre-paint script sets them from storage; app.js keeps them true after.
        section.history #history {
            div.history-head {
                button.history-toggle #history-toggle type="button" {
                    span.history-chevron aria-hidden="true" { "›" }
                    span.history-title { "Local History" }
                }
                // The localStorage opt-in, right beside the heading: a bare HIG
                // switch (state by colour + knob position; app.js drives it).
                button.history-persist #history-persist type="button" hidden
                    title="Save history on this device" {}
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
            // Shown by app.js when the user turns local history off from THIS switch
            // while links exist — the twin above sits under the top pill's toggle.
            p.storage-warning.at-history #storage-warning-history hidden {
                "Local history is off — these links will be gone when you close this page."
            }
            div.history-body {
                ul.history-list #history-list {}
            }
        }

        footer {
            // Stacked: the two site pages first (the links a visitor actually
            // uses), the credit under them, the source link on its own line.
            a href="/help" { "Help" }
            " · "
            a href="/stats" { "Statistics" }
            // The credit links neither name: the source link on the next line is
            // where someone who wants to look goes. Only jooize takes a colour —
            // "AI" is a category, not a name, so a second identifying colour
            // would be decoration, and it sits back in tertiary instead.
            span.footer-line {
                "Created by " span.by-jooize { "jooize" } " + " span.by-ai { "AI" }
            }
            // The terms first, then where to go and read them. The arrow says the
            // link leaves the site, which every other link in this footer does not.
            span.footer-line {
                // The licence links inward, to the colophon, which also carries
                // the attributions it does not cover; the source links outward.
                // The label names the licence rather than the page: "Colophon" is
                // the nicer word for the URL but tells a visitor nothing.
                a href="/colophon" { "MIT/Apache-2.0" }
                ", "
                a.ext href="https://github.com/jooize/YuioLink" {
                    "GitHub" (external_mark())
                }
            }
            span.footer-updated {
                "Updated " (format_card_date(RELEASE_DATE)) " · "
                a href=(format!("https://github.com/jooize/YuioLink/releases/tag/v{VERSION}")) {
                    "v" (VERSION)
                }
            }
        }
    };
    let scripts = script_tag("/static/app.js");
    document_full(
        "YuioLink — Wieldy Ephemeral Links",
        html! {
            meta name="description" content="Redirects and text snippets that always expire — never permanent, and every link shows where it leads before you go.";
        },
        body,
        scripts,
    )
}

/// The no-JS result page shown after `POST /` creates a link. "Open link" leads
/// to the link's own interstitial (the always-preview), not straight out.
/// What the no-JS result page needs to offer "the same content, the other kind".
///
/// The no-JS path has no way to change a link after the fact — it is issued no
/// delete token, since there is nowhere to keep one — so the offer creates a
/// second link rather than converting the first. The copy has to say so: the
/// redirect stays until it expires. This is the whole override on that path,
/// standing in for the Option key, which does not exist on a phone and does not
/// exist without JavaScript.
pub struct ResultRedo<'a> {
    /// The exact content that was just submitted, re-posted verbatim.
    pub content: &'a str,
    /// Expiry to reuse, in seconds, so the second link matches the first
    /// instead of silently falling back to the default.
    pub ttl_seconds: i64,
    /// `public` / `secret` / `once`, likewise carried over.
    pub link_type: &'a str,
}

pub fn result_page(
    url: &str,
    kind_label: &str,
    expires_at: &str,
    max_uses: Option<i64>,
    secret: bool,
    words: usize,
    redo: Option<&ResultRedo>,
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
    let note = (max_uses.is_none() && !secret && words > 1).then(|| {
        format!("Short names are in high demand right now, so this link uses {words} words.")
    });
    let body = html! {
        (home_chip("/", "Create New Link"))
        (result_output(Some(url), meta, note.as_deref()))
        a.btn.btn-block href=(url) { "Open link" }
        // Only offered after a Redirect: a non-URL is already Text, so there is
        // no other kind to offer it as.
        @if let Some(r) = redo {
            form.redo-form method="post" action="/" {
                input type="hidden" name="content" value=(r.content);
                input type="hidden" name="kind" value="text";
                input type="hidden" name="ttl_seconds" value=(r.ttl_seconds);
                input type="hidden" name="link_type" value=(r.link_type);
                button.redo-btn type="submit" { "Share the address as a text link instead" }
            }
            p.meta.redo-note {
                "Creates a second link with the same content. The redirect above "
                "keeps working until it expires."
            }
        }
        // No "Create another" link: the back chip in the corner already goes to
        // the create page, and it said the same thing twice.
    };
    let scripts = script_tag("/static/app.js");
    // The destination is what the creator just typed, so naming it here would tell
    // them nothing; the title identifies which link they are looking at.
    document_full(
        &link_title(kind_label, link_name(url), None),
        html! {},
        body,
        scripts,
    )
}

// --------------------------------------------------------------------------
// Interstitial (always-preview)
// --------------------------------------------------------------------------

/// What the interstitial is gating.
pub enum Target<'a> {
    /// A redirect, with its destination already cut into parts for display and
    /// the canonical stored string that an `href` would carry.
    Redirect { uri: &'a UriView, href: &'a str },
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

/// The mandatory preview shown for `GET /:name`. Spends no use; revealing is a
/// separate POST. An unlimited redirect shows the full destination and a real
/// amber `<a href>` — a link, not a form, so nothing about following it can be
/// blocked by the CSP's `form-action`; a one-time link discloses nothing at all
/// until its Reveal button spends the use.
///
/// `max_uses` is only ever `None` or `Some(1)` (the create surfaces reject
/// anything else), so "limited" and "one-time" are the same condition here.
pub fn interstitial_page(i: Interstitial) -> Markup {
    let one_time = i.max_uses.is_some();

    let kind = match &i.target {
        Target::Redirect { .. } => Kind::Redirect,
        Target::TextSnippet => Kind::Text,
    };
    let body = html! {
        (link_heading(kind, i.name, i.base_host, home_chip("/", "Go to YuioLink")))
        (pv_arrow())
        @match &i.target {
            _ if one_time => (blind_reveal_block(&i)),
            Target::Redirect { uri, href } => (redirect_card(&i, uri, href)),
            Target::TextSnippet => (blind_reveal_block(&i)),
        }
    };
    // noindex: link pages must never end up in a search index — a public link
    // being crawlable would defeat "nothing indexes the name" for everyone.
    let head = html! {
        meta name="robots" content="noindex, nofollow";
        (interstitial_head(&i, one_time))
    };
    let title = match &i.target {
        Target::Redirect { uri, .. } => link_title("Redirect", i.name, Some(&uri.card_domain())),
        Target::TextSnippet => link_title("Text", i.name, None),
    };
    // preview.js only wires ⌘C to the destination, and no-ops when there is no
    // destination on the page (a limited link shows just the domain).
    document_link(&title, head, body, script_tag("/static/preview.js"))
}

/// `<head>` Open Graph / theme-color tags so a shared link unfurls trustworthily.
fn interstitial_head(i: &Interstitial, one_time: bool) -> Markup {
    match &i.target {
        Target::Redirect { uri, .. } => {
            // A one-time link names no destination here either. The page is
            // blind until the use is spent, and an unfurl runs on every chat
            // server the link passes through: naming the domain in an og tag
            // would disclose it to all of them, for free, and leave the
            // recipient's 410 no longer meaning "someone opened it".
            let title = if one_time {
                "One-time link on YuioLink".to_string()
            } else {
                format!("Redirect to {}", uri.card_domain())
            };
            let date = format_card_date(i.expires_at);
            let kind = if one_time { "Single-use" } else { "Ephemeral" };
            let expiry = match format_card_time(i.expires_at) {
                Some(time) => format!("{kind} redirect that expires {date} at {time}."),
                None => format!("{kind} redirect that expires {date}."),
            };
            let desc = if one_time {
                format!("{expiry} {BLIND_LINE}")
            } else {
                expiry
            };
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

/// The heading every page about one link now carries: a quiet "YuioLink Redirect"
/// kicker over the link name, with the bare host beneath. It replaces both the
/// generic wordmark and the old `yuio.link/<name>` source line — the name was the
/// only thing that line added, and it says it far better as the hero.
///
/// The kind word takes the colour it already has in the history list (Redirect
/// accent blue, Text orange) and is the only colour on the kicker; the name stays
/// greyscale, its alternating case doing the work of separating the words. The
/// whole block is one `<h1>`: it is one heading, and it reads as one.
///
/// The back chip is a sibling, not part of the heading — assistive tech should not
/// hear "Go to YuioLink" inside the page title — but it lives in this block so that
/// it travels with the content when the phone sheet centres it.
fn link_heading(kind: Kind, name: &str, host: &str, back: Markup) -> Markup {
    html! {
        div.pv-head {
            (back)
            h1 {
                span.kicker {
                    "YuioLink "
                    span class=(format!("kind kind-{}", kind.slug())) { (kind.label()) }
                }
                span.pv-name { (highlight_name(name)) }
            }
            span.pv-hostline { (host) }
        }
    }
}

/// What a link is, for the heading and the `<title>`.
#[derive(Clone, Copy)]
pub enum Kind {
    Redirect,
    Text,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Redirect => "Redirect",
            Kind::Text => "Text",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Kind::Redirect => "redirect",
            Kind::Text => "text",
        }
    }
}

/// What a one-time link says instead of a destination, on the page and in the
/// unfurl. It states what will happen rather than that something is being
/// withheld, because the server genuinely has not disclosed anything yet.
pub const BLIND_LINE: &str = "The destination is shown when revealed.";

/// The same, shortened for the share card's hero line.
pub const BLIND_HERO: &str = "Shown when revealed";

/// The one-time card, for both kinds. It discloses nothing — not the domain,
/// not the scheme — because disclosing it without spending the use would let
/// anyone holding the link learn where it points invisibly, and the spent use
/// is the whole tamper-evidence a one-time link offers. The line says what will
/// happen rather than that something is being withheld: the server genuinely
/// has not given anything away yet.
fn blind_reveal_block(i: &Interstitial) -> Markup {
    let text = matches!(i.target, Target::TextSnippet);
    html! {
        @if text { span.pv-host.plain { "A text snippet" } }
        span.pv-blind {
            @if text { "The text is shown when revealed." }
            @else { (BLIND_LINE) }
        }
        (consume_form(
            &format!("/{}/reveal", i.name),
            REVEAL_BTN,
            if text { "Reveal Text" } else { "Reveal Destination" },
        ))
        div.pv-badge-wrap { span.pv-badge.once { "Opens Once" } }
        p.pv-meta { "Expires in " (humanize_expires_in(i.expires_at)) }
        span.pv-caution.single { "If this page says the link is gone (410), someone already opened it." }
    }
}

// --------------------------------------------------------------------------
// The redirect card
// --------------------------------------------------------------------------
//
// Three registers, and every character of the stored string appears in at
// least one of them:
//
//   headline    what the link IS, formatted for reading
//   slices      what it CARRIES, each row a verbatim cut
//   exact line  what is STORED, character for character
//
// The headline may be formatted precisely because the exact line sits
// underneath; where the headline already is the stored string, character for
// character, there is nothing to prove and no exact line appears.

/// The whole card for a live redirect, from the tier down to the caution.
fn redirect_card(i: &Interstitial, uri: &UriView, href: &str) -> Markup {
    // Render-time allowlist check. It is the only thing standing between a
    // stored string and an `href`, on this page and on the revealed one, so it
    // runs where the markup is written rather than in a handler that a future
    // route could bypass.
    if uri.tier == Tier::Refused || !is_linkable(href) {
        return html! {
            (refusal_block(uri))
            p.pv-meta { "Expires in " (humanize_expires_in(i.expires_at)) }
        };
    }
    html! {
        (card_body(uri, href))
        p.pv-meta { "Expires in " (humanize_expires_in(i.expires_at)) }
        span.pv-caution {
            // "Will", not "may": reuse is the design, not an accident, and the
            // caution's job is to make the reader believe it.
            "YuioLinks expire and are reused, so will point elsewhere later. "
            strong { "Always check the destination." }
        }
    }
}

/// Everything between the arrow and the expiry line, shared by the preview page
/// and the revealed page so a one-time link's second screen is the same card.
fn card_body(uri: &UriView, href: &str) -> Markup {
    let numbers = phone_numbers(uri);
    html! {
        (headline(uri, &numbers))
        @if let Some(w) = uri.idn_warning() { (idn_panel(w)) }
        (chip_row(uri, &numbers))
        // A chip with more to say answers on click: its note appears here
        // (the user's call, 2026-08-24). No-JS shows the notes in full.
        (hazard_notes(uri))
        // On http(s) the cast answers directly under the hero: click a
        // marked character and its entry appears here (the fold that used
        // to house it is gone — design note 30, round 3). The no-JS page
        // shows the list in full.
        @if uri.tier == Tier::Web { (cast_list(uri, true)) }
        (slice_section(uri))
        (notes(uri))
        (exact_line(uri))
        (action_button(uri, href, &numbers))
        @if uri.tier == Tier::Handoff {
            // The one hedge, once. "If anything" is load-bearing: a magnet
            // with nothing registered does nothing at all, and we cannot see
            // the reader's machine to know.
            span.pv-hedge { "What opens it, if anything, is up to your device." }
        }
    }
}

/// Tier 3: printed, never linked, and given no control at all — not a disabled
/// one. The panel stays neutral; red lives only in the 16px symbol, because the
/// red fill belongs to the lookalike-domain warning, where an actual attack is
/// on the page.
fn refusal_block(uri: &UriView) -> Markup {
    html! {
        code.pv-url.inert { (url_line_parts(uri)) }
        div.pv-refuse-alert {
            svg.sym width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true" {
                circle cx="8" cy="8" r="6.6" stroke="currentColor" stroke-width="1.4" {}
                path d="M8 4.6v4.2" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" {}
                circle cx="8" cy="11.2" r=".95" fill="currentColor" {}
            }
            p.msg {
                b { "An Instruction, Not an Address" }
                "What is stored here would tell your browser to do something rather than go "
                "somewhere, so YuioLink shows it, gives it no button, and stops there."
            }
        }
    }
}

/// True when a stored destination may be emitted as an `href`. This is the
/// render-time replacement for the check that used to live in the deleted
/// `POST /:name/go` handler, and it covers the revealed page too — that page
/// emitted `href=(content)` with no check of its own.
fn is_linkable(target: &str) -> bool {
    yuiolink_core::validate_redirect(target, yuiolink_core::DEFAULT_ALLOWED_SCHEMES).is_ok()
}

// --------------------------------------------------------------------------
// Register 1: the headline
// --------------------------------------------------------------------------

/// Schemes whose headline is the stored string, character for character. They
/// wear it as one inline run (`.pv-line`) so a selection across a soft wrap
/// still copies one unbroken string — and they carry no exact line, because
/// there is nothing about them left to prove.
fn is_one_run(scheme: &str) -> bool {
    matches!(
        scheme,
        "spotify" | "matrix" | "irc" | "ircs" | "ftp" | "ftps"
    )
}

fn headline(uri: &UriView, numbers: &[phone::Number]) -> Markup {
    match uri.scheme.as_str() {
        // The hero carries the parts model when anything is removable: it is
        // the editor now (design note 30, round 3) — preview.js wires each
        // `data-slice` wrapper to strike on click and rebuild the button.
        "http" | "https" => html! {
            code.pv-url #destination data-card=[web_model(uri)] { (url_line_parts(uri)) }
        },
        _ if is_one_run(&uri.scheme) => html! {
            code.pv-line #destination {
                span.sch-big { (uri.scheme) span.colon { ":" } }
                wbr;
                (one_run_body(uri))
            }
        },
        // A magnet IS its list, so the slices are the hero and the plate is the
        // whole headline.
        "magnet" => scheme_plate(&uri.scheme),
        "tel" | "sms" => html! {
            (scheme_plate(&uri.scheme))
            @if numbers.len() > 1 { (number_stack(uri, numbers)) }
            @else if let Some(n) = numbers.first() { (number_value(uri, n)) }
        },
        "mailto" => html! {
            (scheme_plate(&uri.scheme))
            (address_headline(uri))
        },
        _ => html! {
            (scheme_plate(&uri.scheme))
            @if let Some(body) = uri.first(Role::Opaque).filter(|s| !s.value.is_empty()) {
                span.pv-value { (pieces(&body.display, PieceStyle::Headline)) }
            }
        },
    }
}

/// The scheme, large and plain bold. It does not take the accent wash: the
/// wash appears exactly once per page and belongs to the registrable domain,
/// which is who you are dealing with. Size is this one's emphasis.
fn scheme_plate(scheme: &str) -> Markup {
    html! { span.pv-scheme { (scheme) span.colon { ":" } } }
}

/// The body of a one-run headline, with a `<wbr>` after each delimiter so a
/// narrow card breaks at the joints rather than mid-token. Soft wraps are
/// layout, not characters — the clipboard never sees them.
fn one_run_body(uri: &UriView) -> Markup {
    html! {
        @for slice in &uri.slices {
            @match slice.role {
                Role::Host => {
                    span.dl { "//" } wbr;
                    // The one-run promise is character-identity with storage,
                    // so the host renders from the stored characters — never
                    // from the decoded reading the http(s) hero uses.
                    (pieces(&urlview::host_pieces(&slice.value), PieceStyle::Headline))
                }
                Role::Port => { span.dl { ":" } span.port { (slice.value) } }
                Role::Path => (path_run(&slice.value)),
                _ => {
                    @if !slice.delim.is_empty() { span.dl { (slice.delim) } wbr; }
                    @if let Some(k) = &slice.key {
                        span.k { (pieces(&urlview::key_reading(k), PieceStyle::Headline)) }
                    }
                    @if slice.equals { span.dl { "=" } }
                    (pieces(&slice.display, PieceStyle::Headline))
                }
            }
        }
    }
}

/// Recipients as the headline. One address reads as a value; several read as a
/// comma-joined inline run, character-identical to the stored list so a
/// selection across it copies the list unbroken, wrapping one address per line
/// on a phone.
fn address_headline(uri: &UriView) -> Markup {
    let to: Vec<&urlview::Slice> = uri.recipients().collect();
    html! {
        @if to.len() == 1 {
            span.pv-value { (pieces(&to[0].display, PieceStyle::Headline)) }
        } @else if to.len() > 1 {
            code.pv-list {
                @for (n, slice) in to.iter().enumerate() {
                    @if n > 0 { span.dl { "," } wbr; }
                    (pieces(&slice.display, PieceStyle::Headline))
                }
            }
        }
    }
}

/// One number, dressed the way its own country's tables dress it: the country
/// code recedes, the separators go tertiary, the national digits keep the bold.
/// Recede what routes, bold what identifies.
fn number_value(uri: &UriView, n: &phone::Number) -> Markup {
    html! {
        span.pv-value {
            @if let Some(cc) = &n.country_code { span.cc { (cc) } " " }
            (number_parts(&n.national))
            // libphonenumber's own rendering of an extension, riding the
            // number rather than sitting in a table of its own.
            @if let Some(ext) = uri.slices.iter().find(|s| s.key.as_deref() == Some("ext")) {
                span.ext { "ext. " (ext.value) }
            }
        }
    }
}

/// Several numbers, one per line. A phone list read as a comma-joined run is
/// messy, and a stack can align: country codes right into the seam, national
/// numbers sharing a left edge, tabular digits under each other. Each number
/// carries its own facts, because a Premium Rate warning has to point at ITS
/// number — a pooled row cannot say which is which.
fn number_stack(uri: &UriView, numbers: &[phone::Number]) -> Markup {
    let to: Vec<&urlview::Slice> = uri.recipients().collect();
    html! {
        div.pv-stack2 {
            @for (n, number) in numbers.iter().enumerate() {
                @let at = to.get(n).map(|s| index_of(uri, s));
                span.cc data-slice=[at] { (number.country_code.as_deref().unwrap_or("")) }
                span.nn data-slice=[at] { (number_parts(&number.national)) }
                span.facts data-slice=[at] { (number_chips(number)) }
            }
        }
    }
}

fn number_parts(national: &str) -> Markup {
    html! {
        @for part in phone::parts(national) {
            @match part {
                phone::Part::Digits(d) => { (d) }
                phone::Part::Separator(s) => { span.sep { (s) } }
            }
        }
    }
}

/// Every recipient of a `tel:`/`sms:` URI, read against the numbering plans.
/// `tel:` has exactly one by RFC 3966, and it is the opaque body rather than a
/// recipient slice.
fn phone_numbers(uri: &UriView) -> Vec<phone::Number> {
    match uri.scheme.as_str() {
        "tel" => uri
            .first(Role::Opaque)
            .map(|s| vec![phone::read(&s.value)])
            .unwrap_or_default(),
        "sms" => uri.recipients().map(|s| phone::read(&s.value)).collect(),
        _ => Vec::new(),
    }
}

// --------------------------------------------------------------------------
// Register 2: the slices
// --------------------------------------------------------------------------

/// The rows — non-web schemes only, listed outright, because on those cards
/// the parts are most of what there is to read. http(s) has no section at all
/// any more: the hero is the reading and the editor (design note 30, round 3),
/// and the fold that used to live here is gone.
fn slice_section(uri: &UriView) -> Markup {
    if uri.tier == Tier::Web {
        return html! {};
    }
    let rows: Vec<&urlview::Slice> = uri.rows().collect();
    if rows.is_empty() {
        return html! {};
    }
    // The rule follows the parts, never the scheme: a section that could only
    // restate a row nobody can act on has not earned its line. That is what
    // leaves the plain ftp card and the tel card with no rows at all.
    if !uri.fold_is_worth_it() {
        return html! {};
    }
    html! {
        (cast_list(uri, false))
        (slice_rows(uri, &rows))
    }
}

fn slice_rows(uri: &UriView, rows: &[&urlview::Slice]) -> Markup {
    html! {
        div.pv-slices data-card=(card_model(uri)) {
            @for slice in rows {
                div class=(slice_class(slice)) data-slice=(index_of(uri, slice)) {
                    span.txt {
                        @if !slice.delim.is_empty() { span.dl { (slice.delim) } }
                        @if let Some(k) = &slice.key {
                            span.k { (pieces(&urlview::key_reading(k), PieceStyle::Slice)) }
                        }
                        @if slice.equals { span.dl { "=" } }
                        // The value is one inline block, so a long one (a
                        // magnet hash) drops to its own line whole instead of
                        // orphaning its last three characters, and only breaks
                        // inside itself when a line genuinely cannot hold it.
                        span class=(value_class(slice)) {
                            @if slice.role == Role::Path { (path_slices_markup(&slice.display)) }
                            @else { (pieces(&slice.display, PieceStyle::Slice)) }
                        }
                    }
                }
            }
        }
    }
}

/// The extra class a slice's value carries, if any.
///
/// Only the port has one. An explicit port is unusual, and it decides which
/// server actually answers, so it gets a colour of its own rather than another
/// shade of the same grey — see `--c-port` in app.css.
fn value_class(slice: &urlview::Slice) -> &'static str {
    if slice.role == Role::Port {
        "val port"
    } else {
        "val"
    }
}

fn slice_class(slice: &urlview::Slice) -> String {
    let mut c = String::from("pv-slice");
    if slice.role == Role::Userinfo {
        c.push_str(" usr");
    }
    c
}

fn index_of(uri: &UriView, slice: &urlview::Slice) -> usize {
    uri.slices
        .iter()
        .position(|s| std::ptr::eq(s, slice))
        .unwrap_or(0)
}

// --------------------------------------------------------------------------
// Register 3: the exact line
// --------------------------------------------------------------------------

/// "Exactly as stored" — the record, wherever the headline is a *rendering*:
/// a formatted number, a comma-joined address list, a decoded value.
///
/// On an http(s) card the record lives behind the small "Stored Form"
/// disclosure (design note 30, round 3). On a one-run card the headline is
/// the stored string outright, so the record never appears. Everywhere else
/// it is unconditional and never collapsed.
fn exact_line(uri: &UriView) -> Markup {
    match uri.scheme.as_str() {
        "http" | "https" => stored_details(uri),
        s if is_one_run(s) => html! {},
        _ => raw_record(uri),
    }
}

/// The record behind a small native disclosure, with the drift line beside it
/// (round 3 picks: record = link, driftnote = line). `<details>`, so the
/// no-JS page keeps its path to the bytes. It appears only when the reading
/// differs from storage beyond receipted `%20` spaces — an undecoded card
/// would only restate the hero, and a spaces-only card's receipt and cast
/// entry already name the stored form (the user's call, 2026-08-20).
fn stored_details(uri: &UriView) -> Markup {
    if !uri.decoding_changed_more_than_spaces() {
        return html! {};
    }
    html! {
        details.pv-stored {
            summary.pv-stored-lid { "Stored Form" }
            (raw_record(uri))
            // The drift, named once, where the bytes live: the hero is a
            // reading, so a selection copies the readable characters.
            span.pv-drift {
                "Selecting text copies the readable form; the Copy button "
                "copies the link exactly as stored."
            }
        }
    }
}

fn raw_record(uri: &UriView) -> Markup {
    html! {
        code.rawline {
            span.lbl { "Exactly as Stored" }
            span.str { (stored_markup(uri)) }
        }
    }
}

/// The stored string in the shared syntax dress: scheme tertiary, delimiters
/// accent, keys bold, percent escapes dimmed to the encoding noise they are,
/// registrable domains bold but never washed. Nothing here is decoded — this
/// line's whole job is to be the characters.
fn stored_markup(uri: &UriView) -> Markup {
    html! {
        span.sch { (uri.prefix) }
        @for slice in &uri.slices {
            @if !slice.delim.is_empty() { span.dl { (slice.delim) } }
            @if let Some(k) = &slice.key { span.k { (k) } }
            @if slice.equals { span.dl { "=" } }
            (stored_value(&slice.value, slice.role))
        }
    }
}

fn stored_value(value: &str, role: Role) -> Markup {
    let escapes = escape_runs(value);
    html! {
        @for (text, is_escape) in escapes {
            @if is_escape { span.pct { (text) } }
            @else if role == Role::Userinfo { span.usr { (text) } }
            @else if role == Role::Port { span.port { (text) } }
            @else { (text) }
        }
    }
}

/// Split a stored value into alternating plain and `%NN`-escape runs.
fn escape_runs(value: &str) -> Vec<(String, bool)> {
    let bytes = value.as_bytes();
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let escape = bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit();
        let take = if escape {
            3
        } else {
            value[i..].chars().next().map_or(1, char::len_utf8)
        };
        let chunk = &value[i..i + take];
        match out.last_mut() {
            Some((prev, was)) if *was == escape => prev.push_str(chunk),
            _ => out.push((chunk.to_string(), escape)),
        }
        i += take;
    }
    out
}

// --------------------------------------------------------------------------
// The cast: naming the marked characters
// --------------------------------------------------------------------------

/// Why a marked character is in the cast at all — the story its entry tells.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CastKind {
    /// A space stored as `%20` (the dotted receipt).
    Space,
    /// Literal spaces used as padding (the red run).
    Padding,
    /// An escape decoding to something invisible or direction-changing.
    Hidden,
    /// A structure character kept escaped so it cannot redraw the URI.
    Kept,
    /// An escape whose bytes are not valid text — nothing honest to name.
    Undecodable,
}

/// Where an entry's character sits, for the "in the path" clause.
#[derive(Clone, PartialEq, Eq)]
enum CastPlace {
    Path,
    Username,
    Fragment,
    Query,
    Address,
    Key(String),
}

/// One uncommon character the card marked, named. Identical characters
/// collapse to one entry with a count; every mark that drew the character
/// points here through its `data-tell`.
struct CastEntry {
    id: String,
    ch: char,
    stored: String,
    kind: CastKind,
    count: usize,
    places: Vec<CastPlace>,
}

/// The Unicode-chart dress for a character you cannot see: the short symbol
/// worn by the dotted tile, and a lowercase name for the prose. Characters
/// outside the table keep their hex digits as the symbol.
fn char_dress(c: char) -> (String, String) {
    let (sym, name) = match c {
        ' ' => ("SP", "space"),
        '\t' => ("TAB", "tab"),
        '\n' => ("LF", "line feed"),
        '\r' => ("CR", "carriage return"),
        '\u{001b}' => ("ESC", "escape character"),
        '\u{007f}' => ("DEL", "delete"),
        '\u{00a0}' => ("NBSP", "no-break space"),
        '\u{00ad}' => ("SHY", "soft hyphen"),
        '\u{034f}' => ("CGJ", "combining grapheme joiner"),
        '\u{061c}' => ("ALM", "Arabic letter mark"),
        '\u{180e}' => ("MVS", "Mongolian vowel separator"),
        '\u{2000}' => ("NQSP", "en quad"),
        '\u{2001}' => ("MQSP", "em quad"),
        '\u{2002}' => ("ENSP", "en space"),
        '\u{2003}' => ("EMSP", "em space"),
        '\u{2004}' => ("3/MSP", "three-per-em space"),
        '\u{2005}' => ("4/MSP", "four-per-em space"),
        '\u{2006}' => ("6/MSP", "six-per-em space"),
        '\u{2007}' => ("FSP", "figure space"),
        '\u{2008}' => ("PSP", "punctuation space"),
        '\u{2009}' => ("THSP", "thin space"),
        '\u{200a}' => ("HSP", "hair space"),
        '\u{200b}' => ("ZWSP", "zero-width space"),
        '\u{200c}' => ("ZWNJ", "zero-width non-joiner"),
        '\u{200d}' => ("ZWJ", "zero-width joiner"),
        '\u{200e}' => ("LRM", "left-to-right mark"),
        '\u{200f}' => ("RLM", "right-to-left mark"),
        '\u{2028}' => ("LS", "line separator"),
        '\u{2029}' => ("PS", "paragraph separator"),
        '\u{202a}' => ("LRE", "left-to-right embedding"),
        '\u{202b}' => ("RLE", "right-to-left embedding"),
        '\u{202c}' => ("PDF", "pop directional formatting"),
        '\u{202d}' => ("LRO", "left-to-right override"),
        '\u{202e}' => ("RLO", "right-to-left override"),
        '\u{202f}' => ("NNBSP", "narrow no-break space"),
        '\u{205f}' => ("MMSP", "medium mathematical space"),
        '\u{2060}' => ("WJ", "word joiner"),
        '\u{2066}' => ("LRI", "left-to-right isolate"),
        '\u{2067}' => ("RLI", "right-to-left isolate"),
        '\u{2068}' => ("FSI", "first strong isolate"),
        '\u{2069}' => ("PDI", "pop directional isolate"),
        '\u{3000}' => ("IDSP", "ideographic space"),
        '\u{3164}' => ("HF", "hangul filler"),
        '\u{feff}' => ("BOM", "byte order mark"),
        '&' => ("&", "ampersand"),
        '=' => ("=", "equals sign"),
        '#' => ("#", "number sign"),
        '%' => ("%", "percent sign"),
        _ => return (format!("{:04X}", c as u32), "hidden character".to_string()),
    };
    (sym.to_string(), name.to_string())
}

/// The `data-tell` id and hover title for a marked escape run. The run's
/// first character names it, so every mark drawing the same character points
/// at the same cast entry.
fn mark_meta(run: &str) -> (String, String) {
    match urlview::escape_run_pairs(run).first() {
        Some((c, _)) if *c == char::REPLACEMENT_CHARACTER => {
            ("ufffd".to_string(), "undecodable bytes".to_string())
        }
        Some((c, _)) => {
            let (_, name) = char_dress(*c);
            (
                format!("u{:04x}", *c as u32),
                format!("{name} U+{:04X}", *c as u32),
            )
        }
        None => (String::new(), String::new()),
    }
}

fn place_of(slice: &urlview::Slice) -> CastPlace {
    if let Some(k) = &slice.key {
        return CastPlace::Key(k.clone());
    }
    match slice.role {
        Role::Path => CastPlace::Path,
        Role::Userinfo => CastPlace::Username,
        Role::Fragment => CastPlace::Fragment,
        Role::Query => CastPlace::Query,
        _ => CastPlace::Address,
    }
}

/// Walk the same display pieces the card draws and collect one entry per
/// distinct marked character. Hosts and ports never render marks (the host
/// has the IDN machinery instead), so they contribute nothing.
fn cast_entries(uri: &UriView) -> Vec<CastEntry> {
    let mut out: Vec<CastEntry> = Vec::new();
    let mut note = |ch: char, stored: String, kind: CastKind, place: CastPlace, n: usize| {
        let id = if kind == CastKind::Padding {
            "pad".to_string()
        } else {
            format!("u{:04x}", ch as u32)
        };
        match out.iter_mut().find(|e| e.id == id) {
            Some(e) => {
                e.count += n;
                if !e.places.contains(&place) {
                    e.places.push(place);
                }
            }
            None => out.push(CastEntry {
                id,
                ch,
                stored,
                kind,
                count: n,
                places: vec![place],
            }),
        }
    };
    for slice in &uri.slices {
        if matches!(slice.role, Role::Host | Role::Port) {
            continue;
        }
        let place = place_of(slice);
        // A key's hidden characters mark red in the render (key_reading), so
        // they answer here like any other mark. Keys are otherwise verbatim —
        // only the invisible arm can produce an entry.
        if let Some(k) = &slice.key {
            for piece in urlview::key_reading(k) {
                if let Piece::BadEscape(s) = piece {
                    for (c, raw) in urlview::escape_run_pairs(&s) {
                        note(c, raw, CastKind::Hidden, place.clone(), 1);
                    }
                }
            }
        }
        for piece in &slice.display {
            match piece {
                Piece::DecodedSpace => {
                    note(' ', "%20".to_string(), CastKind::Space, place.clone(), 1);
                }
                Piece::Padding(s) => {
                    note(
                        ' ',
                        s.clone(),
                        CastKind::Padding,
                        place.clone(),
                        s.chars().count(),
                    );
                }
                Piece::Escape(s) => {
                    for (c, raw) in urlview::escape_run_pairs(s) {
                        let kind = if c == char::REPLACEMENT_CHARACTER {
                            CastKind::Undecodable
                        } else {
                            CastKind::Kept
                        };
                        note(c, raw, kind, place.clone(), 1);
                    }
                }
                Piece::BadEscape(s) => {
                    for (c, raw) in urlview::escape_run_pairs(s) {
                        note(c, raw, CastKind::Hidden, place.clone(), 1);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// The cast, in note 28's dress: for an invisible character, the dotted-square
/// symbol tile every Unicode chart uses beside one plain field holding the
/// actual character — which appears empty, on purpose. A printable kept escape
/// is its own proof and gets one plain tile.
///
/// With the script running (the user's call, 2026-08-20): a sole entry shows
/// outright — one character to explain needs no ceremony — while several
/// collapse to a compact index of clickable symbol tiles, each opening the
/// entry that names it, the same wiring the marks in the values use. The
/// no-JS page shows the list in full and drops the index instead — that split
/// lives in the stylesheet, keyed on `html.js`.
///
/// `bare` is the http(s) dress (design note 30, round 3): the cast sits on
/// the open card under the hero, so it stays entirely quiet until a marked
/// character is clicked — no index row, no solo pre-show — and the marks in
/// the hero are the whole affordance.
fn cast_list(uri: &UriView, bare: bool) -> Markup {
    let entries = cast_entries(uri);
    let solo = !bare && entries.len() == 1;
    html! {
        @if !entries.is_empty() {
            div.pv-cast {
                @if !bare && entries.len() > 1 { (cast_index(&entries)) }
                @for e in &entries { (cast_entry(e, solo)) }
            }
        }
    }
}

/// The index a many-character cast opens with: every entry's symbol tile in
/// one row, each carrying the `data-tell` that preview.js already wires — so
/// clicking a tile answers exactly the way clicking the character in the
/// value does.
fn cast_index(entries: &[CastEntry]) -> Markup {
    html! {
        div.pv-index {
            @for e in entries {
                @let (sym, name) = char_dress(e.ch);
                @let title = match e.kind {
                    CastKind::Undecodable => "undecodable bytes".to_string(),
                    CastKind::Padding => format!("{name} U+{:04X}, padding", e.ch as u32),
                    _ => format!("{name} U+{:04X}", e.ch as u32),
                };
                @let warn = matches!(e.kind, CastKind::Hidden | CastKind::Padding);
                @if e.kind == CastKind::Kept {
                    span.tile.lit data-tell=(e.id) title=(title) { (e.ch) }
                } @else if e.kind == CastKind::Undecodable {
                    span.tile.sym data-tell=(e.id) title=(title) { "?" }
                } @else if warn {
                    span.tile.sym.warn data-tell=(e.id) title=(title) { (sym) }
                } @else {
                    span.tile.sym data-tell=(e.id) title=(title) { (sym) }
                }
            }
        }
    }
}

fn cast_entry(e: &CastEntry, solo: bool) -> Markup {
    let warn = matches!(e.kind, CastKind::Hidden | CastKind::Padding);
    let (sym, name) = char_dress(e.ch);
    let name = if e.kind == CastKind::Undecodable {
        "undecodable bytes".to_string()
    } else {
        name
    };
    let mut class = String::from("entry");
    if warn {
        class.push_str(" warn");
    }
    if solo {
        class.push_str(" solo");
    }
    html! {
        div class=(class) data-name=(e.id) {
            span.tiles {
                @if e.kind == CastKind::Kept {
                    span.tile.lit { (e.ch) }
                } @else if e.kind == CastKind::Undecodable {
                    // No character exists to put in a box — a replacement
                    // mark here would claim one does. The dotted square asks
                    // the question instead.
                    span.tile.sym { "?" }
                } @else {
                    span.tile.sym { (sym) }
                    span.tile.raw { (e.ch) }
                }
            }
            span.what {
                b { (name) } " "
                @if e.kind != CastKind::Undecodable {
                    span.cp { "U+" (format!("{:04X}", e.ch as u32)) } " "
                }
                "— " (cast_prose(e)) (cast_tail(e))
                @if e.kind == CastKind::Hidden { ". The empty box is the character." }
            }
        }
    }
}

fn cast_prose(e: &CastEntry) -> Markup {
    html! {
        @match e.kind {
            CastKind::Space => { "stored as " code { "%20" } }
            CastKind::Padding => { "literal, used as padding" }
            CastKind::Hidden => { "stored as " code { (e.stored) } }
            CastKind::Kept => {
                "kept escaped as " code { (e.stored) }
                " so it cannot read as " (structure_reason(e.ch))
            }
            CastKind::Undecodable => {
                "kept escaped as " code { (e.stored) }
                " — the bytes are not valid text"
            }
        }
    }
}

/// The count and place clauses: "…, twice, in the path".
fn cast_tail(e: &CastEntry) -> Markup {
    html! {
        @if e.count == 2 { ", twice" }
        @else if e.count > 2 { ", " (e.count) " times" }
        @if e.places.len() == 1 { ", in " (place_markup(&e.places[0])) }
        @else { ", in " (e.places.len()) " places" }
    }
}

/// The place words wear the role ink of the region they name, matching the
/// hero: path green, fragment teal, the query the key violet (the nearest hue
/// the region has), the username the danger red its own render uses. The
/// address — the non-web catch-all — stays plain.
fn place_markup(p: &CastPlace) -> Markup {
    html! {
        @match p {
            CastPlace::Path => { span.pl-path { "the path" } }
            CastPlace::Username => { span.pl-user { "the username" } }
            CastPlace::Fragment => { span.pl-frag { "the fragment" } }
            CastPlace::Query => { span.pl-query { "the query" } }
            CastPlace::Address => { "the address" }
            // The key wears its own ink here too, matching the hero — read
            // through key_reading, so a hidden character inside the key is
            // red here exactly as it is up there, and the rest stays violet.
            CastPlace::Key(k) => { code.k { (pieces(&urlview::key_reading(k), PieceStyle::Slice)) } }
        }
    }
}

/// What decoding this structure character would have redrawn.
fn structure_reason(c: char) -> &'static str {
    match c {
        '&' => "a new parameter",
        '=' => "a key taking a value",
        '#' => "a fragment starting",
        '%' => "another escape starting",
        _ => "the URL's own structure",
    }
}

// --------------------------------------------------------------------------
// Chips
// --------------------------------------------------------------------------

/// Facts keep the pill; warnings are the card speaking plainly — a bare red
/// icon and words, no background. The contrast carries the meaning.
fn chip_row(uri: &UriView, numbers: &[phone::Number]) -> Markup {
    let pooled = numbers.len() == 1;
    let entries = cast_entries(uri);
    let chips = html! {
        @for hazard in &uri.hazards { (warn_chip(*hazard, &entries)) }
        @if pooled { (number_chips(&numbers[0])) }
    };
    let empty = uri.hazards.is_empty() && !pooled;
    html! { @if !empty { div.pv-facts { (chips) } } }
}

/// A number's own chips: where it is, and what kind of line it is. Premium Rate
/// is the one that warns — and it warns for `sms:` as well as `tel:`, because
/// reverse-billed messaging is a real subscription trap.
fn number_chips(n: &phone::Number) -> Markup {
    html! {
        @if let Some(r) = &n.region {
            // A flag is a region's own mark, and it carries its own shape --
            // boxing it in a pill as well reads as two containers around one
            // fact. The chip stands bare; the type chip beside it keeps the pill.
            span.pv-fact.region { span.flag { (r.flag) } " " (r.name) }
        }
        @if let Some(class) = n.class {
            @if class.is_warning() {
                span.pv-fact.warn { (alert_icon()) (class.label()) }
            } @else {
                span.pv-fact { (class.label()) }
            }
        }
    }
}

fn warn_chip(hazard: Hazard, entries: &[CastEntry]) -> Markup {
    let words = match hazard {
        Hazard::NotEncrypted => "Not Encrypted",
        Hazard::UsernameInTheAddress => "Username in the Address",
        Hazard::HiddenCharacters => "Hidden Characters",
        Hazard::PaddedWithSpaces => "Padded With Spaces",
        Hazard::CarriesAnotherAddress => "Carries Another Address",
    };
    // The chips with a note behind them carry its name; preview.js makes
    // them controls, and without the script the note shows in full anyway.
    let note = match hazard {
        Hazard::UsernameInTheAddress => Some("user"),
        Hazard::CarriesAnotherAddress => Some("carries"),
        _ => None,
    };
    // The chips whose evidence lives in the cast carry their entries' ids;
    // preview.js steps through them on click, the same deck the marks open.
    let cast = match hazard {
        Hazard::HiddenCharacters => {
            let ids: Vec<&str> = entries
                .iter()
                .filter(|e| e.kind == CastKind::Hidden)
                .map(|e| e.id.as_str())
                .collect();
            (!ids.is_empty()).then(|| ids.join(" "))
        }
        Hazard::PaddedWithSpaces => entries
            .iter()
            .any(|e| e.kind == CastKind::Padding)
            .then(|| "pad".to_string()),
        _ => None,
    };
    html! {
        span.pv-fact.warn data-note=[note] data-cast=[cast] {
            @if hazard == Hazard::NotEncrypted { (open_padlock_icon()) } @else { (alert_icon()) }
            (words)
        }
    }
}

fn alert_icon() -> Markup {
    html! {
        svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true" {
            circle cx="8" cy="8" r="6.6" stroke="currentColor" stroke-width="1.4" {}
            path d="M8 4.6v4.2" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" {}
            circle cx="8" cy="11.2" r=".95" fill="currentColor" {}
        }
    }
}

fn open_padlock_icon() -> Markup {
    html! {
        svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true" {
            path d="M4.4 7.2V5.1a3.6 3.6 0 0 1 6.4-2.2" stroke="currentColor"
                stroke-width="1.5" stroke-linecap="round" {}
            rect x="3.1" y="7.2" width="9.8" height="6.4" rx="1.8" stroke="currentColor"
                stroke-width="1.5" {}
        }
    }
}

// --------------------------------------------------------------------------
// Notes
// --------------------------------------------------------------------------

/// A note appears only where the standard opens a gap between what is shown and
/// what is true. Nowhere else: a sentence under every card would train the eye
/// to skip the ones that matter.
fn notes(uri: &UriView) -> Markup {
    let mut out: Vec<Markup> = Vec::new();
    match uri.scheme.as_str() {
        "magnet" => out.push(html! {
            "Only " code { "xt" } " identifies the data. " code { "dn" } " is a name the "
            "link's creator chose, and it does not have to match what arrives."
        }),
        "mailto" => {
            if uri.recipients().count() > 1 || uri.param("cc").is_some() {
                out.push(html! {
                    "Every address listed receives the message, and each can see the others."
                    @if uri.param("bcc").is_some() {
                        " A " code { "bcc" } " address receives it hidden from the others."
                    }
                });
            }
            let subject = uri.param("subject").is_some();
            let body = uri.param("body").is_some();
            if subject || body {
                out.push(html! {
                    "The "
                    @if subject { code { "subject" } }
                    @if subject && body { " and " }
                    @if body { code { "body" } }
                    @if subject && body { " are" } @else { " is" }
                    " pre-written by the link's creator — a message sent from here goes out "
                    "as you."
                });
            }
        }
        "sms" => {
            if uri.recipients().count() > 1 {
                out.push(html! {
                    "One message goes to every number listed — anything sent goes out from "
                    "your number."
                });
            } else if uri.param("body").is_some() {
                out.push(html! {
                    "The " code { "body" } " is the text of the message, written by the "
                    "link's creator — anything sent goes out from your number."
                });
            }
        }
        "tel" => {
            if let Some(ext) = uri.slices.iter().find(|s| s.key.as_deref() == Some("ext")) {
                out.push(html! {
                    "The extension, " (ext.value) ", is dialled after the call connects."
                });
            }
        }
        _ => {}
    }

    html! { @for note in out { span.pv-note { (note) } } }
}

/// The two hazard notes fold behind their chips (the user's call,
/// 2026-08-24): the chip warns, clicking it explains. preview.js wires the
/// chips; the no-JS page shows the notes in full — the split lives in the
/// stylesheet, keyed on `html.js`, the same deck the cast uses.
fn hazard_notes(uri: &UriView) -> Markup {
    let mut out: Vec<(&str, Markup)> = Vec::new();
    if uri.has(Hazard::UsernameInTheAddress) {
        let domain = uri.card_domain();
        out.push((
            "user",
            html! {
                "Text before the " code { "@" } " is a login name, not part of "
                "the address. The destination is " code { (domain) } "."
            },
        ));
    }
    if uri.has(Hazard::CarriesAnotherAddress) {
        out.push((
            "carries",
            html! {
                "One parameter is itself a complete web address on a different domain."
            },
        ));
    }
    html! {
        @if !out.is_empty() {
            div.pv-notes {
                @for (id, note) in &out { span.pv-note data-note-for=(id) { (note) } }
            }
        }
    }
}

// --------------------------------------------------------------------------
// The button
// --------------------------------------------------------------------------

/// The lead verb and the line under it.
///
/// The rule the whole tier rests on: **describe the scheme, never predict the
/// outcome.** We cannot see the reader's machine, so "opens your mail app" is a
/// claim about an unseen device. "An email address" is a published fact about
/// the string, and it is true whatever happens next.
struct Action {
    lead: String,
    what: String,
}

fn action(uri: &UriView, numbers: &[phone::Number]) -> Action {
    let generic = |what: &str| Action {
        lead: String::new(),
        what: what.to_string(),
    };
    match uri.scheme.as_str() {
        "mailto" => {
            let to: Vec<&urlview::Slice> = uri.recipients().collect();
            let cc = uri.param("cc").is_some();
            let count = to.len() + usize::from(cc);
            match count {
                0 => Action {
                    lead: "Draft a Message".into(),
                    what: "An email with no recipient".into(),
                },
                1 => Action {
                    lead: format!(
                        "Write to {}",
                        to.first()
                            .map(|s| s.value.clone())
                            .or_else(|| uri.param("cc").map(|s| s.value.clone()))
                            .unwrap_or_default()
                    ),
                    what: "An email address".into(),
                },
                n => Action {
                    lead: format!("Write to {n} addresses"),
                    what: "Email addresses".into(),
                },
            }
        }
        "tel" => Action {
            lead: format!(
                "Call {}",
                numbers.first().map(|n| n.headline.as_str()).unwrap_or("")
            ),
            what: "A phone number".into(),
        },
        // "Message", not "Text": Text is one of this site's two link kinds, and
        // a button that says it would be naming the wrong thing.
        "sms" => match numbers.len() {
            0 | 1 => Action {
                lead: format!(
                    "Message {}",
                    numbers.first().map(|n| n.headline.as_str()).unwrap_or("")
                ),
                what: "A phone number, for a message".into(),
            },
            n => Action {
                lead: format!("Message {n} numbers"),
                what: "Phone numbers, for one message".into(),
            },
        },
        "magnet" => generic("A file identified by its hash"),
        "ftp" | "ftps" => generic("An address on a file server"),
        "spotify" => generic(match uri.type_segment.as_deref() {
            Some("track") => "A track in Spotify's catalogue",
            Some("album") => "An album in Spotify's catalogue",
            Some("artist") => "An artist in Spotify's catalogue",
            Some("playlist") => "A playlist in Spotify's catalogue",
            Some("show") => "A show in Spotify's catalogue",
            Some("episode") => "An episode in Spotify's catalogue",
            _ => "Something in Spotify's catalogue",
        }),
        "xmpp" => generic(if uri.param("join").is_some() {
            "A chat room on XMPP"
        } else {
            "An XMPP chat address"
        }),
        "matrix" => generic(match uri.type_segment.as_deref() {
            Some("u") => "A user on Matrix",
            Some("r") | Some("roomid") => "A room on Matrix",
            Some("e") => "An event in a Matrix room",
            _ => "A room, user, or event on Matrix",
        }),
        "irc" | "ircs" => generic(
            if uri
                .type_segment
                .as_deref()
                .is_some_and(|t| t.contains(",isnick"))
            {
                "A person on IRC"
            } else {
                "An IRC server or channel"
            },
        ),
        _ => generic("An address"),
    }
}

/// The action, full width. `preview.js` splits a blue Copy segment off the
/// right-hand end; without it this stays one button and the page still works.
fn action_button(uri: &UriView, href: &str, numbers: &[phone::Number]) -> Markup {
    if uri.tier == Tier::Web {
        return html! {
            a class=(GO_BTN) href=(href) rel="noopener noreferrer" { (continue_label(uri)) }
        };
    }
    let a = action(uri, numbers);
    html! {
        a class=(GO_BTN_2) href=(href) rel="noopener noreferrer" {
            span.lead {
                @if a.lead.is_empty() {
                    "Open Link " span.mid { "\u{b7}" } " "
                    span.sch-tag { (uri.scheme) ":" }
                } @else {
                    (a.lead)
                }
            }
            span.what { (a.what) }
        }
    }
}

/// Amber "Continue" (leave the site) and blue "Reveal" (stay, spend the use)
/// button class sets. Continue is an anchor; Reveal stays a POST form
/// (Post/Redirect/Get), so a link-unfurl crawler — which only GETs — can never
/// spend a use.
const GO_BTN: &str = "btn btn--go btn-block pv-btn btn-link go";
const GO_BTN_2: &str = "btn btn--go btn-block btn-2 pv-btn btn-link go";
const REVEAL_BTN: &str = "btn btn-block pv-btn";

fn consume_form(action: &str, btn_class: &str, label: &str) -> Markup {
    html! {
        form.pv-form method="post" action=(action) {
            button class=(btn_class) type="submit" { (label) }
        }
    }
}

fn continue_label(uri: &UriView) -> String {
    // Never print the deceptive domain on the button; say "Continue Anyway".
    if uri.idn_warning().is_some() {
        "Continue Anyway".to_string()
    } else {
        format!("Continue to {}", uri.card_domain())
    }
}

// --------------------------------------------------------------------------
// Values, in three dresses
// --------------------------------------------------------------------------

/// Which palette a value is being drawn in. The three registers share one
/// language and differ only in emphasis.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PieceStyle {
    /// The URL line: delimiters accent, escapes dim, the host washed once.
    Url,
    /// A headline: the local part at full colour, the domain bold and washed.
    Headline,
    /// A slice row: the same, one size down and never washed.
    Slice,
}

fn pieces(parts: &[Piece], style: PieceStyle) -> Markup {
    html! {
        @for p in parts {
            @match p {
                Piece::Text(s) => { (s) }
                // A space that was stored as %20 wears a dotted underline: a
                // receipt saying "this space is an escape in the link".
                //
                // Every mark also names itself: `data-tell` points at the cast
                // entry for its character (preview.js wires the click) and the
                // title is the hover crumb the no-JS page keeps.
                Piece::DecodedSpace => { span.dsp data-tell="u0020" title="space U+0020" { " " } }
                Piece::Padding(s) => { span.bad data-tell="pad" title="space U+0020, padding" { (s) } }
                Piece::Escape(s) => {
                    @let (tell, title) = mark_meta(s);
                    @if style == PieceStyle::Url { span.pe data-tell=(tell) title=(title) { (s) } }
                    @else { span.pct data-tell=(tell) title=(title) { (s) } }
                }
                Piece::BadEscape(s) => {
                    @let (tell, title) = mark_meta(s);
                    span.bad data-tell=(tell) title=(title) { (s) }
                }
                Piece::Delim(s) => { span.dl { (s) } }
                Piece::Local(s) => {
                    @if style == PieceStyle::Url { (s) } @else { span.lp { (s) } }
                }
                Piece::Domain(s) => { span.reg { (s) } }
            }
        }
    }
}

/// The full destination URL, coloured by part. Built from the same slices the
/// model lists, so userinfo and an explicit port can no longer go missing — the
/// old renderer had no branch for either, which quietly dropped `alice@` and
/// `:8443` from the line and from its copy.
fn url_line_parts(uri: &UriView) -> Markup {
    html! {
        span class=(if uri.scheme == "http" { "sch insecure" } else { "sch" }) { (uri.scheme) }
        span.pn { (uri.prefix.trim_start_matches(&uri.scheme)) }
        @for (n, slice) in uri.slices.iter().enumerate() {
            @match slice.role {
                // The username goes danger red, co-firing with its chip. Not
                // amber: amber is the button language for "you leave", and
                // this is not that.
                Role::Userinfo => (hero_part(n, slice, html! { span.usr { (slice.value) } })),
                Role::Host => (host_markup(uri)),
                Role::Port => { span.pn { ":" } span.port { (slice.value) } },
                // The path reads at full strength: it is the part of the line
                // that says what opens.
                Role::Path => (path_markup(&slice.display)),
                Role::Opaque => span.seg { (pieces(&slice.display, PieceStyle::Url)) },
                // A keyless fragment is one literal segment; a keyed part is a
                // key, an `=`, and a value.
                // A `<wbr>` after each delimiter: when the line wraps, it
                // prefers to break where the URL has a joint, so a line ends
                // on a character that visibly cannot end a URL (design note
                // 29). No text, so a selection or a copy never picks it up.
                _ if slice.key.is_none() => (hero_part(n, slice, html! {
                    span.pn { (slice.delim) } wbr;
                    span class=(hero_value_class(slice, "seg fg")) {
                        (pieces(&slice.display, PieceStyle::Url))
                    }
                })),
                _ => (hero_part(n, slice, html! {
                    span.pn { (slice.delim) } wbr;
                    // A tail's key is a signpost, so it takes the bold the
                    // raw lines have always given it. `.seg` is left to the
                    // port, which is not a key and must not read as one.
                    // Read through key_reading: verbatim, except an
                    // invisible goes red with its mark.
                    @if let Some(k) = &slice.key {
                        span.qk { (pieces(&urlview::key_reading(k), PieceStyle::Url)) }
                    }
                    @if slice.equals { span.pn { "=" } wbr; }
                    span class=(hero_value_class(slice, "qv")) {
                        (pieces(&slice.display, PieceStyle::Url))
                    }
                })),
            }
        }
    }
}

/// A removable part of the hero, wrapped so preview.js can make it a control:
/// click to strike, click again to restore (design note 30 round 3 — editing
/// lives in the hero, the checkbox table is the other schemes' dress). Fixed
/// parts render unwrapped; without the script the wrapper is inert text.
fn hero_part(n: usize, slice: &urlview::Slice, inner: Markup) -> Markup {
    html! {
        @if slice.removable {
            span.hp data-slice=(n) { (inner) }
        } @else {
            (inner)
        }
    }
}

/// The value's classes: `base`, plus the capsule when the reading earned one —
/// the visible boundary that lets a value's inner `&`, `=`, `?`, `#`, or a
/// whole carried address read as the value's own rather than the URL's.
fn hero_value_class(slice: &urlview::Slice, base: &str) -> String {
    if urlview::needs_capsule(&slice.display) {
        format!("{base} cv")
    } else {
        base.to_string()
    }
}

/// The parts model rides the hero only when something is removable — a bare
/// path has nothing to edit and gets no attribute at all.
fn web_model(uri: &UriView) -> Option<String> {
    uri.slices
        .iter()
        .any(|s| s.removable)
        .then(|| card_model(uri))
}

/// The host, with the accent wash on the registrable domain — the once-per-page
/// mark saying who you are actually dealing with.
fn host_markup(uri: &UriView) -> Markup {
    html! {
        @if let Some(h) = &uri.host {
            @if !h.subdomain.is_empty() { span.sub { (h.subdomain) "." } }
            span.reg { (h.registrable) }
        }
    }
}

/// A path inside a one-run headline: the slashes dim, each segment a token, and
/// a `<wbr>` at every joint so a narrow card steps down whole tokens.
fn path_run(path: &str) -> Markup {
    html! {
        @for (n, part) in path.split('/').enumerate() {
            @if n > 0 { span.dl { "/" } wbr; }
            @if !part.is_empty() { span.ps { (part) } }
        }
    }
}

/// A path's display pieces, split at its `/` separators with every piece's
/// dress intact — flattening to text here used to strip a dotted space or a
/// red escape in the path of its marking (and its colour in the hero).
fn split_path_segments(display: &[Piece]) -> Vec<Vec<Piece>> {
    let mut segments: Vec<Vec<Piece>> = vec![Vec::new()];
    for piece in display {
        match piece {
            Piece::Text(s) if s.contains('/') => {
                for (n, part) in s.split('/').enumerate() {
                    if n > 0 {
                        segments.push(Vec::new());
                    }
                    if !part.is_empty() {
                        segments
                            .last_mut()
                            .expect("segments starts non-empty")
                            .push(Piece::Text(part.to_string()));
                    }
                }
            }
            p => segments
                .last_mut()
                .expect("segments starts non-empty")
                .push(p.clone()),
        }
    }
    segments
}

/// A path inside a slice row: same idea, the row's own delimiter colour.
fn path_slices_markup(display: &[Piece]) -> Markup {
    let segments = split_path_segments(display);
    html! {
        @for (n, seg) in segments.iter().enumerate() {
            @if n > 0 { span.dl { "/" } }
            (pieces(seg, PieceStyle::Slice))
        }
    }
}

fn path_markup(display: &[Piece]) -> Markup {
    let segments = split_path_segments(display);
    html! {
        @for (n, seg) in segments.iter().enumerate() {
            @if n > 0 { span.pn { "/" } wbr; }
            @if !seg.is_empty() { span.ps { (pieces(seg, PieceStyle::Url)) } }
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
// What preview.js is handed
// --------------------------------------------------------------------------

/// The parts model, as JSON on the slices container.
///
/// A data attribute rather than a `<script>` block: it is data, and putting it
/// in a script tag would raise a CSP question for something that is not code.
/// `preview.js` reads it to inject the checkboxes, rebuild the string as they
/// are unticked, and keep the button honest about what it would open.
///
/// Every slice ships as `(class, text)` runs rather than as markup. The site's
/// CSP carries `require-trusted-types-for 'script'` with no policy allowed, so
/// an `innerHTML` assignment would throw outright — the script has to build
/// elements and set `textContent`. That is the stricter arrangement anyway: no
/// HTML crosses this boundary at all, in either direction.
fn card_model(uri: &UriView) -> String {
    let parts: Vec<serde_json::Value> = uri
        .slices
        .iter()
        .enumerate()
        .map(|(n, s)| {
            serde_json::json!({
                "i": n,
                "role": role_key(s.role),
                "d": s.delim,
                "k": s.key,
                "e": s.equals,
                "v": s.value,
                "fixed": !s.removable,
                "row": s.is_row(),
                "label": recipient_label(uri, s),
                "p": stored_runs(s),
            })
        })
        .collect();
    serde_json::json!({
        "scheme": uri.scheme,
        "prefix": uri.prefix,
        "prefixRuns": [["sch", uri.prefix]],
        // What "After your edits" is compared against before it shows itself.
        "stored": uri.raw(),
        // RFC 5724 needs at least one recipient, so the last number standing
        // locks. RFC 6068 needs none, so a mailto never locks -- empty the list
        // and the button offers a draft.
        "floor": u8::from(uri.scheme == "sms"),
        "parts": parts,
    })
    .to_string()
}

fn role_key(role: Role) -> &'static str {
    match role {
        Role::Userinfo => "user",
        Role::Host => "host",
        Role::Port => "port",
        Role::Path => "path",
        Role::PathParam => "pathparam",
        Role::Query => "query",
        Role::Fragment => "fragment",
        Role::Recipient => "recipient",
        Role::Opaque => "opaque",
    }
}

/// How the button names this recipient when it is the only one left: an address
/// as itself, a number in its own country's formatting.
fn recipient_label(uri: &UriView, slice: &urlview::Slice) -> Option<String> {
    (slice.role == Role::Recipient).then(|| {
        if uri.scheme == "sms" {
            phone::read(&slice.value).headline
        } else {
            slice.value.clone()
        }
    })
}

/// One slice in the exact line's dress, as `(class, text)` runs: a bold key, an
/// accent `=`, then the value with its percent escapes dimmed to the encoding
/// noise they are. An empty class means a bare text node.
fn stored_runs(slice: &urlview::Slice) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    if let Some(k) = &slice.key {
        out.push(("k", k.clone()));
    }
    if slice.equals {
        out.push(("dl", "=".to_string()));
    }
    let plain = match slice.role {
        Role::Userinfo => "usr",
        Role::Port => "port",
        _ => "",
    };
    for (text, is_escape) in escape_runs(&slice.value) {
        out.push((if is_escape { "pct" } else { plain }, text));
    }
    out
}

// --------------------------------------------------------------------------
// Revealed view (token-gated, after a use was spent)
// --------------------------------------------------------------------------

pub enum RevealedTarget<'a> {
    /// A redirect: show the full URL and a plain Continue link (going is free now,
    /// the use was spent at reveal). `href` is the canonical destination.
    Redirect { uri: &'a UriView, href: &'a str },
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
    let back = home_chip("/", "Create New Link");
    match r.target {
        RevealedTarget::Redirect { uri, href } => {
            // The same render-time allowlist check the interstitial makes. This
            // page used to emit `href=(content)` with no check at all, so an
            // off-allowlist scheme that somehow reached storage would have been
            // handed to the browser as a live link here even while the
            // interstitial refused it. Both gaps close in one place.
            let linkable = uri.tier != Tier::Refused && is_linkable(href);
            let body = html! {
                (link_heading(Kind::Redirect, r.name, r.base_host, back))
                (pv_arrow())
                // A one-time link is spent to LOOK, never to be thrown: what
                // arrives here is the whole card, with the button waiting.
                @if linkable { (card_body(uri, href)) } @else { (refusal_block(uri)) }
                p.pv-revealed { "Deleted from the server on this view. Refreshing won't bring it back." }
                p.pv-meta { "Expires in " (humanize_expires_in(r.expires_at)) }
                @if linkable {
                    span.pv-caution.single { strong { "Always check the destination." } }
                }
            };
            document_link(
                &link_title("Redirect", r.name, Some(&uri.card_domain())),
                html! {},
                body,
                script_tag("/static/preview.js"),
            )
        }
        RevealedTarget::Text(text) => {
            let body = html! {
                (link_heading(Kind::Text, r.name, r.base_host, back))
                p.pv-revealed { "Deleted from the server on this view. Refreshing won't bring it back." }
                (text_body(text))
            };
            document_link(
                &link_title("Text", r.name, None),
                html! {},
                body,
                script_tag("/static/text.js"),
            )
        }
    }
}

/// A plaintext Text link, rendered immediately (unlimited text). The body is an
/// escaped `<pre>` — maud escapes it, so a `<script>` in the content shows as text
/// and never executes. We never emit it as live HTML.
pub fn text_view_page(base_host: &str, name: &str, text: &str) -> Markup {
    let body = html! {
        (link_heading(Kind::Text, name, base_host, home_chip("/", "Go to YuioLink")))
        (text_body(text))
    };
    document_link(
        &link_title("Text", name, None),
        html! {},
        body,
        script_tag("/static/text.js"),
    )
}

/// The snippet itself, plus the Copy button that is dead without JavaScript.
///
/// No height cap here: the collapse and its "Show all" control are applied by
/// `text.js`, so a visitor without JavaScript gets the whole snippet in flow and
/// scrolls the page, rather than a box they cannot open.
fn text_body(text: &str) -> Markup {
    html! {
        div.text-wrap #text-wrap {
            pre.text-body #text-body { (text) }
        }
        button.btn.btn-block #copy-text type="button" hidden { "Copy" }
    }
}

// --------------------------------------------------------------------------
// Tombstones + errors
// --------------------------------------------------------------------------

/// 410 Gone: the link was real but is now spent or withdrawn. Its name stays
/// reserved until expiry, so it cannot be silently repurposed in the meantime.
pub fn gone_page(expires_at: Option<&str>) -> Markup {
    let body = html! {
        (home_chip("/", "Go to YuioLink"))
        p.error-code { "410" }
        p { "This link has been used or withdrawn." }
        @if let Some(exp) = expires_at {
            p.meta { "Its name stays reserved for " (humanize_expires_in(exp)) "." }
        }
        a.btn.btn-block href="/" { "Create a New Link" }
    };
    document_short("YuioLink — Link Gone", body, html! {})
}

/// 404 Not Found: nothing here — expired, recycled, or never existed. Framed as
/// by-design, since every YuioLink is ephemeral.
pub fn not_found_page() -> Markup {
    let body = html! {
        (home_chip("/", "Go to YuioLink"))
        p.error-code { "404" }
        p { "This link has expired or never existed — links on YuioLink are ephemeral." }
        a.btn.btn-block href="/" { "Create a New Link" }
    };
    document_short("YuioLink — Link Not Found", body, html! {})
}

/// `GET /help` — the usage page: why links expire, what the three types and two
/// kinds are for, worked examples, and the terminal endpoint.
///
/// Type and kind names are capitalized only where they name an on-screen control
/// (the picker segments, the kind chip). In prose they are ordinary adjectives —
/// "a secret link", "a text link" — because the names were chosen to describe
/// themselves, and title case would claim a precision the words do not need.
///
/// `base_url` ends in `/`, so `{base_url}create` is the terminal endpoint and
/// `host_from_base` gives the bare host for the example name.
pub fn help_page(base_url: &str) -> Markup {
    let host = host_from_base(base_url);
    let body = html! {
        (home_chip("/", "Back to YuioLink"))
        h2.help-title { "How to use YuioLink" }
        p.help-lead {
            "Paste a link or some text, choose how long it should last, and you get a "
            "short name to pass on — " code { (host) "/braveOTTER" } ". Whoever opens it "
            "sees where it leads before they go. When the time runs out the link is gone, "
            "and what it held goes with it."
        }

        h3.help-h { "Everything expires" }
        p.help-p {
            "There are no permanent links here. Seven days is the longest life a link can "
            "have, and it is also the starting position of the slider — drag it down to the "
            "shortest span that still does the job. Expiry is deletion, not hiding: the row "
            "is erased, and the name goes back into the pool for someone else."
        }

        h3.help-h #types { "The three types" }
        ul.help-types {
            li {
                span.help-type.dot.t-public { "Public" }
                " — one word, or two or three while short names are in demand. Short enough "
                "to read out or type from the back of a room. The "
                a href="/wordlist.txt" { "wordlist" }
                " is public, so anyone can walk the whole list and turn up every public link. "
                "For convenience, never for secrets."
            }
            li {
                span.help-type.dot.t-secret { "Secret" }
                " — four random words out of about 143 trillion combinations. Nothing lists "
                "or indexes it, so reaching the link means guessing its exact name before it "
                "expires. The name is the secret, and it exists only until the link does. The "
                "server can still read the destination — this hides the link, not its contents."
            }
            li {
                span.help-type.dot.t-once { "One-Time" }
                " — four words as well, and the content is erased the moment it is revealed. "
                "The recipient sees a preview first and chooses to reveal, so a chat unfurler "
                "or a prefetch cannot burn it on the way. If they find it already spent, "
                "someone else opened it first — which is worth knowing."
            }
        }

        h3.help-h { "The two kinds" }
        p.help-p {
            "Paste something that looks like an address and you get a redirect; paste anything "
            "else and you get text, shown escaped and never rendered as HTML. If you wanted the "
            "address itself shown rather than followed, the page you land on after creating "
            "offers to share it as a text link instead."
        }

        h3.help-h { "What it is for" }
        dl.help-cases {
            dt { "A long address onto your phone" }
            dd { "Public, ten minutes. Type one word instead of mailing it to yourself." }
            dt { "The guest Wi-Fi password" }
            dd {
                "Secret text, until the end of the day. You hand out the name; nobody walking "
                "the wordlist finds it."
            }
            dt { "A credential, handed over once" }
            dd {
                "One-time text, an hour or two. If it reads as already spent when they open "
                "it, rotate the credential."
            }
            dt { "A link on a slide" }
            dd {
                "Public, a few hours. Readable from the back of the room, and gone before the "
                "recording goes up."
            }
            dt { "Notes, a log, an error message" }
            dd { "Text, for as long as the conversation needs. Paste it, or pipe a file in." }
        }

        h3.help-h { "From a terminal" }
        pre.help-code {
            span.c { "# a redirect, lasting the default seven days" } "\n"
            "curl -d url=https://example.com/a/very/long/path " (base_url) "create\n\n"
            span.c { "# ten minutes instead" } "\n"
            "curl -d url=https://example.com -d ttl=10m " (base_url) "create\n\n"
            span.c { "# a file, as a one-time text link" } "\n"
            "curl --data-binary @notes.txt -d uses=1 " (base_url) "create\n\n"
            span.c { "# a URL with its own & or = in the query" } "\n"
            "curl --data-urlencode 'url=https://example.com/?a=1&b=2' " (base_url) "create\n"
        }
        p.help-p.help-note {
            code { "ttl" } " and " code { "uses" } " have to come last: everything before them "
            "is the content, so a plain URL keeps its own query string. If that query has an "
            "ampersand of its own, reach for " code { "--data-urlencode" }
            " — the value is decoded once on arrival, so the link is stored exactly as you "
            "typed it. A piped file is taken as raw bytes and never decoded. The reply is the "
            "short URL, or JSON with " code { "Accept: application/json" } ". This endpoint "
            "makes public and one-time links — a secret one needs the "
            a href="/api/v0/openapi.yaml" { "API" }
            "."
        }

        h3.help-h { "What the server knows" }
        p.help-p {
            "Destinations and text are stored as you gave them, so whoever runs the server can "
            "read them. That is true of every link shortener, and it is why the secret type is "
            "about the name rather than the payload. The counters record nothing about you — no "
            "addresses, no agents, no referrers, no per-event row to correlate — and all of them "
            "are on the " a href="/stats" { "statistics page" } ". Local history stays in your "
            "browser and is never sent."
        }

        footer { a href="/" { "Back to YuioLink" } }
    };
    document_full(
        "YuioLink — Help",
        html! {
            meta name="description" content="How YuioLink works: why every link expires, what the public, secret, and one-time types are for, and how to create a link from a terminal.";
        },
        body,
        html! {},
    )
}

/// `GET /colophon` — what YuioLink is made of: the licence it is offered under,
/// and the third-party work it carries that the licence does not cover.
///
/// A page rather than a section of `/help` because the two answer different
/// questions, and because CC-BY-3.0 §4(c) asks that the credit appear "in a
/// manner at least as prominent" as other authorship credit — the footer names
/// jooize, so the EFF credit has to be reachable at that level, not buried.
///
/// The old printer's sense of the word: the note at the end of a book saying who
/// set it, and in what type. That is exactly this page, fonts included.
pub fn colophon_page() -> Markup {
    let body = html! {
        (home_chip("/", "Back to YuioLink"))
        h2.help-title { "Colophon" }
        p.help-lead {
            "What this site is made of — the terms it is offered under, and the work it "
            "carries that those terms do not cover."
        }

        h3.help-h #license { "Licence" }
        p.help-p {
            "YuioLink is dual-licensed "
            a.ext href="https://github.com/jooize/YuioLink/blob/main/LICENSE-MIT" {
                "MIT" (external_mark())
            }
            " or "
            a.ext href="https://github.com/jooize/YuioLink/blob/main/LICENSE-APACHE" {
                "Apache-2.0" (external_mark())
            }
            ", at your option — take whichever suits you. The source is on "
            a.ext href="https://github.com/jooize/YuioLink" { "GitHub" (external_mark()) }
            "."
        }

        h3.help-h #words { "The words" }
        p.help-p {
            "Link names are drawn from a list derived from the "
            a.ext href="https://www.eff.org/deeplinks/2016/07/new-wordlists-random-passphrases" {
                "EFF passphrase wordlists" (external_mark())
            }
            " — © Electronic Frontier Foundation, licensed "
            a.ext href="https://creativecommons.org/licenses/by/3.0/us/" {
                "CC-BY-3.0-US" (external_mark())
            }
            " — together with the BIP-0039 English wordlist. What ships here is a "
            "length-capped, hand-curated subset of those, not a copy of any of them: "
            "sound-alike pairs were dropped so a name survives being read aloud. The "
            "list is at "
            a href="/wordlist.txt" { "/wordlist.txt" }
            "."
        }

        h3.help-h #numbers { "The numbers" }
        p.help-p {
            "A preview of a phone link says which country a number belongs to, and whether "
            "it is a mobile, a toll-free, or a premium-rate line — facts read off Google's "
            a.ext href="https://github.com/google/libphonenumber" {
                "libphonenumber" (external_mark())
            }
            " numbering plans through the "
            a.ext href="https://github.com/whisperfish/rust-phonenumber" {
                "phonenumber" (external_mark())
            }
            " crate, both Apache-2.0. The tables are compiled into the binary, so reading a "
            "number asks nothing of anyone: no lookup leaves this server. The way each "
            "country groups its digits comes from the same tables, which is why a Swedish "
            "number arrives wearing its hyphen and a French one its pairs."
        }

        h3.help-h #type { "The type" }
        p.help-p {
            "The share-card images are drawn server-side with the "
            a.ext href="https://dejavu-fonts.github.io/" { "DejaVu fonts" (external_mark()) }
            " — descended from Bitstream Vera (© 2003 Bitstream, Inc.) and Arev "
            "(© 2006 Tavmjong Bah), with the DejaVu changes in the public domain. They are "
            "embedded in the binary, so a card renders the same with no fonts installed on "
            "the server at all. The pages themselves ask for whatever your system calls its "
            "interface font, and are not sent any."
        }

        footer { a href="/" { "Back to YuioLink" } }
    };
    document_full(
        "YuioLink — Colophon",
        html! {
            meta name="description" content="What YuioLink is made of: the MIT/Apache-2.0 licence it is offered under, the EFF and BIP-0039 wordlists its link names come from, and the DejaVu fonts its share cards are drawn with.";
        },
        body,
        html! {},
    )
}

/// Generic terse error page (used for 400 on the no-JS form and 500).
/// What `/stats` reports. Every field is an aggregate: a live count, or a tally of
/// events per UTC day. Nothing in here is per-link or per-visitor.
pub struct StatsView<'a> {
    /// Links resolvable right now.
    pub live: i64,
    /// All-time totals, keyed by `db::Stat::key()`.
    pub totals: &'a [(String, i64)],
    /// Per-day rows, oldest first: `(day, created, opened)`.
    pub days: &'a [(String, i64, i64)],
}

/// `GET /stats` — the public, aggregate-only counters.
///
/// The page states what is *not* counted as prominently as what is. The site
/// promises no tracking, and a statistics page is exactly where a visitor is
/// entitled to be suspicious, so the disclosure belongs next to the numbers
/// rather than buried in a privacy page nobody opens.
pub fn stats_page(s: &StatsView) -> Markup {
    let total = |key: &str| {
        s.totals
            .iter()
            .find(|(m, _)| m == key)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    };
    let created = total("created_public") + total("created_secret") + total("created_once");

    let body = html! {
        (home_chip("/", "Back to YuioLink"))
        h2.stats-h { "Statistics" }
        p { "Aggregate counts, nothing else. No visitor is measured." }

        div.stats-grid {
            div.stat-cell {
                span.stat-num { (s.live) }
                span.stat-label { "live right now" }
            }
            div.stat-cell {
                span.stat-num { (created) }
                span.stat-label { "links created" }
            }
            div.stat-cell {
                span.stat-num { (total("opened")) }
                span.stat-label { "links opened" }
            }
            div.stat-cell {
                span.stat-num { (total("expired")) }
                span.stat-label { "expired and erased" }
            }
        }

        h3.stats-h { "By type" }
        ul.stats-list {
            li { span.dot.t-public { "Public" } span.stats-n { (total("created_public")) } }
            li { span.dot.t-secret { "Secret" } span.stats-n { (total("created_secret")) } }
            li { span.dot.t-once { "One-Time" } span.stats-n { (total("created_once")) } }
        }

        h3.stats-h { "By kind" }
        ul.stats-list {
            li { span { "Redirect" } span.stats-n { (total("created_redirect")) } }
            li { span { "Text" } span.stats-n { (total("created_text")) } }
        }

        // The preview page's own tallies. They live here, and only here: the
        // per-link counter that used to sit on the row is gone, so this is the
        // whole of what is known about how often a preview is seen.
        h3.stats-h { "Previews" }
        ul.stats-list {
            li { span { "Previews shown" } span.stats-n { (total("previewed")) } }
            li { span { "Destinations revealed" } span.stats-n { (total("revealed")) } }
        }

        h3.stats-h { "Last 7 days" }
        @if s.days.is_empty() {
            p { "Nothing counted yet." }
        } @else {
            table.stats-table {
                thead {
                    tr { th { "Day (UTC)" } th { "Created" } th { "Opened" } }
                }
                tbody {
                    @for (day, created, opened) in s.days {
                        tr {
                            td { (day) }
                            td { (created) }
                            td { (opened) }
                        }
                    }
                }
            }
        }

        div.stats-note {
            p {
                strong { "What is not recorded." }
                " No IP addresses, no user agents, no referrers, no cookies, no sessions. "
                "Link names and destinations never reach these counters, and there is no "
                "per-event row to correlate: a counter only says that something happened "
                "a number of times on a day."
            }
            p {
                "Days are UTC, and a day is the finest resolution kept — anything finer "
                "would start placing a person at a moment, which is the line this page "
                "exists not to cross."
            }
        }

        footer { a href="/" { "Back to YuioLink" } }
    };
    document_full(
        "YuioLink — Statistics",
        html! {
            meta name="description" content="Aggregate, anonymous counters for YuioLink — links created, opened, and expired. No visitors are measured.";
        },
        body,
        html! {},
    )
}

pub fn error_page(code: u16, message: &str) -> Markup {
    error_page_list(code, &[message])
}

/// As [`error_page`], but with one line per message — the no-JS form reports
/// every validation problem at once, not just the first.
pub fn error_page_list(code: u16, messages: &[&str]) -> Markup {
    let body = html! {
        (home_chip("/", "Go to YuioLink"))
        p.error-code { (code) }
        @for message in messages {
            p { (message) }
        }
        footer { a href="/" { "Back to YuioLink" } }
    };
    document_full(
        &format!("YuioLink — Error {code}"),
        html! {},
        body,
        html! {},
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `expires_at` string `secs` from now, in SQLite's stored form.
    fn in_secs(secs: i64) -> String {
        let t = now_unix() + secs;
        // Inverse of parse_sqlite_utc, via days-from-civil run backwards.
        let (days, rem) = (t.div_euclid(86400), t.rem_euclid(86400));
        let z = days + 719468;
        let era = z.div_euclid(146097);
        let doe = z.rem_euclid(146097);
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!(
            "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
            rem / 3600,
            (rem % 3600) / 60,
            rem % 60
        )
    }

    #[test]
    fn days_round_to_the_nearest_while_hours_and_minutes_still_floor() {
        // The case that prompted this: a seven-day link must not read "6 days"
        // from the moment it is created. It says 7 for the first half-day.
        assert_eq!(humanize_expires_in(&in_secs(7 * 86400 - 60)), "7 days");
        assert_eq!(
            humanize_expires_in(&in_secs(6 * 86400 + 13 * 3600)),
            "7 days"
        );
        assert_eq!(
            humanize_expires_in(&in_secs(6 * 86400 + 11 * 3600)),
            "6 days"
        );
        // Unchanged below a day: flooring, so the figure never overstates what
        // is left as expiry approaches.
        assert_eq!(
            humanize_expires_in(&in_secs(23 * 3600 + 40 * 60)),
            "23 hours"
        );
        assert_eq!(humanize_expires_in(&in_secs(59 * 60 + 45)), "59 min");
        assert_eq!(humanize_expires_in(&in_secs(2 * 3600)), "2 hours");
        assert_eq!(humanize_expires_in(&in_secs(3600)), "1 hour");
        assert_eq!(humanize_expires_in(&in_secs(600)), "10 min");
        assert_eq!(humanize_expires_in(&in_secs(30)), "less than a minute");
        assert_eq!(humanize_expires_in(&in_secs(-500)), "less than a minute");
    }

    #[test]
    fn link_titles_name_the_link_and_gate_the_destination() {
        assert_eq!(
            link_title("Redirect", "line", Some("example.com")),
            "YuioLink Redirect: line → example.com"
        );
        // Four words means secret or one-time: the destination stays off the tab.
        assert_eq!(
            link_title("Redirect", "actSPILTvistaCOUNTY", Some("example.com")),
            "YuioLink Redirect: actSPILTvistaCOUNTY"
        );
        assert_eq!(link_title("Text", "line", None), "YuioLink Text: line");
    }

    /// The card for a stored string, with no page around it.
    fn card(stored: &str) -> String {
        let uri = urlview::parse_uri(stored);
        card_body(&uri, stored).into_string()
    }

    #[test]
    fn the_web_tier_is_one_voice_with_the_hero_as_editor() {
        let c = card("https://blog.example.com/articles/2026/post?ref=newsletter");
        // The hero carries the parts model (design note 30, round 3): it is
        // the editor now, and the removable part is wrapped for the script.
        assert!(
            c.contains(r#"<code class="pv-url" id="destination" data-card=""#),
            "{c}"
        );
        assert!(c.contains(r#"<span class="reg">example.com</span>"#));
        assert!(c.contains(r#"<span class="hp" data-slice=""#), "{c}");
        // The fold is gone entirely.
        assert!(!c.contains("pv-parts"), "{c}");
        assert!(!c.contains("Show URL Details"), "{c}");
        assert!(c.contains("Continue to example.com"));
        // The URL line already is the stored string, so nothing is restated
        // and no Stored Form is offered.
        assert!(!c.contains("Exactly as Stored"), "{c}");
        assert!(!c.contains("pv-stored"), "{c}");
    }

    #[test]
    fn a_bare_path_never_folds_and_http_says_so_without_opening_it() {
        let c = card("http://example.com/pay");
        assert!(c.contains(r#"<span class="sch insecure">http</span>"#));
        assert!(c.contains("Not Encrypted"));
        // The fold would only restate one row nobody can act on.
        assert!(!c.contains("pv-parts"), "{c}");
    }

    #[test]
    fn a_warning_about_the_string_keeps_the_chips_and_the_stored_form() {
        let c = card("https://alice@example.com:8443/reset?next=https%3A%2F%2Fother.example%2F");
        assert!(c.contains("Username in the Address"));
        assert!(c.contains("Carries Another Address"));
        // Userinfo and port are on the line -- an old renderer had no branch
        // for either and dropped both.
        assert!(c.contains(r#"<span class="usr">alice@</span>"#));
        assert!(c.contains(r#"<span class="port">8443</span>"#));
        // Bare means real: the carried address reads decoded inside its
        // capsule, domain bold, its own delimiters dim.
        assert!(c.contains(r#"<span class="qv cv">"#), "{c}");
        assert!(
            c.contains(r#"<span class="dl">://</span><span class="reg">other.example</span>"#),
            "{c}"
        );
        // Decoding changed the value, so the record waits behind the Stored
        // Form disclosure, with the drift line beside it.
        let record = c.find("Exactly as Stored").expect("record missing");
        assert!(record > c.find(r#"<details class="pv-stored">"#).unwrap(), "{c}");
        assert!(record < c.find("</details>").unwrap(), "{c}");
        assert!(c.contains("Selecting text copies the readable form"), "{c}");
    }

    #[test]
    fn the_record_waits_behind_the_stored_form_disclosure() {
        let c = card("https://example.com/files/r%C3%A9sum%C3%A9.pdf?q=hello%20world");
        // Harmless decodes: the record lives behind the small Stored Form
        // link rather than doubling the page.
        assert!(c.contains(r#"<details class="pv-stored">"#), "{c}");
        assert!(c.contains(r#"<summary class="pv-stored-lid">Stored Form</summary>"#), "{c}");
        let record = c.find("Exactly as Stored").expect("record missing");
        assert!(record > c.find("<details").unwrap(), "{c}");
        assert!(record < c.find("</details>").unwrap(), "{c}");
    }

    #[test]
    fn a_decoded_host_grows_the_record_and_a_bare_one_still_gets_it() {
        // The hero reads the stored punycode as Unicode, so the record appears.
        let c = card("https://xn--mnchen-3ya.de/kontakt");
        assert!(c.contains("münchen.de"), "{c}");
        assert!(c.contains("Exactly as Stored"), "{c}");
        // No rows at all means no fold to live in: the record falls back to
        // the open card rather than going missing.
        let bare = card("https://EXAMPLE.com");
        assert!(!bare.contains("pv-parts"), "{bare}");
        assert!(bare.contains("Exactly as Stored"), "{bare}");
    }

    #[test]
    fn a_marked_character_in_the_path_keeps_its_dress() {
        // A %20 in the path used to be flattened to a bare space in the hero
        // and the slice row; the dotted receipt must survive the `/` split.
        let c = card("https://example.com/my%20file/x");
        assert!(c.contains(r#"<span class="dsp" data-tell="u0020""#), "{c}");
    }

    /// The cast (design note 29): every marked character gets one entry in the
    /// fold. The markup is one page for everyone — the stylesheet keyed on
    /// `html.js` hides the entries behind a click when the script is present
    /// and shows the full list when it is not.
    #[test]
    fn the_cast_names_every_marked_character() {
        let c = card("https://example.com/my%20file?user=admin%E2%80%8B&q=a%26b");
        assert!(c.contains(r#"<div class="pv-cast">"#), "{c}");
        // The hidden character's entry warns.
        assert!(
            c.contains(r#"<div class="entry warn" data-name="u200b""#),
            "{c}"
        );
        // Bare means real: the & simply decodes inside its capsule, so it
        // needs no entry at all any more — only what stayed closed is cast.
        assert!(!c.contains(r#"data-name="u0026""#), "{c}");
        assert!(c.contains(r#"<span class="qv cv">a<span class="dl">&amp;</span>b</span>"#), "{c}");
        // The raw tile holds the ACTUAL character — empty on purpose — and
        // the prose says so, so the blank never reads as a rendering bug.
        assert!(
            c.contains("<span class=\"tile raw\">\u{200b}</span>"),
            "{c}"
        );
        assert!(c.contains("The empty box is the character."), "{c}");
        // Names, codepoints, provenance, place.
        assert!(
            c.contains(r#"stored as <code>%20</code>, in <span class="pl-path">the path</span>"#),
            "{c}"
        );
        assert!(c.contains("zero-width space"), "{c}");
        assert!(c.contains("U+200B"), "{c}");
        // Every mark points at its entry.
        assert!(c.contains(r#"data-tell="u200b""#), "{c}");
        // And the stylesheet carries both halves of the reveal: the no-JS
        // page shows the full list instead. Entries hide by visibility in
        // one stacked grid cell, so the cast stands as tall as its tallest
        // answer and swapping never shifts the card.
        const APP_CSS: &str = include_str!("../static/app.css");
        assert!(APP_CSS
            .contains("html.js .pv-cast .entry {\n    grid-row: 2;\n    grid-column: 1;\n    visibility: hidden;\n}"));
        assert!(APP_CSS.contains("html:not(.js) .pv-cast .entry + .entry {"));
        assert!(APP_CSS.contains("html:not(.js) .pv-index {\n    display: none;\n}"));
    }

    /// The web cast sits on the open card under the hero (design note 30,
    /// round 3), so it stays entirely quiet until a marked character is
    /// clicked: no solo pre-show, no index row. A non-web card keeps both —
    /// its cast still lives in a section of its own.
    #[test]
    fn the_web_cast_stays_quiet_until_asked() {
        let c = card("https://example.com/my%20file?q=1");
        assert!(c.contains(r#"<div class="entry" data-name="u0020""#), "{c}");
        assert!(!c.contains("entry solo"), "{c}");
        assert!(!c.contains("pv-index"), "{c}");
        let m = card("mailto:a@b.example?subject=Order%204192");
        assert!(m.contains(r#"<div class="entry solo" data-name="u0020""#), "{m}");
        const APP_CSS: &str = include_str!("../static/app.css");
        assert!(APP_CSS
            .contains("html.js .pv-cast .entry.shown,\nhtml.js .pv-cast .entry.solo {\n    visibility: visible;\n}"));
    }

    #[test]
    fn the_cast_collapses_repeats_to_one_entry_with_a_count() {
        let c = card("https://example.com/a%20b%20c%20d");
        assert_eq!(c.matches(r#"data-name="u0020""#).count(), 1, "{c}");
        assert!(
            c.contains(r#"3 times, in <span class="pl-path">the path</span>"#),
            "{c}"
        );
    }

    /// Iterative deploys at one version used to strand the year-long
    /// `immutable` cache on stale CSS (found live 2026-08-24); the asset URL
    /// now carries a fingerprint of the embedded assets beside the version.
    #[test]
    fn asset_urls_carry_a_content_fingerprint() {
        let url = asset_url("/static/app.css");
        assert!(
            url.starts_with(&format!("/static/app.css?v={VERSION}-")),
            "{url}"
        );
        let stamp = url.rsplit('-').next().unwrap();
        assert_eq!(stamp.len(), 16, "{url}");
        assert!(stamp.chars().all(|c| c.is_ascii_hexdigit()), "{url}");
    }

    #[test]
    fn a_card_with_no_marked_characters_has_no_cast() {
        let c = card("https://example.com/plain?q=1");
        assert!(!c.contains("pv-cast"), "{c}");
    }

    /// A carried address whose reading kept escapes closed earns an opened
    /// second line under its row: the inner `=` and `&` are that address's
    /// own grammar, and the domain it points at gets the bold.
    #[test]
    /// The hazard notes fold behind their chips: the chip carries the note's
    /// name, the note waits in the deck, and the no-JS stylesheet shows the
    /// deck in full. The wording avoids "this page", which read as the
    /// YuioLink page rather than the destination (the user, 2026-08-24).
    #[test]
    fn hazard_notes_fold_behind_their_chips() {
        let c = card("https://alice@example.com/x?next=https%3A%2F%2Fother.example%2F");
        assert!(
            c.contains(r#"<span class="pv-fact warn" data-note="user">"#),
            "{c}"
        );
        assert!(
            c.contains(r#"<span class="pv-fact warn" data-note="carries">"#),
            "{c}"
        );
        assert!(c.contains(r#"<div class="pv-notes">"#), "{c}");
        assert!(
            c.contains(r#"<span class="pv-note" data-note-for="user">"#),
            "{c}"
        );
        assert!(c.contains("The destination is <code>example.com</code>."), "{c}");
        assert!(!c.contains("this page is on"), "{c}");
        const APP_CSS: &str = include_str!("../static/app.css");
        assert!(APP_CSS.contains("html.js .pv-notes:not(:has(.pv-note.shown)) .pv-note {"));
        // A chip with nothing more to say is not a control.
        let plain = card("http://example.com/pay");
        assert!(!plain.contains("data-note"), "{plain}");
    }

    /// A hidden character in a parameter KEY marks red, chips, and answers in
    /// the cast, the same as one in a value. Found 2026-08-24: keys rendered
    /// verbatim with none of the invisible machinery, so a stored
    /// `?%E2%80%8Bref=1` showed a plain unmarked key. Keys are still never
    /// decoded — the escape stays on screen — the invisible just goes red.
    #[test]
    fn a_hidden_character_in_a_key_is_marked_and_cast() {
        let c = card("https://example.com/x?%E2%80%8Bref=1");
        // The key's escape wears the red mark, wired to the cast.
        assert!(
            c.contains(r#"<span class="qk"><span class="bad" data-tell="u200b""#),
            "{c}"
        );
        // The chip fires, and carries its entry for the click-to-step wiring.
        assert!(
            c.contains(r#"<span class="pv-fact warn" data-cast="u200b">"#),
            "{c}"
        );
        // The entry names the key as its place — the hidden character red,
        // the rest of the key in the key's own violet, matching the hero.
        assert!(
            c.contains(r#"in <code class="k"><span class="bad" data-tell="u200b""#),
            "{c}"
        );
        assert!(c.contains(r#">%E2%80%8B</span>ref</code>"#), "{c}");
        const APP_CSS_K: &str = include_str!("../static/app.css");
        // `.entry` is load-bearing: it outranks the warn override, so a red
        // entry's key place stays violet.
        assert!(APP_CSS_K.contains(".pv-cast .entry .what code.k {"));
        assert!(APP_CSS_K.contains(".pv-cast .what .bad {"));
        // The rest of the key stays verbatim beside the mark.
        assert!(c.contains(r#"</span>ref</span>"#), "{c}");

        // Padding steps through its chip the same way.
        let padded = card("https://example.com/x?q=a%20%20%20b");
        assert!(
            padded.contains(r#"<span class="pv-fact warn" data-cast="pad">"#),
            "{padded}"
        );

        // A card with nothing hidden tags no chip with a cast.
        let plain = card("https://example.com/x?ref=1");
        assert!(!plain.contains("data-cast"), "{plain}");
    }

    /// The cast's place words wear the ink of the region they point at,
    /// matching the hero — the same treatment the key place already had.
    #[test]
    fn cast_place_words_wear_their_role_ink() {
        // One place per card: a character seen in two places collapses to one
        // entry saying "in 2 places", with no place word at all.
        let c = card("https://example.com/my%20file");
        assert!(
            c.contains(r#"<span class="pl-path">the path</span>"#),
            "{c}"
        );
        let f = card("https://example.com/x#a%20b");
        assert!(
            f.contains(r#"<span class="pl-frag">the fragment</span>"#),
            "{f}"
        );
        const APP_CSS: &str = include_str!("../static/app.css");
        for rule in [
            ".pv-cast .what .pl-path { color: var(--c-path); }",
            ".pv-cast .what .pl-frag { color: var(--c-frag); }",
            ".pv-cast .what .pl-query { color: var(--c-key); }",
            ".pv-cast .what .pl-user { color: var(--danger); }",
        ] {
            assert!(APP_CSS.contains(rule), "missing rule: {rule}");
        }
    }

    #[test]
    fn a_carried_address_reads_bare_inside_its_capsule() {
        let c = card(
            "https://example.com/media?url=https%3A%2F%2Fimg.example%2Fa.jpg%3Fw%3D1080%26s%3Dabc",
        );
        assert!(c.contains("Carries Another Address"), "{c}");
        // Bare means real: the value decodes in place, the capsule is its
        // boundary, and no second line restates it.
        assert!(c.contains(r#"<span class="qv cv">"#), "{c}");
        assert!(c.contains(r#"<span class="reg">img.example</span>"#), "{c}");
        assert!(c.contains(r#"1080<span class="dl">&amp;</span>s"#), "{c}");
        assert!(!c.contains("Reads as"), "{c}");
        // A value with no structure inside earns no capsule.
        let plain = card("https://example.com/r?q=share");
        assert!(!plain.contains(" cv"), "{plain}");
    }

    /// The cast answers under the hero: it renders between the headline and
    /// the button, where a clicked character's entry appears (round 3).
    #[test]
    fn the_cast_sits_under_the_hero() {
        let c = card("https://example.com/my%20file?q=1");
        let hero = c.find(r#"id="destination""#).expect("a hero");
        let cast = c.find(r#"<div class="pv-cast">"#).expect("a cast");
        let button = c.find("Continue to").expect("a button");
        assert!(hero < cast && cast < button, "{c}");
        // And the slice table is gone from the web card entirely.
        assert!(!c.contains("pv-slices"), "{c}");
    }

    /// A card whose only decoding is receipted %20 spaces skips the record:
    /// the dotted receipt and its cast entry already name the stored form,
    /// so "Exactly as Stored" would restate the hero (the user's call,
    /// 2026-08-20). Anything decoded beyond that still earns it.
    #[test]
    fn a_spaces_only_card_keeps_the_receipt_and_skips_the_record() {
        let c = card("https://example.com/my%20file");
        assert!(c.contains("pv-cast"), "{c}");
        assert!(!c.contains("Exactly as Stored"), "{c}");
        let decoded = card("https://example.com/caf%C3%A9%20menu");
        assert!(decoded.contains("Exactly as Stored"), "{decoded}");
    }

    /// Design note 29's other half: the hero prefers to break at the URL's
    /// joints, so a wrapped line ends on a character that visibly cannot end
    /// a URL — and the stylesheet centres a one-line URL while left-aligning
    /// a wrapped one, with the layout engine as the wrap detector.
    #[test]
    fn the_hero_wraps_at_the_structure_and_falls_ragged_left() {
        let c = card("https://example.com/a/b?x=1&y=2");
        assert!(c.contains(r#"<span class="pn">/</span><wbr>"#), "{c}");
        assert!(c.contains(r#"<span class="pn">?</span><wbr>"#), "{c}");
        assert!(c.contains(r#"<span class="pn">&amp;</span><wbr>"#), "{c}");
        assert!(c.contains(r#"<span class="pn">=</span><wbr>"#), "{c}");
        const APP_CSS: &str = include_str!("../static/app.css");
        for rule in [
            "width: fit-content",
            "overflow-wrap: anywhere",
            "text-indent: -1.5ch",
        ] {
            assert!(APP_CSS.contains(rule), "app.css lost `{rule}`");
        }
    }

    #[test]
    fn an_undecodable_escape_stays_on_screen_as_stored() {
        let c = card("https://example.com/x?q=a%FFb");
        // Not valid UTF-8: no replacement mark, the escape itself stays.
        assert!(!c.contains('\u{fffd}'), "{c}");
        assert!(c.contains("%FF"), "{c}");
        // And since nothing was decoded, there is nothing to prove.
        assert!(!c.contains("Exactly as Stored"), "{c}");
    }

    #[test]
    fn a_one_run_host_shows_its_stored_characters() {
        // The one-run promise is character-identity with storage, so the host
        // never takes the decoded reading (no lowercasing, no punycode).
        let c = card("ftp://EXAMPLE.org/pub");
        assert!(c.contains(r#"<span class="reg">EXAMPLE.org</span>"#), "{c}");
        assert!(!c.contains("Exactly as Stored"), "{c}");
    }

    #[test]
    fn a_one_run_headline_is_the_stored_string_and_needs_no_record() {
        for stored in [
            "spotify:track:6rqhFgbbKwnb9MLmUQDhG6",
            "matrix:r/keebs:example.org",
            "ircs://libera.chat/yuiolink",
            "ftp://files.example.org/pub/notes.txt",
        ] {
            let c = card(stored);
            assert!(c.contains(r#"<code class="pv-line""#), "{stored}: {c}");
            assert!(!c.contains("Exactly as Stored"), "{stored}: {c}");
            assert!(c.contains("What opens it, if anything"), "{stored}");
        }
    }

    #[test]
    fn a_formatted_headline_always_carries_the_record_underneath() {
        for stored in [
            "mailto:a@b.example?subject=Hi",
            "tel:+47-820-12-345;ext=4021",
            "sms:+4799123456?body=JOIN",
            "magnet:?xt=urn:btih:abc&dn=x.iso",
            "xmpp:lobby@rooms.example.org?join",
        ] {
            assert!(card(stored).contains("Exactly as Stored"), "{stored}");
        }
    }

    #[test]
    fn the_button_describes_the_scheme_and_never_predicts_the_outcome() {
        assert!(card("mailto:a@b.example").contains("Write to a@b.example"));
        assert!(card("mailto:?subject=Hi").contains("Draft a Message"));
        assert!(card("mailto:a@b.example,c@d.example").contains("Write to 2 addresses"));
        assert!(card("tel:+4782012345").contains("Call +47 820 12 345"));
        // "Message", not "Text" -- Text is one of this site's link kinds.
        assert!(card("sms:+4799123456").contains("Message +47 99 12 34 56"));
        assert!(card("sms:+4799123456,+4791123456").contains("Message 2 numbers"));
        assert!(card("spotify:album:x").contains("An album in Spotify's catalogue"));
        assert!(card("matrix:u/ada:example.org").contains("A user on Matrix"));
        assert!(card("xmpp:a@b.example").contains("An XMPP chat address"));
        assert!(card("xmpp:a@b.example?join").contains("A chat room on XMPP"));
        assert!(card("magnet:?xt=urn:btih:abc").contains("A file identified by its hash"));
        // The hedge appears once, on every handoff card, and never on the web
        // tier -- a website opening is not a guess about the device.
        assert_eq!(
            card("magnet:?xt=urn:btih:abc")
                .matches("if anything")
                .count(),
            1
        );
        assert!(!card("https://example.com/").contains("if anything"));
    }

    #[test]
    fn phone_facts_ride_their_own_number_once_there_is_more_than_one() {
        // One number: the chips pool in a centred row.
        let one = card("sms:+4782012345");
        assert!(one.contains(r#"<div class="pv-facts">"#), "{one}");
        assert!(one.contains("Premium Rate"));
        // Two numbers: the stack, and a Premium Rate warning that points at ITS
        // number instead of at the card.
        let two = card("sms:+46701234567,+4782012345");
        assert!(two.contains(r#"<div class="pv-stack2">"#), "{two}");
        assert!(!two.contains(r#"<div class="pv-facts">"#), "{two}");
        assert!(two.contains("Sweden") && two.contains("Norway"));
        assert!(two.contains("Premium Rate"));
    }

    #[test]
    fn the_page_without_a_script_has_no_dead_controls() {
        // preview.js injects the checkboxes, the Copy pills, and the split's
        // second segment. None of them may be in the served markup: a control
        // that does nothing is worse than no control, and un-hiding one after
        // load is how this site earned its layout-shift history.
        for stored in [
            "https://alice@example.com/reset?next=x",
            "mailto:a@b.example,c@d.example?subject=Hi",
            "magnet:?xt=urn:btih:abc&dn=x.iso",
        ] {
            let c = card(stored);
            assert!(!c.contains("<input"), "{stored}: {c}");
            assert!(!c.contains("copybtn"), "{stored}: {c}");
            assert!(!c.contains("pv-split"), "{stored}: {c}");
            assert!(!c.contains("After Your Edits"), "{stored}: {c}");
            // The action is a full-width link either way.
            assert!(c.contains("btn-block"), "{stored}: {c}");
        }
    }

    /// The script, as text, so the two ends of the parts model can be checked
    /// against each other without a browser.
    const PREVIEW_JS: &str = include_str!("../static/preview.js");

    /// Every `receiver.field` access in `source`, for the receivers named.
    ///
    /// A deliberately small scanner: identifier runs, and whatever identifier
    /// follows the dot. It only has to be right about this one file.
    fn member_accesses(source: &str, receivers: &[&str]) -> std::collections::BTreeSet<String> {
        let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
        let chars: Vec<char> = source.chars().collect();
        let mut out = std::collections::BTreeSet::new();
        let mut i = 0;
        while i < chars.len() {
            if !is_ident(chars[i]) {
                i += 1;
                continue;
            }
            let start = i;
            while i < chars.len() && is_ident(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if chars.get(i) != Some(&'.') || !receivers.contains(&word.as_str()) {
                continue;
            }
            let field_start = i + 1;
            let mut j = field_start;
            while j < chars.len() && is_ident(chars[j]) {
                j += 1;
            }
            if j > field_start {
                out.insert(chars[field_start..j].iter().collect());
            }
        }
        out
    }

    /// The parts model is a contract between two files that cannot see each
    /// other, and this is the test that would have caught it being half-edited.
    ///
    /// It has been broken exactly that way once: a rename shipped on the server
    /// side (`h` -> `p`, `prefixHtml` -> `prefixRuns`) while `preview.js` still
    /// read the old names, so `build()` returned a field nothing consumed and
    /// the "After your edits" line never appeared on any card.
    #[test]
    fn the_script_reads_only_fields_the_server_emits() {
        // Rich enough to carry every kind of part in one model.
        let model: serde_json::Value = serde_json::from_str(&card_model(&urlview::parse_uri(
            "https://alice@example.com:8443/a;s=1?next=x&q=y#f",
        )))
        .expect("the model is JSON");

        let mut emitted: std::collections::BTreeSet<String> = model
            .as_object()
            .unwrap()
            .keys()
            .map(String::from)
            .collect();
        for part in model["parts"].as_array().unwrap() {
            emitted.extend(part.as_object().unwrap().keys().map(String::from));
        }
        // Added by readModel once the JSON is parsed.
        emitted.insert("byIndex".to_string());

        // Ordinary JavaScript, not fields of ours.
        const BUILTIN: &[&str] = &["forEach", "filter", "map", "slice", "push", "length"];

        for field in member_accesses(PREVIEW_JS, &["model", "part", "p"]) {
            assert!(
                emitted.contains(&field) || BUILTIN.contains(&field.as_str()),
                "preview.js reads `{field}`, which the server does not emit. \
                 Emitted: {emitted:?}"
            );
        }
    }

    /// The script's code with its comments removed, so a rule about what the
    /// code may contain is not confused by prose describing that same rule.
    /// (Good enough for this one file: it has no `//` inside a string literal.)
    fn code_only(source: &str) -> String {
        let mut out = String::with_capacity(source.len());
        let mut rest = source;
        loop {
            let block = rest.find("/*");
            let line = rest.find("//");
            match (block, line) {
                (Some(b), l) if l.is_none_or(|l| b < l) => {
                    out.push_str(&rest[..b]);
                    rest = rest[b..].find("*/").map_or("", |e| &rest[b + e + 2..]);
                }
                (_, Some(l)) => {
                    out.push_str(&rest[..l]);
                    rest = rest[l..].find('\n').map_or("", |e| &rest[l + e..]);
                }
                _ => {
                    out.push_str(rest);
                    return out;
                }
            }
        }
    }

    /// The other half of the same rule, from the script's side.
    ///
    /// The CSP carries `require-trusted-types-for 'script'` with no policy, so
    /// an innerHTML assignment throws. The half-edited build() gave itself away
    /// here too: it was still concatenating `'<span class="dl">'`.
    #[test]
    fn the_script_never_assembles_markup() {
        let code = code_only(PREVIEW_JS);
        for forbidden in ["innerHTML", "outerHTML", "insertAdjacentHTML", "<span"] {
            assert!(
                !code.contains(forbidden),
                "preview.js contains `{forbidden}`; Trusted Types forbids it, so it \
                 would throw on the first edit"
            );
        }
    }

    /// `withdrawn` is a three-file agreement — app.js sets it, app.js reads it
    /// back, and app.css draws it — which is the shape of mistake that already
    /// cost this project once (see the parts-model tests above). A rename in one
    /// file silently un-strikes a dead link, which is the one thing the class
    /// exists to prevent.
    #[test]
    fn the_withdrawn_marker_is_set_read_and_drawn() {
        const APP_JS: &str = include_str!("../static/app.js");
        const APP_CSS: &str = include_str!("../static/app.css");
        assert!(
            APP_JS.contains(r#"classList.add("withdrawn", "expired")"#),
            "app.js no longer marks a withdrawn link"
        );
        assert!(
            APP_JS.contains(r#"panel.classList.contains("withdrawn")"#),
            "the countdown no longer reads the marker, so the next tick will \
             un-strike a link the server has stopped serving"
        );
        assert!(
            APP_CSS.contains(".result.withdrawn .result-word"),
            "app.css no longer strikes a withdrawn link"
        );
    }

    #[test]
    fn no_markup_crosses_into_the_parts_model() {
        // The site's CSP carries `require-trusted-types-for 'script'` with no
        // policy allowed, so preview.js cannot assign innerHTML at all. The
        // model therefore ships (class, text) runs; a stray tag in here would
        // be a string the script could only render as literal text -- or, if
        // someone "fixed" that with innerHTML, a page that throws in Chrome.
        let model = card_model(&urlview::parse_uri(
            "https://alice@example.com/a?q=%3Cscript%3E&r=1#f",
        ));
        assert!(!model.contains('<'), "{model}");
        assert!(!model.contains("span"), "{model}");
    }

    #[test]
    fn a_slice_value_is_one_wrappable_unit_in_its_own_dress() {
        // The value is its own inline block, so a long one drops to its own
        // line whole instead of orphaning its tail after the key.
        let magnet = card(
            "magnet:?xt=urn:btih:c12fe3a94b81d7e05f2c6a9048bb3e1d7f4a2c60&tr=udp%3A%2F%2Ftracker.example.org%3A6969",
        );
        assert!(
            magnet.contains(r#"<span class="val">urn:btih:c12fe"#),
            "{magnet}"
        );
        // A tracker reads as the address it is: dim punctuation, bold domain,
        // and no wash -- that is spent once per page, on the headline.
        assert!(
            magnet.contains(
                r#"udp<span class="dl">://</span>tracker.<span class="reg">example.org</span>"#
            ),
            "{magnet}"
        );
        // ...and no chip: a tracker is what a magnet is made of.
        assert!(!magnet.contains("Carries Another Address"), "{magnet}");

        // A cc address wears the dress its recipients wear.
        let mail = card("mailto:sales@example.com?cc=archive@records.example");
        assert!(
            mail.contains(r#"<span class="lp">archive</span><span class="dl">@</span><span class="reg">records.example</span>"#),
            "{mail}"
        );
    }

    /// The register the URL wears, settled 2026-08-15 after the user saw two
    /// candidates side by side.
    ///
    /// It is the raw line's look, applied everywhere: flat full-colour text in
    /// which the BOLD KEYS carry the structure, with dimming spent only on
    /// characters that are genuinely inert. A brief experiment demoted the
    /// tails' VALUES to secondary to lift the path above them; that is reverted,
    /// because bold keys do the same job without draining the line.
    ///
    /// Colour lives in app.css; what this pins is that each token gets the class
    /// that carries it, and that the classes stay separable.
    #[test]
    fn every_token_on_the_url_line_gets_its_own_class() {
        let c = card("https://www.threads.com/@milestogo13/post/Db3LGarHIem?xmt=AQG0&slof=1");
        // The domain keeps bold AND full colour AND the wash AND the size -- the
        // only token wearing all four, which is what keeps its rank unambiguous.
        assert!(c.contains(r#"<span class="reg">threads.com</span>"#), "{c}");
        for segment in ["@milestogo13", "post", "Db3LGarHIem"] {
            assert!(
                c.contains(&format!(r#"<span class="ps">{segment}</span>"#)),
                "{segment} should be a path segment: {c}"
            );
        }
        // Keys are the signposts; values sit beside them at full strength.
        assert!(c.contains(r#"<span class="qk">xmt</span>"#), "{c}");
        assert!(c.contains(r#"<span class="qk">slof</span>"#), "{c}");
        assert!(c.contains(r#"<span class="qv">AQG0</span>"#), "{c}");

        let mixed = card("https://example.com/a;sid=1/b?q=x#f");
        assert!(mixed.contains(r#"<span class="ps">a</span>"#), "{mixed}");
        assert!(mixed.contains(r#"<span class="qk">sid</span>"#), "{mixed}");
        assert!(mixed.contains(r#"<span class="qv">1</span>"#), "{mixed}");
        // A keyless fragment is a value, not a key — and wears the fragment's
        // own teal (C1).
        assert!(mixed.contains(r#"<span class="seg fg">f</span>"#), "{mixed}");
        // An `=`-shaped fragment unrolls first, so the OAuth case's keys ARE keys.
        let oauth = card("https://example.com/cb#access_token=abc&expires_in=3600");
        assert!(
            oauth.contains(r#"<span class="qk">access_token</span>"#),
            "{oauth}"
        );
        assert!(
            card("ftp://files.example.org/pub/notes.txt")
                .contains(r#"<span class="ps">pub</span>"#)
        );
    }

    /// The register itself, which lives in the stylesheet. Pinned because "less
    /// dimming, not more" is a decision a later tidy-up could quietly undo one
    /// declaration at a time.
    #[test]
    fn the_url_register_keeps_its_colours() {
        const APP_CSS: &str = include_str!("../static/app.css");
        assert!(
            APP_CSS.contains(".pv-url .qv {\n    color: var(--text);\n}"),
            "query values must read at full strength"
        );
        assert!(
            APP_CSS.contains(".pv-url .seg {\n    color: var(--text);\n}"),
            "a keyless fragment is a value too"
        );
        assert!(
            APP_CSS
                .contains(".pv-url .qk {\n    color: var(--c-key);\n    font-weight: 700;\n}"),
            "keys are the signposts: their own hue (C1), bold"
        );
        // Every role hue is defined once per theme. The host never uses
        // --accent raw: 4.02:1 on white is an AA fail, so it has its own
        // darker blue, and the subdomain a steel of the same hue.
        assert!(APP_CSS.contains(".pv-url .reg {\n    color: var(--c-host);"));
        assert!(APP_CSS.contains(".pv-url .sub {\n    color: var(--c-sub);\n}"));
        for var in [
            "--c-port:", "--c-host:", "--c-sub:", "--c-path:", "--c-key:", "--c-frag:",
        ] {
            assert_eq!(
                APP_CSS.matches(var).count(),
                2,
                "{var} needs a value in each theme"
            );
        }
        assert!(APP_CSS.contains("color: var(--c-port);"));
        // Still dim: the two things that really are inert.
        assert!(APP_CSS.contains(".pv-url .sch {\n    color: var(--text-tertiary);\n}"));
        assert!(APP_CSS.contains(".pv-url .pe {\n    color: var(--text-tertiary);\n}"));
        // A carried address inside a value: its punctuation recedes, and its
        // domain keeps bold but never the wash, the blue, or the size -- those
        // are spent once per page, on the headline's host.
        assert!(APP_CSS.contains(".pv-url .dl {\n    color: var(--text-tertiary);\n}"));
        assert!(APP_CSS.contains(
            ".pv-url .qv .reg,\n.pv-url .seg .reg {\n    color: var(--text);\n    font-size: 1em;\n    padding: 0;\n    background: none;\n}"
        ));
        // The receipt rides the glyphs' own baseline as decoration, not a
        // border: line-height cannot push it away, and Safari draws it on a
        // lone space (verified on-device 2026-08-20, design note 28).
        assert!(
            APP_CSS.contains(
                ".dsp {\n    text-decoration: underline dotted var(--text-tertiary) 1px;\n    text-underline-offset: 2px;\n}"
            ),
            "the dotted receipt must be text-decoration, not border-bottom"
        );
    }

    /// An explicit port is unusual and it decides which server actually answers,
    /// so it is the one token that gets a colour of its own rather than another
    /// shade of grey. It must reach every surface, and it must not drag along
    /// the keyless fragment it used to share `.seg` with.
    #[test]
    fn the_port_is_marked_on_every_surface() {
        let c = card("https://alice@example.com:8443/reset?next=x");
        assert!(c.contains(r#"<span class="port">8443</span>"#), "{c}");
        // The runs preview.js rebuilds the edited line from carry it too.
        let model = card_model(&urlview::parse_uri("https://example.com:8443/x"));
        assert!(model.contains(r#"["port","8443"]"#), "{model}");
        // A one-run headline gets it as well.
        assert!(
            card("ftp://files.example.org:2121/pub").contains(r#"<span class="port">2121</span>"#)
        );
        // The fragment did not come along for the ride.
        assert!(
            card("https://example.com:8443/x#step-2")
                .contains(r#"<span class="seg fg">step-2</span>"#)
        );
    }

    /// The shared classes must not drag a cannot-be-a-base scheme anywhere. A
    /// magnet has no authority, so nothing to rank a path against; its
    /// parameters are the destination, not noise beside one, and they read at
    /// full strength with their keys bold like everything else.
    #[test]
    fn a_cannot_be_a_base_scheme_keeps_the_dress_it_had() {
        let c = card("magnet:?xt=urn:btih:abc&dn=x.iso");
        assert!(
            c.contains(r#"<span class="val">urn:btih:abc</span>"#),
            "{c}"
        );
        assert!(c.contains(r#"<span class="k">xt</span>"#), "{c}");
        assert!(!c.contains(r#"class="ps""#), "{c}");
        assert!(!c.contains(r#"class="port""#), "{c}");
        assert!(card("mailto:a@b.example?subject=Hi").contains(r#"<span class="val">Hi</span>"#));
    }

    #[test]
    fn the_region_chip_stands_bare_beside_a_pill() {
        let c = card("tel:+4782012345");
        assert!(c.contains(r#"<span class="pv-fact region">"#), "{c}");
        // The type chip keeps the pill, and a warning is still a warning.
        assert!(c.contains("Premium Rate"));
    }

    #[test]
    fn a_note_appears_only_where_the_standard_opens_a_gap() {
        assert!(card("magnet:?xt=urn:btih:abc&dn=x.iso").contains("Only <code>xt</code>"));
        assert!(card("mailto:a@b.example?body=hi").contains("goes out as you"));
        assert!(card("sms:+4799123456?body=hi").contains("goes out from your number"));
        assert!(card("tel:+4782012345;ext=4021").contains("dialled after the call connects"));
        // Nothing to explain, nothing said.
        assert!(!card("tel:+4782012345").contains("pv-note"));
        assert!(!card("https://example.com/a").contains("pv-note"));
    }
}
