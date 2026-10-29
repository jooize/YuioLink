"""Build the share card's fallback fonts from Noto.

usage: build.py NOTO_DIR CJK_DIR EMOJI_DIR OUT_DIR

The card renderer (resvg 0.44) draws only a font's default instance, and
Noto ships most scripts as variable fonts. Each font here becomes a static
Regular instance (the weight the card already draws in) with its hinting
dropped, since the card is rasterized at display sizes where resvg ignores
hints anyway. Every character and every layout feature is kept, so the
scripts still shape (Arabic joining, Indic conjuncts).

Output is deterministic for a given input: timestamps are kept from the
source fonts, never set to now.
"""

import shutil
import sys
from pathlib import Path

from fontTools.subset import Options, Subsetter
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

# Source file in NOTO_DIR, one per script the card can draw. The scripts are
# Unicode's Recommended and Limited Use sets (UTS #31 tables 4 and 7), the
# ones a browser shows as letters in a domain. Historic scripts are left out:
# a domain in one reads as punycode on the card, as it does in browsers.
NOTO = [
    "NotoSans.ttf",
    "NotoSansMono.ttf",
    "NotoSansAdlam.ttf",
    "NotoSansArabic.ttf",
    "NotoSansArmenian.ttf",
    "NotoSansBalinese.ttf",
    "NotoSansBamum.ttf",
    "NotoSansBatak-Regular.otf",
    "NotoSansBengali.ttf",
    "NotoSansCanadianAboriginal.ttf",
    "NotoSansChakma-Regular.otf",
    "NotoSansCham.ttf",
    "NotoSansCherokee.ttf",
    "NotoSansDevanagari.ttf",
    "NotoSansEthiopic.ttf",
    "NotoSansGeorgian.ttf",
    "NotoSansGujarati.ttf",
    "NotoSansGurmukhi.ttf",
    "NotoSansHanifiRohingya.ttf",
    "NotoSansHebrew.ttf",
    "NotoSansJavanese.ttf",
    "NotoSansKannada.ttf",
    "NotoSansKayahLi.ttf",
    "NotoSansKhmer.ttf",
    "NotoSansLao.ttf",
    "NotoSansLepcha-Regular.otf",
    "NotoSansLimbu-Regular.otf",
    "NotoSansLisu.ttf",
    "NotoSansMalayalam.ttf",
    "NotoSansMandaic-Regular.otf",
    "NotoSansMeeteiMayek.ttf",
    "NotoSansMiao-Regular.otf",
    "NotoSansMongolian-Regular.otf",
    "NotoSansMyanmar.ttf",
    "NotoSansNewa-Regular.otf",
    "NotoSansNewTaiLue.ttf",
    "NotoSansNKo-Regular.otf",
    "NotoSansOlChiki.ttf",
    "NotoSansOriya.ttf",
    "NotoSansOsage-Regular.otf",
    "NotoSansSaurashtra-Regular.otf",
    "NotoSansSinhala.ttf",
    "NotoSansSundanese.ttf",
    "NotoSansSylotiNagri-Regular.otf",
    "NotoSansSyriac.ttf",
    "NotoSansTaiLe-Regular.otf",
    "NotoSansTaiTham.ttf",
    "NotoSansTaiViet-Regular.otf",
    "NotoSansTamil.ttf",
    "NotoSansTelugu.ttf",
    "NotoSansThaana.ttf",
    "NotoSansThai.ttf",
    "NotoSansTifinagh-Regular.otf",
    "NotoSansVai-Regular.otf",
    "NotoSansWancho-Regular.otf",
    "NotoSansYi-Regular.otf",
    # Noto has no sans for these two.
    "NotoSerifNPHmong.ttf",
    "NotoSerifTibetan.ttf",
]

# Han, kana, Hangul and Bopomofo: one collection, a face per region, sharing
# one set of outlines. Already static, so it is copied as it is.
CJK = "NotoSansCJK-Regular.ttc"

# Monochrome emoji, drawn in the text's color like any other letter.
EMOJI = "NotoEmoji.ttf"


def static_regular(src: Path, out: Path) -> None:
    font = TTFont(src, recalcTimestamp=False)
    if "fvar" in font:
        pins = {"wght": 400, "wdth": 100}
        axes = {a.axisTag for a in font["fvar"].axes}
        font = instancer.instantiateVariableFont(
            font, {k: v for k, v in pins.items() if k in axes}
        )
    options = Options()
    options.layout_features = ["*"]
    options.layout_scripts = ["*"]
    options.name_IDs = ["*"]
    options.hinting = False
    options.glyph_names = False
    options.notdef_outline = True
    options.recalc_timestamp = False
    subsetter = Subsetter(options)
    subsetter.populate(unicodes=font.getBestCmap().keys())
    subsetter.subset(font)
    font.save(out, reorderTables=True)


def out_name(src: str) -> str:
    stem, ext = src.rsplit(".", 1)
    if not stem.endswith("-Regular"):
        stem += "-Regular"
    return f"{stem}.{ext}"


def main() -> None:
    if len(sys.argv) != 5:
        sys.exit(__doc__.strip().splitlines()[2])
    noto, cjk, emoji, out = (Path(a) for a in sys.argv[1:])
    out.mkdir(parents=True, exist_ok=True)
    for name in NOTO:
        static_regular(noto / name, out / out_name(name))
    static_regular(emoji / EMOJI, out / out_name(EMOJI))
    shutil.copyfile(cjk / CJK, out / CJK)


if __name__ == "__main__":
    main()
