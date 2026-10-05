#!/usr/bin/env python3
"""Extract the real Rocket League car models from YOUR OWN game install for the Bevy demo.

Nothing extracted here may be committed or redistributed: the output goes to `assets/rl/`, which is
git-ignored. Without it the demo keeps its procedural placeholder car.

Pipeline
  1. decrypt the needed packages with RL-UPKSuite (via the small `rldecrypt` wrapper in this folder),
  2. export meshes / materials / textures with UModel (UE Viewer) as glTF + PNG,
  3. rebuild each material for Bevy's PBR (bake the team paint, swizzle normal maps, light masks to
     emissive) and write one glTF per car and team, plus the default wheel;
  4. read the default boost (flame cones, smoke trail) straight from the cooked objects (`boost.py`,
     `ue3.py`) into `boost/`;
  5. write the unbaked textures and parameter values that the Minecraft mod's ports of the game's
     car material shaders use (`shading.py`): `materials.json` per car and the wheel, `shading/`;
     likewise for the ball's material and its ground reticle (`ball/materials.json`);
  6. with --wwiser and --vgmstream: the car's sounds (engine, boost, jumps, dodges, landings, tyres,
     impacts, supersonic) from the game's Wwise sound banks (`audio.py`) into `audio/`;
  7. read the HUD's boost meter (its Scaleform movie, textures and fonts) into `hud/` (`hud.py`,
     `swf.py`).

Objects already exported to the work folder are not exported again.

Requirements: Python 3.9+ with numpy and Pillow, the .NET SDK (8+), UModel
(https://www.gildor.org/en/projects/umodel) and RL-UPKSuite (https://github.com/Martinii89/RL-UPKSuite).

  python tools/rl_assets/extract.py --umodel <dir with umodel_64.exe> --upksuite <dir with Core.dll>
      [--wwiser <wwiser.pyz> --vgmstream <vgmstream-cli.exe>]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import struct
import subprocess
import sys
from pathlib import Path
from typing import NamedTuple

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from audio import PACKAGES as AUDIO_PACKAGES, Audio  # noqa: E402
from boost import PACKAGES as BOOST_PACKAGES, Boost  # noqa: E402
from fx import PACKAGES as FX_PACKAGES, FX  # noqa: E402
from shading import ShadingWriter  # noqa: E402
from ue3 import Package  # noqa: E402
from hud import PACKAGES as HUD_PACKAGES, BoostMeter  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
DEFAULT_GAME = Path(r"C:\Program Files\Epic Games\rocketleague")

# Hitbox preset -> (package, skeletal mesh). One representative car per preset.
CARS = {
    "octane": ("Startup", "Body_Octane_SK"),
    "dominus": ("Body_MuscleCar_SF", "Body_MuscleCar_SK"),
    "plank": ("Body_Orion_SF", "Body_Orion_SK"),  # uses the "Plank" handling preset
    "breakout": ("Body_CarCar_SF", "Body_Breakout_MK2_SK"),
    "hybrid": ("Body_Venom_SF", "Body_Venom_PremiumSkin_SK"),
    "merc": ("Body_Vanquish_SF", "Body_Merc_PremiumSkin_SK"),
    "psyclops": ("body_pixie_SF", "body_pixie_sk"),  # handling preset "Pixie"
}
WHEEL = ("Startup", "WHEEL_Star_SM", "Wheel_OEM_MIC")  # Octane's default "OEM" wheel
BALL = ("GameInfo_Soccar_SF", "Ball_DefaultBall00", "Ball_Default00_D", "Ball_Default00_RGB")
# The rest of the ball's material (MAT_Ball_V3: its normal map and the tiled Detail_Matte normal
# from Startup) and the ground reticle decal's texture (Ball_GroundReticle_DMat), for the
# Minecraft mod's ports of their shaders.
BALL_NORMAL = "Ball_Default00_N"
BALL_DETAIL = ("Startup", "Matte_N")
BALL_RETICLE = "Reticles_01_Pack"

# Team paint (linear RGB). Approximations of the default team colours: the game picks them at
# runtime from a palette lookup texture (CustomColors.Team*_ColorLookup) that is not extracted here.
TEAMS = {
    "blue": (0.0, 0.09, 0.75),
    "orange": (0.90, 0.20, 0.006),
}


# ------------------------------------------------------------------------------------ tools


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def build_decryptor(upksuite: Path, work: Path) -> Path:
    exe = work / "rldecrypt" / "rldecrypt.exe"
    if exe.exists():
        return exe
    print("building rldecrypt ...")
    src = Path(__file__).resolve().parent / "rldecrypt"
    r = run(["dotnet", "build", str(src / "rldecrypt.csproj"), "-c", "Release", "-o", str(exe.parent),
             f"-p:UpkSuiteDir={upksuite}", f"-p:BaseIntermediateOutputPath={work / 'rldecrypt_obj'}{os.sep}"])
    if r.returncode != 0:
        sys.exit(f"dotnet build failed:\n{r.stdout}\n{r.stderr}")
    for dll in upksuite.glob("*.dll"):  # Core.dll's own dependencies
        if not (exe.parent / dll.name).exists():
            shutil.copy2(dll, exe.parent / dll.name)
    return exe


def decrypt(exe: Path, keys: Path, cooked: Path, packages: list[str], out: Path) -> None:
    todo = [p for p in packages if not (out / f"{p}.upk").exists()]
    if not todo:
        return
    print(f"decrypting {len(todo)} package(s) ...")
    r = run([str(exe), str(keys), str(out)] + [str(cooked / f"{p}.upk") for p in todo])
    print("  " + "\n  ".join(l for l in r.stdout.splitlines() if l.startswith(("ok", "FAIL"))))
    if r.returncode != 0:
        sys.exit("decryption failed (unknown key? update RL-UPKSuite's keys.txt)")


def link_texture_caches(cooked: Path, out: Path) -> None:
    """UModel streams mips from Textures*.tfc next to the packages. Hard-link instead of copying 20+ GB."""
    for tfc in cooked.glob("*.tfc"):
        dst = out / tfc.name
        if dst.exists():
            continue
        try:
            os.link(tfc, dst)
        except OSError as e:
            sys.exit(f"cannot hard-link {tfc} -> {dst} ({e}). Put --work on the same drive as the game.")


def umodel_export(umodel: Path, pkgs: Path, out: Path, package: str, obj: str, groups: bool = False) -> None:
    if not groups and (out / package).exists() and any((out / package).rglob(f"{obj}.*")):
        return  # exported by an earlier run
    if not umodel.is_file():
        # Re-running from an existing work folder without UModel: use what is there.
        print(f"  (no UModel: {package}.{obj} was not exported earlier, skipped)")
        return
    r = run([str(umodel), "-game=rocketleague", "-export", "-gltf", "-png", *(["-groups"] if groups else []), f"-out={out}", f"-path={pkgs}", package, obj])
    log = r.stdout + r.stderr
    if r.returncode != 0 or "*** ERROR" in log:
        sys.exit(f"UModel failed to export {package}.{obj}:\n{log[-2000:]}")


def umodel_export_grouped(umodel: Path, pkgs: Path, out: Path, package: str, obj: str, kind: str) -> list[Path]:
    """Exports every object named `obj` into folders named after its groups; returns the files."""
    ext = ".gltf" if kind == "StaticMesh" else ".png"
    hits = sorted((out / package).rglob(f"{obj}{ext}")) if (out / package).exists() else []
    if not hits:
        umodel_export(umodel, pkgs, out, package, obj, groups=True)
        hits = sorted((out / package).rglob(f"{obj}{ext}"))
    return hits


# ------------------------------------------------------------------------------------ materials


def parse_props(path: Path) -> dict:
    """Flatten a UModel *.props.txt: parameter name -> value string (textures as object names)."""
    text = path.read_text(errors="replace")
    out = {}
    parent = re.search(r"^Parent = \w+'([^']*)'", text, re.M)
    out["__parent__"] = parent.group(1) if parent else ""
    for value, name in re.findall(r"ParameterValue = (.*?)\n\s*ParameterName = (\w+)", text):
        tex = re.match(r"Texture2D'([^']*)'", value)
        if tex:
            out[name] = ("tex", tex.group(1).split(".")[-1])
            continue
        col = re.match(r"\{ R=([-\d.e]+), G=([-\d.e]+), B=([-\d.e]+), A=([-\d.e]+) \}", value)
        if col:
            out[name] = ("color", tuple(float(c) for c in col.groups()))
            continue
        try:
            out[name] = ("scalar", float(value))
        except ValueError:
            pass
    return out


def srgb_to_linear(x: np.ndarray) -> np.ndarray:
    return np.where(x <= 0.04045, x / 12.92, ((x + 0.055) / 1.055) ** 2.4)


def linear_to_srgb(x: np.ndarray) -> np.ndarray:
    x = np.clip(x, 0.0, 1.0)
    return np.where(x <= 0.0031308, x * 12.92, 1.055 * np.power(x, 1 / 2.4) - 0.055)


def to_u8(x: np.ndarray) -> np.ndarray:
    return (np.clip(x, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)


class Textures:
    """Finds exported PNGs by object name (the car's own package first)."""

    def __init__(self, export: Path, package: str):
        self.dirs = [export / package, export]

    def path(self, name: str) -> Path | None:
        for d in self.dirs:
            hits = sorted(d.rglob(f"Texture2D/{name}.png"))
            if hits:
                return hits[0]
        return None

    def load(self, name: str) -> np.ndarray | None:
        p = self.path(name)
        return None if p is None else np.asarray(Image.open(p).convert("RGBA")).astype(np.float32) / 255.0


def fix_normal(n: np.ndarray) -> np.ndarray:
    """UE3 normal maps here are either plain RGB or swizzled with X in alpha (R stuck at 1).
    Returns a glTF (OpenGL, +Y) tangent-space normal map as RGB in 0..1."""
    if n[..., 0].min() > 0.99:  # swizzled: X = A, Y = G
        x, y = n[..., 3] * 2 - 1, n[..., 1] * 2 - 1
    else:
        x, y = n[..., 0] * 2 - 1, n[..., 1] * 2 - 1
    y = -y  # DirectX (UE) -> OpenGL (glTF) green
    z = np.sqrt(np.clip(1 - x * x - y * y, 0, 1))
    return np.stack([x, y, z], -1) * 0.5 + 0.5


class MaterialBaker:
    def __init__(self, out_dir: Path, textures: Textures):
        self.out_dir = out_dir
        self.tex = textures
        self.images: list[str] = []

    def save(self, name: str, rgb: np.ndarray, alpha: np.ndarray | None = None) -> int:
        """Writes a PNG next to the glTF and returns its glTF texture index."""
        arr = to_u8(rgb) if alpha is None else np.dstack([to_u8(rgb), to_u8(alpha)])
        Image.fromarray(arr).save(self.out_dir / f"{name}.png", optimize=False, compress_level=4)
        if name not in self.images:
            self.images.append(name)
        return self.images.index(name)

    def bake(self, mat_name: str, props: dict, team: str, team_tag: str) -> dict:
        """Returns a glTF material for one UE material instance."""
        p = props
        lname = mat_name.lower()
        get_tex = lambda key: self.tex.load(p[key][1]) if key in p and p[key][0] == "tex" else None
        get_col = lambda key, default: p[key][1][:3] if key in p and p[key][0] == "color" else default
        get_scalar = lambda key, default: p[key][1] if key in p and p[key][0] == "scalar" else default

        if "RimDiffuse" in p:
            return self.bake_wheel(mat_name, p)
        if "Masks" in p and "Diffuse" in p:
            d, masks, normal = get_tex("Diffuse"), get_tex("Masks"), get_tex("Normal")
            base = d[..., :3]
            metal = masks[..., 3] if masks is not None else np.zeros(base.shape[:2], np.float32)
            orm = np.stack([np.ones_like(metal), 0.75 - 0.4 * metal, 0.6 * metal], -1)
            m = {
                "name": mat_name,
                "pbrMetallicRoughness": {
                    "baseColorTexture": {"index": self.save(f"{mat_name}_base", base)},
                    "metallicRoughnessTexture": {"index": self.save(f"{mat_name}_orm", orm)},
                },
            }
            if normal is not None:
                m["normalTexture"] = {"index": self.save(f"{mat_name}_normal", fix_normal(normal))}
            if masks is not None:
                head = np.array(get_col("HeadlightColor", (0.56, 0.73, 1.0))) * get_scalar("HeadlightBrightness", 10.0)
                tail = np.array(get_col("TailLightColor", (1.0, 0.02, 0.0))) * get_scalar("TailLightBrightness", 3.0)
                # G = headlights (front of the car), R = tail lights, B = brake/reverse (dim tail colour).
                emit = masks[..., 1:2] * head + masks[..., 0:1] * tail + masks[..., 2:3] * tail * 0.25
                peak = float(emit.max())
                if peak > 0:
                    m["emissiveTexture"] = {"index": self.save(f"{mat_name}_emissive", linear_to_srgb(emit / peak))}
                    m["emissiveFactor"] = [1.0, 1.0, 1.0]
                    m["extensions"] = {"KHR_materials_emissive_strength": {"emissiveStrength": peak}}
            return m
        if "Diffuse" in p and any(k in p for k in ("Skin", "BodyMasks", "CurvaturePack")):
            d = get_tex("Diffuse")
            skin = get_tex("Skin") if "Skin" in p else get_tex("BodyMasks")
            lum = srgb_to_linear(d[..., 0])  # paint brightness (R also holds it on tinted diffuses)
            paint = skin[..., 0] if skin is not None else np.clip((d[..., 0] - 0.5) * 4, 0, 1)
            if paint.shape != lum.shape:
                paint = np.asarray(Image.fromarray(to_u8(paint)).resize(lum.shape[::-1], Image.BILINEAR)) / 255.0
            tint = 1 + (np.array(team)[None, None, :] - 1) * paint[..., None]
            base = linear_to_srgb(lum[..., None] * tint)
            orm = np.stack([np.ones_like(paint), 0.55 - 0.25 * paint, np.zeros_like(paint)], -1)
            return {
                "name": mat_name,
                "pbrMetallicRoughness": {
                    "baseColorTexture": {"index": self.save(f"{mat_name}_{team_tag}_base", base)},
                    "metallicRoughnessTexture": {"index": self.save(f"{mat_name}_orm", orm)},
                },
                "extensions": {"KHR_materials_clearcoat": {"clearcoatFactor": 1.0, "clearcoatRoughnessFactor": 0.08}},
            }
        if any(k in lname for k in ("glass", "windshield", "lens")):
            return {
                "name": mat_name,
                "alphaMode": "BLEND",
                "pbrMetallicRoughness": {"baseColorFactor": [0.01, 0.012, 0.015, 0.85], "metallicFactor": 0.0, "roughnessFactor": 0.05},
            }
        if "eye" in lname:
            return {
                "name": mat_name,
                "pbrMetallicRoughness": {"baseColorFactor": [0.8, 0.8, 0.8, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.15},
                "emissiveFactor": [0.6, 0.85, 1.0],
                "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": 0.5}},
            }
        # Trim and anything else: dark satin plastic/metal.
        return {
            "name": mat_name,
            "pbrMetallicRoughness": {"baseColorFactor": [0.03, 0.03, 0.03, 1.0], "metallicFactor": 0.3, "roughnessFactor": 0.45},
        }

    def bake_wheel(self, mat_name: str, p: dict) -> dict:
        rim_d = self.tex.load(p["RimDiffuse"][1])
        rgb = self.tex.load(p["RimRGB"][1]) if "RimRGB" in p else None
        normal = self.tex.load(p["RimNormal"][1]) if "RimNormal" in p else None
        tire_d = self.tex.load(p["TireDiffuse"][1]) if "TireDiffuse" in p else None
        tire = (rgb[..., 0] > 0.5).astype(np.float32) if rgb is not None else np.zeros(rim_d.shape[:2], np.float32)
        tire_col = tire_d[..., :3].reshape(-1, 3).mean(0) if tire_d is not None else np.array([0.08, 0.08, 0.08])
        rim = np.clip(rim_d[..., :3] * 2.2, 0, 1)
        base = rim * (1 - tire[..., None]) + tire_col[None, None, :] * tire[..., None]
        orm = np.stack([np.ones_like(tire), 0.35 + 0.55 * tire, 0.85 * (1 - tire)], -1)
        m = {
            "name": mat_name,
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": self.save(f"{mat_name}_base", base)},
                "metallicRoughnessTexture": {"index": self.save(f"{mat_name}_orm", orm)},
            },
        }
        if normal is not None:
            m["normalTexture"] = {"index": self.save(f"{mat_name}_normal", fix_normal(normal))}
        return m


def bake_ball(baker: MaterialBaker, diffuse: str, masks: str) -> dict:
    d = baker.tex.load(diffuse)
    m = baker.tex.load(masks)
    base = np.clip(d[..., :3] * 1.5, 0, 1)
    out = {
        "name": "Ball",
        "pbrMetallicRoughness": {"baseColorTexture": {"index": baker.save("Ball_base", base)}, "metallicFactor": 0.2, "roughnessFactor": 0.45},
    }
    if m is not None:
        glow = (m[..., 0:1] > 0.5).astype(np.float32) * d[..., :3]
        if float(glow.max()) > 0:
            out["emissiveTexture"] = {"index": baker.save("Ball_emissive", np.clip(glow * 2.0, 0, 1))}
            out["emissiveFactor"] = [1.0, 1.0, 1.0]
    return out


class StaticMesh(NamedTuple):
    pos: np.ndarray  # (n, 3) glTF axes, metres
    normal: np.ndarray  # (n, 3)
    tangent: np.ndarray  # (n, 4) xyz and the bitangent sign, glTF convention
    uv: np.ndarray  # (n, 2) the first UV set
    color: np.ndarray | None  # (n, 4) RGBA 0..1, when the mesh has vertex colours
    idx: np.ndarray  # triangle indices


def read_static_mesh(package: Path, path: str) -> StaticMesh:
    """LOD 0 of a cooked StaticMesh, read straight from the package.

    UModel's export of the ball scrambles its UVs (half of them come out as 0, 0): its single,
    half-precision UV channel is misread; and it drops the vertex colours (the wheel's material
    tells the tyre from the rim by them). The buffers are found by their headers: the position
    buffer (stride 12, n, then a bulk array of n 12-byte vectors), the vertex buffer right after it
    (texture coordinate count, stride, n, full-precision flag, then a bulk array of n elements:
    packed tangent X and Z, the UVs), the colour buffer when there is one (stride 4, n, then a bulk
    array of n BGRA colours), and the index buffer (n, then a bulk array of 2-byte indices).
    """
    p = Package.open(package)
    e = p.export_at(path)
    if e is None:
        raise SystemExit(f"{path} not in {package.name}")
    d = p.data[e.offset : e.offset + e.size]
    for i in range(len(d) - 40):
        stride, n, esize, count = struct.unpack_from("<4i", d, i)
        if stride != 12 or esize != 12 or n <= 0 or count != n or i + 16 + 12 * n + 24 > len(d):
            continue
        vb = i + 16 + 12 * n
        uvs, vstride, vn, full, vesize, vcount = struct.unpack_from("<6i", d, vb)
        if vn == n and vcount == n and vstride == vesize and 1 <= uvs <= 8 and vstride == 8 + uvs * (8 if full else 4):
            break
    else:
        raise SystemExit(f"{path}: vertex buffers not found")
    pos = np.frombuffer(d, "<f4", count=3 * n, offset=i + 16).reshape(n, 3)
    vend = vb + 24 + vstride * n
    vert = np.frombuffer(d, np.uint8, count=vstride * n, offset=vb + 24).reshape(n, vstride)
    unit = lambda v: v / np.maximum(np.linalg.norm(v, axis=1, keepdims=True), 1e-6)
    tangent = unit(vert[:, 0:3].astype(np.float32) / 127.5 - 1.0)
    normal = unit(vert[:, 4:7].astype(np.float32) / 127.5 - 1.0)
    # The bitangent sign in TangentZ.W, as UModel writes it (checked against its export of the wheel).
    sign = np.where(vert[:, 7] >= 128, 1.0, -1.0).astype(np.float32)
    uv = (np.frombuffer(vert[:, 8:16].tobytes(), "<f4") if full else np.frombuffer(vert[:, 8:12].tobytes(), "<f2")).reshape(n, 2)
    color = None
    cstride, cn = struct.unpack_from("<2i", d, vend)
    if cstride == 4 and cn == n and struct.unpack_from("<2i", d, vend + 8) == (4, n):
        bgra = np.frombuffer(d, np.uint8, count=4 * n, offset=vend + 16).reshape(n, 4)
        color = bgra[:, [2, 1, 0, 3]].astype(np.float32) / 255.0
        vend += 16 + 4 * n
    for j in range(vend, len(d) - 12):
        vn2, isize, icount = struct.unpack_from("<3i", d, j)
        if vn2 == n and isize == 2 and icount > 0 and icount % 3 == 0 and j + 12 + 2 * icount <= len(d):
            idx = np.frombuffer(d, "<u2", count=icount, offset=j + 12)
            if idx.max() < n:
                break
    else:
        raise SystemExit(f"{path}: index buffer not found")
    # Unreal (X, Y, Z) uu -> glTF (X, Z, Y) m, as UModel writes it (the swap keeps the winding).
    swap = [0, 2, 1]
    return StaticMesh(pos[:, swap] / 100.0, normal[:, swap], np.concatenate([tangent[:, swap], sign[:, None]], 1),
                      uv.astype(np.float32), color, idx.astype(np.uint16))


def write_mesh_gltf(out: Path, mesh: StaticMesh, material: str) -> None:
    """A one-primitive glTF (+ .bin of the same stem) in UModel's layout, for `write_gltf`. The vertex
    colours go in `_RL_VERTEX_COLOR` (read by the Minecraft mod), not COLOR_0: glTF viewers and Bevy
    multiply COLOR_0 into the base colour, and the game's materials use them as masks."""
    arrays = [("POSITION", mesh.pos, "VEC3"), ("NORMAL", mesh.normal, "VEC3"), ("TANGENT", mesh.tangent, "VEC4"), ("TEXCOORD_0", mesh.uv, "VEC2")]
    if mesh.color is not None:
        arrays.append(("_RL_VERTEX_COLOR", mesh.color, "VEC4"))
    parts = [mesh.idx.astype("<u2").tobytes()] + [a.astype("<f4").tobytes() for _, a, _ in arrays]
    parts[0] += b"\0" * (-len(parts[0]) % 4)
    views, offset = [], 0
    for b in parts:
        views.append({"buffer": 0, "byteOffset": offset, "byteLength": len(b)})
        offset += len(b)
    out.with_suffix(".bin").write_bytes(b"".join(parts))
    g = {
        "asset": {"generator": "tools/rl_assets/extract.py", "version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": out.stem, "mesh": 0}],
        "materials": [{"name": material}],
        "meshes": [{"name": out.stem, "primitives": [{"attributes": {name: i + 1 for i, (name, _, _) in enumerate(arrays)}, "indices": 0, "material": 0}]}],
        "buffers": [{"uri": f"{out.stem}.bin", "byteLength": offset}],
        "bufferViews": views,
        "accessors": [{"bufferView": 0, "componentType": 5123, "count": len(mesh.idx), "type": "SCALAR"}] + [
            {"bufferView": i + 1, "componentType": 5126, "count": len(a), "type": t} for i, (_, a, t) in enumerate(arrays)
        ],
    }
    g["accessors"][1].update(min=mesh.pos.min(0).tolist(), max=mesh.pos.max(0).tolist())
    out.write_text(json.dumps(g, indent=1))


def write_ball_shading(textures: Textures, ball_dir: Path, diffuse: str, masks: str) -> None:
    """Inputs of the Minecraft mod's ports of the ball's shaders: `MAT_Ball_V3`'s four textures as the
    game samples them (unbaked) in `materials.json` (keyed by the glTF material, "Ball"), with the
    team colours its `TeamColor_WorldSpace` function blends between, and the ground reticle's texture."""
    tex = {}
    for key, name in (("Diffuse", diffuse), ("Normal", BALL_NORMAL), ("Detail", BALL_DETAIL[1]), ("Mask", masks), ("Reticle", BALL_RETICLE)):
        src = textures.path(name)
        if src is None:
            print(f"  ball: texture {name} was not exported; the mod draws the ball with its base colour")
            return
        shutil.copy2(src, ball_dir / f"{name}.png")
        tex[key] = f"{name}.png"
    reticle = tex.pop("Reticle")
    doc = {
        "materials": {"Ball": {"kind": "ball", "base": "MAT_Ball_V3", "textures": tex}},
        "teams": {k: list(v) for k, v in TEAMS.items()},
        "reticle": reticle,
    }
    (ball_dir / "materials.json").write_text(json.dumps(doc, indent=1))


def write_gltf(src_gltf: Path, out_dir: Path, out_name: str, materials: list[dict], images: list[str]) -> None:
    g = json.loads(src_gltf.read_text())
    bin_name = f"{src_gltf.stem}.bin"
    shutil.copy2(src_gltf.with_suffix(".bin"), out_dir / bin_name)
    for b in g["buffers"]:
        b["uri"] = bin_name
    # Keep only the images these materials use (the baker's list is shared by both team variants).
    refs = [t for m in materials for t in (*m.get("pbrMetallicRoughness", {}).values(), m.get("normalTexture"), m.get("emissiveTexture")) if isinstance(t, dict) and "index" in t]
    used = sorted({t["index"] for t in refs})
    remap = {old: new for new, old in enumerate(used)}
    for t in refs:
        t["index"] = remap[t["index"]]
    g["materials"] = materials
    g["samplers"] = [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]
    g["images"] = [{"uri": f"{images[i]}.png"} for i in used]
    g["textures"] = [{"sampler": 0, "source": i} for i in range(len(used))]
    used = {e for m in materials for e in m.get("extensions", {})}
    g["extensionsUsed"] = sorted(used)
    (out_dir / out_name).write_text(json.dumps(g, indent=1))


def write_wheel_anchors(src_gltf: Path, out: Path) -> None:
    """Bind-pose positions of the FL/FR/BL/BR wheel hub bones (`*_Disc_jnt`), in the glTF frame.
    The game hangs its wheel meshes on these bones; the Psyclops' physics merges its two front wheels
    into one on the centre line, so the visuals need the model's own wheel positions."""
    g = json.loads(src_gltf.read_text())
    buf = src_gltf.with_suffix(".bin").read_bytes()
    skin = g["skins"][0]
    acc = g["accessors"][skin["inverseBindMatrices"]]
    view = g["bufferViews"][acc["bufferView"]]
    start = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
    ibms = np.frombuffer(buf, np.float32, count=16 * acc["count"], offset=start).reshape(-1, 4, 4)
    lines = []
    for corner in ("FL", "FR", "BL", "BR"):
        idx = next(i for i, j in enumerate(skin["joints"]) if g["nodes"][j]["name"] == f"{corner}_Disc_jnt")
        world = np.linalg.inv(ibms[idx].T)  # glTF matrices are column-major
        lines.append(f"{corner} {world[0, 3]:.5f} {world[1, 3]:.5f} {world[2, 3]:.5f}")
    out.write_text("\n".join(lines) + "\n")


def material_props(export: Path, package: str, name: str) -> dict:
    hits = sorted((export / package).rglob(f"{name}.props.txt")) or sorted(export.rglob(f"{name}.props.txt"))
    return parse_props(hits[0]) if hits else {}


# ------------------------------------------------------------------------------------ main


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--game", type=Path, default=DEFAULT_GAME, help="Rocket League install folder")
    ap.add_argument("--umodel", type=Path, required=True, help="folder with umodel_64.exe")
    ap.add_argument("--upksuite", type=Path, required=True, help="RL-UPKSuite release folder (Core.dll, keys.txt)")
    ap.add_argument("--wwiser", type=Path, help="wwiser.pyz (https://github.com/bnnm/wwiser), for the sounds")
    ap.add_argument("--vgmstream", type=Path, help="vgmstream-cli executable (https://github.com/vgmstream/vgmstream), for the sounds")
    ap.add_argument("--out", type=Path, default=REPO / "assets" / "rl")
    ap.add_argument("--work", type=Path, default=REPO / "target" / "rl_assets_work")
    args = ap.parse_args()

    cooked = args.game / "TAGame" / "CookedPCConsole"
    if not (cooked / "Startup.upk").exists():
        sys.exit(f"no Rocket League install at {args.game}")
    umodel = args.umodel / "umodel_64.exe"
    keys = args.upksuite / "keys.txt"
    pkgs, export = args.work / "packages", args.work / "export"
    for d in (pkgs, export, args.out):
        d.mkdir(parents=True, exist_ok=True)

    exe = build_decryptor(args.upksuite.resolve(), args.work.resolve())
    decrypt(exe, keys, cooked, sorted({p for p, _ in CARS.values()} | {WHEEL[0], BALL[0]} | set(BOOST_PACKAGES) | set(AUDIO_PACKAGES) | set(FX_PACKAGES) | set(HUD_PACKAGES)), pkgs)
    link_texture_caches(cooked, pkgs)

    for preset, (package, mesh) in CARS.items():
        print(f"{preset}: {package}.{mesh}")
        umodel_export(umodel, pkgs, export, package, mesh)
        src = next((export / package).rglob(f"SkeletalMesh3/{mesh}.gltf"))
        mat_names = [m["name"] for m in json.loads(src.read_text())["materials"]]
        for name in mat_names:
            umodel_export(umodel, pkgs, export, package, name)
        car_dir = args.out / "cars" / preset
        car_dir.mkdir(parents=True, exist_ok=True)
        baker = MaterialBaker(car_dir, Textures(export, package))
        for team, color in TEAMS.items():
            mats = [baker.bake(n, material_props(export, package, n), color, team) for n in mat_names]
            write_gltf(src, car_dir, f"body_{team}.gltf", mats, list(baker.images))
        write_wheel_anchors(src, car_dir / "wheels.txt")
        ShadingWriter(export, args.out, Textures(export, package).path).write(mat_names, car_dir, TEAMS)

    ShadingWriter(export, args.out, Textures(export, "Startup").path).write_shared()

    package, mesh, mat = WHEEL
    print(f"wheel: {package}.{mesh}")
    umodel_export(umodel, pkgs, export, package, mat)
    # The mesh from the package itself, with its vertex colours (UModel drops them): the wheel's
    # material tells the tyre (red) from the rim by them.
    src = args.work / "wheel_mesh" / f"{mesh}.gltf"
    src.parent.mkdir(parents=True, exist_ok=True)
    write_mesh_gltf(src, read_static_mesh(pkgs / f"{package}.upk", f"WHEEL_Star.{mesh}"), mat)
    wheel_dir = args.out / "wheel"
    wheel_dir.mkdir(parents=True, exist_ok=True)
    baker = MaterialBaker(wheel_dir, Textures(export, package))
    mats = [baker.bake(m["name"], material_props(export, package, m["name"]), TEAMS["blue"], "blue") for m in json.loads(src.read_text())["materials"]]
    write_gltf(src, wheel_dir, "wheel.gltf", mats, baker.images)
    ShadingWriter(export, args.out, Textures(export, package).path).write([m["name"] for m in json.loads(src.read_text())["materials"]], wheel_dir)

    package, mesh, diffuse, masks = BALL
    print(f"ball: {package}.{mesh}")
    umodel_export(umodel, pkgs, export, package, mesh)
    for tex in (diffuse, masks, BALL_NORMAL, BALL_RETICLE):
        umodel_export(umodel, pkgs, export, package, tex)
    umodel_export(umodel, pkgs, export, *BALL_DETAIL)
    # The mesh from the package itself (UModel's export of it has scrambled UVs).
    src = args.work / "ball_mesh" / "Ball_Default.gltf"
    src.parent.mkdir(parents=True, exist_ok=True)
    write_mesh_gltf(src, read_static_mesh(pkgs / f"{package}.upk", f"Ball_Default.Meshes.{mesh}"), "MAT_Ball_V3")
    ball_dir = args.out / "ball"
    ball_dir.mkdir(parents=True, exist_ok=True)
    baker = MaterialBaker(ball_dir, Textures(export, package))
    write_gltf(src, ball_dir, "ball.gltf", [bake_ball(baker, diffuse, masks)], baker.images)
    write_ball_shading(Textures(export, package), ball_dir, diffuse, masks)

    print("boost: flame cones, smoke trail")
    grouped = args.work / "export_groups"
    Boost(pkgs, args.out, CARS, lambda package, obj, kind: umodel_export_grouped(umodel, pkgs, grouped, package, obj, kind)).run()

    print("hud: boost meter")
    BoostMeter(pkgs, args.out).run()

    print("fx: jump/dodge/supersonic/impact particles, camera shakes, rumble")
    FX(pkgs, args.out, lambda package, obj, kind: umodel_export_grouped(umodel, pkgs, grouped, package, obj, kind)).run()

    if args.wwiser and args.vgmstream:
        print("audio: car sounds from the Wwise banks")
        Audio(pkgs, cooked, args.out, args.work, args.wwiser.resolve(), args.vgmstream.resolve()).run()
    else:
        print("audio: skipped (pass --wwiser and --vgmstream to extract the car sounds)")

    (args.out / "README.txt").write_text(
        "Extracted from a local Rocket League install by tools/rl_assets/extract.py.\n"
        "Rocket League assets (c) Psyonix / Epic Games. Personal local use only: do not commit or redistribute.\n"
    )
    print(f"done -> {args.out}")


if __name__ == "__main__":
    main()
