use super::*;

use serde_json::json;

fn problem_of(entry: serde_json::Value) -> Vec<String> {
    let mut problem = Vec::new();
    check_entry(&entry, "here", &mut problem);
    problem
}

#[test]
fn a_well_formed_entry_passes() {
    assert!(problem_of(json!({"ts": "2026-07-21", "text": "something"})).is_empty());
}

#[test]
fn the_mistake_that_caused_this_is_caught() {
    // `at` instead of `ts`. It rendered as the literal string "undefined" at
    // the top of the plan and nothing noticed until it was read by eye.
    let problem = problem_of(json!({"at": "2026-07-21", "text": "something"}));
    assert_eq!(problem.len(), 1);
    assert!(problem[0].contains("no `ts`"), "{problem:?}");
}

#[test]
fn a_bare_string_where_an_entry_belongs_is_caught() {
    // The other half of the same mistake: item notes written as plain strings,
    // which the viewer would have rendered as "[undefined] undefined".
    let problem = problem_of(json!("just a string"));
    assert_eq!(problem.len(), 1);
    assert!(problem[0].contains("not an object"), "{problem:?}");
}

#[test]
fn a_timestamp_has_to_be_a_date() {
    assert!(is_date("2026-07-21"));
    assert!(!is_date("2026-7-21"));
    assert!(!is_date("yesterday"));
    assert!(!is_date("2026-07-21T19:45:13Z"));
    assert!(problem_of(json!({"ts": "soon", "text": "x"}))[0].contains("not YYYY-MM-DD"));
}

#[test]
fn an_empty_text_is_not_an_entry() {
    assert!(problem_of(json!({"ts": "2026-07-21", "text": ""}))[0].contains("no `text`"));
}

#[test]
fn the_real_plan_is_well_formed() {
    // The check against the actual file, so the suite fails where the mistake
    // is made rather than only in CI.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("xtask lives at <root>/src/rust/xtask");
    run(root).expect("the plan should be well formed");
}
