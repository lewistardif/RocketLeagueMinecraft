//! Validation scenarios: initial state + scripted controls + plane world.
//! The same definition is written to a text file for the RocketSim oracle and run through the core.

use rl_car_core::{CarState, Controls, HitboxPreset, Plane, PlaneWorld, RotMat, Vec3};
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Wheels-only driving on flat ground.
    Ground,
    /// Jumps, dodges and aerial control (may include landing).
    Air,
    /// Wall / ceiling driving.
    Wall,
    /// Car body hitting the world (body contact solver).
    Body,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Ground => "ground",
            Category::Air => "air",
            Category::Wall => "wall",
            Category::Body => "body-contact",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub name: String,
    pub description: String,
    pub category: Category,
    pub preset: HitboxPreset,
    pub ticks: u32,
    pub planes: Vec<Plane>,
    pub pos: Vec3,
    pub vel: Vec3,
    pub ang_vel: Vec3,
    pub rot: RotMat,
    pub boost: f32,
    /// (from_tick, controls), sorted by tick.
    pub controls: Vec<(u32, Controls)>,
    /// Ticks to compare (comparison stops at the first tick listed here if set — used for
    /// scenarios whose tail is chaotic, e.g. long tumbling after a crash).
    pub compare_ticks: Option<u32>,
}

impl Scenario {
    fn new(name: &str, description: &str, category: Category) -> Scenario {
        Scenario {
            name: name.into(),
            description: description.into(),
            category,
            preset: HitboxPreset::Octane,
            ticks: 240,
            planes: vec![floor()],
            pos: Vec3::new(0.0, 0.0, 17.0),
            vel: Vec3::ZERO,
            ang_vel: Vec3::ZERO,
            rot: RotMat::IDENTITY,
            boost: 100.0,
            controls: vec![],
            compare_ticks: None,
        }
    }

    fn ctrl(mut self, from: u32, c: Controls) -> Self {
        self.controls.push((from, c));
        self
    }

    pub fn world(&self) -> PlaneWorld {
        PlaneWorld::new(self.planes.clone())
    }

    pub fn initial_state(&self) -> CarState {
        let mut s = CarState::new(self.preset);
        s.position = self.pos;
        s.velocity = self.vel;
        s.angular_velocity = self.ang_vel;
        s.orientation = self.rot;
        s.boost_amount = self.boost;
        s
    }

    pub fn controls_at(&self, tick: u32) -> Controls {
        let mut c = Controls::default();
        for (from, ctl) in &self.controls {
            if *from <= tick {
                c = *ctl;
            }
        }
        c
    }

    pub fn to_oracle_text(&self) -> String {
        let mut s = String::new();
        let v = |v: Vec3| format!("{} {} {}", v.x, v.y, v.z);
        writeln!(s, "# {}", self.description).unwrap();
        writeln!(s, "name {}", self.name).unwrap();
        writeln!(s, "preset {}", self.preset.name()).unwrap();
        writeln!(s, "ticks {}", self.ticks).unwrap();
        for p in &self.planes {
            writeln!(s, "plane {} {}", v(p.point), v(p.normal)).unwrap();
        }
        writeln!(s, "pos {}", v(self.pos)).unwrap();
        writeln!(s, "vel {}", v(self.vel)).unwrap();
        writeln!(s, "angvel {}", v(self.ang_vel)).unwrap();
        writeln!(s, "rot {} {} {}", v(self.rot.forward()), v(self.rot.right()), v(self.rot.up())).unwrap();
        writeln!(s, "boost {}", self.boost).unwrap();
        for (from, c) in &self.controls {
            writeln!(
                s,
                "ctrl {} {} {} {} {} {} {} {} {}",
                from, c.throttle, c.steer, c.pitch, c.yaw, c.roll, c.jump as i32, c.boost as i32, c.handbrake as i32
            )
            .unwrap();
        }
        s
    }
}

fn floor() -> Plane {
    Plane { point: Vec3::ZERO, normal: Vec3::Z }
}

fn c() -> Controls {
    Controls::default()
}

fn throttle(t: f32) -> Controls {
    Controls { throttle: t, ..c() }
}

/// Jump held for `hold` ticks, released for `wait` ticks, then a dodge press with (pitch, yaw, roll)
/// held for the rest of the scenario.
fn dodge_script(mut s: Scenario, base: Controls, pitch: f32, yaw: f32, roll: f32) -> Scenario {
    let (hold, wait) = (6, 6);
    s = s.ctrl(0, Controls { jump: true, ..base });
    s = s.ctrl(hold, base);
    s = s.ctrl(hold + wait, Controls { jump: true, pitch, yaw, roll, ..base });
    s = s.ctrl(hold + wait + 1, Controls { pitch, yaw, roll, ..base });
    s
}

pub fn all() -> Vec<Scenario> {
    let mut v = Vec::new();

    // ------------------------------------------------------------------ ground driving
    v.push(
        Scenario { ticks: 480, ..Scenario::new("boost_to_supersonic", "Full throttle + boost from rest for 4 s on flat ground.", Category::Ground) }
            .ctrl(0, Controls { throttle: 1.0, boost: true, ..c() }),
    );
    v.push(
        Scenario { ticks: 480, ..Scenario::new("throttle_to_max", "Full throttle, no boost, 4 s: approaches the 1410 uu/s throttle cap.", Category::Ground) }
            .ctrl(0, throttle(1.0)),
    );
    v.push(
        Scenario { ticks: 600, ..Scenario::new("coast_brake_reverse", "Throttle 2 s, coast 1 s, brake 0.75 s, reverse 1.25 s.", Category::Ground) }
            .ctrl(0, throttle(1.0))
            .ctrl(240, throttle(0.0))
            .ctrl(360, throttle(-1.0)),
    );
    v.push(
        Scenario { ticks: 360, ..Scenario::new("half_throttle", "Throttle 0.5 for 3 s.", Category::Ground) }.ctrl(0, throttle(0.5)),
    );
    for &(speed, boost) in &[(0.0f32, false), (500.0, false), (1000.0, false), (1400.0, false), (2200.0, true)] {
        let name = format!("turn_{}", speed as i32);
        v.push(
            Scenario {
                ticks: 360,
                vel: Vec3::new(speed, 0.0, 0.0),
                ..Scenario::new(&name, &format!("Full right steer from {speed} uu/s, throttle held{} (turning radius).", if boost { " + boost" } else { "" }), Category::Ground)
            }
            .ctrl(0, Controls { throttle: 1.0, steer: 1.0, boost, ..c() }),
        );
    }
    v.push(
        Scenario { ticks: 300, vel: Vec3::new(1000.0, 0.0, 0.0), ..Scenario::new("turn_left_half", "Half left steer at 1000 uu/s.", Category::Ground) }
            .ctrl(0, Controls { throttle: 1.0, steer: -0.5, ..c() }),
    );
    v.push(
        Scenario { ticks: 300, vel: Vec3::new(1400.0, 0.0, 0.0), ..Scenario::new("powerslide", "Powerslide turn at 1400 uu/s for 1.5 s, then release.", Category::Ground) }
            .ctrl(0, Controls { throttle: 1.0, steer: 1.0, handbrake: true, ..c() })
            .ctrl(180, Controls { throttle: 1.0, steer: 0.0, ..c() }),
    );
    v.push(
        Scenario { ticks: 240, vel: Vec3::new(1500.0, 0.0, 0.0), ..Scenario::new("powerslide_coast", "Powerslide with no throttle at 1500 uu/s.", Category::Ground) }
            .ctrl(0, Controls { steer: -1.0, handbrake: true, ..c() }),
    );
    v.push(
        Scenario { ticks: 300, rot: RotMat::from_angles(0.7, 0.0, 0.0), ..Scenario::new("slalom", "Alternating steer every 0.5 s with throttle, car yawed 0.7 rad.", Category::Ground) }
            .ctrl(0, Controls { throttle: 1.0, steer: 1.0, ..c() })
            .ctrl(60, Controls { throttle: 1.0, steer: -1.0, ..c() })
            .ctrl(120, Controls { throttle: 1.0, steer: 1.0, ..c() })
            .ctrl(180, Controls { throttle: 1.0, steer: -1.0, ..c() })
            .ctrl(240, Controls { throttle: 1.0, steer: 0.0, ..c() }),
    );
    v.push(Scenario { ticks: 120, pos: Vec3::new(0.0, 0.0, 60.0), ..Scenario::new("settle_drop", "Car dropped from z=60 onto its wheels, no input (suspension settle).", Category::Ground) });

    for p in HitboxPreset::ALL {
        let name = format!("preset_{}", p.name());
        v.push(
            Scenario { preset: p, ticks: 300, ..Scenario::new(&name, &format!("{}: boost-drive 1 s, steer, then jump.", p.name()), Category::Ground) }
                .ctrl(0, Controls { throttle: 1.0, boost: true, ..c() })
                .ctrl(120, Controls { throttle: 1.0, steer: 0.6, ..c() })
                .ctrl(200, Controls { throttle: 1.0, jump: true, ..c() })
                .ctrl(215, throttle(1.0)),
        );
    }

    // ------------------------------------------------------------------ jumps
    v.push(Scenario { ticks: 180, ..Scenario::new("jump_full", "Hold jump for 0.25 s (full-height single jump).", Category::Air) }.ctrl(0, Controls { jump: true, ..c() }).ctrl(30, c()));
    v.push(Scenario { ticks: 150, ..Scenario::new("jump_short", "Tap jump for one tick (minimum-height jump).", Category::Air) }.ctrl(0, Controls { jump: true, ..c() }).ctrl(1, c()));
    v.push(
        Scenario { ticks: 240, ..Scenario::new("double_jump", "Full jump, then second jump press at 0.3 s.", Category::Air) }
            .ctrl(0, Controls { jump: true, ..c() })
            .ctrl(26, c())
            .ctrl(36, Controls { jump: true, ..c() })
            .ctrl(38, c()),
    );
    v.push(
        Scenario { ticks: 300, ..Scenario::new("double_jump_late", "Second jump at 1.3 s after leaving the ground (outside the 1.25 s window: no double jump).", Category::Air) }
            .ctrl(0, Controls { jump: true, ..c() })
            .ctrl(24, c())
            .ctrl(180, Controls { jump: true, ..c() })
            .ctrl(182, c()),
    );

    // ------------------------------------------------------------------ dodges
    for &(name, pitch, yaw, roll) in &[
        ("dodge_forward", -1.0, 0.0, 0.0),
        ("dodge_backward", 1.0, 0.0, 0.0),
        ("dodge_left", 0.0, -1.0, 0.0),
        ("dodge_right", 0.0, 1.0, 0.0),
        ("dodge_diag_fr", -0.7071, 0.7071, 0.0),
        ("dodge_diag_bl", 0.7071, -0.7071, 0.0),
        ("dodge_roll_right", 0.0, 0.0, 1.0),
    ] {
        let s = Scenario { ticks: 180, ..Scenario::new(name, &format!("Jump then dodge (pitch {pitch}, yaw {yaw}, roll {roll}) from rest, through the landing."), Category::Air) };
        v.push(dodge_script(s, c(), pitch, yaw, roll));
    }
    for &(name, pitch, yaw) in &[("dodge_forward_moving", -1.0, 0.0), ("dodge_backward_moving", 1.0, 0.0), ("dodge_side_moving", 0.0, 1.0)] {
        let s = Scenario {
            ticks: 150,
            vel: Vec3::new(1200.0, 0.0, 0.0),
            
            ..Scenario::new(name, &format!("Dodge (pitch {pitch}, yaw {yaw}) while driving at 1200 uu/s."), Category::Air)
        };
        v.push(dodge_script(s, throttle(1.0), pitch, yaw, 0.0));
    }
    v.push(dodge_script(
        Scenario { ticks: 120, ..Scenario::new("stall", "Stall: dodge press with yaw and opposite roll (no flip direction).", Category::Air) },
        c(),
        0.0,
        -1.0,
        1.0,
    ));
    {
        // Flip cancel: backflip, then stick forward after 0.1 s (half-flip core).
        let s = Scenario { ticks: 150, vel: Vec3::new(-600.0, 0.0, 0.0), ..Scenario::new("half_flip", "Half-flip: backflip while reversing, cancel with stick forward, air-roll right.", Category::Air) };
        let s = s
            .ctrl(0, Controls { throttle: -1.0, jump: true, ..c() })
            .ctrl(5, throttle(-1.0))
            .ctrl(8, Controls { jump: true, pitch: 1.0, ..c() })
            .ctrl(9, Controls { pitch: 1.0, ..c() })
            .ctrl(20, Controls { pitch: -1.0, throttle: 1.0, ..c() })
            .ctrl(40, Controls { pitch: -1.0, roll: 1.0, throttle: 1.0, ..c() })
            .ctrl(90, Controls { throttle: 1.0, ..c() });
        v.push(s);
    }

    // ------------------------------------------------------------------ aerial control (no floor)
    let air = |name: &str, desc: &str, ctl: Controls| {
        Scenario { ticks: 240, planes: vec![], pos: Vec3::new(0.0, 0.0, 1000.0), ..Scenario::new(name, desc, Category::Air) }.ctrl(0, ctl)
    };
    v.push(air("air_pitch", "Sustained pitch up in the air for 2 s.", Controls { pitch: 1.0, ..c() }));
    v.push(air("air_yaw", "Sustained yaw right in the air for 2 s.", Controls { yaw: 1.0, ..c() }));
    v.push(air("air_roll", "Sustained air roll left in the air for 2 s.", Controls { roll: -1.0, ..c() }));
    v.push(air("air_combo", "Pitch + yaw + roll together (hits the 5.5 rad/s cap).", Controls { pitch: -0.6, yaw: 0.8, roll: 1.0, ..c() }));
    v.push(air("air_boost_pitch", "Aerial: boost while pitching up, then release pitch.", Controls { pitch: 1.0, boost: true, ..c() }).ctrl(60, Controls { boost: true, ..c() }));
    v.push(
        Scenario { ang_vel: Vec3::new(2.0, -3.0, 1.0), ..air("air_damping", "Free spin with no input (angular damping only applies to roll), throttle in air.", throttle(1.0)) },
    );

    // ------------------------------------------------------------------ walls / ceiling
    let wall = Plane { point: Vec3::new(4096.0, 0.0, 0.0), normal: Vec3::new(-1.0, 0.0, 0.0) };
    let ceiling = Plane { point: Vec3::new(0.0, 0.0, 2048.0), normal: Vec3::new(0.0, 0.0, -1.0) };
    // Car on the +X wall, wheels on the wall: up = -X.
    let on_wall_up = RotMat(rl_car_core::Mat3::from_cols(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(-1.0, 0.0, 0.0)));
    let on_wall_side = RotMat(rl_car_core::Mat3::from_cols(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, -1.0), Vec3::new(-1.0, 0.0, 0.0)));
    v.push(
        Scenario {
            ticks: 240,
            planes: vec![floor(), wall],
            pos: Vec3::new(4096.0 - 17.0, 0.0, 600.0),
            vel: Vec3::new(0.0, 0.0, 800.0),
            rot: on_wall_up,
            ..Scenario::new("wall_drive_up", "Driving straight up a wall at 800 uu/s with throttle (sticky force on a vertical surface).", Category::Wall)
        }
        .ctrl(0, throttle(1.0)),
    );
    v.push(
        Scenario {
            ticks: 300,
            planes: vec![floor(), wall],
            pos: Vec3::new(4096.0 - 17.0, 0.0, 800.0),
            vel: Vec3::new(0.0, 1000.0, 0.0),
            rot: on_wall_side,
            ..Scenario::new("wall_drive_side", "Driving horizontally along a wall at 1000 uu/s with throttle and slight steer up.", Category::Wall)
        }
        .ctrl(0, Controls { throttle: 1.0, steer: -0.15, ..c() }),
    );
    v.push(
        Scenario {
            ticks: 180,
            planes: vec![floor(), wall],
            pos: Vec3::new(4096.0 - 17.0, 0.0, 800.0),
            vel: Vec3::new(0.0, 1000.0, 0.0),
            rot: on_wall_side,
            
            ..Scenario::new("wall_no_throttle", "Same as wall_drive_side but no throttle: car loses stick and falls off.", Category::Wall)
        },
    );
    v.push(
        Scenario {
            ticks: 180,
            planes: vec![floor(), ceiling],
            pos: Vec3::new(0.0, 0.0, 2048.0 - 17.0),
            vel: Vec3::new(1000.0, 0.0, 0.0),
            rot: RotMat::from_angles(0.0, 0.0, core::f32::consts::PI),
            
            ..Scenario::new("ceiling_drive", "Upside down on the ceiling with throttle (sticky force is not enough: car drops).", Category::Wall)
        }
        .ctrl(0, throttle(1.0)),
    );
    v.push(
        Scenario {
            ticks: 120,
            planes: vec![floor(), wall],
            pos: Vec3::new(4096.0 - 17.0, 0.0, 600.0),
            vel: Vec3::new(0.0, 500.0, 300.0),
            rot: on_wall_side,
            
            ..Scenario::new("wall_jump", "Jump off a wall.", Category::Wall)
        }
        .ctrl(0, throttle(1.0))
        .ctrl(20, Controls { throttle: 1.0, jump: true, ..c() })
        .ctrl(30, throttle(1.0)),
    );

    // ------------------------------------------------------------------ body contacts
    v.push(Scenario {
        ticks: 180,
        pos: Vec3::new(0.0, 0.0, 300.0),
        rot: RotMat::from_angles(0.0, 0.0, core::f32::consts::PI),
        
        ..Scenario::new("drop_upside_down", "Dropped upside down from z=300 onto the floor (roof hits the ground).", Category::Body)
    });
    v.push(
        Scenario {
            ticks: 160,
            pos: Vec3::new(0.0, 0.0, 300.0),
            rot: RotMat::from_angles(0.0, 0.0, core::f32::consts::PI),
            
            ..Scenario::new("turtle_autoflip", "Upside down on the floor, press jump: auto-flip back onto the wheels.", Category::Body)
        }
        .ctrl(100, Controls { jump: true, ..c() })
        .ctrl(103, c()),
    );
    v.push(Scenario {
        ticks: 120,
        planes: vec![floor(), wall],
        pos: Vec3::new(3500.0, 0.0, 17.0),
        vel: Vec3::new(1500.0, 0.0, 0.0),
        
        ..Scenario::new("hit_wall", "Driving head-on into a wall at 1500 uu/s (body restitution 0.3, friction 0.3).", Category::Body)
    });
    v.push(Scenario {
        ticks: 150,
        pos: Vec3::new(0.0, 0.0, 250.0),
        vel: Vec3::new(600.0, 0.0, 0.0),
        rot: RotMat::from_angles(0.0, 0.3, 0.4),
        
        ..Scenario::new("land_tilted", "Falling onto the floor tilted (pitch 0.3, roll 0.4) while moving.", Category::Body)
    });
    v.push(
        Scenario {
            ticks: 240,
            pos: Vec3::new(0.0, 0.0, 60.0),
            rot: RotMat::from_angles(0.3, 0.0, core::f32::consts::FRAC_PI_2),
            ..Scenario::new("side_autoroll", "Lying on its side with throttle held: auto-roll pushes the car back onto its wheels.", Category::Body)
        }
        .ctrl(0, throttle(1.0)),
    );
    v.push(
        Scenario {
            ticks: 240,
            pos: Vec3::new(0.0, 0.0, 400.0),
            vel: Vec3::new(300.0, 0.0, 0.0),
            ang_vel: Vec3::new(4.0, 2.0, 0.0),
            ..Scenario::new("tumble_landing", "Spinning car falling from z=400 onto the floor and tumbling to rest.", Category::Body)
        },
    );

    // ------------------------------------------------------------------ surface transitions
    let ramp45 = Plane { point: Vec3::new(1000.0, 0.0, 0.0), normal: Vec3::new(-core::f32::consts::FRAC_1_SQRT_2, 0.0, core::f32::consts::FRAC_1_SQRT_2) };
    v.push(
        Scenario {
            ticks: 300,
            planes: vec![floor(), ramp45],
            vel: Vec3::new(800.0, 0.0, 0.0),
            ..Scenario::new("ramp_45", "Driving from the floor onto a 45-degree ramp (sharp crease) with throttle, wheels re-orient onto the new surface.", Category::Wall)
        }
        .ctrl(0, throttle(1.0)),
    );
    let ramp30 = Plane { point: Vec3::new(600.0, 0.0, 0.0), normal: Vec3::new(-0.5, 0.0, 0.866_025_4) };
    v.push(
        Scenario {
            ticks: 300,
            planes: vec![floor(), ramp30],
            vel: Vec3::new(1200.0, 300.0, 0.0),
            ..Scenario::new("ramp_30_boost", "Boosting diagonally onto a 30-degree ramp with steering.", Category::Wall)
        }
        .ctrl(0, Controls { throttle: 1.0, boost: true, steer: 0.3, ..c() }),
    );

    // ------------------------------------------------------------------ mixed
    v.push(
        Scenario { ticks: 360, ..Scenario::new("aerial_takeoff", "Jump, then boost while pitching up and holding jump (fast aerial), then air roll.", Category::Air) }
            .ctrl(0, Controls { jump: true, ..c() })
            .ctrl(12, Controls { jump: false, pitch: 1.0, boost: true, ..c() })
            .ctrl(16, Controls { jump: true, pitch: 1.0, boost: true, ..c() })
            .ctrl(18, Controls { pitch: 1.0, boost: true, ..c() })
            .ctrl(40, Controls { boost: true, roll: 1.0, ..c() })
            .ctrl(120, Controls { yaw: -1.0, ..c() }),
    );
    v.push(
        Scenario { ticks: 180, boost: 5.0, ..Scenario::new("boost_runs_out", "Boosting with only 5 boost: minimum boost time and running empty.", Category::Ground) }
            .ctrl(0, Controls { throttle: 1.0, boost: true, ..c() }),
    );
    v.push(
        Scenario { ticks: 240, vel: Vec3::new(1300.0, 0.0, 0.0), ..Scenario::new("jump_while_turning", "Jumping mid-turn at 1300 uu/s and landing.", Category::Air) }
            .ctrl(0, Controls { throttle: 1.0, steer: 1.0, ..c() })
            .ctrl(40, Controls { throttle: 1.0, steer: 1.0, jump: true, ..c() })
            .ctrl(50, Controls { throttle: 1.0, steer: 1.0, ..c() }),
    );

    v
}
