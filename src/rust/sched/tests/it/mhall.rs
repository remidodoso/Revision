//! The compiler against real material.
//!
//! The numbers here are deliberately checkable by hand: 5040 ticks per quarter
//! (R-003), 120 bpm, 48 kHz — so a quarter note is exactly 24 000 samples and a
//! reader can verify the first four onsets without running anything.

use rev_core::tick::{PPQ, Tick, bpm_to_usec_per_quarter};
use rev_engine::SampleTime;
use rev_sched::{Compiler, TempoMap};
use rev_store::query;
use rev_testkit::{TempProject, fixture};

const RATE: u32 = 48_000;
const QUARTER: u64 = 24_000;

/// MHALL, its track, and a compiler pointed at it at the fixture's tempo.
fn rig() -> (TempProject, Compiler) {
    let mut temp = TempProject::create().expect("project");
    let built = fixture::mhall(temp.project_mut()).expect("mhall");
    let point: Vec<(Tick, i64)> = query::tempo_point(temp.project().reader(), built.arrangement)
        .expect("tempo")
        .into_iter()
        .map(|p| (p.at_tick, p.usec_per_quarter))
        .collect();
    let compiler = Compiler::new(TempoMap::new(point, RATE), vec![built.track]);
    (temp, compiler)
}

/// The whole tune in one window — 8 bars at 120 bpm is 16 seconds.
fn whole(temp: &TempProject, compiler: &mut Compiler) -> rev_engine::Chunk {
    compiler
        .chunk(temp.project(), SampleTime(0), SampleTime(RATE as u64 * 20))
        .expect("compile")
}

#[test]
fn the_tune_compiles_to_positions_a_human_can_check() {
    let (temp, mut compiler) = rig();
    let chunk = whole(&temp, &mut compiler);

    assert_eq!(chunk.note.len(), 26, "every note of the tune");
    assert_eq!(compiler.unplayable(), 0, "nothing fell outside its tuning");

    // Bar 1: E D C D, one quarter each.
    let onset: Vec<u64> = chunk.note.iter().take(4).map(|n| n.at.0).collect();
    assert_eq!(onset, vec![0, QUARTER, QUARTER * 2, QUARTER * 3]);
    for note in chunk.note.iter().take(4) {
        assert_eq!(note.dur, QUARTER as u32);
    }

    // The last note is a whole note at bar 8 — 28 quarters in, four quarters long.
    let last = chunk.note.last().expect("a last note");
    assert_eq!(last.at, SampleTime(QUARTER * 28));
    assert_eq!(last.dur, QUARTER as u32 * 4);
}

#[test]
fn notes_carry_frequencies_not_note_numbers() {
    // R-312 made literal: the engine is never told what a note number is.
    let (temp, mut compiler) = rig();
    let chunk = whole(&temp, &mut compiler);

    // MHALL opens on E4 — note 64 in 12-ET, which is about 329.63 Hz.
    let first = chunk.note[0].hz;
    assert!(
        (first - 329.628).abs() < 0.01,
        "E4 should be about 329.63 Hz, got {first}"
    );

    // The lowest note of the tune is C4, the highest G4.
    let low = chunk.note.iter().map(|n| n.hz).fold(f32::MAX, f32::min);
    let high = chunk.note.iter().map(|n| n.hz).fold(0.0f32, f32::max);
    assert!((low - 261.626).abs() < 0.01, "C4: {low}");
    assert!((high - 391.995).abs() < 0.01, "G4: {high}");
}

#[test]
fn velocity_becomes_a_level() {
    let (temp, mut compiler) = rig();
    let chunk = whole(&temp, &mut compiler);
    // The fixture plays mezzo-forte: 49 152 of 65 535, which is 0.75.
    let level = chunk.note[0].level;
    assert!((level - 0.75).abs() < 1e-4, "{level}");
}

#[test]
fn a_window_admits_onsets_and_nothing_else() {
    let (temp, mut compiler) = rig();

    let early = compiler
        .chunk(temp.project(), SampleTime(0), SampleTime(QUARTER * 2))
        .expect("compile");
    assert_eq!(early.note.len(), 2);
    assert_eq!(early.from, SampleTime(0));
    assert_eq!(early.to, SampleTime(QUARTER * 2));

    // Consecutive windows partition the tune: every note appears exactly once,
    // which is the property that makes look-ahead refilling safe.
    let mut total = 0;
    let mut at = 0u64;
    while at < RATE as u64 * 20 {
        let next = at + QUARTER * 3; // deliberately not a bar multiple
        total += compiler
            .chunk(temp.project(), SampleTime(at), SampleTime(next))
            .expect("compile")
            .note
            .len();
        at = next;
    }
    assert_eq!(total, 26, "no note dropped or duplicated across windows");
}

#[test]
fn a_note_may_extend_past_the_window_that_admitted_it() {
    // R-405a: a window gates onsets and never truncates a duration. The final
    // whole note starts inside a window that ends before it does.
    let (temp, mut compiler) = rig();
    let chunk = compiler
        .chunk(
            temp.project(),
            SampleTime(QUARTER * 28),
            SampleTime(QUARTER * 29),
        )
        .expect("compile");

    assert_eq!(chunk.note.len(), 1);
    let note = chunk.note[0];
    assert_eq!(note.dur, QUARTER as u32 * 4);
    assert!(
        note.at.0 + u64::from(note.dur) > chunk.to.0,
        "the note outlives its window, and that is correct"
    );
}

#[test]
fn compiling_twice_produces_the_same_bytes() {
    // What makes R-1402's render-twice gate mean something for real material.
    let (temp, mut compiler) = rig();
    let first = whole(&temp, &mut compiler);
    let second = whole(&temp, &mut compiler);

    assert_eq!(first.note.len(), second.note.len());
    for (a, b) in first.note.iter().zip(&second.note) {
        assert_eq!(a.at, b.at);
        assert_eq!(a.dur, b.dur);
        assert_eq!(a.hz.to_bits(), b.hz.to_bits(), "frequencies bit-identical");
        assert_eq!(a.level.to_bits(), b.level.to_bits());
        assert_eq!(a.voice, b.voice);
    }
}

#[test]
fn a_tempo_change_moves_positions_and_durations_together() {
    let mut temp = TempProject::create().expect("project");
    let built = fixture::mhall(temp.project_mut()).expect("mhall");

    // 120 for the first four beats, then 240.
    let map = TempoMap::new(
        [
            (Tick(0), bpm_to_usec_per_quarter(120.0)),
            (Tick(PPQ * 4), bpm_to_usec_per_quarter(240.0)),
        ],
        RATE,
    );
    let mut compiler = Compiler::new(map, vec![built.track]);
    let chunk = compiler
        .chunk(temp.project(), SampleTime(0), SampleTime(RATE as u64 * 20))
        .expect("compile");

    // The first four notes are unchanged.
    assert_eq!(chunk.note[0].at, SampleTime(0));
    assert_eq!(chunk.note[3].at, SampleTime(QUARTER * 3));
    // The fifth is the first at the new tempo, and a quarter now costs half as
    // many samples — in position and in duration alike.
    assert_eq!(chunk.note[4].at, SampleTime(QUARTER * 4));
    assert_eq!(chunk.note[4].dur, QUARTER as u32 / 2);
    assert_eq!(chunk.note[5].at, SampleTime(QUARTER * 4 + QUARTER / 2));
}

#[test]
fn the_sixteen_equal_party_trick_moves_every_frequency_and_no_position() {
    // One command retunes the phrase. Nothing about *when* anything happens
    // changes; everything about *what* it sounds like does.
    let mut temp = TempProject::create().expect("project");
    let built = fixture::mhall(temp.project_mut()).expect("mhall");
    let map = TempoMap::new([(Tick(0), bpm_to_usec_per_quarter(120.0))], RATE);
    let mut compiler = Compiler::new(map, vec![built.track]);

    let before = compiler
        .chunk(temp.project(), SampleTime(0), SampleTime(RATE as u64 * 20))
        .expect("compile");

    temp.project_mut()
        .apply(rev_core::Command::SetPhrase {
            id: built.melody,
            patch: rev_core::phrase::PhrasePatch {
                tuning_id: rev_core::phrase::Change::Set(built.tuning_16et),
                ..Default::default()
            },
        })
        .expect("retune");
    compiler.retune();

    let after = compiler
        .chunk(temp.project(), SampleTime(0), SampleTime(RATE as u64 * 20))
        .expect("compile");

    assert_eq!(before.note.len(), after.note.len());
    let mut moved = 0;
    for (a, b) in before.note.iter().zip(&after.note) {
        assert_eq!(a.at, b.at, "positions are untouched by tuning");
        assert_eq!(a.dur, b.dur);
        if a.hz.to_bits() != b.hz.to_bits() {
            moved += 1;
        }
    }
    assert!(
        moved > 20,
        "retuning should move nearly every frequency; {moved} moved"
    );
}
