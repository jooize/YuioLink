//! Decompose a stored redirect URL for trustworthy display on the interstitial.
//!
//! Two jobs: split the URL into parts the view can colour (scheme / delimiters /
//! subdomain / **registrable domain** / path / query), and judge whether the
//! host is a deceptive internationalized lookalike.
//!
//! IDN policy (UTS #46 + UTS #39): decode punycode to Unicode for display, but
//! only when every label is *single-script*. A single-script label — Latin (incl.
//! diacritics, e.g. `münchen`), all-Cyrillic, all-Greek, CJK — is a legitimate
//! international domain and is shown decoded with no warning. A label that mixes
//! scripts (the classic `аpple.com`, Cyrillic `а` + Latin `pple`) is a homograph
//! attack: we show the raw `xn--…` punycode instead and flag it. Note this
//! catches mixed-script labels, not whole-script confusables (e.g. an all-Cyrillic
//! string shaped like Latin); those pass, consistent with "all-Cyrillic = legit".

use idna::domain_to_unicode;
use unicode_security::MixedScript;

/// A redirect URL split into displayable, individually styleable parts.
pub struct UrlView {
    pub scheme: String,
    /// The host, decomposed and IDN-classified. `None` for hostless schemes
    /// (`mailto:`, `tel:`, `magnet:`, …), where [`Self::opaque`] holds the rest.
    pub host: Option<HostView>,
    /// Path including its leading `/` (empty for hostless schemes).
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
    /// Everything after `scheme:` for a hostless URL (e.g. `a@b.com` for mailto).
    pub opaque: Option<String>,
}

/// A host split at the registrable-domain boundary, in display form.
pub struct HostView {
    /// Subdomain labels with no trailing dot (`docs`, `a.b`), or empty.
    pub subdomain: String,
    /// The registrable domain (eTLD+1), e.g. `example.com` — the part to trust.
    pub registrable: String,
    /// Set when the host is a deceptive lookalike; carries both forms to warn.
    pub warning: Option<IdnWarning>,
}

/// The two faces of a deceptive host, for the red warning panel.
pub struct IdnWarning {
    /// What the punycode decodes to — the misleading Unicode (`аpple.com`).
    pub displays_as: String,
    /// The unambiguous real address shown instead (`xn--pple-43d.com`).
    pub real: String,
}

impl UrlView {
    /// True when the host is a deceptive internationalized lookalike.
    pub fn is_deceptive(&self) -> bool {
        self.host.as_ref().is_some_and(|h| h.warning.is_some())
    }

    /// The single line shown for a limited link's domain-only preview, and the
    /// domain used in share-card / OG copy: the registrable domain for an
    /// HTTP(S) host, else the scheme as a stand-in.
    pub fn card_domain(&self) -> String {
        match &self.host {
            Some(h) => h.registrable.clone(),
            None => self.scheme.clone(),
        }
    }
}

/// Parse a canonical (already validated, ASCII) redirect URL into displayable
/// parts. Falls back to a bare opaque view if parsing somehow fails — the URL was
/// validated at creation, so this is just defensive.
pub fn parse(url: &str) -> UrlView {
    match url::Url::parse(url) {
        Ok(u) => from_url(&u, url),
        Err(_) => UrlView {
            scheme: String::new(),
            host: None,
            path: String::new(),
            query: None,
            fragment: None,
            opaque: Some(url.to_string()),
        },
    }
}

fn from_url(u: &url::Url, raw: &str) -> UrlView {
    let scheme = u.scheme().to_string();
    match u.host_str() {
        Some(host) => UrlView {
            scheme,
            host: Some(build_host(host)),
            path: u.path().to_string(),
            query: u.query().map(str::to_string),
            fragment: u.fragment().map(str::to_string),
            opaque: None,
        },
        // Hostless scheme (mailto:, tel:, magnet:, …): keep the remainder verbatim.
        None => UrlView {
            scheme,
            host: None,
            path: String::new(),
            query: None,
            fragment: None,
            opaque: raw.split_once(':').map(|(_, rest)| rest.to_string()),
        },
    }
}

/// Split an ASCII host at the registrable boundary (via the Public Suffix List)
/// and classify it for safe display.
fn build_host(host_ascii: &str) -> HostView {
    let (decoded, decode_result) = domain_to_unicode(host_ascii);
    let deceptive = decode_result.is_err() || has_mixed_script_label(&decoded);

    // The PSL works on the ASCII/punycode form; fall back to the whole host when
    // it has no recognized public suffix (IPs, intranet names, …).
    let registrable_ascii = psl::domain_str(host_ascii).unwrap_or(host_ascii);
    let subdomain_ascii = host_ascii
        .strip_suffix(registrable_ascii)
        .map(|s| s.trim_end_matches('.'))
        .unwrap_or("");

    if deceptive {
        // Show the raw punycode; reveal both forms in the warning.
        HostView {
            subdomain: subdomain_ascii.to_string(),
            registrable: registrable_ascii.to_string(),
            warning: Some(IdnWarning {
                displays_as: decoded,
                real: host_ascii.to_string(),
            }),
        }
    } else {
        // Safe: show the decoded Unicode for each part.
        HostView {
            subdomain: decode_part(subdomain_ascii),
            registrable: decode_part(registrable_ascii),
            warning: None,
        }
    }
}

/// Decode a host fragment (one or more labels, no leading/trailing dot) from
/// punycode to Unicode. Empty in, empty out.
fn decode_part(part: &str) -> String {
    if part.is_empty() {
        String::new()
    } else {
        domain_to_unicode(part).0
    }
}

/// True if any label of the decoded host is not single-script (a homograph risk).
/// ASCII labels (`com`, `de`) are trivially single-script, so a non-Latin SLD
/// under an ASCII TLD (`δοκιμή.gr`) is judged per label and stays legitimate.
fn has_mixed_script_label(decoded_host: &str) -> bool {
    decoded_host
        .split('.')
        .filter(|label| !label.is_empty())
        .any(|label| !label.is_single_script())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host_of(url: &str) -> HostView {
        parse(url).host.expect("expected a host")
    }

    /// Punycode-encode a Unicode host so tests can express the readable form.
    fn to_ascii(host: &str) -> String {
        idna::domain_to_ascii(host).expect("encodable host")
    }

    #[test]
    fn plain_ascii_url_decomposes() {
        let v = parse("https://example.com/blog/2026/the-post?ref=share");
        assert_eq!(v.scheme, "https");
        let h = v.host.unwrap();
        assert_eq!(h.subdomain, "");
        assert_eq!(h.registrable, "example.com");
        assert!(h.warning.is_none());
        assert_eq!(v.path, "/blog/2026/the-post");
        assert_eq!(v.query.as_deref(), Some("ref=share"));
    }

    #[test]
    fn subdomain_splits_from_registrable() {
        let h = host_of("https://docs.acme.co/q3");
        assert_eq!(h.subdomain, "docs");
        assert_eq!(h.registrable, "acme.co");
    }

    #[test]
    fn multi_label_subdomain_under_compound_suffix() {
        let h = host_of("https://a.b.example.co.uk/");
        assert_eq!(h.registrable, "example.co.uk");
        assert_eq!(h.subdomain, "a.b");
    }

    #[test]
    fn ascii_host_has_no_warning() {
        assert!(host_of("https://example.com/").warning.is_none());
    }

    #[test]
    fn single_script_latin_idn_is_shown_decoded() {
        // münchen.de — Latin with a diacritic: legitimate, decoded, no warning.
        let url = format!("https://{}/tickets", to_ascii("münchen.de"));
        let h = host_of(&url);
        assert_eq!(h.registrable, "münchen.de");
        assert!(h.warning.is_none());
    }

    #[test]
    fn single_script_non_latin_idn_is_legit() {
        // All-Greek SLD under an ASCII TLD must not be flagged (per-label check).
        let url = format!("https://{}/", to_ascii("δοκιμή.gr"));
        let h = host_of(&url);
        assert!(h.warning.is_none(), "all-Greek label should be legit");
        assert_eq!(h.registrable, "δοκιμή.gr");
    }

    #[test]
    fn all_cyrillic_idn_is_legit() {
        // All-Cyrillic label + Cyrillic TLD (.рф): legit, no warning.
        let url = format!("https://{}/", to_ascii("почта.рф"));
        let h = host_of(&url);
        assert!(h.warning.is_none(), "all-Cyrillic should be legit");
    }

    #[test]
    fn mixed_script_lookalike_is_flagged() {
        // аpple.com with a Cyrillic 'а' — a homograph attack.
        let punycode = to_ascii("аpple.com");
        let url = format!("https://{punycode}/login");
        let h = host_of(&url);
        let w = h.warning.expect("mixed-script host must warn");
        assert_eq!(w.displays_as, "аpple.com");
        assert_eq!(w.real, punycode);
        // The URL shows the punycode, not the deceptive Unicode.
        assert_eq!(h.registrable, punycode);
    }

    #[test]
    fn hostless_scheme_keeps_opaque_remainder() {
        let v = parse("mailto:hi@example.com");
        assert_eq!(v.scheme, "mailto");
        assert!(v.host.is_none());
        assert_eq!(v.opaque.as_deref(), Some("hi@example.com"));
        assert_eq!(v.card_domain(), "mailto");
    }
}
