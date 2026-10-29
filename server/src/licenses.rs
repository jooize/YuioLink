//! Third-party licenses: NOTICE and every license text it names (the fonts',
//! the Unicode data's and every linked crate's), built into the binary so they
//! travel with every copy of it. The fonts' licenses (OFL 1.1, Bitstream Vera),
//! the crates' MIT and BSD terms and Apache-2.0 section 4(d) all ask for this,
//! and a release asset is a bare binary. Shown at `/legal/licenses`, served as
//! one plain text at `/legal/licenses.txt`, printed by
//! `yuiolink-server --licenses`, and attached to each release by release.yml
//! from that same output.

use crate::views::{document_full, home_chip};
use maud::{Markup, html};
use std::sync::OnceLock;

/// One license text: where it lives in the repository (the path NOTICE names),
/// its anchor on `/legal/licenses`, the heading shown there, and the text.
struct Part {
    path: &'static str,
    id: &'static str,
    title: &'static str,
    text: &'static str,
}

/// Every part, in the order both forms show them.
const PARTS: &[Part] = &[
    Part {
        path: "NOTICE",
        id: "notice",
        title: "Notice",
        text: include_str!("../../NOTICE"),
    },
    Part {
        path: "LICENSE-MIT",
        id: "yuiolink-mit",
        title: "YuioLink: MIT License",
        text: include_str!("../../LICENSE-MIT"),
    },
    Part {
        path: "LICENSE-APACHE",
        id: "yuiolink-apache",
        title: "YuioLink: Apache License 2.0",
        text: include_str!("../../LICENSE-APACHE"),
    },
    Part {
        path: "server/assets/fonts/LICENSE.txt",
        id: "dejavu",
        title: "DejaVu Fonts",
        text: include_str!("../assets/fonts/LICENSE.txt"),
    },
    Part {
        path: "server/assets/fonts/noto/LICENSE.txt",
        id: "noto",
        title: "Noto Fonts",
        text: include_str!("../assets/fonts/noto/LICENSE.txt"),
    },
    Part {
        path: "server/assets/crates/LICENSE.txt",
        id: "crates",
        title: "Rust Crates",
        text: include_str!("../assets/crates/LICENSE.txt"),
    },
    Part {
        path: "server/assets/unicode/LICENSE.txt",
        id: "unicode",
        title: "Unicode Data",
        text: include_str!("../assets/unicode/LICENSE.txt"),
    },
];

/// The name a saved copy of the plain text gets, on the web and as a release
/// asset: it says whose licenses they are once it sits in a downloads folder.
pub const FILE_NAME: &str = "yuiolink-server-licenses.txt";

/// Every part in one plain text, each under a heading naming its path.
pub fn text() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        let mut out = String::new();
        for (i, part) in PARTS.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            let rule = "=".repeat(part.path.len());
            out.push_str(&format!("{rule}\n{}\n{rule}\n\n", part.path));
            out.push_str(part.text.trim_end());
            out.push('\n');
        }
        out
    })
}

/// `/legal/licenses`: the same parts as [`text`], each under its own heading,
/// with a contents list first, since the whole runs to a few hundred KB.
pub fn page() -> Markup {
    let body = html! {
        (home_chip("/legal", "Back to Legal"))
        h2.help-title.legal-title { "Licenses" }
        p.help-lead {
            "YuioLink includes fonts, data and Rust crates made by others. Each "
            "is used under its own license, and the full texts follow. The same "
            "texts as one plain file: "
            a href="/legal/licenses.txt" { "/legal/licenses.txt" }
            "."
        }
        nav.licenses-toc aria-label="Contents" {
            ol {
                @for part in PARTS {
                    li { a href=(format!("#{}", part.id)) { (part.title) } }
                }
            }
        }
        @for part in PARTS {
            h3.help-h id=(part.id) { (part.title) }
            pre.help-code.license-text { (part.text.trim_end()) }
        }
        footer { a href="/legal" { "Back to Legal" } }
    };
    document_full(
        "YuioLink — Licenses",
        html! {
            meta name="description" content="The licenses of the fonts, data and Rust crates YuioLink includes, in full.";
        },
        body,
        html! {},
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new NOTICE section that points at a license file must bring that file
    /// into [`PARTS`], or the binary would ship the pointer without the text.
    #[test]
    fn every_license_file_notice_names_is_built_in() {
        let notice = PARTS[0].text;
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
                PARTS.iter().any(|p| p.path == path),
                "NOTICE names {path}, which is not in licenses::PARTS"
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
            "Rust crates linked into yuiolink-server",
            "Copyright (c) 2014 Chris Wong",
        ] {
            assert!(t.contains(needle), "licenses lack {needle:?}");
        }
    }

    #[test]
    fn every_anchor_is_unique() {
        let mut ids: Vec<&str> = PARTS.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), PARTS.len());
    }
}
