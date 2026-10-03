//! Boost visuals.
//!
//! With the extracted Rocket League assets (`assets/rl/boost/`, written by
//! `tools/rl_assets/extract.py`), the real cars show the game's default boost ("Standard"):
//! - the flame cones, placed per car body as the game places them, drawn with a port of the boost
//!   material's compiled pixel shader (`shaders/boost_flame.wgsl`);
//! - the smoke trail (`Boost_Painted_PS`) while boosting and the small exhaust puffs (`Drive_PS`)
//!   while only throttling, simulated on the CPU from the extracted particle module data and drawn
//!   with a port of `SmokePuff_Mat` (`shaders/boost_smoke.wgsl`).
//!
//! Without them (or on the placeholder box car) a simple flickering flame cone is shown instead.
//!
//! Particles live in Bevy world space (metres); the extracted data is in Unreal units (uu, Z up)
//! and converted where it is used.

use crate::convert::UU_PER_M;
use crate::visuals::{CarVisuals, asset_root};
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use rl_car_core::HitboxPreset;
use serde::Deserialize;
use std::collections::HashMap;

const DATA_PATH: &str = "rl/boost/boost.json";

pub struct BoostPlugin;

impl Plugin for BoostPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/boost_flame.wgsl");
        embedded_asset!(app, "shaders/boost_smoke.wgsl");
        app.add_plugins((MaterialPlugin::<FlameMaterial>::default(), MaterialPlugin::<SmokeMaterial>::default()))
            .insert_resource(BoostData::load())
            .init_resource::<BoostEmitters>()
            .add_systems(Startup, setup);
    }
}

// ------------------------------------------------------------------------------------ data

#[derive(Deserialize)]
struct BoostFile {
    flame: FlameFile,
    smoke: SmokeFile,
    emitters: EmittersFile,
    cars: HashMap<String, CarFile>,
}

#[derive(Deserialize)]
struct FlameFile {
    params: HashMap<String, serde_json::Value>,
    textures: HashMap<String, String>,
}

#[derive(Deserialize)]
struct SmokeFile {
    textures: HashMap<String, String>,
}

#[derive(Deserialize)]
struct EmittersFile {
    boost: EmitterDef,
    drive: EmitterDef,
}

#[derive(Deserialize)]
struct CarFile {
    cones: String,
    cone_delay: f32,
    /// Boost sockets, car model space (m).
    emitters: Vec<[f32; 3]>,
}

/// One Cascade sprite emitter, as far as the boost's two systems use it (units: uu, s, UE axes).
#[derive(Deserialize, Clone)]
pub struct EmitterDef {
    local_space: bool,
    subuv: [u32; 2],
    spawn_rate: Option<SpawnRate>,
    spawn_per_unit: Option<SpawnPerUnit>,
    lifetime: Dist,
    size: Dist,
    size_life: Option<Dist>,
    velocity: Option<Dist>,
    vel_life: Option<Dist>,
    accel: Option<Dist>,
    rotation: Option<Dist>,
    color: Dist,
    alpha: Dist,
    color_life: Option<Dist>,
    alpha_life: Option<Dist>,
}

#[derive(Deserialize, Clone)]
struct SpawnRate {
    rate: Dist,
    scale: Dist,
}

#[derive(Deserialize, Clone)]
struct SpawnPerUnit {
    unit: f32,
    count: Dist,
    max_frame_distance: f32,
    movement_tolerance: f32,
}

/// A cooked UE3 distribution: a lookup table sampled like `FRawDistribution::GetValue`, or a
/// uniform random range (a particle parameter the boost sets).
#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum Dist {
    Table { table: Vec<f32>, random: bool, chunk: usize, time_scale: f32, start_time: f32, dim: usize },
    Range { min: Vec<f32>, max: Vec<f32> },
}

impl Dist {
    fn sample(&self, time: f32, rng: &mut Rng) -> [f32; 3] {
        let mut out = [0.0; 3];
        match self {
            Dist::Table { table, random, chunk, time_scale, start_time, dim } => {
                let entries = (table.len() / chunk).max(1);
                let index = ((time - start_time) * time_scale).max(0.0);
                let i = index as usize;
                let alpha = index - i as f32;
                let (e1, e2) = (i.min(entries - 1) * chunk, (i + 1).min(entries - 1) * chunk);
                for c in 0..*dim {
                    let lo = table[e1 + c] + (table[e2 + c] - table[e1 + c]) * alpha;
                    out[c] = if *random {
                        let hi = table[e1 + dim + c] + (table[e2 + dim + c] - table[e1 + dim + c]) * alpha;
                        lo + (hi - lo) * rng.next()
                    } else {
                        lo
                    };
                }
            }
            Dist::Range { min, max } => {
                for c in 0..min.len().min(3) {
                    out[c] = min[c] + (max[c] - min[c]) * rng.next();
                }
            }
        }
        out
    }

    fn scalar(&self, time: f32, rng: &mut Rng) -> f32 {
        self.sample(time, rng)[0]
    }
}

fn sample_or(d: &Option<Dist>, time: f32, rng: &mut Rng, default: f32) -> [f32; 3] {
    d.as_ref().map_or([default; 3], |d| d.sample(time, rng))
}

/// UE world/local direction or offset (uu) -> Bevy (m).
fn ue_vec(v: [f32; 3]) -> Vec3 {
    Vec3::new(v[0], v[2], v[1]) / UU_PER_M
}

struct CarBoost {
    cones: String,
    cone_delay: f32,
    sockets: Vec<Vec3>,
}

/// The extracted boost, if present.
#[derive(Resource, Default)]
pub struct BoostData {
    flame_params: FlameUniform,
    flame_textures: [String; 2],
    smoke_textures: [String; 3],
    boost: Option<EmitterDef>,
    drive: Option<EmitterDef>,
    cars: HashMap<String, CarBoost>,
}

impl BoostData {
    fn load() -> BoostData {
        let path = asset_root().join(DATA_PATH);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return BoostData::default();
        };
        let file: BoostFile = match serde_json::from_str(&text) {
            Ok(f) => f,
            Err(e) => {
                warn!("cannot read {}: {e}; using the simple boost flame", path.display());
                return BoostData::default();
            }
        };
        let p = |k: &str| file.flame.params.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let color: Vec<f32> = file.flame.params.get("CustomColor").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
        let brightness = p("Brightness");
        let tex = |t: &HashMap<String, String>, k: &str| format!("rl/boost/{}", t.get(k).cloned().unwrap_or_default());
        BoostData {
            flame_params: FlameUniform {
                color: Vec4::new(color.first().copied().unwrap_or(1.0) * brightness, color.get(1).copied().unwrap_or(1.0) * brightness, color.get(2).copied().unwrap_or(1.0) * brightness, 1.0),
                speed_tile: Vec4::new(p("Inner_Speed"), p("Outer_Speed"), p("TileX"), p("TileY")),
                sparks_gradient: Vec4::new(p("Inner_Sparks"), p("Outer_Sparks"), p("GradientAmount"), p("GradientSharpness")),
                fresnel_opacity: Vec4::new(p("FresnelBase"), p("FresnelEnd"), p("Opacity"), 0.0),
            },
            flame_textures: [tex(&file.flame.textures, "noise"), tex(&file.flame.textures, "sparks")],
            smoke_textures: [tex(&file.smoke.textures, "smoke"), tex(&file.smoke.textures, "gradient"), tex(&file.smoke.textures, "radial")],
            boost: Some(file.emitters.boost),
            drive: Some(file.emitters.drive),
            cars: file
                .cars
                .into_iter()
                .map(|(k, c)| (k, CarBoost { cones: format!("rl/boost/{}", c.cones), cone_delay: c.cone_delay, sockets: c.emitters.iter().map(|s| Vec3::from_array(*s)).collect() }))
                .collect(),
        }
    }

    /// The game's boost is available for this car.
    pub fn has(&self, preset: HitboxPreset) -> bool {
        self.boost.is_some() && self.cars.contains_key(preset.name())
    }
}

// ------------------------------------------------------------------------------------ materials

#[derive(Clone, Copy, Default, ShaderType)]
struct FlameUniform {
    color: Vec4,
    speed_tile: Vec4,
    sparks_gradient: Vec4,
    fresnel_opacity: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct FlameMaterial {
    #[uniform(0)]
    params: FlameUniform,
    #[texture(1)]
    #[sampler(2)]
    noise: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    sparks: Handle<Image>,
}

impl Material for FlameMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://rl_car_bevy/shaders/boost_flame.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        // Premultiplied blending with alpha 0 is One/One, the game's additive blend.
        AlphaMode::Add
    }

    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None; // two-sided
        Ok(())
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SmokeMaterial {
    #[texture(0)]
    #[sampler(1)]
    smoke: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    gradient: Handle<Image>,
    #[texture(4)]
    #[sampler(5)]
    radial: Handle<Image>,
}

impl Material for SmokeMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://rl_car_bevy/shaders/boost_smoke.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// The textures tile (the shaders scale and scroll their UVs), with trilinear filtering.
fn load_tiling(assets: &AssetServer, path: &str) -> Handle<Image> {
    assets.load_builder().with_settings(|s: &mut ImageLoaderSettings| {
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

// ------------------------------------------------------------------------------------ entities

/// The flame cones of the current real car (child of the car root, model space).
#[derive(Component)]
pub struct BoostCones(pub HitboxPreset);

/// The smoke of every boost emitter, rebuilt each frame (world space).
#[derive(Component)]
struct SmokeMesh;

#[derive(Resource, Default)]
pub struct BoostHandles {
    flame: Option<Handle<FlameMaterial>>,
    smoke_mesh: Option<Handle<Mesh>>,
}

fn setup(mut commands: Commands, data: Res<BoostData>, assets: Res<AssetServer>, mut flames: ResMut<Assets<FlameMaterial>>, mut smokes: ResMut<Assets<SmokeMaterial>>, mut meshes: ResMut<Assets<Mesh>>) {
    if data.boost.is_none() {
        commands.insert_resource(BoostHandles::default());
        return;
    }
    let flame = flames.add(FlameMaterial { params: data.flame_params, noise: load_tiling(&assets, &data.flame_textures[0]), sparks: load_tiling(&assets, &data.flame_textures[1]) });
    let smoke = smokes.add(SmokeMaterial {
        smoke: load_tiling(&assets, &data.smoke_textures[0]),
        gradient: load_tiling(&assets, &data.smoke_textures[1]),
        radial: load_tiling(&assets, &data.smoke_textures[2]),
    });
    let mesh = meshes.add(empty_smoke_mesh());
    commands.spawn((SmokeMesh, Mesh3d(mesh.clone()), MeshMaterial3d(smoke), Transform::default(), NoFrustumCulling, NotShadowCaster));
    commands.insert_resource(BoostHandles { flame: Some(flame), smoke_mesh: Some(mesh) });
}

/// The flame cones for a real car, or None when the boost was not extracted.
fn cones_bundle(data: &BoostData, handles: &BoostHandles, assets: &AssetServer, preset: HitboxPreset) -> Option<impl Bundle> {
    let car = data.cars.get(preset.name())?;
    let flame = handles.flame.clone()?;
    let mesh = assets.load(GltfAssetLabel::Primitive { mesh: 0, primitive: 0 }.from_asset(car.cones.clone()));
    Some((BoostCones(preset), Mesh3d(mesh), MeshMaterial3d(flame), Transform::default(), Visibility::Hidden, NotShadowCaster))
}

/// The smoke mesh is never empty (Bevy's mesh allocator does not like zero-sized meshes): with no
/// particles it holds one degenerate, transparent triangle.
const NO_SMOKE: usize = 3;

fn empty_smoke_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; NO_SMOKE]);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; NO_SMOKE]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; NO_SMOKE]);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; NO_SMOKE]);
    m.insert_indices(Indices::U32((0..NO_SMOKE as u32).collect()));
    m
}

// ------------------------------------------------------------------------------------ particles

/// Small xorshift generator (particles only need cheap uniform randoms).
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

impl Default for Rng {
    fn default() -> Self {
        Rng(0x9e37_79b9)
    }
}

struct Particle {
    /// World position (world-space emitters) or offset from the socket in car space (local ones), m.
    pos: Vec3,
    base_vel: Vec3,
    accel: Vec3,
    size: f32,
    /// Relative time 0..1 and its rate.
    rel: f32,
    inv_life: f32,
    color: Vec3,
    alpha: f32,
    rotation: f32,
    cell: u32,
}

#[derive(Default)]
struct Emitter {
    particles: Vec<Particle>,
    /// Last world position of the socket while active (spawn per unit).
    last: Option<Vec3>,
    travelled: f32,
    spawn_acc: f32,
}

#[derive(Resource, Default)]
pub struct BoostEmitters {
    /// Per socket: (boost trail, drive puffs).
    sockets: Vec<(Emitter, Emitter)>,
    preset: Option<HitboxPreset>,
    rng: Rng,
    boost_time: f32,
}

impl Emitter {
    fn spawn(&mut self, def: &EmitterDef, rng: &mut Rng, pos: Vec3, age: f32) {
        let life = def.lifetime.scalar(0.0, rng).max(1e-3);
        let size = def.size.sample(0.0, rng)[0] / UU_PER_M;
        let vel = def.velocity.as_ref().map_or(Vec3::ZERO, |d| ue_vec(d.sample(0.0, rng)));
        let accel = def.accel.as_ref().map_or(Vec3::ZERO, |d| ue_vec(d.sample(0.0, rng)));
        let color = Vec3::from_array(def.color.sample(0.0, rng));
        let alpha = def.alpha.scalar(0.0, rng);
        let rotation = def.rotation.as_ref().map_or(0.0, |d| d.scalar(0.0, rng) * std::f32::consts::TAU);
        let cells = def.subuv[0] * def.subuv[1];
        let cell = ((rng.next() * cells as f32) as u32).min(cells.saturating_sub(1));
        let mut p = Particle { pos, base_vel: vel, accel, size, rel: 0.0, inv_life: 1.0 / life, color, alpha, rotation, cell };
        p.tick(def, rng, age);
        self.particles.push(p);
    }

    /// One frame: age and move the particles, then spawn. `socket` is the emitter's world position.
    fn update(&mut self, def: &EmitterDef, rng: &mut Rng, dt: f32, active: bool, socket: Vec3) {
        for p in &mut self.particles {
            p.tick(def, rng, dt);
        }
        self.particles.retain(|p| p.rel < 1.0);
        if !active {
            self.last = None;
            self.travelled = 0.0;
            self.spawn_acc = 0.0;
            return;
        }
        if let Some(rate) = &def.spawn_rate {
            self.spawn_acc += rate.rate.scalar(0.0, rng) * rate.scale.scalar(0.0, rng) * dt;
            while self.spawn_acc >= 1.0 {
                self.spawn_acc -= 1.0;
                let age = self.spawn_acc / (rate.rate.scalar(0.0, rng).max(1e-3));
                let at = if def.local_space { Vec3::ZERO } else { socket };
                self.spawn(def, rng, at, age.min(dt));
            }
        }
        if let Some(spu) = &def.spawn_per_unit {
            // ParticleModuleSpawnPerUnit: particles per `unit` uu travelled, spread along the path.
            if let Some(last) = self.last {
                let travel = (socket - last).length() * UU_PER_M;
                if spu.max_frame_distance > 0.0 && travel > spu.max_frame_distance {
                    self.travelled = 0.0;
                } else if travel > spu.movement_tolerance * spu.unit {
                    let per_unit = spu.count.scalar(0.0, rng);
                    let total = travel + self.travelled;
                    let count = (total * per_unit / spu.unit).floor();
                    self.travelled = total - count * spu.unit / per_unit.max(1e-3);
                    let n = count as usize;
                    for k in 0..n {
                        let f = (k + 1) as f32 / n as f32;
                        self.spawn(def, rng, last.lerp(socket, f), dt * (1.0 - f));
                    }
                }
            }
            self.last = Some(socket);
        }
    }
}

impl Particle {
    /// Cascade's per-frame update: velocity reset to the base velocity, acceleration added to both,
    /// velocity scaled over life, then moved.
    fn tick(&mut self, def: &EmitterDef, rng: &mut Rng, dt: f32) {
        self.rel += dt * self.inv_life;
        self.base_vel += self.accel * dt;
        let scale = Vec3::from_array(sample_or(&def.vel_life, self.rel, rng, 1.0));
        // The scale is per UE axis; Bevy swaps Y and Z.
        let vel = self.base_vel * Vec3::new(scale[0], scale[2], scale[1]);
        self.pos += vel * dt;
    }
}

pub fn update(
    time: Res<Time>,
    sim: Res<crate::Sim>,
    data: Res<BoostData>,
    visuals: Res<CarVisuals>,
    handles: Res<BoostHandles>,
    mut emitters: ResMut<BoostEmitters>,
    car: Query<&Transform, With<crate::CarRoot>>,
    camera: Query<&Transform, (With<Camera3d>, Without<crate::CarRoot>)>,
    mut cones: Query<&mut Visibility, With<BoostCones>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Ok(car), Ok(camera)) = (car.single(), camera.single()) else { return };
    let dt = time.delta_secs().min(0.1);
    let s = &sim.stepper.current;
    let boosting = s.is_boosting;
    let throttling = sim.last_input.throttle != 0.0 && !boosting;
    let emitters = &mut *emitters;
    emitters.boost_time = if boosting { emitters.boost_time + dt } else { 0.0 };

    let car_data = if visuals.real { data.cars.get(sim.preset.name()) } else { None };
    for mut v in cones.iter_mut() {
        let show = boosting && car_data.is_some_and(|c| emitters.boost_time >= c.cone_delay);
        *v = if show { Visibility::Inherited } else { Visibility::Hidden };
    }

    let (Some(boost_def), Some(drive_def), Some(mesh)) = (&data.boost, &data.drive, &handles.smoke_mesh) else { return };
    let sockets: &[Vec3] = car_data.map_or(&[], |c| &c.sockets);
    if emitters.preset != Some(sim.preset) || emitters.sockets.len() != sockets.len() {
        emitters.preset = Some(sim.preset);
        emitters.sockets = sockets.iter().map(|_| Default::default()).collect();
    }
    let rng = &mut emitters.rng;
    for (socket, (trail, drive)) in sockets.iter().zip(emitters.sockets.iter_mut()) {
        let world = car.transform_point(*socket);
        trail.update(boost_def, rng, dt, boosting, world);
        drive.update(drive_def, rng, dt, throttling, world);
    }

    // Camera-facing quads, sorted back to front (the game sorts this emitter by view distance).
    let (right, up) = (camera.rotation * Vec3::X, camera.rotation * Vec3::Y);
    let mut quads: Vec<(f32, Vec3, &Particle, &EmitterDef)> = Vec::new();
    for (socket, (trail, drive)) in sockets.iter().zip(emitters.sockets.iter()) {
        for p in &trail.particles {
            quads.push((0.0, p.pos, p, boost_def));
        }
        for p in &drive.particles {
            // Local space: the puffs move with the car.
            quads.push((0.0, car.transform_point(*socket + p.pos), p, drive_def));
        }
    }
    for q in &mut quads {
        q.0 = q.1.distance_squared(camera.translation);
    }
    quads.sort_by(|a, b| b.0.total_cmp(&a.0));

    let n = quads.len();
    let (mut pos, mut uv, mut col, mut idx) = (Vec::with_capacity(n * 4), Vec::with_capacity(n * 4), Vec::with_capacity(n * 4), Vec::with_capacity(n * 6));
    let mut rng = Rng(emitters.rng.0 ^ 0x5bd1_e995);
    for (_, center, p, def) in quads {
        let size = p.size * sample_or(&def.size_life, p.rel, &mut rng, 1.0)[0];
        let c = Vec3::from_array(sample_or(&def.color_life, p.rel, &mut rng, 1.0)) * p.color;
        let a = p.alpha * sample_or(&def.alpha_life, p.rel, &mut rng, 1.0)[0];
        let (sin, cos) = p.rotation.sin_cos();
        let (r, u) = ((right * cos + up * sin) * size * 0.5, (up * cos - right * sin) * size * 0.5);
        let (cols, rows) = (def.subuv[0].max(1), def.subuv[1].max(1));
        let (u0, v0) = ((p.cell % cols) as f32 / cols as f32, (p.cell / cols) as f32 / rows as f32);
        let (du, dv) = (1.0 / cols as f32, 1.0 / rows as f32);
        let base = pos.len() as u32;
        for (corner, t) in [(center - r + u, [u0, v0]), (center + r + u, [u0 + du, v0]), (center + r - u, [u0 + du, v0 + dv]), (center - r - u, [u0, v0 + dv])] {
            pos.push(corner.to_array());
            uv.push(t);
            col.push([c.x, c.y, c.z, a]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    if pos.is_empty() {
        pos.resize(NO_SMOKE, [0.0; 3]);
        uv.resize(NO_SMOKE, [0.0; 2]);
        col.resize(NO_SMOKE, [0.0; 4]);
        idx.extend(0..NO_SMOKE as u32);
    }
    if let Some(mut m) = meshes.get_mut(mesh) {
        let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        m.insert_indices(Indices::U32(idx));
    }
}

/// Spawns or replaces the real car's cones when the car (or its visuals) changed.
pub fn sync_cones(
    mut commands: Commands,
    sim: Res<crate::Sim>,
    data: Res<BoostData>,
    visuals: Res<CarVisuals>,
    handles: Res<BoostHandles>,
    assets: Res<AssetServer>,
    root: Query<Entity, With<crate::CarRoot>>,
    cones: Query<(Entity, &BoostCones)>,
) {
    let Ok(root) = root.single() else { return };
    let wanted = visuals.real && data.has(sim.preset);
    let mut have = false;
    for (e, c) in cones.iter() {
        if wanted && c.0 == sim.preset {
            have = true;
        } else {
            commands.entity(e).despawn();
        }
    }
    if wanted
        && !have
        && let Some(bundle) = cones_bundle(&data, &handles, &assets, sim.preset)
    {
        commands.entity(root).with_child(bundle);
    }
}
