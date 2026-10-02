//! A drivable arena built from static avian colliders (Bevy space, metres, Y up).
//!
//! Soccar-sized box (8192 x 10240 x 2044 uu) with curved quarter-pipes joining the floor to the
//! walls and the walls to the ceiling, so you can drive up the walls and onto the ceiling, plus
//! a few ramps in the middle. Curves are made of convex planks, which keeps every collider convex
//! (robust raycasts and contacts). No Rocket League assets are used.

use crate::collision::ArenaCollider;
use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Half extents of the inner arena (m): x = RL x, z = RL y.
pub const HALF_X: f32 = 40.96;
pub const HALF_Z: f32 = 51.20;
pub const HEIGHT: f32 = 20.44;
const PIPE_RADIUS: f32 = 3.2;
const PIPE_SEGMENTS: usize = 12;
const THICK: f32 = 1.0;

fn checker_texture(images: &mut Assets<Image>, a: [u8; 4], b: [u8; 4]) -> Handle<Image> {
    const N: usize = 64;
    let mut data = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            let line = x < 2 || y < 2;
            let c = if line { [b[0] / 2 + 100, b[1] / 2 + 100, b[2] / 2 + 100, 255] } else if (x / 32 + y / 32) % 2 == 0 { a } else { b };
            data.extend_from_slice(&c);
        }
    }
    let mut img = Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    images.add(img)
}

/// Which material a slab is drawn with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlabKind {
    Floor,
    Wall,
    Pipe,
    Ramp,
}

/// One static box of the arena (Bevy space, metres).
#[derive(Clone, Copy, Debug)]
pub struct Slab {
    pub size: Vec3,
    pub transform: Transform,
    pub kind: SlabKind,
}

/// A curved quarter-pipe made of planks. `center` is the arc centre (inside the arena), `a` and
/// `b` are the unit directions from the centre to the arc's two ends, `axis` the direction the
/// pipe runs along, `length` its length.
fn quarter_pipe(out: &mut Vec<Slab>, center: Vec3, a: Vec3, b: Vec3, axis: Vec3, length: f32) {
    let r = PIPE_RADIUS;
    for i in 0..PIPE_SEGMENTS {
        let t0 = i as f32 / PIPE_SEGMENTS as f32 * std::f32::consts::FRAC_PI_2;
        let t1 = (i + 1) as f32 / PIPE_SEGMENTS as f32 * std::f32::consts::FRAC_PI_2;
        let p0 = center + (a * t0.cos() + b * t0.sin()) * r;
        let p1 = center + (a * t1.cos() + b * t1.sin()) * r;
        let chord = p1 - p0;
        let tangent = chord.normalize();
        let mid = (p0 + p1) * 0.5;
        // Outward normal (away from the arc centre).
        let outward = (mid - center).normalize();
        // Proper (right-handed) basis; the plank's length runs along +-axis.
        let along = tangent.cross(outward).normalize();
        debug_assert!(along.dot(axis.normalize()).abs() > 0.99);
        let rot = Quat::from_mat3(&Mat3::from_cols(tangent, outward, along));
        // Inner face lies on the chord; the plank extends outwards.
        let pos = mid + outward * (THICK * 0.5);
        out.push(Slab {
            size: Vec3::new(chord.length() + 0.02, THICK, length),
            transform: Transform::from_translation(pos).with_rotation(rot),
            kind: SlabKind::Pipe,
        });
    }
}

/// The full arena as a list of convex boxes.
pub fn layout() -> Vec<Slab> {
    let mut v = Vec::new();
    let (hx, hz, h) = (HALF_X, HALF_Z, HEIGHT);
    let r = PIPE_RADIUS;
    let slab = |size: Vec3, t: Transform, kind: SlabKind| Slab { size, transform: t, kind };

    // Floor and ceiling.
    v.push(slab(Vec3::new(2.0 * hx + 4.0, THICK, 2.0 * hz + 4.0), Transform::from_xyz(0.0, -THICK * 0.5, 0.0), SlabKind::Floor));
    v.push(slab(Vec3::new(2.0 * hx + 4.0, THICK, 2.0 * hz + 4.0), Transform::from_xyz(0.0, h + THICK * 0.5, 0.0), SlabKind::Wall));
    // Side walls (x) and back walls (z).
    v.push(slab(Vec3::new(THICK, h, 2.0 * hz), Transform::from_xyz(hx + THICK * 0.5, h * 0.5, 0.0), SlabKind::Wall));
    v.push(slab(Vec3::new(THICK, h, 2.0 * hz), Transform::from_xyz(-hx - THICK * 0.5, h * 0.5, 0.0), SlabKind::Wall));
    v.push(slab(Vec3::new(2.0 * hx, h, THICK), Transform::from_xyz(0.0, h * 0.5, hz + THICK * 0.5), SlabKind::Wall));
    v.push(slab(Vec3::new(2.0 * hx, h, THICK), Transform::from_xyz(0.0, h * 0.5, -hz - THICK * 0.5), SlabKind::Wall));

    // Quarter-pipes: floor->wall and wall->ceiling, along all four walls.
    for sx in [-1.0f32, 1.0] {
        let wall = Vec3::X * sx;
        quarter_pipe(&mut v, Vec3::new(sx * (hx - r), r, 0.0), -Vec3::Y, wall, Vec3::Z, 2.0 * hz);
        quarter_pipe(&mut v, Vec3::new(sx * (hx - r), h - r, 0.0), wall, Vec3::Y, Vec3::Z, 2.0 * hz);
    }
    for sz in [-1.0f32, 1.0] {
        let wall = Vec3::Z * sz;
        quarter_pipe(&mut v, Vec3::new(0.0, r, sz * (hz - r)), -Vec3::Y, wall, Vec3::X, 2.0 * hx);
        quarter_pipe(&mut v, Vec3::new(0.0, h - r, sz * (hz - r)), wall, Vec3::Y, Vec3::X, 2.0 * hx);
    }

    // Mid-field kickers: tilted slabs whose top surface passes through `pos`.
    let ramp = |angle: f32, len: f32, pos: Vec3, yaw: f32| {
        let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(angle);
        let center = pos + rot * Vec3::new(0.0, -THICK * 0.5, 0.0);
        slab(Vec3::new(len, THICK, 6.0), Transform::from_translation(center).with_rotation(rot), SlabKind::Ramp)
    };
    v.push(ramp(0.30, 8.0, Vec3::new(0.0, 1.15, 18.0), -std::f32::consts::FRAC_PI_2));
    v.push(ramp(0.45, 6.0, Vec3::new(-18.0, 1.3, -10.0), 0.0));
    v.push(ramp(0.20, 12.0, Vec3::new(18.0, 1.2, -25.0), 0.6));
    v
}

/// Colliders only (no rendering) — used by headless tests.
#[cfg_attr(not(test), allow(dead_code))]
pub fn spawn_colliders(commands: &mut Commands) {
    for s in layout() {
        commands.spawn((s.transform, RigidBody::Static, Collider::cuboid(s.size.x, s.size.y, s.size.z), ArenaCollider));
    }
}

pub fn spawn_arena(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    let floor_tex = checker_texture(&mut images, [46, 92, 58, 255], [40, 80, 50, 255]);
    let wall_tex = checker_texture(&mut images, [70, 78, 104, 255], [60, 66, 90, 255]);
    let floor_mat = materials.add(StandardMaterial {
        base_color_texture: Some(floor_tex),
        uv_transform: Affine2::from_scale(Vec2::new(HALF_X / 4.0, HALF_Z / 4.0)),
        perceptual_roughness: 0.9,
        ..default()
    });
    let wall_mat = materials.add(StandardMaterial {
        base_color_texture: Some(wall_tex),
        uv_transform: Affine2::from_scale(Vec2::new(12.0, 4.0)),
        perceptual_roughness: 0.8,
        ..default()
    });
    let pipe_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.45, 0.5, 0.62), perceptual_roughness: 0.7, ..default() });
    let ramp_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.85, 0.55, 0.2), perceptual_roughness: 0.6, ..default() });

    for s in layout() {
        let mat = match s.kind {
            SlabKind::Floor => &floor_mat,
            SlabKind::Wall => &wall_mat,
            SlabKind::Pipe => &pipe_mat,
            SlabKind::Ramp => &ramp_mat,
        };
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(s.size.x, s.size.y, s.size.z))),
            MeshMaterial3d(mat.clone()),
            s.transform,
            RigidBody::Static,
            Collider::cuboid(s.size.x, s.size.y, s.size.z),
            ArenaCollider,
        ));
    }

    // Lighting.
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(30.0, 60.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() });
}
