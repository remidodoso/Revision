use super::*;

use crate::table::Table;
use crate::voice::Span;

const RATE: u32 = 48_000;
const QUANTUM: usize = crate::graph::QUANTUM;

/// A sawtooth table at 100 Hz. Rich enough that a filter has something to take
/// away, unlike a sine.
fn tables() -> TableSet {
    let mut set = TableSet::new();
    let len = (RATE / 100) as usize;
    let sample: Vec<f32> = (0..len)
        .map(|n| 2.0 * (n as f32 / len as f32) - 1.0)
        .collect();
    set.add(Table::new(sample, 100.0));
    set
}

fn render(instrument: &mut Instrument, quanta: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; QUANTUM * 2];
    let mut collected = Vec::new();
    for _ in 0..quanta {
        out.fill(0.0);
        instrument.render(
            Span {
                phase: 0,
                frames: QUANTUM,
                stride: QUANTUM,
            },
            &mut out,
        );
        collected.extend_from_slice(&out[..QUANTUM]);
    }
    collected
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

#[test]
fn the_voice_from_the_inventory_builds() {
    let instrument =
        Instrument::new(Patch::default(), tables(), 8, RATE).expect("the graph should build");
    assert_eq!(instrument.pool().len(), 8);
    // Eight nodes: two heads, two trims, two panners, an amp gain, a filter.
    assert_eq!(instrument.pool().free_count(), 8);
}

#[test]
fn a_note_sounds_in_stereo() {
    let mut instrument = Instrument::new(Patch::plucked(), tables(), 4, RATE).expect("build");
    instrument.note_on(220.0, 1.0, RATE as u64 / 4, 0, 1);

    let mut out = vec![0.0f32; QUANTUM * 2];
    let mut left = 0.0f32;
    let mut right = 0.0f32;
    for _ in 0..64 {
        out.fill(0.0);
        instrument.render(
            Span {
                phase: 0,
                frames: QUANTUM,
                stride: QUANTUM,
            },
            &mut out,
        );
        left = left.max(peak(&out[..QUANTUM]));
        right = right.max(peak(&out[QUANTUM..]));
    }
    assert!(left > 0.05, "audible on the left: {left}");
    assert!(right > 0.05, "and on the right: {right}");
}

#[test]
fn the_two_heads_are_decorrelated_so_the_image_is_wide() {
    // One bake, two read heads at different offsets, panned apart. If the heads
    // were identical the two channels would be identical and the sound would be
    // mono however far the panners were pushed.
    let mut instrument = Instrument::new(Patch::default(), tables(), 1, RATE).expect("build");
    instrument.note_on(220.0, 1.0, RATE as u64, 0, 12_345);

    let mut out = vec![0.0f32; QUANTUM * 2];
    instrument.render(
        Span {
            phase: 0,
            frames: QUANTUM,
            stride: QUANTUM,
        },
        &mut out,
    );
    // Skip the very start, where the amplitude envelope is near zero.
    for _ in 0..40 {
        out.fill(0.0);
        instrument.render(
            Span {
                phase: 0,
                frames: QUANTUM,
                stride: QUANTUM,
            },
            &mut out,
        );
    }

    let difference: f32 = out[..QUANTUM]
        .iter()
        .zip(&out[QUANTUM..])
        .map(|(l, r)| (l - r).abs())
        .sum();
    assert!(
        difference > 0.1,
        "the channels should differ: total difference {difference}"
    );
}

#[test]
fn the_attack_is_linear_and_the_note_starts_from_silence() {
    // An exponential ramp from near zero is inaudible and then snaps; linear is
    // what the ear reads as an attack. So the first quantum of a slow attack
    // must be quiet and rising, not already loud.
    let patch = Patch {
        attack: 0.2,
        ..Patch::default()
    };
    let mut instrument = Instrument::new(patch, tables(), 1, RATE).expect("build");
    instrument.note_on(220.0, 1.0, RATE as u64, 0, 1);

    let early = render(&mut instrument, 1);
    let later = render(&mut instrument, 1);
    assert!(peak(&early) < 0.2, "starts quiet: {}", peak(&early));
    assert!(peak(&later) > peak(&early), "and rises");
}

#[test]
fn a_plucked_note_decays_without_being_told_to_stop() {
    // Sustain 0 means the decay target is silence, so the sound dies on its own
    // — which is what makes a pluck a pluck.
    let mut instrument = Instrument::new(Patch::plucked(), tables(), 1, RATE).expect("build");
    instrument.note_on(220.0, 1.0, RATE as u64 * 2, 0, 1);

    // Measured as a single quantum at three points, not as the peak of a span:
    // a span's peak is its *loudest* moment, which for a decaying sound is the
    // start of it, so spans would compare the wrong instants.
    let at = |instrument: &mut Instrument, skip: usize| {
        render(instrument, skip);
        peak(&render(instrument, 1))
    };

    let early = at(&mut instrument, 2);
    let middle = at(&mut instrument, 60);
    let late = at(&mut instrument, 120);

    assert!(early > 0.05, "sounds: {early}");
    assert!(middle < early * 0.8, "decays: {middle} after {early}");
    assert!(
        late < middle * 0.5,
        "and keeps decaying: {late} after {middle}"
    );
}

#[test]
fn key_tracking_moves_the_cutoff_with_pitch() {
    // A high note gets a proportionally higher cutoff, so the instrument does
    // not get duller as it goes up — which is what key tracking is for.
    let bright = |hz: f32, key_track: f32| {
        let patch = Patch {
            key_track,
            cutoff: 800.0,
            filter_env: 0.0,
            attack: 0.001,
            sustain: 1.0,
            decay: 10.0,
            ..Patch::default()
        };
        let mut instrument = Instrument::new(patch, tables(), 1, RATE).expect("build");
        instrument.note_on(hz, 1.0, RATE as u64 * 2, 0, 1);
        render(&mut instrument, 8);
        peak(&render(&mut instrument, 24))
    };

    // With tracking, the higher note keeps more of its energy; without, the
    // fixed cutoff takes progressively more away.
    let tracked_low = bright(110.0, 1.0);
    let tracked_high = bright(880.0, 1.0);
    let untracked_high = bright(880.0, 0.0);
    assert!(
        tracked_high > untracked_high,
        "tracking should keep the high note brighter: {tracked_high} vs {untracked_high}"
    );
    assert!(tracked_low > 0.0);
}

#[test]
fn the_same_note_renders_identically_twice() {
    // R-1402 at the instrument level: the seeded head offsets are the one place
    // the source this is ported from was nondeterministic, and seeding them is
    // the upgrade R-706 asks for.
    let play = || {
        let mut instrument = Instrument::new(Patch::plucked(), tables(), 4, RATE).expect("build");
        instrument.note_on(330.0, 0.8, RATE as u64 / 2, 0, 99);
        render(&mut instrument, 40)
    };
    let first = play();
    let second = play();
    assert_eq!(
        first.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        second.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        "identical input renders bit-identically"
    );
}

#[test]
fn a_different_seed_sounds_different() {
    let play = |seed: u64| {
        let mut instrument = Instrument::new(Patch::plucked(), tables(), 4, RATE).expect("build");
        instrument.note_on(330.0, 0.8, RATE as u64 / 2, 0, seed);
        render(&mut instrument, 40)
    };
    assert_ne!(play(1), play(2), "the head offsets genuinely vary");
}

#[test]
fn a_chord_uses_a_voice_each() {
    let mut instrument = Instrument::new(Patch::plucked(), tables(), 8, RATE).expect("build");
    for (n, hz) in [261.6, 329.6, 392.0].into_iter().enumerate() {
        instrument.note_on(hz, 0.6, RATE as u64, n * 16, n as u64);
    }
    assert_eq!(instrument.pool().sounding(), 3);
    assert_eq!(instrument.pool().stolen(), 0);
}

#[test]
fn all_notes_off_silences_everything() {
    let mut instrument = Instrument::new(Patch::plucked(), tables(), 8, RATE).expect("build");
    for n in 0..6 {
        instrument.note_on(200.0 + 50.0 * n as f32, 0.5, RATE as u64 * 10, 0, n as u64);
    }
    instrument.all_notes_off();
    // Long enough for every release tail to finish.
    render(&mut instrument, 400);
    assert_eq!(instrument.pool().sounding(), 0);
}

#[test]
fn playing_allocates_nothing() {
    // Note-on, envelope scheduling and rendering all happen on the audio thread.
    // The guard is the only thing that would ever tell us otherwise.
    let mut instrument = Instrument::new(Patch::plucked(), tables(), 8, RATE).expect("build");
    let mut out = vec![0.0f32; QUANTUM * 2];

    let _rt = crate::guard::RtScope::enter();
    for n in 0..32u64 {
        instrument.note_on(
            220.0 + 10.0 * n as f32,
            0.5,
            4_000,
            (n as usize * 3) % QUANTUM,
            n,
        );
        out.fill(0.0);
        instrument.render(
            Span {
                phase: 0,
                frames: QUANTUM,
                stride: QUANTUM,
            },
            &mut out,
        );
    }
    instrument.all_notes_off();
    for _ in 0..8 {
        out.fill(0.0);
        instrument.render(
            Span {
                phase: 0,
                frames: QUANTUM,
                stride: QUANTUM,
            },
            &mut out,
        );
    }
}
