//! Project creation: schema, the genesis gesture, and the builtins it seeds.

use rev_core::NoteNumber;
use rev_store::{query, schema};
use rev_testkit::TempProject;

const MIDDLE_C: f64 = 261.625_565_300_598_6;

#[test]
fn create_seeds_meta() {
    let temp = TempProject::create().unwrap();
    let reader = temp.project().reader();

    assert_eq!(
        query::meta(reader, schema::META_SCHEMA_VERSION).unwrap(),
        Some(schema::SCHEMA_VERSION.to_string())
    );
    assert_eq!(
        query::meta(reader, schema::META_PPQ).unwrap(),
        Some("5040".to_string())
    );
    assert!(query::meta(reader, schema::META_CREATED).unwrap().is_some());
    assert!(
        query::meta(reader, schema::META_DEFAULT_TUNING_ID)
            .unwrap()
            .is_some()
    );
    assert_eq!(query::meta(reader, "no_such_key").unwrap(), None);
}

#[test]
fn create_seeds_the_builtin_tunings() {
    let temp = TempProject::create().unwrap();
    let reader = temp.project().reader();

    for name in ["12-ET", "16-ET", "Just (5-limit)"] {
        let tuning = query::tuning_by_name(reader, name)
            .unwrap()
            .unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(tuning.spec.origin.as_deref(), Some("builtin"));
        // Every builtin anchors note 60 at middle C, so switching a phrase's
        // tuning keeps its home pitch.
        assert_eq!(tuning.spec.anchor_note, NoteNumber(60));
        assert!((tuning.spec.anchor_freq - MIDDLE_C).abs() < 1e-12);
        // And each is materialized at genesis, ready to resolve.
        assert!(
            query::latest_materialized_instance(reader, tuning.id)
                .unwrap()
                .is_some(),
            "{name} was never materialized"
        );
    }
}

#[test]
fn twelve_equal_resolves_to_concert_pitch() {
    let temp = TempProject::create().unwrap();
    let reader = temp.project().reader();
    let tuning = query::tuning_by_name(reader, "12-ET").unwrap().unwrap();
    let instance = query::latest_materialized_instance(reader, tuning.id)
        .unwrap()
        .unwrap();
    let table = query::materialized_tuning(reader, instance)
        .unwrap()
        .unwrap();

    assert!((table.freq(NoteNumber(69)).unwrap() - 440.0).abs() < 1e-9);
    assert!((table.freq(NoteNumber(60)).unwrap() - MIDDLE_C).abs() < 1e-12);
    assert_eq!(table.note_per_period(), Some(12));
    assert!(table.has_period());
    assert_eq!(table.note_range(), (NoteNumber(0), NoteNumber(127)));
}

#[test]
fn sixteen_equal_resolves_to_seventy_five_cent_steps() {
    let temp = TempProject::create().unwrap();
    let reader = temp.project().reader();
    let tuning = query::tuning_by_name(reader, "16-ET").unwrap().unwrap();
    let instance = query::latest_materialized_instance(reader, tuning.id)
        .unwrap()
        .unwrap();
    let table = query::materialized_tuning(reader, instance)
        .unwrap()
        .unwrap();

    let c4 = table.freq(NoteNumber(60)).unwrap();
    let step = table.freq(NoteNumber(61)).unwrap() / c4;
    let cents = 1200.0 * (step.ln() / std::f64::consts::LN_2);
    assert!((cents - 75.0).abs() < 1e-9, "step is {cents} cents");
    // The anchor is shared with 12-ET: the same note number, the same pitch.
    assert!((c4 - MIDDLE_C).abs() < 1e-12);
}

#[test]
fn just_intonation_resolves_to_exact_ratios() {
    let temp = TempProject::create().unwrap();
    let reader = temp.project().reader();
    let tuning = query::tuning_by_name(reader, "Just (5-limit)")
        .unwrap()
        .unwrap();

    // Twelve canonical rows, stored as exact integer pairs (R-504).
    let note = query::tuning_note(reader, tuning.id).unwrap();
    assert_eq!(note.len(), 12);

    let instance = query::latest_materialized_instance(reader, tuning.id)
        .unwrap()
        .unwrap();
    let table = query::materialized_tuning(reader, instance)
        .unwrap()
        .unwrap();
    let c4 = table.freq(NoteNumber(60)).unwrap();
    let g4 = table.freq(NoteNumber(67)).unwrap();
    assert!((g4 / c4 - 1.5).abs() < 1e-12, "just fifth is {}", g4 / c4);
    // The canonical period extends downward too.
    let c3 = table.freq(NoteNumber(48)).unwrap();
    assert!((c4 / c3 - 2.0).abs() < 1e-12);
}

#[test]
fn create_seeds_the_scale_library() {
    let temp = TempProject::create().unwrap();
    let reader = temp.project().reader();

    let twelve = query::scale_for_period(reader, 12).unwrap();
    let sixteen = query::scale_for_period(reader, 16).unwrap();
    assert_eq!(twelve.len(), 16);
    assert_eq!(sixteen.len(), 6);

    // Chromatic is not a row: a NULL scale binding is chromatic (R-510).
    assert!(query::scale_by_name(reader, "Chromatic").unwrap().is_none());

    let major = query::scale_by_name(reader, "Major (Ionian)")
        .unwrap()
        .unwrap();
    assert_eq!(major.spec.mask, vec![0, 2, 4, 5, 7, 9, 11]);
    // One mask serves every tuning of that periodicity — 12-ET and just
    // intonation alike (R-509).
    assert!(major.spec.applies_to(Some(12)));
    assert!(!major.spec.applies_to(Some(16)));
    assert!(query::scale(reader, major.id).unwrap().is_some());

    let mavila = query::scale_by_name(reader, "Mavila (7)").unwrap().unwrap();
    assert_eq!(mavila.spec.note_per_period, Some(16));
}

#[test]
fn genesis_is_journaled_history_not_a_side_door() {
    // Creation goes through ordinary commands, which is what lets replay
    // rebuild a project from nothing and the interchange format embed tunings
    // with no special case (R-506).
    let temp = TempProject::create().unwrap();
    let entry = rev_store::journal::entry(temp.project().reader()).unwrap();
    assert!(!entry.is_empty());
    assert!(
        entry.iter().all(|e| e.gesture == 1),
        "genesis is one gesture"
    );

    let names: Vec<String> = temp
        .project()
        .reader()
        .prepare("SELECT DISTINCT command FROM journal WHERE command IS NOT NULL ORDER BY command")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(names.contains(&"create_tuning".to_string()));
    assert!(names.contains(&"materialize_tuning".to_string()));
    assert!(names.contains(&"create_scale".to_string()));
    assert!(names.contains(&"set_meta".to_string()));
}

#[test]
fn opening_a_non_project_is_refused() {
    let temp = TempProject::create_bare().unwrap();
    let path = temp.path();
    // Schema but no genesis: no schema_version row, so this is not yet a
    // project as far as `open` is concerned.
    assert!(rev_store::Project::open(&path).is_err());
}
