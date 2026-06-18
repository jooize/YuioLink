//! The EFF Short Wordlist #1 — 1296 short, memorable, easy-to-type words used to
//! build YuioLink's "shoutkey" link names (e.g. `braveOTTER`).
//!
//! The list is the canonical EFF short list (each word 3-5 letters, the first
//! three letters unique). It is embedded verbatim so the server, the future CLI,
//! and the macOS app all draw names from the same namespace.
//!
//! Source: Electronic Frontier Foundation, "EFF Short Wordlist #1", at
//! <https://www.eff.org/files/2016/09/08/eff_short_wordlist_1.txt>. The official
//! file is tab-separated (`dice-number<TAB>word`); `eff_short.txt` is that file
//! with the dice column stripped, leaving one word per line in the original
//! order. Verified identical to the official words (sha256 of the word column:
//! 36ecca49e4fa20ca84b176c32f2e9c82f98f446585190e75f9879a95c08247bf).
//! Licensed CC BY 3.0 US (<https://creativecommons.org/licenses/by/3.0/us/>).
//!
//! Note: the list contains one hyphenated entry, `yo-yo`; it is kept as-is so the
//! embedded list stays identical to the published EFF list.

use std::sync::OnceLock;

/// The raw wordlist, one lowercase word per line.
const WORDS_RAW: &str = include_str!("eff_short.txt");

/// Number of words in the list (4-dice diceware: 6^4).
pub const WORD_COUNT: usize = 1296;

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
        assert_eq!(w[0], "acid");
        assert_eq!(*w.last().unwrap(), "zoom");
    }

    #[test]
    fn words_are_lowercase_ascii() {
        // Every word is lowercase ASCII; `yo-yo` is the lone hyphenated entry.
        for &word in words() {
            assert!(
                word.bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-'),
                "unexpected character in {word:?}"
            );
        }
    }

    #[test]
    fn words_are_unique() {
        let set: HashSet<&str> = words().iter().copied().collect();
        assert_eq!(set.len(), WORD_COUNT);
    }
}
