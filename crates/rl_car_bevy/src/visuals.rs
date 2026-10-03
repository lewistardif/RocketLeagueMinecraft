//! Optional real Rocket League car models, extracted from the user's own game install by
//! `tools/rl_assets/extract.py` into `assets/rl/` (git-ignored, never redistributed).
//!
//! When the extracted files are missing the demo keeps its procedural placeholder car.
//! The models use the same car-local frame as the rest of the adapter (+X forward, +Y up, +Z
//! right, metres), and their origin is the car's centre of mass, so they sit directly on the
//! physics state with no offset.

use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use rl_car_core::HitboxPreset;
use std::path::PathBuf;

/// Workspace `assets/` folder (Bevy's asset root for this demo).
pub fn asset_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Radius of the exported default wheel mesh (`WHEEL_Star_SM`), metres.
const WHEEL_MESH_RADIUS: f32 = 0.16313;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Team {
    #[default]
    Blue,
    Orange,
}

impl Team {
    pub fn name(self) -> &'static str {
        match self {
            Team::Blue => "blue",
            Team::Orange => "orange",
        }
    }
}

#[derive(Resource)]
pub struct CarVisuals {
    /// Extracted Rocket League models are present.
    pub real: bool,
    pub team: Team,
    /// Per preset: the model's wheel hub positions (FL, FR, BL, BR), car-local metres.
    anchors: Vec<(HitboxPreset, [Vec3; 4])>,
}

impl CarVisuals {
    pub fn detect() -> CarVisuals {
        let real = asset_root().join(body_path(HitboxPreset::Octane, Team::Blue)).is_file() && asset_root().join(WHEEL_PATH).is_file();
        if real {
            info!("using Rocket League models from {}", asset_root().join("rl").display());
        } else {
            info!("no extracted Rocket League models (see tools/rl_assets/extract.py); using the placeholder car");
        }
        let anchors = if real { HitboxPreset::ALL.iter().filter_map(|&p| Some((p, read_anchors(p)?))).collect() } else { Vec::new() };
        CarVisuals { real, team: Team::Blue, anchors }
    }

    /// Where the model wants this wheel (only x/z are used: the height comes from the suspension).
    pub fn wheel_anchor(&self, preset: HitboxPreset, front: bool, left: bool) -> Option<Vec3> {
        let (_, a) = self.anchors.iter().find(|(p, _)| *p == preset)?;
        Some(a[(!front as usize) * 2 + (!left) as usize])
    }
}

/// Parses `wheels.txt` (`FL x y z` per line) written by the extractor.
fn read_anchors(preset: HitboxPreset) -> Option<[Vec3; 4]> {
    let text = std::fs::read_to_string(asset_root().join(format!("rl/cars/{}/wheels.txt", preset.name()))).ok()?;
    let mut out = [Vec3::ZERO; 4];
    for (i, corner) in ["FL", "FR", "BL", "BR"].iter().enumerate() {
        let line = text.lines().find(|l| l.starts_with(corner))?;
        let v: Vec<f32> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
        out[i] = Vec3::new(*v.first()?, *v.get(1)?, *v.get(2)?);
    }
    Some(out)
}

const WHEEL_PATH: &str = "rl/wheel/wheel.gltf";

fn body_path(preset: HitboxPreset, team: Team) -> String {
    format!("rl/cars/{}/body_{}.gltf", preset.name(), team.name())
}

/// The spawned body model; replaced when the preset or team changes.
#[derive(Component)]
pub struct RealBody(pub HitboxPreset, pub Team);

pub fn body_bundle(assets: &AssetServer, preset: HitboxPreset, team: Team) -> impl Bundle {
    (RealBody(preset, team), WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(body_path(preset, team)))), Transform::default())
}

/// Child of a `Wheel` entity (whose uniform scale is the wheel radius in metres).
/// The mesh's outer face is +Z; left wheels are turned around so it faces outwards.
pub fn wheel_bundle(assets: &AssetServer, left: bool) -> impl Bundle {
    let turn = if left { Quat::from_rotation_y(std::f32::consts::PI) } else { Quat::IDENTITY };
    (
        WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(WHEEL_PATH))),
        Transform::from_rotation(turn).with_scale(Vec3::splat(1.0 / WHEEL_MESH_RADIUS)),
    )
}

/// Bevy does not build mip chains for PNG textures; without them the 2K car textures shimmer.
/// Generates them on the CPU (box filter) once, when an RGBA8 image finishes loading.
pub fn generate_mipmaps(mut events: MessageReader<AssetEvent<Image>>, mut images: ResMut<Assets<Image>>) {
    for ev in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = ev else { continue };
        let Some(image) = images.get(*id) else { continue };
        let desc = &image.texture_descriptor;
        let (w, h) = (desc.size.width, desc.size.height);
        let rgba8 = matches!(desc.format, TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm);
        if !rgba8 || desc.mip_level_count > 1 || desc.size.depth_or_array_layers != 1 || w < 2 || h < 2 || !w.is_power_of_two() || !h.is_power_of_two() {
            continue;
        }
        let Some(base) = image.data.clone() else { continue };
        let srgb = desc.format == TextureFormat::Rgba8UnormSrgb;
        let (data, levels) = build_mip_chain(base, w, h, srgb);
        let Some(mut image) = images.get_mut(*id) else { continue };
        image.data = Some(data);
        image.texture_descriptor.mip_level_count = levels;
    }
}

fn build_mip_chain(base: Vec<u8>, w: u32, h: u32, srgb: bool) -> (Vec<u8>, u32) {
    // Average in linear light for sRGB textures so mips don't darken.
    let to_lin: Vec<f32> = (0..256).map(|v| if srgb { srgb_to_linear(v as f32 / 255.0) } else { v as f32 / 255.0 }).collect();
    let mut out = base.clone();
    let mut prev = base;
    let (mut pw, mut ph, mut levels) = (w as usize, h as usize, 1u32);
    while pw > 1 || ph > 1 {
        let (nw, nh) = ((pw / 2).max(1), (ph / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut acc = 0.0;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(pw - 1);
                        let sy = (y * 2 + dy).min(ph - 1);
                        let v = prev[(sy * pw + sx) * 4 + c];
                        acc += if c == 3 { v as f32 / 255.0 } else { to_lin[v as usize] };
                    }
                    acc *= 0.25;
                    let v = if c == 3 || !srgb { acc } else { linear_to_srgb(acc) };
                    next[(y * nw + x) * 4 + c] = (v * 255.0 + 0.5) as u8;
                }
            }
        }
        out.extend_from_slice(&next);
        prev = next;
        pw = nw;
        ph = nh;
        levels += 1;
    }
    (out, levels)
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain_has_all_levels_and_preserves_flat_colour() {
        let base = [200u8, 100, 50, 255].repeat(8 * 4);
        let (data, levels) = build_mip_chain(base, 8, 4, true);
        assert_eq!(levels, 4); // 8x4, 4x2, 2x1, 1x1
        assert_eq!(data.len(), (32 + 8 + 2 + 1) * 4);
        assert_eq!(&data[data.len() - 4..], &[200, 100, 50, 255]);
    }
}
