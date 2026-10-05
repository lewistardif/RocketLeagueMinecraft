"""Cascade (UE3 particle system) data from decrypted packages: modules over their class defaults,
and cooked distributions as lookup tables, shared by `boost.py` and `fx.py`."""

from __future__ import annotations

from pathlib import Path

from ue3 import Package


def ref(v) -> str | None:
    return v.get("__ref__") if isinstance(v, dict) else None


def vec(v, default=(0.0, 0.0, 0.0)) -> list[float]:
    return list(v["v"]) if isinstance(v, dict) and "v" in v else list(default)


def instance_params(params) -> dict:
    """Particle instance parameters (`ParticleSysParam`) -> {"min", "max"} vectors."""
    out = {}
    for ip in params or []:
        kind = ip.get("ParamType", "PSPT_None")
        hi, lo = vec(ip.get("Vector")), vec(ip.get("Vector_Low"))
        if kind == "PSPT_Scalar":
            out[ip["Name"]] = {"min": [ip.get("Scalar", 0.0)], "max": [ip.get("Scalar", 0.0)]}
        elif kind == "PSPT_ScalarRand":
            out[ip["Name"]] = {"min": [ip.get("Scalar_Low", 0.0)], "max": [ip.get("Scalar", 0.0)]}
        elif kind == "PSPT_Vector":
            out[ip["Name"]] = {"min": hi, "max": hi}
        elif kind == "PSPT_VectorRand":
            out[ip["Name"]] = {"min": lo, "max": hi}
    return out


class Cascade:
    def __init__(self, pkgs: Path):
        self.pkgs = pkgs
        self.cache: dict[str, Package] = {}

    def pkg(self, name: str) -> Package:
        if name not in self.cache:
            self.cache[name] = Package.open(self.pkgs / f"{name}.upk")
        return self.cache[name]

    def module(self, p: Package, path: str) -> tuple[str, dict]:
        """Module class and properties, over the class defaults (cooking drops unchanged values)."""
        cls = p.class_name(p.export_at(path))
        props = dict(self.pkg("Engine").props_at(f"Default__{cls}"))
        for k, v in p.props_at(path).items():
            # Struct members left at their default are not saved either (e.g. a lookup table).
            props[k] = {**props[k], **v} if isinstance(v, dict) and isinstance(props.get(k), dict) else v
        return cls.removeprefix("ParticleModule"), props

    def dist(self, p: Package, raw, dim: int, params: dict) -> dict | None:
        """A cooked RawDistribution: its lookup table, or a particle parameter resolved to the value
        the effect sets (`{"min", "max"}`: uniform random per particle and component)."""
        if not isinstance(raw, dict):
            return None
        dist_path = ref(raw.get("Distribution"))
        if dist_path:
            # UDistribution{Float,Vector}ParticleParameter: the instance parameter (or `Constant` when
            # it is not set), per component either used directly or remapped (DPM_Normal) from
            # [MinInput, MaxInput] to [MinOutput, MaxOutput], clamped.
            dp = p.props_at(dist_path)
            name = dp.get("ParameterName")
            per = lambda key, default: (vec(dp[key]) if isinstance(dp.get(key), dict) else [dp.get(key, default)] * 3)[:dim]
            modes = dp.get("ParamModes") or {}
            mode = [dp.get("ParamMode") or modes.get(i, "DPM_Normal") for i in range(dim)]
            if any(m not in ("DPM_Direct", "DPM_Normal") for m in mode):
                raise SystemExit(f"particle parameter {name}: mode {mode} is not supported")
            in0, in1, out0, out1 = per("MinInput", 0.0), per("MaxInput", 1.0), per("MinOutput", 0.0), per("MaxOutput", 1.0)

            def remap(v: list[float]) -> list[float]:
                return [x if m == "DPM_Direct" else o0 + (o1 - o0) * min(max((x - i0) / (i1 - i0), 0.0), 1.0)
                        for x, m, i0, i1, o0, o1 in zip(v, mode, in0, in1, out0, out1)]

            if name in params:
                lo, hi = params[name]["min"][:dim], params[name]["max"][:dim]
            else:
                c = dp.get("Constant", 0.0)
                lo = hi = (vec(c) if isinstance(c, dict) else [c] * 3)[:dim]
            return {"min": remap(lo), "max": remap(hi)}
        if not raw.get("LookupTable"):
            return None  # never set: the module's default (zero) distribution
        return {
            "table": raw["LookupTable"][2:],
            "random": raw.get("Op", 1) == 2,
            "chunk": raw.get("LookupTableChunkSize", dim),
            "time_scale": raw.get("LookupTableTimeScale", 0.0),
            "start_time": raw.get("LookupTableStartTime", 0.0),
            "dim": dim,
        }
