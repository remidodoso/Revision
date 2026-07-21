use super::*;

fn major() -> ScaleSpec {
    ScaleSpec::periodic("major", 12, vec![0, 2, 4, 5, 7, 9, 11])
}

fn mavila7() -> ScaleSpec {
    ScaleSpec::periodic("mavila7", 16, vec![0, 2, 4, 6, 9, 11, 13])
}

#[test]
fn membership_is_root_relative() {
    let scale = major();
    // C major rooted at 0: E is in, E-flat is out.
    assert!(scale.contains(NoteNumber(64), 0));
    assert!(!scale.contains(NoteNumber(63), 0));
    // The same shape rooted at 2 (D major) admits F-sharp instead.
    assert!(scale.contains(NoteNumber(66), 2));
    assert!(!scale.contains(NoteNumber(65), 2));
}

#[test]
fn membership_holds_below_the_anchor() {
    // Signed note numbers must behave: euclidean modulo, not bare `%`.
    let scale = major();
    assert!(scale.contains(NoteNumber(-12), 0));
    assert!(scale.contains(NoteNumber(-8), 0)); // pitch class 4
    assert!(!scale.contains(NoteNumber(-11), 0)); // pitch class 1
}

#[test]
fn nearest_prefers_the_lower_neighbour() {
    let scale = major();
    // C-sharp is out; C below is as near as D above, so the lower wins.
    assert_eq!(scale.nearest(NoteNumber(61), 0), NoteNumber(60));
    // An in-scale note is its own nearest.
    assert_eq!(scale.nearest(NoteNumber(64), 0), NoteNumber(64));
}

#[test]
fn step_moves_by_scale_degree() {
    let scale = major();
    assert_eq!(scale.step(NoteNumber(60), 0, 1), NoteNumber(62));
    assert_eq!(scale.step(NoteNumber(64), 0, 1), NoteNumber(65)); // E to F: a semitone
    assert_eq!(scale.step(NoteNumber(60), 0, -1), NoteNumber(59));
    // An off-scale note snaps onto the scale in the direction of travel.
    assert_eq!(scale.step(NoteNumber(61), 0, 1), NoteNumber(62));
    assert_eq!(scale.step(NoteNumber(61), 0, -1), NoteNumber(60));
}

#[test]
fn applicability_is_by_modulus_not_tuning() {
    // One mask serves every tuning of the same periodicity — 12-ET and 5-limit
    // just intonation alike. Idiomatic fit is a separate, advisory question.
    assert!(major().applies_to(Some(12)));
    assert!(!major().applies_to(Some(16)));
    assert!(!major().applies_to(None));
    assert!(mavila7().applies_to(Some(16)));
}

#[test]
fn sixteen_tone_masks_work_the_same_way() {
    // Mavila[7] over 16 notes per period: pitch classes 0, 2, 4, 6, 9, 11, 13.
    let scale = mavila7();
    assert!(scale.contains(NoteNumber(64), 0)); // pitch class 0
    assert!(scale.contains(NoteNumber(66), 0)); // pitch class 2
    assert!(!scale.contains(NoteNumber(65), 0)); // pitch class 1
    assert_eq!(scale.step(NoteNumber(64), 0, 1), NoteNumber(66));
}

#[test]
fn validation_rejects_out_of_range_masks() {
    let bad = ScaleSpec::periodic("nonsense", 12, vec![0, 4, 15]);
    assert!(matches!(
        bad.validate(),
        Err(CoreError::ScaleMaskOutOfRange { value: 15, .. })
    ));
    let empty = ScaleSpec::periodic("empty", 12, vec![]);
    assert!(matches!(
        empty.validate(),
        Err(CoreError::ScaleEmpty { .. })
    ));
    assert!(major().validate().is_ok());
}

#[test]
fn aperiodic_scales_are_absolute_subsets() {
    let mut scale = ScaleSpec::periodic("sparse", 12, vec![]);
    scale.note_per_period = None;
    scale.tuning_id = Some(crate::id::TuningId(1));
    scale.mask = vec![10, 12, 15];
    assert!(scale.contains(NoteNumber(12), 0));
    assert!(!scale.contains(NoteNumber(11), 0));
    // Root is irrelevant without periodicity.
    assert!(scale.contains(NoteNumber(12), 7));
    // Nearest and step scan the absolute set.
    assert_eq!(scale.nearest(NoteNumber(11), 0), NoteNumber(10));
    assert_eq!(scale.step(NoteNumber(12), 0, 1), NoteNumber(15));
    assert_eq!(scale.step(NoteNumber(12), 0, -1), NoteNumber(10));
}
