//! QR code for a created link, drawn as a small SVG.
//!
//! The encoder is the `qrcode` crate with its default features off: it hands back
//! the module grid and nothing else, and this file draws it. The style is
//! "liquid": touching modules merge into one shape, a corner with nothing beside
//! it rounds off, and an inside corner gets a small fillet. The three finder
//! patterns are drawn apart as soft squares. Ink is the site's accent text blue
//! on a white tile in both themes, since an inverted code is the one phone
//! cameras miss; `#0062d8` on white is about 5.6:1, well inside what scanners read.
//!
//! It encodes only `<base_url><name>`, and the name is checked against the link
//! alphabet first, so the route cannot be used to mint a code for arbitrary text.
//! It deliberately does not look the name up: the code is a pure function of the
//! URL, which is public the moment it exists, so asking for it reveals nothing
//! about whether a link lives there and costs no database read.

use std::fmt::Write;

use qrcode::{Color, EcLevel, QrCode};

/// Light modules around the code. Four is what the standard asks for.
const QUIET: usize = 4;

/// The ink: `--accent-text` from app.css (light theme).
const INK: &str = "#0062d8";

/// Corner radius of a lone module's rounded corner (half a module: a dot).
const ROUND: f32 = 0.5;

/// Radius of the fillet in an inside corner.
const FILLET: f32 = 0.35;

/// How far a cell reaches into a dark neighbour. Shapes that merely touch leave
/// a hairline where anti-aliasing blends both edges; overlapping by a sliver
/// closes it.
const BLEED: f32 = 0.02;

/// Whether `name` could be a link name: ASCII letters, digits and inner hyphens,
/// at most 64 bytes. Mirrors what `generate_name` can produce, loosely.
pub fn is_plausible_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The SVG for `url`, or `None` if it does not fit a QR code (it always does for
/// a real link URL; the longest is well under 100 bytes).
pub fn svg(url: &str) -> Option<String> {
    // M (~15% recovery) keeps a four-word URL at version 3 or 4: large modules,
    // readable off a screen at arm's length.
    let code = QrCode::with_error_correction_level(url.as_bytes(), EcLevel::M).ok()?;
    let w = code.width() as i32;
    let colors = code.to_colors();
    let size = code.width() + 2 * QUIET;

    // The finder patterns: three 7x7 squares in the corners, drawn on their own.
    let in_finder = |x: i32, y: i32| ((x < 7 || x >= w - 7) && y < 7) || (x < 7 && y >= w - 7);
    let dark = |x: i32, y: i32| {
        (0..w).contains(&x)
            && (0..w).contains(&y)
            && colors[(y * w + x) as usize] == Color::Dark
            && !in_finder(x, y)
    };

    let q = QUIET as f32;
    let mut d = String::new();
    for y in 0..w {
        for x in 0..w {
            let (x0, y0) = (x as f32 + q, y as f32 + q);
            if dark(x, y) {
                let (l, r, u, b) = (
                    dark(x - 1, y),
                    dark(x + 1, y),
                    dark(x, y - 1),
                    dark(x, y + 1),
                );
                // A corner rounds only where neither neighbour beside it is dark.
                let rad = |a: bool, c: bool| if a || c { 0.0 } else { ROUND };
                let (tl, tr, br, bl) = (rad(l, u), rad(r, u), rad(r, b), rad(l, b));
                let e = |n: bool| if n { BLEED } else { 0.0 };
                let (x0, x1) = (x0 - e(l), x0 + 1.0 + e(r));
                let (y0, y1) = (y0 - e(u), y0 + 1.0 + e(b));
                let _ = write!(d, "M{},{}H{}", x0 + tl, y0, x1 - tr);
                arc(&mut d, tr, x1, y0 + tr);
                let _ = write!(d, "V{}", y1 - br);
                arc(&mut d, br, x1 - br, y1);
                let _ = write!(d, "H{}", x0 + bl);
                arc(&mut d, bl, x0, y1 - bl);
                let _ = write!(d, "V{}", y0 + tl);
                arc(&mut d, tl, x0 + tl, y0);
                d.push('Z');
            } else if !in_finder(x, y) {
                // A light cell with dark on both sides of a corner, and at the
                // diagonal, sits in an inside corner: fill it with a fillet that
                // reaches a sliver into both neighbours.
                let (f, e) = (FILLET, BLEED);
                let (x1, y1) = (x0 + 1.0, y0 + 1.0);
                if dark(x - 1, y) && dark(x, y - 1) && dark(x - 1, y - 1) {
                    let _ = write!(
                        d,
                        "M{},{}H{}V{}A{f},{f} 0 0 0 {},{}H{}Z",
                        x0 - e,
                        y0 - e,
                        x0 + f,
                        y0,
                        x0,
                        y0 + f,
                        x0 - e
                    );
                }
                if dark(x + 1, y) && dark(x, y - 1) && dark(x + 1, y - 1) {
                    let _ = write!(
                        d,
                        "M{},{}V{}H{}A{f},{f} 0 0 0 {},{}V{}Z",
                        x1 + e,
                        y0 - e,
                        y0 + f,
                        x1,
                        x1 - f,
                        y0,
                        y0 - e
                    );
                }
                if dark(x + 1, y) && dark(x, y + 1) && dark(x + 1, y + 1) {
                    let _ = write!(
                        d,
                        "M{},{}H{}V{}A{f},{f} 0 0 0 {},{}H{}Z",
                        x1 + e,
                        y1 + e,
                        x1 - f,
                        y1,
                        x1,
                        y1 - f,
                        x1 + e
                    );
                }
                if dark(x - 1, y) && dark(x, y + 1) && dark(x - 1, y + 1) {
                    let _ = write!(
                        d,
                        "M{},{}V{}H{}A{f},{f} 0 0 0 {},{}V{}Z",
                        x0 - e,
                        y1 + e,
                        y1 - f,
                        x0,
                        x0 + f,
                        y1,
                        y1 + e
                    );
                }
            }
        }
    }

    // Each finder: a ring (6 wide, one module thick) around a 3x3 centre, both
    // with soft corners.
    let mut eyes = String::new();
    for (ex, ey) in [(0, 0), (w - 7, 0), (0, w - 7)] {
        let (cx, cy) = (ex as f32 + q + 3.5, ey as f32 + q + 3.5);
        let _ = write!(
            eyes,
            "<rect x=\"{}\" y=\"{}\" width=\"6\" height=\"6\" rx=\"1.6\" fill=\"none\" stroke=\"{INK}\"/>\
             <rect x=\"{}\" y=\"{}\" width=\"3\" height=\"3\" rx=\"0.8\" fill=\"{INK}\"/>",
            cx - 3.0,
            cy - 3.0,
            cx - 1.5,
            cy - 1.5
        );
    }

    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {size} {size}\">\
         <rect width=\"{size}\" height=\"{size}\" fill=\"#fff\"/>\
         <path d=\"{d}\" fill=\"{INK}\"/>{eyes}</svg>"
    ))
}

/// A clockwise quarter arc of radius `r` to (`x`, `y`); nothing when `r` is 0,
/// which leaves a square corner.
fn arc(d: &mut String, r: f32, x: f32, y: f32) {
    if r > 0.0 {
        let _ = write!(d, "A{r},{r} 0 0 1 {x},{y}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_checked() {
        assert!(is_plausible_name("runnyDUSK"));
        assert!(is_plausible_name("half-baked"));
        assert!(!is_plausible_name(""));
        assert!(!is_plausible_name("-dash"));
        assert!(!is_plausible_name("a/b"));
        assert!(!is_plausible_name("a b"));
        assert!(!is_plausible_name(&"a".repeat(65)));
    }

    #[test]
    fn draws_a_square_with_a_quiet_zone() {
        let svg = svg("https://yuio.link/hazelOTTERdimKITE").unwrap();
        // The code's own width plus the light modules on each side.
        let code = QrCode::with_error_correction_level(
            "https://yuio.link/hazelOTTERdimKITE".as_bytes(),
            EcLevel::M,
        )
        .unwrap();
        let size = code.width() + 2 * QUIET;
        assert!(svg.contains(&format!("viewBox=\"0 0 {size} {size}\"")));
        // The top-left finder's ring sits half a module inside the quiet zone.
        assert!(svg.contains("<rect x=\"4.5\" y=\"4.5\" width=\"6\""));
    }
}
