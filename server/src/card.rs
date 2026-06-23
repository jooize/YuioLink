//! Server-side share-card (og:image) rendering.
//!
//! Builds an SVG card from a template and rasterises it to PNG with resvg — pure
//! Rust, no system libraries (it builds `default-features = false, features =
//! ["text"]`, so the SVG-embedded-raster decoders and system-font loading are
//! dropped). Two DejaVu fonts are embedded so the card renders identically with no
//! system fonts installed, e.g. in a slim container. The card always shows the
//! destination domain — decided, so a shared link reads as trustworthy.

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
    /// "Ephemeral redirect" or "One-time redirect".
    pub kicker: &'a str,
    /// The destination's registrable domain, shown big.
    pub domain: &'a str,
    /// e.g. "expires Jun 29, 2026 · may change after".
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
    let domain = xml_escape(&fit_domain(card.domain));
    let kicker = xml_escape(card.kicker);
    let foot = xml_escape(card.foot);
    let dest_size = dest_font_size(card.domain);
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
    <linearGradient id="dot" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0a84ff"/>
      <stop offset="1" stop-color="#007aff"/>
    </linearGradient>
  </defs>
  <rect width="{WIDTH}" height="{HEIGHT}" fill="url(#bg)"/>
  <rect width="{WIDTH}" height="{HEIGHT}" fill="url(#glow)"/>
  <rect x="96" y="92" width="30" height="30" rx="8" fill="url(#dot)"/>
  <text x="140" y="117" font-family="DejaVu Sans" font-weight="bold" font-size="40" fill="#1d1d1f">YuioLink</text>
  <text x="96" y="338" font-family="DejaVu Sans" font-size="33" fill="#6e6e73">{kicker} <tspan fill="#007aff">&#8594;</tspan></text>
  <text x="94" y="432" font-family="DejaVu Sans Mono" font-weight="bold" font-size="{dest_size}" fill="#1d1d1f">{domain}</text>
  <text x="96" y="566" font-family="DejaVu Sans" font-size="31" fill="#6e6e73">{foot}</text>
</svg>"##
    )
}

/// Shrink the destination font so even a long domain stays on one line. DejaVu
/// Sans Mono advances ~0.6 em per glyph; 0.62 leaves a little slack.
fn dest_font_size(domain: &str) -> f32 {
    let len = domain.chars().count().max(1) as f32;
    let avail = WIDTH as f32 - 2.0 * PAD;
    (avail / (0.62 * len)).clamp(28.0, 92.0)
}

/// Guard against an absurdly long registrable domain overflowing the card.
fn fit_domain(domain: &str) -> String {
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
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_nonempty_png() {
        let png = render_png(&Card {
            kicker: "Ephemeral redirect",
            domain: "example.com",
            foot: "expires Jun 29, 2026 · may change after",
        })
        .expect("render");
        // PNG magic number.
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert!(png.len() > 1000, "card png should be substantial");
    }

    #[test]
    fn long_domain_is_truncated() {
        let long = "a".repeat(60);
        let out = fit_domain(&long);
        assert_eq!(out.chars().count(), 40);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn dest_font_shrinks_for_long_domains() {
        assert!(dest_font_size("example.com") > dest_font_size(&"x".repeat(30)));
        assert!(dest_font_size("x") <= 92.0);
    }

    #[test]
    fn xml_escapes_special_chars() {
        assert_eq!(xml_escape("a&b<c>"), "a&amp;b&lt;c&gt;");
    }
}
