//! Rocket League's boost meter, bottom right.
//!
//! The artwork comes from the extracted HUD (`assets/rl/hud/`, written by
//! `tools/rl_assets/hud.py`: the meter's textures, its two 101-frame fill timelines, its text fields
//! and glyph atlases of the game's fonts). The behaviour is `rl_car_core::boost_meter`, the port of
//! the meter's ActionScript. Each Scaleform layer of the clip is one `Mesh2d` drawn by an overlay
//! 2D camera in the clip's display order, with `shaders/boost_meter.wgsl` doing Flash's 3D tilt,
//! perspective and colour transforms.
//!
//! Scaleform blends in gamma space. To get the same soft glows, the meter is drawn into its own
//! HDR (float) target with gamma-space values, so its layers blend with each other exactly as in
//! the game, and that image is composited once over the scene by a full-window UI node
//! (`shaders/boost_meter_composite.wgsl`). Only that last step blends in linear light.
//!
//! Without the extracted HUD there is no meter (the text HUD still shows the boost amount).

use crate::visuals::asset_root;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::RenderTarget;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d};
use bevy::ui_render::prelude::{UiMaterial, UiMaterialPlugin};
use bevy::window::PrimaryWindow;
use rl_car_core::boost_meter::{self as bm, BoostMeterFrame, BoostMeterLayout, BoostMeterView, ColorTransform};
use serde::Deserialize;
use std::collections::HashMap;

const DATA_PATH: &str = "rl/hud/boost_meter.json";
const LAYER: usize = 1;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/boost_meter.wgsl");
        embedded_asset!(app, "shaders/boost_meter_composite.wgsl");
        app.add_plugins((Material2dPlugin::<MeterMaterial>::default(), UiMaterialPlugin::<CompositeMaterial>::default())).add_systems(Startup, setup);
    }
}

// ------------------------------------------------------------------------------------ data

#[derive(Deserialize)]
struct MeterFile {
    background: BitmapFile,
    glow: BitmapFile,
    fill: BarFile,
    fill_tinted: BarFile,
    texts: HashMap<String, TextFile>,
    fonts: HashMap<String, FontFile>,
}

#[derive(Deserialize)]
struct BitmapFile {
    texture: String,
    rect: [f32; 4],
    depth: u32,
}

#[derive(Deserialize)]
struct BarFile {
    texture: String,
    rect: [f32; 4],
    depth: u32,
    /// Per frame, the polygon clipping the bitmap (empty = nothing).
    frames: Vec<Vec<[f32; 2]>>,
}

#[derive(Deserialize, Clone)]
struct TextFile {
    depth: u32,
    origin: [f32; 2],
    /// xmin, xmax, ymin, ymax
    bounds: [f32; 4],
    font: String,
    size: f32,
    margins: [f32; 2],
}

#[derive(Deserialize)]
struct FontFile {
    atlas: String,
    atlas_size: [f32; 2],
    units_per_em: f32,
    ascent: f32,
    space_advance: f32,
    glyphs: HashMap<String, GlyphFile>,
}

#[derive(Deserialize)]
struct GlyphFile {
    advance: f32,
    plane: [f32; 4],
    uv: [f32; 4],
}

// ------------------------------------------------------------------------------------ material

#[derive(Clone, Copy, Default, ShaderType)]
struct MeterUniform {
    rotation_x: Vec4,
    rotation_y: Vec4,
    rotation_z: Vec4,
    position: Vec4,
    center: Vec4,
    mult: Vec4,
    add: Vec4,
    text_color: Vec4,
    params: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct MeterMaterial {
    #[uniform(0)]
    meter: MeterUniform,
    #[texture(1)]
    #[sampler(2)]
    texture: Handle<Image>,
}

impl Material2d for MeterMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://rl_car_bevy/shaders/boost_meter.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://rl_car_bevy/shaders/boost_meter.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Puts the meter's layer (premultiplied, gamma space) over the scene.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct CompositeMaterial {
    #[texture(0)]
    #[sampler(1)]
    layer: Handle<Image>,
}

impl UiMaterial for CompositeMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://rl_car_bevy/shaders/boost_meter_composite.wgsl".into()
    }
}

// ------------------------------------------------------------------------------------ entities

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Part {
    Background,
    Glow,
    FillTinted,
    Fill,
    BackgroundText,
    Label,
    BoostText,
}

#[derive(Component)]
pub struct MeterLayer(Part);

/// The meter's state and the meshes that change with it.
#[derive(Resource)]
pub struct BoostMeter {
    view: BoostMeterView,
    fill_frames: Vec<Handle<Mesh>>,
    tinted_frames: Vec<Handle<Mesh>>,
    numbers: Font,
    texts: HashMap<Part, TextFile>,
    shown_text: String,
    /// The meter's own render target (window sized).
    target: Handle<Image>,
}

struct Font {
    units_per_em: f32,
    ascent: f32,
    space_advance: f32,
    atlas_size: [f32; 2],
    glyphs: HashMap<char, GlyphFile>,
}

impl Font {
    fn from_file(f: FontFile) -> Font {
        Font {
            units_per_em: f.units_per_em,
            ascent: f.ascent,
            space_advance: f.space_advance,
            atlas_size: f.atlas_size,
            glyphs: f.glyphs.into_iter().filter_map(|(k, g)| k.chars().next().map(|c| (c, g))).collect(),
        }
    }

    fn advance(&self, c: char) -> f32 {
        self.glyphs.get(&c).map_or(self.space_advance, |g| g.advance)
    }
}

fn load_clamped(assets: &AssetServer, path: String) -> Handle<Image> {
    assets
        .load_builder()
        .with_settings(|s: &mut ImageLoaderSettings| {
            s.is_srgb = false; // Scaleform works on the stored (gamma) values
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::ClampToEdge,
                address_mode_v: ImageAddressMode::ClampToEdge,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                ..default()
            });
        })
        .load(path)
}

fn mesh(mut positions: Vec<[f32; 3]>, mut uvs: Vec<[f32; 2]>, mut colors: Vec<[f32; 4]>, mut indices: Vec<u32>) -> Mesh {
    if indices.is_empty() {
        // Nothing to draw (an empty frame or text). Bevy's mesh allocator wants some data.
        (positions, uvs, colors, indices) = (vec![[0.0; 3]; 3], vec![[0.0; 2]; 3], vec![[0.0; 4]; 3], vec![0, 1, 2]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices))
}

/// A polygon clipping a bitmap placed at `rect`, as a triangle fan around its vertex closest to
/// the meter's centre (the fill wedges are star-shaped around it).
fn polygon_mesh(poly: &[[f32; 2]], rect: [f32; 4], z: f32) -> Mesh {
    let n = poly.len();
    let center = (0..n).min_by(|&a, &b| {
        let d = |i: usize| poly[i][0] * poly[i][0] + poly[i][1] * poly[i][1];
        d(a).total_cmp(&d(b))
    });
    let positions = poly.iter().map(|p| [p[0], p[1], z]).collect();
    let uvs = poly.iter().map(|p| [(p[0] - rect[0]) / rect[2], (p[1] - rect[1]) / rect[3]]).collect();
    let mut indices = Vec::new();
    if let Some(c) = center {
        for k in 1..n.saturating_sub(1) {
            indices.extend([c as u32, ((c + k) % n) as u32, ((c + k + 1) % n) as u32]);
        }
    }
    mesh(positions, uvs, vec![[1.0; 4]; n], indices)
}

fn rect_poly(r: [f32; 4]) -> Vec<[f32; 2]> {
    vec![[r[0], r[1]], [r[0] + r[2], r[1]], [r[0] + r[2], r[1] + r[3]], [r[0], r[1] + r[3]]]
}

/// Lays out one line of a Flash text field (centred, 2 px gutter) as glyph quads: first the glow
/// quads (vertex colour 0) if `glow`, then the glyphs (1).
fn text_mesh(field: &TextFile, font: &Font, text: &str, z: f32, glow: bool) -> Mesh {
    let k = field.size / font.units_per_em;
    let [xmin, xmax, ymin, _] = field.bounds;
    let width: f32 = text.chars().map(|c| font.advance(c)).sum::<f32>() * k;
    let inner = xmax - xmin - 4.0 - field.margins[0] - field.margins[1];
    let left = field.origin[0] + xmin + 2.0 + field.margins[0] + (inner - width) * 0.5;
    let baseline = field.origin[1] + ymin + 2.0 + font.ascent * k;
    let (mut pos, mut uv, mut col, mut idx) = (vec![], vec![], vec![], vec![]);
    for pass in [0.0, 1.0] {
        if pass == 0.0 && !glow {
            continue;
        }
        let mut x = left;
        for c in text.chars() {
            if let Some(g) = font.glyphs.get(&c) {
                let base = pos.len() as u32;
                let (x0, y0, x1, y1) = (x + g.plane[0] * k, baseline + g.plane[1] * k, x + g.plane[2] * k, baseline + g.plane[3] * k);
                let [w, h] = font.atlas_size;
                let (u0, v0, u1, v1) = (g.uv[0] / w, g.uv[1] / h, g.uv[2] / w, g.uv[3] / h);
                pos.extend([[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]]);
                uv.extend([[u0, v0], [u1, v0], [u1, v1], [u0, v1]]);
                col.extend([[pass, pass, pass, 1.0]; 4]);
                idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            x += font.advance(c) * k;
        }
    }
    mesh(pos, uv, col, idx)
}

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MeterMaterial>>,
    mut composites: ResMut<Assets<CompositeMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let path = asset_root().join(DATA_PATH);
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let file: MeterFile = match serde_json::from_str(&text) {
        Ok(f) => f,
        Err(e) => {
            warn!("cannot read {}: {e}; no boost meter", path.display());
            return;
        }
    };
    let tex = |name: &str| load_clamped(&assets, format!("rl/hud/{name}"));
    let field = |name: &str| file.texts.get(name).cloned();
    let (Some(bg_text), Some(label), Some(boost_text)) = (field("backgroundTextField"), field("boostLabel"), field("boostTextField")) else {
        warn!("{}: missing text fields; no boost meter", path.display());
        return;
    };
    let mut fonts = file.fonts;
    let (Some(numbers), Some(header)) = (fonts.remove(&boost_text.font), fonts.remove(&label.font)) else {
        warn!("{}: missing fonts; no boost meter", path.display());
        return;
    };
    let numbers_atlas = tex(&numbers.atlas);
    let header_atlas = tex(&header.atlas);

    // The meter's camera: an HDR target holding gamma-space values (no tonemapping, no dithering),
    // resized with the window in `update`.
    let target = images.add(Image::new_target_texture(16, 16, TextureFormat::Rgba16Float, None));
    commands.spawn((
        Camera2d,
        Camera { order: -1, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
        RenderTarget::Image(target.clone().into()),
        bevy::camera::Hdr,
        Tonemapping::None,
        DebandDither::Disabled,
        RenderLayers::layer(LAYER),
    ));
    commands.spawn((
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        MaterialNode(composites.add(CompositeMaterial { layer: target.clone() })),
        GlobalZIndex(-1),
    ));

    let front = bm::FRONT_Z;
    let mut spawn = |part: Part, depth: u32, mesh: Handle<Mesh>, texture: Handle<Image>| {
        let material = materials.add(MeterMaterial { meter: MeterUniform::default(), texture });
        // 2D transparent meshes are drawn back to front by z: the clip's depth order.
        commands.spawn((MeterLayer(part), Mesh2d(mesh), MeshMaterial2d(material), Transform::from_xyz(0.0, 0.0, depth as f32), NoFrustumCulling, RenderLayers::layer(LAYER), Visibility::Hidden));
    };
    let b = &file.background;
    spawn(Part::Background, b.depth, meshes.add(polygon_mesh(&rect_poly(b.rect), b.rect, 0.0)), tex(&b.texture));
    let g = &file.glow;
    spawn(Part::Glow, g.depth, meshes.add(polygon_mesh(&rect_poly(g.rect), g.rect, 0.0)), tex(&g.texture));
    let bar = |bar: &BarFile, meshes: &mut Assets<Mesh>| -> Vec<Handle<Mesh>> { bar.frames.iter().map(|f| meshes.add(polygon_mesh(f, bar.rect, front))).collect() };
    let tinted_frames = bar(&file.fill_tinted, &mut meshes);
    let fill_frames = bar(&file.fill, &mut meshes);
    spawn(Part::FillTinted, file.fill_tinted.depth, tinted_frames[0].clone(), tex(&file.fill_tinted.texture));
    spawn(Part::Fill, file.fill.depth, fill_frames[0].clone(), tex(&file.fill.texture));
    spawn(Part::BackgroundText, bg_text.depth, meshes.add(mesh(vec![], vec![], vec![], vec![])), numbers_atlas.clone());
    let header = Font::from_file(header);
    spawn(Part::Label, label.depth, meshes.add(text_mesh(&label, &header, bm::LABEL_TEXT, 0.0, false)), header_atlas);
    spawn(Part::BoostText, boost_text.depth, meshes.add(mesh(vec![], vec![], vec![], vec![])), numbers_atlas);

    commands.insert_resource(BoostMeter {
        view: BoostMeterView::new(),
        fill_frames,
        tinted_frames,
        numbers: Font::from_file(numbers),
        texts: HashMap::from([(Part::BackgroundText, bg_text), (Part::Label, label), (Part::BoostText, boost_text)]),
        shown_text: String::new(),
        target,
    });
}

// ------------------------------------------------------------------------------------ update

/// Advances the meter with the car's boost and updates its layers.
pub fn update(
    meter: Option<ResMut<BoostMeter>>,
    sim: Res<crate::Sim>,
    time: Res<Time>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut layers: Query<(&MeterLayer, &mut Mesh2d, &MeshMaterial2d<MeterMaterial>, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MeterMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut meter) = meter else { return };
    let Ok(window) = window.single() else { return };
    let size = window.physical_size().as_vec2();
    if size.x < 1.0 || size.y < 1.0 {
        return;
    }
    let extent = Extent3d { width: size.x as u32, height: size.y as u32, depth_or_array_layers: 1 };
    if images.get(&meter.target).is_some_and(|i| i.texture_descriptor.size != extent)
        && let Some(mut image) = images.get_mut(&meter.target)
    {
        image.resize(extent);
    }
    let frame = meter.view.update(sim.stepper.current.boost_amount, time.delta_secs());
    let layout = BoostMeterLayout::new(size.x, size.y, 1.0);
    let text = frame.text().to_string();
    let text_changed = text != meter.shown_text;
    meter.shown_text = text.clone();

    let (sx, cx) = bm::ROTATION_DEG[0].to_radians().sin_cos();
    let (sy, cy) = bm::ROTATION_DEG[1].to_radians().sin_cos();
    // R = Ry * Rx (rows).
    let rot = [Vec4::new(cy, sy * sx, sy * cx, 0.0), Vec4::new(0.0, cx, -sx, 0.0), Vec4::new(-sy, cy * sx, cy * cx, 0.0)];
    let base = MeterUniform {
        rotation_x: rot[0],
        rotation_y: rot[1],
        rotation_z: rot[2],
        position: Vec4::new(layout.position[0], layout.position[1], layout.scale, bm::FOCAL_LENGTH),
        center: Vec4::new(layout.position[0] + bm::PROJECTION_CENTER_OFFSET[0], layout.position[1] + bm::PROJECTION_CENTER_OFFSET[1], size.x, size.y),
        params: Vec4::new(1.0, 1.0, 0.0, 0.0),
        ..default()
    };
    let glow_channel = if frame.text_glow_blur > bm::TEXT_GLOW_NORMAL { 2.0 } else { 1.0 };

    for (layer, mut mesh2d, material, mut vis) in &mut layers {
        *vis = if frame.visible { Visibility::Visible } else { Visibility::Hidden };
        let (ct, text_color, scale) = layer_look(layer.0, &frame);
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.meter = MeterUniform {
                mult: Vec4::from_array(ct.mult),
                add: Vec4::from_array(ct.add) / 255.0,
                text_color: text_color.map_or(Vec4::ZERO, |c| Vec4::new(c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, 1.0)),
                params: Vec4::new(scale, glow_channel, 0.0, 0.0),
                ..base
            };
        }
        let i = frame.fill_frame as usize - 1;
        match layer.0 {
            Part::Fill => mesh2d.0 = meter.fill_frames[i.min(meter.fill_frames.len() - 1)].clone(),
            Part::FillTinted => mesh2d.0 = meter.tinted_frames[i.min(meter.tinted_frames.len() - 1)].clone(),
            Part::BackgroundText | Part::BoostText if text_changed => {
                let field = &meter.texts[&layer.0];
                let (z, glow) = if layer.0 == Part::BoostText { (bm::FRONT_Z, true) } else { (0.0, false) };
                let m = text_mesh(field, &meter.numbers, &text, z, glow);
                if let Some(mut slot) = meshes.get_mut(&mesh2d.0) {
                    *slot = m;
                }
            }
            _ => {}
        }
    }
}

/// A layer's colour transform, text colour (text layers) and scale about the meter's origin.
fn layer_look(part: Part, f: &BoostMeterFrame) -> (ColorTransform, Option<[f32; 3]>, f32) {
    match part {
        Part::Background => (f.background, None, 1.0),
        Part::Glow => (f.glow, None, f.glow_scale),
        Part::FillTinted => (f.fill_tinted, None, 1.0),
        Part::Fill => (f.fill, None, 1.0),
        Part::BackgroundText => (f.background_text, Some(bm::BACKGROUND_TEXT_COLOR), 1.0),
        Part::Label => (f.label, Some(bm::LABEL_TEXT_COLOR), 1.0),
        Part::BoostText => (f.boost_text, Some(bm::BOOST_TEXT_COLOR), 1.0),
    }
}
