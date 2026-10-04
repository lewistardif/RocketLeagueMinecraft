"""Inputs for the ports of Rocket League's car material shaders (Minecraft mod).

The game draws car bodies with `Body_Paintable_Mat`, chassis with `MasterChassis_MAT` (and a few
variants with the same parameters) and wheels with `Wheel_Master_Mat`. Their compiled pixel shaders,
read from the game's shader cache (`RefShaderCache-PC-D3D-SM5.upk`), are ported line by line in
`minecraft/src/client/resources/assets/rlcar/shaders/core/rl_*.fsh`. Those ports need the
material's own textures, unbaked, and its parameter values; this module writes them:

  cars/<preset>/materials.json   per glTF material: shader kind, texture files, parameter values
  cars/<preset>/*.png            the textures as the game samples them (no baking, no swizzling)
  wheel/materials.json           the same for the wheel
  shading/*.png                  textures shared by every car: the lighting ramp atlas
                                 (`LightFalloffArray`), the reflection pack (`ENVPack`), the default
                                 tertiary normal and flat stand-ins for unset textures

Parameter values follow the material instance chain (instance -> parent instances -> base
material defaults), from UModel's `*.props.txt`. Static switches are not visible there; the port
uses `BodyMasks` (the tertiary material mask) and `Normal` only when the car's own instances set
them, so a car never inherits another body's UV layout from a parent default.
"""

from __future__ import annotations

import json
import re
import shutil
from pathlib import Path

from PIL import Image

BODY_BASE = "Body_Paintable_Mat"
CHASSIS_BASES = {"MasterChassis_MAT", "MAT_Chassis_Paintable", "MAT_BANDAID_Chassis_Paintable", "GoodChassis_Painted_Mat"}
WHEEL_BASE = "Wheel_Master_Mat"
SHARED = {"LightFalloffArray": "lut", "ENVPack": "env", "CarbonFiber_Flipped_N": "tertiary_normal", "Swirls_D": "wheel_swirl"}

BODY_TEXTURES = ["Diffuse", "Skin", "CurvaturePack", "F1DetailNormal", "F2DetailNormal", "TertiaryMaterial_Normal"]
BODY_OWN_ONLY = ["BodyMasks", "Normal"]  # used only when set below the base material
BODY_PARAMS = [
    "TeamColor", "CustomColor", "PaintColor", "TrimColor", "F1ControlA", "F2ControlA", "F1ControlB", "F2ControlB",
    "TertiaryMaterial_ControlA", "TertiaryMaterial_ControlB", "TertiaryMaterial_Color", "TertiaryNormalTiling",
    "F1Type", "F2Type", "TertiaryMaterial_Type",
]
CHASSIS_TEXTURES = ["Diffuse", "Masks"]
CHASSIS_PARAMS = ["TailLightColor", "HeadlightColor", "BoostGlowColor", "Brake"]
WHEEL_TEXTURES = ["RimDiffuse", "RimNormal", "Rim_AdditionalNormal", "TireDiffuse", "TireNormal", "RimRGB"]
WHEEL_PARAMS = ["RimColor", "Rim_AdditionalNormal_Power", "ReflectionBrightness", "SpecIntensity", "SpecPower"]

# Values the shader cache's base materials compile in when nothing in the chain sets them.
FALLBACK = {
    "TertiaryMaterial_Normal": ("tex", "CarbonFiber_Flipped_N"),
    "F1DetailNormal": ("tex", "Blank_N"),
    "F2DetailNormal": ("tex", "Blank_N"),
}


# ------------------------------------------------------------------------------------ props


def find_props(export: Path, name: str) -> Path | None:
    hits = sorted(export.rglob(f"{name}.props.txt"))
    return hits[0] if hits else None


def _value(raw: str):
    tex = re.match(r"Texture2D'([^']*)'", raw)
    if tex:
        return ("tex", tex.group(1).split(".")[-1])
    col = re.match(r"\{ ?R=([-\d.e]+), G=([-\d.e]+), B=([-\d.e]+), A=([-\d.e]+) ?\}", raw)
    if col:
        return ("color", [float(c) for c in col.groups()])
    try:
        return ("scalar", float(raw))
    except ValueError:
        return None


def instance_values(text: str) -> dict:
    """Parameter values set by a material instance (`ParameterValue = ... ParameterName = ...`)."""
    out = {}
    for raw, name in re.findall(r"ParameterValue = (.*?)\n\s*ParameterName = (\w+)", text):
        v = _value(raw.strip())
        if v is not None:
            out.setdefault(name, v)
    return out


def material_defaults(text: str) -> dict:
    """A base material's parameter defaults (its `Collected*Parameters`, first entry per name)."""
    out = {}
    pairs = re.findall(r"(?:Texture|Value) = (.*?)\n\s*Name = (\w+)", text)
    pairs += [(raw, name) for raw, name in re.findall(r"\{ (?:Texture|Value)=(.*?), Name=(\w+)", text)]
    for raw, name in pairs:
        v = _value(raw.strip())
        if v is not None:
            out.setdefault(name, v)
    return out


def resolve(export: Path, name: str) -> tuple[str, dict, dict]:
    """(base material, all values, values set by instances only) along the instance chain."""
    values, own = {}, {}
    base = name
    seen = set()
    while name and name not in seen:
        seen.add(name)
        path = find_props(export, name)
        if path is None:
            break
        text = path.read_text(errors="replace")
        parent = re.search(r"^Parent = \w+'([^']*)'", text, re.M)
        if parent:
            for k, v in instance_values(text).items():
                values.setdefault(k, v)
                own.setdefault(k, v)
            name = parent.group(1).split(".")[-1]
            base = name
        else:
            for k, v in material_defaults(text).items():
                values.setdefault(k, v)
            base = name
            break
    return base, values, own


# ------------------------------------------------------------------------------------ writing


class ShadingWriter:
    def __init__(self, export: Path, out: Path, textures):
        """`textures(name)`: path of an exported PNG by texture object name, or None."""
        self.export = export
        self.out = out
        self.find = textures
        self.shared = out / "shading"

    def write_shared(self) -> None:
        self.shared.mkdir(parents=True, exist_ok=True)
        for name in SHARED:
            src = self.find(name)
            if src is None:
                raise SystemExit(f"shading: texture {name} was not exported")
            shutil.copy2(src, self.shared / f"{name}.png")
        flat = {
            "flat_normal": (128, 128, 255, 255),  # RGB normal map, alpha 1
            "flat_normal_xa": (255, 128, 255, 128),  # normal with X in alpha (UE3 swizzled)
            "black": (0, 0, 0, 0),
        }
        for name, rgba in flat.items():
            Image.new("RGBA", (4, 4), rgba).save(self.shared / f"{name}.png")

    def copy(self, name: str, dst_dir: Path) -> str | None:
        src = self.find(name)
        if src is None:
            return None
        dst = dst_dir / f"{name}.png"
        if not dst.exists():
            shutil.copy2(src, dst)
        return dst.name

    def material(self, mat_name: str, dst_dir: Path) -> dict:
        base, values, own = resolve(self.export, mat_name)
        lname = mat_name.lower()
        if base == BODY_BASE:
            return self.entry("body", base, values, BODY_TEXTURES, BODY_PARAMS, dst_dir,
                              extra={k: own.get(k) for k in BODY_OWN_ONLY})
        if base in CHASSIS_BASES:
            return self.entry("chassis", base, values, CHASSIS_TEXTURES, CHASSIS_PARAMS, dst_dir)
        if base == WHEEL_BASE:
            return self.entry("wheel", base, values, WHEEL_TEXTURES, WHEEL_PARAMS, dst_dir)
        if any(k in lname for k in ("glass", "windshield", "lens")):
            return {"kind": "glass", "base": base}
        return {"kind": "basic", "base": base}

    def entry(self, kind: str, base: str, values: dict, textures: list, params: list, dst_dir: Path, extra: dict | None = None) -> dict:
        tex = {}
        for key in textures:
            v = values.get(key) or FALLBACK.get(key)
            tex[key] = self.copy(v[1], dst_dir) if v and v[0] == "tex" else None
        for key, v in (extra or {}).items():
            tex[key] = self.copy(v[1], dst_dir) if v and v[0] == "tex" else None
        if tex.get("BodyMasks") and not self.has_alpha(dst_dir / tex["BodyMasks"]):
            # The tertiary mask is the alpha channel; a mask without one (a DXT1 skin) does not
            # select the tertiary material anywhere in the game either.
            tex["BodyMasks"] = None
        par = {}
        for key in params:
            v = values.get(key)
            if v is not None and v[0] in ("color", "scalar"):
                par[key] = v[1]
        return {"kind": kind, "base": base, "textures": tex, "params": par}

    @staticmethod
    def has_alpha(png: Path) -> bool:
        with Image.open(png) as im:
            if "A" not in im.getbands():
                return False
            lo, hi = im.getchannel("A").getextrema()
            return lo < 255

    def write(self, mat_names: list[str], dst_dir: Path, teams: dict | None = None) -> None:
        mats = {n: self.material(n, dst_dir) for n in mat_names}
        doc = {"materials": mats}
        if teams:
            doc["teams"] = {k: list(v) for k, v in teams.items()}
        (dst_dir / "materials.json").write_text(json.dumps(doc, indent=1))
