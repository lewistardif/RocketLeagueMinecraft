use crate::consts::Curve;
use crate::math::Vec3;
use crate::state::CarState;

pub const BUMP_COOLDOWN_TIME: f32 = 0.25;
pub const BUMP_MIN_FORWARD_DIST: f32 = 64.5;
pub const BUMP_VEL_AMOUNT_GROUND: Curve = Curve(&[(0.0, 5.0 / 6.0), (1400.0, 1100.0), (2200.0, 1530.0)]);
pub const BUMP_VEL_AMOUNT_AIR: Curve = Curve(&[(0.0, 5.0 / 6.0), (1400.0, 1390.0), (2200.0, 1945.0)]);
pub const BUMP_UPWARD_VEL_AMOUNT: Curve = Curve(&[(0.0, 2.0 / 6.0), (1400.0, 278.0), (2200.0, 417.0)]);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BumpVictim {
    pub position: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub up: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bump {
    Demolish,
    Push(Vec3),
}

fn normalized(v: Vec3) -> Vec3 {
    let l = v.length();
    if l > f32::EPSILON * f32::EPSILON { v / l } else { Vec3::ZERO }
}

pub fn bump(attacker: &CarState, victim: &BumpVictim, contact_local_x: f32, force_scale: f32, demolish: bool) -> Option<Bump> {
    let delta = victim.position - attacker.position;
    if attacker.velocity.dot(delta) <= 0.0 {
        return None;
    }
    let vel_dir = normalized(attacker.velocity);
    let speed_towards = attacker.velocity.dot(normalized(delta));
    let away = victim.velocity.dot(vel_dir);
    if speed_towards <= away || contact_local_x <= BUMP_MIN_FORWARD_DIST {
        return None;
    }
    if attacker.is_supersonic && demolish {
        return Some(Bump::Demolish);
    }
    let base = if victim.on_ground { BUMP_VEL_AMOUNT_GROUND } else { BUMP_VEL_AMOUNT_AIR }.eval(speed_towards);
    let up = if victim.on_ground { victim.up } else { Vec3::Z };
    Some(Bump::Push(vel_dir * base + up * BUMP_UPWARD_VEL_AMOUNT.eval(speed_towards) * force_scale))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HitboxPreset;

    fn car(speed: f32, supersonic: bool) -> CarState {
        let mut s = CarState::new(HitboxPreset::Octane);
        s.velocity = Vec3::new(speed, 0.0, 0.0);
        s.is_supersonic = supersonic;
        s
    }

    fn ahead() -> BumpVictim {
        BumpVictim { position: Vec3::new(200.0, 0.0, 17.0), velocity: Vec3::ZERO, on_ground: true, up: Vec3::Z }
    }

    #[test]
    fn supersonic_bumper_hit_demolishes() {
        assert_eq!(bump(&car(2250.0, true), &ahead(), 70.0, 1.0, true), Some(Bump::Demolish));
    }

    #[test]
    fn supersonic_without_demolition_is_a_big_bump() {
        let Some(Bump::Push(v)) = bump(&car(2250.0, true), &ahead(), 70.0, 1.0, false) else { panic!() };
        assert!((v.x - 1530.0).abs() < 0.01, "{v:?}");
    }

    #[test]
    fn bump_follows_rocket_leagues_curves() {
        let Some(Bump::Push(v)) = bump(&car(1400.0, false), &ahead(), 70.0, 1.0, true) else { panic!() };
        assert!((v.x - 1100.0).abs() < 0.01 && (v.z - 278.0).abs() < 0.01, "{v:?}");
    }

    #[test]
    fn no_bump_from_the_side_moving_away_or_off_the_bumper() {
        assert_eq!(bump(&car(1400.0, false), &ahead(), 10.0, 1.0, true), None);
        assert_eq!(bump(&car(-1400.0, false), &ahead(), 70.0, 1.0, true), None);
        let fleeing = BumpVictim { velocity: Vec3::new(1500.0, 0.0, 0.0), ..ahead() };
        assert_eq!(bump(&car(1400.0, false), &fleeing, 70.0, 1.0, true), None);
    }
}
