//! HTML views, rendered with maud (escaped by default).
//!
//! The markup leans on semantic elements — `header`/`main`/`footer`, `fieldset`/
//! `legend` for the radio groups, `output` for created links, `code`/`pre` for
//! machine text — and reserves classes for genuinely styled components.

use std::time::{SystemTime, UNIX_EPOCH};

use maud::{DOCTYPE, Markup, html};

use crate::urlview::{IdnWarning, UrlView};

/// The crate version, shown in the footer and linked to its release tag.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The date the footer reports as "last updated", as `YYYY-MM-DD`. A constant
/// rather than a build timestamp on purpose: builds stay reproducible, and a
/// visitor wants to know when the site last changed, not when this binary was
/// compiled. Bump it alongside the workspace version.
const RELEASE_DATE: &str = "2026-08-04";

/// The shared page shell: head, the glass "app window", and the masthead.
///
/// Every page passes its own `<title>` — there is no bare "YuioLink" fallback,
/// because a tab or a history entry that says only the site name is the one place
/// the visitor cannot tell two of them apart. The masthead `<h1>` is plain text,
/// not a home link: clicking it on the create page would discard whatever the
/// user had typed, so it is no longer a navigation target.
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
                        // Two-tone wordmark, matching the share card: the accent
                        // lives inside the name rather than in a separate mark.
                        h1 { "Yuio" span.wm-link { "Link" } }
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
                div.result-actions {
                    // A real link to the created URL (right-click gives Copy Link);
                    // app.js fills the href and turns a left click into a copy.
                    a.result-copy #copy-result hidden { "Copy" }
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
                // Dead without JS; app.js un-hides it when it wires the handler.
                button #clear.btn.split-clear type="button" hidden { "Clear" }
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
                        "Public names are short words from a public wordlist, so anyone can "
                        "run the whole list and turn up every public link. "
                        strong { "Ideal for convenience and easy sharing" }
                        " — never for anything secret. "
                        a href="/wordlist.txt" { "Browse the wordlist →" }
                    }
                    div.details-body.for-secret {
                        "Link name is four random words from a 47-bit namespace — about "
                        "143 trillion possibilities — and nothing lists or indexes it, so "
                        "reaching the link means guessing its exact name within its "
                        "lifetime. "
                        strong.nowrap { "The name is the secret" }
                        ", and it exists only until the link expires. The server can read "
                        "the destination — this hides the link, not its contents."
                    }
                    div.details-body.for-once {
                        strong { "Deleted from the server when revealed" }
                        // Every link previews (docs/PREVIEW.md), but it only matters here:
                        // it is what keeps an unfurler or prefetch bot from burning the
                        // link before the recipient sees it.
                        ", and with the same security as a secret link. The recipient opens "
                        "a preview first and chooses to reveal — nothing is deleted until "
                        "they do."
                    }
                }
            }

            fieldset.picker #ttl-picker {
                legend { "Expires after" }
                // JS path (app.js un-hides and drives these): a big readout over a
                // stepped slider whose 17 stops are sensible durations from 1 minute
                // to 7 days. Tapping the readout opens the exact field below.
                button.ttl-readout #ttl-readout type="button" hidden
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
        section.history.collapsed #history hidden {
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
            "A project by " a href="https://github.com/jooize" { "jooize" }
            span.with-ai { " (with AI)" }
            " · "
            a href="https://github.com/jooize/YuioLink" { "Source on GitHub" }
            " · "
            a href="/help" { "How to Use" }
            " · "
            a href="/stats" { "Statistics" }
            span.footer-updated {
                "Updated " (format_card_date(RELEASE_DATE)) " · "
                a href=(format!("https://github.com/jooize/YuioLink/releases/tag/v{VERSION}")) {
                    "v" (VERSION)
                }
            }
        }
    };
    let scripts = html! { script src="/static/app.js" {} };
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
        p { a href="/" { "Create another" } }
    };
    let scripts = html! { script src="/static/app.js" {} };
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
    let title = match &i.target {
        Target::Redirect(url) => link_title("Redirect", i.name, Some(&url.card_domain())),
        Target::TextSnippet => link_title("Text", i.name, None),
    };
    document_full(&title, head, body, html! {})
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
            let date = format_card_date(i.expires_at);
            let desc = match format_card_time(i.expires_at) {
                Some(time) => format!("{kind} redirect that expires {date} at {time}."),
                None => format!("{kind} redirect that expires {date}."),
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
            document_full(
                &link_title("Redirect", r.name, Some(&url.card_domain())),
                html! {},
                body,
                html! {},
            )
        }
        RevealedTarget::Text(text) => {
            let body = html! {
                (back)
                p.pv-revealed { "Deleted from the server on this view — refreshing won't bring it back." }
                pre.text-body #text-body { (text) }
                // Dead without JS; text.js un-hides it when it wires the handler.
                button.btn.btn-block #copy-text type="button" hidden { "Copy" }
            };
            document_full(
                &link_title("Text", r.name, None),
                html! {},
                body,
                html! { script src="/static/text.js" {} },
            )
        }
    }
}

/// A plaintext Text link, rendered immediately (unlimited text). The body is an
/// escaped `<pre>` — maud escapes it, so a `<script>` in the content shows as text
/// and never executes. We never emit it as live HTML.
pub fn text_view_page(name: &str, text: &str) -> Markup {
    let body = html! {
        pre.text-body #text-body { (text) }
        // Dead without JS; text.js un-hides it when it wires the handler.
        button.btn.btn-block #copy-text type="button" hidden { "Copy" }
    };
    document_full(
        &link_title("Text", name, None),
        html! {},
        body,
        html! { script src="/static/text.js" {} },
    )
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
    document_full("YuioLink — Link Gone", html! {}, body, html! {})
}

/// 404 Not Found: nothing here — expired, recycled, or never existed. Framed as
/// by-design, since every YuioLink is ephemeral.
pub fn not_found_page() -> Markup {
    let body = html! {
        p.error-code { "404" }
        p { "This link has expired or never existed — links on YuioLink are ephemeral." }
        a.btn.btn-block href="/" { "Create a New Link" }
    };
    document_full("YuioLink — Link Not Found", html! {}, body, html! {})
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
        p.back-link { a href="/" { "← Back to YuioLink" } }
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

        h3.help-h { "The three types" }
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
            "curl --data-binary @notes.txt -d uses=1 " (base_url) "create\n"
        }
        p.help-p.help-note {
            code { "ttl" } " and " code { "uses" } " have to come last: everything before them "
            "is the content, so a URL keeps its own query string. The reply is the short URL, "
            "or JSON with " code { "Accept: application/json" } ". This endpoint makes public "
            "and one-time links — a secret one needs the "
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
        "YuioLink — How to Use",
        html! {
            meta name="description" content="How YuioLink works: why every link expires, what the public, secret, and one-time types are for, and how to create a link from a terminal.";
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
        p.back-link { a href="/" { "← Back to YuioLink" } }
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
}
