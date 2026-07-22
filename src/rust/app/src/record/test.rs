//! Recorder tests (rec-01 §7).
//!
//! Placement and pairing are pure — they need a synthetic [`Position`] stream
//! and a [`Captured`] stream, no device and no store. Mode and durability need a
//! real [`Project`], because clearing and journaling are what they are about;
//! those build a throwaway project in a temp dir, the same pattern the demo
//! binaries use.

use rev_core::phrase::{Container, EventSpec, PhraseSpec, TrackSpec};
use rev_core::tick::{PPQ, Tick};
use rev_core::{Command, NoteNumber, TrackId};
use rev_engine::{Position, SampleTime};
use rev_midi::{Captured, Message};
use rev_sched::TempoMap;
use rev_store::query;
use rev_testkit::TempProject;

use super::{Mode, Recorder};

const RATE: u32 = 48_000;

/// 120 bpm: a quarter is half a second, so at 48 kHz a quarter is 24 000
/// samples and a tick is 24000/5040 samples. Whole numbers keep the tests exact.
fn tempo() -> TempoMap {
    TempoMap::constant(500_000, RATE)
}

/// A position that is running, whose correlation pair says "session sample S was
/// seen at nanos N", with the transport started at session zero (offset 0).
fn running_at(sample: u64, nanos: u64) -> Position {
    Position {
        at: SampleTime(sample),
        play: SampleTime(sample),
        running: true,
        correlate_at: SampleTime(sample),
        correlate_nanos: nanos,
        sample_rate: RATE,
        ..Position::default()
    }
}

/// Nanoseconds at which a given play-sample was reached, for a clock running at
/// exactly the nominal rate from nanos 0 at sample 0.
fn nanos_for(sample: u64) -> u64 {
    // sample / rate seconds → nanoseconds.
    sample * 1_000_000_000 / u64::from(RATE)
}

fn note_on(note: i32, velocity: u8, nanos: u64) -> Captured {
    Captured {
        message: Message::NoteOn {
            channel: 0,
            note: NoteNumber(note),
            velocity,
        },
        nanos,
    }
}

fn note_off(note: i32, nanos: u64) -> Captured {
    Captured {
        message: Message::NoteOff {
            channel: 0,
            note: NoteNumber(note),
        },
        nanos,
    }
}

/// Prime a recorder's correlation with a short history of on-the-nose pairs, so
/// `sample_at` has a fit to extrapolate from.
fn prime(recorder: &mut Recorder) {
    for block in 0..8u64 {
        let sample = block * 512;
        recorder.observe(&running_at(sample, nanos_for(sample)));
    }
}

#[test]
fn a_note_lands_at_the_tick_it_was_played() {
    let mut recorder = Recorder::new(TrackId(1), tempo());
    recorder.arm(Mode::Overdub);
    prime(&mut recorder);

    // Play a note on at exactly one quarter in (24 000 samples), off a quarter
    // later. One quarter is PPQ ticks by construction.
    let on = 24_000;
    let off = 48_000;
    recorder.capture(note_on(60, 100, nanos_for(on)));
    recorder.capture(note_off(60, nanos_for(off)));

    let staged = recorder.staged();
    assert_eq!(staged.len(), 1, "one completed note");
    let note = &staged[0];
    // Within a tick of the quarter grid — the two integer conversions compose
    // exactly here, but a tick of slack keeps the test about placement, not
    // rounding.
    assert!(
        (note.at_tick.get() - PPQ).abs() <= 1,
        "onset at ~1 quarter, got {}",
        note.at_tick.get()
    );
    assert!(
        (note.dur_tick.get() - PPQ).abs() <= 1,
        "duration ~1 quarter, got {}",
        note.dur_tick.get()
    );
    assert_eq!(note.note_number, Some(NoteNumber(60)));
}

#[test]
fn velocity_reaches_the_16_bit_domain() {
    let mut recorder = Recorder::new(TrackId(1), tempo());
    recorder.arm(Mode::Overdub);
    prime(&mut recorder);
    recorder.capture(note_on(60, 127, nanos_for(24_000)));
    recorder.capture(note_off(60, nanos_for(48_000)));
    assert_eq!(
        recorder.staged()[0].velocity,
        Some(0xFFFF),
        "full 7-bit velocity maps to full 16-bit"
    );
}

#[test]
fn an_unmatched_note_off_is_ignored() {
    let mut recorder = Recorder::new(TrackId(1), tempo());
    recorder.arm(Mode::Overdub);
    prime(&mut recorder);
    recorder.capture(note_off(60, nanos_for(24_000)));
    assert!(
        recorder.staged().is_empty(),
        "an off with no on stages nothing"
    );
}

#[test]
fn a_repeated_key_nests_on_and_off_pairs() {
    let mut recorder = Recorder::new(TrackId(1), tempo());
    recorder.arm(Mode::Overdub);
    prime(&mut recorder);
    // Two overlapping presses of the same key: on, on, off, off. The most
    // recent onset pairs with the first off (rposition), so both notes exist.
    recorder.capture(note_on(60, 80, nanos_for(24_000)));
    recorder.capture(note_on(60, 80, nanos_for(36_000)));
    recorder.capture(note_off(60, nanos_for(48_000)));
    recorder.capture(note_off(60, nanos_for(60_000)));
    assert_eq!(recorder.staged().len(), 2, "both presses complete");
}

#[test]
fn a_note_played_while_disarmed_is_not_captured() {
    let mut recorder = Recorder::new(TrackId(1), tempo());
    prime(&mut recorder);
    recorder.capture(note_on(60, 100, nanos_for(24_000)));
    recorder.capture(note_off(60, nanos_for(48_000)));
    assert!(recorder.staged().is_empty(), "disarmed captures nothing");
}

#[test]
fn a_held_note_is_dropped_on_disarm_and_counted() {
    let mut recorder = Recorder::new(TrackId(1), tempo());
    recorder.arm(Mode::Overdub);
    prime(&mut recorder);
    recorder.capture(note_on(60, 100, nanos_for(24_000))); // never released
    assert_eq!(recorder.disarm(), 1, "one still-held note dropped");
    assert!(recorder.staged().is_empty());
}

// --- Mode and durability: these need a real project. -------------------------

/// A throwaway project with one arrangement phrase and one track, plus one
/// existing direct note on the track, so Replace has something to clear.
fn project_with_a_track() -> (TempProject, TrackId) {
    let mut temp = TempProject::create().expect("create");
    let track = temp
        .project_mut()
        .gesture(|g| {
            let bar = PPQ * 4;
            let arrangement = match g.exec(Command::CreatePhrase {
                id: None,
                phrase: PhraseSpec::new("Arrangement", Tick(bar)),
            })? {
                Command::CreatePhrase { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            let track = match g.exec(Command::CreateTrack {
                id: None,
                track: TrackSpec::new(arrangement, "Take", 0),
            })? {
                Command::CreateTrack { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            // One note already on the track — the material Replace must clear and
            // Overdub must keep.
            g.exec(Command::AddEvent {
                container: Container::Track(track),
                event: vec![EventSpec::note(Tick::ZERO, Tick(PPQ), 48, 40_000)],
            })?;
            Ok(track)
        })
        .expect("build");
    (temp, track)
}

fn record_one_note(recorder: &mut Recorder) {
    prime(recorder);
    recorder.capture(note_on(72, 100, nanos_for(24_000)));
    recorder.capture(note_off(72, nanos_for(48_000)));
}

fn track_notes(temp: &TempProject, track: TrackId) -> Vec<i32> {
    query::event_on_track(temp.project().reader(), track)
        .expect("events")
        .iter()
        .filter_map(|e| e.note_number.map(|n| n.get()))
        .collect()
}

#[test]
fn overdub_keeps_what_was_there() {
    let (mut temp, track) = project_with_a_track();
    let mut recorder = Recorder::new(track, tempo());
    recorder.arm(Mode::Overdub);
    record_one_note(&mut recorder);
    let journaled = recorder.flush(temp.project_mut()).expect("flush");
    assert_eq!(journaled, 1);

    let notes = track_notes(&temp, track);
    assert!(
        notes.contains(&48),
        "the pre-existing note survives overdub"
    );
    assert!(notes.contains(&72), "the recorded note is added");
}

#[test]
fn replace_clears_the_track_first() {
    let (mut temp, track) = project_with_a_track();
    let mut recorder = Recorder::new(track, tempo());
    recorder.arm(Mode::Replace);
    record_one_note(&mut recorder);
    recorder.flush(temp.project_mut()).expect("flush");

    let notes = track_notes(&temp, track);
    assert!(!notes.contains(&48), "the pre-existing note is gone");
    assert_eq!(notes, vec![72], "only the recorded note remains");
}

#[test]
fn replace_is_undoable() {
    let (mut temp, track) = project_with_a_track();
    let mut recorder = Recorder::new(track, tempo());
    recorder.arm(Mode::Replace);
    record_one_note(&mut recorder);
    recorder.flush(temp.project_mut()).expect("flush");
    // Undo the record, then the clear — the pre-existing note comes back.
    assert!(temp.project_mut().undo().expect("undo record"));
    assert!(temp.project_mut().undo().expect("undo clear"));
    assert!(
        track_notes(&temp, track).contains(&48),
        "the cleared note is restored by undo"
    );
}

#[test]
fn capture_commits_as_it_goes_not_at_stop() {
    // The durability premise (rec-01 §5), as a unit fact: after two frames each
    // completing a note and flushing, the journal already holds both notes —
    // there is no buffered take that a crash could take with it. The actual
    // kill -9 process test is rec-02.
    let (mut temp, track) = project_with_a_track();
    let mut recorder = Recorder::new(track, tempo());
    recorder.arm(Mode::Overdub);
    prime(&mut recorder);

    recorder.capture(note_on(72, 100, nanos_for(24_000)));
    recorder.capture(note_off(72, nanos_for(36_000)));
    assert_eq!(recorder.flush(temp.project_mut()).expect("frame 1"), 1);

    recorder.capture(note_on(74, 100, nanos_for(48_000)));
    recorder.capture(note_off(74, nanos_for(60_000)));
    assert_eq!(recorder.flush(temp.project_mut()).expect("frame 2"), 1);

    // Reopen from disk — only committed gestures survive, so this is exactly
    // what a crash-and-reopen would see.
    temp.reopen().expect("reopen");
    let recorded: Vec<i32> = track_notes(&temp, track)
        .into_iter()
        .filter(|&n| n == 72 || n == 74)
        .collect();
    assert_eq!(
        recorded,
        vec![72, 74],
        "both notes are durable before any Stop"
    );
}

/// A silent recorder placed without an offset (never saw a running position)
/// cannot place a note, and drops it rather than guessing.
#[test]
fn without_a_running_transport_nothing_is_placed() {
    let mut recorder = Recorder::new(TrackId(1), tempo());
    recorder.arm(Mode::Overdub);
    // No observe(): no correlation, no offset.
    recorder.capture(note_on(60, 100, nanos_for(24_000)));
    recorder.capture(note_off(60, nanos_for(48_000)));
    assert!(
        recorder.staged().is_empty(),
        "unplaceable events are dropped"
    );
}
