//! Reveal tokens — stateless, signed capabilities for re-rendering a revealed
//! link without re-consuming it.
//!
//! When a limited link is consumed at `POST /:name/reveal`, the use is spent
//! immediately and the response 303-redirects to `GET /:name/revealed`, carrying
//! the token in a short-lived, path-scoped `yl_reveal` cookie. That GET must be
//! safe to refresh and back-button, so it cannot consume again; the token is what
//! authorises the re-render. It carries the link name and an expiry, signed with
//! the server secret, so it can be verified with no stored state.
//!
//! Format: `base64url(name|exp) "." base64url(HMAC_SHA256(secret, "name|exp"))`,
//! where `exp` is a Unix timestamp (seconds). The MAC is taken over the textual
//! payload, so tampering with either field invalidates the token.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// How long a freshly minted reveal token stays valid (10 minutes) — long enough
/// to read and click through, short enough that a leaked token is not a lasting
/// capability. The link's own expiry still bounds everything.
pub const TTL_SECS: i64 = 10 * 60;

/// Sign `name`+`exp` and return the URL-safe token string. `exp` is the absolute
/// Unix expiry (seconds); the caller computes it as `now + TTL_SECS`.
pub fn mint(secret: &[u8], name: &str, exp_unix: i64) -> String {
    let payload = format!("{name}|{exp_unix}");
    let sig = sign(secret, payload.as_bytes());
    format!("{}.{}", B64.encode(payload.as_bytes()), B64.encode(sig))
}

/// Verify a token against `secret` at time `now_unix`. Returns the link name it
/// authorises when the signature is valid and the token has not expired;
/// otherwise `None` (malformed, tampered, or expired). Signature comparison is
/// constant-time (via `Mac::verify_slice`).
pub fn verify(secret: &[u8], token: &str, now_unix: i64) -> Option<String> {
    let (payload_b64, sig_b64) = token.split_once('.')?;
    let payload = B64.decode(payload_b64).ok()?;
    let sig = B64.decode(sig_b64).ok()?;

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&payload);
    mac.verify_slice(&sig).ok()?;

    // Signature is authentic; now parse the trusted payload.
    let payload = String::from_utf8(payload).ok()?;
    let (name, exp) = payload.rsplit_once('|')?;
    let exp: i64 = exp.parse().ok()?;
    if exp < now_unix {
        return None;
    }
    Some(name.to_string())
}

fn sign(secret: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-secret-do-not-use";

    #[test]
    fn valid_token_round_trips() {
        let t = mint(SECRET, "braveOTTER", 1_000);
        assert_eq!(verify(SECRET, &t, 500).as_deref(), Some("braveOTTER"));
    }

    #[test]
    fn expired_token_is_rejected() {
        let t = mint(SECRET, "braveOTTER", 1_000);
        // now == exp is still valid; strictly past it is not.
        assert!(verify(SECRET, &t, 1_000).is_some());
        assert!(verify(SECRET, &t, 1_001).is_none());
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let t = mint(SECRET, "braveOTTER", 1_000);
        assert!(verify(b"other-secret", &t, 500).is_none());
    }

    #[test]
    fn tampered_payload_is_rejected() {
        // Forge a token claiming a different name but keep the original signature.
        let t = mint(SECRET, "braveOTTER", 1_000);
        let sig = t.split_once('.').unwrap().1;
        let forged = format!("{}.{}", B64.encode(b"evilNAME|1000"), sig);
        assert!(verify(SECRET, &forged, 500).is_none());
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(verify(SECRET, "not-a-token", 0).is_none());
        assert!(verify(SECRET, "", 0).is_none());
        assert!(verify(SECRET, "a.b.c", 0).is_none());
    }

    #[test]
    fn name_with_unusual_chars_round_trips() {
        // Defensive: the payload is split on the LAST '|', so a name is recovered
        // intact even if it somehow contained one.
        let t = mint(SECRET, "od|d", 1_000);
        assert_eq!(verify(SECRET, &t, 500).as_deref(), Some("od|d"));
    }
}
