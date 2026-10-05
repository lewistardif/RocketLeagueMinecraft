"""The car's visual effects (besides the boost) and its camera shakes and rumble, from the decrypted
game packages.

What the game shows, and where it comes from:

* `FXActors.Car.Car_FXActor` (TAGame): the jump (`Jump_Metal_PS`), double jump and dodge
  (`Dodge_PS`) particles, the jump/dodge camera shakes, the supersonic wheel trails
  (`SupersonicWheelTemplate`, `WheelFX_Supersonic_PS`), and the wheel landing shake scaled by the
  impact momentum (`WheelImpactShake`, `ShakeScaleCurve`);
* `Body_FX.Body_FXActor` (each body package): the supersonic streaks around the car
  (`Supersonic_Team1_PS` / `Supersonic_Team2_PS`, by team);
* `Archetypes.Car.Car_Default.ImpactEffectsComponent` (GameInfo_Soccar_SF): the body impact sparks
  (`CarImpact_EffectsMap`, entry of the arena's physical material) and impact shake;
* the shake components' force feedback waveforms (gamepad rumble), and the boost's activation
  rumble (`BoostBase.FXActor`).

Particle systems are written as their emitters' module lists in order (the order the engine applies
them), distributions as cooked lookup tables (see `cascade.py`). Materials are written as their blend
mode and the textures their compiled shaders sample; their shading itself is not in the packages in a
readable form, so the runtime draws texture x particle colour (see `crates/rl_car_bevy/src/fx.rs`).

Writes into `<out>/fx/`: fx.json and the textures (PNG).
"""

from __future__ import annotations

import json
import shutil
import struct
from pathlib import Path

from cascade import Cascade, instance_params, ref, vec
from ue3 import Package

PACKAGES = ["TAGame", "Startup", "GameInfo_Soccar_SF", "Engine", "Boost_Standard_SF", "Body_Octane_SF"]
CAR_FX = ("TAGame", "FXActors.Car.Car_FXActor")
BODY_FX = ("Body_Octane_SF", "Body_FX.Body_FXActor")  # the parent of every body's FX actor
CAR_ARCHETYPE = ("GameInfo_Soccar_SF", "Archetypes.Car.Car_Default")
BOOST_BASE_FX = ("Boost_Standard_SF", "BoostBase.FXActor")
# The arena's collision surfaces use this physical material; its parent is the one the effects maps
# list (wheel effects have no entry for it, body impacts do).
ARENA_PHYSMAT = ("Startup", "PhysicalMaterials.Collision_Sticky")

# Spawn/update modules this runtime implements (anything else is reported, not silently dropped).
MODULES = {
    "Lifetime": {"LifeTime": 1},
    "Size": {"StartSize": 3},
    "SizeMultiplyLife": {"LifeMultiplier": 3},
    "Velocity": {"StartVelocity": 3, "StartVelocityRadial": 1},
    "VelocityOverLifetime": {"VelOverLife": 3},
    "VelocityInheritParent": {"Scale": 3},
    "Acceleration": {"Acceleration": 3},
    "Rotation": {"StartRotation": 1},
    "RotationRate": {"StartRotationRate": 1},
    "Color": {"StartColor": 3, "StartAlpha": 1},
    "ColorOverLife": {"ColorOverLife": 3, "AlphaOverLife": 1},
    "ColorScaleOverLife": {"ColorScaleOverLife": 3, "AlphaScaleOverLife": 1},
    "Location": {"StartLocation": 3},
    "LocationPrimitiveSphere": {"StartRadius": 1, "VelocityScale": 1, "StartLocation": 3},
    "LocationPrimitiveCylinder": {"StartRadius": 1, "StartHeight": 1, "VelocityScale": 1, "StartLocation": 3},
    "SubUV": {"SubImageIndex": 1},
    "CameraOffset": {"CameraOffset": 1},
    "OrientationAxisLock": {},
    "TrailSource": {},
}
FLAGS = [
    "bInWorldSpace", "bAlwaysInWorldSpace", "Absolute", "MultiplyX", "MultiplyY", "MultiplyZ", "bClampAlpha", "bEmitterTime",
    "Positive_X", "Positive_Y", "Positive_Z", "Negative_X", "Negative_Y", "Negative_Z", "SurfaceOnly", "Velocity", "RadialVelocity",
    "HeightAxis", "LockAxisFlags", "MaxAddedVelocity", "bSpawnTimeOnly", "UpdateMethod", "SourceOffsetDefaults", "SourceMethod",
]


class FX(Cascade):
    def __init__(self, pkgs: Path, out: Path, export_fn):
        super().__init__(pkgs)
        self.dir = out / "fx"
        self.dir.mkdir(parents=True, exist_ok=True)
        self.export = export_fn
        self.systems: dict[str, dict] = {}
        self.materials: dict[str, dict] = {}

    def run(self) -> None:
        effects = []
        car_fx = self.props(*CAR_FX)
        effects += self.fx_particles(*CAR_FX, "car")
        effects += self.fx_particles(*BODY_FX, "body")
        wheel = self.system(CAR_FX[0], ref(car_fx["SupersonicWheelTemplate"]), {})
        impacts = self.props(CAR_ARCHETYPE[0], ref(self.props(*CAR_ARCHETYPE)["ImpactEffectsComponent"]))
        body_impact = self.effects_map(CAR_ARCHETYPE[0], ref(impacts["ImpactEffectsMap"]))
        wheel_effect = self.effects_map(CAR_FX[0], ref(car_fx["WheelEffectsMap"]))
        data = {
            "systems": self.systems,
            "materials": self.materials,
            "effects": effects,
            "wheel_supersonic": wheel,
            "body_impact": body_impact,
            "wheel_surface": wheel_effect,
            "shakes": self.shakes(car_fx, impacts),
        }
        (self.dir / "fx.json").write_text(json.dumps(data, indent=1))

    def props(self, package: str, path: str | None) -> dict:
        """An object's properties, from `package` or, when it only holds an empty copy, from the
        first of PACKAGES that has them."""
        if not path:
            return {}
        out = self.pkg(package).props_at(path)
        for p in PACKAGES:
            if out:
                break
            out = self.pkg(p).props_at(path)
        return out

    def find(self, path: str) -> str:
        """The package (of PACKAGES) that holds an object."""
        for p in PACKAGES:
            if self.pkg(p).export_at(path):
                return p
        raise SystemExit(f"{path} is in none of {PACKAGES}")

    # ------------------------------------------------------------------------------ FX actors

    def fx_particles(self, package: str, fx_path: str, owner: str) -> list:
        """An FX actor's particle attachments: system, FX events, socket and offset."""
        out = []
        p = self.pkg(package)
        for att in self.props(package, fx_path).get("Attachments", []):
            comp = ref(att.get("Component"))
            if not comp or p.class_name(p.export_at(comp)) != "ParticleSystemComponent":
                continue
            cp = self.props(package, comp)
            params = instance_params(cp.get("InstanceParameters"))
            ev = lambda key: [ref(e).split(".")[-1] for e in att.get(key, [])]
            out.append({
                "name": f"{owner}.{att['Name']}",
                "system": self.system(package, ref(cp["Template"]), params),
                "attach_any": ev("AttachAny"), "attach_all": ev("AttachAll"), "detach_any": ev("DetachAny"),
                "socket": att.get("SocketOrBoneName", "None"),
                "offset": vec(cp.get("Translation")),
                "local_only": att.get("Target") == "FXComponentTarget_Local" or bool(cp.get("bOnlyOwnerSee")),
            })
        return out

    def effects_map(self, package: str, path: str) -> str | None:
        """The system an `EffectsMap_X` plays on the arena's surfaces: the entry of its physical
        material, else of the closest parent material."""
        entries = {ref(e["PhysicalMaterial"]): ref(e["Particle"]) for e in self.props(package, path).get("Effects", [])}
        mat = ARENA_PHYSMAT[1]
        while mat:
            if mat in entries:
                return self.system(package, entries[mat], {}) if entries[mat] else None
            mat = ref(self.props(self.find(mat), mat).get("Parent"))
        return None

    # ------------------------------------------------------------------------------ systems

    def system(self, package: str, path: str | None, params: dict) -> str | None:
        if not path:
            return None
        key = path if not params else f"{path}#{json.dumps(params, sort_keys=True)}"
        if key in self.systems:
            return key
        p = self.pkg(package) if self.pkg(package).export_at(path) else self.pkg(self.find(path))
        ps = p.props_at(path)
        emitters = [e for e in (self.emitter(p, ref(x), params) for x in ps.get("Emitters", [])) if e]
        self.systems[key] = {"source": path, "emitters": emitters}
        return key

    def emitter(self, p: Package, path: str, params: dict) -> dict | None:
        ep = p.props_at(path)
        if ep.get("bCookedOut"):
            return None
        lods = [p.props_at(ref(l)) for l in ep.get("LODLevels", [])]
        lod = next((l for l in lods if l.get("Level", 0) == 0), None)
        if lod is None or lod.get("bEnabled") is False:
            return None
        _, req = self.module(p, ref(lod["RequiredModule"]))
        material = self.material(p, ref(req.get("Material")))
        if material is None:
            return None  # distortion (refraction of the scene behind): not reproduced
        d = lambda mp, key, dim: self.dist(p, mp.get(key), dim, params)
        e = {
            "source": path,
            "kind": "sprite",
            "material": material,
            "local_space": bool(req.get("bUseLocalSpace", False)),
            "alignment": req.get("ScreenAlignment", "PSA_Square"),
            "subuv": [req.get("SubImages_Horizontal", 1), req.get("SubImages_Vertical", 1)],
            "subuv_mode": req.get("InterpolationMethod", "PSUVIM_None"),
            "duration": req.get("EmitterDuration", 1.0),
            "loops": req.get("EmitterLoops", 0),
            "delay": req.get("EmitterDelay", 0.0),
            "kill_on_completed": bool(req.get("bKillOnCompleted", False)),
            "spawn": None, "spawn_per_unit": None, "ribbon": None, "modules": [],
        }
        typedata = ref(lod.get("TypeDataModule"))
        if typedata:
            cls, tp = self.module(p, typedata)
            if cls != "TypeDataRibbon":
                raise SystemExit(f"{path}: emitter type {cls} is not supported")
            e["kind"] = "ribbon"
            e["ribbon"] = {k: tp.get(k) for k in ("MaxTrailCount", "MaxParticleInTrailCount", "TilingDistance", "RenderAxis", "bSpawnInitialParticle", "SheetsPerTrail")}
        for m in [ref(lod["SpawnModule"])] + [ref(m) for m in lod.get("Modules", [])]:
            cls, mp = self.module(p, m)
            if mp.get("bEnabled") is False:
                continue
            if cls == "Spawn":
                e["spawn"] = {"rate": d(mp, "Rate", 1), "scale": d(mp, "RateScale", 1), "process_rate": mp.get("bProcessSpawnRate", True),
                              "bursts": [[b["Count"], b.get("CountLow", -1), b.get("Time", 0.0)] for b in mp.get("BurstList", [])]}
            elif cls == "SpawnPerUnit":
                e["spawn_per_unit"] = {"unit": mp["UnitScalar"], "count": d(mp, "SpawnPerUnit", 1), "max_frame_distance": mp.get("MaxFrameDistance", 0.0),
                                       "movement_tolerance": mp.get("MovementTolerance", 0.1), "process_rate": mp.get("bProcessSpawnRate", True),
                                       "ignore_rate_when_moving": mp.get("bIgnoreSpawnRateWhenMoving", False)}
            elif cls in MODULES:
                mod = {"type": cls}
                for key, dim in MODULES[cls].items():
                    mod[key] = d(mp, key, dim)
                for f in FLAGS:
                    if f in mp:
                        mod[f] = mp[f]
                e["modules"].append(mod)
            elif cls in ("ParameterDynamic",):
                pass  # feeds material parameters; the runtime draws without the material graph
            else:
                raise SystemExit(f"{path}: module {cls} is not supported")
        return e

    # ------------------------------------------------------------------------------ materials

    def material(self, p: Package, path: str | None) -> str | None:
        """Blend mode and sampled textures of a particle material (exported as PNG). None for
        distortion materials."""
        if not path:
            return None
        if path in self.materials:
            return path if self.materials[path] else None
        overrides, mat, permutation = [], path, False
        while True:
            e = p.export_at(mat)
            mp = p.props_at(mat)
            if p.class_name(e) != "MaterialInstanceConstant":
                break
            overrides += [ref(t["ParameterValue"]) for t in mp.get("TextureParameterValues", []) if ref(t.get("ParameterValue"))]
            permutation |= bool(mp.get("bHasStaticPermutationResource"))
            mat = ref(mp["Parent"])
        if mp.get("bUsesDistortion") and not (permutation and self.distortion_switched_off(p, mat)):
            self.materials[path] = None
            return None
        textures = overrides or self.compiled_textures(p, mat)
        self.materials[path] = {
            "base": mat,
            "blend": mp.get("BlendMode", "BLEND_Opaque"),
            "two_sided": bool(mp.get("TwoSided", False)),
            "textures": [self.texture(p, t) for t in textures],
        }
        return path

    @staticmethod
    def distortion_switched_off(p: Package, path: str) -> bool:
        """A parent material whose distortion sits behind a static switch that is off by default:
        an instance with its own static permutation (and no override, which this reader cannot see)
        compiles without it."""
        e = p.export_at(path)
        switches = [p.properties(x) for x in p.exports if x.outer_index == e.index and p.class_name(x) == "MaterialExpressionStaticSwitchParameter"]
        dist = [s for s in switches if "Distortion" in str(s.get("ParameterName"))]
        return bool(dist) and not any(s.get("DefaultValue") for s in dist)

    @staticmethod
    def compiled_textures(p: Package, path: str) -> list[str]:
        """The textures a cooked material's compiled shader samples. After the tagged properties a
        cooked material keeps its FMaterialResource: ..., the material id (GUID), a flag, then the
        array of textures the uniform expressions reference."""
        e = p.export_at(path)
        d = p.data[e.offset : e.offset + e.size]
        none = p.names.index("None")
        end = max(i for i in range(len(d) - 8) if struct.unpack_from("<ii", d, i) == (none, 0)) + 8
        ints = struct.unpack_from(f"<{(len(d) - end) // 4}i", d, end)
        is_tex = lambda v: v != 0 and -len(p.imports) <= v <= len(p.exports) and "Texture" in (
            p.class_name(p.exports[v - 1]) if v > 0 else p.imports[-v - 1].class_name)
        for i in range(len(ints)):
            n = ints[i]
            if 0 < n < 16 and i + n < len(ints) and all(is_tex(v) for v in ints[i + 1 : i + 1 + n]):
                return [p.obj_path(v) for v in ints[i + 1 : i + 1 + n]]
        return []

    def texture(self, p: Package, path: str) -> str:
        name = path.split(".")[-1]
        dst = self.dir / f"{name}.png"
        if not dst.exists():
            package = next((q for q in PACKAGES if self.pkg(q).export_at(path)), None)
            if package is None:
                raise SystemExit(f"texture {path} not found in {PACKAGES}")
            groups = path.split(".")[1:-1]
            src = next((s for s in self.export(package, name, "Texture2D") if all(g in s.parts for g in groups)), None)
            if src is None:
                raise SystemExit(f"UModel did not export {path}")
            shutil.copy2(src, dst)
        return dst.name

    # ------------------------------------------------------------------------------ shakes

    def shakes(self, car_fx: dict, impacts: dict) -> dict:
        """Camera shakes (UE3 CameraShake oscillations, over the class defaults) and force feedback
        waveforms, keyed by the FX event or impact that plays them."""
        out = {}
        pk = self.pkg(CAR_FX[0])
        for att in car_fx.get("Attachments", []):
            comp = ref(att.get("Component"))
            if comp and pk.class_name(pk.export_at(comp)) == "ShakeComponent_X":
                cp = self.props(CAR_FX[0], comp)
                for ev in att.get("AttachAny", []):
                    out[ref(ev).split(".")[-1]] = {"shake": self.shake(ref(cp.get("ShakeParams"))), "rumble": self.rumble(ref(cp.get("ForceFeedbackWaveform")))}
        out["WheelImpact"] = {"shake": self.shake(ref(car_fx.get("WheelImpactShake"))), "rumble": self.rumble(ref(car_fx.get("WheelImpactForceFeedback"))),
                              "scale_curve": curve(car_fx.get("ShakeScaleCurve")), "min_momentum": car_fx.get("MinImpactMomentum", 0.0)}
        out["BodyImpact"] = {"shake": self.shake(ref(impacts.get("ImpactCameraShake"))), "rumble": self.rumble(ref(impacts.get("ImpactForceFeedback"))),
                             "scale_curve": curve(impacts.get("ShakeScaleCurve")), "min_momentum": impacts.get("MinImpactMomentum", 0.0)}
        bp = self.pkg(BOOST_BASE_FX[0])
        for att in self.props(*BOOST_BASE_FX).get("Attachments", []):
            comp = ref(att.get("Component"))
            if comp and bp.class_name(bp.export_at(comp)) == "ShakeComponent_X":
                cp = self.props(BOOST_BASE_FX[0], comp)
                for ev in att.get("AttachAny", []):
                    out[f"Boost{ref(ev).split('.')[-1]}"] = {"shake": self.shake(ref(cp.get("ShakeParams"))), "rumble": self.rumble(ref(cp.get("ForceFeedbackWaveform")))}
        return out

    def shake(self, path: str | None) -> dict | None:
        if not path:
            return None
        sp = {**self.props("Engine", "Default__CameraShake"), **self.props(self.find(path), path)}
        osc = lambda o: {k: {"amplitude": v.get("Amplitude", 0.0), "frequency": v.get("Frequency", 0.0), "random_offset": v.get("InitialOffset", "EOO_OffsetRandom") == "EOO_OffsetRandom"}
                         for k, v in (o or {}).items() if isinstance(v, dict) and k != "__struct__"}
        return {"source": path, "duration": sp.get("OscillationDuration", 0.0), "blend_in": sp.get("OscillationBlendInTime", 0.0), "blend_out": sp.get("OscillationBlendOutTime", 0.0),
                "rot": osc(sp.get("RotOscillation")), "loc": osc(sp.get("LocOscillation")), "fov": osc({"FOV": sp["FOVOscillation"]}) if isinstance(sp.get("FOVOscillation"), dict) else {}}

    def rumble(self, path: str | None) -> dict | None:
        if not path:
            return None
        wp = self.props(self.find(path), path)
        return {"source": path, "looping": bool(wp.get("bIsLooping", False)),
                "samples": [{"left": s.get("LeftAmplitude", 0) / 100.0, "right": s.get("RightAmplitude", 0) / 100.0, "left_fn": s.get("LeftFunction", "WF_Constant"),
                             "right_fn": s.get("RightFunction", "WF_Constant"), "duration": s.get("Duration", 0.0)} for s in wp.get("Samples", [])]}


def curve(c) -> list[list[float]] | None:
    return [[pt["InVal"], pt["OutVal"]] for pt in c.get("Points", [])] if isinstance(c, dict) else None
