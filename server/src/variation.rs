//! Registered variation sequences: a base character followed by a variation
//! selector that Unicode lists as meaningful. Such a selector picks a form of
//! the character before it (❤ U+2764 + VS16 asks for the emoji style; 葛 +
//! U+E0100 asks for one registered glyph of that kanji), so a display shows
//! the pair as the character it draws, not as a hidden character. Every other
//! selector, including any after the first, draws nothing and stays marked.
//!
//! The tables are Unicode's own files, embedded verbatim and pinned by hash
//! (see [`SOURCES`]): StandardizedVariants.txt and
//! emoji-variation-sequences.txt from the Unicode Character Database, and
//! IVD_Sequences.txt from the Ideographic Variation Database. Licensed under
//! Unicode License v3, server/assets/unicode/LICENSE.txt.

use std::sync::OnceLock;

const STANDARDIZED: &str = include_str!("../assets/unicode/StandardizedVariants.txt");
const EMOJI: &str = include_str!("../assets/unicode/emoji-variation-sequences.txt");
const IDEOGRAPHIC: &str = include_str!("../assets/unicode/IVD_Sequences.txt");

/// Where each embedded file came from, and the SHA-256 it must have. A test
/// holds the embedded bytes to these, so an update is a deliberate change to
/// both the file and its pin.
#[cfg_attr(not(test), allow(dead_code))]
pub const SOURCES: [(&str, &str, &str); 3] = [
    (
        "StandardizedVariants.txt",
        "https://www.unicode.org/Public/18.0.0/ucd/StandardizedVariants.txt",
        "c7ae634a7e2bb0932258548a1e81df984fbb38e30cecf4269391d6a4f94581ac",
    ),
    (
        "emoji-variation-sequences.txt",
        "https://www.unicode.org/Public/18.0.0/ucd/emoji/emoji-variation-sequences.txt",
        "ff1707564aa1f1b2fcf4ec92d609d4cb26940bc0d4dcf07f5328ad6879e84da3",
    ),
    (
        "IVD_Sequences.txt",
        "https://www.unicode.org/ivd/data/2026-08-03/IVD_Sequences.txt",
        "2b466659c2bfde1c52e60bd9fc883ab14dfa5c583df024b80352420265d2cd87",
    ),
];

/// Which list registers a sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Registry {
    /// StandardizedVariants.txt: math and letter forms, CJK compatibility
    /// ideographs, Mongolian and other script variants.
    Standardized,
    /// emoji-variation-sequences.txt: text style (VS15) or emoji style (VS16).
    Emoji,
    /// The Ideographic Variation Database: registered glyphs of a Han
    /// character, in a named collection.
    Ideographic,
}

/// What a registered sequence is, as its list describes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variant {
    pub registry: Registry,
    /// The list's own words: "emoji style", "short diagonal stroke form",
    /// "CJK COMPATIBILITY IDEOGRAPH-FA10", or an IVD collection name such as
    /// "Adobe-Japan1" (the first one listed, when several share a sequence).
    pub description: &'static str,
}

/// True for every variation selector: VS1-16, VS17-256, and the Mongolian
/// free variation selectors.
pub fn is_selector(c: char) -> bool {
    matches!(c,
        '\u{fe00}'..='\u{fe0f}'
            | '\u{e0100}'..='\u{e01ef}'
            | '\u{180b}'..='\u{180d}'
            | '\u{180f}')
}

/// True when `c` is a variation selector completing a registered sequence
/// with `prev`, the character just before it. A selector after a selector,
/// or with nothing before it, never does.
pub fn completes(prev: Option<char>, c: char) -> bool {
    is_selector(c) && prev.is_some_and(|p| registered(p, c).is_some())
}

/// The registered sequence `base` + `selector` forms, if any.
pub fn registered(base: char, selector: char) -> Option<Variant> {
    let table = table();
    let key = (base as u32, selector as u32);
    table
        .binary_search_by_key(&key, |e| (e.0, e.1))
        .ok()
        .map(|i| table[i].2)
}

/// Every registered sequence, sorted by (base, selector), one entry each.
/// Parsed once, on first use, from the embedded files.
fn table() -> &'static [(u32, u32, Variant)] {
    static TABLE: OnceLock<Vec<(u32, u32, Variant)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut out = Vec::new();
        for (text, registry) in [
            (STANDARDIZED, Registry::Standardized),
            (EMOJI, Registry::Emoji),
            (IDEOGRAPHIC, Registry::Ideographic),
        ] {
            for line in text.lines() {
                if let Some(entry) = parse_line(line, registry) {
                    out.push(entry);
                }
            }
        }
        // Stable, so the first collection listed for a shared IVD sequence
        // is the one kept.
        out.sort_by_key(|e| (e.0, e.1));
        out.dedup_by_key(|e| (e.0, e.1));
        out
    })
}

/// One data line of any of the three files: `BASE SELECTOR; description; ...`.
/// Comment and blank lines, and anything malformed, give `None`.
fn parse_line(line: &'static str, registry: Registry) -> Option<(u32, u32, Variant)> {
    let line = line.split('#').next()?.trim();
    if line.is_empty() {
        return None;
    }
    let mut fields = line.split(';');
    let mut seq = fields.next()?.split_whitespace();
    let base = u32::from_str_radix(seq.next()?, 16).ok()?;
    let selector = u32::from_str_radix(seq.next()?, 16).ok()?;
    if seq.next().is_some() {
        return None;
    }
    let description = fields.next()?.trim();
    Some((
        base,
        selector,
        Variant {
            registry,
            description,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn the_embedded_files_match_their_pins() {
        for ((name, _, pin), text) in SOURCES.iter().zip([STANDARDIZED, EMOJI, IDEOGRAPHIC]) {
            let got: String = Sha256::digest(text.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(&got, pin, "{name} differs from its pinned hash");
        }
    }

    #[test]
    fn each_list_is_read() {
        let emoji = registered('\u{2764}', '\u{fe0f}').expect("heart + VS16");
        assert_eq!(emoji.registry, Registry::Emoji);
        assert_eq!(emoji.description, "emoji style");
        assert_eq!(
            registered('\u{2764}', '\u{fe0e}').map(|v| v.description),
            Some("text style")
        );

        let digit = registered('0', '\u{fe00}').expect("zero + VS1");
        assert_eq!(digit.registry, Registry::Standardized);
        assert_eq!(digit.description, "short diagonal stroke form");
        assert_eq!(
            registered('\u{585a}', '\u{fe00}').map(|v| v.description),
            Some("CJK COMPATIBILITY IDEOGRAPH-FA10")
        );
        // Mongolian free variation selectors, FVS4 included.
        assert_eq!(
            registered('\u{1820}', '\u{180b}').map(|v| v.description),
            Some("second form")
        );

        let kanji = registered('\u{845b}', '\u{e0100}').expect("kuzu + VS17");
        assert_eq!(kanji.registry, Registry::Ideographic);
        assert_eq!(kanji.description, "Adobe-Japan1");
    }

    #[test]
    fn an_unlisted_pair_is_not_registered() {
        // VS16 after a letter, a selector after a selector, and a selector
        // with nothing registered for that kanji.
        assert_eq!(registered('a', '\u{fe0f}'), None);
        assert_eq!(registered('\u{fe0f}', '\u{fe0f}'), None);
        assert_eq!(registered('\u{845b}', '\u{e01ef}'), None);
        // Every data line parsed: 1437 standardized + 742 emoji + 29642
        // distinct IVD sequences (39509 lines, shared across collections).
        assert_eq!(table().len(), 1437 + 742 + 29642);
    }

    #[test]
    fn every_selector_is_recognized() {
        for c in [
            '\u{fe00}',
            '\u{fe0f}',
            '\u{e0100}',
            '\u{e01ef}',
            '\u{180b}',
            '\u{180f}',
        ] {
            assert!(is_selector(c), "{:04X}", c as u32);
        }
        for c in ['\u{180e}', '\u{200b}', 'a'] {
            assert!(!is_selector(c), "{:04X}", c as u32);
        }
    }
}
