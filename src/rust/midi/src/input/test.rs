use super::*;

use std::time::Instant;

use rev_core::NoteNumber;
use rev_core::tuning::{Ratio, TuningKind, TuningSpec, materialize};
use rev_engine::{Live, session_with_thru};

use crate::event::Message;

fn twelve_et_snapshot() -> NoteHz {
    let mut spec = TuningSpec::new("12-ET", TuningKind::Equal, 69, 440.0);
    spec.period = Some(Ratio::OCTAVE);
    spec.note_per_period = Some(12);
    spec.note_min = Some(NoteNumber(0));
    spec.note_max = Some(NoteNumber(127));
    NoteHz::from_tuning(&materialize(&spec, &[]).expect("materialize"))
}

/// A fork whose thru ring the engine's `RtPort` would drain — here drained
/// directly, so the test sees the physics that reaches the voice.
fn fork() -> (Fork, Events, rev_engine::RtPort) {
    let (_app, rt, thru) = session_with_thru();
    let (fork, events) = Fork::new(thru, twelve_et_snapshot(), Instant::now());
    (fork, events, rt)
}

/// The raw bytes of a note-on / note-off.
fn note_on(channel: u8, note: u8, vel: u8) -> [u8; 3] {
    [0x90 | channel, note, vel]
}
fn note_off(channel: u8, note: u8) -> [u8; 3] {
    [0x80 | channel, note, 0]
}

#[test]
fn a_note_forks_into_physics_and_the_model() {
    let (mut fork, mut events, mut rt) = fork();
    fork.on_message(&note_on(0, 69, 100)); // A4

    // --- the model's side: a note number (R-002), stamped.
    let captured = events.take().expect("an event reached the app");
    assert!(matches!(
        captured.message,
        Message::NoteOn { note, .. } if note == NoteNumber(69)
    ));

    // --- physics: the engine gets a frequency (R-312), not a note number.
    let live = rt.next_live().expect("a live note reached the engine");
    match live {
        Live::NoteOn { hz, level, .. } => {
            assert!((hz - 440.0).abs() < 0.02, "A4 resolved to {hz}");
            assert!((level - 100.0 / 127.0).abs() < 0.01, "velocity to level");
        }
        other => panic!("expected a note-on, got {other:?}"),
    }
}

#[test]
fn note_off_pairs_by_the_same_opaque_key() {
    let (mut fork, _events, mut rt) = fork();
    fork.on_message(&note_on(3, 60, 80));
    fork.on_message(&note_off(3, 60));

    let on = rt.next_live().expect("on");
    let off = rt.next_live().expect("off");
    let (on_key, off_key) = match (on, off) {
        (Live::NoteOn { key: a, .. }, Live::NoteOff { key: b }) => (a, b),
        _ => panic!("expected on then off, got {on:?} {off:?}"),
    };
    assert_eq!(on_key, off_key, "the off pairs to its on by key");
}

#[test]
fn a_note_on_with_velocity_zero_is_a_note_off() {
    // Every keyboard does this; treating it as a note-on strands the note.
    let (mut fork, _events, mut rt) = fork();
    fork.on_message(&note_on(0, 64, 0)); // velocity 0
    match rt.next_live().expect("something reached the engine") {
        Live::NoteOff { .. } => {}
        other => panic!("velocity-0 note-on should be a note-off, got {other:?}"),
    }
}

#[test]
fn a_key_that_resolves_to_nothing_sends_no_physics_but_still_records() {
    // The silent snapshot: no key resolves, so nothing sounds — but the model
    // still sees the event, because capture is about what was played, not what
    // was heard.
    let (_app, rt, thru) = session_with_thru();
    let mut rt = rt;
    let (mut fork, mut events) = Fork::new(thru, NoteHz::silent(), Instant::now());
    fork.on_message(&note_on(0, 60, 100));
    assert!(rt.next_live().is_none(), "nothing to sound");
    assert!(events.take().is_some(), "but it was still captured");
}

#[test]
fn a_note_off_always_reaches_the_engine_even_unresolved() {
    // A note-off must go through regardless, or a note held before a snapshot
    // swap could never be released.
    let (_app, rt, thru) = session_with_thru();
    let mut rt = rt;
    let (mut fork, _events) = Fork::new(thru, NoteHz::silent(), Instant::now());
    fork.on_message(&note_off(0, 60));
    assert!(matches!(rt.next_live(), Some(Live::NoteOff { .. })));
}

#[test]
fn swapping_the_snapshot_remaps_the_next_key() {
    // The whole of midi-04 in miniature: same key, different snapshot, different
    // frequency — swapped live, effective on the next note.
    let (mut fork, _events, mut rt) = fork();
    fork.on_message(&note_on(0, 69, 100));
    let before = match rt.next_live() {
        Some(Live::NoteOn { hz, .. }) => hz,
        _ => panic!(),
    };

    // A snapshot an octave up: note 69 now resolves to 880.
    let mut spec = TuningSpec::new("12-ET+8", TuningKind::Equal, 69, 880.0);
    spec.period = Some(Ratio::OCTAVE);
    spec.note_per_period = Some(12);
    spec.note_min = Some(NoteNumber(0));
    spec.note_max = Some(NoteNumber(127));
    fork.set_snapshot(NoteHz::from_tuning(&materialize(&spec, &[]).expect("m")));

    fork.on_message(&note_on(0, 69, 100));
    let after = match rt.next_live() {
        Some(Live::NoteOn { hz, .. }) => hz,
        _ => panic!(),
    };
    assert!((before - 440.0).abs() < 0.02);
    assert!((after - 880.0).abs() < 0.04, "remapped to {after}");
}

#[test]
fn a_malformed_message_is_ignored() {
    let (mut fork, mut events, mut rt) = fork();
    fork.on_message(&[0x90]); // truncated
    fork.on_message(&[]); // empty
    assert!(rt.next_live().is_none());
    assert!(events.take().is_none());
}
