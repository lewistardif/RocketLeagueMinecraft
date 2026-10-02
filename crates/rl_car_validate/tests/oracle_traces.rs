//! Compares the core against the committed RocketSim traces in `validation/traces`.
//! Regenerate the traces with `oracle/build.sh` + `cargo run -p rl_car_validate --release -- gen`.

use rl_car_validate::*;

#[test]
fn core_matches_rocketsim_traces() {
    let mut failures = Vec::new();
    for sc in scenarios::all() {
        let path = trace_path(&sc.name);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let oracle = parse_trace(&text).unwrap();
        assert_eq!(oracle.len() as u32, sc.ticks + 1, "{}: trace length", sc.name);
        let cmp = compare(&run_core(&sc), &oracle, sc.compare_ticks);
        if !cmp.passes(tolerance(sc.category)) || cmp.flag_mismatches > 0 {
            failures.push(format!("{}: {cmp:?}", sc.name));
        }
    }
    assert!(failures.is_empty(), "scenarios out of tolerance:\n{}", failures.join("\n"));
}
