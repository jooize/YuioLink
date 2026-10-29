//! Third-party notices: NOTICE and every license text it names, built into the
//! binary so they travel with every copy of it. The fonts' licenses (OFL 1.1,
//! Bitstream Vera) and Apache-2.0 section 4(d) all ask for this, and a release
//! asset is a bare binary. Served at `/legal/notices.txt`, printed by
//! `yuiolink-server --notices`, and attached to each release by release.yml
//! from that same output.

use std::sync::OnceLock;

/// Each part as `(path in the repository, text)`, in the order they print.
/// The paths are the ones NOTICE names, so a reader can find each license the
/// notice points to by its heading.
const PARTS: &[(&str, &str)] = &[
    ("NOTICE", include_str!("../../NOTICE")),
    ("LICENSE-MIT", include_str!("../../LICENSE-MIT")),
    ("LICENSE-APACHE", include_str!("../../LICENSE-APACHE")),
    (
        "server/assets/fonts/LICENSE.txt",
        include_str!("../assets/fonts/LICENSE.txt"),
    ),
    (
        "server/assets/fonts/noto/LICENSE.txt",
        include_str!("../assets/fonts/noto/LICENSE.txt"),
    ),
    (
        "server/assets/unicode/LICENSE.txt",
        include_str!("../assets/unicode/LICENSE.txt"),
    ),
];

/// Every part in one plain text, each under a heading naming its path.
pub fn text() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        let mut out = String::new();
        for (i, (path, body)) in PARTS.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            let rule = "=".repeat(path.len());
            out.push_str(&format!("{rule}\n{path}\n{rule}\n\n"));
            out.push_str(body.trim_end());
            out.push('\n');
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new NOTICE section that points at a license file must bring that file
    /// into [`PARTS`], or the binary would ship the pointer without the text.
    #[test]
    fn every_license_file_notice_names_is_built_in() {
        let notice = PARTS[0].1;
        let named: Vec<&str> = notice
            .split_whitespace()
            .map(|w| w.trim_end_matches([',', '.', ';', ')']))
            .filter(|w| w.starts_with("server/") && w.ends_with("LICENSE.txt"))
            .collect();
        assert!(
            named.len() >= 3,
            "NOTICE should name the font and Unicode licenses: {named:?}"
        );
        for path in named {
            assert!(
                PARTS.iter().any(|(p, _)| *p == path),
                "NOTICE names {path}, which is not in notices::PARTS"
            );
        }
    }

    #[test]
    fn the_text_carries_each_license_in_full() {
        let t = text();
        for needle in [
            "SIL OPEN FONT LICENSE Version 1.1",
            "Bitstream Vera Fonts Copyright",
            "Arev Fonts Copyright",
            "Apache License",
            "Permission is hereby granted",
            "UNICODE LICENSE V3",
        ] {
            assert!(t.contains(needle), "notices lack {needle:?}");
        }
    }
}
