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
//! `trusted-types 'none'` then forbids minting a policy to get around that. The
//! client builds every node it needs (`createElement`, `createElementNS` for the
//! inline icons), so it never wanted either.

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

/// A fresh 128-bit nonce, base64 as the CSP grammar wants it.
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
         require-trusted-types-for 'script'; \
         trusted-types 'none'"
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
    // but the old header costs one line and covers whatever does not honour it.
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
    // Anything that opened us lands in its own browsing-context group, so a
    // window handle to this page cannot be used to poke at it.
    h.insert(COOP, HeaderValue::from_static("same-origin"));
    if !h.contains_key(&CORP) {
        h.insert(CORP, HeaderValue::from_static("same-origin"));
    }
    res
}
