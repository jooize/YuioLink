//! Joiners that are ordinary spelling. Persian writes a zero-width non-joiner
//! inside everyday words (می‌خواهم), and Hindi, Malayalam, Sinhala and the
//! other Indic scripts put a joiner or non-joiner after a virama to pick a
//! letter form. In a text such a joiner is still marked, but with the quiet
//! word tier: shown, never counted with the rest, never taken out by Remove.
//! In an address it stays red, as every hidden character does there.
//!
//! The rule starts from IDNA2008's CONTEXTJ (RFC 5892, Appendix A.1 and
//! A.2), the standard test for where these two characters belong in a word,
//! and adds the two joiner uses the Unicode Standard describes beyond it.
//! Each reads only character properties, so it covers every script that has
//! them, and each holds only where the joiner changes how the letters draw:
//! - either one right after a virama (Canonical_Combining_Class 9);
//! - a non-joiner between a letter that joins forward (Joining_Type L or D)
//!   and one that joins back (R or D), with only transparent marks (T, such
//!   as Arabic vowel signs) between them and it;
//! - a joiner right before a virama, after a letter of the virama's script
//!   (Unicode 18.0 section 12.1.2: Devanagari and Oriya, also Kannada and
//!   Telugu RA; UAX #31 revision 37 section 2.3.2);
//! - a joiner that shows a cursive letter's joined form with no partner
//!   (section 23.2.2: Persian abbreviations, teaching letter forms): a letter
//!   joins toward it on exactly one side. Between two letters that join
//!   anyway it changes nothing, so it stays red.
//!
//! The properties come from ICU4X's compiled data. app.js applies the same
//! rule to the ranges [`client_table`] gives it, so the two never disagree;
//! until they arrive, the form marks every joiner red (it fails closed).

use std::sync::OnceLock;

use icu_properties::CodePointMapData;
use icu_properties::props::{
    CanonicalCombiningClass, GeneralCategory, GeneralCategoryGroup, JoiningType, Script,
};

const ZWNJ: char = '\u{200c}';
const ZWJ: char = '\u{200d}';

fn joining(c: char) -> JoiningType {
    CodePointMapData::<JoiningType>::new().get(c)
}

fn is_virama(c: char) -> bool {
    CodePointMapData::<CanonicalCombiningClass>::new().get(c) == CanonicalCombiningClass::Virama
}

fn is_letter(c: char) -> bool {
    GeneralCategoryGroup::Letter.contains(CodePointMapData::<GeneralCategory>::new().get(c))
}

fn script(c: char) -> Script {
    CodePointMapData::<Script>::new().get(c)
}

fn joins_forward(c: char) -> bool {
    matches!(
        joining(c),
        JoiningType::LeftJoining | JoiningType::DualJoining
    )
}

fn joins_back(c: char) -> bool {
    matches!(
        joining(c),
        JoiningType::RightJoining | JoiningType::DualJoining
    )
}

/// True when `c`, at byte `i` of `text`, is a joiner where spelling puts
/// one (see the module notes).
pub fn spells(text: &str, i: usize, c: char) -> bool {
    if c != ZWNJ && c != ZWJ {
        return false;
    }
    let before = &text[..i];
    let after = &text[i + c.len_utf8()..];
    if before.chars().next_back().is_some_and(is_virama) {
        return true;
    }
    let past_marks = |ch: &char| joining(*ch) == JoiningType::Transparent;
    let back = before.chars().rev().find(|ch| !past_marks(ch));
    let ahead = after.chars().find(|ch| !past_marks(ch));
    let forward = back.is_some_and(joins_forward);
    let backward = ahead.is_some_and(joins_back);
    if c == ZWNJ {
        return forward && backward;
    }
    let next = after.chars().next();
    let before_virama = next.is_some_and(is_virama)
        && back.is_some_and(|b| is_letter(b) && next.is_some_and(|v| script(b) == script(v)));
    before_virama || forward != backward
}

/// The ranges app.js needs to apply [`spells`], as JSON: the viramas, the
/// letters that join forward (L, D) and back (R, D), the transparent ones,
/// and per script that has a virama its letters and viramas, as `[first,
/// last]` code point pairs. Small, but app.js fetches it only once the
/// create form holds a joiner. Built once.
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
    // A script's letters and viramas, for a joiner right before a virama.
    let mut scripts: Vec<Script> = Vec::new();
    for v in viramas
        .iter()
        .flat_map(|r| r[0]..=r[1])
        .filter_map(char::from_u32)
    {
        if !scripts.contains(&script(v)) {
            scripts.push(script(v));
        }
    }
    let sc = CodePointMapData::<Script>::new();
    let gc = CodePointMapData::<GeneralCategory>::new();
    let pairs = |it: &mut dyn Iterator<Item = u32>| {
        let mut out: Vec<[u32; 2]> = Vec::new();
        for cp in it {
            match out.last_mut() {
                Some(last) if last[1] + 1 == cp => last[1] = cp,
                _ => out.push([cp, cp]),
            }
        }
        out
    };
    let per_script: Vec<serde_json::Value> = scripts
        .iter()
        .map(|&s| {
            let letters = pairs(&mut sc.iter_ranges_for_value(s).flatten().filter(|&cp| {
                char::from_u32(cp).is_some_and(|c| GeneralCategoryGroup::Letter.contains(gc.get(c)))
            }));
            let marks = pairs(
                &mut viramas
                    .iter()
                    .flat_map(|r| r[0]..=r[1])
                    .filter(|&cp| char::from_u32(cp).is_some_and(|c| script(c) == s)),
            );
            serde_json::json!({ "letters": letters, "viramas": marks })
        })
        .collect();
    serde_json::json!({
        "virama": viramas,
        "scripts": per_script,
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
    fn joiners_before_a_virama_are_spelling() {
        // Devanagari RA without reph (12.1.2); Kannada and Telugu RA.
        assert_eq!(spelled("र\u{200d}्य"), [true]);
        assert_eq!(spelled("ರ\u{200d}್ಯ"), [true]);
        assert_eq!(spelled("ర\u{200d}్య"), [true]);
        // After a letter of another script, or with nothing before it.
        assert_eq!(spelled("a\u{200d}्य"), [false]);
        assert_eq!(spelled("\u{200d}्य"), [false]);
        // A non-joiner there has no such use.
        assert_eq!(spelled("र\u{200c}्य"), [false]);
    }

    #[test]
    fn a_joiner_showing_a_joined_form_alone_is_spelling() {
        // Persian abbreviation (heh, joiner); a medial ghain on its own.
        assert_eq!(spelled("ه\u{200d}"), [true]);
        assert_eq!(spelled("\u{200d}غ\u{200d}"), [true, true]);
        // A vowel sign between the letter and the joiner is transparent.
        assert_eq!(spelled("بِ\u{200d} "), [true]);
    }

    #[test]
    fn joiners_elsewhere_are_not() {
        // Between Latin letters, at a word's edge, after a right-joining
        // letter (alef joins only back), and a joiner between Arabic letters.
        assert_eq!(spelled("sig\u{200c}ned"), [false]);
        assert_eq!(spelled("\u{200c}خواهم"), [false]);
        assert_eq!(spelled("می\u{200c}"), [false]);
        assert_eq!(spelled("ا\u{200c}ب"), [false]);
        // A joiner between two letters that join anyway changes nothing.
        assert_eq!(spelled("می\u{200d}خواهم"), [false]);
        // A joiner beside letters that never join (hamza, Latin).
        assert_eq!(spelled("ء\u{200d}ء"), [false]);
        assert_eq!(spelled("a\u{200d}"), [false]);
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
        let kannada = t["scripts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| {
                s["viramas"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r[0] == 0x0ccd)
            })
            .unwrap();
        assert!(
            kannada["letters"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r[0].as_u64().unwrap() <= 0x0cb0 && 0x0cb0 <= r[1].as_u64().unwrap())
        );
    }
}
