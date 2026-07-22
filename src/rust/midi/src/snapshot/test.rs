use super::*;

use rev_core::NoteNumber;
use rev_core::tuning::{MaterializedTuning, Ratio, TuningKind, TuningSpec, materialize};

/// 12-ET anchored at A4 = 440 (note 69), materialized over the MIDI range.
fn twelve_et() -> MaterializedTuning {
    let mut spec = TuningSpec::new("12-ET", TuningKind::Equal, 69, 440.0);
    spec.period = Some(Ratio::new(2, 1));
    spec.note_per_period = Some(12);
    spec.note_min = Some(NoteNumber(0));
    spec.note_max = Some(NoteNumber(127));
    materialize(&spec, &[]).expect("materialize")
}

#[test]
fn a_key_resolves_exactly_as_the_tuning_does() {
    // The identity that makes what-you-play-is-what-you-hear structural: the
    // snapshot is the same `freq` table the compiler and the roll read.
    let tuning = twelve_et();
    let snap = NoteHz::from_tuning(&tuning);
    for key in 0..128u8 {
        let expect = tuning.freq(NoteNumber(i32::from(key)));
        match (snap.resolve(key), expect) {
            (Some(hz), Some(f)) => assert!((f64::from(hz) - f).abs() < 0.01, "key {key}"),
            (None, Some(f)) => assert!(f <= 0.0, "key {key} lost a real frequency"),
            (None, None) => {}
            (Some(_), None) => panic!("key {key} invented a frequency"),
        }
    }
}

#[test]
fn a_reference_key_is_the_reference_pitch() {
    let snap = NoteHz::from_tuning(&twelve_et());
    let a4 = snap.resolve(69).expect("A4 resolves");
    assert!((a4 - 440.0).abs() < 0.01, "A4 = {a4}");
    let a5 = snap.resolve(81).expect("A5 resolves");
    assert!((a5 - 880.0).abs() < 0.02, "an octave up is double: {a5}");
}

#[test]
fn the_silent_snapshot_sounds_nothing() {
    let snap = NoteHz::silent();
    for key in 0..128u8 {
        assert!(snap.resolve(key).is_none(), "key {key} should be silent");
    }
}

#[test]
fn an_out_of_range_key_is_silence_not_a_panic() {
    let snap = NoteHz::from_tuning(&twelve_et());
    assert!(snap.resolve(200).is_none());
    assert!(snap.resolve(255).is_none());
}
