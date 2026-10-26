//! Joiners that are ordinary spelling. Persian writes a zero-width non-joiner
//! inside everyday words (می‌خواهم), and Hindi, Malayalam, Sinhala and the
//! other Indic scripts put a joiner or non-joiner after a virama to pick a
//! letter form. In a text such a joiner is still marked, but with the quiet
//! word tier: shown, never counted with the rest, never taken out by Remove.
//! In an address it stays red, as every hidden character does there.
//!
//! The rule is IDNA2008's CONTEXTJ (RFC 5892, Appendix A.1 and A.2), the
//! standard test for where these two characters belong in a word:
//! - either one right after a virama (Canonical_Combining_Class 9);
//! - a non-joiner between a letter that joins forward (Joining_Type L or D)
//!   and one that joins back (R or D), with only transparent marks (T, such
//!   as Arabic vowel signs) between them and it.
//!
//! The properties come from ICU4X's compiled data. app.js applies the same
//! rule to the ranges [`client_table`] gives it, so the two never disagree;
//! until they arrive, the form marks every joiner red (it fails closed).

use std::sync::OnceLock;

use icu_properties::CodePointMapData;
use icu_properties::props::{CanonicalCombiningClass, JoiningType};

const ZWNJ: char = '\u{200c}';
const ZWJ: char = '\u{200d}';

fn joining(c: char) -> JoiningType {
    CodePointMapData::<JoiningType>::new().get(c)
}

fn is_virama(c: char) -> bool {
    CodePointMapData::<CanonicalCombiningClass>::new().get(c) == CanonicalCombiningClass::Virama
}

/// True when `c`, at byte `i` of `text`, is a joiner where spelling puts
/// one (RFC 5892 CONTEXTJ).
pub fn spells(text: &str, i: usize, c: char) -> bool {
    if c != ZWNJ && c != ZWJ {
        return false;
    }
    let before = &text[..i];
    let after = &text[i + c.len_utf8()..];
    if before.chars().next_back().is_some_and(is_virama) {
        return true;
    }
    if c == ZWJ {
        return false;
    }
    let past_marks = |ch: &char| joining(*ch) == JoiningType::Transparent;
    let back = before.chars().rev().find(|ch| !past_marks(ch));
    let ahead = after.chars().find(|ch| !past_marks(ch));
    back.is_some_and(|b| {
        matches!(
            joining(b),
            JoiningType::LeftJoining | JoiningType::DualJoining
        )
    }) && ahead.is_some_and(|a| {
        matches!(
            joining(a),
            JoiningType::RightJoining | JoiningType::DualJoining
        )
    })
}

/// The ranges app.js needs to apply [`spells`], as JSON: the viramas, the
/// letters that join forward (L, D) and back (R, D), and the transparent
/// ones, as `[first, last]` code point pairs. About 7 KB, so app.js fetches
/// it only once the create form holds a joiner. Built once.
pub fn client_table() -> &'static str {
    static JSON: OnceLock<String> = OnceLock::new();
    JSON.get_or_init(|| table().to_string())
}

fn table() -> serde_json::Value {
    let ccc = CodePointMapData::<CanonicalCombiningClass>::new();
    let jt = CodePointMapData::<JoiningType>::new();
    let ranges = |keep: &dyn Fn(JoiningType) -> bool| {
        let mut out: Vec<[u32; 2]> = Vec::new();
        for r in jt.iter_ranges().filter(|r| keep(r.value)) {
            match out.last_mut() {
                Some(last) if last[1] + 1 == *r.range.start() => last[1] = *r.range.end(),
                _ => out.push([*r.range.start(), *r.range.end()]),
            }
        }
        out
    };
    let viramas: Vec<[u32; 2]> = ccc
        .iter_ranges_for_value(CanonicalCombiningClass::Virama)
        .map(|r| [*r.start(), *r.end()])
        .collect();
    serde_json::json!({
        "virama": viramas,
        "forward": ranges(&|t| matches!(t, JoiningType::LeftJoining | JoiningType::DualJoining)),
        "back": ranges(&|t| matches!(t, JoiningType::RightJoining | JoiningType::DualJoining)),
        "transparent": ranges(&|t| t == JoiningType::Transparent),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spelled(text: &str) -> Vec<bool> {
        text.char_indices()
            .filter(|&(_, c)| c == ZWNJ || c == ZWJ)
            .map(|(i, c)| spells(text, i, c))
            .collect()
    }

    #[test]
    fn persian_non_joiners_inside_words_are_spelling() {
        // mi-khaham ("I want"); ketab-ha ("books").
        assert_eq!(spelled("می\u{200c}خواهم"), [true]);
        assert_eq!(spelled("کتاب\u{200c}ها"), [true]);
        // A vowel sign between the letter and the non-joiner is transparent.
        assert_eq!(spelled("بِ\u{200c}ه"), [true]);
    }

    #[test]
    fn joiners_after_a_virama_are_spelling() {
        // Devanagari eyelash ra; Sinhala yansaya; Malayalam explicit virama.
        assert_eq!(spelled("र्\u{200d}य"), [true]);
        assert_eq!(spelled("ක්\u{200d}ය"), [true]);
        assert_eq!(spelled("ക്\u{200c}ക"), [true]);
    }

    #[test]
    fn joiners_elsewhere_are_not() {
        // Between Latin letters, at a word's edge, after a right-joining
        // letter (alef joins only back), and a joiner between Arabic letters.
        assert_eq!(spelled("sig\u{200c}ned"), [false]);
        assert_eq!(spelled("\u{200c}خواهم"), [false]);
        assert_eq!(spelled("می\u{200c}"), [false]);
        assert_eq!(spelled("ا\u{200c}ب"), [false]);
        assert_eq!(spelled("می\u{200d}خواهم"), [false]);
    }

    #[test]
    fn the_client_table_holds_what_the_rule_reads() {
        let t: serde_json::Value = serde_json::from_str(client_table()).unwrap();
        let has =
            |key: &str, cp: u32| {
                t[key].as_array().unwrap().iter().any(|r| {
                    r[0].as_u64().unwrap() <= cp as u64 && cp as u64 <= r[1].as_u64().unwrap()
                })
            };
        assert!(has("virama", 0x094d));
        assert!(has("forward", 0x0628) && has("back", 0x0628));
        assert!(!has("forward", 0x0627) && has("back", 0x0627));
        assert!(has("transparent", 0x0650));
    }
}
