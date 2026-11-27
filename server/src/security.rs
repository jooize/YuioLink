//! Response hardening: the security headers every response carries, and the
//! per-request nonce the Content-Security-Policy is built around.
//!
//! The policy is `default-src 'none'` with each kind of subresource added back
//! explicitly. That is affordable here because the site loads nothing from
//! anywhere else: one stylesheet, one script, both same-origin, no fonts, no
//! images, no analytics.
//!
//! Scripts are allowed by nonce rather than by origin. A nonce is minted per
//! request, [`nonce`] hands it to the views so both the inline `js` marker and
//! the `<script src>` can carry it, and `'strict-dynamic'` extends the trust to
//! anything those scripts load themselves. An origin allowlist (`script-src
//! 'self'`) would be one served file away from being a bypass — this site takes
//! text from strangers and hands it back — so it is not used at all.
//!
//! `require-trusted-types-for 'script'` closes the DOM-XSS sinks the policy
//! cannot see: with it, `innerHTML` and friends reject plain strings outright.
//! The client builds every node it needs (`createElement`, `createElementNS`
//! for the inline icons), so it never wanted them.
//!
//! There is deliberately no `trusted-types` directive. `trusted-types 'none'`
//! (or any list of policy names) would forbid creating a policy, and Apple's
//! LinkPresentation, which builds the rich link Messages shows, runs its own
//! extractor script in the page and that script creates one. Under the
//! restriction it throws on any page with a description, and Messages shows a
//! bare link: every link preview in iMessage was lost that way from August to
//! November 2026. What the directive added was a refusal for code already
//! running here, and the only such code is ours, which creates no policy; the
//! test below holds it to that, so the sinks stay closed without it.

use std::sync::Arc;

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rand::RngCore;

tokio::task_local! {
    /// The nonce minted for the request currently being served. Set for the whole
    /// of `next.run`, so a view reaches it wherever it renders — including from
    /// an error's `IntoResponse`, which has no access to the request.
    static NONCE: Arc<str>;
}

/// A fresh 128-bit nonce.
///
/// The CSP grammar takes either base64 alphabet and up to two `=`, and a browser
/// compares the nonce as an opaque string — it never decodes it — so nothing
/// about the policy cares which. This is the standard alphabet, `+/` and padding
/// included; `token.rs` uses the URL-safe one because its output goes in a URL.
fn mint() -> Arc<str> {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    Arc::from(B64.encode(bytes))
}

/// The nonce every `<script>` in this response must carry.
///
/// Outside a request — the view unit tests render pages directly — this is a
/// fresh nonce that matches no header. It fails closed: such a page would run no
/// script at all rather than run script under a policy that permits anything.
pub fn nonce() -> Arc<str> {
    NONCE.try_with(Arc::clone).unwrap_or_else(|_| mint())
}

/// The policy, with this request's script nonce spliced in.
fn csp(nonce: &str) -> String {
    format!(
        "default-src 'none'; \
         script-src 'nonce-{nonce}' 'strict-dynamic'; \
         style-src 'self'; \
         img-src 'self'; \
         connect-src 'self'; \
         form-action 'self'; \
         base-uri 'none'; \
         frame-ancestors 'none'; \
         object-src 'none'; \
         require-trusted-types-for 'script'"
    )
}

const COOP: HeaderName = HeaderName::from_static("cross-origin-opener-policy");
const CORP: HeaderName = HeaderName::from_static("cross-origin-resource-policy");

/// Mint the request's nonce, run the request under it, then stamp the headers
/// onto whatever comes back.
///
/// `Cross-Origin-Resource-Policy` is only filled in where a handler has not set
/// its own: the share card is meant to be fetched by other origins, and says so
/// for itself.
pub async fn headers(req: Request, next: Next) -> Response {
    let nonce = mint();
    let csp = csp(&nonce);
    let mut res = NONCE.scope(nonce, next.run(req)).await;

    let h = res.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp).expect("the policy is ASCII"),
    );
    // Belt to the policy's braces: `frame-ancestors` is the modern half of this,
    // but the old header costs one line and covers whatever does not honor it.
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // A destination learns nothing about where its visitor came from — not the
    // link's name, not that a shortener was involved.
    //
    // Current browsers default to `strict-origin-when-cross-origin` and already
    // keep the path off a cross-origin navigation, so the name does not leak on
    // its own; older ones (before Chrome 85 / Firefox 87) sent the whole URL, and
    // the name is the one thing a secret link cannot afford to spend. Stating the
    // policy makes that a property of the site rather than of the visitor's
    // browser. The alternative worth having is `origin`, which would let a site
    // owner see YuioLink in their referrer report — attribution for a third party,
    // paid for by the visitor, which is not the trade this site makes elsewhere.
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    // A page carries a nonce, so it must never be stored anywhere a second
    // visitor could be handed the first one's — a shared cache in front of this
    // would turn the strictest part of the policy into a constant to read off.
    // `no-store` says so to every cache in the path, including whatever CDN ends
    // up there later, and it costs a page this small nothing. Content type, not
    // route: it is exactly the responses that carry a nonce that must not be
    // stored, and the versioned static assets keep their year.
    if h.get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"text/html"))
    {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    // Anything that opened us lands in its own browsing-context group, so a
    // window handle to this page cannot be used to poke at it.
    h.insert(COOP, HeaderValue::from_static("same-origin"));
    if !h.contains_key(&CORP) {
        h.insert(CORP, HeaderValue::from_static("same-origin"));
    }
    res
}

#[cfg(test)]
mod tests {
    use super::csp;

    /// Every script the site serves. A new file under static/ that is not
    /// listed here would escape the sink check below, so the list is checked
    /// against the directory too.
    const SCRIPTS: [(&str, &str); 3] = [
        ("app.js", include_str!("../static/app.js")),
        ("preview.js", include_str!("../static/preview.js")),
        ("text.js", include_str!("../static/text.js")),
    ];

    /// `source` with its comments removed, so a comment that names a sink (to
    /// explain why it is not used) does not trip the check.
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

    /// The browser closes the sinks; this keeps the guarantee the policy no
    /// longer states for itself (see the module comment): no script of ours
    /// creates a Trusted Types policy, and none writes markup into the page.
    #[test]
    fn no_script_creates_a_policy_or_writes_markup() {
        for (name, source) in SCRIPTS {
            let code = code_only(source);
            for forbidden in [
                "createPolicy",
                "innerHTML",
                "outerHTML",
                "insertAdjacentHTML",
                "document.write",
                "srcdoc",
                "DOMParser",
                "createContextualFragment",
            ] {
                assert!(
                    !code.contains(forbidden),
                    "{name} contains `{forbidden}`; the site creates no Trusted Types \
                     policy, so it would throw (and a policy would reopen the sink)"
                );
            }
        }
    }

    #[test]
    fn every_served_script_is_checked() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .expect("static/ is readable")
            .map(|e| e.expect("static/ entry").file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".js"))
            .collect();
        on_disk.sort();
        let listed: Vec<&str> = SCRIPTS.iter().map(|(n, _)| *n).collect();
        assert_eq!(on_disk, listed, "static/*.js and SCRIPTS disagree");
    }

    /// Sinks stay closed, and policy creation stays open: a `trusted-types`
    /// directive breaks iMessage link previews (see the module comment).
    #[test]
    fn trusted_types_are_required_but_policy_names_are_not_restricted() {
        let policy = csp("n");
        assert!(policy.contains("require-trusted-types-for 'script'"), "{policy}");
        let directives: Vec<&str> = policy.split(';').map(str::trim).collect();
        assert!(
            !directives.iter().any(|d| d.starts_with("trusted-types")),
            "{policy}"
        );
    }
}
