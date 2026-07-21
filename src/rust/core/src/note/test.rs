use super::*;

#[test]
fn pitch_class_is_euclidean_below_the_anchor() {
    // The whole point of the Arithmetic law: bare `%` would give -7 here.
    assert_eq!(NoteNumber(-7).pitch_class(12), 5);
    assert_eq!(NoteNumber(-12).pitch_class(12), 0);
    assert_eq!(NoteNumber(-1).pitch_class(16), 15);
}

#[test]
fn pitch_class_above_the_anchor() {
    assert_eq!(NoteNumber(60).pitch_class(12), 0);
    assert_eq!(NoteNumber(64).pitch_class(12), 4);
    assert_eq!(NoteNumber(60).pitch_class(16), 12);
}

#[test]
fn period_index_walks_both_directions() {
    let anchor = NoteNumber(60);
    assert_eq!(NoteNumber(60).period_index(anchor, 12), 0);
    assert_eq!(NoteNumber(71).period_index(anchor, 12), 0);
    assert_eq!(NoteNumber(72).period_index(anchor, 12), 1);
    assert_eq!(NoteNumber(59).period_index(anchor, 12), -1);
    assert_eq!(NoteNumber(48).period_index(anchor, 12), -1);
    assert_eq!(NoteNumber(47).period_index(anchor, 12), -2);
}

#[test]
#[should_panic(expected = "positive modulus")]
fn pitch_class_rejects_zero_modulus() {
    NoteNumber(60).pitch_class(0);
}
