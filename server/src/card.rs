//! Server-side share-card (og:image) rendering.
//!
//! Builds an SVG card from a template and rasterises it to PNG with resvg — pure
//! Rust, no system libraries (it builds `default-features = false, features =
//! ["text"]`, so the SVG-embedded-raster decoders and system-font loading are
//! dropped). The fonts are embedded so the card renders identically with no
//! system fonts installed, e.g. in a slim container. An ordinary card shows the
//! destination domain — decided, so a shared link reads as trustworthy.
//!
//! DejaVu draws the card's own words and any domain in Latin, Greek or
//! Cyrillic. A domain in another script draws in Noto, one font per script
//! (see [`hero_runs`]), so `例子.中国` reads as letters rather than as empty
//! boxes. A domain with a character no bundled font has is shown as punycode
//! instead (`xn--fsqu00a.xn--fiqs8s`): the card never draws a box where a
//! letter should be.
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

use icu_properties::props::{DefaultIgnorableCodePoint, ExtendedPictographic, Script};
use icu_properties::{CodePointMapData, CodePointSetData};
use resvg::tiny_skia;
use resvg::usvg;
use resvg::usvg::fontdb::{Database, Family, ID, Query, Source};

/// Embedded fonts (see assets/fonts/LICENSE.txt). Sans for the wordmark/kicker/
/// foot, Mono for the destination domain (it reads as a precise address).
const SANS_FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
const MONO_FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");

/// The Noto fonts for every other script, built by tools/card-fonts/build.bash
/// (see assets/fonts/noto/LICENSE.txt). All Regular, the weight the card draws.
macro_rules! noto {
    ($($file:literal),* $(,)?) => {
        &[$(($file, include_bytes!(concat!("../assets/fonts/noto/", $file)) as &[u8])),*]
    };
}
const NOTO_FONTS: &[(&str, &[u8])] = noto![
    // The scripts with the most domains first: when two fonts could draw a
    // run, the first one does.
    "NotoSansArabic-Regular.ttf",
    "NotoSansHebrew-Regular.ttf",
    "NotoSansDevanagari-Regular.ttf",
    "NotoSansBengali-Regular.ttf",
    "NotoSansTamil-Regular.ttf",
    "NotoSansTelugu-Regular.ttf",
    "NotoSansKannada-Regular.ttf",
    "NotoSansMalayalam-Regular.ttf",
    "NotoSansGujarati-Regular.ttf",
    "NotoSansGurmukhi-Regular.ttf",
    "NotoSansOriya-Regular.ttf",
    "NotoSansSinhala-Regular.ttf",
    "NotoSansThai-Regular.ttf",
    "NotoSansLao-Regular.ttf",
    "NotoSansKhmer-Regular.ttf",
    "NotoSansMyanmar-Regular.ttf",
    "NotoSansGeorgian-Regular.ttf",
    "NotoSansArmenian-Regular.ttf",
    "NotoSansEthiopic-Regular.ttf",
    "NotoSansThaana-Regular.ttf",
    "NotoSerifTibetan-Regular.ttf",
    "NotoSansAdlam-Regular.ttf",
    "NotoSansBalinese-Regular.ttf",
    "NotoSansBamum-Regular.ttf",
    "NotoSansBatak-Regular.otf",
    "NotoSansCanadianAboriginal-Regular.ttf",
    "NotoSansChakma-Regular.otf",
    "NotoSansCham-Regular.ttf",
    "NotoSansCherokee-Regular.ttf",
    "NotoSansHanifiRohingya-Regular.ttf",
    "NotoSansJavanese-Regular.ttf",
    "NotoSansKayahLi-Regular.ttf",
    "NotoSansLepcha-Regular.otf",
    "NotoSansLimbu-Regular.otf",
    "NotoSansLisu-Regular.ttf",
    "NotoSansMandaic-Regular.otf",
    "NotoSansMeeteiMayek-Regular.ttf",
    "NotoSansMiao-Regular.otf",
    "NotoSansMongolian-Regular.otf",
    "NotoSansNewa-Regular.otf",
    "NotoSansNewTaiLue-Regular.ttf",
    "NotoSansNKo-Regular.otf",
    "NotoSansOlChiki-Regular.ttf",
    "NotoSansOsage-Regular.otf",
    "NotoSansSaurashtra-Regular.otf",
    "NotoSansSundanese-Regular.ttf",
    "NotoSansSylotiNagri-Regular.otf",
    "NotoSansSyriac-Regular.ttf",
    "NotoSansTaiLe-Regular.otf",
    "NotoSansTaiTham-Regular.ttf",
    "NotoSansTaiViet-Regular.otf",
    "NotoSansTifinagh-Regular.otf",
    "NotoSansVai-Regular.otf",
    "NotoSansWancho-Regular.otf",
    "NotoSansYi-Regular.otf",
    "NotoSerifNPHmong-Regular.ttf",
    // The faces with a role of their own, found by family name below.
    "NotoSans-Regular.ttf",
    "NotoSansMono-Regular.ttf",
    "NotoSansCJK-Regular.ttc",
    "NotoEmoji-Regular.ttf",
];

const SANS: &str = "DejaVu Sans";
const MONO: &str = "DejaVu Sans Mono";
/// Latin, Greek and Cyrillic letters DejaVu lacks: still a monospace.
const MONO_WIDE: &str = "Noto Sans Mono";
/// Punctuation a script's own face lacks: the Latin its letters were drawn
/// beside, so a dot in Thai is no wider than the letters around it.
const NOTO_SANS: &str = "Noto Sans";
const EMOJI: &str = "Noto Emoji";

/// Standard large-summary card size.
const WIDTH: u32 = 1200;
const HEIGHT: u32 = 630;
/// Horizontal padding; the destination must fit within `WIDTH - 2*PAD`.
const PAD: f32 = 96.0;
const AVAIL: f32 = WIDTH as f32 - 2.0 * PAD;

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
    let fonts = fonts();
    let (runs, size) = fit_hero(fonts, card)?;
    let tree = parse(fonts, &build_svg(card, &runs, size))?;
    let mut pixmap = tiny_skia::Pixmap::new(WIDTH, HEIGHT)?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

fn parse(fonts: &Fonts, svg: &str) -> Option<usvg::Tree> {
    let opt = usvg::Options {
        // Fallback family for any text we didn't explicitly set.
        font_family: SANS.to_string(),
        fontdb: fonts.db.clone(),
        // No glyph-by-glyph fallback. usvg shapes the whole line once per
        // span font, and a fallback face that has every character replaces
        // the line's glyphs wholesale, so Latin beside Han would draw in the
        // CJK face. [`hero_runs`] already gave each span a face that has its
        // characters.
        font_resolver: usvg::FontResolver {
            select_fallback: Box::new(|_, _, _| None),
            ..usvg::FontResolver::default()
        },
        ..Default::default()
    };
    usvg::Tree::from_str(svg, &opt).ok()
}

/// The bundled fonts, parsed once into a shared, immutable database, and the
/// faces the hero picks from.
struct Fonts {
    db: Arc<Database>,
    mono: ID,
    mono_wide: ID,
    noto_sans: ID,
    emoji: ID,
    /// One face per CJK region, all sharing the collection's outlines.
    cjk: [(Region, ID); 5],
    /// Every other Noto face, in [`NOTO_FONTS`] order.
    scripts: Vec<ID>,
}

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut db = Database::new();
        // Borrowed from the binary rather than copied: a font is read only
        // when a card needs it, so the CJK collection costs no memory until
        // a CJK domain is shared.
        for data in [SANS_FONT, MONO_FONT]
            .into_iter()
            .chain(NOTO_FONTS.iter().map(|f| f.1))
        {
            db.load_font_source(Source::Binary(Arc::new(data)));
        }
        let face = |family: &str| {
            db.query(&Query {
                families: &[Family::Name(family)],
                ..Query::default()
            })
            .unwrap_or_else(|| panic!("bundled font {family} is missing"))
        };
        let mono = face(MONO);
        let mono_wide = face(MONO_WIDE);
        let noto_sans = face(NOTO_SANS);
        let emoji = face(EMOJI);
        let cjk = Region::ALL.map(|r| (r, face(r.family())));
        let sans = face(SANS);
        let roles = [sans, mono, mono_wide, noto_sans, emoji];
        let scripts = db
            .faces()
            .map(|f| f.id)
            .filter(|id| !roles.contains(id) && !cjk.iter().any(|(_, c)| c == id))
            .collect();
        Fonts {
            db: Arc::new(db),
            mono,
            mono_wide,
            noto_sans,
            emoji,
            cjk,
            scripts,
        }
    })
}

impl Fonts {
    fn has(&self, face: ID, c: char) -> bool {
        self.db
            .with_face_data(face, |data, index| {
                ttf_parser::Face::parse(data, index)
                    .ok()
                    .and_then(|f| f.glyph_index(c))
                    .is_some()
            })
            .unwrap_or(false)
    }

    fn family(&self, face: ID) -> &str {
        let f = self.db.face(face).expect("face from this database");
        let english = f
            .families
            .iter()
            .find(|(_, lang)| *lang == usvg::fontdb::Language::English_UnitedStates);
        &english.unwrap_or(&f.families[0]).0
    }
}

/// Which region's glyphs a Han character takes. One character can be drawn
/// differently in Japan, Korea, mainland China, Taiwan and Hong Kong, and a
/// reader notices the wrong one.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Region {
    Jp,
    Kr,
    Sc,
    Tc,
    Hk,
}

impl Region {
    const ALL: [Region; 5] = [Region::Jp, Region::Kr, Region::Sc, Region::Tc, Region::Hk];

    fn family(self) -> &'static str {
        match self {
            Region::Jp => "Noto Sans CJK JP",
            Region::Kr => "Noto Sans CJK KR",
            Region::Sc => "Noto Sans CJK SC",
            Region::Tc => "Noto Sans CJK TC",
            Region::Hk => "Noto Sans CJK HK",
        }
    }

    /// Kana mean Japanese, Hangul Korean and Bopomofo Taiwanese, whatever
    /// the domain ends in. Han alone follows the top-level domain, and
    /// mainland forms are the default: most Han-only domains are Chinese.
    fn of(domain: &str) -> Region {
        let scripts: Vec<Script> = domain.chars().map(script).collect();
        if scripts
            .iter()
            .any(|s| matches!(*s, Script::Hiragana | Script::Katakana))
        {
            return Region::Jp;
        }
        if scripts.contains(&Script::Hangul) {
            return Region::Kr;
        }
        if scripts.contains(&Script::Bopomofo) {
            return Region::Tc;
        }
        match domain
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "jp" => Region::Jp,
            "kr" | "한국" => Region::Kr,
            "tw" | "台灣" | "台湾" | "mo" | "澳門" | "澳门" => Region::Tc,
            "hk" | "香港" => Region::Hk,
            _ => Region::Sc,
        }
    }
}

fn script(c: char) -> Script {
    CodePointMapData::<Script>::new().get(c)
}

/// What decides a character's font. Digits, dots and hyphens have none of
/// their own and go with their neighbors.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Lgc,
    Cjk,
    Emoji,
    Other(Script),
}

fn kind(fonts: &Fonts, c: char) -> Option<Kind> {
    // A symbol DejaVu has (❤, ☺) stays text, as a browser draws it without
    // U+FE0F, which a domain never keeps (UTS #46 maps it away).
    if CodePointSetData::new::<ExtendedPictographic>().contains(c)
        && !fonts.has(fonts.mono, c)
        && fonts.has(fonts.emoji, c)
    {
        return Some(Kind::Emoji);
    }
    match script(c) {
        Script::Common | Script::Inherited => None,
        Script::Latin | Script::Greek | Script::Cyrillic => Some(Kind::Lgc),
        Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul | Script::Bopomofo => {
            Some(Kind::Cjk)
        }
        s => Some(Kind::Other(s)),
    }
}

/// A stretch of the hero drawn in one face.
#[derive(Debug, PartialEq)]
struct Run {
    face: ID,
    text: String,
}

/// Split a domain into runs, one face each: DejaVu Sans Mono for Latin,
/// Greek and Cyrillic (Noto Sans Mono where DejaVu lacks a letter), the
/// region's CJK face for Han, kana and Hangul, Noto Emoji for emoji, and
/// the first Noto face that has every letter for any other script. A digit,
/// dot or hyphen joins the run before it; where that face lacks it, it takes
/// the Latin that face was designed beside (Noto Sans for a Noto script,
/// DejaVu Sans Mono for an address in Latin).
///
/// Picking the face per script, rather than letting the renderer fall back
/// glyph by glyph, keeps each word in one design and lets Arabic and Indic
/// letters shape together. `None` when some character has no face at all.
fn hero_runs(fonts: &Fonts, hero: &str) -> Option<Vec<Run>> {
    let ignorable = CodePointSetData::new::<DefaultIgnorableCodePoint>();
    let chars: Vec<(char, Option<Kind>)> = hero.chars().map(|c| (c, kind(fonts, c))).collect();
    let mut current = chars.iter().find_map(|(_, k)| *k).unwrap_or(Kind::Lgc);

    // Consecutive characters of one kind, neutral ones resolved.
    let mut groups: Vec<(Kind, Vec<(char, bool)>)> = Vec::new();
    for &(c, k) in &chars {
        if let Some(k) = k {
            current = k;
        }
        match groups.last_mut() {
            Some((g, members)) if *g == current => members.push((c, k.is_none())),
            _ => groups.push((current, vec![(c, k.is_none())])),
        }
    }

    let region = Region::of(hero);
    let mut runs: Vec<Run> = Vec::new();
    for (k, members) in groups {
        let covers = |face: &ID| {
            members
                .iter()
                .all(|&(c, neutral)| neutral || ignorable.contains(c) || fonts.has(*face, c))
        };
        let cjk = fonts
            .cjk
            .iter()
            .find(|(r, _)| *r == region)
            .map(|(_, id)| *id);
        let face = match k {
            Kind::Lgc => [fonts.mono, fonts.mono_wide].into_iter().find(covers),
            Kind::Cjk => cjk.filter(covers),
            Kind::Emoji => Some(fonts.emoji).filter(covers),
            Kind::Other(_) => fonts.scripts.iter().copied().find(covers),
        }?;
        for (c, neutral) in members {
            let face = if ignorable.contains(c) || fonts.has(face, c) {
                face
            } else if !neutral {
                return None;
            } else {
                let noto = match k {
                    Kind::Lgc | Kind::Emoji => None,
                    Kind::Cjk | Kind::Other(_) => Some(fonts.noto_sans),
                };
                noto.into_iter()
                    .chain([fonts.mono])
                    .find(|f| fonts.has(*f, c))?
            };
            match runs.last_mut() {
                Some(run) if run.face == face => run.text.push(c),
                _ => runs.push(Run {
                    face,
                    text: c.to_string(),
                }),
            }
        }
    }
    Some(runs)
}

/// The hero's runs and font size. The size is measured, not guessed from the
/// character count: a Han character is twice as wide as a Latin one, and an
/// Arabic word narrower than its letters. A domain no font can draw becomes
/// punycode; one too long even at the smallest size is cut with an ellipsis.
fn fit_hero(fonts: &Fonts, card: &Card) -> Option<(Vec<Run>, f32)> {
    let (min, max) = if card.blind {
        (28.0, 72.0)
    } else {
        (28.0, 92.0)
    };
    let mut hero: Vec<char> = if card.blind {
        card.hero.chars().collect()
    } else {
        cap_chars(card.hero).chars().collect()
    };
    if !card.blind && hero_runs(fonts, &String::from_iter(&hero)).is_none() {
        let ascii = idna::domain_to_ascii(card.hero).ok()?;
        hero = cap_chars(&ascii).chars().collect();
    }
    let mut cut = false;
    loop {
        let mut text = String::from_iter(&hero);
        if cut {
            text.push('…');
        }
        let runs = if card.blind {
            vec![Run {
                face: fonts.db.query(&Query {
                    families: &[Family::Name(SANS)],
                    ..Query::default()
                })?,
                text,
            }]
        } else {
            hero_runs(fonts, &text)?
        };
        // Measured at 100 px, then scaled: text width grows in step with
        // the font size.
        let width = hero_width(fonts, card, &runs)?;
        let size = (AVAIL / width * 100.0).clamp(min, max);
        if width * size / 100.0 <= AVAIL || hero.len() <= 1 {
            return Some((runs, size));
        }
        let keep = (hero.len() as f32 * AVAIL / (width * min / 100.0)) as usize;
        hero.truncate(keep.min(hero.len() - 1).max(1));
        cut = true;
    }
}

fn hero_width(fonts: &Fonts, card: &Card, runs: &[Run]) -> Option<f32> {
    let tree = parse(fonts, &build_svg(card, runs, 100.0))?;
    Some(tree.node_by_id("hero")?.abs_bounding_box().width())
}

fn build_svg(card: &Card, runs: &[Run], hero_size: f32) -> String {
    let fonts = fonts();
    let hero: String = runs
        .iter()
        .map(|r| {
            format!(
                "<tspan font-family=\"'{}'\">{}</tspan>",
                xml_escape(fonts.family(r.face)),
                xml_escape(&r.text)
            )
        })
        .collect();
    let kicker = xml_escape(card.kicker);
    let foot = xml_escape(card.foot);
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
  <text x="96" y="117" font-family="{SANS}" font-weight="bold" font-size="44" fill="#1d1d1f">Yuio<tspan fill="#007aff">Link</tspan></text>
  <text x="96" y="338" font-family="{SANS}" font-size="35" fill="#55555c">{kicker}{arrow}</text>
  <text id="hero" x="94" y="432" font-weight="bold" font-size="{hero_size}" fill="#1d1d1f">{hero}</text>
  <text x="96" y="566" font-family="{SANS}" font-size="33" fill="#55555c">{foot}</text>
</svg>"##
    )
}

/// Guard against an absurdly long hero line: past 40 characters even a
/// measured size would be too small to read.
fn cap_chars(domain: &str) -> String {
    const MAX: usize = 40;
    if domain.chars().count() > MAX {
        let mut s: String = domain.chars().take(MAX - 1).collect();
        s.push('…');
        s
    } else {
        domain.to_string()
    }
}

/// Escape the characters that would break SVG/XML text content or a quoted
/// attribute. The domain can be attacker-influenced, so this is a real
/// (small) XSS/inject guard.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn families(hero: &str) -> Option<Vec<(String, String)>> {
        let fonts = fonts();
        hero_runs(fonts, hero).map(|runs| {
            runs.into_iter()
                .map(|r| (fonts.family(r.face).to_string(), r.text))
                .collect()
        })
    }

    fn run(family: &str, text: &str) -> (String, String) {
        (family.to_string(), text.to_string())
    }

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
        let card = Card {
            kicker: "One-time link",
            hero: "Shown when revealed",
            blind: true,
            foot: "expires Jun 29, 2026",
        };
        let (runs, size) = fit_hero(fonts(), &card).expect("fits");
        let svg = build_svg(&card, &runs, size);
        assert!(svg.contains("Shown when revealed"));
        // The arrow promises a destination; a blind card has none to point at.
        assert!(!svg.contains("8594"), "{svg}");
        // A sentence reads in the UI face, not the address face.
        assert!(!svg.contains(MONO), "{svg}");
        // And it still rasterises.
        assert!(render_png(&card).is_some());
    }

    #[test]
    fn every_built_font_is_embedded() {
        // build.bash writes the directory; this list must name all of it.
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts/noto");
        let mut built: Vec<String> = std::fs::read_dir(dir)
            .expect("fonts directory")
            .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
            .filter(|n| n != "LICENSE.txt")
            .collect();
        built.sort();
        let mut embedded: Vec<String> = NOTO_FONTS.iter().map(|f| f.0.to_string()).collect();
        embedded.sort();
        assert_eq!(built, embedded);
    }

    #[test]
    fn a_latin_domain_stays_in_dejavu() {
        assert_eq!(
            families("bücher-münchen.de"),
            Some(vec![run(MONO, "bücher-münchen.de")])
        );
        assert_eq!(families("δοκιμή.gr"), Some(vec![run(MONO, "δοκιμή.gr")]));
    }

    #[test]
    fn each_script_takes_its_own_face_and_neighbors_take_the_dot() {
        assert_eq!(
            families("مثال.مصر"),
            Some(vec![run("Noto Sans Arabic", "مثال.مصر")])
        );
        assert_eq!(
            families("उदाहरण.com"),
            Some(vec![
                run("Noto Sans Devanagari", "उदाहरण."),
                run(MONO, "com")
            ])
        );
        assert_eq!(
            families("café-東京.com"),
            Some(vec![
                run(MONO, "café-"),
                run("Noto Sans CJK SC", "東京."),
                run(MONO, "com")
            ])
        );
    }

    #[test]
    fn han_takes_the_region_of_its_domain() {
        assert_eq!(Region::of("例子.中国"), Region::Sc);
        assert_eq!(Region::of("東京.jp"), Region::Jp);
        assert_eq!(Region::of("ドメイン名例.com"), Region::Jp);
        assert_eq!(Region::of("도메인.com"), Region::Kr);
        assert_eq!(Region::of("台北.tw"), Region::Tc);
        assert_eq!(Region::of("香港.hk"), Region::Hk);
        assert_eq!(families("東京.jp").unwrap()[0].0, "Noto Sans CJK JP");
    }

    #[test]
    fn emoji_draw_from_the_emoji_face() {
        assert_eq!(families("i❤.ws"), Some(vec![run(MONO, "i❤.ws")]));
        assert_eq!(
            families("i😀.ws"),
            Some(vec![run(MONO, "i"), run(EMOJI, "😀"), run(MONO, ".ws")])
        );
    }

    #[test]
    fn a_domain_no_font_can_draw_falls_back_to_punycode() {
        // Cuneiform is historic: no bundled font has it.
        let domain = "𒀀𒀁.com";
        assert_eq!(families(domain), None);
        let card = Card {
            kicker: "Ephemeral redirect",
            hero: domain,
            blind: false,
            foot: "",
        };
        let (runs, _) = fit_hero(fonts(), &card).expect("fits");
        let text: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(text, idna::domain_to_ascii(domain).unwrap());
        assert!(text.starts_with("xn--"), "{text}");
    }

    #[test]
    fn long_domain_is_truncated() {
        let long = "a".repeat(60);
        let out = cap_chars(&long);
        assert_eq!(out.chars().count(), 40);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn the_hero_size_follows_its_measured_width() {
        let size = |hero: &str| {
            let card = Card {
                kicker: "",
                hero,
                blind: false,
                foot: "",
            };
            fit_hero(fonts(), &card).expect("fits").1
        };
        assert_eq!(size("example.com"), 92.0);
        assert!(size(&"x".repeat(30)) < 92.0);
        // Twenty Han characters are as wide as forty Latin letters.
        let han = size("中国互联网络信息中心域名注册服务机构.中国");
        assert!(han < size("abcdefghijklmnopqrst.com"), "{han}");
        let fonts = fonts();
        let card = Card {
            kicker: "",
            hero: "中国互联网络信息中心域名注册服务机构.中国",
            blind: false,
            foot: "",
        };
        let (runs, size) = fit_hero(fonts, &card).unwrap();
        let width = hero_width(fonts, &card, &runs).unwrap() * size / 100.0;
        assert!(width <= AVAIL, "{width}");
    }

    #[test]
    fn a_hero_too_wide_at_the_smallest_size_is_cut() {
        let hero = "中".repeat(40);
        let card = Card {
            kicker: "",
            hero: &hero,
            blind: false,
            foot: "",
        };
        let fonts = fonts();
        let (runs, size) = fit_hero(fonts, &card).unwrap();
        assert_eq!(size, 28.0);
        let text: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert!(text.ends_with('…'), "{text}");
        let width = hero_width(fonts, &card, &runs).unwrap() * size / 100.0;
        assert!(width <= AVAIL, "{width}");
    }

    /// Writes a card per sample domain to `$CARD_SPECIMENS`, for looking at
    /// the fonts. `cargo test -p yuiolink-server card_specimens -- --ignored`
    #[test]
    #[ignore]
    fn card_specimens() {
        let dir = std::env::var("CARD_SPECIMENS").expect("set CARD_SPECIMENS to a directory");
        for (file, hero) in SPECIMENS {
            let png = render_png(&Card {
                kicker: "Ephemeral redirect",
                hero,
                blind: false,
                foot: "expires Jun 29, 2026 · 14:30 UTC",
            })
            .expect("render");
            std::fs::write(format!("{dir}/{file}.png"), png).expect("write");
        }
    }

    const SPECIMENS: &[(&str, &str)] = &[
        ("latin", "example.com"),
        ("german", "bücher-münchen.de"),
        ("greek", "δοκιμή.gr"),
        ("russian", "пример.рф"),
        ("chinese", "例子.中国"),
        ("japanese", "ドメイン名例.jp"),
        ("korean", "도메인.한국"),
        ("arabic", "مثال.مصر"),
        ("hebrew", "דוגמה.ישראל"),
        ("hindi", "उदाहरण.भारत"),
        ("tamil", "எடுத்துக்காட்டு.இந்தியா"),
        ("thai", "ตัวอย่าง.ไทย"),
        ("georgian", "მაგალითი.გე"),
        ("armenian", "օրինակ.հայ"),
        ("sinhala", "උදාහරණ.ලංකා"),
        ("amharic", "ምሳሌ.et"),
        ("emoji", "i😀.ws"),
        ("heart", "i❤.ws"),
        ("mixed", "café-東京.com"),
        ("devanagari-com", "उदाहरण.com"),
        ("cuneiform", "𒀀𒀁.com"),
        ("long-han", "中国互联网络信息中心域名注册服务机构.中国"),
    ];

    #[test]
    fn xml_escapes_special_chars() {
        assert_eq!(xml_escape("a&b<c>\"'"), "a&amp;b&lt;c&gt;&quot;&apos;");
    }
}
