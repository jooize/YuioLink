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
        p.app-subtitle { "Redirect any link, and secure it with client-side encryption." }
        noscript { p.app-subtitle { "JavaScript is required to create and decrypt links." } }

        div.result #link-panel role="status" hidden {
            div.result-label { (PreEscaped(ICON_CHECK)) " Your link is ready" }
            div.result-row { code #link-element {} }
        }

        form #create-form method="post" action="/" {
            div.form-group {
                input #uri.form-control type="text" name="uri" inputmode="url"
                    autocomplete="off" autocapitalize="off" spellcheck="false"
                    placeholder="https://www.youtube.com/watch?v=dQw4w9WgXcQ" autofocus;
            }

            label.switch-row for="encrypt" {
                span.switch-label {
                    span.switch-title { "Encrypt Link" }
                    span.switch-desc { "End-to-end, in your browser. The key never reaches the server." }
                }
                span.switch {
                    input #encrypt type="checkbox" name="encrypt" checked;
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

pub fn error_page(code: u16, message: &str) -> Markup {
    let body = html! {
        p.error-code { (code) }
        p.app-subtitle { (message) }
        footer.app-footer { a href="/" { "Back to YuioLink" } }
    };
    document(body, html! {})
}
