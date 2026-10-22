//! Hidden characters that are part of an emoji: a zero-width joiner between
//! two emoji (a family, the rainbow flag) and the tags that spell a
//! subdivision flag (Scotland's). They draw as part of the picture, so a
//! snippet does not mark them. Every other joiner and tag stays marked.
//!
//! Which characters are emoji comes from Unicode's own emoji-data.txt,
//! embedded verbatim and pinned by hash (see [`SOURCE`]), Licensed under
//! Unicode License v3, server/assets/unicode/LICENSE.txt. app.js applies the
//! same rule with the browser's `\p{Extended_Pictographic}`.

use std::sync::OnceLock;

const DATA: &str = include_str!("../assets/unicode/emoji-data.txt");

/// Where the embedded file came from, and the SHA-256 it must have.
#[cfg_attr(not(test), allow(dead_code))]
pub const SOURCE: (&str, &str, &str) = (
    "emoji-data.txt",
    "https://www.unicode.org/Public/18.0.0/ucd/emoji/emoji-data.txt",
    "80d00f8e616a0ef27fd6b8de3b758c06383b5d917e2977709578e68baf733bf1",
);

/// The Extended_Pictographic ranges, sorted. Parsed once, on first use.
fn pictographic() -> &'static [(u32, u32)] {
    static RANGES: OnceLock<Vec<(u32, u32)>> = OnceLock::new();
    RANGES.get_or_init(|| {
        let mut out: Vec<(u32, u32)> = DATA
            .lines()
            .filter_map(|line| {
                let line = line.split('#').next()?;
                let (cps, prop) = line.split_once(';')?;
                if prop.trim() != "Extended_Pictographic" {
                    return None;
                }
                let (a, b) = cps
                    .trim()
                    .split_once("..")
                    .unwrap_or((cps.trim(), cps.trim()));
                Some((
                    u32::from_str_radix(a, 16).ok()?,
                    u32::from_str_radix(b, 16).ok()?,
                ))
            })
            .collect();
        out.sort_unstable();
        out
    })
}

pub fn is_pictographic(c: char) -> bool {
    let cp = c as u32;
    let ranges = pictographic();
    let i = ranges.partition_point(|&(a, _)| a <= cp);
    i > 0 && cp <= ranges[i - 1].1
}

const fn is_tag(c: char) -> bool {
    matches!(c, '\u{e0020}'..='\u{e007e}')
}

/// True when `c`, at byte `i` of `text`, is part of an emoji: a joiner with
/// an emoji on each side (a skin tone or VS16 may end the one before), or a
/// tag in a black flag's tag sequence, its cancel tag included.
pub fn in_emoji(text: &str, i: usize, c: char) -> bool {
    let before = &text[..i];
    let after = &text[i + c.len_utf8()..];
    match c {
        '\u{200d}' => {
            let prev = before
                .chars()
                .rev()
                .find(|&b| !matches!(b, '\u{fe0f}' | '\u{1f3fb}'..='\u{1f3ff}'));
            prev.is_some_and(is_pictographic) && after.chars().next().is_some_and(is_pictographic)
        }
        '\u{e0020}'..='\u{e007f}' => {
            let mut back = before.chars().rev().skip_while(|&t| is_tag(t));
            let tags_before = before.chars().rev().take_while(|&t| is_tag(t)).count();
            let flag = back.next() == Some('\u{1f3f4}');
            if c == '\u{e007f}' {
                flag && tags_before > 0
            } else {
                flag && after.chars().find(|&t| !is_tag(t)) == Some('\u{e007f}')
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn the_embedded_file_matches_its_pin() {
        let got: String = Sha256::digest(DATA.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(got, SOURCE.2);
    }

    /// The hidden characters of `text` that are not part of an emoji.
    fn marked(text: &str) -> Vec<char> {
        text.char_indices()
            .filter(|&(i, c)| {
                matches!(c, '\u{200d}' | '\u{e0020}'..='\u{e007f}') && !in_emoji(text, i, c)
            })
            .map(|(_, c)| c)
            .collect()
    }

    #[test]
    fn joiners_and_tags_inside_an_emoji_are_not_marked() {
        // Family; rainbow flag (VS16 before the joiner); a skin tone before it.
        assert!(marked("\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}").is_empty());
        assert!(marked("\u{1f3f3}\u{fe0f}\u{200d}\u{1f308}").is_empty());
        assert!(marked("\u{1f469}\u{1f3fd}\u{200d}\u{1f4bb}").is_empty());
        // Scotland: black flag, g b s c t, cancel tag.
        assert!(
            marked("\u{1f3f4}\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}").is_empty()
        );
    }

    #[test]
    fn joiners_and_tags_elsewhere_stay_marked() {
        assert_eq!(marked("sig\u{200d}ned"), ['\u{200d}']);
        assert_eq!(marked("\u{1f468}\u{200d}a"), ['\u{200d}']);
        // Tags with no flag, and a flag's tags with no cancel tag.
        assert_eq!(marked("a\u{e0061}\u{e007f}"), ['\u{e0061}', '\u{e007f}']);
        assert_eq!(
            marked("\u{1f3f4}\u{e0067}\u{e0062}"),
            ['\u{e0067}', '\u{e0062}']
        );
        // A cancel tag right after the flag spells nothing.
        assert_eq!(marked("\u{1f3f4}\u{e007f}"), ['\u{e007f}']);
    }
}
