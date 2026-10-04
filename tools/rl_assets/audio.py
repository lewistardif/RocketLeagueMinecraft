"""Rocket League's car sounds from the decrypted game packages and the game's Wwise sound banks.

Which sounds a car plays, and on what, comes from the cooked game objects:

* `FXActors.Car.Car_FXActor` (TAGame): the jump, double jump and dodge sounds, the in-air whoosh
  loop, the wheel landing impact, the tyre rolling loop, and the supersonic sounds (enter + loop);
* `Archetypes.Car.Car_Default` (GameInfo_Soccar_SF): the engine and exhaust loops with the engine
  audio profile, the body impact and body slide sounds (`ImpactEffectsComponent`);
* `Boost_Standard.FX.FXActor` and its parent `BoostBase.FXActor`: the boost loop and the "dry fire"
  sound of boosting with an empty tank.

Each of these is an `AkSoundCue` naming a Wwise event and its sound bank. The banks (`*.bnk` next to
the packages, not encrypted) are parsed with wwiser (https://github.com/bnnm/wwiser) and the part of
the Wwise object graph the events reach is written out in a compact form (events and actions,
sounds, random/sequence, switch and blend/layer containers, actor-mixers, buses, their volume and
pitch properties, random ranges, RTPC curves and state offsets, the game parameters' defaults and the
RTPC-driven switch groups of `Init.bnk`). The embedded media (Wwise Vorbis) is decoded to WAV with
vgmstream (https://github.com/vgmstream/vgmstream).

Writes into `<out>/audio/`:
  audio.json   cues (what the game plays, on which event, with its game-side parameters), the
               Wwise graph, game parameters
  *.wav        the decoded media, named by Wwise source id

Wwise ids are FNV-1 hashes of lower-case names. Names come from the packages' name tables (they
hold every event/RTPC/switch name the game's script uses) and a few extra candidates; a name is only
used where its hash matches, so nothing is guessed.
"""

from __future__ import annotations

import json
import re
import shutil
import struct
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

from ue3 import Package

PACKAGES = ["TAGame", "Startup", "GameInfo_Soccar_SF", "Boost_Standard_SF", "EngineAudio_Car01_OE_SF", "Engine", "ProjectX"]
CAR_FX = ("TAGame", "FXActors.Car.Car_FXActor")
CAR_ARCHETYPE = ("GameInfo_Soccar_SF", "Archetypes.Car.Car_Default")
BOOST_FX = ("Boost_Standard_SF", "Boost_Standard.FX.FXActor")
BOOST_BASE_FX = ("Boost_Standard_SF", "BoostBase.FXActor")

# Names whose hashes appear in the banks but that are neither in the packages' name tables nor in the
# executable's strings (each is only applied where its FNV hash matches an id).
EXTRA_NAMES = ["DopplerAngle", "TRUE", "FALSE"]


def ref(v) -> str | None:
    return v.get("__ref__") if isinstance(v, dict) else None


def fnv(name: str) -> int:
    h = 2166136261
    for c in name.lower().encode():
        h = (h * 16777619) & 0xFFFFFFFF
        h ^= c
    return h


def curve(c) -> list[list[float]] | None:
    """UE3 InterpCurveFloat -> [[in, out], ...] (all of the game's here are linear or a step)."""
    if not isinstance(c, dict):
        return None
    return [[p["InVal"], p["OutVal"]] for p in c.get("Points", [])]


class Audio:
    def __init__(self, pkgs: Path, cooked: Path, out: Path, work: Path, wwiser: Path, vgmstream: Path):
        self.pkgs, self.cooked, self.work = pkgs, cooked, work
        self.wwiser, self.vgmstream = wwiser, vgmstream
        self.dir = out / "audio"
        self.dir.mkdir(parents=True, exist_ok=True)
        self.cache: dict[str, Package] = {}

    def pkg(self, name: str) -> Package:
        if name not in self.cache:
            self.cache[name] = Package.open(self.pkgs / f"{name}.upk")
        return self.cache[name]

    def props(self, package: str, path: str | None) -> dict:
        return self.pkg(package).props_at(path) if path else {}

    def run(self) -> None:
        cues = self.cues()
        banks = sorted({c["bank"] for c in cues.values()})
        wanted = sorted({e for c in cues.values() for e in (c["play"], c.get("stop")) if e})
        graph = Wwise(self.work / "wwise", self.cooked, self.wwiser, self.names())
        data = graph.build(banks, wanted)
        self.decode_media(graph, data)
        data["cues"] = cues
        data["engine"] = self.engine_profile()
        (self.dir / "audio.json").write_text(json.dumps(data, indent=1))

    # ------------------------------------------------------------------------------ game side

    def cue(self, package: str, path: str | None) -> dict | None:
        """AkSoundCue -> its bank and start/stop events (cooked into whichever package uses it)."""
        if not path:
            return None
        for p in [package] + PACKAGES:
            cp = self.props(p, path)
            if cp.get("StartEvent"):
                bank = ref(cp.get("RequiredBank")).split(".")[-1]
                return {"source": path, "bank": bank, "play": cp["StartEvent"], "stop": cp.get("StopEvent")}
        raise SystemExit(f"sound cue {path} not found in {PACKAGES}")

    def fx_sounds(self, package: str, fx_path: str) -> dict:
        """An FX actor's sound attachments: name -> cue + the FX events that start / stop it."""
        out = {}
        for att in self.props(package, fx_path).get("Attachments", []):
            comp = ref(att.get("Component"))
            if not comp or self.pkg(package).class_name(self.pkg(package).export_at(comp)) != "AkPlaySoundComponent":
                continue
            cp = self.props(package, comp)
            c = self.cue(package, ref(cp.get("SoundCue")))
            ev = lambda key: [ref(e).split(".")[-1] for e in att.get(key, [])]
            c.update({"attach_any": ev("AttachAny"), "attach_all": ev("AttachAll"), "detach_any": ev("DetachAny"),
                      "local_only": cp.get("Receiver") == "PlaySoundReceiver_Local"})
            out[att["Name"]] = c
        return out

    def cues(self) -> dict:
        out = {}
        package, path = CAR_FX
        fx = self.props(package, path)
        for name, c in self.fx_sounds(package, path).items():
            out[f"car_fx.{name}"] = c
        for key in ("AkWheelImpactSound", "AkWheelDriveSound", "AkEnterSupersonicSound", "AkLoopSupersonicSound"):
            out[f"car_fx.{key}"] = self.cue(package, ref(fx.get(key)))
        out["car_fx.AkWheelImpactSound"]["params"] = {
            "MinImpactMomentum": fx["MinImpactMomentum"],
            "ShakeScaleCurve": curve(fx.get("ShakeScaleCurve")),
            **{k: self.props(package, "Default__FXActor_Car_TA")[k] for k in ("AkImpactTypeKey", "AkImpactIntensityKey", "DefaultPhysMatName")},
        }

        package, path = CAR_ARCHETYPE
        car = self.props(package, path)
        engine = self.props(package, ref(car["EngineAudio"]))
        for key in ("EngineAudio", "ExhaustAudio"):
            out[f"engine.{key}"] = self.cue(package, ref(self.props(package, ref(engine[key])).get("SoundCue")))
        impacts = self.props(package, ref(car["ImpactEffectsComponent"]))
        defaults = self.props("TAGame", "Default__ImpactEffectsComponent_TA")
        params = {**defaults, **{k: v for k, v in impacts.items() if not isinstance(v, dict) or "Points" in v}}
        params = {k: curve(v) if isinstance(v, dict) else v for k, v in params.items()}
        out["impacts.AkImpactSound"] = {**self.cue(package, ref(impacts["AkImpactSound"])), "params": params}
        out["impacts.AkSlideSound"] = {**self.cue(package, ref(impacts["AkSlideSound"])), "params": params}

        package, path = BOOST_FX
        for name, c in self.fx_sounds(package, path).items():
            out[f"boost.{name}"] = c
        base = self.props(*BOOST_BASE_FX)
        out["boost.DryFireSound"] = self.cue(package, ref(base.get("DryFireSound")))
        return out

    def engine_profile(self) -> dict:
        """The default car's engine audio profile (`EngineAudioProfile_TA`, cooked values over the
        class defaults). The game turns it into the RPM and Throttle_Input game parameters in
        native code."""
        package, path = CAR_ARCHETYPE
        engine = self.props(package, ref(self.props(package, path)["EngineAudio"]))
        p = {**self.props("TAGame", "Default__EngineAudioProfileBase_TA"), **self.props("TAGame", "Default__EngineAudioProfile_TA"),
             **self.props(package, ref(engine.get("Profile")))}
        p["Gears"] = [{k: {"min": g[k]["Min"], "rand": g[k]["RandRange"]} for k in ("RPMShiftDownRange", "RPMShiftUpRange")} for g in p["Gears"]]
        return p

    def names(self) -> list[str]:
        """Candidate names, lowest priority first (a later spelling of the same hash wins): the
        strings in the game executable (the native code sets RTPCs such as Car_SlideAngle or
        Throttle_Input by name), the packages' name tables, then EXTRA_NAMES."""
        names: list[str] = []
        exe = self.cooked.parents[1] / "Binaries" / "Win64" / "RocketLeague.exe"
        if exe.exists():
            d = exe.read_bytes()
            names += sorted({m.decode() for m in re.findall(rb"[A-Za-z][A-Za-z0-9_]{2,63}", d)})
            names += sorted({m.decode("utf-16-le") for m in re.findall(rb"(?:[A-Za-z0-9_]\x00){3,64}", d)})
        for p in PACKAGES:
            names += self.pkg(p).names
        return names + EXTRA_NAMES

    # ------------------------------------------------------------------------------ media

    def decode_media(self, graph: "Wwise", data: dict) -> None:
        tmp = self.work / "wwise" / "media"
        tmp.mkdir(parents=True, exist_ok=True)
        for node in data["nodes"].values():
            if node["kind"] != "sound":
                continue
            src = node.pop("source")
            wav = self.dir / f"{src}.wav"
            node["media"] = wav.name
            if wav.exists():
                continue
            wem = tmp / f"{src}.wem"
            wem.write_bytes(graph.media[src])
            r = subprocess.run([str(self.vgmstream), "-o", str(wav), str(wem)], capture_output=True, text=True)
            if r.returncode != 0 or not wav.exists():
                raise SystemExit(f"vgmstream failed on {wem}:\n{r.stdout}\n{r.stderr}")


# ---------------------------------------------------------------------------------- Wwise banks


def xml_value(el):
    v = el.get("value")
    for conv in (int, float):
        try:
            return conv(v)
        except (TypeError, ValueError):
            pass
    return v


def xml_to_py(el):
    """wwiser's XML dump -> nested dicts/lists. Enum-like fields keep their name (`valuefmt` "0x01
    [Pitch]" -> "Pitch"), hashed ids stay numbers."""
    if el.tag == "list":
        return [xml_to_py(c) for c in el]
    if el.tag == "field" and len(el) == 0:
        fmt = el.get("valuefmt") or ""
        if el.get("name") in ENUM_FIELDS and "[" in fmt:
            return fmt[fmt.index("[") + 1 : fmt.rindex("]")]
        return xml_value(el)
    d = {"_v": xml_value(el)} if el.tag == "field" else {}
    for c in el:
        k, v = c.get("name"), xml_to_py(c)
        if k in d:
            d[k] = d[k] if isinstance(d[k], list) and c.tag == "object" else [d[k]] if not isinstance(d[k], list) else d[k]
            d[k].append(v)
        else:
            d[k] = v
    return d


ENUM_FIELDS = {"pID", "Interp", "eScaling", "ulActionType", "eGroupType", "eMode", "eRandomMode", "rtpcType", "eBindToBuiltInParam", "rampType"}


def as_list(v) -> list:
    return v if isinstance(v, list) else [] if v is None else [v]


def points(lst) -> list:
    return [[p["From"], p["To"], p["Interp"]] for p in as_list(lst)]


def f32_as_u32(x: float) -> int:
    """RTPC -> switch curves store switch ids in their float 'To' values."""
    return struct.unpack("<I", struct.pack("<f", x))[0]


class Wwise:
    def __init__(self, work: Path, cooked: Path, wwiser: Path, names: list[str]):
        self.work, self.cooked, self.wwiser = work, cooked, wwiser
        self.work.mkdir(parents=True, exist_ok=True)
        (self.work / "wwnames.txt").write_text("\n".join(names))
        self.hash_names = {fnv(n): n for n in names}
        self.objects: dict[int, tuple[str, dict]] = {}
        self.media: dict[int, bytes] = {}
        self.init: dict = {}
        # Property id -> name. Since bank version ~145 an RTPC's ParamID is a property id too.
        self.prop_names: dict[int, str] = {}

    def name(self, v) -> str | int:
        return self.hash_names.get(v, v) if isinstance(v, int) else v

    def load(self, bank: str) -> list:
        path = self.cooked / f"{bank}.bnk"
        if not path.exists():
            raise SystemExit(f"no sound bank {path}")
        dst = self.work / path.name
        shutil.copy2(path, dst)
        xml = self.work / f"{bank}.xml"
        if not xml.exists():
            r = subprocess.run([sys.executable, str(self.wwiser), "-d", "xml", "-dn", xml.stem, dst.name], cwd=self.work, capture_output=True, text=True)
            if not xml.exists():
                raise SystemExit(f"wwiser failed on {bank}:\n{r.stdout}\n{r.stderr}")
        self.read_media(dst.read_bytes())
        root = ET.fromstring(xml.read_text(encoding="utf-8"))
        for f in root.iter("field"):
            fmt = f.get("valuefmt") or ""
            if f.get("name") == "pID" and "[" in fmt:
                self.prop_names[int(f.get("value"))] = fmt[fmt.index("[") + 1 : fmt.rindex("]")]
        items = []
        for o in root.iter("object"):
            name = o.get("name", "")
            if name.startswith("CAk") and o.get("index") is not None and o.find("field[@name='ulID']") is not None:
                items.append((name, xml_to_py(o)))
            elif name == "GlobalSettingsChunk":
                self.init = xml_to_py(o)
        for kind, d in items:
            self.objects[d["ulID"]] = (kind, d)
        return items

    def read_media(self, d: bytes) -> None:
        pos, index, data_at = 0, [], None
        while pos < len(d):
            tag, n = d[pos : pos + 4], struct.unpack_from("<I", d, pos + 4)[0]
            if tag == b"DIDX":
                index = [struct.unpack_from("<III", d, pos + 8 + 12 * i) for i in range(n // 12)]
            elif tag == b"DATA":
                data_at = pos + 8
            pos += 8 + n
        for sid, off, size in index:
            self.media[sid] = d[data_at + off : data_at + off + size]

    # ------------------------------------------------------------------------------ graph

    def build(self, banks: list[str], events: list[str]) -> dict:
        self.load("Init")
        for b in banks:
            self.load(b)
        out_events, todo = {}, []
        for ev in events:
            kind, e = self.objects.get(fnv(ev), (None, None))
            if kind != "CAkEvent":
                raise SystemExit(f"event {ev} not in banks {banks}")
            actions = []
            for a in as_list(e["EventInitialValues"]["actions"]):
                akind, ad = self.objects[a["ulActionID"]]
                act = self.action(akind, ad)
                actions.append(act)
                if act.get("target"):
                    todo.append(act["target"])
            out_events[ev] = actions
        nodes: dict[str, dict] = {}
        while todo:
            nid = todo.pop()
            if str(nid) in nodes or nid not in self.objects:
                continue
            node = self.node(nid)
            nodes[str(nid)] = node
            todo += node.get("children", []) + [x for x in (node.get("parent"), node.get("bus")) if x]
        return {"events": out_events, "nodes": nodes, "game": self.game_params()}

    def action(self, kind: str, d: dict) -> dict:
        iv = d["ActionInitialValues"]
        props = {p["pID"]: p["pValue"] for p in as_list(iv.get("AkPropBundle<AkPropValue,unsigned char>", {}).get("pProps"))}
        out = {"type": kind.removeprefix("CAkAction").lower(), "target": iv.get("idExt") or None,
               "delay_ms": props.get("DelayTime", 0), "fade_ms": props.get("TransitionTime", 0)}
        if kind not in ("CAkActionPlay", "CAkActionStop"):
            out["raw"] = iv
        return out

    def node(self, nid: int) -> dict:
        kind, d = self.objects[nid]
        body = next(v for k, v in d.items() if k.endswith("InitialValues"))
        base = body.get("NodeBaseParams") or body.get("BusInitialParams") or {}
        bundle = base.get("NodeInitialParams", base)
        props = {p["pID"]: p["pValue"] for p in as_list(bundle.get("AkPropBundle<AkPropValue,unsigned char>", {}).get("pProps"))}
        if kind in ("CAkBus", "CAkAuxBus"):
            props = {p["pID"]: p["pValue"] for p in as_list(body.get("BusInitialParams", {}).get("AkPropBundle<AkPropValue,unsigned char>", {}).get("pProps"))}
        ranges = {p["pID"]: [p["min"], p["max"]] for p in as_list(bundle.get("AkPropBundle<RANGED_MODIFIERS<AkPropValue>>", {}).get("pProps"))}
        rtpc_holder = base if "InitialRTPC" in base else body
        node = {
            "kind": KINDS[kind],
            "parent": base.get("DirectParentID") or None,
            "bus": base.get("OverrideBusId") or None,
            "props": {k: v for k, v in props.items() if isinstance(k, str)},
            "ranges": {k: v for k, v in ranges.items() if isinstance(k, str)},
            "rtpcs": self.rtpcs(rtpc_holder.get("InitialRTPC")),
            "states": self.states(rtpc_holder.get("StateChunk") or base.get("StateChunk")),
        }
        adv = base.get("AdvSettingsParams") or {}
        if adv.get("u16MaxNumInstance"):
            bits = as_list(adv.get("byBitVector"))[0]
            node["max_instances"] = {"count": adv["u16MaxNumInstance"], "kill_newest": bool(bits.get("bKillNewest"))}
        if kind in ("CAkBus", "CAkAuxBus"):
            node["parent"] = body.get("OverrideBusId") or None
            node["bus"] = None
        children = as_list(d.get("Children", body.get("Children", {})).get("ulChildID")) if isinstance(d.get("Children", body.get("Children")), dict) else []
        node["children"] = children
        if kind == "CAkSound":
            src = body["AkBankSourceData"]
            if src["StreamType"] != 0:
                raise SystemExit(f"sound {nid}: streamed media is not supported")
            node["source"] = src["AkMediaInformation"]["sourceID"]
            if node["source"] not in self.media:
                raise SystemExit(f"sound {nid}: media {node['source']} not in its bank")
            node["loop"] = node["props"].pop("Loop", None)  # absent: play once; 0: forever; n: n times
        elif kind == "CAkRanSeqCntr":
            pl = as_list(d.get("CAkPlayList", body.get("CAkPlayList", {})).get("pItems"))
            src = d if "eMode" in d else body
            bits = src["byBitVector"]
            node.update({
                "mode": src["eMode"], "random_mode": src["eRandomMode"], "avoid_repeat": src["wAvoidRepeatCount"],
                "loop": src["sLoopCount"], "continuous": bool(bits.get("bIsContinuous")),
                "transition_mode": src["eTransitionMode"], "transition_ms": src["fTransitionTime"],
                "playlist": [[i["ulPlayID"], i["weight"] / 1000.0] for i in pl],
            })
        elif kind == "CAkSwitchCntr":
            src = d if "ulGroupID" in d else body
            group = src["ulGroupID"]
            node.update({
                "group": self.name(group), "group_type": src["eGroupType"], "default": self.name(src["ulDefaultSwitch"]),
                "switches": {str(self.name(p["ulSwitchID"])): as_list((p.get("NodeList") or {}).get("NodeID")) for p in as_list(src.get("SwitchList"))},
            })
        elif kind == "CAkLayerCntr":
            src = d if "pLayers" in d else body
            node["layers"] = [{
                "rtpcs": self.rtpcs(l["LayerInitialValues"]["InitialRTPC"]),
                "crossfade": self.name(l["LayerInitialValues"].get("rtpcID")) or None,
                "assoc": {str(a["ulAssociatedChildID"]): points(a.get("pRTPCMgr")) for a in as_list(l["LayerInitialValues"].get("assocs"))},
            } for l in as_list(src.get("pLayers"))]
        return node

    def rtpcs(self, holder) -> list:
        out = []
        for r in as_list((holder or {}).get("pRTPCMgr")):
            out.append({"rtpc": self.name(r["RTPCID"]), "rtpc_type": r.get("rtpcType"), "param": self.prop_names.get(r["ParamID"], r["ParamID"]), "accum": r.get("rtpcAccum"),
                        "scaling": r["eScaling"], "points": points(r.get("pRTPCMgr"))})
        return out

    def states(self, chunk) -> list:
        out = []
        for g in as_list((chunk or {}).get("pStateChunks")):
            vals = {}
            for s in as_list(g.get("pStates")):
                props = {p["pID"]: p["pValue"] for p in as_list(s.get("AkPropBundle<float,unsigned short>", {}).get("pProps"))}
                vals[str(self.name(s.get("ulStateID")))] = props
            out.append({"group": self.name(g["ulStateGroupID"]), "values": vals})
        return out

    def game_params(self) -> dict:
        gs = self.init
        params = {}
        for r in as_list(gs.get("pRTPCMgr")):
            params[str(self.name(r["RTPC_ID"]))] = {"default": r["fValue"], "ramp": r.get("rampType"), "up": r.get("fRampUp"), "down": r.get("fRampDown"),
                                                     "builtin": r.get("eBindToBuiltInParam")}
        switches = {}
        for s in as_list(gs.get("pItems")):
            if "SwitchGroupID" in s:
                switches[str(self.name(s["SwitchGroupID"]))] = {"rtpc": self.name(s["rtpcID"]),
                                                               "points": [[x, str(self.name(f32_as_u32(y))), i] for x, y, i in points(s.get("pSwitchMgr"))]}
        states = {str(self.name(g["ulStateGroupID"])): g["DefaultTransitionTime"] for g in as_list(gs.get("StateGroups"))}
        return {"params": params, "rtpc_switches": switches, "state_transition_ms": states}


KINDS = {
    "CAkSound": "sound", "CAkRanSeqCntr": "random", "CAkSwitchCntr": "switch", "CAkLayerCntr": "layer",
    "CAkActorMixer": "actor", "CAkBus": "bus", "CAkAuxBus": "bus",
}


if __name__ == "__main__":
    # Standalone re-run over already decrypted packages (extract.py does this as one of its steps).
    import argparse

    repo = Path(__file__).resolve().parents[2]
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", type=Path, default=Path(r"C:\Program Files\Epic Games\rocketleague"))
    ap.add_argument("--wwiser", type=Path, required=True, help="wwiser.pyz")
    ap.add_argument("--vgmstream", type=Path, required=True, help="vgmstream-cli executable")
    ap.add_argument("--out", type=Path, default=repo / "assets" / "rl")
    ap.add_argument("--work", type=Path, default=repo / "target" / "rl_assets_work")
    a = ap.parse_args()
    Audio(a.work / "packages", a.game / "TAGame" / "CookedPCConsole", a.out, a.work, a.wwiser.resolve(), a.vgmstream.resolve()).run()
