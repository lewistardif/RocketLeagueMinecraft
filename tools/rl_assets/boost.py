"""Rocket League's default boost ("Standard") from the decrypted game packages.

What the game shows while a car boosts with the default boost, and where the data comes from:

* the flame cones: the body's `BoostConeMesh*` FX attachments, whose meshes and placement the boost
  replaces per car body (`Parent_Boost_Mesh` archetype, `MeshOverrides`), drawn with the boost's
  material `MasterBoost_Standard_MIC` (parent `BoostMesh_Paintable_MAT`). Its parameters and
  textures are extracted here; the shading itself is a port of the game's compiled pixel shader
  (see `crates/rl_car_bevy/src/shaders/boost_flame.wgsl`);
* the smoke trail: particle system `Boost_Painted_PS` while boosting, and `Drive_PS` while only
  throttling, both with material `SmokePuff_Mat`, emitted from the body's boost sockets.

Writes into `<out>/boost/`:
  boost.json                  material parameters, the two particle emitters, per-car placement
  cones_<preset>.gltf/.bin    each car's flame cones, baked into the car model's frame
  *.png                       the textures the two materials sample

Particle curves are kept as UE3 cooked them (lookup tables), and the runtimes sample them the way
the engine does (`FRawDistribution::GetValue`).
"""

from __future__ import annotations

import json
import math
import shutil
from pathlib import Path

import numpy as np

from cascade import Cascade, instance_params, ref, vec
from ue3 import Package

BOOST_PACKAGE = "Boost_Standard_SF"
FX_ACTOR = "Boost_Standard.FX.FXActor"
BOOST_MESH_ARCHETYPE = "Parent_Boost_Mesh.Archetype.Parent_Boost_Mesh"
FLAME_MATERIAL = "Boost_Standard.Materials.MasterBoost_Standard_MIC"
SMOKE_TEXTURES = {  # SmokePuff_Mat samplers 0, 1, 2 (its uniform texture order)
    "smoke": ("Boost_Standard_SF", "Ball_FX.Textures.Smoke01_D"),
    "gradient": ("Boost_Standard_SF", "Gradient_FX.Mat.Sphere_Gradient01_D"),
    "radial": ("Startup", "FX_Textures.Gradients.Radial_Generic_Tiling_Pack"),
}
FLAME_TEXTURES = {  # BoostMesh_Paintable_MAT samplers 0, 1
    "noise": ("Startup", "FX_Water_Tex.Water_02_N"),
    "sparks": ("Startup", "Boost_Tests.Textures.ParticleSheet_T"),
}
FLAME_PARAMS = [
    "Brightness", "Inner_Speed", "Outer_Speed", "TileX", "TileY", "Inner_Sparks", "Outer_Sparks",
    "GradientAmount", "GradientSharpness", "FresnelBase", "FresnelEnd", "Opacity",
]
# Hitbox preset -> (body package, body product asset, body FX actor).
BODIES = {
    "octane": ("Body_Octane_SF", "Body_Octane.Body_Octane", "Body_Octane.FXActor"),
    "dominus": ("Body_MuscleCar_SF", "Body_MuscleCar.Body_MuscleCar", "Body_MuscleCar.FXActor"),
    "plank": ("Body_Orion_SF", "Body_Orion.Body_Orion", "Body_Orion.FXActor"),
    "breakout": ("Body_CarCar_SF", "Body_CarCar.Body_CarCar", "Body_CarCar.FXActor"),
    "hybrid": ("Body_Venom_SF", "Body_Venom.Body_Venom", "Body_Venom.FXActor"),
    "merc": ("Body_Vanquish_SF", "Body_Vanquish.Body_Vanquish", "Body_Vanquish.FXActor"),
    "psyclops": ("body_pixie_SF", "body_pixie.body_pixie", "body_pixie.FXActor_pixie"),
}
PACKAGES = sorted({BOOST_PACKAGE, "Startup", "Engine"} | {b[0] for b in BODIES.values()})


def ue_to_model(v) -> list[float]:
    """UE car space (X fwd, Y right, Z up, uu) -> car model space (X fwd, Y up, Z right, m)."""
    return [v[0] / 100.0, v[2] / 100.0, v[1] / 100.0]


def rotator_matrix(pitch: int, yaw: int, roll: int) -> np.ndarray:
    """UE3 FRotationMatrix (row vectors: v' = v @ M), rotator units are 65536 per turn."""
    p, y, r = (a * 2.0 * math.pi / 65536.0 for a in (pitch, yaw, roll))
    sp, cp, sy, cy, sr, cr = math.sin(p), math.cos(p), math.sin(y), math.cos(y), math.sin(r), math.cos(r)
    return np.array([
        [cp * cy, cp * sy, sp],
        [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp],
        [-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp],
    ])


class Boost(Cascade):
    def __init__(self, pkgs: Path, out: Path, cars: dict, export_fn):
        """`cars`: preset -> (package, skeletal mesh) of the extracted car models.
        `export_fn(package, object, out_dir)` exports one object with UModel (glTF / PNG, by group)."""
        super().__init__(pkgs)
        self.out = out
        self.dir = out / "boost"
        self.dir.mkdir(parents=True, exist_ok=True)
        self.cars = cars
        self.export = export_fn

    def run(self) -> None:
        data = {
            "source": "Boost_Standard (the default boost)",
            "flame": {"params": self.flame_params(), "textures": self.textures(FLAME_TEXTURES)},
            "smoke": {"textures": self.textures(SMOKE_TEXTURES)},
            "emitters": self.emitters(),
            "cars": {preset: self.car(preset) for preset in self.cars},
        }
        (self.dir / "boost.json").write_text(json.dumps(data, indent=1))

    # ------------------------------------------------------------------------------ material

    def flame_params(self) -> dict:
        """MasterBoost_Standard_MIC's values over its parent material's parameter defaults."""
        p = self.pkg(BOOST_PACKAGE)
        mic = p.props_at(FLAME_MATERIAL)
        parent = p.export_at(ref(mic.get("Parent")))
        out: dict = {}
        for e in p.exports:  # parent defaults (parameter expressions inside the parent material)
            if e.outer_index == parent.index and p.class_name(e) in ("MaterialExpressionScalarParameter", "MaterialExpressionVectorParameter"):
                ep = p.properties(e)
                d = ep.get("DefaultValue", 0.0 if "Scalar" in p.class_name(e) else None)
                out[ep["ParameterName"]] = vec(d, (0, 0, 0, 0)) if isinstance(d, dict) else d
        for s in mic.get("ScalarParameterValues", []):
            out[s["ParameterName"]] = s["ParameterValue"]
        for s in mic.get("VectorParameterValues", []):
            out[s["ParameterName"]] = vec(s["ParameterValue"])
        missing = [k for k in FLAME_PARAMS + ["CustomColor"] if out.get(k) is None]
        if missing:
            raise SystemExit(f"boost flame material: no value for {missing}")
        return {k: out[k] for k in FLAME_PARAMS + ["CustomColor"]}

    def textures(self, wanted: dict) -> dict:
        out = {}
        for key, (package, path) in wanted.items():
            name = path.split(".")[-1]
            dst = self.dir / f"{name}.png"
            if not dst.exists():
                src = self.export(package, name, "Texture2D")
                # Several textures can share a name: pick the one in the right group.
                groups = path.split(".")[1:-1]
                src = next((s for s in src if all(g in s.parts for g in groups)), None)
                if src is None:
                    raise SystemExit(f"UModel did not export {path}")
                shutil.copy2(src, dst)
            out[key] = dst.name
        return out

    # ------------------------------------------------------------------------------ particles

    def emitters(self) -> dict:
        p = self.pkg(BOOST_PACKAGE)
        fx = p.props_at(FX_ACTOR)
        out = {}
        for att in fx.get("Attachments", []):
            psc_path = ref(att.get("Component"))
            psc = p.props_at(psc_path) if psc_path and "ParticleSystemComponent" in psc_path else None
            if psc is None:
                continue
            params = instance_params(psc.get("InstanceParameters"))
            for t in att.get("Traits", []):  # the boost trait's parameters win
                params.update(instance_params(p.props_at(ref(t)).get("SharedParameters")))
            key = {"BoostParticle": "boost", "DrivingParticle": "drive"}.get(att["Name"])
            if key:
                out[key] = self.emitter(p, ref(psc["Template"]), params)
        if set(out) != {"boost", "drive"}:
            raise SystemExit(f"boost FX actor: expected the boost and driving particles, got {sorted(out)}")
        return out

    def emitter(self, p: Package, system: str, params: dict) -> dict:
        """The first (highest detail) LOD of the system's only sprite emitter."""
        ps = p.export_at(system)
        ems = [e for e in p.exports if e.outer_index == ps.index and p.class_name(e).endswith("Emitter")]
        if len(ems) != 1:
            raise SystemExit(f"{system}: expected one emitter, found {len(ems)}")
        lods = [p.props_at(ref(l)) for l in p.properties(ems[0])["LODLevels"]]
        lod = next(l for l in lods if l.get("Level", 0) == 0)
        req = self.module(p, ref(lod["RequiredModule"]))[1]
        e = {
            "system": system,
            "local_space": bool(req.get("bUseLocalSpace", False)),
            "subuv": [req.get("SubImages_Horizontal", 1), req.get("SubImages_Vertical", 1)],
            "subuv_mode": req.get("InterpolationMethod", "PSUVIM_None"),
            "spawn_rate": None, "spawn_per_unit": None, "lifetime": None, "size": None, "size_life": None,
            "velocity": None, "vel_life": None, "accel": None, "rotation": None,
            "color": None, "alpha": None, "color_life": None, "alpha_life": None,
        }
        for m in [ref(lod["SpawnModule"])] + [ref(m) for m in lod.get("Modules", [])]:
            cls, mp = self.module(p, m)
            if mp.get("bEnabled") is False:
                continue
            d = lambda key, dim: self.dist(p, mp.get(key), dim, params)
            if cls == "Spawn":
                e["spawn_rate"] = {"rate": d("Rate", 1), "scale": d("RateScale", 1)}
            elif cls == "SpawnPerUnit":
                e["spawn_per_unit"] = {"unit": mp["UnitScalar"], "count": d("SpawnPerUnit", 1), "max_frame_distance": mp.get("MaxFrameDistance", 0.0),
                                       "movement_tolerance": mp.get("MovementTolerance", 0.1)}
            elif cls == "Lifetime":
                e["lifetime"] = d("LifeTime", 1)
            elif cls == "Size":
                e["size"] = d("StartSize", 3)
            elif cls == "SizeMultiplyLife":
                e["size_life"] = d("LifeMultiplier", 3)
            elif cls == "Velocity":
                e["velocity"] = d("StartVelocity", 3)
            elif cls == "VelocityOverLifetime":
                e["vel_life"] = d("VelOverLife", 3)
            elif cls == "Acceleration":
                e["accel"] = d("Acceleration", 3)
            elif cls == "Rotation":
                e["rotation"] = d("StartRotation", 1)
            elif cls == "Color":
                e["color"], e["alpha"] = d("StartColor", 3), d("StartAlpha", 1)
            elif cls == "ColorScaleOverLife":
                e["color_life"], e["alpha_life"] = d("ColorScaleOverLife", 3), d("AlphaScaleOverLife", 1)
            elif cls in ("SubUV", "ParameterDynamic"):
                pass  # SubUV index is unused in random modes; SmokePuff_Mat does not read the dynamic parameter
            else:
                raise SystemExit(f"{system}: module {cls} is not supported")
        return e

    # ------------------------------------------------------------------------------ per car

    def car(self, preset: str) -> dict:
        body_pkg, asset_path, fx_path = BODIES[preset]
        bp = self.pkg(body_pkg)
        mesh_pkg, mesh_name = self.cars[preset]
        sockets = self.sockets(self.pkg(mesh_pkg), mesh_name)
        asset = bp.props_at(asset_path)
        emit = asset.get("BoostEmitterSockets") or ["RocketBoost"]
        if any(s not in sockets for s in emit):
            raise SystemExit(f"{preset}: boost sockets {emit} not all on {mesh_name}")

        fx = bp.props_at(fx_path)
        overrides = {}
        for mp in self.pkg("Startup").props_at(BOOST_MESH_ARCHETYPE)["MaterialParams"]:
            for o in mp["MeshOverrides"]:
                if o["CarTypePath"] == fx_path:
                    overrides[mp["MeshAttachmentName"]] = ref(o["Mesh"])
        cones, delay = [], 0.0
        for att in fx.get("Attachments", []):
            if not att["Name"].startswith("BoostConeMesh"):
                continue
            comp_path = overrides.get(att["Name"])
            cp = self.pkg("Startup").props_at(comp_path) if comp_path else bp.props_at(ref(att["Component"]))
            mesh = ref(cp.get("StaticMesh"))
            if not mesh:
                continue  # an empty slot that the boost does not fill for this car
            at = att["SocketOrBoneName"]
            if at != "chassis_jnt" and at not in sockets:
                raise SystemExit(f"{preset}: cone socket {at} is not a plain offset on {mesh_name}")
            cones.append({"mesh": mesh, "socket": sockets.get(at, [0.0, 0.0, 0.0]), "component": cp,
                          "package": "Startup" if comp_path else body_pkg})
            delay = max(delay, att.get("AttachDelay", 0.0))
        if not cones:
            raise SystemExit(f"{preset}: no boost cone")
        name = f"cones_{preset}.gltf"
        self.bake_cones(cones, self.dir / name)
        return {"cones": name, "cone_delay": delay, "emitters": [ue_to_model(sockets[s]) for s in emit]}

    @staticmethod
    def sockets(p: Package, mesh: str) -> dict:
        """Socket name -> location (uu) on the car mesh, for the sockets that are a plain offset from
        `chassis_jnt` (the boost's all are), which sits at the model origin in the bind pose."""
        out = {}
        for e in p.exports:
            if p.class_name(e) == "SkeletalMeshSocket" and (p.obj_name(e.outer_index) or "").lower() == mesh.lower():
                sp = p.properties(e)
                if sp.get("BoneName", "chassis_jnt") == "chassis_jnt" and not sp.get("RelativeRotation"):
                    out[sp["SocketName"]] = vec(sp.get("RelativeLocation"))
        return out

    def bake_cones(self, cones: list[dict], dst: Path) -> None:
        """All of a car's cones in one triangle list in the car model frame: position, normal and
        both UV sets (the material splits inner/outer shells on UV1.x; a mesh with one UV set
        repeats it, as UE3's local vertex factory does)."""
        pos, nrm, uv0, uv1, idx = [], [], [], [], []
        for c in cones:
            name = c["mesh"].split(".")[-1]
            pkg = "Startup" if self.pkg("Startup").export_at(c["mesh"]) else c["package"]
            src = next((s for s in self.export(pkg, name, "StaticMesh") if s.suffix == ".gltf"), None)
            if src is None:
                raise SystemExit(f"UModel did not export {c['mesh']}")
            g = json.loads(src.read_text())
            buf = src.with_suffix(".bin").read_bytes()
            prim = g["meshes"][0]["primitives"][0]
            a = prim["attributes"]
            p = accessor(g, buf, a["POSITION"])[:, [0, 2, 1]] * 100.0  # glTF -> UE mesh space
            n = accessor(g, buf, a["NORMAL"])[:, [0, 2, 1]]
            t0 = accessor(g, buf, a["TEXCOORD_0"])
            t1 = accessor(g, buf, a["TEXCOORD_1"]) if "TEXCOORD_1" in a else t0
            cp = c["component"]
            scale = np.array(vec(cp.get("Scale3D"), (1, 1, 1))) * cp.get("Scale", 1.0)
            rot = rotator_matrix(*vec(cp.get("Rotation"), (0, 0, 0)))
            p = (p * scale) @ rot + np.array(vec(cp.get("Translation"))) + np.array(c["socket"])
            n = (n / scale) @ rot
            n /= np.linalg.norm(n, axis=1, keepdims=True)
            base = sum(len(x) for x in pos)
            pos.append(p[:, [0, 2, 1]] / 100.0)  # UE -> model frame
            nrm.append(n[:, [0, 2, 1]])
            uv0.append(t0)
            uv1.append(t1)
            idx.append(accessor(g, buf, prim["indices"]).reshape(-1).astype(np.uint32) + base)
        write_gltf(dst, np.concatenate(pos), np.concatenate(nrm), np.concatenate(uv0), np.concatenate(uv1), np.concatenate(idx))


def accessor(g: dict, buf: bytes, index: int) -> np.ndarray:
    acc = g["accessors"][index]
    view = g["bufferViews"][acc["bufferView"]]
    comps = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[acc["type"]]
    dtype = {5126: np.float32, 5125: np.uint32, 5123: np.uint16, 5121: np.uint8}[acc["componentType"]]
    start = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
    stride = view.get("byteStride", 0)
    item = comps * np.dtype(dtype).itemsize
    if stride and stride != item:
        rows = [np.frombuffer(buf, dtype, count=comps, offset=start + i * stride) for i in range(acc["count"])]
        return np.array(rows).astype(np.float64 if dtype == np.float32 else dtype)
    out = np.frombuffer(buf, dtype, count=comps * acc["count"], offset=start).reshape(acc["count"], comps)
    return out.astype(np.float64) if dtype == np.float32 else out


def write_gltf(dst: Path, pos, nrm, uv0, uv1, idx) -> None:
    arrays = [(pos.astype("<f4"), "VEC3", 5126, 34962), (nrm.astype("<f4"), "VEC3", 5126, 34962), (uv0.astype("<f4"), "VEC2", 5126, 34962),
              (uv1.astype("<f4"), "VEC2", 5126, 34962), (idx.astype("<u4"), "SCALAR", 5125, 34963)]
    blob, views, accessors = b"", [], []
    for i, (a, kind, comp, target) in enumerate(arrays):
        views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": a.nbytes, "target": target})
        acc = {"bufferView": i, "componentType": comp, "count": len(a), "type": kind}
        if i == 0:
            acc["min"], acc["max"] = a.min(0).tolist(), a.max(0).tolist()
        accessors.append(acc)
        blob += a.tobytes()
    g = {
        "asset": {"version": "2.0", "generator": "rl_assets/boost.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": dst.stem}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2, "TEXCOORD_1": 3}, "indices": 4}]}],
        "buffers": [{"uri": dst.with_suffix(".bin").name, "byteLength": len(blob)}],
        "bufferViews": views,
        "accessors": accessors,
    }
    dst.with_suffix(".bin").write_bytes(blob)
    dst.write_text(json.dumps(g, indent=1))
