"""Minimal reader for the Scaleform (GFx) Flash movies cooked into Rocket League's UI packages.

A `SwfMovie` export's `RawData` is a SWF file with the signature `GFX` (uncompressed) or `CFX`
(zlib), plus a few Scaleform tags (`DefineExternalImage2`: a bitmap that is a UE texture). This
reads what the HUD extraction needs: the display list (sprites, placements with their matrices,
colour transforms and filters), shapes as edge lists, text fields, and DefineFont3 glyphs. ActionScript
(DoABC) is not decompiled; the logic it holds is ported by hand.

Units: twips (1/20 px) are converted to movie pixels everywhere, except font glyphs, which stay in
their 1024 * 20 units per em.
"""

from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass, field

TWIPS = 20.0


class Bits:
    """MSB-first bit reader (SWF RECT, MATRIX, CXFORM and shape records)."""

    def __init__(self, data: bytes, pos: int):
        self.d = data
        self.bit = pos * 8

    def u(self, n: int) -> int:
        v = 0
        for _ in range(n):
            v = (v << 1) | ((self.d[self.bit >> 3] >> (7 - (self.bit & 7))) & 1)
            self.bit += 1
        return v

    def s(self, n: int) -> int:
        v = self.u(n)
        return v - (1 << n) if n and v & (1 << (n - 1)) else v

    def fixed(self, n: int) -> float:  # FB[n], 16.16
        return self.s(n) / 65536.0

    def align(self) -> int:
        self.bit = (self.bit + 7) & ~7
        return self.bit >> 3


def read_rect(d: bytes, pos: int) -> tuple[list[float], int]:
    b = Bits(d, pos)
    n = b.u(5)
    r = [b.s(n) / TWIPS for _ in range(4)]  # xmin, xmax, ymin, ymax
    return r, b.align()


@dataclass
class Matrix:
    """2D affine: x' = a*x + c*y + tx, y' = b*x + d*y + ty (movie px)."""

    a: float = 1.0
    b: float = 0.0
    c: float = 0.0
    d: float = 1.0
    tx: float = 0.0
    ty: float = 0.0

    def to_list(self) -> list[float]:
        return [self.a, self.b, self.c, self.d, self.tx, self.ty]

    def apply(self, x: float, y: float) -> tuple[float, float]:
        return self.a * x + self.c * y + self.tx, self.b * x + self.d * y + self.ty


def read_matrix(d: bytes, pos: int) -> tuple[Matrix, int]:
    b = Bits(d, pos)
    m = Matrix()
    if b.u(1):
        n = b.u(5)
        m.a, m.d = b.fixed(n), b.fixed(n)
    if b.u(1):
        n = b.u(5)
        m.b, m.c = b.fixed(n), b.fixed(n)  # RotateSkew0, RotateSkew1
    n = b.u(5)
    m.tx, m.ty = b.s(n) / TWIPS, b.s(n) / TWIPS
    return m, b.align()


@dataclass
class CxForm:
    """Colour transform: c' = c * mult + add, channels 0..255 (RGBA)."""

    mult: list[float] = field(default_factory=lambda: [1.0, 1.0, 1.0, 1.0])
    add: list[float] = field(default_factory=lambda: [0.0, 0.0, 0.0, 0.0])


def read_cxform(d: bytes, pos: int, alpha: bool) -> tuple[CxForm, int]:
    b = Bits(d, pos)
    has_add, has_mult = b.u(1), b.u(1)
    n = b.u(4)
    k = 4 if alpha else 3
    cx = CxForm()
    if has_mult:
        cx.mult[:k] = [b.s(n) / 256.0 for _ in range(k)]
    if has_add:
        cx.add[:k] = [float(b.s(n)) for _ in range(k)]
    return cx, b.align()


# --------------------------------------------------------------------------------------- shapes


@dataclass
class FillStyle:
    kind: int  # 0x00 solid, 0x40..0x43 bitmap
    color: tuple[int, int, int, int] = (0, 0, 0, 255)
    bitmap: int = 0
    matrix: Matrix | None = None  # bitmap: bitmap pixels -> shape space (movie px)


@dataclass
class Edge:
    """A straight edge or a quadratic curve (control point `c`), in shape units."""

    x0: float
    y0: float
    x1: float
    y1: float
    fill0: int
    fill1: int
    c: tuple[float, float] | None = None


@dataclass
class Shape:
    id: int
    bounds: list[float]
    fills: list[FillStyle]
    edges: list[Edge]

    def polygons(self, fill: int, curve_steps: int = 8) -> list[list[tuple[float, float]]]:
        """Closed contours of one fill style (1-based), with the fill on the right of each
        contour (Flash's fill1 side), curves flattened."""
        segs = []
        for e in self.edges:
            pts = [(e.x0, e.y0)]
            if e.c is not None:
                for i in range(1, curve_steps):
                    t = i / curve_steps
                    u = 1.0 - t
                    pts.append((u * u * e.x0 + 2 * u * t * e.c[0] + t * t * e.x1, u * u * e.y0 + 2 * u * t * e.c[1] + t * t * e.y1))
            pts.append((e.x1, e.y1))
            if e.fill1 == fill:
                segs.append(pts)
            if e.fill0 == fill:
                segs.append(pts[::-1])
        return chain(segs)


def chain(segs: list[list[tuple[float, float]]]) -> list[list[tuple[float, float]]]:
    """Joins directed polylines end to start into closed contours."""

    def key(p):
        return (round(p[0] * 1000), round(p[1] * 1000))

    by_start: dict = {}
    for i, s in enumerate(segs):
        by_start.setdefault(key(s[0]), []).append(i)
    used = [False] * len(segs)
    out = []
    for i in range(len(segs)):
        if used[i]:
            continue
        used[i] = True
        contour = list(segs[i])
        while key(contour[-1]) != key(contour[0]):
            nxt = next((j for j in by_start.get(key(contour[-1]), []) if not used[j]), None)
            if nxt is None:
                break
            used[nxt] = True
            contour.extend(segs[nxt][1:])
        out.append(contour[:-1] if key(contour[-1]) == key(contour[0]) else contour)
    return out


def _fill_styles(d: bytes, pos: int, version: int) -> tuple[list[FillStyle], int]:
    n = d[pos]
    pos += 1
    if n == 0xFF and version >= 2:
        n = struct.unpack_from("<H", d, pos)[0]
        pos += 2
    fills = []
    for _ in range(n):
        kind = d[pos]
        pos += 1
        if kind == 0x00:
            if version >= 3:
                fills.append(FillStyle(kind, tuple(d[pos:pos + 4])))
                pos += 4
            else:
                fills.append(FillStyle(kind, tuple(d[pos:pos + 3]) + (255,)))
                pos += 3
        elif kind in (0x10, 0x12, 0x13):  # gradients: skipped (not used by the boost meter)
            _, pos = read_matrix(d, pos)
            count = d[pos] & 0x0F
            pos += 1 + count * (5 if version >= 3 else 4)
            if kind == 0x13:
                pos += 2
            fills.append(FillStyle(kind))
        elif kind in (0x40, 0x41, 0x42, 0x43):
            bitmap = struct.unpack_from("<H", d, pos)[0]
            m, pos = read_matrix(d, pos + 2)
            # Bitmap matrices map bitmap pixels to twips; express them in movie px.
            m = Matrix(m.a / TWIPS, m.b / TWIPS, m.c / TWIPS, m.d / TWIPS, m.tx, m.ty)
            fills.append(FillStyle(kind, bitmap=bitmap, matrix=m))
        else:
            raise ValueError(f"fill style 0x{kind:02x}")
    return fills, pos


def _line_styles(d: bytes, pos: int, version: int) -> int:
    n = d[pos]
    pos += 1
    if n == 0xFF:
        n = struct.unpack_from("<H", d, pos)[0]
        pos += 2
    for _ in range(n):
        if version == 4:  # LINESTYLE2: width, cap(2) join(2) has_fill(1) ..., then miter / fill / colour
            flags = d[pos + 2]
            pos += 4
            if (flags >> 4) & 3 == 2:  # miter join
                pos += 2
            if flags & 0x08:
                raise ValueError("line fill styles unsupported")
            pos += 4
        else:
            pos += 2 + (4 if version >= 3 else 3)
    return pos


def read_shape_records(d: bytes, pos: int, version: int, fills: list[FillStyle], scale: float = TWIPS,
                       fill_bits: int | None = None, line_bits: int | None = None) -> list[Edge]:
    """SHAPE / SHAPEWITHSTYLE records after the style arrays. `fills` is extended in place by
    StateNewStyles records (their indices continue past the existing ones)."""
    if fill_bits is None:
        fill_bits, line_bits = d[pos] >> 4, d[pos] & 0x0F
        pos += 1
    b = Bits(d, pos)
    x = y = 0.0
    f0 = f1 = 0
    base = 0
    edges = []
    while True:
        if b.u(1) == 0:
            flags = b.u(5)
            if flags == 0:
                break
            if flags & 1:  # move to
                n = b.u(5)
                x, y = b.s(n) / scale, b.s(n) / scale
            if flags & 2:
                v = b.u(fill_bits)
                f0 = base + v if v else 0
            if flags & 4:
                v = b.u(fill_bits)
                f1 = base + v if v else 0
            if flags & 8:
                b.u(line_bits)
            if flags & 16:  # new styles
                p = b.align()
                base = len(fills)
                new, p = _fill_styles(d, p, version)
                fills.extend(new)
                p = _line_styles(d, p, version)
                fill_bits, line_bits = d[p] >> 4, d[p] & 0x0F
                b = Bits(d, p + 1)
        elif b.u(1):  # straight
            n = b.u(4) + 2
            if b.u(1):
                dx, dy = b.s(n), b.s(n)
            elif b.u(1):
                dx, dy = 0, b.s(n)
            else:
                dx, dy = b.s(n), 0
            nx, ny = x + dx / scale, y + dy / scale
            edges.append(Edge(x, y, nx, ny, f0, f1))
            x, y = nx, ny
        else:  # curved
            n = b.u(4) + 2
            cx, cy = x + b.s(n) / scale, y + b.s(n) / scale
            ax, ay = cx + b.s(n) / scale, cy + b.s(n) / scale
            edges.append(Edge(x, y, ax, ay, f0, f1, (cx, cy)))
            x, y = ax, ay
    return edges


def read_shape(code: int, d: bytes, pos: int) -> Shape:
    version = {2: 1, 22: 2, 32: 3, 83: 4}[code]
    sid = struct.unpack_from("<H", d, pos)[0]
    bounds, pos = read_rect(d, pos + 2)
    if version == 4:
        _, pos = read_rect(d, pos)  # edge bounds
        pos += 1
    fills, pos = _fill_styles(d, pos, version)
    pos = _line_styles(d, pos, version)
    edges = read_shape_records(d, pos, version, fills)
    return Shape(sid, bounds, fills, edges)


# ---------------------------------------------------------------------------------- characters


@dataclass
class Place:
    depth: int
    move: bool
    char: int | None = None
    name: str | None = None
    class_name: str | None = None
    matrix: Matrix | None = None
    cxform: CxForm | None = None
    ratio: int | None = None
    clip_depth: int | None = None
    filters: list[dict] = field(default_factory=list)
    blend: int | None = None


@dataclass
class Sprite:
    id: int
    frames: list[list]  # per frame: [Place | ("remove", depth)]


@dataclass
class EditText:
    id: int
    bounds: list[float]
    font_class: str | None
    font_id: int | None
    height: float  # px
    color: tuple[int, int, int, int]
    align: int  # 0 left, 1 right, 2 center, 3 justify
    left_margin: float
    right_margin: float
    indent: float
    leading: float
    html: bool
    text: str


@dataclass
class Font:
    id: int
    name: str
    glyphs: list[list[Edge]]  # 1024 * 20 units per em, y down
    codes: list[int]
    ascent: float = 0.0
    descent: float = 0.0
    leading: float = 0.0
    advances: list[float] = field(default_factory=list)
    bounds: list[list[float]] = field(default_factory=list)
    kerning: dict = field(default_factory=dict)  # (code, code) -> adjustment


@dataclass
class ExternalImage:
    id: int
    format: int
    width: int
    height: int
    export_name: str
    file_name: str


def _cstr(d: bytes, pos: int) -> tuple[str, int]:
    end = d.index(0, pos)
    return d[pos:end].decode("utf-8", "replace"), end + 1


def _pstr(d: bytes, pos: int) -> tuple[str, int]:
    n = d[pos]
    return d[pos + 1:pos + 1 + n].decode("utf-8", "replace"), pos + 1 + n


def read_filters(d: bytes, pos: int) -> tuple[list[dict], int]:
    n = d[pos]
    pos += 1
    out = []
    for _ in range(n):
        fid = d[pos]
        pos += 1
        if fid == 2:  # glow
            r, g, b_, a = d[pos:pos + 4]
            bx, by = struct.unpack_from("<ii", d, pos + 4)
            strength = struct.unpack_from("<H", d, pos + 12)[0] / 256.0
            flags = d[pos + 14]
            out.append({
                "type": "glow", "color": [r, g, b_, a], "blur": [bx / 65536.0, by / 65536.0], "strength": strength,
                "inner": bool(flags & 0x80), "knockout": bool(flags & 0x40), "passes": flags & 0x1F,
            })
            pos += 15
        else:
            sizes = {0: 23, 1: 9, 3: 27, 4: None, 5: None, 6: 80, 7: None}
            size = sizes.get(fid)
            if size is None:
                raise ValueError(f"filter {fid}")
            out.append({"type": f"filter{fid}"})
            pos += size
    return out, pos


def read_place(code: int, d: bytes, pos: int, end: int) -> Place:
    if code == 4:  # PlaceObject
        char, depth = struct.unpack_from("<HH", d, pos)
        m, pos = read_matrix(d, pos + 4)
        p = Place(depth, False, char, matrix=m)
        if pos < end:
            p.cxform, _ = read_cxform(d, pos, False)
        return p
    f = d[pos]
    f2 = d[pos + 1] if code == 70 else 0
    pos += 2 if code == 70 else 1
    depth = struct.unpack_from("<H", d, pos)[0]
    pos += 2
    p = Place(depth, bool(f & 1))
    if code == 70 and f2 & 0x08:  # HasClassName (HasImage alone does not add one in practice)
        p.class_name, pos = _cstr(d, pos)
    if f & 2:
        p.char = struct.unpack_from("<H", d, pos)[0]
        pos += 2
    if f & 4:
        p.matrix, pos = read_matrix(d, pos)
    if f & 8:
        p.cxform, pos = read_cxform(d, pos, True)
    if f & 16:
        p.ratio = struct.unpack_from("<H", d, pos)[0]
        pos += 2
    if f & 32:
        p.name, pos = _cstr(d, pos)
    if f & 64:
        p.clip_depth = struct.unpack_from("<H", d, pos)[0]
        pos += 2
    if code == 70:
        if f2 & 1:
            p.filters, pos = read_filters(d, pos)
        if f2 & 2:
            p.blend = d[pos]
            pos += 1
    return p


def read_edit_text(d: bytes, pos: int) -> EditText:
    cid = struct.unpack_from("<H", d, pos)[0]
    bounds, pos = read_rect(d, pos + 2)
    f1, f2 = d[pos], d[pos + 1]
    pos += 2
    font_id = font_class = None
    height = 0.0
    color = (0, 0, 0, 255)
    align, lm, rm, ind, lead = 0, 0.0, 0.0, 0.0, 0.0
    text = ""
    if f1 & 1:  # HasFont
        font_id = struct.unpack_from("<H", d, pos)[0]
        pos += 2
    if f2 & 0x80:  # HasFontClass
        font_class, pos = _cstr(d, pos)
    if f1 & 1 or f2 & 0x80:
        height = struct.unpack_from("<H", d, pos)[0] / TWIPS
        pos += 2
    if f1 & 4:  # HasTextColor
        color = tuple(d[pos:pos + 4])
        pos += 4
    if f1 & 2:  # HasMaxLength
        pos += 2
    if f2 & 0x20:  # HasLayout
        align = d[pos]
        lm, rm, ind = (v / TWIPS for v in struct.unpack_from("<HHh", d, pos + 1))
        lead = struct.unpack_from("<h", d, pos + 7)[0] / TWIPS
        pos += 9
    _, pos = _cstr(d, pos)  # variable name
    if f1 & 0x80:  # HasText
        text, pos = _cstr(d, pos)
    return EditText(cid, bounds, font_class, font_id, height, color, align, lm, rm, ind, lead, bool(f2 & 0x02), text)


def read_font3(d: bytes, pos: int, end: int) -> Font:
    fid = struct.unpack_from("<H", d, pos)[0]
    flags = d[pos + 2]
    pos += 4  # id, flags, language
    name_len = d[pos]
    name = d[pos + 1:pos + 1 + name_len].rstrip(b"\0").decode("utf-8", "replace")
    pos += 1 + name_len
    n = struct.unpack_from("<H", d, pos)[0]
    pos += 2
    wide_offsets, wide_codes, has_layout = flags & 0x08, flags & 0x04, flags & 0x80
    fmt = "<I" if wide_offsets else "<H"
    sz = 4 if wide_offsets else 2
    table = pos
    offsets = [struct.unpack_from(fmt, d, table + i * sz)[0] for i in range(n + 1)]
    glyphs = []
    for i in range(n):
        g = table + offsets[i]
        glyphs.append(read_shape_records(d, g, 1, [FillStyle(0)], scale=1.0))
    pos = table + offsets[n]
    codes = []
    for _ in range(n):
        if wide_codes:
            codes.append(struct.unpack_from("<H", d, pos)[0])
            pos += 2
        else:
            codes.append(d[pos])
            pos += 1
    font = Font(fid, name, glyphs, codes)
    if has_layout:
        font.ascent, font.descent, font.leading = struct.unpack_from("<HHh", d, pos)
        pos += 6
        font.advances = list(struct.unpack_from(f"<{n}h", d, pos))
        pos += 2 * n
        for _ in range(n):
            r, pos = read_rect(d, pos)
            font.bounds.append([v * TWIPS for v in r])  # back to font units
        kn = struct.unpack_from("<H", d, pos)[0]
        pos += 2
        for _ in range(kn):
            if wide_codes:
                a, b_, adj = struct.unpack_from("<HHh", d, pos)
                pos += 6
            else:
                a, b_, adj = d[pos], d[pos + 1], struct.unpack_from("<h", d, pos + 2)[0]
                pos += 4
            font.kerning[(a, b_)] = adj
    return font


# ------------------------------------------------------------------------------------- movie


@dataclass
class Movie:
    version: int
    frame_size: list[float]
    shapes: dict[int, Shape] = field(default_factory=dict)
    sprites: dict[int, Sprite] = field(default_factory=dict)
    texts: dict[int, EditText] = field(default_factory=dict)
    fonts: dict[int, Font] = field(default_factory=dict)
    images: dict[int, ExternalImage] = field(default_factory=dict)
    symbols: dict[int, str] = field(default_factory=dict)  # character id -> AS3 class
    root: Sprite | None = None

    def symbol(self, class_name: str) -> int:
        return next(k for k, v in self.symbols.items() if v == class_name)


def _tags(d: bytes, pos: int, end: int):
    while pos < end:
        h = struct.unpack_from("<H", d, pos)[0]
        pos += 2
        code, ln = h >> 6, h & 0x3F
        if ln == 0x3F:
            ln = struct.unpack_from("<I", d, pos)[0]
            pos += 4
        yield code, pos, ln
        pos += ln
        if code == 0:
            return


def _timeline(movie: Movie, d: bytes, pos: int, end: int, sid: int) -> Sprite:
    sprite = Sprite(sid, [[]])
    for code, p, ln in _tags(d, pos, end):
        cur = sprite.frames[-1]
        if code == 1:
            sprite.frames.append([])
        elif code in (4, 26, 70):
            cur.append(read_place(code, d, p, p + ln))
        elif code in (5, 28):
            depth = struct.unpack_from("<H", d, p + (2 if code == 5 else 0))[0]
            cur.append(("remove", depth))
        elif code in (2, 22, 32, 83):
            s = read_shape(code, d, p)
            movie.shapes[s.id] = s
        elif code == 39:
            cid, _ = struct.unpack_from("<HH", d, p)
            movie.sprites[cid] = _timeline(movie, d, p + 4, p + ln, cid)
        elif code == 37:
            t = read_edit_text(d, p)
            movie.texts[t.id] = t
        elif code == 75:
            f = read_font3(d, p, p + ln)
            movie.fonts[f.id] = f
        elif code == 1009:
            cid, _id_type, fmt, w, h = struct.unpack_from("<HHHHH", d, p)
            export, q = _pstr(d, p + 10)
            fname, q = _pstr(d, q)
            movie.images[cid] = ExternalImage(cid, fmt, w, h, export, fname)
        elif code == 76:
            n = struct.unpack_from("<H", d, p)[0]
            q = p + 2
            for _ in range(n):
                cid = struct.unpack_from("<H", d, q)[0]
                name, q = _cstr(d, q + 2)
                movie.symbols[cid] = name
    if not sprite.frames[-1]:
        sprite.frames.pop()
    return sprite


def parse(raw: bytes) -> Movie:
    sig, version, length = raw[:3], raw[3], struct.unpack_from("<I", raw, 4)[0]
    if sig in (b"CFX", b"CWS"):
        raw = raw[:8] + zlib.decompress(raw[8:])
    elif sig not in (b"GFX", b"FWS"):
        raise ValueError(f"not a SWF/GFx movie: {sig!r}")
    frame_size, pos = read_rect(raw, 8)
    movie = Movie(version, frame_size)
    movie.root = _timeline(movie, raw, pos + 4, len(raw), 0)
    return movie


def frame_display_lists(sprite: Sprite) -> list[dict[int, Place]]:
    """The display list (depth -> merged placement) on each frame of a timeline."""
    out, cur = [], {}
    for frame in sprite.frames:
        for item in frame:
            if isinstance(item, tuple):
                cur.pop(item[1], None)
            elif item.move and item.depth in cur:
                old = cur[item.depth]
                merged = Place(item.depth, False, item.char if item.char is not None else old.char, item.name or old.name,
                               item.class_name or old.class_name, item.matrix or old.matrix, item.cxform or old.cxform,
                               item.ratio if item.ratio is not None else old.ratio, old.clip_depth, item.filters or old.filters, old.blend)
                cur[item.depth] = merged
            else:
                cur[item.depth] = item
        out.append(dict(cur))
    return out
