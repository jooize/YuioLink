//! The YuioLink curated wordlist — 3516 short (<=6 letter), memorable,
//! easy-to-type, broadly inoffensive words used to build the "shoutkey" link
//! names (e.g. `braveOTTER`).
//!
//! The list is embedded verbatim so the server, the future CLI, and the macOS
//! app all draw names from the same namespace. A bigger list buys entropy per
//! word: ~11.8 bits each, so a four-word single-use name clears ~47 bits — enough
//! that the name itself, with no separate secret, resists enumeration of the
//! single view it guards. See `tools/` for the curation provenance.
//!
//! Provenance: a length-capped (<=6 chars) union of the EFF Short Wordlist #1,
//! BIP39, and the EFF Large list (base forms only), then hand-curated down with
//! `tools/wordlist-curator.html` — dropping the rarest words, redundant plurals,
//! brands/trademarks, slurs and adult/clinical terms, and hard-to-spell entries.
//! The curation lens was "anyone can use this": short, memorable, concrete,
//! unsurprising words. The canonical source of truth is
//! `tools/yuiolink-curated.txt`, copied here as `words.txt`.
//!
//! Note: the list contains one hyphenated entry, `yo-yo`; it is kept as-is.

use std::sync::OnceLock;

/// The raw wordlist, one lowercase word per line.
const WORDS_RAW: &str = include_str!("words.txt");

/// Number of words in the list.
pub const WORD_COUNT: usize = 3516;

/// The word list, split once and cached.
pub fn words() -> &'static [&'static str] {
    static WORDS: OnceLock<Vec<&'static str>> = OnceLock::new();
    WORDS.get_or_init(|| {
        WORDS_RAW
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn list_has_expected_size_and_bounds() {
        let w = words();
        assert_eq!(w.len(), WORD_COUNT);
        // The embedded list is sorted; these anchor the first and last entries.
        assert_eq!(w[0], "abacus");
        assert_eq!(*w.last().unwrap(), "zoom");
    }

    #[test]
    fn words_are_short_lowercase_ascii() {
        // Every word is lowercase ASCII and at most six letters; `yo-yo` is the
        // lone hyphenated entry (its hyphen does not count toward the cap).
        for &word in words() {
            assert!(
                word.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'),
                "unexpected character in {word:?}"
            );
            assert!(
                word.chars().filter(|&c| c != '-').count() <= 6,
                "word longer than six letters: {word:?}"
            );
        }
    }

    #[test]
    fn words_are_unique() {
        let set: HashSet<&str> = words().iter().copied().collect();
        assert_eq!(set.len(), WORD_COUNT);
    }
}
