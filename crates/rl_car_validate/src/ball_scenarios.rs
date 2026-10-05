use crate::scenarios::{Category, Scenario};
use crate::{Comparison, Sample, compare, parse_trace};
use rl_car_core::{BallConfig, BallState, Controls, Plane, RotMat, Scene, SimConfig, TICK_DT, Vec3, step_scene};
use std::fmt::Write as _;

#[derive(Clone, Debug)]
pub struct BallScenario {
    pub base: Scenario,
    pub has_car: bool,
    pub ball_pos: Vec3,
    pub ball_vel: Vec3,
    pub ball_ang_vel: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct BallSample {
    pub pos: Vec3,
    pub vel: Vec3,
    pub ang_vel: Vec3,
}

#[derive(Clone, Debug, Default)]
pub struct BallComparison {
    pub car: Option<Comparison>,
    pub max_pos: f32,
    pub max_vel: f32,
    pub max_ang_vel: f32,
    pub diverge_tick: Option<u32>,
}

impl BallScenario {
    fn new(name: &str, description: &str, ticks: u32) -> BallScenario {
        let mut base = Scenario::named(name, description, Category::Body);
        base.ticks = ticks;
        BallScenario { base, has_car: true, ball_pos: Vec3::new(0.0, 0.0, 93.15), ball_vel: Vec3::ZERO, ball_ang_vel: Vec3::ZERO }
    }

    fn ball(mut self, pos: [f32; 3], vel: [f32; 3], ang_vel: [f32; 3]) -> Self {
        self.ball_pos = Vec3::from_array(pos);
        self.ball_vel = Vec3::from_array(vel);
        self.ball_ang_vel = Vec3::from_array(ang_vel);
        self
    }

    fn no_car(mut self) -> Self {
        self.has_car = false;
        self
    }

    fn car(mut self, pos: [f32; 3], vel: [f32; 3], rot: RotMat) -> Self {
        self.base.pos = Vec3::from_array(pos);
        self.base.vel = Vec3::from_array(vel);
        self.base.rot = rot;
        self
    }

    fn ctrl(mut self, from: u32, c: Controls) -> Self {
        self.base.controls.push((from, c));
        self
    }

    fn plane(mut self, point: [f32; 3], normal: [f32; 3]) -> Self {
        self.base.planes.push(Plane { point: Vec3::from_array(point), normal: Vec3::from_array(normal) });
        self
    }

    pub fn name(&self) -> &str {
        &self.base.name
    }

    pub fn to_oracle_text(&self) -> String {
        let mut s = self.base.to_oracle_text();
        let v = |v: Vec3| format!("{} {} {}", v.x, v.y, v.z);
        writeln!(s, "ball {} {} {}", v(self.ball_pos), v(self.ball_vel), v(self.ball_ang_vel)).unwrap();
        if !self.has_car {
            writeln!(s, "nocar").unwrap();
        }
        s
    }

    pub fn initial_scene(&self) -> Scene {
        let mut ball = BallState::new(self.ball_pos);
        ball.velocity = self.ball_vel;
        ball.angular_velocity = self.ball_ang_vel;
        let cars = if self.has_car { vec![self.base.initial_state()] } else { vec![] };
        Scene::new(cars, ball)
    }
}

pub fn run_core(sc: &BallScenario) -> (Vec<Sample>, Vec<BallSample>) {
    let world = sc.base.world();
    let mut scene = sc.initial_scene();
    let (cfg, ball_cfg) = (SimConfig::default(), BallConfig::default());
    let sample = |scene: &Scene, t: u32| {
        let car = scene.cars.first().map(|c| Sample::from_state(t, &c.state));
        let b = &scene.ball;
        (car, BallSample { pos: b.position, vel: b.velocity, ang_vel: b.angular_velocity })
    };
    let mut cars = Vec::new();
    let mut balls = Vec::new();
    let (c0, b0) = sample(&scene, 0);
    cars.extend(c0);
    balls.push(b0);
    for t in 0..sc.base.ticks {
        let c = sc.base.controls_at(t);
        step_scene(&mut scene, &[c], &world, &cfg, &ball_cfg, TICK_DT);
        let (c1, b1) = sample(&scene, t + 1);
        cars.extend(c1);
        balls.push(b1);
    }
    (cars, balls)
}

pub fn parse_ball_trace(text: &str) -> Result<(Vec<Sample>, Vec<BallSample>), String> {
    let cars = parse_trace(text)?;
    let mut balls = Vec::new();
    for (ln, line) in text.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let f: Vec<f32> = line.split(',').map(|x| x.trim().parse::<f32>()).collect::<Result<_, _>>().map_err(|e| format!("line {}: {e}", ln + 1))?;
        if f.len() < 37 {
            return Err(format!("line {}: expected 37 columns, got {}", ln + 1, f.len()));
        }
        let v = |i: usize| Vec3::new(f[i], f[i + 1], f[i + 2]);
        balls.push(BallSample { pos: v(28), vel: v(31), ang_vel: v(34) });
    }
    Ok((cars, balls))
}

pub fn compare_ball(sc: &BallScenario, core: &(Vec<Sample>, Vec<BallSample>), oracle: &(Vec<Sample>, Vec<BallSample>)) -> BallComparison {
    let mut c = BallComparison::default();
    if sc.has_car {
        c.car = Some(compare(&core.0, &oracle.0, sc.base.compare_ticks));
    }
    let n = core.1.len().min(oracle.1.len());
    let n = sc.base.compare_ticks.map_or(n, |l| n.min(l as usize + 1));
    for i in 0..n {
        let (a, b) = (&core.1[i], &oracle.1[i]);
        let pe = (a.pos - b.pos).length();
        c.max_pos = c.max_pos.max(pe);
        c.max_vel = c.max_vel.max((a.vel - b.vel).length());
        c.max_ang_vel = c.max_ang_vel.max((a.ang_vel - b.ang_vel).length());
        if pe > 1.0 && c.diverge_tick.is_none() {
            c.diverge_tick = Some(i as u32);
        }
    }
    c
}

fn throttle(t: f32) -> Controls {
    Controls { throttle: t, ..Default::default() }
}

fn boost() -> Controls {
    Controls { throttle: 1.0, boost: true, ..Default::default() }
}

#[allow(clippy::vec_init_then_push)]
pub fn all() -> Vec<BallScenario> {
    let upside_down = RotMat::from_angles(0.0, 0.0, core::f32::consts::PI);
    let mut v = Vec::new();
    v.push(BallScenario::new("ball_drop", "Ball dropped from 600 uu onto the floor, bouncing until it settles.", 480).ball([0.0, 0.0, 600.0], [0.0, 0.0, -1.0], [0.0; 3]).no_car());
    v.push(BallScenario::new("ball_slide_to_roll", "Ball sliding at 1500 uu/s with no spin; friction turns the slide into a roll.", 360).ball([0.0, 0.0, 91.25], [1500.0, 0.0, 0.0], [0.0; 3]).no_car());
    v.push(BallScenario::new("ball_spin_bounce", "Ball thrown down at an angle with sidespin and topspin.", 360).ball([0.0, 0.0, 400.0], [300.0, 200.0, -800.0], [0.0, 5.0, 2.0]).no_car());
    v.push(
        BallScenario::new("ball_wall_bounce", "Ball hitting a wall at an angle, then the floor.", 300)
            .ball([0.0, 0.0, 300.0], [2000.0, 500.0, 300.0], [1.0, -2.0, 0.0])
            .plane([1500.0, 0.0, 0.0], [-1.0, 0.0, 0.0])
            .no_car(),
    );
    v.push(
        BallScenario::new("ball_corner", "Ball driven into the corner between floor and wall (two surfaces at once).", 240)
            .ball([1300.0, 0.0, 120.0], [2500.0, 0.0, -400.0], [0.0, 4.0, 0.0])
            .plane([1500.0, 0.0, 0.0], [-1.0, 0.0, 0.0])
            .no_car(),
    );
    v.push(
        BallScenario::new("car_hits_resting_ball", "Car boosts into a sleeping kickoff ball.", 240)
            .ball([1200.0, 0.0, 93.15], [0.0; 3], [0.0; 3])
            .ctrl(0, boost()),
    );
    v.push(
        BallScenario::new("car_hits_rolling_ball", "Ball rolling into a car driving towards it, off centre.", 240)
            .ball([1500.0, 60.0, 91.25], [-800.0, 0.0, 0.0], [0.0, -8.0, 0.0])
            .ctrl(0, throttle(1.0)),
    );
    v.push(
        BallScenario::new("ball_lands_on_roof", "Ball dropped onto a slowly accelerating car (dribble start).", 300)
            .ball([0.0, 0.0, 300.0], [0.0, 0.0, -1.0], [0.0; 3])
            .ctrl(0, throttle(0.2)),
    );
    v.push(
        BallScenario::new("car_jump_hit", "Car jumps and boosts into a ball hanging in the air.", 240)
            .ball([500.0, 0.0, 300.0], [0.0; 3], [0.0; 3])
            .ctrl(0, Controls { throttle: 1.0, boost: true, jump: true, ..Default::default() })
            .ctrl(20, boost()),
    );
    v.push(
        BallScenario::new("wheels_on_ball", "Upside-down car rising into a hanging ball with its wheels (flip-reset contact).", 120)
            .ball([0.0, 0.0, 500.0], [0.0; 3], [0.0; 3])
            .car([0.0, 0.0, 330.0], [0.0, 0.0, 500.0], upside_down),
    );
    v.push(
        BallScenario::new("pinch", "Car boosts the ball into a wall.", 150)
            .ball([500.0, 0.0, 93.15], [0.0; 3], [0.0; 3])
            .plane([800.0, 0.0, 0.0], [-1.0, 0.0, 0.0])
            .car([-800.0, 0.0, 17.0], [1800.0, 0.0, 0.0], RotMat::IDENTITY)
            .ctrl(0, boost()),
    );
    v.push(
        BallScenario::new("dodge_into_ball", "Car drives at a resting ball, jumps and front-flips into it.", 240)
            .ball([900.0, 0.0, 93.15], [0.0; 3], [0.0; 3])
            .car([0.0, 0.0, 17.0], [1000.0, 0.0, 0.0], RotMat::IDENTITY)
            .ctrl(0, throttle(1.0))
            .ctrl(30, Controls { throttle: 1.0, jump: true, ..Default::default() })
            .ctrl(36, throttle(1.0))
            .ctrl(42, Controls { throttle: 1.0, jump: true, pitch: -1.0, ..Default::default() })
            .ctrl(50, throttle(1.0)),
    );
    v.push(
        BallScenario::new("corner_hit_turning", "Car turning and boosting clips the ball with its front corner.", 240)
            .ball([1000.0, 250.0, 93.15], [0.0; 3], [0.0; 3])
            .ctrl(0, Controls { throttle: 1.0, boost: true, steer: 0.25, ..Default::default() }),
    );
    v
}

pub fn passes(c: &BallComparison) -> bool {
    let ball_ok = c.max_pos <= 2.5 && c.max_vel <= 25.0 && c.max_ang_vel <= 0.5;
    let car_ok = c.car.as_ref().is_none_or(|k| k.max_pos <= 2.5 && k.max_vel <= 25.0 && k.max_rot_deg <= 2.0);
    ball_ok && car_ok
}
