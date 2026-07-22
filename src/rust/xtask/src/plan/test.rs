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
    // is made rather than only in CI. This also exercises filing and links
    // against the live tree — the whole point of misc-05.
    run(real_root()).expect("the plan should be well formed");
}

fn real_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("xtask lives at <root>/src/rust/xtask")
}

// --- misc-05: filing. The rule is asymmetric on purpose, so both directions
// and the cross-reference case that must NOT fire are pinned here.

#[test]
fn a_completed_proposal_left_in_doc_is_the_ui06_bug() {
    // The exact regression: item complete, proposal still loose in doc/.
    let fault = filing_fault("revision_ui06_proposal.md", false, Some(&vec![true]));
    assert!(fault.expect("a fault").contains("still in doc/"));
}

#[test]
fn a_filed_proposal_of_a_live_item_is_correct() {
    // In completed/ needs a finished owner; loose in doc/ needs a live one.
    assert!(filing_fault("p.md", true, Some(&vec![true])).is_none());
    assert!(filing_fault("p.md", false, Some(&vec![false])).is_none());
}

#[test]
fn a_proposal_archived_too_soon_is_caught() {
    // Under completed/ but no owner is finished.
    let fault = filing_fault("p.md", true, Some(&vec![false]));
    assert!(fault.expect("a fault").contains("doc/completed/"));
}

#[test]
fn a_live_owner_keeps_a_shared_proposal_in_doc() {
    // A finished item also cites it, but a live item still needs it: it stays.
    assert!(filing_fault("p.md", false, Some(&vec![true, false])).is_none());
    // And a finished item citing an already-archived proposal is fine.
    assert!(filing_fault("p.md", true, Some(&vec![true, false])).is_none());
}

#[test]
fn a_proposal_no_item_links_is_orphaned() {
    let fault = filing_fault("revision_stray_proposal.md", true, None);
    assert!(fault.expect("a fault").contains("orphaned"));
}

#[test]
fn a_proposal_is_recognised_by_name() {
    assert!(is_proposal("completed/revision_ui06_proposal.md"));
    assert!(is_proposal("revision_dsp02_proposal.md"));
    assert!(!is_proposal("revision_getstarted.md"));
    assert!(!is_proposal("revision_ui06_proposal.txt"));
    assert_eq!(basename("a/b/c.md"), "c.md");
    assert_eq!(basename("c.md"), "c.md");
}

// --- misc-05: link scanning. Fake paths are built from parts so the literal
// `doc/<name>` never appears in this file's source — the real-tree scan reads
// it too, and an unresolvable citation here would fail that check.

#[test]
fn a_citation_is_found_and_shaped() {
    let slash = "doc/";
    let text =
        format!("//! Approved at eng-02; see `{slash}completed/revision_eng02_proposal.md`.");
    let found = doc_citations(&text);
    assert_eq!(
        found,
        vec![format!("{slash}completed/revision_eng02_proposal.md")]
    );
}

#[test]
fn a_trailing_period_is_not_part_of_the_extension() {
    let slash = "doc/";
    // Prose ending a sentence right after the file name.
    let text = format!("see {slash}revision_plan.json.");
    assert_eq!(
        doc_citations(&text),
        vec![format!("{slash}revision_plan.json")]
    );
}

#[test]
fn two_citations_on_one_line_both_land() {
    let slash = "doc/";
    let text = format!("{slash}revision_a.md and {slash}revision_b.json");
    assert_eq!(
        doc_citations(&text),
        vec![
            format!("{slash}revision_a.md"),
            format!("{slash}revision_b.json")
        ]
    );
}

#[test]
fn a_bare_directory_mention_is_not_a_citation() {
    let slash = "doc/";
    let text = format!("the {slash} tree, and {slash}completed/ within it");
    assert!(doc_citations(&text).is_empty());
}

#[test]
fn json_config_paths_are_citations_too() {
    // Not only proposals — the real string literals that read the plan itself.
    let slash = "doc/";
    let text = format!("read(root, \"{slash}revision_plan.json\")");
    assert_eq!(
        doc_citations(&text),
        vec![format!("{slash}revision_plan.json")]
    );
}
