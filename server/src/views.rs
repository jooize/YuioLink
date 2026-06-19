//! HTML views, rendered with maud (escaped by default).

use maud::{DOCTYPE, Markup, PreEscaped, html};

const ICON_CHAIN: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>"#;

const ICON_CHECK: &str = r#"<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M9.55 17.6 4.4 12.45l1.4-1.4 3.75 3.75 8.85-8.85 1.4 1.4z"/></svg>"#;

/// The shared page shell: head, the glass "app window", and the icon/title header.
fn document(body: Markup, scripts: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover";
                meta name="color-scheme" content="light dark";
                title { "YuioLink" }
                link rel="stylesheet" href="/static/app.css";
            }
            body {
                main.app-window {
                    div.app-head {
                        div.app-icon aria-hidden="true" { (PreEscaped(ICON_CHAIN)) }
                        h1.app-title { "YuioLink" }
                    }
                    (body)
                }
                (scripts)
            }
        }
    }
}

pub fn index_page() -> Markup {
    let body = html! {
        p.app-subtitle { "Memorable links that always expire — redirect or text, optionally encrypted." }
        noscript { p.app-subtitle { "JavaScript is required to create and decrypt links." } }

        div.result #link-panel role="status" hidden {
            div.result-label { (PreEscaped(ICON_CHECK)) " Your ephemeral link is ready" }
            div.result-row { code #link-element {} }
            div.result-expiry #link-expiry {}
        }

        form #create-form method="post" action="/" {
            div.form-group {
                textarea #content.form-control name="content" rows="1"
                    autocomplete="off" autocapitalize="off" spellcheck="false"
                    placeholder="Paste a link to redirect, or type text to share" autofocus {}
            }

            // Auto-detected redirect-vs-text, with a one-tap manual override.
            div.field-label { "Type" }
            div.segmented #mode-toggle role="group" aria-label="Link type" {
                button #mode-redirect.seg-btn type="button" data-mode="redirect" { "Redirect" }
                button #mode-text.seg-btn type="button" data-mode="text" { "Text" }
            }

            div.field-label { "Expires in" }
            div.segmented #ttl-toggle role="group" aria-label="Expiry" {
                button.seg-btn type="button" data-ttl="600" { "10 min" }
                button.seg-btn type="button" data-ttl="3600" { "1 hour" }
                button.seg-btn.active type="button" data-ttl="86400" { "1 day" }
                button.seg-btn type="button" data-ttl="604800" { "7 days" }
            }

            details.advanced {
                summary { "Advanced" }
                label.field-row for="max-uses" {
                    span.field-row-label { "Burn after" }
                    input #max-uses type="number" min="1" inputmode="numeric"
                        placeholder="Unlimited";
                    span.field-row-suffix { "uses" }
                }
            }

            label.switch-row for="encrypt" {
                span.switch-label {
                    span.switch-title { "Encrypt" }
                    span.switch-desc { "End-to-end, in your browser. The key never reaches the server." }
                }
                span.switch {
                    input #encrypt type="checkbox" name="encrypt";
                    span.switch-track { span.switch-thumb {} }
                }
            }

            div.split-btn {
                button #submit.btn.split-primary type="submit" { "Create Link" }
                button #copy.btn.split-copy type="button" disabled { "+ Copy" }
            }
        }

        footer.app-footer {
            "A project by " a href="https://github.com/jooize" { "jooize" } " · "
            a href="https://github.com/jooize/YuioLink" { "Source on GitHub" }
        }
    };
    let scripts = html! {
        script src="/static/crypto.js" {}
        script src="/static/app.js" {}
    };
    document(body, scripts)
}

pub fn js_required_page() -> Markup {
    let body = html! {
        p.app-subtitle { "JavaScript is required to create links — encryption happens in your browser." }
        footer.app-footer { a href="/" { "Back to YuioLink" } }
    };
    document(body, html! {})
}

pub fn encrypted_redirect_page(sealed: &str) -> Markup {
    let body = html! {
        p.app-subtitle #status { "Decrypting your link…" }
        noscript { p.app-subtitle { "JavaScript is required to decrypt this link." } }
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
        p.app-subtitle { "Text" }
        pre.text-body #text-body { (text) }
        button.btn.btn-block #copy-text type="button" { "Copy" }
        footer.app-footer { a href="/" { "Back to YuioLink" } }
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
        p.app-subtitle #status { "Decrypting…" }
        noscript { p.app-subtitle { "JavaScript is required to decrypt this text." } }
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
        p.app-subtitle { (message) }
        footer.app-footer { a href="/" { "Back to YuioLink" } }
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
        p.app-subtitle { "Link preview" }

        div.result role="status" {
            div.result-label { (p.kind) }
            div.result-row { code { (p.short_url) } }
        }

        @if let Some(target) = p.target {
            p.app-subtitle { "Destination" }
            div.result-row { code { a href=(target) rel="nofollow noopener noreferrer" { (target) } } }
            a.btn.btn-block href=(p.short_url) { "Continue" }
        } @else if p.encrypted {
            p.app-subtitle { "Encrypted — the content is hidden from the server and opens in your browser with the key from the original link." }
        } @else {
            p.app-subtitle { "This is a Text link — open it to read." }
            a.btn.btn-block href=(p.short_url) { "Open" }
        }

        p.app-subtitle {
            "created " (p.created_at) " · " (p.hits) " hits · expires " (p.expires_at) " UTC"
            @if let Some(max) = p.max_uses {
                " · " ((max - p.hits).max(0)) " of " (max) " uses left"
            }
        }

        footer.app-footer { a href="/" { "Back to YuioLink" } }
    };
    document(body, html! {})
}
