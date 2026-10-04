//! Rocket League's car sounds.
//!
//! With the extracted sounds (`assets/rl/audio/`, written by `tools/rl_assets/extract.py` from the
//! game's Wwise sound banks), the car plays what the game's car plays, on the same occasions:
//! - `Car_FXActor`: jump, double jump and dodge, the in-air whoosh loop, wheel landings, the tyre
//!   rolling loop, entering supersonic and the supersonic loop;
//! - the soccar car archetype: the engine and exhaust loops, body impacts and the body slide loop;
//! - the default boost: the boost loop (its stop event plays the boost tail) and the empty-tank
//!   "dry fire".
//!
//! The sounds are driven by a small player for the part of Wwise those events use ([`Wwise`]):
//! events and their play/stop actions with fades, sounds, random/sequence containers (weights,
//! avoid-repeat, continuous playlists), switch containers (game switches and RTPC-driven switch
//! groups), blend/layer containers (crossfades), actor-mixers and buses, with volume, make-up gain,
//! bus volume and pitch from properties, random ranges and RTPC curves, initial delays and
//! instance limits. The game parameters (RTPCs) are set from the car's state under the names the
//! game's native code uses (`Speed`, `RPM`, `Throttle_Input`, `WheelForwardSpeed`, ...).
//!
//! Not reproduced: filters (LPF/HPF), 3D positioning and attenuation (the car is always close to
//! the camera), effects, states, modulators (LFOs/envelopes), and parameters the demo has no source
//! for (split screen, replays, teams...), which keep the bank's default value. The engine RPM is
//! computed by native game code that is not in the packages: [`Engine`] drives it from the engine
//! audio profile's own values, and is the one place where the behaviour is reconstructed.

use crate::visuals::asset_root;
use bevy::audio::{AudioSink, AudioSinkPlayback, PlaybackMode, Volume};
use bevy::prelude::*;
use rl_car_core::CarState;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

const DATA_PATH: &str = "rl/audio/audio.json";
/// Wwise treats anything this quiet as silent.
const SILENT_DB: f32 = -96.3;

pub struct CarAudioPlugin;

impl Plugin for CarAudioPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Wwise::load()).init_resource::<CarSounds>().add_systems(Startup, preload);
    }
}

// ------------------------------------------------------------------------------------ data

#[derive(Deserialize)]
struct AudioFile {
    cues: HashMap<String, CueFile>,
    events: HashMap<String, Vec<ActionFile>>,
    nodes: HashMap<String, NodeFile>,
    game: GameFile,
    engine: EngineProfile,
}

#[derive(Deserialize, Clone)]
struct CueFile {
    play: String,
    stop: Option<String>,
    #[serde(default)]
    params: HashMap<String, Value>,
}

#[derive(Deserialize)]
struct ActionFile {
    #[serde(rename = "type")]
    kind: String,
    target: Option<u32>,
    fade_ms: f32,
}

#[derive(Deserialize)]
struct NodeFile {
    kind: String,
    parent: Option<u32>,
    bus: Option<u32>,
    #[serde(default)]
    props: HashMap<String, Value>,
    #[serde(default)]
    ranges: HashMap<String, [f32; 2]>,
    #[serde(default)]
    rtpcs: Vec<RtpcFile>,
    max_instances: Option<MaxInstances>,
    #[serde(default)]
    children: Vec<u32>,
    media: Option<String>,
    #[serde(rename = "loop")]
    loop_count: Option<i64>,
    // random/sequence
    mode: Option<String>,
    avoid_repeat: Option<usize>,
    #[serde(default)]
    continuous: bool,
    #[serde(default)]
    playlist: Vec<(u32, f32)>,
    // switch
    group: Option<Value>,
    default: Option<Value>,
    #[serde(default)]
    switches: HashMap<String, Vec<u32>>,
    // layer
    #[serde(default)]
    layers: Vec<LayerFile>,
}

#[derive(Deserialize, Clone, Copy)]
struct MaxInstances {
    count: usize,
    kill_newest: bool,
}

#[derive(Deserialize)]
struct RtpcFile {
    rtpc: Value,
    rtpc_type: Option<String>,
    param: Value,
    scaling: Option<String>,
    points: Vec<(f32, f32, String)>,
}

#[derive(Deserialize)]
struct LayerFile {
    rtpcs: Vec<RtpcFile>,
    crossfade: Option<Value>,
    assoc: HashMap<String, Vec<(f32, f32, String)>>,
}

#[derive(Deserialize)]
struct GameFile {
    params: HashMap<String, GameParam>,
    rtpc_switches: HashMap<String, RtpcSwitch>,
}

#[derive(Deserialize)]
struct GameParam {
    default: f32,
}

#[derive(Deserialize)]
struct RtpcSwitch {
    rtpc: Value,
    points: Vec<(f32, String, String)>,
}

/// `EngineAudioProfile_TA` of the default car (cooked values over the class defaults).
#[derive(Deserialize, Clone, Default)]
#[allow(non_snake_case)]
struct EngineProfile {
    Gears: Vec<Gear>,
    GearSwitchTime: f32,
    RPMAccelClutched: f32,
    RPMDecelClutched: f32,
    RPMMaxClutched: f32,
    RPMAccelFactor: f32,
    RPMDecelFactor: f32,
    RPMShiftUpBoost: f32,
    AirMaxThrottleTime: f32,
    RevLimitRPM: f32,
    RevLimitRPMDecel: f32,
    WheelForwardSpeedInterpRate: f32,
    WheelSideSpeedInterpRate: f32,
}

#[derive(Deserialize, Clone, Default)]
#[allow(non_snake_case)]
struct Gear {
    RPMShiftDownRange: GearRange,
    RPMShiftUpRange: GearRange,
}

#[derive(Deserialize, Clone, Default)]
struct GearRange {
    min: f32,
    rand: f32,
}

/// A JSON id or name as the string key used everywhere (names when the extractor resolved them,
/// decimal ids otherwise).
fn key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ------------------------------------------------------------------------------------ graph

#[derive(Clone, Copy, PartialEq)]
enum Interp {
    Log3,
    Sine,
    Log1,
    InvSCurve,
    Linear,
    SCurve,
    Exp1,
    SineRecip,
    Exp3,
    Constant,
}

impl Interp {
    fn parse(s: &str) -> Interp {
        match s {
            "Log3" => Interp::Log3,
            "Sine" => Interp::Sine,
            "Log1" => Interp::Log1,
            "InvSCurve" => Interp::InvSCurve,
            "SCurve" => Interp::SCurve,
            "Exp1" => Interp::Exp1,
            "SineRecip" => Interp::SineRecip,
            "Exp3" => Interp::Exp3,
            "Constant" => Interp::Constant,
            _ => Interp::Linear,
        }
    }

    /// Shape of a segment, 0..1 -> 0..1 (Wwise's curve shapes: logarithmic/exponential with base
    /// 1.41 or 3, sine, reciprocal sine, S and inverted S).
    fn shape(self, t: f32) -> f32 {
        use std::f32::consts::{FRAC_PI_2, PI};
        match self {
            Interp::Linear => t,
            Interp::Constant => 0.0,
            Interp::Log1 => 1.0 - (1.0 - t).powf(1.41),
            Interp::Log3 => 1.0 - (1.0 - t).powi(3),
            Interp::Exp1 => t.powf(1.41),
            Interp::Exp3 => t.powi(3),
            Interp::Sine => (t * FRAC_PI_2).sin(),
            Interp::SineRecip => 1.0 - (t * FRAC_PI_2).cos(),
            Interp::SCurve => 0.5 - 0.5 * (t * PI).cos(),
            Interp::InvSCurve => 0.5 + (2.0 * t - 1.0).clamp(-1.0, 1.0).asin() / PI,
        }
    }
}

#[derive(Clone)]
struct Curve(Vec<(f32, f32, Interp)>);

impl Curve {
    fn new(points: &[(f32, f32, String)]) -> Curve {
        Curve(points.iter().map(|(x, y, i)| (*x, *y, Interp::parse(i))).collect())
    }

    fn eval(&self, x: f32) -> f32 {
        let p = &self.0;
        let Some(first) = p.first() else { return 0.0 };
        if x <= first.0 {
            return first.1;
        }
        for w in p.windows(2) {
            let ((x0, y0, i), (x1, y1, _)) = (w[0], w[1]);
            if x < x1 {
                let t = if x1 > x0 { (x - x0) / (x1 - x0) } else { 1.0 };
                return y0 + (y1 - y0) * i.shape(t);
            }
        }
        p[p.len() - 1].1
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Param {
    Volume,
    Pitch,
    Other,
}

struct Rtpc {
    name: String,
    param: Param,
    /// The curve's y is stored as linear gain - 1 (Wwise "dB" scaling).
    db_scaled: bool,
    curve: Curve,
}

impl Rtpc {
    fn from_file(r: &RtpcFile) -> Option<Rtpc> {
        if r.rtpc_type.as_deref() == Some("Modulator") {
            return None;
        }
        let param = match key(&r.param).as_str() {
            "Volume" | "MakeUpGain" | "BusVolume" => Param::Volume,
            "Pitch" => Param::Pitch,
            _ => Param::Other,
        };
        (param != Param::Other).then(|| Rtpc { name: key(&r.rtpc), param, db_scaled: r.scaling.as_deref() == Some("dB"), curve: Curve::new(&r.points) })
    }

    /// (dB, cents) this curve adds at the current parameter value.
    fn eval(&self, params: &Params) -> (f32, f32) {
        let y = self.curve.eval(params.get(&self.name));
        match self.param {
            Param::Volume if self.db_scaled => (gain_to_db(1.0 + y), 0.0),
            Param::Volume => (y, 0.0),
            Param::Pitch => (0.0, y),
            Param::Other => (0.0, 0.0),
        }
    }
}

fn gain_to_db(g: f32) -> f32 {
    if g <= 1e-5 { SILENT_DB } else { 20.0 * g.log10() }
}

enum Kind {
    Sound { media: String, looping: bool },
    Random { sequence: bool, continuous: bool, loop_count: i64, avoid_repeat: usize, playlist: Vec<(u32, f32)> },
    Switch { group: String, default: String, switches: HashMap<String, Vec<u32>> },
    Layer { layers: Vec<Layer> },
    Container,
}

struct Layer {
    rtpcs: Vec<Rtpc>,
    crossfade: Option<String>,
    assoc: HashMap<u32, Curve>,
}

struct Node {
    kind: Kind,
    parent: Option<u32>,
    bus: Option<u32>,
    volume_db: f32,
    pitch_cents: f32,
    delay_s: f32,
    volume_range: Option<[f32; 2]>,
    pitch_range: Option<[f32; 2]>,
    rtpcs: Vec<Rtpc>,
    max_instances: Option<MaxInstances>,
    children: Vec<u32>,
}

struct Action {
    play: bool,
    target: u32,
    fade_s: f32,
}

/// Game parameter values (RTPCs), by name.
#[derive(Default)]
pub struct Params {
    values: HashMap<String, f32>,
    defaults: HashMap<String, f32>,
}

impl Params {
    pub fn set(&mut self, name: &str, v: f32) {
        self.values.insert(name.to_string(), v);
    }

    fn get(&self, name: &str) -> f32 {
        self.values.get(name).or_else(|| self.defaults.get(name)).copied().unwrap_or(0.0)
    }
}

/// The extracted part of the game's Wwise project, and what is playing.
#[derive(Resource, Default)]
pub struct Wwise {
    nodes: HashMap<u32, Node>,
    events: HashMap<String, Vec<Action>>,
    cues: HashMap<String, CueFile>,
    rtpc_switches: HashMap<String, (String, Vec<(f32, String)>)>,
    engine: EngineProfile,
    pub params: Params,
    /// Switch values the game sets (switch groups not driven by an RTPC).
    switches: HashMap<String, String>,
    media: HashMap<String, Handle<AudioSource>>,
    /// Random containers: recently played children (avoid repeat) / next sequence index.
    history: HashMap<u32, Vec<u32>>,
    sequence: HashMap<u32, usize>,
    rng: u32,
    next_play: u64,
    loaded: bool,
    /// `--audio-log`: log posted events and, twice a second, every voice's level.
    log: bool,
    log_timer: f32,
}

impl Wwise {
    fn load() -> Wwise {
        let path = asset_root().join(DATA_PATH);
        let Ok(text) = std::fs::read_to_string(&path) else { return Wwise::default() };
        let file: AudioFile = match serde_json::from_str(&text) {
            Ok(f) => f,
            Err(e) => {
                warn!("cannot read {}: {e}; no car sounds", path.display());
                return Wwise::default();
            }
        };
        let num = |v: Option<&Value>| v.and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let mut nodes = HashMap::new();
        for (id, n) in &file.nodes {
            let Ok(id) = id.parse::<u32>() else { continue };
            let kind = match n.kind.as_str() {
                "sound" => Kind::Sound { media: n.media.clone().unwrap_or_default(), looping: n.loop_count == Some(0) },
                "random" => Kind::Random {
                    sequence: n.mode.as_deref() == Some("Sequence"),
                    continuous: n.continuous,
                    loop_count: n.loop_count.unwrap_or(1),
                    avoid_repeat: n.avoid_repeat.unwrap_or(0),
                    playlist: n.playlist.clone(),
                },
                "switch" => Kind::Switch {
                    group: n.group.as_ref().map(key).unwrap_or_default(),
                    default: n.default.as_ref().map(key).unwrap_or_default(),
                    switches: n.switches.clone(),
                },
                "layer" => Kind::Layer {
                    layers: n
                        .layers
                        .iter()
                        .map(|l| Layer {
                            rtpcs: l.rtpcs.iter().filter_map(Rtpc::from_file).collect(),
                            crossfade: l.crossfade.as_ref().filter(|v| !v.is_null()).map(key),
                            assoc: l.assoc.iter().filter_map(|(c, p)| Some((c.parse().ok()?, Curve::new(p)))).collect(),
                        })
                        .collect(),
                },
                _ => Kind::Container,
            };
            let p = |k: &str| num(n.props.get(k));
            nodes.insert(
                id,
                Node {
                    kind,
                    parent: n.parent,
                    bus: n.bus,
                    volume_db: p("Volume") + p("MakeUpGain") + p("BusVolume"),
                    pitch_cents: p("Pitch"),
                    delay_s: p("InitialDelay"),
                    volume_range: n.ranges.get("Volume").copied(),
                    pitch_range: n.ranges.get("Pitch").copied(),
                    rtpcs: n.rtpcs.iter().filter_map(Rtpc::from_file).collect(),
                    max_instances: n.max_instances,
                    children: n.children.clone(),
                },
            );
        }
        let events = file
            .events
            .iter()
            .map(|(name, acts)| {
                let acts = acts
                    .iter()
                    .filter(|a| a.kind == "play" || a.kind == "stop")
                    .filter_map(|a| Some(Action { play: a.kind == "play", target: a.target?, fade_s: a.fade_ms / 1000.0 }))
                    .collect();
                (name.clone(), acts)
            })
            .collect();
        let rtpc_switches = file.game.rtpc_switches.iter().map(|(g, s)| (g.clone(), (key(&s.rtpc), s.points.iter().map(|(x, v, _)| (*x, v.clone())).collect()))).collect();
        let defaults = file.game.params.iter().map(|(k, p)| (k.clone(), p.default)).collect();
        Wwise {
            nodes,
            events,
            cues: file.cues,
            rtpc_switches,
            engine: file.engine,
            params: Params { values: HashMap::new(), defaults },
            rng: 0x2545_f491,
            log: std::env::args().any(|a| a == "--audio-log"),
            ..default()
        }
    }

    pub fn available(&self) -> bool {
        !self.nodes.is_empty()
    }

    fn random(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn switch_value(&self, group: &str, default: &str) -> String {
        if let Some(v) = self.switches.get(group) {
            return v.clone();
        }
        if let Some((rtpc, points)) = self.rtpc_switches.get(group) {
            let x = self.params.get(rtpc);
            if let Some((_, v)) = points.iter().rev().find(|(px, _)| x >= *px).or(points.first()) {
                return v.clone();
            }
        }
        default.to_string()
    }

    /// The ancestors of a node (itself first), following the Wwise parent links.
    fn ancestors(&self, id: u32) -> Vec<u32> {
        let mut out = vec![id];
        let mut cur = id;
        while let Some(p) = self.nodes.get(&cur).and_then(|n| n.parent) {
            if out.contains(&p) {
                break;
            }
            out.push(p);
            cur = p;
        }
        out
    }

    /// Picks a random/sequence container's next child.
    fn pick(&mut self, id: u32) -> Option<u32> {
        let Some(Node { kind: Kind::Random { sequence, avoid_repeat, playlist, .. }, .. }) = self.nodes.get(&id) else { return None };
        let (sequence, avoid, playlist) = (*sequence, *avoid_repeat, playlist.clone());
        if playlist.is_empty() {
            return None;
        }
        if sequence {
            let i = self.sequence.entry(id).or_default();
            let child = playlist[*i % playlist.len()].0;
            *i += 1;
            return Some(child);
        }
        let recent = self.history.get(&id).cloned().unwrap_or_default();
        let allowed: Vec<(u32, f32)> = playlist.iter().copied().filter(|(c, _)| !recent.contains(c)).collect();
        let pool = if allowed.is_empty() { playlist } else { allowed };
        let total: f32 = pool.iter().map(|(_, w)| w.max(0.0)).sum();
        let mut r = self.random() * total;
        let mut child = pool[pool.len() - 1].0;
        for (c, w) in &pool {
            if r < *w {
                child = *c;
                break;
            }
            r -= w;
        }
        let keep = avoid.min(playlist_len(self, id).saturating_sub(1));
        let h = self.history.entry(id).or_default();
        h.push(child);
        while h.len() > keep {
            h.remove(0);
        }
        Some(child)
    }

    /// The sounds a play of `id` starts, with the continuous container each one belongs to.
    fn resolve(&mut self, id: u32, continuous: Option<(u32, i64)>, out: &mut Vec<(u32, Option<(u32, i64)>)>) {
        let Some(node) = self.nodes.get(&id) else { return };
        match &node.kind {
            Kind::Sound { .. } => out.push((id, continuous)),
            Kind::Random { continuous: cont, loop_count, .. } => {
                let seq = if *cont { Some((id, *loop_count)) } else { continuous };
                if let Some(c) = self.pick(id) {
                    self.resolve(c, seq, out);
                }
            }
            Kind::Switch { group, default, switches } => {
                let value = self.switch_value(group, default);
                let children = switches.get(&value).cloned().unwrap_or_default();
                for c in children {
                    self.resolve(c, continuous, out);
                }
            }
            Kind::Layer { .. } | Kind::Container => {
                for c in node.children.clone() {
                    self.resolve(c, continuous, out);
                }
            }
        }
    }
}

fn playlist_len(w: &Wwise, id: u32) -> usize {
    match w.nodes.get(&id) {
        Some(Node { kind: Kind::Random { playlist, .. }, .. }) => playlist.len(),
        _ => 0,
    }
}

// ------------------------------------------------------------------------------------ voices

/// One playing sound.
#[derive(Component)]
pub struct Voice {
    play: u64,
    /// The node the play action targeted (stop actions match it).
    target: u32,
    path: Vec<u32>,
    /// Random volume (dB) / pitch (cents) offsets rolled at play, per node of `path`.
    offsets: Vec<(f32, f32)>,
    age: f32,
    delay: f32,
    fade_in: f32,
    stop: Option<(f32, f32)>,
    continuous: Option<(u32, i64)>,
    started: bool,
}

impl Wwise {
    /// Posts a Wwise event by name.
    pub fn post(&mut self, commands: &mut Commands, voices: &mut Query<(Entity, &mut Voice, Option<&mut AudioSink>)>, event: &str) {
        let Some(actions) = self.events.get(event) else { return };
        if self.log {
            info!("audio event {event}");
        }
        let actions: Vec<(bool, u32, f32)> = actions.iter().map(|a| (a.play, a.target, a.fade_s)).collect();
        for (play, target, fade) in actions {
            if play {
                self.next_play += 1;
                let play = self.next_play;
                let mut sounds = Vec::new();
                self.resolve(target, None, &mut sounds);
                for (sound, continuous) in sounds {
                    self.start(commands, voices, sound, play, target, fade, continuous);
                }
            } else {
                for (_, mut v, _) in voices.iter_mut() {
                    if (v.target == target || v.path.contains(&target)) && v.stop.is_none() {
                        v.stop = Some((v.age, fade));
                    }
                }
            }
        }
    }

    fn start(&mut self, commands: &mut Commands, voices: &mut Query<(Entity, &mut Voice, Option<&mut AudioSink>)>, sound: u32, play: u64, target: u32, fade: f32, continuous: Option<(u32, i64)>) {
        let path = self.ancestors(sound);
        // Instance limits: count the plays already sounding under each limited node.
        for &n in &path {
            let Some(limit) = self.nodes.get(&n).and_then(|x| x.max_instances) else { continue };
            let mut plays: Vec<(u64, f32)> = Vec::new();
            for (_, v, _) in voices.iter() {
                if v.path.contains(&n) && v.stop.is_none() && v.play != play && !plays.iter().any(|p| p.0 == v.play) {
                    plays.push((v.play, v.age));
                }
            }
            if plays.len() >= limit.count {
                if limit.kill_newest {
                    return;
                }
                let oldest = plays.iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|p| p.0);
                for (_, mut v, _) in voices.iter_mut() {
                    if Some(v.play) == oldest {
                        v.stop = Some((v.age, 0.0));
                    }
                }
            }
        }
        let Some(Node { kind: Kind::Sound { media, looping }, .. }) = self.nodes.get(&sound) else { return };
        let Some(handle) = self.media.get(media).cloned() else { return };
        let looping = *looping;
        let mut offsets = Vec::with_capacity(path.len());
        let mut delay = 0.0;
        for &n in &path {
            let (vr, pr, d) = self.nodes.get(&n).map_or((None, None, 0.0), |x| (x.volume_range, x.pitch_range, x.delay_s));
            let v = vr.map_or(0.0, |r| r[0] + (r[1] - r[0]) * self.random());
            let p = pr.map_or(0.0, |r| r[0] + (r[1] - r[0]) * self.random());
            offsets.push((v, p));
            delay += d;
        }
        let voice = Voice { play, target, path, offsets, age: 0.0, delay, fade_in: fade, stop: None, continuous, started: false };
        let (db, cents) = self.level(&voice);
        let settings = PlaybackSettings {
            mode: if looping { PlaybackMode::Loop } else { PlaybackMode::Once },
            volume: Volume::Decibels(if fade > 0.0 { SILENT_DB } else { db }),
            speed: cents_to_speed(cents),
            paused: true,
            ..PlaybackSettings::ONCE
        };
        commands.spawn((AudioPlayer::new(handle), settings, voice));
    }

    /// Volume (dB) and pitch (cents) of a voice now: properties, random offsets and RTPCs of the
    /// sound and its ancestors, layer crossfades, then the output bus chain.
    fn level(&self, v: &Voice) -> (f32, f32) {
        let (mut db, mut cents, mut gain) = (0.0, 0.0, 1.0f32);
        let mut bus = None;
        let mut child = None;
        for (&id, off) in v.path.iter().zip(&v.offsets) {
            let Some(n) = self.nodes.get(&id) else { continue };
            db += n.volume_db + off.0;
            cents += n.pitch_cents + off.1;
            for r in &n.rtpcs {
                let (d, c) = r.eval(&self.params);
                db += d;
                cents += c;
            }
            if let (Kind::Layer { layers }, Some(c)) = (&n.kind, child) {
                for l in layers.iter().filter(|l| l.assoc.contains_key(&c)) {
                    if let Some(x) = &l.crossfade {
                        gain *= l.assoc[&c].eval(self.params.get(x)).clamp(0.0, 1.0);
                    }
                    for r in &l.rtpcs {
                        let (d, cc) = r.eval(&self.params);
                        db += d;
                        cents += cc;
                    }
                }
            }
            if bus.is_none() {
                bus = n.bus;
            }
            child = Some(id);
        }
        let mut seen = 0;
        while let Some(b) = bus.and_then(|b| self.nodes.get(&b)) {
            db += b.volume_db;
            for r in &b.rtpcs {
                db += r.eval(&self.params).0;
            }
            bus = b.parent;
            seen += 1;
            if seen > 32 {
                break;
            }
        }
        (db + gain_to_db(gain), cents)
    }
}

fn cents_to_speed(cents: f32) -> f32 {
    2f32.powf(cents.clamp(-4800.0, 4800.0) / 1200.0)
}

/// Loads every decoded sound up front (they are short; this avoids a gap on first play).
fn preload(mut wwise: ResMut<Wwise>, assets: Res<AssetServer>) {
    if wwise.loaded || !wwise.available() {
        return;
    }
    let media: Vec<String> = wwise.nodes.values().filter_map(|n| if let Kind::Sound { media, .. } = &n.kind { Some(media.clone()) } else { None }).collect();
    for m in media {
        let h = assets.load(format!("rl/audio/{m}"));
        wwise.media.insert(m, h);
    }
    wwise.loaded = true;
}

/// Ages the voices, applies fades and the current levels, ends finished ones and moves continuous
/// playlists on to their next sound.
pub fn update_voices(time: Res<Time>, mut commands: Commands, mut wwise: ResMut<Wwise>, mut voices: Query<(Entity, &mut Voice, Option<&mut AudioSink>)>) {
    let dt = time.delta_secs();
    let mut next: Vec<(u32, u64, u32, i64)> = Vec::new();
    wwise.log_timer += dt;
    let log = wwise.log && wwise.log_timer >= 0.5;
    if log {
        wwise.log_timer = 0.0;
        let p = &wwise.params;
        info!(
            "audio params Speed={:.0} RPM={:.0} Throttle_Input={:.2} WheelForwardSpeed={:.0} WheelSideSpeed={:.0} Car_Pitch/Yaw/Roll={:.2}/{:.2}/{:.2}",
            p.get("Speed"), p.get("RPM"), p.get("Throttle_Input"), p.get("WheelForwardSpeed"), p.get("WheelSideSpeed"), p.get("Car_Pitch"), p.get("Car_Yaw"), p.get("Car_Roll")
        );
    }
    for (e, mut v, sink) in voices.iter_mut() {
        let Some(mut sink) = sink else { continue };
        v.age += dt;
        if !v.started {
            if v.age < v.delay {
                continue;
            }
            sink.play();
            v.started = true;
        }
        let (db, cents) = wwise.level(&v);
        if log {
            let media = match wwise.nodes.get(&v.path[0]).map(|n| &n.kind) {
                Some(Kind::Sound { media, .. }) => media.as_str(),
                _ => "?",
            };
            info!("audio voice {media} target {} {db:+.1} dB {cents:+.0} cents{}", v.target, if v.stop.is_some() { " (stopping)" } else { "" });
        }
        let t = v.age - v.delay;
        let mut fade = if v.fade_in > 0.0 { (t / v.fade_in).min(1.0) } else { 1.0 };
        if let Some((at, len)) = v.stop {
            let f = if len > 0.0 { 1.0 - (v.age - at) / len } else { 0.0 };
            if f <= 0.0 {
                commands.entity(e).despawn();
                continue;
            }
            fade *= f;
        }
        sink.set_volume(Volume::Decibels((db + gain_to_db(fade)).max(SILENT_DB)));
        sink.set_speed(cents_to_speed(cents));
        if sink.empty() {
            commands.entity(e).despawn();
            if let Some((container, loops)) = v.continuous
                && v.stop.is_none()
                && loops != 1
            {
                next.push((container, v.play, v.target, if loops > 1 { loops - 1 } else { loops }));
            }
        }
    }
    for (container, play, target, loops) in next {
        if let Some(child) = wwise.pick(container) {
            let mut sounds = Vec::new();
            wwise.resolve(child, Some((container, loops)), &mut sounds);
            for (sound, cont) in sounds {
                wwise.start(&mut commands, &mut voices, sound, play, target, 0.0, cont);
            }
        }
    }
}

// ------------------------------------------------------------------------------------ the car

/// FInterpTo: moves `current` towards `target` by `rate` per second of the remaining distance.
fn interp_to(current: f32, target: f32, dt: f32, rate: f32) -> f32 {
    if rate <= 0.0 { target } else { current + (target - current) * (dt * rate).min(1.0) }
}

/// What the car's sound components remember between ticks.
#[derive(Resource, Default)]
pub struct CarSounds {
    last: Option<CarState>,
    started: bool,
    whoosh: bool,
    supersonic: bool,
    boosting: bool,
    slide: Option<f32>,
    sliding: bool,
    last_impact: f32,
    clock: f32,
    spam_boost: f32,
    wheel_forward: f32,
    wheel_side: f32,
    engine: Engine,
}

fn cue_param(w: &Wwise, cue: &str, name: &str, default: f32) -> f32 {
    w.cues.get(cue).and_then(|c| c.params.get(name)).and_then(Value::as_f64).map_or(default, |v| v as f32)
}

/// Runs the car's sound logic over every physics tick of this frame.
pub fn drive(time: Res<Time>, mut commands: Commands, sim: Res<crate::Sim>, mut wwise: ResMut<Wwise>, mut car: ResMut<CarSounds>, mut voices: Query<(Entity, &mut Voice, Option<&mut AudioSink>)>) {
    if !wwise.available() {
        return;
    }
    let w = &mut *wwise;
    let c = &mut *car;
    // The local player's car.
    w.params.set("IsLocal", 1.0);
    w.params.set("NumOfLocalPlayers", 1.0);
    if !c.started {
        for cue in ["engine.EngineAudio", "engine.ExhaustAudio", "car_fx.AkWheelDriveSound"] {
            if let Some(ev) = w.cues.get(cue).map(|c| c.play.clone()) {
                w.post(&mut commands, &mut voices, &ev);
            }
        }
        c.started = true;
    }
    let cue = |w: &Wwise, k: &str, play: bool| w.cues.get(k).and_then(|c| if play { Some(c.play.clone()) } else { c.stop.clone() });
    let tick = rl_car_core::TICK_DT;
    for s in sim.ticks.iter() {
        let Some(prev) = c.last.replace(*s) else { continue };
        c.clock += tick;
        let mut post = |w: &mut Wwise, k: &str, play: bool| {
            if let Some(ev) = cue(w, k, play) {
                w.post(&mut commands, &mut voices, &ev);
            }
        };
        // FXActor events.
        if s.has_jumped && !prev.has_jumped {
            post(w, "car_fx.JumpSound", true);
        }
        if s.has_double_jumped && !prev.has_double_jumped {
            post(w, "car_fx.DoubleJumpSound", true);
        }
        if s.has_flipped && !prev.has_flipped {
            post(w, "car_fx.DodgeSound", true);
        }
        let in_air = !s.on_ground;
        if in_air != c.whoosh {
            post(w, "car_fx.WhooshSound", in_air);
            c.whoosh = in_air;
        }
        if s.is_supersonic != c.supersonic {
            if s.is_supersonic {
                post(w, "car_fx.AkEnterSupersonicSound", true);
            }
            post(w, "car_fx.AkLoopSupersonicSound", s.is_supersonic);
            c.supersonic = s.is_supersonic;
        }
        if s.is_boosting != c.boosting {
            if s.is_boosting {
                // AkRTPCDecayComponent on the Boost FX event: +1 per activation, up to MaxValue.
                c.spam_boost = (c.spam_boost + 1.0).min(5.0);
            }
            post(w, "boost.BoostSound", s.is_boosting);
            c.boosting = s.is_boosting;
        }
        if s.last_controls.boost && !prev.last_controls.boost && s.boost_amount <= 0.0 {
            post(w, "boost.DryFireSound", true);
        }
        // Wheel landings: per wheel, with the speed into the surface as the impact momentum.
        let min_wheel = cue_param(w, "car_fx.AkWheelImpactSound", "MinImpactMomentum", 50.0);
        for i in 0..4 {
            if s.wheel_contacts[i]
                && !prev.wheel_contacts[i]
                && let Some((_, n)) = s.wheels[i].contact
            {
                let momentum = -prev.velocity.dot(n);
                if momentum >= min_wheel {
                    w.params.set("ImpactIntensity", momentum);
                    post(w, "car_fx.AkWheelImpactSound", true);
                }
            }
        }
        // Car body against the world (ImpactEffectsComponent): impacts and the slide loop.
        let min_body = cue_param(w, "impacts.AkImpactSound", "MinImpactMomentum", 50.0);
        let min_delay = cue_param(w, "impacts.AkImpactSound", "MinImpactDelay", 0.15);
        let slide_delay = cue_param(w, "impacts.AkSlideSound", "AkSlideSoundDelay", 0.15);
        let slide_min = cue_param(w, "impacts.AkSlideSound", "AkSlideMomentumMin", 200.0);
        match s.world_contact_normal {
            Some(n) => {
                if prev.world_contact_normal.is_none() {
                    let momentum = -prev.velocity.dot(n);
                    if momentum >= min_body && c.clock - c.last_impact >= min_delay {
                        w.params.set("ImpactIntensity", momentum);
                        post(w, "impacts.AkImpactSound", true);
                        c.last_impact = c.clock;
                    }
                }
                let along = s.velocity - n * s.velocity.dot(n);
                let speed = along.length();
                let t = c.slide.unwrap_or(0.0) + tick;
                c.slide = Some(t);
                w.params.set("Car_SlideAngle", if s.velocity.length() > 1.0 { speed / s.velocity.length() } else { 0.0 });
                let slide = t >= slide_delay && speed >= slide_min;
                if slide != c.sliding {
                    post(w, "impacts.AkSlideSound", slide);
                    c.sliding = slide;
                }
            }
            None => {
                c.slide = None;
                if c.sliding {
                    post(w, "impacts.AkSlideSound", false);
                    c.sliding = false;
                }
            }
        }
    }

    // Game parameters from the latest state (frame rate: what the listener hears).
    let Some(s) = c.last else { return };
    let dt = time.delta_secs().min(0.1);
    let speed = s.velocity.length();
    w.params.set("Speed", speed);
    // Angular speed about each car axis, as a fraction of the maximum.
    let local = s.orientation.0.transpose() * s.angular_velocity;
    let max_ang = rl_car_core::consts::CAR_MAX_ANG_SPEED;
    w.params.set("Car_Roll", (local.x.abs() / max_ang).min(1.0));
    w.params.set("Car_Pitch", (local.y.abs() / max_ang).min(1.0));
    w.params.set("Car_Yaw", (local.z.abs() / max_ang).min(1.0));
    // WheelSpeedComponent_TA: forward and sideways speed of the wheels on the ground, eased.
    let p = w.engine.clone();
    let on = s.wheel_contacts.iter().filter(|&&x| x).count();
    let (fwd, side) = if on > 0 { (s.velocity.dot(s.forward()).abs(), s.velocity.dot(s.right()).abs()) } else { (0.0, 0.0) };
    c.wheel_forward = interp_to(c.wheel_forward, fwd, dt, p.WheelForwardSpeedInterpRate);
    c.wheel_side = interp_to(c.wheel_side, side, dt, p.WheelSideSpeedInterpRate);
    w.params.set("WheelForwardSpeed", c.wheel_forward);
    w.params.set("WheelSideSpeed", c.wheel_side);
    w.params.set("WheelsOnGround", on as f32);
    // SpamControl_Boost decays (DecayPerSecond: 0.2/s at 0 up to 3/s at 5).
    let decay = 0.2 + (3.0 - 0.2) * (c.spam_boost / 5.0).clamp(0.0, 1.0);
    c.spam_boost = (c.spam_boost - decay * dt).max(0.0);
    w.params.set("SpamControl_Boost", c.spam_boost);
    let throttle = s.last_controls.throttle;
    let (rpm, input) = c.engine.update(&p, &s, throttle, dt);
    w.params.set("RPM", rpm);
    w.params.set("Throttle_Input", input);
}

// ------------------------------------------------------------------------------------ engine

/// The engine's RPM. The game computes it natively from the engine audio profile; this follows
/// the profile's values by their names: forward gears with randomised shift-up/down points, a gear
/// switch time, the clutched behaviour in the air (`RPMAccelClutched`, `RPMDecelClutched`,
/// `RPMMaxClutched`, for at most `AirMaxThrottleTime` of throttle), the higher shift point while
/// boosting (`RPMShiftUpBoost`), the rev limiter at the last gear (`RevLimitRPM`), and rising /
/// falling smoothing (`RPMAccelFactor`, `RPMDecelFactor`). On the ground the gear and the RPM
/// within it follow the car's forward speed, the gears splitting the car's 0..2300 uu/s evenly.
#[derive(Default)]
pub struct Engine {
    rpm: f32,
    gear: usize,
    shifting: f32,
    air_throttle: f32,
    shift_up: f32,
    shift_down: f32,
    limiter: f32,
    rng: u32,
}

impl Engine {
    fn roll(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    fn pick_points(&mut self, p: &EngineProfile) {
        let g = p.Gears.get(self.gear).cloned().unwrap_or_default();
        self.shift_up = g.RPMShiftUpRange.min + g.RPMShiftUpRange.rand * self.roll();
        self.shift_down = g.RPMShiftDownRange.min + g.RPMShiftDownRange.rand * self.roll();
    }

    /// (RPM, Throttle_Input) after `dt`.
    fn update(&mut self, p: &EngineProfile, s: &CarState, throttle: f32, dt: f32) -> (f32, f32) {
        let gears = p.Gears.len().max(1);
        let idle = p.Gears.first().map_or(1000.0, |g| g.RPMShiftDownRange.min);
        if self.rpm == 0.0 {
            self.rpm = idle;
            self.pick_points(p);
        }
        let input = throttle.abs();
        let target = if s.on_ground {
            self.air_throttle = 0.0;
            let band = rl_car_core::consts::CAR_MAX_SPEED / gears as f32;
            let fwd = s.velocity.dot(s.forward()).abs();
            let gear = ((fwd / band) as usize).min(gears - 1);
            if gear != self.gear {
                self.gear = gear;
                self.shifting = p.GearSwitchTime;
                self.pick_points(p);
            }
            let t = ((fwd - band * gear as f32) / band).clamp(0.0, 1.0);
            let top = self.shift_up + if s.is_boosting { p.RPMShiftUpBoost } else { 0.0 };
            let low = if gear == 0 { idle } else { self.shift_down };
            low + (top - low) * t
        } else {
            // In the air the engine is clutched: throttle revs it, for a limited time.
            self.air_throttle += dt;
            if input > 0.0 && self.air_throttle <= p.AirMaxThrottleTime {
                (self.rpm + p.RPMAccelClutched * input * dt).min(p.RPMMaxClutched)
            } else {
                (self.rpm - p.RPMDecelClutched * dt).max(idle)
            }
        };
        self.shifting = (self.shifting - dt).max(0.0);
        if s.on_ground {
            let rate = if target > self.rpm { p.RPMAccelFactor } else { p.RPMDecelFactor };
            // While a gear engages the engine is clutched and only falls.
            let goal = if self.shifting > 0.0 { target.min(self.rpm) } else { target };
            self.rpm = interp_to(self.rpm, goal, dt, rate);
        } else {
            self.rpm = target;
        }
        // Rev limiter: at the last gear's shift point the RPM bounces back.
        let last = self.gear + 1 >= gears;
        self.limiter = (self.limiter - p.RevLimitRPMDecel * dt).max(0.0);
        if s.on_ground && last && input > 0.0 && self.rpm >= self.shift_up - 1.0 && self.limiter == 0.0 {
            self.limiter = p.RevLimitRPM;
        }
        (self.rpm - self.limiter, input)
    }
}
