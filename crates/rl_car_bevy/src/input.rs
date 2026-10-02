//! Keyboard / gamepad -> `rl_car_core::Controls`.
//!
//! Keyboard (Rocket League-style defaults):
//!   W / S        throttle forward / reverse, and pitch down / up in the air
//!   A / D        steer, and yaw in the air
//!   Q / E        air roll left / right
//!   Space        jump (hold for a higher jump, press again to double jump / dodge)
//!   Left Shift   boost
//!   Left Ctrl    powerslide; in the air, turns A/D into air roll
//! Gamepad (Xbox layout): left stick steer/pitch/yaw, RT/LT throttle/reverse, A jump, B boost,
//! X powerslide + free air roll, LB/RB air roll left/right.

use bevy::prelude::*;
use rl_car_core::Controls;

const STICK_DEADZONE: f32 = 0.1;

fn axis(neg: bool, pos: bool) -> f32 {
    (pos as i32 - neg as i32) as f32
}

fn deadzone(v: f32) -> f32 {
    if v.abs() < STICK_DEADZONE { 0.0 } else { (v - STICK_DEADZONE * v.signum()) / (1.0 - STICK_DEADZONE) }
}

pub fn read_controls(keys: &ButtonInput<KeyCode>, gamepads: &Query<&Gamepad>) -> Controls {
    let k = |c: KeyCode| keys.pressed(c);
    let fwd = axis(k(KeyCode::KeyS), k(KeyCode::KeyW));
    let side = axis(k(KeyCode::KeyA), k(KeyCode::KeyD));
    let air_roll_mod = k(KeyCode::ControlLeft);

    let mut c = Controls {
        throttle: fwd,
        steer: side,
        pitch: -fwd,
        yaw: if air_roll_mod { 0.0 } else { side },
        roll: axis(k(KeyCode::KeyQ), k(KeyCode::KeyE)) + if air_roll_mod { side } else { 0.0 },
        jump: k(KeyCode::Space),
        boost: k(KeyCode::ShiftLeft) || k(KeyCode::ShiftRight),
        handbrake: air_roll_mod,
    };

    for pad in gamepads.iter() {
        let stick = Vec2::new(deadzone(pad.left_stick().x), deadzone(pad.left_stick().y));
        let rt = pad.get(GamepadButton::RightTrigger2).unwrap_or(0.0);
        let lt = pad.get(GamepadButton::LeftTrigger2).unwrap_or(0.0);
        let free_roll = pad.pressed(GamepadButton::West);
        let roll_buttons = axis(pad.pressed(GamepadButton::LeftTrigger), pad.pressed(GamepadButton::RightTrigger));
        let pad_c = Controls {
            throttle: rt - lt,
            steer: stick.x,
            pitch: -stick.y,
            yaw: if free_roll { 0.0 } else { stick.x },
            roll: roll_buttons + if free_roll { stick.x } else { 0.0 },
            jump: pad.pressed(GamepadButton::South),
            boost: pad.pressed(GamepadButton::East),
            handbrake: free_roll,
        };
        // Merge: whichever device has the larger input wins per axis.
        let pick = |a: f32, b: f32| if b.abs() > a.abs() { b } else { a };
        c.throttle = pick(c.throttle, pad_c.throttle);
        c.steer = pick(c.steer, pad_c.steer);
        c.pitch = pick(c.pitch, pad_c.pitch);
        c.yaw = pick(c.yaw, pad_c.yaw);
        c.roll = pick(c.roll, pad_c.roll);
        c.jump |= pad_c.jump;
        c.boost |= pad_c.boost;
        c.handbrake |= pad_c.handbrake;
    }
    c.clamped()
}
