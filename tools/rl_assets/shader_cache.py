"""Developer tool: find a material's compiled shaders in the game's RefShaderCache and disassemble
them, to port them by hand (the WGSL ports in crates/rl_car_bevy/src/shaders/ were made this way).

The cache is an Oodle-compressed package (Mermaid blocks in UE3 compressed chunks, 36-byte RL chunk
records); it is decompressed with ooz (https://github.com/powzix/ooz, built as a DLL exporting
`ooz_decompress`). Its material shader maps are found by the material's GUID and name; each lists, per
vertex factory, (shader type, shader GUID); a shader is stored as (type, GUID, ..., code size, DXBC).

  python tools/rl_assets/shader_cache.py --ooz <ooz.dll> Startup:FX_Smoke.Mat.Smoke_Puff_01_Mat [...]

Writes <work>/shaders/<Material>__<VertexFactory>__<ShaderType>.asm (D3DDisassemble output).
"""

from __future__ import annotations

import argparse
import ctypes
import mmap
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ue3 import Package  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
CACHE = "RefShaderCache-PC-D3D-SM5.upk"


def decompress(src: Path, dst: Path, ooz: Path) -> None:
    dll = ctypes.CDLL(str(ooz))
    d = src.read_bytes()
    n = struct.unpack_from("<i", d, 113)[0]
    chunks = [struct.unpack_from("<qiqi", d, 117 + 36 * k) for k in range(n)]
    with open(dst, "wb") as out:
        out.truncate(max(u + us for u, us, _, _ in chunks))
        out.seek(0)
        out.write(d[:141])
        for uoff, usize, coff, _ in chunks:
            tag, block, _, total = struct.unpack_from("<4I", d, coff)
            assert tag == 0x9E2A83C1 and total == usize
            blocks = [struct.unpack_from("<II", d, coff + 16 + 8 * i) for i in range((total + block - 1) // block)]
            p = coff + 16 + 8 * len(blocks)
            out.seek(uoff)
            for c, u in blocks:
                buf = ctypes.create_string_buffer(u + 64)
                if dll.ooz_decompress(d[p : p + c], c, buf, u) != u:
                    raise SystemExit("Oodle block failed to decompress")
                out.write(buf.raw[:u])
                p += c


def disassemble(code: bytes) -> str:
    d3d = ctypes.WinDLL("d3dcompiler_47.dll")
    blob = ctypes.c_void_p()
    if d3d.D3DDisassemble(code, len(code), 0, None, ctypes.byref(blob)) != 0:
        return "; D3DDisassemble failed"
    vt = ctypes.cast(blob, ctypes.POINTER(ctypes.POINTER(ctypes.c_void_p))).contents
    ptr = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p)(vt[3])(blob)
    size = ctypes.CFUNCTYPE(ctypes.c_size_t, ctypes.c_void_p)(vt[4])(blob)
    return ctypes.string_at(ptr, size).decode("latin-1").rstrip("\0")


def material_id(package: Path, path: str) -> bytes:
    """The id a cooked material (or instance with a static permutation) compiled its shaders under:
    after its tagged properties, its FMaterialResource starts with 4 ints, then that GUID."""
    p = Package.open(package)
    e = p.export_at(path)
    if e is None:
        raise SystemExit(f"{path} not in {package.name}")
    d = p.data[e.offset : e.offset + e.size]
    none = p.names.index("None")
    end = max(i for i in range(len(d) - 8) if struct.unpack_from("<ii", d, i) == (none, 0)) + 8
    return d[end + 16 : end + 32]


class Cache:
    def __init__(self, path: Path):
        self.f = open(path, "rb")
        self.m = mmap.mmap(self.f.fileno(), 0, access=mmap.ACCESS_READ)
        pk = Package.__new__(Package)
        pk.path, pk.data, pk.names, pk.imports, pk.exports = path, self.m[:1_000_000], [], [], []
        pk._read_tables()
        self.names = pk.names

    def name(self, pos: int) -> str | None:
        i, num = struct.unpack_from("<ii", self.m, pos)
        return self.names[i] if 0 <= i < len(self.names) and num == 0 else None

    def shader_map(self, guid: bytes) -> dict[str, dict[str, bytes]]:
        """VF name -> shader type -> shader GUID of the shader map with this material id. The map is
        [mesh shader maps][a name][material id][friendly name (FString)]..., read backwards."""
        at = 0
        while True:
            at = self.m.find(guid, at)
            if at < 0:
                raise SystemExit(f"material id {guid.hex()} has no shader map in the cache")
            n = struct.unpack_from("<i", self.m, at + 16)[0]
            if 0 < n < 128 and self.m[at + 20 + n - 1] == 0 and self.m[at + 20 : at + 19 + n].isascii():
                break
            at += 1
        out: dict[str, dict[str, bytes]] = {}
        pos = at - 8
        while True:
            found = False
            for count in range(1, 40):
                start = pos - 12 - count * 32
                if start < 0:
                    break
                vf = self.name(start)
                if vf and vf.endswith("VertexFactory") and struct.unpack_from("<i", self.m, start + 8)[0] == count:
                    out[vf] = {self.name(start + 12 + 32 * k): self.m[start + 20 + 32 * k : start + 36 + 32 * k] for k in range(count)}
                    pos, found = start, True
                    break
            if not found:
                break
        if not out:
            raise SystemExit(f"material id {guid.hex()}: shader map not understood near {at}")
        return out

    def uniforms(self, guid: bytes) -> list:
        at = 0
        while True:
            at = self.m.find(guid, at)
            n = struct.unpack_from("<i", self.m, at + 16)[0]
            if 0 < n < 128 and self.m[at + 20 + n - 1] == 0:
                return Expressions(self, at + 20 + n).read()
            at += 1

    def code(self, type_name: str, guid: bytes) -> bytes:
        tid = self.names.index(type_name)
        key = struct.pack("<ii", tid, 0) + guid
        i = 0
        while True:
            i = self.m.find(key, i)
            if i < 0:
                raise SystemExit(f"shader {guid.hex()} not found")
            dx = self.m.find(b"DXBC", i, i + 128)
            if dx > 0:
                size = struct.unpack_from("<I", self.m, dx - 4)[0]
                return bytes(self.m[dx : dx + size])
            i += 1


OPS = {0: "+", 1: "-", 2: "*", 3: "/", 4: "dot"}


class Expressions:
    """Reads a material's uniform expression set (after its friendly name): the values the shader
    reads from its constant buffer, from cb0[57] on (scalars packed first when there are any)."""

    def __init__(self, cache: "Cache", pos: int):
        self.c, self.pos = cache, pos

    def i32(self) -> int:
        v = struct.unpack_from("<i", self.c.m, self.pos)[0]
        self.pos += 4
        return v

    def f32(self) -> float:
        v = struct.unpack_from("<f", self.c.m, self.pos)[0]
        self.pos += 4
        return v

    def byte(self) -> int:
        v = self.c.m[self.pos]
        self.pos += 1
        return v

    def name(self) -> str:
        n = self.c.name(self.pos)
        self.pos += 8
        return n

    def expr(self) -> str:
        t = self.name()
        k = t.removeprefix("FMaterialUniformExpression") if t else "?"
        if k == "VectorParameter":
            n = self.name()
            v = [round(self.f32(), 6) for _ in range(4)]
            return f"VectorParameter({n}, default={v})"
        if k == "ScalarParameter":
            n = self.name()
            return f"ScalarParameter({n}, default={self.f32():.6g})"
        if k == "Constant":
            v = [round(self.f32(), 6) for _ in range(4)]
            self.byte()  # value type
            return f"Constant({v})"
        if k in ("Time", "RealTime"):
            return k
        if k in ("Periodic", "Floor", "Ceil", "Frac", "Abs", "SquareRoot", "Length"):
            return f"{k}({self.expr()})"
        if k == "Sine":
            x = self.expr()
            return f"{'Cos' if self.i32() else 'Sin'}({x})"
        if k == "FoldedMath":
            a, b = self.expr(), self.expr()
            return f"({a} {OPS.get(self.byte(), '?')} {b})"
        if k == "AppendVector":
            a, b = self.expr(), self.expr()
            return f"Append({a}, {b}, components_a={self.i32()})"
        if k in ("Clamp",):
            return f"Clamp({self.expr()}, {self.expr()}, {self.expr()})"
        if k in ("Min", "Max"):
            return f"{k}({self.expr()}, {self.expr()})"
        if k in ("Texture", "TextureParameter", "FlipBookTextureParameter"):
            n = self.name() if k != "Texture" else ""
            return f"{k}({n}, index={self.i32()})"
        raise SystemExit(f"uniform expression {t} not understood at {self.pos}")

    def read(self) -> list[tuple[str, list[str]]]:
        """Pixel uniform vectors, scalars, then 2D textures, after a 16-byte GUID and 16 zero bytes
        (the vertex shader set is empty for these materials)."""
        self.pos += 32
        out = []
        for label in ("vectors", "scalars", "textures2d"):
            n = self.i32()
            out.append((label, [self.expr() for _ in range(n)]))
        return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("materials", nargs="+", help="Package:Object.Path of a material or material instance with its own shaders")
    ap.add_argument("--ooz", type=Path, required=True)
    ap.add_argument("--game", type=Path, default=Path(r"C:\Program Files\Epic Games\rocketleague"))
    ap.add_argument("--work", type=Path, default=REPO / "target" / "rl_assets_work")
    ap.add_argument("--all", action="store_true", help="every shader type, not only the base pass pixel shaders")
    a = ap.parse_args()
    cache = a.work / "packages" / CACHE
    if not cache.exists():
        print("decompressing the shader cache (2.5 GB) ...")
        decompress(a.game / "TAGame" / "CookedPCConsole" / CACHE, cache, a.ooz)
    c = Cache(cache)
    out = a.work / "shaders"
    out.mkdir(parents=True, exist_ok=True)
    for arg in a.materials:
        package, mat = arg.split(":")
        mid = material_id(a.work / "packages" / f"{package}.upk", mat)
        try:
            for label, exprs in c.uniforms(mid):
                for i, x in enumerate(exprs):
                    print(f"{mat} {label}[{i}]: {x}")
        except SystemExit as e:
            print(f"{mat}: {e}")
        for vf, shaders in c.shader_map(mid).items():
            for t, guid in shaders.items():
                if not a.all and not t.startswith("TBasePassPixelShader"):
                    continue
                safe = "".join(ch if ch.isalnum() or ch in "_-" else "_" for ch in f"{mat.split('.')[-1]}__{vf}__{t}")
                path = out / f"{safe}.asm"
                path.write_text(disassemble(c.code(t, guid)))
                print(path)


if __name__ == "__main__":
    main()
