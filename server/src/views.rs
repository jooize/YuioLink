//! HTML views, rendered with maud (escaped by default).
//!
//! The markup leans on semantic elements — `header`/`main`/`footer`, `fieldset`/
//! `legend` for the radio groups, `output` for created links, `code`/`pre` for
//! machine text — and reserves classes for genuinely styled components.

use maud::{DOCTYPE, Markup, PreEscaped, html};

const ICON_CHAIN: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>"#;

const ICON_CHECK: &str = r#"<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M9.55 17.6 4.4 12.45l1.4-1.4 3.75 3.75 8.85-8.85 1.4 1.4z"/></svg>"#;

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
/// no-JS path, populated in place by `app.js` otherwise).
fn result_output(url: Option<&str>, expiry_line: Markup) -> Markup {
    html! {
        output.result #link-panel hidden[url.is_none()] {
            strong.result-label { (PreEscaped(ICON_CHECK)) " Your ephemeral link is ready" }
            code #link-element { @if let Some(u) = url { (u) } }
            small.result-expiry #link-expiry { (expiry_line) }
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
        p { "Memorable links that always expire — redirect or text." }
        @if encryption_enabled {
            noscript { p { "Creating links works without JavaScript; encryption needs it." } }
        }

        // Filled in place by app.js; the no-JS path reloads to a result page.
        (result_output(None, html! {}))

        form #create-form method="post" action="/" {
            textarea #content.form-control name="content" rows="1"
                autocomplete="off" autocapitalize="off" spellcheck="false"
                placeholder="Paste a link to redirect, or type text to share" autofocus {}

            // Native radios so the picker works without JS; "Auto" lets the server
            // detect. app.js shows a live hint and resolves "Auto" before the API call.
            fieldset.picker {
                legend { "Type" small.hint #detected-hint {} }
                div.segmented {
                    input.seg-radio #kind-auto type="radio" name="kind" value="auto" checked;
                    label.seg-label for="kind-auto" { "Auto" }
                    input.seg-radio #kind-redirect type="radio" name="kind" value="redirect";
                    label.seg-label for="kind-redirect" { "Redirect" }
                    input.seg-radio #kind-text type="radio" name="kind" value="text";
                    label.seg-label for="kind-text" { "Text" }
                }
            }

            fieldset.picker {
                legend { "Expires in" }
                div.segmented {
                    input.seg-radio #ttl-600 type="radio" name="ttl_seconds" value="600";
                    label.seg-label for="ttl-600" { "10 min" }
                    input.seg-radio #ttl-3600 type="radio" name="ttl_seconds" value="3600";
                    label.seg-label for="ttl-3600" { "1 hour" }
                    input.seg-radio #ttl-86400 type="radio" name="ttl_seconds" value="86400" checked;
                    label.seg-label for="ttl-86400" { "1 day" }
                    input.seg-radio #ttl-604800 type="radio" name="ttl_seconds" value="604800";
                    label.seg-label for="ttl-604800" { "7 days" }
                }
            }

            details.advanced {
                summary { "Advanced" }
                label.field-row for="max-uses" {
                    "Burn after"
                    input #max-uses name="max_uses" type="number" min="1" inputmode="numeric"
                        placeholder="Unlimited";
                    "uses"
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

            div.split-btn {
                button #submit.btn.split-primary type="submit" { "Create Link" }
                button #copy.btn.split-copy type="button" disabled { "+ Copy" }
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
pub fn result_page(url: &str, expires_at: &str, max_uses: Option<i64>) -> Markup {
    let expiry = html! {
        "Expires " (expires_at) " UTC"
        @if let Some(max) = max_uses {
            " · burns after " (max) (if max == 1 { " use" } else { " uses" })
        }
    };
    let body = html! {
        (result_output(Some(url), expiry))
        div.split-btn {
            a.btn.split-primary href=(url) { "Open link" }
            button #copy-link.btn.split-copy type="button" { "+ Copy" }
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
