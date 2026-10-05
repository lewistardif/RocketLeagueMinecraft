use crate::body::Body;
use crate::consts::solver::*;
use crate::math::Vec3;

pub(crate) struct IslandBody {
    pub body: Body,
    pub no_rot: bool,
    ext_v: Vec3,
    ext_w: Vec3,
    d_lin: Vec3,
    d_ang: Vec3,
    push: Vec3,
    turn: Vec3,
    special_count: u32,
    special_normal: Vec3,
    special_dist: f32,
    special_friction: f32,
    special_restitution: f32,
}

impl IslandBody {
    pub fn new(body: Body, no_rot: bool) -> IslandBody {
        IslandBody {
            body,
            no_rot,
            ext_v: Vec3::ZERO,
            ext_w: Vec3::ZERO,
            d_lin: Vec3::ZERO,
            d_ang: Vec3::ZERO,
            push: Vec3::ZERO,
            turn: Vec3::ZERO,
            special_count: 0,
            special_normal: Vec3::ZERO,
            special_dist: 0.0,
            special_friction: 0.0,
            special_restitution: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct IslandContact {
    pub a: usize,
    pub b: Option<usize>,
    pub point_a: Vec3,
    pub point_b: Vec3,
    pub normal: Vec3,
    pub distance: f32,
    pub warm_impulse: f32,
    pub friction: f32,
    pub restitution: f32,
    pub special: bool,
}

#[derive(Clone, Copy, Default)]
struct Row {
    a: usize,
    b: Option<usize>,
    n1: Vec3,
    rc1: Vec3,
    ang_a: Vec3,
    n2: Vec3,
    rc2: Vec3,
    ang_b: Vec3,
    jac_inv: f32,
    rhs: f32,
    rhs_pen: f32,
    applied: f32,
    applied_push: f32,
    lower: f32,
    upper: f32,
    friction: f32,
    special: bool,
    normal_row: usize,
}

fn plane_space(n: Vec3) -> Vec3 {
    if n.z.abs() > core::f32::consts::FRAC_1_SQRT_2 {
        let a = n.y * n.y + n.z * n.z;
        let k = 1.0 / a.sqrt();
        Vec3::new(0.0, -n.z * k, n.y * k)
    } else {
        let a = n.x * n.x + n.y * n.y;
        let k = 1.0 / a.sqrt();
        Vec3::new(-n.y * k, n.x * k, 0.0)
    }
}

fn apply(bodies: &mut [IslandBody], row: &Row, delta: f32) {
    let a = &mut bodies[row.a];
    a.d_lin += row.n1 * a.body.inv_mass * delta;
    a.d_ang += row.ang_a * delta;
    if let Some(b) = row.b {
        let b = &mut bodies[b];
        b.d_lin += row.n2 * b.body.inv_mass * delta;
        b.d_ang += row.ang_b * delta;
    }
}

fn apply_push(bodies: &mut [IslandBody], row: &Row, delta: f32) {
    let a = &mut bodies[row.a];
    a.push += row.n1 * a.body.inv_mass * delta;
    a.turn += row.ang_a * delta;
    if let Some(b) = row.b {
        let b = &mut bodies[b];
        b.push += row.n2 * b.body.inv_mass * delta;
        b.turn += row.ang_b * delta;
    }
}

fn vel_no_delta(bodies: &[IslandBody], i: Option<usize>, rel: Vec3) -> Vec3 {
    match i {
        Some(i) => {
            let b = &bodies[i];
            (b.body.vel + b.ext_v) + (b.body.ang_vel + b.ext_w).cross(rel)
        }
        None => Vec3::ZERO,
    }
}

#[allow(clippy::too_many_arguments)]
fn normal_row(bodies: &mut [IslandBody], a: usize, b: Option<usize>, r1: Vec3, r2: Vec3, n: Vec3, distance: f32, warm: f32, restitution: f32, inv_dt: f32) -> Row {
    let ba = &bodies[a].body;
    let torque_axis0 = r1.cross(n);
    let ang_a = ba.inv_inertia_world * torque_axis0;
    let torque_axis1 = r2.cross(n);
    let ang_b = b.map_or(Vec3::ZERO, |b| bodies[b].body.inv_inertia_world * -torque_axis1);
    let denom0 = ba.inv_mass + n.dot(ang_a.cross(r1));
    let denom1 = b.map_or(0.0, |b| bodies[b].body.inv_mass + n.dot((-ang_b).cross(r2)));
    let jac_inv = 1.0 / (denom0 + denom1);

    let (n2, rc2) = if b.is_some() { (-n, -torque_axis1) } else { (Vec3::ZERO, Vec3::ZERO) };

    let vel1 = ba.velocity_at(r1);
    let vel2 = b.map_or(Vec3::ZERO, |b| bodies[b].body.velocity_at(r2));
    let rel_vel0 = n.dot(vel1 - vel2);
    let mut rest = if rel_vel0.abs() < RESTITUTION_VELOCITY_THRESHOLD_BT { 0.0 } else { restitution * -rel_vel0 };
    if rest <= 0.0 {
        rest = 0.0;
    }

    let row = Row {
        a,
        b,
        n1: n,
        rc1: torque_axis0,
        ang_a,
        n2,
        rc2,
        ang_b,
        jac_inv,
        applied: warm,
        lower: 0.0,
        upper: 1e10,
        ..Default::default()
    };
    apply(bodies, &row, warm);

    let ia = &bodies[a];
    let vel1_dotn = row.n1.dot(ia.body.vel + ia.ext_v) + row.rc1.dot(ia.body.ang_vel + ia.ext_w);
    let vel2_dotn = b.map_or(0.0, |b| {
        let ib = &bodies[b];
        row.n2.dot(ib.body.vel + ib.ext_v) + row.rc2.dot(ib.body.ang_vel + ib.ext_w)
    });
    let rel_vel = vel1_dotn + vel2_dotn;
    let velocity_error = rest - rel_vel;
    let positional_error = if distance > 0.0 { 0.0 } else { -distance * ERP2 * inv_dt };
    Row { rhs: velocity_error * jac_inv, rhs_pen: positional_error * jac_inv, ..row }
}

#[allow(clippy::too_many_arguments)]
fn friction_row(bodies: &[IslandBody], a: usize, b: Option<usize>, r1: Vec3, r2: Vec3, n: Vec3, friction: f32, normal_index: usize) -> Row {
    let vel = vel_no_delta(bodies, Some(a), r1) - vel_no_delta(bodies, b, r2);
    let rel_vel = n.dot(vel);
    let lat = vel - n * rel_vel;
    let l2 = lat.length_squared();
    let dir = if l2 > f32::EPSILON { lat * (1.0 / l2.sqrt()) } else { plane_space(n) };

    let ba = &bodies[a].body;
    let rc1 = r1.cross(dir);
    let ang_a = ba.inv_inertia_world * rc1;
    let (n2, rc2, ang_b) = match b {
        Some(b) => {
            let rc2 = r2.cross(-dir);
            (-dir, rc2, bodies[b].body.inv_inertia_world * rc2)
        }
        None => (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO),
    };
    let denom0 = ba.inv_mass + dir.dot(ang_a.cross(r1));
    let denom1 = b.map_or(0.0, |b| bodies[b].body.inv_mass + dir.dot((-ang_b).cross(r2)));
    let jac_inv = 1.0 / (denom0 + denom1);

    let ia = &bodies[a];
    let vel1_dotn = dir.dot(ia.body.vel + ia.ext_v) + rc1.dot(ia.body.ang_vel);
    let vel2_dotn = b.map_or(0.0, |b| {
        let ib = &bodies[b];
        n2.dot(ib.body.vel + ib.ext_v) + rc2.dot(ib.body.ang_vel)
    });
    let rel_vel = vel1_dotn + vel2_dotn;
    Row {
        a,
        b,
        n1: dir,
        rc1,
        ang_a,
        n2,
        rc2,
        ang_b,
        jac_inv,
        rhs: (0.0 - rel_vel) * jac_inv,
        lower: -friction,
        upper: friction,
        friction,
        normal_row: normal_index,
        ..Default::default()
    }
}

fn dv(bodies: &[IslandBody], row: &Row, push: bool) -> (f32, f32) {
    let a = &bodies[row.a];
    let (la, wa) = if push { (a.push, a.turn) } else { (a.d_lin, a.d_ang) };
    let d1 = row.n1.dot(la) + row.rc1.dot(wa);
    let d2 = row.b.map_or(0.0, |b| {
        let b = &bodies[b];
        let (lb, wb) = if push { (b.push, b.turn) } else { (b.d_lin, b.d_ang) };
        row.n2.dot(lb) + row.rc2.dot(wb)
    });
    (d1, d2)
}

fn resolve(bodies: &mut [IslandBody], row: &mut Row) {
    let (d1, d2) = dv(bodies, row, false);
    let mut delta = row.rhs;
    delta -= d1 * row.jac_inv;
    delta -= d2 * row.jac_inv;
    let sum = row.applied + delta;
    if sum < row.lower {
        delta = row.lower - row.applied;
        row.applied = row.lower;
    } else if sum > row.upper {
        delta = row.upper - row.applied;
        row.applied = row.upper;
    } else {
        row.applied = sum;
    }
    apply(bodies, row, delta);
}

pub(crate) fn solve_and_integrate(bodies: &mut [IslandBody], contacts: &[IslandContact], dt: f32) -> Vec<f32> {
    let inv_dt = 1.0 / dt;
    for ib in bodies.iter_mut() {
        ib.ext_v = ib.body.total_force * ib.body.inv_mass * dt;
        ib.ext_w = ib.body.total_ang_accel * dt;
    }

    let mut rows: Vec<Row> = Vec::with_capacity(contacts.len() + 1);
    let mut frows: Vec<Row> = Vec::with_capacity(contacts.len() + 1);
    for c in contacts {
        let r1 = c.point_a - bodies[c.a].body.pos;
        let r2 = c.b.map_or(Vec3::ZERO, |b| c.point_b - bodies[b].body.pos);
        let mut row = normal_row(bodies, c.a, c.b, r1, r2, c.normal, c.distance, c.warm_impulse, c.restitution, inv_dt);
        row.special = c.special;
        row.friction = c.friction;
        if c.special {
            let ia = &mut bodies[c.a];
            ia.special_count += 1;
            ia.special_friction = c.friction;
            ia.special_restitution = c.restitution;
            ia.special_normal += c.normal;
            ia.special_dist += r1.length();
        }
        let idx = rows.len();
        rows.push(row);
        frows.push(friction_row(bodies, c.a, c.b, r1, r2, c.normal, c.friction, idx));
    }

    for i in 0..bodies.len() {
        let n = bodies[i].special_count;
        if n == 0 {
            continue;
        }
        let ib = &bodies[i];
        let distance = ib.special_dist / n as f32;
        let normal = ib.special_normal * (1.0 / n as f32);
        let (friction, restitution) = (ib.special_friction, ib.special_restitution);
        let r1 = normal * -distance;
        let mut row = normal_row(bodies, i, None, r1, Vec3::ZERO, normal, distance, 0.0, restitution, inv_dt);
        row.friction = friction;
        let idx = rows.len();
        rows.push(row);
        frows.push(friction_row(bodies, i, None, r1, Vec3::ZERO, normal, friction, idx));
        let ib = &mut bodies[i];
        ib.special_count = 0;
        ib.special_normal = Vec3::ZERO;
        ib.special_dist = 0.0;
    }

    if !rows.is_empty() {
        for iteration in 0..NUM_ITERATIONS {
            let mut residual = 0.0f32;
            for row in rows.iter_mut() {
                if row.rhs_pen == 0.0 {
                    continue;
                }
                let (d1, d2) = dv(bodies, row, true);
                let mut delta = row.rhs_pen;
                delta -= d1 * row.jac_inv;
                delta -= d2 * row.jac_inv;
                let sum = row.applied_push + delta;
                if sum < row.lower {
                    delta = row.lower - row.applied_push;
                    row.applied_push = row.lower;
                } else {
                    row.applied_push = sum;
                }
                apply_push(bodies, row, delta);
                let r = delta * (1.0 / row.jac_inv);
                residual = residual.max(r * r);
            }
            if residual <= 0.0 || iteration >= NUM_ITERATIONS - 1 {
                break;
            }
        }

        for _ in 0..NUM_ITERATIONS {
            for row in rows.iter_mut().filter(|r| !r.special) {
                resolve(bodies, row);
            }
            for frow in frows.iter_mut() {
                let total = rows[frow.normal_row].applied;
                if total > 0.0 {
                    frow.lower = -(frow.friction * total);
                    frow.upper = frow.friction * total;
                    resolve(bodies, frow);
                }
            }
        }
    }

    for ib in bodies.iter_mut() {
        let b = &mut ib.body;
        b.vel += ib.d_lin;
        b.ang_vel += ib.d_ang;
        if !ib.push.is_zero() || !ib.turn.is_zero() {
            if ib.no_rot {
                b.pos += ib.push * dt;
            } else {
                let (p, basis) = crate::math::integrate_transform(b.pos, &b.basis, ib.push, ib.turn * SPLIT_IMPULSE_TURN_ERP, dt);
                b.pos = p;
                b.basis = basis;
                b.update_inertia_tensor();
            }
        }
        b.vel += ib.ext_v;
        b.ang_vel += ib.ext_w;
        if ib.no_rot {
            b.pos += b.vel * dt;
        } else {
            b.integrate(dt);
        }
    }

    rows[..contacts.len()].iter().map(|r| if r.special { 0.0 } else { r.applied }).collect()
}
