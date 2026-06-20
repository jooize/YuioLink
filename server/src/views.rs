//! HTML views, rendered with maud (escaped by default).
//!
//! The markup leans on semantic elements — `header`/`main`/`footer`, `fieldset`/
//! `legend` for the radio groups, `output` for created links, `code`/`pre` for
//! machine text — and reserves classes for genuinely styled components.

use maud::{DOCTYPE, Markup, PreEscaped, html};

const ICON_CHAIN: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>"#;

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
                        span.app-icon aria-hidden="true" { (PreEscaped(ICON_CHAIN)) }
                        h1 { "YuioLink" }
                    }
                    (body)
                }
                (scripts)
            }
        }
    }
}

/// The result `<output>` shown after a link is created (server-rendered on the
/// no-JS path, populated in place by `app.js` otherwise). The URL is the focus;
/// a single meta line carries kind, expiry, and any use limit.
fn result_output(url: Option<&str>, meta: Markup) -> Markup {
    html! {
        // tabindex=-1: focused after creation so the link selection survives for ⌘C
        // and the next Tab lands on the input (the panel precedes the form in the DOM).
        output.result #link-panel tabindex="-1" hidden[url.is_none()] {
            code.result-url #link-element { @if let Some(u) = url { (u) } }
            div.result-foot {
                small.result-meta #link-expiry { (meta) }
                // app.js fills this with the platform copy shortcut (⌘C / Ctrl+C).
                small.result-hint #result-hint {}
            }
        }
    }
}

/// The landing page. Works without JavaScript (the form posts to `POST /` and a
/// result page comes back); `app.js` progressively enhances it with live type
/// detection, keyboard shortcuts, an in-place result, and copy. Encryption is
/// only offered when the operator enabled it (`encryption_enabled`).
pub fn index_page(encryption_enabled: bool, api_base: &str) -> Markup {
    let head_extra = html! {
        // app.js reads this to decide which backend to call; empty = same origin.
        meta name="yuiolink-api-base" content=(api_base);
    };
    let body = html! {
        p { "Wieldy Ephemeral Link" }
        @if encryption_enabled {
            noscript { p { "Creating links works without JavaScript; encryption needs it." } }
        }

        // Compact storage status (top) linking down to the full history. Its space
        // is reserved (CSS `visibility`) so it never moves the layout; app.js adds
        // `.shown` once there is history.
        a.storage-indicator #storage-indicator href="#history" {}

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
                button #copy.btn.split-copy type="button" disabled { "+ Copy" }
            }

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
                    label.seg-label for="ttl-custom" { "Custom" }
                }
                div.custom-field #ttl-custom-field {
                    input #ttl-custom-value.custom-num name="ttl_custom" type="number"
                        min="1" inputmode="numeric" placeholder="30";
                    select #ttl-custom-unit.custom-unit name="ttl_unit" {
                        option value="m" selected { "minutes" }
                        option value="h" { "hours" }
                        option value="d" { "days" }
                    }
                }
            }

            fieldset.picker {
                legend { "Limit" }
                div.segmented {
                    input.seg-radio #limit-unlimited type="radio" name="limit" value="unlimited" checked;
                    label.seg-label for="limit-unlimited" {
                        span.infinity aria-label="Unlimited" { "∞" }
                    }
                    input.seg-radio #limit-1 type="radio" name="limit" value="1";
                    label.seg-label for="limit-1" { "1" }
                    input.seg-radio #limit-custom type="radio" name="limit" value="custom";
                    label.seg-label for="limit-custom" { "Custom" }
                }
                div.custom-field #limit-custom-field {
                    input #limit-custom-value.custom-num name="limit_custom" type="number"
                        min="1" inputmode="numeric" placeholder="Number of uses";
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
        section.history #history hidden {
            h2.history-title { "Recent links" }
            ul.history-list #history-list {}
            div.history-actions {
                label.history-persist for="history-persist" {
                    input #history-persist type="checkbox";
                    span { "Save on this device" }
                }
                button.history-clear #history-clear type="button" { "Clear" }
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
        div.split-btn {
            a.btn.split-primary href=(url) { "Open link" }
            button #copy-link.btn.split-copy type="button" { "Copy" }
        }
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
