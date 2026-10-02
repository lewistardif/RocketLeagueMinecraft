//! Car-body vs static-world contact resolution.
//!
//! A single-body sequential-impulse solver modelled on the behaviour of Bullet's
//! `btSequentialImpulseConstraintSolver` with the settings Rocket League/RocketSim use:
//! 10 iterations, split-impulse penetration recovery (erp2 = 0.8, turn erp = 0.1), restitution
//! with a 0.2 bt/s threshold, one velocity-aligned friction direction per contact, friction
//! clamped by the contact's normal impulse, warm starting from the cached manifold points.

use crate::body::Body;
use crate::consts::solver::*;
use crate::math::Vec3;

/// A contact point prepared for the solver (bt units).
#[derive(Clone, Copy, Debug)]
pub(crate) struct SolverContact {
    /// Contact point on the car, world space.
    pub point_on_car: Vec3,
    /// World -> car.
    pub normal: Vec3,
    /// Signed separation (negative = penetrating).
    pub distance: f32,
    /// Warm-start impulse (already scaled by the warm-starting factor).
    pub warm_impulse: f32,
}

struct Row {
    normal: Vec3,
    torque_axis: Vec3,
    ang_component: Vec3,
    jac_diag_inv: f32,
    rhs: f32,
    rhs_penetration: f32,
    applied: f32,
    applied_push: f32,
    lower: f32,
    upper: f32,
    friction: f32,
}

#[derive(Default)]
struct Deltas {
    lin: Vec3,
    ang: Vec3,
    push: Vec3,
    turn: Vec3,
}

/// Bullet `btPlaneSpace1`: any unit vector orthogonal to `n`.
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

/// Resolves contacts and integrates the body for one tick:
/// `v += F/m dt`, contact impulses, split-impulse position correction, then position update.
/// Returns the final normal impulse of each contact (for warm starting next tick).
pub(crate) fn solve_and_integrate(body: &mut Body, contacts: &[SolverContact], friction: f32, restitution: f32, dt: f32) -> Vec<f32> {
    let ext_v = body.total_force * body.inv_mass * dt;
    let ext_w = body.total_ang_accel * dt;

    let mut rows: Vec<Row> = Vec::with_capacity(contacts.len());
    let mut frows: Vec<Row> = Vec::with_capacity(contacts.len());

    let mut d = Deltas::default();

    for c in contacts {
        let n = c.normal;
        let r = c.point_on_car - body.pos;

        let torque_axis = r.cross(n);
        let ang_component = body.inv_inertia_world * torque_axis;
        let denom = body.inv_mass + n.dot(ang_component.cross(r));
        let jac_diag_inv = 1.0 / denom;

        let rel_vel0 = n.dot(body.velocity_at(r));
        let mut rest = if rel_vel0.abs() < RESTITUTION_VELOCITY_THRESHOLD_BT { 0.0 } else { restitution * -rel_vel0 };
        if rest <= 0.0 {
            rest = 0.0;
        }

        // Warm start.
        d.lin += n * body.inv_mass * c.warm_impulse;
        d.ang += ang_component * c.warm_impulse;

        let rel_vel = n.dot(body.vel + ext_v) + torque_axis.dot(body.ang_vel + ext_w);
        let penetration = c.distance;
        let velocity_error = rest - rel_vel;
        let positional_error = if penetration > 0.0 { 0.0 } else { -penetration * ERP2 / dt };

        rows.push(Row {
            normal: n,
            torque_axis,
            ang_component,
            jac_diag_inv,
            rhs: velocity_error * jac_diag_inv,
            rhs_penetration: positional_error * jac_diag_inv,
            applied: c.warm_impulse,
            applied_push: 0.0,
            lower: 0.0,
            upper: 1e10,
            friction,
        });

        // Single friction direction along the relative tangential velocity.
        let vel = (body.vel + ext_v) + (body.ang_vel + ext_w).cross(r);
        let lat = vel - n * n.dot(vel);
        let l2 = lat.length_squared();
        let dir = if l2 > f32::EPSILON { lat * (1.0 / l2.sqrt()) } else { plane_space(n) };
        let f_torque = r.cross(dir);
        let f_ang = body.inv_inertia_world * f_torque;
        let f_denom = body.inv_mass + dir.dot(f_ang.cross(r));
        let f_jac_inv = 1.0 / f_denom;
        let f_rel_vel = dir.dot(body.vel + ext_v) + f_torque.dot(body.ang_vel);
        frows.push(Row {
            normal: dir,
            torque_axis: f_torque,
            ang_component: f_ang,
            jac_diag_inv: f_jac_inv,
            rhs: (0.0 - f_rel_vel) * f_jac_inv,
            rhs_penetration: 0.0,
            applied: 0.0,
            applied_push: 0.0,
            lower: -friction,
            upper: friction,
            friction,
        });
    }

    if !rows.is_empty() {
        // Split-impulse penetration recovery.
        for iteration in 0..NUM_ITERATIONS {
            let mut residual = 0.0f32;
            for row in rows.iter_mut() {
                if row.rhs_penetration == 0.0 {
                    continue;
                }
                let mut delta = row.rhs_penetration;
                let dv = row.normal.dot(d.push) + row.torque_axis.dot(d.turn);
                delta -= dv * row.jac_diag_inv;
                let sum = row.applied_push + delta;
                if sum < row.lower {
                    delta = row.lower - row.applied_push;
                    row.applied_push = row.lower;
                } else {
                    row.applied_push = sum;
                }
                d.push += row.normal * body.inv_mass * delta;
                d.turn += row.ang_component * delta;
                let r = delta * (1.0 / row.jac_diag_inv);
                residual = residual.max(r * r);
            }
            if residual <= 0.0 || iteration >= NUM_ITERATIONS - 1 {
                break;
            }
        }

        // Velocity iterations: all normal rows, then all friction rows.
        for _ in 0..NUM_ITERATIONS {
            for row in rows.iter_mut() {
                resolve_row(row, &mut d, body.inv_mass);
            }
            for (i, frow) in frows.iter_mut().enumerate() {
                let total = rows[i].applied;
                if total > 0.0 {
                    frow.lower = -(frow.friction * total);
                    frow.upper = frow.friction * total;
                    resolve_row(frow, &mut d, body.inv_mass);
                }
            }
        }
    }

    body.vel += d.lin;
    body.ang_vel += d.ang;
    if !d.push.is_zero() || !d.turn.is_zero() {
        let (p, b) = crate::math::integrate_transform(body.pos, &body.basis, d.push, d.turn * SPLIT_IMPULSE_TURN_ERP, dt);
        body.pos = p;
        body.basis = b;
        body.update_inertia_tensor();
    }
    body.vel += ext_v;
    body.ang_vel += ext_w;

    body.integrate(dt);
    rows.iter().map(|r| r.applied).collect()
}

fn resolve_row(row: &mut Row, d: &mut Deltas, inv_mass: f32) {
    let mut delta = row.rhs;
    let dv = row.normal.dot(d.lin) + row.torque_axis.dot(d.ang);
    delta -= dv * row.jac_diag_inv;
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
    d.lin += row.normal * inv_mass * delta;
    d.ang += row.ang_component * delta;
}
