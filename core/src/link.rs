//! Link-name generation, redirect-target validation, and redirect-vs-text
//! detection.

use rand::RngCore;
use rand::rngs::OsRng;
use url::Url;

use crate::words::{WORD_COUNT, words};

/// Words in a public (unlimited) name. A public link guards nothing — there is
/// no view to burn and no secrecy promised — so it gets a single wieldy word and
/// grows by one only when names run short (see [`words_for_uses`]).
pub const PUBLIC_WORDS: usize = 1;

/// Words in a limited (single-use) name. The whole value of a limited link is its
/// one un-spent view, which an enumerator could burn by guessing the name, so the
/// name must stand on its own as a secret: four words over the ~3500-word list is
/// ~47 bits, unguessable enough to protect that view for the full 7-day maximum
/// without a separate capability token.
pub const LIMITED_WORDS: usize = 4;

/// What a link carries. Both kinds share one namespace and one storage table;
/// the kind only changes how the stored `content` is resolved and rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `content` is a redirect target (a URL, or sealed ciphertext).
    Redirect,
    /// `content` is a body of text (plain, or sealed ciphertext).
    Text,
}

impl Kind {
    /// Stable lowercase identifier used on the wire and in storage.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Redirect => "redirect",
            Kind::Text => "text",
        }
    }
}

/// Number of words a fresh name needs. A name must be unguessable
/// ([`LIMITED_WORDS`]) when EITHER the link is limited — a guesser could burn its
/// single view — OR the creator asked for a private reusable link. An ordinary
/// public link has nothing to protect, so it gets one wieldy [`PUBLIC_WORDS`] word
/// (grown on collision at insert time as a capacity valve). TTL never drives name
/// length.
pub fn words_for(max_uses: Option<i64>, private: bool) -> usize {
    if private || max_uses.is_some() {
        LIMITED_WORDS
    } else {
        PUBLIC_WORDS
    }
}

/// Render words in YuioLink's alternating-case display form: the first word
/// lowercase, the next UPPERCASE, and so on (`braveOTTER`). The casing is a
/// readability aid only — lookups and uniqueness are case-insensitive.
fn alternating_case(parts: &[&str]) -> String {
    let mut out = String::new();
    for (i, word) in parts.iter().enumerate() {
        if i % 2 == 0 {
            out.push_str(&word.to_lowercase());
        } else {
            out.push_str(&word.to_uppercase());
        }
    }
    out
}

/// Pick a uniformly random word index in `[0, WORD_COUNT)` from the OS CSPRNG,
/// using rejection sampling to avoid modulo bias.
fn pick_index() -> usize {
    // 1296 < 2^16; reject the tail so every index is equally likely.
    let max_unbiased = (u16::MAX as u32 + 1) / WORD_COUNT as u32 * WORD_COUNT as u32;
    let mut buf = [0u8; 2];
    loop {
        OsRng.fill_bytes(&mut buf);
        let v = u16::from_le_bytes(buf) as u32;
        if v < max_unbiased {
            return (v % WORD_COUNT as u32) as usize;
        }
    }
}

/// Generate a random link name from `words` EFF-short words in alternating-case
/// display form. The words come from the OS CSPRNG, so names are unguessable.
pub fn generate_name(words_count: usize) -> String {
    let list = words();
    let picked: Vec<&str> = (0..words_count.max(1)).map(|_| list[pick_index()]).collect();
    alternating_case(&picked)
}

/// True if `s` already starts with an explicit `scheme:` (RFC 3986 scheme
/// characters, with no `/` before the colon so a path colon does not count).
pub fn has_scheme(s: &str) -> bool {
    match s.find(':') {
        Some(i) if i > 0 => {
            let scheme = &s[..i];
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        }
        _ => false,
    }
}

/// True if `s` is a single token that looks like a bare domain (`example.com`,
/// `sub.example.co.uk/path`) — no whitespace, a dotted host, an alphabetic TLD.
fn looks_like_domain(s: &str) -> bool {
    if s.chars().any(char::is_whitespace) {
        return false;
    }
    // The host is everything before the first path/query/fragment, minus a port.
    let host = s.split(['/', '?', '#']).next().unwrap_or(s);
    let host = host.split(':').next().unwrap_or(host);
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    // Unicode-aware so internationalized domains (e.g. `åäö.se`, `münchen.de`)
    // are recognized, not just ASCII ones. The browser / `url` crate handle the
    // IDNA punycode conversion when the link is opened.
    let tld_ok = labels
        .last()
        .is_some_and(|tld| tld.chars().count() >= 2 && tld.chars().all(char::is_alphabetic));
    let labels_ok = labels
        .iter()
        .all(|l| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || c == '-'));
    tld_ok && labels_ok
}

/// Best-effort guess of whether input is a [`Kind::Redirect`] or [`Kind::Text`].
///
/// Multi-line input is Text; a single line that has a scheme or looks like a
/// bare domain is a Redirect; anything else is Text. The UI always offers a
/// manual toggle, so this only needs to be right for the common case. Detection
/// never decides encryption — only redirect-vs-text.
pub fn detect_kind(s: &str) -> Kind {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Kind::Text;
    }
    // A newline that survives trimming means real multi-line content -> Text.
    // (Trailing blank lines after a single URL are trimmed away, so they stay a
    // Redirect.)
    if trimmed.contains('\n') {
        return Kind::Text;
    }
    if has_scheme(trimmed) || looks_like_domain(trimmed) {
        Kind::Redirect
    } else {
        Kind::Text
    }
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

/// Validate a redirect target and return its canonical serialization: it must
/// parse as a URL whose scheme is on the allowlist. The returned string is the
/// `url` crate's normalized form — crucially ASCII (an internationalized host is
/// IDNA/punycode-encoded), so it is safe to put in a `Location` header. Applied
/// to *unencrypted* redirects only — encrypted targets are opaque to the server
/// and only ever decrypted client-side by the key holder.
pub fn validate_redirect(uri: &str, allowed_schemes: &[&str]) -> Result<String, UriError> {
    let parsed = Url::parse(uri).map_err(|_| UriError::Invalid)?;
    let scheme = parsed.scheme();
    if allowed_schemes.iter().any(|s| s.eq_ignore_ascii_case(scheme)) {
        Ok(parsed.into())
    } else {
        Err(UriError::SchemeNotAllowed(scheme.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn one_word_name_is_a_lowercase_list_member() {
        let list = words();
        for _ in 0..200 {
            let name = generate_name(1);
            assert!(list.contains(&name.as_str()), "{name:?} not in word list");
        }
    }

    #[test]
    fn alternating_case_lowercases_then_uppercases() {
        assert_eq!(alternating_case(&["brave", "otter"]), "braveOTTER");
        assert_eq!(alternating_case(&["one", "two", "three"]), "oneTWOthree");
        assert_eq!(alternating_case(&["solo"]), "solo");
    }

    #[test]
    fn two_word_name_has_both_cases() {
        for _ in 0..50 {
            let name = generate_name(2);
            assert!(name.chars().any(|c| c.is_ascii_lowercase()));
            assert!(name.chars().any(|c| c.is_ascii_uppercase()));
        }
    }

    #[test]
    fn names_are_distinct() {
        // Two-word names: 3518^2 ≈ 12.4M, so 1000 draws collide vanishingly rarely.
        let set: HashSet<String> = (0..1000).map(|_| generate_name(2)).collect();
        assert!(set.len() > 990);
    }

    #[test]
    fn lookup_is_case_insensitive_by_design() {
        // Display casing is cosmetic; the server compares names with NOCASE.
        assert!("braveOTTER".eq_ignore_ascii_case("BRAVEotter"));
        assert!("braveOTTER".eq_ignore_ascii_case("braveotter"));
    }

    #[test]
    fn words_for_keys_off_privacy_and_use_type() {
        assert_eq!(words_for(None, false), PUBLIC_WORDS); // public: wieldy 1-word
        assert_eq!(words_for(None, true), LIMITED_WORDS); // private reusable: unguessable
        assert_eq!(words_for(Some(1), false), LIMITED_WORDS); // single-use: always unguessable
        assert_eq!(words_for(Some(5), true), LIMITED_WORDS);
    }

    #[test]
    fn detect_kind_classifies_common_input() {
        assert_eq!(detect_kind("https://example.com/watch?v=x"), Kind::Redirect);
        assert_eq!(detect_kind("mailto:a@b.com"), Kind::Redirect);
        assert_eq!(detect_kind("example.com"), Kind::Redirect); // bare domain
        assert_eq!(detect_kind("sub.example.co.uk/path"), Kind::Redirect);
        assert_eq!(detect_kind("åäö.se"), Kind::Redirect); // IDN bare domain
        assert_eq!(detect_kind("münchen.de/weg"), Kind::Redirect); // IDN + path
        assert_eq!(detect_kind("hello"), Kind::Text); // single word
        assert_eq!(detect_kind("just some prose here"), Kind::Text); // spaces
        assert_eq!(detect_kind("line one\nline two"), Kind::Text); // multi-line
        assert_eq!(detect_kind(""), Kind::Text);
    }

    #[test]
    fn detect_kind_ignores_trailing_blank_lines() {
        // A single URL with trailing newlines is still a Redirect.
        assert_eq!(detect_kind("https://example.com\n\n\n"), Kind::Redirect);
        assert_eq!(detect_kind("  https://example.com  "), Kind::Redirect);
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

    #[test]
    fn idn_redirect_canonicalizes_to_ascii() {
        // An internationalized host must come back ASCII (punycode) so it is a
        // valid Location header value and never panics the redirect handler.
        let canonical = validate_redirect("https://åäö.se", DEFAULT_ALLOWED_SCHEMES).unwrap();
        assert!(canonical.is_ascii(), "must be ASCII: {canonical:?}");
        assert!(canonical.contains("xn--"), "host should be punycode: {canonical:?}");
    }
}
