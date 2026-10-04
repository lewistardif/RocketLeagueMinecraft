//! The car's visual effects besides the boost, its camera shakes and its gamepad rumble.
//!
//! With the extracted effects (`assets/rl/fx/fx.json`, written by `tools/rl_assets/extract.py`
//! from the game's FX actors and particle systems), the car shows what the game's car shows:
//! - jump smoke (`Jump_Metal_PS`), double jump and dodge smoke, glow and the corner ribbons
//!   (`Dodge_PS`), on the FX actor's Jump / DoubleJump / Dodge events;
//! - while supersonic: the speed streaks around the car (`Supersonic_Team1_PS`/`Team2_PS`, by team)
//!   and the wheel trails (`WheelFX_Supersonic_PS`) on the wheels touching the ground;
//! - sparks where the body hits the world (`VehicleCollisionEffects.FX.Metal_PS`, the arena
//!   surface's entry of the car's impact effects map);
//! - the jump, double jump, dodge, landing and impact camera shakes (scaled by impact momentum as
//!   the game scales them) and the force feedback waveforms, including the boost's.
//!
//! Particle systems are simulated on the CPU from the extracted Cascade modules, applied in the
//! emitters' module order the way UE3 applies them (spawn, then per-frame reset to the base values
//! and the update modules), in Unreal space (uu, Z up) and drawn in Bevy space. The materials are
//! ports of the game's compiled pixel shaders (`shaders/car_fx.wgsl`). Not reproduced: the jump's
//! distortion sphere (it refracts the scene behind), and where the native FX code places the wheel
//! trails (here: on each touching wheel's hub).

use crate::boost::{Dist, Rng};
use crate::convert::{pos_to_bevy, pos_to_rl};
use crate::visuals::{CarVisuals, Team, asset_root};
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use rl_car_core::{CarState, Vec3 as RVec3};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::time::Duration;

const DATA_PATH: &str = "rl/fx/fx.json";

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/car_fx.wgsl");
        app.add_plugins(MaterialPlugin::<FxMaterial>::default())
            .insert_resource(FxData::load())
            .init_resource::<FxState>()
            .init_resource::<CameraShakes>()
            .add_systems(Startup, setup);
    }
}

// ------------------------------------------------------------------------------------ data

#[derive(Deserialize)]
struct FxFile {
    systems: HashMap<String, SystemDef>,
    materials: HashMap<String, Option<MaterialDef>>,
    effects: Vec<EffectDef>,
    wheel_supersonic: Option<String>,
    body_impact: Option<String>,
    shakes: HashMap<String, ShakeEntry>,
}

#[derive(Deserialize)]
struct SystemDef {
    emitters: Vec<EmitterDef>,
}

#[derive(Deserialize)]
struct MaterialDef {
    base: String,
    blend: String,
    textures: Vec<String>,
}

#[derive(Deserialize)]
struct EffectDef {
    name: String,
    system: Option<String>,
    attach_any: Vec<String>,
    attach_all: Vec<String>,
    offset: [f32; 3],
}

#[derive(Deserialize)]
struct EmitterDef {
    kind: String,
    material: String,
    local_space: bool,
    alignment: String,
    subuv: [u32; 2],
    subuv_mode: String,
    duration: f32,
    loops: i32,
    delay: f32,
    spawn: Option<SpawnDef>,
    spawn_per_unit: Option<SpawnPerUnitDef>,
    ribbon: Option<RibbonDef>,
    modules: Vec<Module>,
}

#[derive(Deserialize)]
struct SpawnDef {
    rate: Option<Dist>,
    scale: Option<Dist>,
    process_rate: bool,
    /// (count, count low or -1, time as a fraction of the emitter duration)
    bursts: Vec<(i32, i32, f32)>,
}

#[derive(Deserialize)]
struct SpawnPerUnitDef {
    unit: f32,
    count: Option<Dist>,
    max_frame_distance: f32,
    movement_tolerance: f32,
    process_rate: bool,
    ignore_rate_when_moving: bool,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct RibbonDef {
    MaxTrailCount: Option<usize>,
    MaxParticleInTrailCount: Option<usize>,
    TilingDistance: Option<f32>,
    RenderAxis: Option<String>,
    bSpawnInitialParticle: Option<bool>,
}

/// A Cascade module, as `fx.py` writes it (distributions in Unreal units and axes).
#[derive(Deserialize)]
#[serde(tag = "type")]
#[allow(non_snake_case, clippy::enum_variant_names)]
enum Module {
    Lifetime { LifeTime: Option<Dist> },
    Size { StartSize: Option<Dist> },
    SizeMultiplyLife { LifeMultiplier: Option<Dist>, MultiplyX: Option<bool>, MultiplyY: Option<bool>, MultiplyZ: Option<bool> },
    Velocity { StartVelocity: Option<Dist>, StartVelocityRadial: Option<Dist>, bInWorldSpace: Option<bool> },
    VelocityOverLifetime { VelOverLife: Option<Dist>, Absolute: Option<bool> },
    VelocityInheritParent { Scale: Option<Dist>, MaxAddedVelocity: Option<f32> },
    Acceleration { Acceleration: Option<Dist>, bAlwaysInWorldSpace: Option<bool> },
    Rotation { StartRotation: Option<Dist> },
    RotationRate { StartRotationRate: Option<Dist> },
    Color { StartColor: Option<Dist>, StartAlpha: Option<Dist> },
    ColorOverLife { ColorOverLife: Option<Dist>, AlphaOverLife: Option<Dist> },
    ColorScaleOverLife { ColorScaleOverLife: Option<Dist>, AlphaScaleOverLife: Option<Dist>, bEmitterTime: Option<bool> },
    Location { StartLocation: Option<Dist> },
    LocationPrimitiveSphere {
        StartRadius: Option<Dist>,
        VelocityScale: Option<Dist>,
        StartLocation: Option<Dist>,
        Positive_X: Option<bool>,
        Positive_Y: Option<bool>,
        Positive_Z: Option<bool>,
        Negative_X: Option<bool>,
        Negative_Y: Option<bool>,
        Negative_Z: Option<bool>,
        SurfaceOnly: Option<bool>,
        Velocity: Option<bool>,
    },
    LocationPrimitiveCylinder {
        StartRadius: Option<Dist>,
        StartHeight: Option<Dist>,
        VelocityScale: Option<Dist>,
        StartLocation: Option<Dist>,
        Positive_X: Option<bool>,
        Positive_Y: Option<bool>,
        Positive_Z: Option<bool>,
        Negative_X: Option<bool>,
        Negative_Y: Option<bool>,
        Negative_Z: Option<bool>,
        SurfaceOnly: Option<bool>,
        Velocity: Option<bool>,
        RadialVelocity: Option<bool>,
        HeightAxis: Option<Value>,
    },
    SubUV {},
    CameraOffset { CameraOffset: Option<Dist> },
    OrientationAxisLock {},
    TrailSource { SourceOffsetDefaults: Option<Vec<[f32; 3]>> },
}

#[derive(Deserialize, Clone)]
struct ShakeEntry {
    shake: Option<ShakeDef>,
    rumble: Option<RumbleDef>,
    scale_curve: Option<Vec<[f32; 2]>>,
    min_momentum: Option<f32>,
}

#[derive(Deserialize, Clone)]
struct ShakeDef {
    duration: f32,
    blend_in: f32,
    blend_out: f32,
    rot: HashMap<String, Oscillator>,
    loc: HashMap<String, Oscillator>,
}

#[derive(Deserialize, Clone, Copy)]
struct Oscillator {
    amplitude: f32,
    frequency: f32,
    random_offset: bool,
}

#[derive(Deserialize, Clone)]
struct RumbleDef {
    samples: Vec<RumbleSample>,
}

#[derive(Deserialize, Clone)]
struct RumbleSample {
    left: f32,
    right: f32,
    left_fn: String,
    right_fn: String,
    duration: f32,
}

/// Piecewise-linear InterpCurveFloat (the game's shake scale curves are linear or near enough).
fn eval_curve(points: &[[f32; 2]], x: f32) -> f32 {
    let Some(first) = points.first() else { return 1.0 };
    if x <= first[0] {
        return first[1];
    }
    for w in points.windows(2) {
        if x < w[1][0] {
            let t = (x - w[0][0]) / (w[1][0] - w[0][0]).max(1e-6);
            return w[0][1] + (w[1][1] - w[0][1]) * t;
        }
    }
    points[points.len() - 1][1]
}

fn sample3(d: &Option<Dist>, t: f32, rng: &mut Rng) -> Vec3 {
    d.as_ref().map_or(Vec3::ZERO, |d| Vec3::from_array(d.sample(t, rng)))
}

fn sample1(d: &Option<Dist>, t: f32, rng: &mut Rng, default: f32) -> f32 {
    d.as_ref().map_or(default, |d| d.scalar(t, rng))
}

/// The ported material programs (`car_fx.wgsl`), by base material.
fn material_kind(base: &str) -> Option<u32> {
    let name = base.rsplit('.').next().unwrap_or(base);
    Some(match name {
        "SupersonicStreaks_Mat" => 1,
        "Smoke_Puff_01_Mat" => 2,
        "Unlit_Translucent_Mat" => 3,
        "Spark_Mat" => 4,
        "Glow_Translucent_Mat" => 5,
        "Wheel_Trail_Mat" => 6,
        "DodgeRibbon_Mat" => 7,
        "StandardFlare_Mat" => 8,
        _ => return None,
    })
}

/// Additive or translucent, as the ported shader writes it (Glow01_MIC's permutation writes alpha 0
/// although its parent is translucent).
fn material_additive(kind: u32, blend: &str) -> bool {
    matches!(kind, 1 | 3 | 6 | 7 | 8) || blend == "BLEND_Additive"
}

#[derive(Resource, Default)]
pub struct FxData {
    systems: HashMap<String, SystemDef>,
    materials: HashMap<String, MaterialDef>,
    effects: Vec<EffectDef>,
    wheel_supersonic: Option<String>,
    body_impact: Option<String>,
    shakes: HashMap<String, ShakeEntry>,
}

impl FxData {
    fn load() -> FxData {
        let path = asset_root().join(DATA_PATH);
        let Ok(text) = std::fs::read_to_string(&path) else { return FxData::default() };
        match serde_json::from_str::<FxFile>(&text) {
            Ok(f) => FxData {
                systems: f.systems,
                materials: f.materials.into_iter().filter_map(|(k, v)| Some((k, v?))).collect(),
                effects: f.effects,
                wheel_supersonic: f.wheel_supersonic,
                body_impact: f.body_impact,
                shakes: f.shakes,
            },
            Err(e) => {
                warn!("cannot read {}: {e}; no car effects", path.display());
                FxData::default()
            }
        }
    }
}

// ------------------------------------------------------------------------------------ material

#[derive(Clone, Copy, Default, ShaderType)]
struct FxUniform {
    kind: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct FxMaterial {
    #[uniform(0)]
    params: FxUniform,
    #[texture(1)]
    #[sampler(2)]
    texture: Handle<Image>,
    additive: bool,
}

impl Material for FxMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://rl_car_bevy/shaders/car_fx.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        if self.additive { AlphaMode::Add } else { AlphaMode::Blend }
    }

    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// One mesh per material, rebuilt every frame from the particles drawn with it.
#[derive(Component)]
struct FxMesh;

#[derive(Resource, Default)]
pub struct FxMeshes(HashMap<String, Handle<Mesh>>);

fn setup(mut commands: Commands, data: Res<FxData>, assets: Res<AssetServer>, mut materials: ResMut<Assets<FxMaterial>>, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>) {
    let white = images.add(Image::new_fill(
        bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        &[255, 255, 255, 255],
        bevy::render::render_resource::TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    ));
    let mut handles = FxMeshes::default();
    for (path, m) in &data.materials {
        let Some(kind) = material_kind(&m.base) else {
            warn!("car effects: no port of material {}; its particles are not drawn", m.base);
            continue;
        };
        let texture = m.textures.first().map_or(white.clone(), |t| load_tiling(&assets, &format!("rl/fx/{t}")));
        let mat = materials.add(FxMaterial { params: FxUniform { kind, ..default() }, texture, additive: material_additive(kind, &m.blend) });
        let mesh = meshes.add(empty_mesh());
        commands.spawn((FxMesh, Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::default(), NoFrustumCulling, NotShadowCaster));
        handles.0.insert(path.clone(), mesh);
    }
    commands.insert_resource(handles);
}

fn load_tiling(assets: &AssetServer, path: &str) -> Handle<Image> {
    assets
        .load_builder()
        .with_settings(|s: &mut ImageLoaderSettings| {
            s.is_srgb = false;
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                ..default()
            });
        })
        .load(path.to_string())
}

const NO_MESH: usize = 3;

fn empty_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; NO_MESH]);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; NO_MESH]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; NO_MESH]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0f32; 2]; NO_MESH]);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; NO_MESH]);
    m.insert_indices(Indices::U32((0..NO_MESH as u32).collect()));
    m
}

// ------------------------------------------------------------------------------------ simulation

/// Where a system is: its component's location and rotation (Unreal world space, uu), and the
/// owner's velocity (for VelocityInheritParent).
#[derive(Clone, Copy)]
struct Frame {
    pos: Vec3,
    rot: Mat3,
    owner_vel: Vec3,
}

impl Frame {
    fn to_world(&self, local: Vec3) -> Vec3 {
        self.pos + self.rot * local
    }
}

#[derive(Clone)]
struct Particle {
    pos: Vec3,
    base_vel: Vec3,
    vel: Vec3,
    accel: Vec3,
    base_size: Vec3,
    size: Vec3,
    rel: f32,
    inv_life: f32,
    base_color: Vec4,
    color: Vec4,
    rotation: f32,
    base_rot_rate: f32,
    rot_rate: f32,
    cell: u32,
    camera_offset: f32,
    /// Ribbons: distance along the trail from its first particle (uu).
    distance: f32,
}

#[derive(Default)]
struct EmitterState {
    time: f32,
    loops_done: i32,
    finished: bool,
    spawn_acc: f32,
    bursts_fired: Vec<bool>,
    last_pos: Option<Vec3>,
    travelled: f32,
    particles: Vec<Particle>,
    /// Ribbons: particles per trail, oldest first.
    trails: Vec<Vec<Particle>>,
    trail_last: Vec<Option<Vec3>>,
    trail_travelled: Vec<f32>,
    trail_distance: Vec<f32>,
    started: bool,
}

struct Instance {
    system: String,
    emitters: Vec<EmitterState>,
    active: bool,
    frame: Frame,
    /// Continuous attachments (supersonic): key, so the same one is kept across frames.
    key: Option<String>,
    /// Attached to the car at this offset (car frame, uu): the frame follows the car.
    attached: Option<Vec3>,
}

impl Instance {
    fn new(data: &FxData, system: &str, frame: Frame, key: Option<String>, attached: Option<Vec3>) -> Option<Instance> {
        let def = data.systems.get(system)?;
        Some(Instance { system: system.to_string(), emitters: def.emitters.iter().map(|_| EmitterState::default()).collect(), active: true, frame, key, attached })
    }

    fn alive(&self) -> bool {
        self.active || self.emitters.iter().any(|e| !e.particles.is_empty() || e.trails.iter().any(|t| !t.is_empty()))
    }
}

impl EmitterState {
    fn spawn(&mut self, def: &EmitterDef, frame: &Frame, at: Vec3, age: f32, rng: &mut Rng, emitter_time: f32) -> Particle {
        let mut p = Particle {
            pos: if def.local_space { Vec3::ZERO } else { at },
            base_vel: Vec3::ZERO,
            vel: Vec3::ZERO,
            accel: Vec3::ZERO,
            base_size: Vec3::ZERO,
            size: Vec3::ZERO,
            rel: 0.0,
            inv_life: 0.0,
            base_color: Vec4::ONE,
            color: Vec4::ONE,
            rotation: 0.0,
            base_rot_rate: 0.0,
            rot_rate: 0.0,
            cell: 0,
            camera_offset: 0.0,
            distance: 0.0,
        };
        // Directions in the component's frame for world-space emitters; local-space emitters keep
        // everything in the component frame.
        let to_world = |v: Vec3| if def.local_space { v } else { frame.rot * v };
        for m in &def.modules {
            match m {
                Module::Lifetime { LifeTime } => {
                    let life = sample1(LifeTime, emitter_time, rng, 0.0);
                    p.inv_life = if life > 0.0 { 1.0 / life } else { 0.0 };
                }
                Module::Size { StartSize } => {
                    p.base_size += sample3(StartSize, emitter_time, rng);
                    p.size = p.base_size;
                }
                Module::SizeMultiplyLife { LifeMultiplier, MultiplyX, MultiplyY, MultiplyZ } => {
                    apply_size_mult(&mut p, LifeMultiplier, [*MultiplyX, *MultiplyY, *MultiplyZ], rng);
                }
                Module::Velocity { StartVelocity, StartVelocityRadial, bInWorldSpace } => {
                    let v = sample3(StartVelocity, emitter_time, rng);
                    let mut v = if bInWorldSpace.unwrap_or(false) { v } else { to_world(v) };
                    let radial = sample1(StartVelocityRadial, emitter_time, rng, 0.0);
                    if radial != 0.0 {
                        let origin = if def.local_space { Vec3::ZERO } else { frame.pos };
                        v += (p.pos - origin).normalize_or_zero() * radial;
                    }
                    p.base_vel += v;
                    p.vel += v;
                }
                Module::VelocityInheritParent { Scale, MaxAddedVelocity } => {
                    let scale = Scale.as_ref().map_or(Vec3::ONE, |d| Vec3::from_array(d.sample(emitter_time, rng)));
                    let mut v = frame.owner_vel * scale;
                    if let Some(max) = MaxAddedVelocity
                        && *max > 0.0
                    {
                        v = v.clamp_length_max(*max);
                    }
                    let v = if def.local_space { frame.rot.transpose() * v } else { v };
                    p.base_vel += v;
                    p.vel += v;
                }
                Module::Acceleration { Acceleration, bAlwaysInWorldSpace } => {
                    let a = sample3(Acceleration, emitter_time, rng);
                    p.accel = if bAlwaysInWorldSpace.unwrap_or(false) && !def.local_space { a } else { to_world(a) };
                }
                Module::Rotation { StartRotation } => p.rotation += sample1(StartRotation, emitter_time, rng, 0.0) * TAU,
                Module::RotationRate { StartRotationRate } => {
                    p.base_rot_rate += sample1(StartRotationRate, emitter_time, rng, 0.0) * TAU;
                    p.rot_rate = p.base_rot_rate;
                }
                Module::Color { StartColor, StartAlpha } => {
                    let c = StartColor.as_ref().map_or(Vec3::ONE, |d| Vec3::from_array(d.sample(emitter_time, rng)));
                    p.base_color = c.extend(sample1(StartAlpha, emitter_time, rng, 1.0));
                    p.color = p.base_color;
                }
                Module::ColorOverLife { ColorOverLife, AlphaOverLife } => {
                    let c = color_at(ColorOverLife, AlphaOverLife, 0.0, rng, p.color);
                    p.base_color = c;
                    p.color = c;
                }
                Module::ColorScaleOverLife { ColorScaleOverLife, AlphaScaleOverLife, bEmitterTime } => {
                    let t = if bEmitterTime.unwrap_or(false) { emitter_time } else { 0.0 };
                    p.color *= color_at(ColorScaleOverLife, AlphaScaleOverLife, t, rng, Vec4::ONE);
                }
                Module::Location { StartLocation } => p.pos += to_world(sample3(StartLocation, emitter_time, rng)),
                Module::LocationPrimitiveSphere { StartRadius, VelocityScale, StartLocation, Positive_X, Positive_Y, Positive_Z, Negative_X, Negative_Y, Negative_Z, SurfaceOnly, Velocity } => {
                    let flags = [[*Positive_X, *Negative_X], [*Positive_Y, *Negative_Y], [*Positive_Z, *Negative_Z]];
                    let mut dir = unit_direction(flags, rng);
                    if SurfaceOnly.unwrap_or(false) {
                        dir = dir.normalize_or_zero();
                    }
                    let offset = dir * sample1(StartRadius, emitter_time, rng, 0.0) + sample3(StartLocation, emitter_time, rng);
                    p.pos += to_world(offset);
                    if Velocity.unwrap_or(false) {
                        let v = to_world(offset) * sample1(VelocityScale, emitter_time, rng, 1.0);
                        p.vel += v;
                        p.base_vel += v;
                    }
                }
                Module::LocationPrimitiveCylinder {
                    StartRadius, StartHeight, VelocityScale, StartLocation, Positive_X, Positive_Y, Positive_Z, Negative_X, Negative_Y, Negative_Z, SurfaceOnly, Velocity, RadialVelocity, HeightAxis,
                } => {
                    let axis = match HeightAxis.as_ref().and_then(Value::as_str) {
                        Some("PMLPC_HEIGHTAXIS_X") => 0,
                        Some("PMLPC_HEIGHTAXIS_Y") => 1,
                        _ => 2,
                    };
                    let flags = [[*Positive_X, *Negative_X], [*Positive_Y, *Negative_Y], [*Positive_Z, *Negative_Z]];
                    let mut dir = unit_direction(flags, rng);
                    let along = dir[axis];
                    dir[axis] = 0.0;
                    if SurfaceOnly.unwrap_or(false) {
                        dir = dir.normalize_or_zero();
                    }
                    let radius = sample1(StartRadius, emitter_time, rng, 0.0);
                    let height = sample1(StartHeight, emitter_time, rng, 0.0);
                    let mut offset = dir * radius;
                    offset[axis] = along * height * 0.5;
                    let offset = offset + sample3(StartLocation, emitter_time, rng);
                    p.pos += to_world(offset);
                    if Velocity.unwrap_or(false) {
                        let mut v = offset * sample1(VelocityScale, emitter_time, rng, 1.0);
                        if RadialVelocity.unwrap_or(false) {
                            v[axis] = 0.0;
                        }
                        let v = to_world(v);
                        p.vel += v;
                        p.base_vel += v;
                    }
                }
                Module::VelocityOverLifetime { VelOverLife, Absolute } => {
                    // Absolute: the velocity is the curve's (in world space) from the start.
                    if Absolute.unwrap_or(false) && VelOverLife.is_some() {
                        let v = sample3(VelOverLife, 0.0, rng);
                        p.vel = v;
                        p.base_vel = v;
                    }
                }
                Module::SubUV {} | Module::OrientationAxisLock {} | Module::TrailSource { .. } => {}
                Module::CameraOffset { CameraOffset } => p.camera_offset = sample1(CameraOffset, emitter_time, rng, 0.0),
            }
        }
        let cells = def.subuv[0] * def.subuv[1];
        if cells > 1 && def.subuv_mode == "PSUVIM_Random" {
            p.cell = ((rng.next() * cells as f32) as u32).min(cells - 1);
        }
        // Spawned during the frame: catch up with the time since then.
        p.rel += age * p.inv_life;
        p.pos += p.vel * age;
        p
    }
}

fn unit_direction(flags: [[Option<bool>; 2]; 3], rng: &mut Rng) -> Vec3 {
    let mut v = Vec3::ZERO;
    for (i, [pos, neg]) in flags.iter().enumerate() {
        let (pos, neg) = (pos.unwrap_or(true), neg.unwrap_or(true));
        v[i] = match (pos, neg) {
            (true, true) => rng.next() * 2.0 - 1.0,
            (true, false) => rng.next(),
            (false, true) => -rng.next(),
            (false, false) => 0.0,
        };
    }
    v
}

fn color_at(c: &Option<Dist>, a: &Option<Dist>, t: f32, rng: &mut Rng, default: Vec4) -> Vec4 {
    let rgb = c.as_ref().map_or(default.truncate(), |d| Vec3::from_array(d.sample(t, rng)));
    let alpha = a.as_ref().map_or(default.w, |d| d.scalar(t, rng));
    rgb.extend(alpha)
}

fn apply_size_mult(p: &mut Particle, d: &Option<Dist>, axes: [Option<bool>; 3], rng: &mut Rng) {
    let m = d.as_ref().map_or(Vec3::ONE, |d| Vec3::from_array(d.sample(p.rel, rng)));
    for i in 0..3 {
        if axes[i].unwrap_or(i < 2) {
            p.size[i] *= m[i];
        }
    }
}

/// One frame of a particle: reset to the base values, the update modules in order, then move.
fn update_particle(p: &mut Particle, def: &EmitterDef, dt: f32, emitter_time: f32, rng: &mut Rng) {
    p.rel += dt * p.inv_life;
    p.vel = p.base_vel;
    p.size = p.base_size;
    p.rot_rate = p.base_rot_rate;
    p.color = p.base_color;
    for m in &def.modules {
        match m {
            Module::SizeMultiplyLife { LifeMultiplier, MultiplyX, MultiplyY, MultiplyZ } => apply_size_mult(p, LifeMultiplier, [*MultiplyX, *MultiplyY, *MultiplyZ], rng),
            Module::VelocityOverLifetime { VelOverLife, Absolute } => {
                let v = sample3(VelOverLife, p.rel, rng);
                if VelOverLife.is_some() {
                    if Absolute.unwrap_or(false) {
                        p.vel = v;
                        p.base_vel = v;
                    } else {
                        p.vel *= v;
                    }
                }
            }
            Module::Acceleration { .. } => {
                p.vel += p.accel * dt;
                p.base_vel += p.accel * dt;
            }
            Module::ColorOverLife { ColorOverLife, AlphaOverLife } => p.color = color_at(ColorOverLife, AlphaOverLife, p.rel, rng, p.color),
            Module::ColorScaleOverLife { ColorScaleOverLife, AlphaScaleOverLife, bEmitterTime } => {
                let t = if bEmitterTime.unwrap_or(false) { emitter_time } else { p.rel };
                p.color *= color_at(ColorScaleOverLife, AlphaScaleOverLife, t, rng, Vec4::ONE);
            }
            _ => {}
        }
    }
    p.pos += p.vel * dt;
    p.rotation += p.rot_rate * dt;
}

fn tick_emitter(st: &mut EmitterState, def: &EmitterDef, frame: &Frame, active: bool, dt: f32, rng: &mut Rng) {
    let first = !st.started;
    st.started = true;
    if first {
        st.bursts_fired = def.spawn.as_ref().map_or(Vec::new(), |s| vec![false; s.bursts.len()]);
    }
    // Emitter time and loops (UE3: EmitterTime over EmitterDuration, EmitterLoops 0 = forever).
    let prev_time = st.time;
    st.time += dt;
    let duration = def.duration.max(1e-4);
    let local_time = (st.time - def.delay).max(0.0);
    if local_time >= duration * (st.loops_done + 1) as f32 {
        st.loops_done += 1;
        st.bursts_fired.iter_mut().for_each(|b| *b = false);
        if def.loops > 0 && st.loops_done >= def.loops {
            st.finished = true;
        }
    }
    let emitter_time = (local_time % duration) / duration;

    for p in &mut st.particles {
        update_particle(p, def, dt, emitter_time, rng);
    }
    st.particles.retain(|p| p.inv_life == 0.0 || p.rel < 1.0);
    for t in &mut st.trails {
        for p in t.iter_mut() {
            update_particle(p, def, dt, emitter_time, rng);
        }
        t.retain(|p| p.inv_life == 0.0 || p.rel < 1.0);
    }

    let spawning = active && !st.finished && st.time >= def.delay;
    if def.kind == "ribbon" {
        tick_ribbon(st, def, frame, spawning, first, dt, emitter_time, rng);
        return;
    }
    if !spawning {
        st.last_pos = None;
        return;
    }
    let world = frame.pos;
    let mut count = 0usize;
    let mut moving = false;
    if let Some(spu) = &def.spawn_per_unit {
        if let Some(last) = st.last_pos {
            let travel = (world - last).length();
            moving = travel > spu.movement_tolerance * spu.unit;
            if spu.max_frame_distance > 0.0 && travel > spu.max_frame_distance {
                st.travelled = 0.0;
            } else if moving {
                let per_unit = sample1(&spu.count, emitter_time, rng, 0.0);
                let total = travel + st.travelled;
                let n = (total * per_unit / spu.unit).floor();
                st.travelled = total - n * spu.unit / per_unit.max(1e-3);
                for k in 0..n as usize {
                    let f = (k + 1) as f32 / n;
                    let p = st.spawn(def, frame, last.lerp(world, f), dt * (1.0 - f), rng, emitter_time);
                    st.particles.push(p);
                    count += 1;
                }
            }
        }
        st.last_pos = Some(world);
    }
    let process_rate = def.spawn_per_unit.as_ref().is_none_or(|s| s.process_rate && !(s.ignore_rate_when_moving && moving));
    if let Some(sp) = &def.spawn {
        if sp.process_rate && process_rate {
            let rate = sample1(&sp.rate, emitter_time, rng, 0.0) * sample1(&sp.scale, emitter_time, rng, 1.0);
            st.spawn_acc += rate * dt;
            while st.spawn_acc >= 1.0 {
                st.spawn_acc -= 1.0;
                let age = if rate > 0.0 { (st.spawn_acc / rate).min(dt) } else { 0.0 };
                let p = st.spawn(def, frame, world, age, rng, emitter_time);
                st.particles.push(p);
                count += 1;
                if count > 4096 {
                    break;
                }
            }
        }
        let frac_prev = ((prev_time - def.delay).max(0.0) % duration) / duration;
        for (i, (n, low, t)) in sp.bursts.iter().enumerate() {
            if !st.bursts_fired[i] && (emitter_time >= *t || frac_prev > emitter_time) {
                st.bursts_fired[i] = true;
                let n = if *low >= 0 { *low + ((*n - *low + 1) as f32 * rng.next()) as i32 } else { *n };
                for _ in 0..n.max(0) {
                    let p = st.spawn(def, frame, world, 0.0, rng, emitter_time);
                    st.particles.push(p);
                }
            }
        }
    }
}

/// Ribbon emitters (TypeDataRibbon): a trail per source (TrailSource offsets in the component's
/// frame, or the component itself), particles laid along it by spawn rate / distance.
fn tick_ribbon(st: &mut EmitterState, def: &EmitterDef, frame: &Frame, spawning: bool, first: bool, dt: f32, emitter_time: f32, rng: &mut Rng) {
    let r = def.ribbon.as_ref();
    let offsets: Vec<Vec3> = def
        .modules
        .iter()
        .find_map(|m| if let Module::TrailSource { SourceOffsetDefaults } = m { SourceOffsetDefaults.clone() } else { None })
        .map(|o| o.into_iter().map(Vec3::from_array).collect())
        .unwrap_or_else(|| vec![Vec3::ZERO]);
    let trails = r.and_then(|r| r.MaxTrailCount).unwrap_or(1).clamp(1, offsets.len().max(1));
    let max_particles = r.and_then(|r| r.MaxParticleInTrailCount).filter(|n| *n > 0).unwrap_or(256);
    if st.trails.len() != trails {
        st.trails = vec![Vec::new(); trails];
        st.trail_last = vec![None; trails];
        st.trail_travelled = vec![0.0; trails];
        st.trail_distance = vec![0.0; trails];
    }
    if !spawning {
        st.trail_last.iter_mut().for_each(|l| *l = None);
        return;
    }
    for i in 0..trails {
        let source = frame.to_world(offsets[i.min(offsets.len() - 1)]);
        let spawn_at = |st: &mut EmitterState, at: Vec3, age: f32, rng: &mut Rng| {
            let last = st.trails[i].last().map(|p| p.pos);
            st.trail_distance[i] += last.map_or(0.0, |l| (at - l).length());
            let mut p = st.spawn(def, frame, at, age, rng, emitter_time);
            p.distance = st.trail_distance[i];
            st.trails[i].push(p);
            if st.trails[i].len() > max_particles {
                st.trails[i].remove(0);
            }
        };
        if first && r.and_then(|r| r.bSpawnInitialParticle).unwrap_or(false) {
            spawn_at(st, source, 0.0, rng);
        }
        if let Some(spu) = &def.spawn_per_unit
            && let Some(last) = st.trail_last[i]
        {
            let travel = (source - last).length();
            if travel > spu.movement_tolerance * spu.unit {
                let per_unit = sample1(&spu.count, emitter_time, rng, 0.0);
                let total = travel + st.trail_travelled[i];
                let n = (total * per_unit / spu.unit).floor();
                st.trail_travelled[i] = total - n * spu.unit / per_unit.max(1e-3);
                for k in 0..n as usize {
                    let f = (k + 1) as f32 / n;
                    spawn_at(st, last.lerp(source, f), dt * (1.0 - f), rng);
                }
            }
        }
        st.trail_last[i] = Some(source);
        if let Some(sp) = &def.spawn
            && sp.process_rate
        {
            // The rate is shared by the trails (UE3 spawns each trail's share).
            let rate = sample1(&sp.rate, emitter_time, rng, 0.0) * sample1(&sp.scale, emitter_time, rng, 1.0) / trails as f32;
            let acc = st.spawn_acc + rate * dt;
            let n = acc.floor();
            if i + 1 == trails {
                st.spawn_acc = acc - n;
            }
            for _ in 0..n as usize {
                spawn_at(st, source, 0.0, rng);
            }
        }
    }
}

// ------------------------------------------------------------------------------------ state

#[derive(Resource, Default)]
pub struct FxState {
    instances: Vec<Instance>,
    rng: Rng,
    last: Option<CarState>,
    last_impact: f32,
    clock: f32,
}

/// The car's component frame in Unreal space (its centre; the FX actor follows the car).
fn car_frame(s: &CarState) -> Frame {
    let o = s.orientation.0;
    let col = |v: RVec3| Vec3::new(v.x, v.y, v.z);
    Frame { pos: col(s.position), rot: Mat3::from_cols(col(o.col(0)), col(o.col(1)), col(o.col(2))), owner_vel: col(s.velocity) }
}

/// Runs the car's FX actor logic over the frame's physics ticks, then simulates every instance.
pub fn update(
    time: Res<Time>,
    sim: Res<crate::Sim>,
    data: Res<FxData>,
    visuals: Res<CarVisuals>,
    mut state: ResMut<FxState>,
    mut shakes: ResMut<CameraShakes>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
    gamepads: Query<Entity, With<Gamepad>>,
) {
    if data.systems.is_empty() && data.shakes.is_empty() {
        return;
    }
    let st = &mut *state;
    let tick = rl_car_core::TICK_DT;
    let mut play_shake = |name: &str, scale: f32, shakes: &mut CameraShakes, rng: &mut Rng| {
        let Some(entry) = data.shakes.get(name) else { return };
        if scale <= 0.0 {
            return;
        }
        if let Some(def) = &entry.shake {
            shakes.start(def, scale, rng);
        }
        if let Some(r) = &entry.rumble {
            for pad in gamepads.iter() {
                send_rumble(&mut rumble, pad, r, scale);
            }
        }
    };
    for s in sim.ticks.iter() {
        let Some(prev) = st.last.replace(*s) else { continue };
        st.clock += tick;
        let frame = car_frame(s);
        let mut events: Vec<&str> = Vec::new();
        if s.has_jumped && !prev.has_jumped {
            events.push("Jump");
        }
        if s.has_double_jumped && !prev.has_double_jumped {
            events.push("DoubleJump");
        }
        if s.has_flipped && !prev.has_flipped {
            events.push("Dodge");
        }
        if s.is_boosting && !prev.is_boosting {
            play_shake("BoostActive", 1.0, &mut shakes, &mut st.rng);
        }
        for ev in &events {
            for e in data.effects.iter().filter(|e| e.attach_any.iter().any(|a| a == ev)) {
                let Some(sys) = &e.system else { continue };
                let offset = Vec3::from_array(e.offset);
                let f = Frame { pos: frame.to_world(offset), ..frame };
                if let Some(i) = Instance::new(&data, sys, f, None, Some(offset)) {
                    st.instances.push(i);
                }
            }
            play_shake(ev, 1.0, &mut shakes, &mut st.rng);
        }
        // Wheel landings: the landing shake, scaled by the impact momentum (ShakeScaleCurve).
        if let Some(entry) = data.shakes.get("WheelImpact") {
            let mut best = 0.0f32;
            for i in 0..4 {
                if s.wheel_contacts[i]
                    && !prev.wheel_contacts[i]
                    && let Some((_, n)) = s.wheels[i].contact
                {
                    best = best.max(-prev.velocity.dot(n));
                }
            }
            if best >= entry.min_momentum.unwrap_or(0.0) && best > 0.0 {
                let scale = entry.scale_curve.as_deref().map_or(1.0, |c| eval_curve(c, best));
                play_shake("WheelImpact", scale, &mut shakes, &mut st.rng);
            }
        }
        // Body impacts: sparks at the contact, and the impact shake.
        if let Some(n) = s.world_contact_normal
            && prev.world_contact_normal.is_none()
        {
            let momentum = -prev.velocity.dot(n);
            let entry = data.shakes.get("BodyImpact");
            if momentum >= entry.and_then(|e| e.min_momentum).unwrap_or(0.0) && st.clock - st.last_impact >= 0.15 {
                st.last_impact = st.clock;
                let scale = entry.and_then(|e| e.scale_curve.as_deref()).map_or(1.0, |c| eval_curve(c, momentum));
                play_shake("BodyImpact", scale, &mut shakes, &mut st.rng);
                if let Some(sys) = &data.body_impact {
                    // The hit point: the car's hitbox face towards the surface; the effect's X axis
                    // is the surface normal (the game spawns it with the hit normal's rotation).
                    let cfg = s.config();
                    let half = cfg.effective_half_extents();
                    let ln = frame.rot.transpose() * Vec3::new(n.x, n.y, n.z);
                    let reach = (ln * Vec3::new(half.x, half.y, half.z)).abs().element_sum();
                    let nn = Vec3::new(n.x, n.y, n.z);
                    let pos = frame.pos - nn * reach;
                    let x = nn;
                    let y = if x.z.abs() < 0.9 { Vec3::Z.cross(x).normalize() } else { Vec3::X.cross(x).normalize() };
                    let f = Frame { pos, rot: Mat3::from_cols(x, y, x.cross(y)), owner_vel: frame.owner_vel };
                    if let Some(i) = Instance::new(&data, sys, f, None, None) {
                        st.instances.push(i);
                    }
                }
            }
        }
    }
    let Some(s) = st.last else { return };
    let frame = car_frame(&s);

    // Continuous attachments: the supersonic streaks (by team) and wheel trails.
    let team = if visuals.team == Team::Blue { "Team0" } else { "Team1" };
    let mut wanted: Vec<(String, String, Frame)> = Vec::new();
    for e in &data.effects {
        let Some(sys) = &e.system else { continue };
        if e.attach_all.is_empty() {
            continue;
        }
        let on = e.attach_all.iter().all(|a| match a.as_str() {
            "SuperSonic" => s.is_supersonic,
            "Team0" | "Team1" => a == team,
            _ => false,
        });
        if on {
            wanted.push((e.name.clone(), sys.clone(), Frame { pos: frame.to_world(Vec3::from_array(e.offset)), ..frame }));
        }
    }
    if let Some(sys) = &data.wheel_supersonic
        && s.is_supersonic
    {
        let cfg = s.config();
        for i in 0..4 {
            if !s.wheel_contacts[i] {
                continue;
            }
            let (cp, _, _, _) = cfg.wheel(i);
            let hub = Vec3::new(cp.x, cp.y, cp.z - s.wheels[i].suspension_length);
            wanted.push((format!("wheel{i}"), sys.clone(), Frame { pos: frame.to_world(hub), ..frame }));
        }
    }
    for inst in &mut st.instances {
        if let Some(k) = &inst.key {
            match wanted.iter().position(|(wk, _, _)| wk == k) {
                Some(i) => {
                    inst.frame = wanted[i].2;
                    inst.active = true;
                    wanted.remove(i);
                }
                None => inst.active = false,
            }
        } else if let Some(offset) = inst.attached {
            // One-shot systems on the FX actor ride along with the car.
            inst.frame = Frame { pos: frame.to_world(offset), ..frame };
        }
    }
    for (k, sys, f) in wanted {
        if let Some(i) = Instance::new(&data, &sys, f, Some(k), None) {
            st.instances.push(i);
        }
    }

    let dt = time.delta_secs().min(0.1);
    let FxState { instances, rng, .. } = st;
    for inst in instances.iter_mut() {
        let Some(def) = data.systems.get(&inst.system) else { continue };
        for (e, ed) in inst.emitters.iter_mut().zip(&def.emitters) {
            tick_emitter(e, ed, &inst.frame, inst.active, dt, rng);
        }
        if inst.key.is_none() && inst.emitters.iter().zip(&def.emitters).all(|(e, _)| e.finished) {
            inst.active = false;
        }
    }
    instances.retain(|i| i.alive());
}

/// Draws every instance's particles into the per-material meshes.
pub fn draw(data: Res<FxData>, state: Res<FxState>, handles: Option<Res<FxMeshes>>, camera: Query<&Transform, With<Camera3d>>, mut meshes: ResMut<Assets<Mesh>>) {
    let (Some(handles), Ok(camera)) = (handles, camera.single()) else { return };
    let cam = pos_to_rl(camera.translation);
    let cam = Vec3::new(cam.x, cam.y, cam.z);
    let to_rl = |v: Vec3| {
        let r = crate::convert::dir_to_rl(v);
        Vec3::new(r.x, r.y, r.z)
    };
    let (cam_right, cam_up) = (to_rl(camera.rotation * Vec3::X), to_rl(camera.rotation * Vec3::Y));
    let mut buffers: HashMap<&str, Buffers> = HashMap::new();
    for inst in &state.instances {
        let Some(def) = data.systems.get(&inst.system) else { continue };
        for (e, ed) in inst.emitters.iter().zip(&def.emitters) {
            if !handles.0.contains_key(&ed.material) {
                continue;
            }
            let b = buffers.entry(ed.material.as_str()).or_default();
            let place = |p: &Particle| if ed.local_space { inst.frame.to_world(p.pos) } else { p.pos };
            if ed.kind == "ribbon" {
                let tiling = ed.ribbon.as_ref().and_then(|r| r.TilingDistance).filter(|t| *t > 0.0).unwrap_or(1.0);
                let world_up = ed.ribbon.as_ref().and_then(|r| r.RenderAxis.as_deref()) == Some("Trails_WorldUp");
                for trail in &e.trails {
                    let pts: Vec<(Vec3, &Particle)> = trail.iter().rev().map(|p| (place(p), p)).collect();
                    b.ribbon(&pts, cam, world_up, tiling);
                }
            } else {
                for p in &e.particles {
                    b.sprite(ed, place(p), p, cam, cam_right, cam_up, &inst.frame);
                }
            }
        }
    }
    for (path, handle) in &handles.0 {
        let Some(mut m) = meshes.get_mut(handle) else { continue };
        let b = buffers.remove(path.as_str()).unwrap_or_default();
        b.write(&mut m, cam);
    }
}

#[derive(Default)]
struct Buffers {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    uv_b: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    quads: Vec<(f32, [u32; 6])>,
}

impl Buffers {
    fn push(&mut self, p: Vec3, uv: [f32; 2], uv_b: [f32; 2], c: Vec4) -> u32 {
        self.pos.push(pos_to_bevy(RVec3::new(p.x, p.y, p.z)).to_array());
        self.uv.push(uv);
        self.uv_b.push(uv_b);
        self.col.push(c.to_array());
        (self.pos.len() - 1) as u32
    }

    fn sprite(&mut self, def: &EmitterDef, center: Vec3, p: &Particle, cam: Vec3, right: Vec3, up: Vec3, frame: &Frame) {
        let to_cam = (cam - center).normalize_or_zero();
        let center = center + to_cam * p.camera_offset;
        let (r, u) = if def.alignment == "PSA_Velocity" {
            // The sprite's up axis along the velocity, its right axis facing the camera.
            let v = if def.local_space { frame.rot * p.vel } else { p.vel };
            let up = v.normalize_or_zero();
            if up == Vec3::ZERO {
                return;
            }
            let right = to_cam.cross(up).normalize_or_zero();
            (right * p.size.x * 0.5, up * p.size.y * 0.5)
        } else {
            let (s, c) = p.rotation.sin_cos();
            ((right * c + up * s) * p.size.x * 0.5, (up * c - right * s) * p.size.x * 0.5)
        };
        let (cols, rows) = (def.subuv[0].max(1), def.subuv[1].max(1));
        let (u0, v0) = ((p.cell % cols) as f32 / cols as f32, (p.cell / cols) as f32 / rows as f32);
        let (du, dv) = (1.0 / cols as f32, 1.0 / rows as f32);
        let base = self.pos.len() as u32;
        for (corner, t) in [(center - r + u, [u0, v0]), (center + r + u, [u0 + du, v0]), (center + r - u, [u0 + du, v0 + dv]), (center - r - u, [u0, v0 + dv])] {
            self.push(corner, t, [0.0, 0.0], p.color);
        }
        let d = (center - cam).length_squared();
        self.quads.push((d, [base, base + 1, base + 2, base, base + 2, base + 3]));
    }

    /// A strip through the trail's points, head (newest) first: UV (along 0..1, across 0..1),
    /// UV_B (across 0..1, distance from the head / TilingDistance).
    fn ribbon(&mut self, pts: &[(Vec3, &Particle)], cam: Vec3, world_up: bool, tiling: f32) {
        if pts.len() < 2 {
            return;
        }
        let n = pts.len();
        let head = pts[0].1.distance;
        let mut prev: Option<(u32, u32)> = None;
        for (k, (pos, p)) in pts.iter().enumerate() {
            let tangent = if k + 1 < n { pts[k + 1].0 - *pos } else { *pos - pts[k - 1].0 }.normalize_or_zero();
            let side = if world_up { Vec3::Z } else { tangent.cross((cam - *pos).normalize_or_zero()).normalize_or_zero() };
            let half = side * p.size.x * 0.5;
            let along = k as f32 / (n - 1) as f32;
            let dist = (head - p.distance).abs() / tiling;
            let a = self.push(*pos - half, [along, 0.0], [0.0, dist], p.color);
            let b = self.push(*pos + half, [along, 1.0], [1.0, dist], p.color);
            if let Some((pa, pb)) = prev {
                let d = (*pos - cam).length_squared();
                self.quads.push((d, [pa, pb, b, pa, b, a]));
            }
            prev = Some((a, b));
        }
    }

    fn write(mut self, m: &mut Mesh, _cam: Vec3) {
        // Back to front (the translucent materials need it; additive ones do not mind).
        self.quads.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut idx: Vec<u32> = self.quads.iter().flat_map(|q| q.1).collect();
        if self.pos.is_empty() {
            self.pos = vec![[0.0; 3]; NO_MESH];
            self.uv = vec![[0.0; 2]; NO_MESH];
            self.uv_b = vec![[0.0; 2]; NO_MESH];
            self.col = vec![[0.0; 4]; NO_MESH];
        }
        if idx.is_empty() {
            idx = (0..NO_MESH as u32).collect();
        }
        let normals = vec![[0.0, 1.0, 0.0]; self.pos.len()];
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uv);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.uv_b);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.col);
        m.insert_indices(Indices::U32(idx));
    }
}

// ------------------------------------------------------------------------------------ shakes

/// Playing camera shakes (UE3 CameraShake oscillations: each oscillator `Amplitude *
/// sin(offset + Frequency * t)`, blended in and out, for OscillationDuration seconds; 0 means
/// no oscillation, less than 0 forever).
#[derive(Resource, Default)]
pub struct CameraShakes(Vec<Shake>);

struct Shake {
    def: ShakeDef,
    scale: f32,
    time: f32,
    offsets: HashMap<String, f32>,
}

impl CameraShakes {
    fn start(&mut self, def: &ShakeDef, scale: f32, rng: &mut Rng) {
        if def.duration == 0.0 {
            return;
        }
        let mut offsets = HashMap::new();
        for (k, o) in def.rot.iter().map(|(k, o)| (format!("rot.{k}"), o)).chain(def.loc.iter().map(|(k, o)| (format!("loc.{k}"), o))) {
            offsets.insert(k, if o.random_offset { rng.next() * TAU } else { 0.0 });
        }
        self.0.push(Shake { def: def.clone(), scale, time: 0.0, offsets });
    }

    /// Advances the shakes and returns the camera's location offset (uu, camera X forward, Y
    /// right, Z up) and rotation offset (pitch, yaw, roll in radians).
    pub fn advance(&mut self, dt: f32) -> (Vec3, Vec3) {
        let (mut loc, mut rot) = (Vec3::ZERO, Vec3::ZERO);
        for s in &mut self.0 {
            s.time += dt;
            let d = &s.def;
            let mut w = 1.0f32;
            if d.blend_in > 0.0 {
                w = w.min(s.time / d.blend_in);
            }
            if d.duration > 0.0 && d.blend_out > 0.0 {
                w = w.min((d.duration - s.time) / d.blend_out);
            }
            let w = w.clamp(0.0, 1.0) * s.scale;
            let osc = |k: &str, o: &Oscillator| o.amplitude * (s.offsets.get(k).copied().unwrap_or(0.0) + o.frequency * s.time).sin();
            for (axis, o) in &d.loc {
                let v = osc(&format!("loc.{axis}"), o) * w;
                match axis.as_str() {
                    "X" => loc.x += v,
                    "Y" => loc.y += v,
                    "Z" => loc.z += v,
                    _ => {}
                }
            }
            for (axis, o) in &d.rot {
                // Rotator units: 65536 per turn.
                let v = osc(&format!("rot.{axis}"), o) * w * TAU / 65536.0;
                match axis.as_str() {
                    "Pitch" => rot.x += v,
                    "Yaw" => rot.y += v,
                    "Roll" => rot.z += v,
                    _ => {}
                }
            }
        }
        self.0.retain(|s| s.def.duration < 0.0 || s.time < s.def.duration);
        (loc, rot)
    }
}

/// UE3 force feedback waveform sample shapes (amplitude over the sample's duration).
fn waveform(f: &str, t: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    match f {
        "WF_LinearIncreasing" => t,
        "WF_LinearDecreasing" => 1.0 - t,
        "WF_Sin0to90" => (t * FRAC_PI_2).sin(),
        "WF_Sin90to180" => (FRAC_PI_2 + t * FRAC_PI_2).sin(),
        "WF_Sin0to180" => (t * PI).sin(),
        _ => 1.0,
    }
}

/// Sends a waveform's samples as rumbles, left motor = strong, right = weak as on an XInput pad.
/// A Bevy rumble has a constant strength, so each sample plays at its waveform's mean strength;
/// looping waveforms (the boost's) play once.
fn send_rumble(w: &mut MessageWriter<GamepadRumbleRequest>, pad: Entity, r: &RumbleDef, scale: f32) {
    for s in &r.samples {
        let n = 8;
        let mean = |amp: f32, f: &str| (0..n).map(|k| amp * waveform(f, (k as f32 + 0.5) / n as f32)).sum::<f32>() / n as f32;
        w.write(GamepadRumbleRequest::Add {
            duration: Duration::from_secs_f32(s.duration.max(0.01)),
            intensity: GamepadRumbleIntensity { strong_motor: (mean(s.left, &s.left_fn) * scale).clamp(0.0, 1.0), weak_motor: (mean(s.right, &s.right_fn) * scale).clamp(0.0, 1.0) },
            gamepad: pad,
        });
    }
}
