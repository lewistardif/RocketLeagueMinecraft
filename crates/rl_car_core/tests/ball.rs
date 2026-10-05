use rl_car_core::*;

fn drive(t: u32) -> Controls {
    Controls {
        throttle: 1.0,
        boost: t % 300 < 200,
        steer: if t % 400 > 300 { 0.5 } else { 0.0 },
        jump: t % 240 < 8,
        pitch: if t % 240 < 30 { -0.5 } else { 0.0 },
        ..Default::default()
    }
}

#[test]
fn car_is_unchanged_when_the_ball_is_out_of_reach() {
    let world = PlaneWorld::soccar_box();
    let mut car = CarState::new(HitboxPreset::Octane);
    car.position = Vec3::new(0.0, -4000.0, 17.0);
    let mut ball = BallState::new(Vec3::new(3000.0, 4000.0, 800.0));
    ball.velocity = Vec3::new(-300.0, 100.0, 0.0);
    let mut scene = Scene::new(vec![car], ball);
    let (cfg, ball_cfg) = (SimConfig::default(), BallConfig::default());
    for t in 0..600 {
        let c = drive(t);
        car = step(&car, &c, &world, TICK_DT);
        step_scene(&mut scene, &[c], &world, &cfg, &ball_cfg, TICK_DT);
        assert_eq!(scene.cars[0].state, car, "diverged at tick {t}");
    }
    assert!(scene.ball.position.z < 200.0, "ball should have fallen and be bouncing");
}

#[test]
fn ball_bounce_keeps_sixty_percent_of_its_speed() {
    let world = PlaneWorld::floor();
    let mut ball = BallState::new(Vec3::new(0.0, 0.0, 1000.0));
    ball.velocity = Vec3::new(0.0, 0.0, -1.0);
    let mut scene = Scene::new(vec![], ball);
    let (cfg, ball_cfg) = (SimConfig::default(), BallConfig::default());
    let mut before = 0.0;
    let mut after = None;
    for _ in 0..240 {
        let v = scene.ball.velocity.z;
        step_scene(&mut scene, &[], &world, &cfg, &ball_cfg, TICK_DT);
        if v < 0.0 && scene.ball.velocity.z > 0.0 {
            before = v;
            after = Some(scene.ball.velocity.z);
            break;
        }
    }
    let ratio = after.expect("ball bounced") / -before;
    assert!((0.55..0.65).contains(&ratio), "bounce ratio {ratio}");
}

#[test]
fn sleeping_ball_floats_until_hit() {
    let world = PlaneWorld::floor();
    let car = CarState::new(HitboxPreset::Octane);
    let mut scene = Scene::new(vec![car], BallState::new(Vec3::new(1000.0, 0.0, 300.0)));
    let (cfg, ball_cfg) = (SimConfig::default(), BallConfig::default());
    for _ in 0..60 {
        step_scene(&mut scene, &[Controls::default()], &world, &cfg, &ball_cfg, TICK_DT);
    }
    assert_eq!(scene.ball.position, Vec3::new(1000.0, 0.0, 300.0));
    let boost = Controls { throttle: 1.0, boost: true, jump: true, ..Default::default() };
    let mut touched = false;
    for _ in 0..240 {
        step_scene(&mut scene, &[boost], &world, &cfg, &ball_cfg, TICK_DT);
        touched |= scene.cars[0].touched_ball;
    }
    assert!(touched);
    assert!(scene.ball.velocity.x > 500.0, "ball was hit forward: {:?}", scene.ball.velocity);
}

#[test]
fn ball_never_exceeds_its_speed_cap() {
    let world = PlaneWorld::soccar_box();
    let mut ball = BallState::new(Vec3::new(0.0, 0.0, 1000.0));
    ball.velocity = Vec3::new(9000.0, 7000.0, 3000.0);
    ball.angular_velocity = Vec3::new(20.0, 0.0, 0.0);
    let mut scene = Scene::new(vec![], ball);
    let (cfg, ball_cfg) = (SimConfig::default(), BallConfig::default());
    for _ in 0..600 {
        step_scene(&mut scene, &[], &world, &cfg, &ball_cfg, TICK_DT);
        assert!(scene.ball.velocity.length() <= 6000.5);
        assert!(scene.ball.angular_velocity.length() <= 6.0001);
        assert!(scene.ball.position.x.abs() < 4200.0 && scene.ball.position.y.abs() < 5200.0 && scene.ball.position.z > 0.0 && scene.ball.position.z < 2100.0);
    }
}
