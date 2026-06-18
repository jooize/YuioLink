//! Link-name generation and redirect-target validation.

use rand::RngCore;
use rand::rngs::OsRng;
use url::Url;

/// Unambiguous alphabet for human-friendly link names — excludes the visually
/// confusable `0/O`, `1/l/I`. 55 symbols.
const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Default link-name length. 55^7 ≈ 1.5e12 — ample, and unguessable because the
/// bytes come from the OS CSPRNG (unlike the old `math/rand`-seeded scheme).
pub const DEFAULT_NAME_LEN: usize = 7;

/// Generate a random link name using the OS CSPRNG with rejection sampling to
/// avoid modulo bias.
pub fn generate_name(len: usize) -> String {
    let n = ALPHABET.len();
    // Largest multiple of `n` that fits in a byte; sampled bytes >= this are
    // rejected so every symbol is equally likely.
    let max_unbiased = (256 / n * n) as u16;

    let mut out = String::with_capacity(len);
    let mut buf = [0u8; 1];
    while out.len() < len {
        OsRng.fill_bytes(&mut buf);
        let b = buf[0] as u16;
        if b < max_unbiased {
            out.push(ALPHABET[(b as usize) % n] as char);
        }
    }
    out
}

/// Default allowlist of URL schemes permitted for unencrypted redirects.
///
/// Notably excludes `javascript:`, `data:`, and `vbscript:` — storing those and
/// later reflecting them would be an XSS vector.
pub const DEFAULT_ALLOWED_SCHEMES: &[&str] = &[
    "http", "https", "mailto", "tel", "sms", "ftp", "ftps", "magnet", "spotify", "xmpp", "irc",
    "ircs", "matrix",
];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UriError {
    #[error("not a valid URL")]
    Invalid,
    #[error("scheme '{0}' is not allowed")]
    SchemeNotAllowed(String),
}

/// Validate a redirect target: it must parse as a URL whose scheme is on the
/// allowlist. Applied to *unencrypted* redirects only — encrypted targets are
/// opaque to the server and only ever decrypted client-side by the key holder.
pub fn validate_redirect(uri: &str, allowed_schemes: &[&str]) -> Result<(), UriError> {
    let parsed = Url::parse(uri).map_err(|_| UriError::Invalid)?;
    let scheme = parsed.scheme();
    if allowed_schemes.iter().any(|s| s.eq_ignore_ascii_case(scheme)) {
        Ok(())
    } else {
        Err(UriError::SchemeNotAllowed(scheme.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn name_has_requested_length_and_alphabet() {
        for _ in 0..100 {
            let name = generate_name(DEFAULT_NAME_LEN);
            assert_eq!(name.len(), DEFAULT_NAME_LEN);
            assert!(name.bytes().all(|b| ALPHABET.contains(&b)));
        }
    }

    #[test]
    fn names_are_distinct() {
        let set: HashSet<String> = (0..1000).map(|_| generate_name(DEFAULT_NAME_LEN)).collect();
        // Collisions at this length/count would be astronomically unlikely.
        assert_eq!(set.len(), 1000);
    }

    #[test]
    fn accepts_allowed_schemes() {
        assert!(validate_redirect("https://example.com", DEFAULT_ALLOWED_SCHEMES).is_ok());
        assert!(validate_redirect("mailto:a@b.com", DEFAULT_ALLOWED_SCHEMES).is_ok());
        assert!(validate_redirect("HTTPS://EXAMPLE.COM", DEFAULT_ALLOWED_SCHEMES).is_ok());
    }

    #[test]
    fn rejects_dangerous_schemes() {
        assert_eq!(
            validate_redirect("javascript:alert(1)", DEFAULT_ALLOWED_SCHEMES),
            Err(UriError::SchemeNotAllowed("javascript".into()))
        );
        assert!(matches!(
            validate_redirect("data:text/html,<script>", DEFAULT_ALLOWED_SCHEMES),
            Err(UriError::SchemeNotAllowed(_))
        ));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(
            validate_redirect("not a url", DEFAULT_ALLOWED_SCHEMES),
            Err(UriError::Invalid)
        );
    }
}
