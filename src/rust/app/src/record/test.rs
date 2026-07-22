//! Recorder tests (rec-01 §7).
//!
//! Placement and pairing are pure — they need a synthetic [`Position`] stream
//! and a [`Captured`] stream, no device and no store. Mode and durability need a
//! real [`Project`], because clearing and journaling are what they are about;
//! those build a throwaway project in a temp dir, the same pattern the demo
//! binaries use.

use rev_core::phrase::{Change, Container, EventSpec, PhrasePatch, PhraseSpec, TrackSpec};
use rev_core::tick::{PPQ, Tick};
use rev_core::{Command, NoteNumber, PhraseId, TrackId};
use rev_engine::{Position, SampleTime};
use rev_midi::{Captured, Message};
use rev_sched::{Compiler, TempoMap};
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

// --- Two tracks: the rec-02 multi-track mechanism, headless. -----------------

/// A project whose arrangement carries a tuning (so notes resolve to pitches and
/// can be compiled) and holds two empty tracks.
fn two_track_project() -> (TempProject, PhraseId, [TrackId; 2]) {
    let mut temp = TempProject::create().expect("create");
    let tuning = query::tuning_by_name(temp.project().reader(), "12-ET")
        .expect("query")
        .map(|t| t.id);
    let (arrangement, track) = temp
        .project_mut()
        .gesture(|g| {
            let mut phrase = PhraseSpec::new("Arrangement", Tick(PPQ * 4 * 8));
            phrase.tuning_id = tuning;
            let arrangement = match g.exec(Command::CreatePhrase { id: None, phrase })? {
                Command::CreatePhrase { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
            let mut track = [TrackId(0); 2];
            for (i, slot) in track.iter_mut().enumerate() {
                *slot = match g.exec(Command::CreateTrack {
                    id: None,
                    track: TrackSpec::new(arrangement, format!("Track {}", i + 1), i as i32),
                })? {
                    Command::CreateTrack { id: Some(id), .. } => id,
                    _ => unreachable!(),
                };
            }
            Ok((arrangement, track))
        })
        .expect("build");
    (temp, arrangement, track)
}

#[test]
fn two_tracks_record_and_replay_together() {
    let (mut temp, arrangement, track) = two_track_project();

    // A take on each track, at different pitches so the two are distinguishable.
    for (slot, note) in [(0usize, 60), (1usize, 67)] {
        let mut recorder = Recorder::new(track[slot], tempo());
        recorder.arm(Mode::Overdub);
        prime(&mut recorder);
        recorder.capture(note_on(note, 100, nanos_for(24_000)));
        recorder.capture(note_off(note, nanos_for(48_000)));
        assert_eq!(recorder.flush(temp.project_mut()).expect("flush"), 1);
    }

    // Both tracks hold their take.
    assert_eq!(track_notes(&temp, track[0]), vec![60], "track 1 recorded");
    assert_eq!(track_notes(&temp, track[1]), vec![67], "track 2 recorded");

    // Replay: one schedule over both tracks carries notes from each, tagged by
    // the track's index in the compile list (voice 0 and voice 1).
    let point: Vec<(Tick, i64)> = query::tempo_point(temp.project().reader(), arrangement)
        .expect("tempo")
        .into_iter()
        .map(|p| (p.at_tick, p.usec_per_quarter))
        .collect();
    let mut compiler = Compiler::new(TempoMap::new(point, RATE), vec![track[0], track[1]]);
    let chunk = compiler
        .chunk(
            temp.project(),
            SampleTime(0),
            SampleTime(u64::from(RATE) * 4),
        )
        .expect("compile");
    assert_eq!(
        compiler.unplayable(),
        0,
        "both notes resolve through the tuning"
    );
    assert!(chunk.note.iter().any(|n| n.voice == 0), "track 1 replays");
    assert!(chunk.note.iter().any(|n| n.voice == 1), "track 2 replays");
    assert_eq!(chunk.note.len(), 2, "exactly the two recorded notes");
}

/// rec-03, the party trick as a fact: retuning a recorded take is one command,
/// and it moves the *physics* under the notes while the *degrees* stay put — the
/// same performance, heard in a new tuning. Proof the pipeline is degree-native
/// (R-002), not 12-ET with tuning bolted on.
#[test]
fn retuning_a_take_keeps_the_degrees_and_moves_the_pitch() {
    let (mut temp, arrangement, track) = two_track_project();
    let track = track[0];

    // A note away from the anchor (60), so the two tunings actually disagree —
    // note 64 is 4 semitones in 12-ET but 4 steps of 16 in 16-ET.
    let mut recorder = Recorder::new(track, tempo());
    recorder.arm(Mode::Overdub);
    prime(&mut recorder);
    recorder.capture(note_on(64, 100, nanos_for(24_000)));
    recorder.capture(note_off(64, nanos_for(48_000)));
    recorder.flush(temp.project_mut()).expect("flush");

    let compile = |temp: &TempProject| -> f32 {
        let point: Vec<(Tick, i64)> = query::tempo_point(temp.project().reader(), arrangement)
            .expect("tempo")
            .into_iter()
            .map(|p| (p.at_tick, p.usec_per_quarter))
            .collect();
        let mut compiler = Compiler::new(TempoMap::new(point, RATE), vec![track]);
        let chunk = compiler
            .chunk(
                temp.project(),
                SampleTime(0),
                SampleTime(u64::from(RATE) * 4),
            )
            .expect("compile");
        assert_eq!(compiler.unplayable(), 0);
        assert_eq!(chunk.note.len(), 1);
        chunk.note[0].hz
    };

    let twelve = compile(&temp);

    // The one-line swap.
    let sixteen_id = query::tuning_by_name(temp.project().reader(), "16-ET")
        .expect("query")
        .expect("16-ET is seeded")
        .id;
    temp.project_mut()
        .apply(Command::SetPhrase {
            id: arrangement,
            patch: PhrasePatch {
                tuning_id: Change::Set(sixteen_id),
                ..PhrasePatch::default()
            },
        })
        .expect("retune");

    let sixteen = compile(&temp);

    // The degree did not move: the model still holds note 64.
    assert_eq!(
        track_notes(&temp, track),
        vec![64],
        "the recorded degree is unchanged"
    );
    // The pitch did: 16-ET's fourth step is not 12-ET's major third.
    assert!(
        (twelve - sixteen).abs() > 1.0,
        "the same degree sounds at a different pitch: {twelve} Hz vs {sixteen} Hz"
    );
}
