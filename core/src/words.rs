//! The YuioLink curated wordlist — 3456 short (<=6 letter), memorable,
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
//! `tools/yuiolink-curated.txt`, copied here as `words.txt`. A hand-reviewed
//! 2026-07-09 pass (see `design/wordlist-soundalikes.html`) dropped 60
//! sound-alikes — homophones and near-homophones like mace/maze — removing
//! BOTH words of a confusable pair, since a name that survives with a
//! sound-alike spelling still cannot be written down reliably from speech.
//!
//! Note: the list contains one hyphenated entry, `yo-yo`; it is kept as-is.

use std::sync::OnceLock;

/// The raw wordlist, one lowercase word per line.
const WORDS_RAW: &str = include_str!("words.txt");

/// Number of words in the list.
pub const WORD_COUNT: usize = 3456;

/// The entropy the site markets on: a four-word name is claimed to be ~47 bits.
/// The claim is public copy ("47-bit namespace", "about 153 trillion
/// possibilities"), so it is asserted in the tests rather than left to arithmetic
/// nobody redoes after a curation pass.
///
/// At 3456 words a name carries 4 × log2(3456) ≈ 47.02 bits, which leaves room to
/// drop only about ten more words before the claim stops being true. Pruning past
/// that means editing the copy, not the assertion.
pub const CLAIMED_NAME_BITS: f64 = 47.0;

/// Words in a name whose entropy the claim covers (Secret and One-Time links).
pub const CLAIMED_NAME_WORDS: u32 = 4;

/// Bits of entropy in a name of `word_count` words drawn uniformly from the list.
///
/// `RESERVED_NAMES` does not enter into this: only one-word names can be
/// reserved, so the four-word tier draws from the full list.
pub fn name_bits(word_count: u32) -> f64 {
    (WORD_COUNT as f64).log2() * f64::from(word_count)
}

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
    fn four_word_names_still_clear_the_advertised_47_bits() {
        let bits = name_bits(CLAIMED_NAME_WORDS);
        assert!(
            bits >= CLAIMED_NAME_BITS,
            "a {CLAIMED_NAME_WORDS}-word name is {bits:.2} bits, below the advertised \
             {CLAIMED_NAME_BITS} — either restore words or change the public copy \
             (views.rs \"47-bit namespace\", README, docs/NAMESPACES.md)"
        );
        // The other half of the claim: "about 153 trillion possibilities".
        let combos = (WORD_COUNT as f64).powi(CLAIMED_NAME_WORDS as i32);
        assert!(
            (1.4e14..1.6e14).contains(&combos),
            "combination count {combos:.3e} no longer rounds to ~153 trillion"
        );
        // Headroom, so a curation pass can see how close it is running.
        let floor = (2f64.powf(CLAIMED_NAME_BITS / f64::from(CLAIMED_NAME_WORDS))).ceil() as usize;
        assert!(
            WORD_COUNT >= floor,
            "the list may not go below {floor} words while the copy claims \
             {CLAIMED_NAME_BITS} bits"
        );
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
