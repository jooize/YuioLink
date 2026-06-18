//! Paste content types.
//!
//! A strict allowlist — the server must never echo an arbitrary client-supplied
//! `content_type` into markup or a highlighter mode (an injection vector in the
//! old implementation). Unknown values map to [`ContentType::PlainText`].

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentType {
    Markdown,
    #[default]
    PlainText,
    CSharp,
    JavaScript,
}

impl ContentType {
    /// Canonical lowercase identifier (stable wire/storage value).
    pub fn as_str(self) -> &'static str {
        match self {
            ContentType::Markdown => "markdown",
            ContentType::PlainText => "plaintext",
            ContentType::CSharp => "csharp",
            ContentType::JavaScript => "javascript",
        }
    }

    /// Human-readable label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            ContentType::Markdown => "Markdown",
            ContentType::PlainText => "Plain Text",
            ContentType::CSharp => "C#",
            ContentType::JavaScript => "JavaScript",
        }
    }

    /// All selectable content types, in UI order.
    pub fn all() -> &'static [ContentType] {
        &[
            ContentType::Markdown,
            ContentType::PlainText,
            ContentType::CSharp,
            ContentType::JavaScript,
        ]
    }
}

impl FromStr for ContentType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "markdown" | "md" => Ok(ContentType::Markdown),
            "plaintext" | "text" | "plain" => Ok(ContentType::PlainText),
            "csharp" | "c#" | "cs" => Ok(ContentType::CSharp),
            "javascript" | "js" => Ok(ContentType::JavaScript),
            _ => Err(()),
        }
    }
}

impl ContentType {
    /// Parse, falling back to [`ContentType::PlainText`] for unknown input —
    /// the safe default for untrusted `content_type` values.
    pub fn parse_or_default(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_aliases_parse() {
        assert_eq!("md".parse::<ContentType>().unwrap(), ContentType::Markdown);
        assert_eq!("JS".parse::<ContentType>().unwrap(), ContentType::JavaScript);
        assert_eq!("c#".parse::<ContentType>().unwrap(), ContentType::CSharp);
    }

    #[test]
    fn unknown_falls_back_to_plaintext() {
        assert_eq!(ContentType::parse_or_default("ruby"), ContentType::PlainText);
        assert_eq!(ContentType::parse_or_default("<script>"), ContentType::PlainText);
    }

    #[test]
    fn as_str_is_stable() {
        for ct in ContentType::all() {
            assert_eq!(ContentType::parse_or_default(ct.as_str()), *ct);
        }
    }
}
