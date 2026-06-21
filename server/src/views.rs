//! HTML views, rendered with maud (escaped by default).
//!
//! The markup leans on semantic elements — `header`/`main`/`footer`, `fieldset`/
//! `legend` for the radio groups, `output` for created links, `code`/`pre` for
//! machine text — and reserves classes for genuinely styled components.

use maud::{DOCTYPE, Markup, html};

/// The shared page shell: head, the glass "app window", and the masthead.
fn document(body: Markup, scripts: Markup) -> Markup {
    document_full(html! {}, body, scripts)
}

/// As [`document`], but with extra `<head>` markup (e.g. a config `<meta>`).
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
                        h1 { "YuioLink" }
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

/// The result `<output>` shown after a link is created (server-rendered on the
/// no-JS path, populated in place by `app.js` otherwise). The memorable word (the
/// link name) is the hero; the full URL sits small beneath it; a single meta line
/// carries kind, expiry, and any use limit.
fn result_output(url: Option<&str>, meta: Markup) -> Markup {
    html! {
        // tabindex=-1: focused after creation so ⌘C copies the link (app.js intercepts
        // it — no visible selection needed) and the next Tab lands on the input (the
        // panel precedes the form in the DOM).
        output.result #link-panel tabindex="-1" hidden[url.is_none()] {
            // The link name is the giant hero (R4); the full URL sits small beneath,
            // then the meta line and a Copy pill, with an optional note last.
            code.result-word #link-word { @if let Some(u) = url { (link_name(u)) } }
            code.result-url #link-element { @if let Some(u) = url { (u) } }
            div.result-foot {
                small.result-meta #link-expiry { (meta) }
                div.result-actions {
                    // Revealed by app.js (copy needs JS). The link already exists here,
                    // so this copy is synchronous and reliable.
                    button.result-copy #copy-result type="button" hidden { "Copy" }
                }
            }
            small.result-note #result-note hidden {}
        }
    }
}

/// The landing page. Works without JavaScript (the form posts to `POST /` and a
/// result page comes back); `app.js` progressively enhances it with live type
/// detection, keyboard shortcuts, an in-place result, and copy. Encryption is
/// only offered when the operator enabled it (`encryption_enabled`).
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

pub fn index_page(encryption_enabled: bool, api_base: &str, max_ttl_secs: i64) -> Markup {
    let head_extra = html! {
        // app.js reads this to decide which backend to call; empty = same origin.
        meta name="yuiolink-api-base" content=(api_base);
    };
    let body = html! {
        p { "Wieldy Ephemeral Link" }
        @if encryption_enabled {
            noscript { p { "Creating links works without JavaScript; encryption needs it." } }
        }

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

            // No Type picker: a URL is always a redirect, anything else is text, so
            // the kind is detected and named on the button itself. app.js updates the
            // label live ("Create Redirect Link" / "Create Text Link"); without JS the
            // server detects on submit and this generic label is fine. The Copy button
            // creates first, then copies (it drops the "+" once the current input has
            // a link).
            div.split-btn {
                button #submit.btn.split-primary type="submit" { "Create Link" }
                button #clear.btn.split-clear type="button" { "Clear" }
            }
            // Whole-form errors (failed request, server rejection) appear here, on the
            // page — app.js fills and reveals it instead of alerting.
            p.form-error #form-error role="alert" hidden {}

            // Native radios so the pickers work without JS. "Custom" reveals an extra
            // field (CSS `:has()` on the no-JS path; app.js also focuses it).
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
                    // No real value — the greyed placeholder shows the default (5) that
                    // app.js applies when the box is left blank. min/step give the
                    // browser native validation of anything typed.
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
                    // Blank defaults to Once (app.js); native min/step validate a typed value.
                    input #limit-custom-value.custom-num name="limit_custom" type="number"
                        min="1" max="1000000000" step="1" inputmode="numeric" placeholder="Times";
                }
            }

            @if encryption_enabled {
                label.switch-row for="encrypt" {
                    span.switch-label {
                        strong { "Encrypt" }
                        small { "End-to-end, in your browser. The key never reaches the server." }
                    }
                    span.switch {
                        input #encrypt type="checkbox";
                        span.switch-track { span.switch-thumb {} }
                    }
                }
            }
        }

        // Created-link history (bottom). Kept in memory for the session unless the
        // user ticks "Save on this device", which opts into localStorage. app.js
        // fills the list and toggles persistence.
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
    let scripts = html! {
        @if encryption_enabled { script src="/static/crypto.js" {} }
        script src="/static/app.js" {}
    };
    document_full(head_extra, body, scripts)
}

/// The no-JS result page shown after `POST /` creates a link.
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
        p {
            a href={ (url) "+" } { "Preview" } " · " a href="/" { "Create another" }
        }
    };
    // app.js wires the Copy button if present; the link works without it.
    let scripts = html! { script src="/static/app.js" {} };
    document(body, scripts)
}

pub fn encrypted_redirect_page(sealed: &str) -> Markup {
    let body = html! {
        p #status { "Decrypting your link…" }
        noscript { p { "JavaScript is required to decrypt this link." } }
        // The ciphertext rides in an attribute (maud escapes it) rather than an
        // inline script string — no breakout, no XSS.
        div #payload data-sealed=(sealed) hidden {}
    };
    let scripts = html! {
        script src="/static/crypto.js" {}
        script src="/static/redirect.js" {}
    };
    document(body, scripts)
}

/// A plaintext Text link. The body is rendered as an escaped `<pre>` — maud
/// escapes it, so a `<script>` in the content shows as text and never executes.
/// We never emit it as live HTML (that would be stored XSS on our own origin).
pub fn text_view_page(text: &str) -> Markup {
    let body = html! {
        pre.text-body #text-body { (text) }
        button.btn.btn-block #copy-text type="button" { "Copy" }
        footer { a href="/" { "Back to YuioLink" } }
    };
    // text.js only wires the Copy button here (no payload to decrypt).
    let scripts = html! { script src="/static/text.js" {} };
    document(body, scripts)
}

/// An encrypted Text link. The ciphertext rides in a data attribute; `text.js`
/// decrypts it with the key from the URL fragment and fills the `<pre>` via
/// `textContent` (never `innerHTML`), so decrypted content is also inert.
pub fn encrypted_text_page(sealed: &str) -> Markup {
    let body = html! {
        p #status { "Decrypting…" }
        noscript { p { "JavaScript is required to decrypt this text." } }
        pre.text-body #text-body hidden {}
        button.btn.btn-block #copy-text type="button" hidden { "Copy" }
        div #payload data-sealed=(sealed) hidden {}
    };
    let scripts = html! {
        script src="/static/crypto.js" {}
        script src="/static/text.js" {}
    };
    document(body, scripts)
}

pub fn error_page(code: u16, message: &str) -> Markup {
    let body = html! {
        p.error-code { (code) }
        p { (message) }
        footer { a href="/" { "Back to YuioLink" } }
    };
    document(body, html! {})
}

pub struct Preview<'a> {
    pub short_url: &'a str,
    pub kind: &'a str,
    pub encrypted: bool,
    pub target: Option<&'a str>,
    pub hits: i64,
    pub created_at: &'a str,
    pub expires_at: &'a str,
    pub max_uses: Option<i64>,
}

/// The `yuio.link/:name+` preview/info page: where a link goes, its kind, hit
/// count, expiry, and remaining uses — without redirecting or counting a hit.
pub fn preview_page(p: Preview) -> Markup {
    let body = html! {
        p { "Link preview" }

        output.result {
            strong.result-label { (p.kind) }
            code { (p.short_url) }
        }

        @if let Some(target) = p.target {
            p { "Destination" }
            p { code { a href=(target) rel="nofollow noopener noreferrer" { (target) } } }
            a.btn.btn-block href=(p.short_url) { "Continue" }
        } @else if p.encrypted {
            p { "Encrypted — the content is hidden from the server and opens in your browser with the key from the original link." }
        } @else {
            p { "This is a Text link — open it to read." }
            a.btn.btn-block href=(p.short_url) { "Open" }
        }

        p.meta {
            "created " (p.created_at) " · " (p.hits) " hits · expires " (p.expires_at) " UTC"
            @if let Some(max) = p.max_uses {
                " · " ((max - p.hits).max(0)) " of " (max) " uses left"
            }
        }

        footer { a href="/" { "Back to YuioLink" } }
    };
    document(body, html! {})
}
