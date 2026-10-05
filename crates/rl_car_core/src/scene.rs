use crate::ball::{BallConfig, BallState, hit};
use crate::body::Body;
use crate::consts::{BT_TO_UU, UU_TO_BT};
use crate::island::{self, IslandBody, IslandContact};
use crate::manifold::{MAX_POINTS, ManifoldPoint, Manifolds};
use crate::math::{Mat3, Vec3};
use crate::sim::{self, CarTick, DynamicGround, SimConfig};
use crate::solver;
use crate::state::{CarState, Controls};
use crate::world::{CollisionWorld, Contact};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PairPoint {
    pub local_a: Vec3,
    pub local_b: Vec3,
    pub normal: Vec3,
    pub distance: f32,
    pub applied_impulse: f32,
    pub lifetime: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PairManifold {
    pub count: usize,
    pub points: [PairPoint; MAX_POINTS],
}

impl PairManifold {
    fn remove(&mut self, index: usize) {
        let last = self.count - 1;
        if index != last {
            self.points[index] = self.points[last];
        }
        self.points[last] = PairPoint::default();
        self.count -= 1;
    }

    fn refresh(&mut self, pos_a: Vec3, basis_a: &Mat3, pos_b: Vec3, basis_b: &Mat3, threshold: f32) {
        for i in (0..self.count).rev() {
            let p = &mut self.points[i];
            let wa = *basis_a * p.local_a + pos_a;
            let wb = *basis_b * p.local_b + pos_b;
            p.distance = (wa - wb).dot(p.normal);
            p.lifetime += 1;
        }
        for i in (0..self.count).rev() {
            let p = self.points[i];
            if p.distance > threshold {
                self.remove(i);
                continue;
            }
            let wa = *basis_a * p.local_a + pos_a;
            let wb = *basis_b * p.local_b + pos_b;
            let diff = wb - (wa - p.normal * p.distance);
            if diff.dot(diff) > threshold * threshold {
                self.remove(i);
            }
        }
    }

    fn add(&mut self, pt: PairPoint) {
        let index = if self.count == MAX_POINTS {
            self.replace_index(&pt)
        } else {
            self.count += 1;
            self.count - 1
        };
        self.points[index] = pt;
    }

    fn replace_index(&self, pt: &PairPoint) -> usize {
        let mut max_pen_index: i32 = -1;
        let mut max_pen = pt.distance;
        for i in 0..MAX_POINTS {
            if self.points[i].distance < max_pen {
                max_pen_index = i as i32;
                max_pen = self.points[i].distance;
            }
        }
        let p = |i: usize| self.points[i].local_a;
        let a = pt.local_a;
        let mut res = [0.0f32; 4];
        if max_pen_index != 0 {
            res[0] = (a - p(1)).cross(p(3) - p(2)).length_squared();
        }
        if max_pen_index != 1 {
            res[1] = (a - p(0)).cross(p(3) - p(2)).length_squared();
        }
        if max_pen_index != 2 {
            res[2] = (a - p(0)).cross(p(3) - p(1)).length_squared();
        }
        if max_pen_index != 3 {
            res[3] = (a - p(0)).cross(p(2) - p(1)).length_squared();
        }
        let mut best = 0;
        let mut best_val = f32::MIN;
        for (i, v) in res.iter().enumerate() {
            if v.abs() > best_val {
                best = i;
                best_val = v.abs();
            }
        }
        best
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneCar {
    pub state: CarState,
    pub ball_contact: PairManifold,
    pub last_extra_impulse_tick: u64,
    pub touched_ball: bool,
    pub ball_hit_extra_velocity: Vec3,
}

impl SceneCar {
    pub fn new(state: CarState) -> SceneCar {
        SceneCar {
            state,
            ball_contact: PairManifold::default(),
            last_extra_impulse_tick: u64::MAX,
            touched_ball: false,
            ball_hit_extra_velocity: Vec3::ZERO,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub cars: Vec<SceneCar>,
    pub ball: BallState,
    pub tick: u64,
}

impl Scene {
    pub fn new(cars: Vec<CarState>, ball: BallState) -> Scene {
        Scene { cars: cars.into_iter().map(SceneCar::new).collect(), ball, tick: 0 }
    }
}

fn ball_body(ball: &BallState, cfg: &BallConfig) -> Body {
    let inv_i = 1.0 / cfg.inertia_bt();
    Body {
        pos: ball.position * UU_TO_BT,
        vel: ball.velocity * UU_TO_BT,
        ang_vel: ball.angular_velocity,
        basis: Mat3::IDENTITY,
        inv_mass: 1.0 / cfg.mass,
        inv_inertia_local: Vec3::new(inv_i, inv_i, inv_i),
        inv_inertia_world: Mat3::IDENTITY.scaled(Vec3::new(inv_i, inv_i, inv_i)).mul_mat(&Mat3::IDENTITY),
        total_force: Vec3::ZERO,
        total_ang_accel: Vec3::ZERO,
    }
}

struct SphereHit {
    point_on_box: Vec3,
    normal: Vec3,
    depth: f32,
}

fn sphere_box(t: &CarTick, center: Vec3, radius: f32, threshold: f32) -> Option<SphereHit> {
    let car = &t.car;
    let body = &t.body;
    let half = car.hitbox_size * UU_TO_BT * 0.5;
    let margin_default = crate::consts::solver::BOX_MARGIN_BT;
    let inner = half - Vec3::new(margin_default, margin_default, margin_default);
    let margin = (0.1 * half.x.min(half.y).min(half.z)).min(margin_default);

    let box_origin = body.basis * (car.hitbox_pos_offset * UU_TO_BT) + body.pos;
    let rel = body.basis.transpose_mul_vec(center - box_origin);
    let closest = Vec3::new(
        rel.x.min(inner.x).max(-inner.x),
        rel.y.min(inner.y).max(-inner.y),
        rel.z.min(inner.z).max(-inner.z),
    );
    let intersection = radius + margin;
    let contact_dist = intersection + threshold;
    let mut normal = rel - closest;
    let dist2 = normal.length_squared();
    if dist2 > contact_dist * contact_dist {
        return None;
    }
    let mut closest = closest;
    let distance = if dist2 <= f32::EPSILON {
        let mut min_dist = inner.x - rel.x;
        closest.x = inner.x;
        normal = Vec3::X;
        let faces = [
            (inner.x + rel.x, 0, -1.0),
            (inner.y - rel.y, 1, 1.0),
            (inner.y + rel.y, 1, -1.0),
            (inner.z - rel.z, 2, 1.0),
            (inner.z + rel.z, 2, -1.0),
        ];
        for (face_dist, axis, sign) in faces {
            if face_dist < min_dist {
                min_dist = face_dist;
                closest = rel;
                let mut n = Vec3::ZERO;
                match axis {
                    0 => {
                        closest.x = -inner.x;
                        n.x = sign;
                    }
                    1 => {
                        closest.y = inner.y * sign;
                        n.y = sign;
                    }
                    _ => {
                        closest.z = inner.z * sign;
                        n.z = sign;
                    }
                }
                normal = n;
            }
        }
        -min_dist
    } else {
        let d = normal.length();
        normal *= 1.0 / d;
        d
    };
    let point_local = closest + normal * margin;
    Some(SphereHit { point_on_box: body.basis * point_local + box_origin, normal: body.basis * normal, depth: distance - intersection })
}

fn extra_hit_velocity(car: &CarTick, ball_pos: Vec3, ball_vel: Vec3, cfg: &BallConfig) -> Vec3 {
    let car_pos = car.body.pos * BT_TO_UU;
    let car_vel = car.body.vel * BT_TO_UU;
    let forward = car.body.forward();
    let rel_pos = ball_pos - car_pos;
    let rel_vel = ball_vel - car_vel;
    let rel_speed = rel_vel.length().min(hit::MAX_DELTA_VEL);
    if rel_speed <= 0.0 {
        return Vec3::ZERO;
    }
    let normalized = |v: Vec3| {
        let l = v.length();
        if l > f32::EPSILON * f32::EPSILON { v / l } else { Vec3::ZERO }
    };
    let mut hit_dir = normalized(rel_pos.mul_elem(Vec3::new(1.0, 1.0, hit::Z_SCALE)));
    let forward_adjustment = forward * hit_dir.dot(forward) * (1.0 - hit::FORWARD_SCALE);
    hit_dir = normalized(hit_dir - forward_adjustment);
    (hit_dir * rel_speed) * hit::FACTOR_CURVE.eval(rel_speed) * cfg.hit_extra_force_scale
}

pub fn step_scene(scene: &mut Scene, controls: &[Controls], world: &dyn CollisionWorld, cfg: &SimConfig, ball_cfg: &BallConfig, dt: f32) {
    let radius = ball_cfg.radius_bt();
    let ball_threshold = ball_cfg.contact_breaking_threshold_bt();
    let sleeping = scene.ball.is_sleeping();

    let mut bb = ball_body(&scene.ball, ball_cfg);
    let ground = DynamicGround {
        center: bb.pos,
        radius,
        vel: bb.vel,
        ang_vel: bb.ang_vel,
        inv_mass: bb.inv_mass,
        inv_inertia: bb.inv_inertia_local.x,
    };

    let mut ticks: Vec<CarTick> = scene
        .cars
        .iter()
        .enumerate()
        .map(|(i, c)| sim::car_begin(&c.state, &controls.get(i).copied().unwrap_or_default(), world, cfg, dt, Some(&ground)))
        .collect();

    if !sleeping {
        bb.apply_central_force(cfg.gravity * UU_TO_BT * (1.0 / bb.inv_mass));
    }
    bb.vel *= (1.0 - ball_cfg.drag.clamp(0.0, 1.0)).powf(dt);

    for t in ticks.iter_mut() {
        sim::car_collide_world(t, world);
    }

    if !sleeping {
        let mut found: Vec<Contact> = Vec::new();
        world.sphere_contacts(bb.pos * BT_TO_UU, ball_cfg.radius, ball_threshold * BT_TO_UU, &mut found);
        for c in &found {
            let distance = -c.depth * UU_TO_BT;
            if distance > ball_threshold {
                continue;
            }
            let on_b = c.point * UU_TO_BT;
            let on_a = on_b + c.normal * distance;
            scene.ball.manifolds.get_or_create(c.surface).add(ManifoldPoint {
                local_a: on_a - bb.pos,
                world_b: on_b,
                normal: c.normal,
                distance,
                applied_impulse: 0.0,
                lifetime: 0,
            });
        }
        scene.ball.manifolds.refresh_all(bb.pos, &Mat3::IDENTITY, ball_threshold);
        scene.ball.manifolds.prune();
    }

    let mut velocity_cache = Vec3::ZERO;
    let ball_pos_uu = bb.pos * BT_TO_UU;
    let ball_vel_uu = bb.vel * BT_TO_UU;
    for (sc, t) in scene.cars.iter_mut().zip(ticks.iter()) {
        sc.touched_ball = false;
        sc.ball_hit_extra_velocity = Vec3::ZERO;
        let threshold = ball_threshold.min(t.breaking_threshold);
        sc.ball_contact.refresh(bb.pos, &Mat3::IDENTITY, t.body.pos, &t.body.basis, threshold);
        if let Some(h) = sphere_box(t, bb.pos, radius, threshold)
            && h.depth <= threshold
        {
            let on_a = h.point_on_box + h.normal * h.depth;
            sc.ball_contact.add(PairPoint {
                local_a: on_a - bb.pos,
                local_b: t.body.basis.transpose_mul_vec(h.point_on_box - t.body.pos),
                normal: h.normal,
                distance: h.depth,
                applied_impulse: 0.0,
                lifetime: 0,
            });
            sc.touched_ball = true;
            let tick = scene.tick;
            let last = sc.last_extra_impulse_tick;
            if tick > last.wrapping_add(1) || last > tick {
                sc.last_extra_impulse_tick = tick;
                let added = extra_hit_velocity(t, ball_pos_uu, ball_vel_uu, ball_cfg);
                sc.ball_hit_extra_velocity = added;
                velocity_cache += added * UU_TO_BT;
            }
        }
        sc.ball_contact.refresh(bb.pos, &Mat3::IDENTITY, t.body.pos, &t.body.basis, threshold);
    }

    let coupled: Vec<usize> = (0..ticks.len()).filter(|&i| scene.cars[i].ball_contact.count > 0).collect();

    for (i, t) in ticks.iter_mut().enumerate() {
        if coupled.contains(&i) {
            continue;
        }
        let contacts = sim::car_world_contacts(t);
        let applied = solver::solve_and_integrate(&mut t.body, &contacts, cfg.car_world_friction, cfg.car_world_restitution, dt);
        sim::car_store_world_impulses(t, &applied);
    }

    let ball_active = !sleeping || !coupled.is_empty();
    if ball_active {
        let mut bodies: Vec<IslandBody> = Vec::with_capacity(coupled.len() + 1);
        for &i in &coupled {
            bodies.push(IslandBody::new(ticks[i].body, false));
        }
        let ball_index = bodies.len();
        bodies.push(IslandBody::new(bb, true));

        let warm = Manifolds::warmstart_factor();
        let mut contacts: Vec<IslandContact> = Vec::new();
        let mut world_counts = Vec::with_capacity(coupled.len());
        for (k, &i) in coupled.iter().enumerate() {
            let t = &ticks[i];
            let before = contacts.len();
            for m in t.s.manifolds.list.iter().filter(|m| m.active) {
                for p in &m.points[..m.count] {
                    contacts.push(IslandContact {
                        a: k,
                        b: None,
                        point_a: t.body.basis * p.local_a + t.body.pos,
                        point_b: p.world_b,
                        normal: p.normal,
                        distance: p.distance,
                        warm_impulse: p.applied_impulse * warm,
                        friction: cfg.car_world_friction,
                        restitution: cfg.car_world_restitution,
                        special: false,
                    });
                }
            }
            world_counts.push(contacts.len() - before);
        }
        let mut pair_starts = Vec::with_capacity(coupled.len());
        for (k, &i) in coupled.iter().enumerate() {
            let t = &ticks[i];
            let m = &scene.cars[i].ball_contact;
            pair_starts.push(contacts.len());
            for p in &m.points[..m.count] {
                contacts.push(IslandContact {
                    a: ball_index,
                    b: Some(k),
                    point_a: p.local_a + bb.pos,
                    point_b: t.body.basis * p.local_b + t.body.pos,
                    normal: p.normal,
                    distance: p.distance,
                    warm_impulse: p.applied_impulse * warm,
                    friction: ball_cfg.car_friction,
                    restitution: ball_cfg.car_restitution,
                    special: false,
                });
            }
        }
        for m in scene.ball.manifolds.list.iter().filter(|m| m.active) {
            for p in &m.points[..m.count] {
                contacts.push(IslandContact {
                    a: ball_index,
                    b: None,
                    point_a: p.local_a + bb.pos,
                    point_b: p.world_b,
                    normal: p.normal,
                    distance: p.distance,
                    warm_impulse: 0.0,
                    friction: ball_cfg.world_friction,
                    restitution: ball_cfg.world_restitution,
                    special: true,
                });
            }
        }

        let applied = island::solve_and_integrate(&mut bodies, &contacts, dt);

        let mut offset = 0;
        for (k, &i) in coupled.iter().enumerate() {
            let n = world_counts[k];
            sim::car_store_world_impulses(&mut ticks[i], &applied[offset..offset + n]);
            offset += n;
        }
        for (k, &i) in coupled.iter().enumerate() {
            let m = &mut scene.cars[i].ball_contact;
            let start = pair_starts[k];
            for (j, p) in m.points[..m.count].iter_mut().enumerate() {
                p.applied_impulse = applied[start + j];
            }
        }
        for (k, &i) in coupled.iter().enumerate() {
            ticks[i].body = bodies[k].body;
        }
        bb = bodies[ball_index].body;
    }

    for (sc, t) in scene.cars.iter_mut().zip(ticks) {
        sc.state = sim::car_end(t, cfg, dt);
    }

    if ball_active {
        let mut vel = bb.vel + velocity_cache;
        let max = ball_cfg.max_speed * UU_TO_BT;
        if vel.length_squared() > max * max {
            vel = vel.normalized() * max;
        }
        let mut ang = bb.ang_vel;
        if ang.length_squared() > ball_cfg.max_ang_speed * ball_cfg.max_ang_speed {
            ang = ang.normalized() * ball_cfg.max_ang_speed;
        }
        scene.ball.position = bb.pos * BT_TO_UU;
        scene.ball.velocity = vel * BT_TO_UU;
        scene.ball.angular_velocity = ang;
    }
    scene.tick += 1;
}
