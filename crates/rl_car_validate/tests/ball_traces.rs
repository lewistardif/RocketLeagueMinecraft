use rl_car_validate::ball_scenarios as bs;
use rl_car_validate::trace_path;

#[test]
fn ball_and_car_match_rocketsim_traces() {
    let mut failures = Vec::new();
    for sc in bs::all() {
        let path = trace_path(sc.name());
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let oracle = bs::parse_ball_trace(&text).unwrap();
        assert_eq!(oracle.1.len() as u32, sc.base.ticks + 1, "{}: trace length", sc.name());
        let core = bs::run_core(&sc);
        let cmp = bs::compare_ball(&sc, &core, &oracle);
        if !bs::passes(&cmp) || cmp.car.as_ref().is_some_and(|c| c.flag_mismatches > 0) {
            failures.push(format!("{}: {cmp:?}", sc.name()));
        }
    }
    assert!(failures.is_empty(), "scenarios out of tolerance:\n{}", failures.join("\n"));
}
