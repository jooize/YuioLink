//! Server-side share-card (og:image) rendering.
//!
//! Builds an SVG card from a template and rasterises it to PNG with resvg — pure
//! Rust, no system libraries (it builds `default-features = false, features =
//! ["text"]`, so the SVG-embedded-raster decoders and system-font loading are
//! dropped). Two DejaVu fonts are embedded so the card renders identically with no
//! system fonts installed, e.g. in a slim container. An ordinary card shows the
//! destination domain — decided, so a shared link reads as trustworthy.
//!
//! A **one-time** link's card shows no destination at all. The preview page is
//! blind until the use is spent, and an unfurl that named the domain would hand
//! it to every chat server the link passed through without spending anything —
//! which is precisely the disclosure the blind card exists to prevent.
//!
//! The wordmark carries the only brand colour — "Link" in the accent blue, with no
//! separate mark beside it — so the card holds one accent, not three. The greys are
//! `#55555c` (~6.5:1) rather than the lighter `#6e6e73`: a share card is usually
//! seen scaled down to a couple of hundred pixels, where the secondary lines are
//! the first thing to dissolve.

use std::sync::{Arc, OnceLock};

use resvg::tiny_skia;
use resvg::usvg;

/// Embedded fonts (see assets/fonts/LICENSE.txt). Sans for the wordmark/kicker/
/// foot, Mono for the destination domain (it reads as a precise address).
const SANS_FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
const MONO_FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");

/// Standard large-summary card size.
const WIDTH: u32 = 1200;
const HEIGHT: u32 = 630;
/// Horizontal padding; the destination must fit within `WIDTH - 2*PAD`.
const PAD: f32 = 96.0;

/// What a share card states. All plaintext.
pub struct Card<'a> {
    /// "Ephemeral redirect" or "One-time link".
    pub kicker: &'a str,
    /// The line under the kicker: a destination's registrable domain, or --
    /// for a one-time link, which has disclosed nothing yet -- what will
    /// happen instead.
    pub hero: &'a str,
    /// True when `hero` is a sentence rather than an address. It then reads in
    /// the UI font, and the kicker drops its arrow: there is no destination
    /// for the arrow to point at.
    pub blind: bool,
    /// e.g. "expires Jun 29, 2026 · 14:30 UTC".
    pub foot: &'a str,
}

/// Render a card to PNG bytes. `None` only if rasterisation fails (it should not
/// for our fixed-size template).
pub fn render_png(card: &Card) -> Option<Vec<u8>> {
    let svg = build_svg(card);
    let opt = usvg::Options {
        // Fallback family for any text we didn't explicitly set.
        font_family: "DejaVu Sans".to_string(),
        fontdb: fontdb(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&svg, &opt).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(WIDTH, HEIGHT)?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

/// The bundled fonts, parsed once into a shared, immutable database.
fn fontdb() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_font_data(SANS_FONT.to_vec());
        db.load_font_data(MONO_FONT.to_vec());
        Arc::new(db)
    })
    .clone()
}

fn build_svg(card: &Card) -> String {
    let hero = xml_escape(&fit_hero(card.hero));
    let kicker = xml_escape(card.kicker);
    let foot = xml_escape(card.foot);
    let hero_size = hero_font_size(card.hero, card.blind);
    let hero_font = if card.blind {
        "DejaVu Sans"
    } else {
        "DejaVu Sans Mono"
    };
    // The arrow promises a destination, so a blind card does not draw one.
    let arrow = if card.blind {
        String::new()
    } else {
        " <tspan fill=\"#007aff\">&#8594;</tspan>".to_string()
    };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#f7f8fa"/>
      <stop offset="1" stop-color="#e7e8ec"/>
    </linearGradient>
    <radialGradient id="glow" cx="1" cy="0" r="0.9">
      <stop offset="0" stop-color="#007aff" stop-opacity="0.12"/>
      <stop offset="0.55" stop-color="#007aff" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="{WIDTH}" height="{HEIGHT}" fill="url(#bg)"/>
  <rect width="{WIDTH}" height="{HEIGHT}" fill="url(#glow)"/>
  <text x="96" y="117" font-family="DejaVu Sans" font-weight="bold" font-size="44" fill="#1d1d1f">Yuio<tspan fill="#007aff">Link</tspan></text>
  <text x="96" y="338" font-family="DejaVu Sans" font-size="35" fill="#55555c">{kicker}{arrow}</text>
  <text x="94" y="432" font-family="{hero_font}" font-weight="bold" font-size="{hero_size}" fill="#1d1d1f">{hero}</text>
  <text x="96" y="566" font-family="DejaVu Sans" font-size="33" fill="#55555c">{foot}</text>
</svg>"##
    )
}

/// Shrink the hero font so even a long line stays on one line. DejaVu Sans Mono
/// advances ~0.6 em per glyph (0.62 leaves a little slack); the proportional
/// face is narrower, so a sentence gets a tighter factor and a lower ceiling --
/// it is a phrase, not the address the card is about.
fn hero_font_size(hero: &str, blind: bool) -> f32 {
    let len = hero.chars().count().max(1) as f32;
    let avail = WIDTH as f32 - 2.0 * PAD;
    if blind {
        (avail / (0.52 * len)).clamp(28.0, 72.0)
    } else {
        (avail / (0.62 * len)).clamp(28.0, 92.0)
    }
}

/// Guard against an absurdly long hero line overflowing the card.
fn fit_hero(domain: &str) -> String {
    const MAX: usize = 40;
    if domain.chars().count() > MAX {
        let mut s: String = domain.chars().take(MAX - 1).collect();
        s.push('…');
        s
    } else {
        domain.to_string()
    }
}

/// Escape the three characters that would break SVG/XML text content. The domain
/// can be attacker-influenced, so this is a real (small) XSS/inject guard.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_nonempty_png() {
        let png = render_png(&Card {
            kicker: "Ephemeral redirect",
            hero: "example.com",
            blind: false,
            foot: "expires Jun 29, 2026 · 14:30 UTC",
        })
        .expect("render");
        // PNG magic number.
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert!(png.len() > 1000, "card png should be substantial");
    }

    #[test]
    fn a_blind_card_draws_no_destination_and_no_arrow() {
        let svg = build_svg(&Card {
            kicker: "One-time link",
            hero: "Shown when revealed",
            blind: true,
            foot: "expires Jun 29, 2026",
        });
        assert!(svg.contains("Shown when revealed"));
        // The arrow promises a destination; a blind card has none to point at.
        assert!(!svg.contains("8594"), "{svg}");
        // A sentence reads in the UI face, not the address face.
        assert!(!svg.contains("DejaVu Sans Mono"), "{svg}");
        // And it still rasterises.
        assert!(
            render_png(&Card {
                kicker: "One-time link",
                hero: "Shown when revealed",
                blind: true,
                foot: "expires Jun 29, 2026",
            })
            .is_some()
        );
    }

    #[test]
    fn long_domain_is_truncated() {
        let long = "a".repeat(60);
        let out = fit_hero(&long);
        assert_eq!(out.chars().count(), 40);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn dest_font_shrinks_for_long_domains() {
        assert!(hero_font_size("example.com", false) > hero_font_size(&"x".repeat(30), false));
        assert!(hero_font_size("x", false) <= 92.0);
        assert!(hero_font_size("x", true) <= 72.0);
    }

    #[test]
    fn xml_escapes_special_chars() {
        assert_eq!(xml_escape("a&b<c>"), "a&amp;b&lt;c&gt;");
    }
}
