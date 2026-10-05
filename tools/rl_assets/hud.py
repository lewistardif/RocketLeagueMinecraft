"""Rocket League's boost meter (the HUD's bottom-right gauge) from the decrypted UI packages.

The HUD is a Scaleform movie (`GFX_Hud_SF`, export `GFX_Hud.HUD`, source `Hud.swf`, 1120 x 720
stage). The boost meter is its `BoostMeterViewMovie` symbol (AS3 class `tagame.hud.BoostMeterView`),
placed on the HUD's root timeline as `boostMeterView`. Its pieces:

* `backgroundMovieClip` (texture `BoostMeter_Background`) and `glowMovieClip` (`BoostMeter_Glow`);
* `fillProgressBar` / `fillProgressBarTinted`: two 101-frame `FrameBasedProgressBar` timelines, each
  frame a wedge polygon clipping `BoostMeter_Fill` / `BoostMeter_FillTintablePortion` (frame 101
  shows the whole bitmap);
* `backgroundTextField` / `boostTextField` (font alias `$NumbersWideFont` = "Dashboard Numbers Wide")
  and `boostLabel` ("BOOST", `$HeaderFont` = "Bourgeois Medium"). The font aliases are resolved
  from the font library movie (`GFX_Fonts_SF`, `Fonts_EFIGS`), whose sample text fields are listed
  in the same order as its labels (SMALL, SMALL BOLD, NORMAL, HEADER, HEADER THIN, NUMBERS WIDE).

What it does at runtime (colour transforms, the 3D tilt and its perspective, the tweens) is
ActionScript, ported by hand into the hosts (`crates/rl_car_bevy/src/hud.rs`, `CarHud.java`).

Writes into `<out>/hud/`:
  boost_meter.json        layout (movie px, y down), wedge polygons, text fields, glyph metrics
  boost_*.png             the four textures, straight alpha, gamma space (as Scaleform uses them)
  font_numbers.png, font_header.png
                          glyph atlases: R = coverage, G / B = the coverage blurred like the text
                          field's GlowFilter (blur 6 / 8, two box passes), A = 255
"""

from __future__ import annotations

import json
import struct
from pathlib import Path

import numpy as np
from PIL import Image

import swf
from ue3 import Package

HUD_PACKAGE, HUD_MOVIE = "GFX_Hud_SF", "GFX_Hud.HUD"
FONT_PACKAGE, FONT_MOVIE = "GFX_Fonts_SF", "GFX_Fonts.Fonts_EFIGS"
PACKAGES = [HUD_PACKAGE, FONT_PACKAGE]
TEXTURES = {
    "BoostMeter_Background": "boost_background.png",
    "BoostMeter_Glow": "boost_glow.png",
    "BoostMeter_Fill": "boost_fill.png",
    "BoostMeter_FillTintablePortion": "boost_fill_tinted.png",
}
# Font library sample order -> alias (see the module docstring).
FONT_ALIASES = ["$SmallFont", "$SmallBoldFont", "$NormalFont", "$HeaderFont", "$HeaderThinFont", "$NumbersWideFont", "$SymbolReplaceFont"]
GLYPH_SETS = {"$NumbersWideFont": "0123456789", "$HeaderFont": "ABCDEFGHIJKLMNOPQRSTUVWXYZ"}
ATLAS_SCALE = 4.0  # atlas texels per movie px at the text field's font size (sharp up to 2880p)
GLOW_BLURS = (6.0, 8.0)  # BoostMeterView.GLOW_NORMAL / GLOW_MAX (and the placed filter, 6)
EM = 1024 * 20  # DefineFont3 units per em


def swf_movie(pkg: Package, path: str) -> swf.Movie:
    e = pkg.export_at(path)
    if e is None:
        raise SystemExit(f"{path} not found in {pkg.path.name}")
    return swf.parse(bytes.fromhex(pkg.properties(e)["RawData"]["__raw__"]))


def texture_png(pkg: Package, path: str) -> Image.Image:
    """A cooked UI Texture2D with its single mip stored inline (DXT5 or BGRA8)."""
    e = pkg.export_at(path)
    props = pkg.properties(e)
    w, h, fmt = props["SizeX"], props["SizeY"], props["Format"]
    size = {"PF_DXT5": w * h, "PF_DXT1": w * h // 2, "PF_A8R8G8B8": w * h * 4}[fmt]
    d = pkg.data[e.offset:e.offset + e.size]
    end = d.rfind(struct.pack("<ii", w, h))  # mip 0 data is followed by its size
    if end < 0 or struct.unpack_from("<ii", d, end - size - 8) != (size, size):
        raise SystemExit(f"{path}: no inline mip data (streamed textures are not supported here)")
    data = d[end - size:end]
    if fmt == "PF_A8R8G8B8":
        return Image.frombytes("RGBA", (w, h), data, "raw", "BGRA")
    return Image.frombytes("RGBA", (w, h), data, "bcn", 3 if fmt == "PF_DXT5" else 1)


# ------------------------------------------------------------------------------------ geometry


def image_rect(m: swf.Matrix, img: swf.ExternalImage) -> list[float]:
    """[x, y, w, h] covered by a bitmap placed with `m` (axis-aligned only)."""
    assert m.b == 0 and m.c == 0, "rotated bitmap"
    return [m.tx, m.ty, m.a * img.width, m.d * img.height]


def combine(outer: swf.Matrix, inner: swf.Matrix) -> swf.Matrix:
    return swf.Matrix(
        outer.a * inner.a + outer.c * inner.b, outer.b * inner.a + outer.d * inner.b,
        outer.a * inner.c + outer.c * inner.d, outer.b * inner.c + outer.d * inner.d,
        outer.a * inner.tx + outer.c * inner.ty + outer.tx, outer.b * inner.tx + outer.d * inner.ty + outer.ty,
    )


def cxform_json(cx: swf.CxForm | None) -> dict | None:
    return None if cx is None else {"mult": cx.mult, "add": cx.add}


class Meter:
    def __init__(self, movie: swf.Movie):
        self.m = movie
        self.id = movie.symbol("BoostMeterViewMovie")
        self.places = {p.name: p for p in swf.frame_display_lists(movie.sprites[self.id])[0].values() if p.name}

    def position(self) -> list[float]:
        root = swf.frame_display_lists(self.m.root)[0]
        place = next(p for p in root.values() if p.name == "boostMeterView")
        return [place.matrix.tx, place.matrix.ty]

    def bitmap_clip(self, name: str) -> dict:
        """A single-frame clip holding one bitmap (background, glow)."""
        place = self.places[name]
        (inner,) = swf.frame_display_lists(self.m.sprites[place.char])[0].values()
        img = self.m.images[inner.char]
        return {"texture": TEXTURES[img.export_name], "rect": image_rect(combine(place.matrix, inner.matrix), img),
                "depth": place.depth, "place_cxform": cxform_json(place.cxform)}

    def progress_bar(self, name: str) -> dict:
        place = self.places[name]
        frames, rect, tex = [], None, None
        for dl in swf.frame_display_lists(self.m.sprites[place.char]):
            if not dl:
                frames.append([])
                continue
            (p,) = dl.values()
            m = combine(place.matrix, p.matrix or swf.Matrix())
            if p.char in self.m.images:  # the full bitmap
                img = self.m.images[p.char]
                r = image_rect(m, img)
                frames.append([[r[0], r[1]], [r[0] + r[2], r[1]], [r[0] + r[2], r[1] + r[3]], [r[0], r[1] + r[3]]])
            else:
                shape = self.m.shapes[p.char]
                (fill,) = shape.fills
                img = self.m.images[fill.bitmap]
                r = image_rect(combine(m, fill.matrix), img)
                (poly,) = shape.polygons(1)
                frames.append([list(m.apply(x, y)) for x, y in poly])
            if rect is None:
                rect, tex = r, TEXTURES[img.export_name]
            assert rect == r and tex == TEXTURES[img.export_name], f"{name}: bitmap moves between frames"
        return {"texture": tex, "rect": rect, "depth": place.depth, "frames": frames}

    def text_field(self, name: str, outer: swf.Matrix | None = None) -> dict:
        place = self.places[name]
        m = place.matrix
        char = place.char
        if char in self.m.sprites:  # SimpleLabel: a clip with a `textField` child
            (inner,) = swf.frame_display_lists(self.m.sprites[char])[0].values()
            m = combine(m, inner.matrix)
            char = inner.char
        t = self.m.texts[char]
        assert m.a == 1 and m.d == 1 and m.b == 0 and m.c == 0
        return {
            "depth": place.depth, "origin": [m.tx, m.ty], "bounds": t.bounds, "font": t.font_class, "size": t.height,
            "color": list(t.color), "align": ["left", "right", "center", "justify"][t.align],
            "margins": [t.left_margin, t.right_margin], "indent": t.indent, "leading": t.leading,
            "filters": place.filters,
        }


# ------------------------------------------------------------------------------------ glyphs


def rasterize(edges: list[swf.Edge], scale: float, ox: float, oy: float, w: int, h: int, ss: int = 4) -> np.ndarray:
    """Coverage of a glyph (nonzero winding, `ss` x `ss` samples per texel)."""
    segs = []
    for e in edges:
        pts = [(e.x0, e.y0)]
        if e.c is not None:
            for i in range(1, 12):
                t = i / 12
                u = 1 - t
                pts.append((u * u * e.x0 + 2 * u * t * e.c[0] + t * t * e.x1, u * u * e.y0 + 2 * u * t * e.c[1] + t * t * e.y1))
        pts.append((e.x1, e.y1))
        # Fill style 1 on the right (fill1) or left (fill0) of the edge's direction.
        sign = (1 if e.fill1 else 0) - (1 if e.fill0 else 0)
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            segs.append((x0 * scale + ox, y0 * scale + oy, x1 * scale + ox, y1 * scale + oy, sign))
    W, H = w * ss, h * ss
    ys = (np.arange(H) + 0.5) / ss
    xs = (np.arange(W) + 0.5) / ss
    wind = np.zeros((H, W), np.int32)
    for x0, y0, x1, y1, sign in segs:
        if y0 == y1 or sign == 0:
            continue
        d = sign if y1 > y0 else -sign
        lo, hi = min(y0, y1), max(y0, y1)
        rows = np.nonzero((ys >= lo) & (ys < hi))[0]
        if len(rows) == 0:
            continue
        xi = x0 + (ys[rows] - y0) * (x1 - x0) / (y1 - y0)
        wind[rows] += np.where(xs[None, :] < xi[:, None], d, 0)
    cov = (wind != 0).astype(np.float32)
    return cov.reshape(h, ss, w, ss).mean(axis=(1, 3))


def box_blur(a: np.ndarray, width: float, passes: int) -> np.ndarray:
    """Flash's BlurFilter: `passes` box blurs `width` px wide along each axis."""
    n = max(1, int(round(width)))
    k = np.ones(n, np.float32) / n
    for _ in range(passes):
        a = np.apply_along_axis(lambda r: np.convolve(r, k, mode="same"), 1, a)
        a = np.apply_along_axis(lambda c: np.convolve(c, k, mode="same"), 0, a)
    return a


def glyph_atlas(font: swf.Font, chars: str, size: float, out_png: Path) -> dict:
    """Packs `chars` into one row. Glyph metrics stay in font units (1/20480 em, y down)."""
    scale = size * ATLAS_SCALE / EM  # texels per font unit
    pad = int(np.ceil((max(GLOW_BLURS) + 2) * ATLAS_SCALE))
    width = 1024  # power-of-two atlas (the hosts build mip chains for those)
    cells, x, y, row_h = [], 0, 0, 0
    for ch in chars:
        i = font.codes.index(ord(ch))
        xs = [v for e in font.glyphs[i] for v in (e.x0, e.x1)]
        ys = [v for e in font.glyphs[i] for v in (e.y0, e.y1)]
        x0, x1 = int(np.floor(min(xs) * scale)) - pad, int(np.ceil(max(xs) * scale)) + pad
        y0, y1 = int(np.floor(min(ys) * scale)) - pad, int(np.ceil(max(ys) * scale)) + pad
        if x + x1 - x0 > width:
            x, y, row_h = 0, y + row_h + 2, 0
        cells.append((ch, i, x, y, x0, y0, x1 - x0, y1 - y0))
        x += x1 - x0 + 2
        row_h = max(row_h, y1 - y0)
    height = 1 << int(np.ceil(np.log2(y + row_h)))
    atlas = np.zeros((height, width, 4), np.float32)
    atlas[..., 3] = 1.0
    glyphs = {}
    for ch, i, ax, ay, x0, y0, w, h in cells:
        cov = rasterize(font.glyphs[i], scale, -x0, -y0, w, h)
        atlas[ay:ay + h, ax:ax + w, 0] = cov
        for c, blur in enumerate(GLOW_BLURS):
            atlas[ay:ay + h, ax:ax + w, 1 + c] = np.clip(box_blur(cov, blur * ATLAS_SCALE, 2), 0, 1)
        glyphs[ch] = {
            "advance": font.advances[i],
            "plane": [x0 / scale, y0 / scale, (x0 + w) / scale, (y0 + h) / scale],  # font units, rel. to pen/baseline
            "uv": [ax, ay, ax + w, ay + h],  # texels
        }
    Image.fromarray((atlas * 255 + 0.5).astype(np.uint8), "RGBA").save(out_png)
    return {
        "name": font.name, "atlas": out_png.name, "atlas_size": [width, height], "units_per_em": EM,
        "ascent": font.ascent, "descent": font.descent, "leading": font.leading,
        "space_advance": font.advances[font.codes.index(32)], "glyphs": glyphs,
    }


# ------------------------------------------------------------------------------------ driver


class BoostMeter:
    def __init__(self, packages: Path, out: Path):
        self.packages = packages
        self.out = out / "hud"

    def run(self) -> None:
        self.out.mkdir(parents=True, exist_ok=True)
        hud_pkg = Package.open(self.packages / f"{HUD_PACKAGE}.upk")
        movie = swf_movie(hud_pkg, HUD_MOVIE)
        meter = Meter(movie)

        for name, png in TEXTURES.items():
            texture_png(hud_pkg, f"GFX_Hud.{name}").save(self.out / png)

        fonts_movie = swf_movie(Package.open(self.packages / f"{FONT_PACKAGE}.upk"), FONT_MOVIE)
        samples = [fonts_movie.texts[p.char] for p in swf.frame_display_lists(fonts_movie.root)[0].values()
                   if p.char in fonts_movie.texts and fonts_movie.texts[p.char].font_id is not None]
        sample_fonts = []
        for t in samples:  # one sample per font, before the labels (which all use Arial)
            if t.font_id not in sample_fonts:
                sample_fonts.append(t.font_id)
        aliases = {alias: fonts_movie.fonts[fid] for alias, fid in zip(FONT_ALIASES, sample_fonts)}

        texts = {n: meter.text_field(n) for n in ("backgroundTextField", "boostTextField", "boostLabel")}
        fonts = {}
        for alias, chars in GLYPH_SETS.items():
            size = max(t["size"] for t in texts.values() if t["font"] == alias)
            key = "numbers" if alias == "$NumbersWideFont" else "header"
            fonts[alias] = glyph_atlas(aliases[alias], chars, size, self.out / f"font_{key}.png")

        data = {
            "source": f"{HUD_PACKAGE} {HUD_MOVIE} (BoostMeterViewMovie), {FONT_PACKAGE} {FONT_MOVIE}",
            "stage": [movie.frame_size[1], movie.frame_size[3]],
            "position": meter.position(),
            "background": meter.bitmap_clip("backgroundMovieClip"),
            "glow": meter.bitmap_clip("glowMovieClip"),
            "fill": meter.progress_bar("fillProgressBar"),
            "fill_tinted": meter.progress_bar("fillProgressBarTinted"),
            "texts": texts,
            "fonts": fonts,
            "atlas_scale": ATLAS_SCALE,
            "glow_blurs": list(GLOW_BLURS),
        }
        (self.out / "boost_meter.json").write_text(json.dumps(data, indent=1))


if __name__ == "__main__":
    import argparse

    ap = argparse.ArgumentParser(description="Extract only the boost meter from already decrypted packages.")
    ap.add_argument("--packages", type=Path, required=True, help="folder with decrypted GFX_Hud_SF.upk and GFX_Fonts_SF.upk")
    ap.add_argument("--out", type=Path, required=True, help="assets folder (writes <out>/hud/)")
    a = ap.parse_args()
    BoostMeter(a.packages, a.out).run()
