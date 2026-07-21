//! The aliasing sweep — the test dsp-02 exists for.
//!
//! The standard is Notorolla's Padlington, which has no aliasing character
//! anywhere on the keyboard. That was a property of a build nobody had
//! measured; four mechanisms happened to align (dsp-02 §4.2) and the margin was
//! never written down. This makes it a property the build cannot lose.
//!
//! It measures **inharmonic energy across the whole output band, up to
//! Nyquist** — not up to 20 kHz. Ultrasonic content is not harmless: every
//! reproduction chain is nonlinear somewhere, and a nonlinearity translates
//! inharmonic content *downward*, into the range the ear is most critical in.
//! Harmonic content translates as harmonic and fuses with the note; inharmonic
//! content does not (R-720).

use rev_dsp::bake::{BASE_HIGH, BASE_LOW, bake_with_limit, band_limit, base_hz};
use rev_dsp::{BakeSpec, Source};
use rev_engine::{Instrument, Patch, Span, Table, TableSet};
use rev_testkit::spectrum::{analyse, db};

const RATE: u32 = 48_000;
/// **The real table length.** A shorter one bakes just as correctly but its bins
/// are coarser than the smear at low bases, and the one-bin floor then makes a
/// low note's pitch quantize: at 2^15 the bottom of the keyboard measured 23
/// cents sharp, which is the table's resolution, not a defect in the note. A
/// full-size bake is ten milliseconds — the reason to use a short one was never
/// real.
const LEN: usize = rev_dsp::TABLE_LEN;
const QUANTUM: usize = 128;
const ANCHOR: f64 = 261.625_58;

/// The patch the sweep uses: **the filter wide open**.
///
/// Deliberately not the default. A key-tracked lowpass at 7 kHz would hide
/// exactly the artifacts being measured, and then the test would be certifying
/// the filter rather than the bake. This is the raw table, as naked as the
/// instrument can make it.
fn bare() -> Patch {
    Patch {
        attack: 0.005,
        decay: 10.0,
        sustain: 1.0,
        release: 0.1,
        filter_env: 0.0,
        cutoff: 21_000.0,
        resonance: 0.5,
        key_track: 0.0,
        pitch_attack: 0.0,
        ..Patch::default()
    }
}

/// Bake a table set: one table every `step` half-octaves, band-limited at
/// `limit_hz`.
fn table_set(spec: &BakeSpec, step: i32, limit_hz: f64) -> TableSet {
    let mut set = TableSet::new();
    let mut n = BASE_LOW;
    while n <= BASE_HIGH {
        let base = base_hz(ANCHOR, n);
        set.add(Table::new(
            bake_with_limit(spec, base, RATE, LEN, limit_hz),
            base as f32,
        ));
        n += step;
    }
    set
}

/// Render one sustained note and return the left channel.
fn play(table: &TableSet, hz: f32, frames: usize) -> Vec<f32> {
    let mut instrument = Instrument::new(bare(), table.clone(), 1, RATE).expect("build");
    instrument.note_on(hz, 1.0, u64::from(RATE) * 20, 0, 7);
    let mut out = vec![0.0f32; QUANTUM * 2];
    let mut collected = Vec::with_capacity(frames);
    while collected.len() < frames {
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
    // Past the attack, so the envelope is not modulating what we measure.
    collected.split_off(QUANTUM * 8)
}

/// Every note from 55 Hz to 3520 Hz, six octaves in two-semitone steps.
fn keyboard() -> Vec<f32> {
    (0..37)
        .map(|step| 55.0 * 2f32.powf(step as f32 / 6.0))
        .collect()
}

/// The worst inharmonic component of one note, in dB below its loudest partial.
fn worst_db(table: &TableSet, hz: f32) -> f64 {
    let sample = play(table, hz, WINDOW * 2);
    let spectrum = analyse(&sample[..WINDOW], RATE);
    db(spectrum.worst_inharmonic(f64::from(hz), 20.0, 30.0))
}

const WINDOW: usize = 1 << 15;

/// The sweep's patch: **a narrow band**, 2 cents rather than the default 25.
fn narrow() -> BakeSpec {
    BakeSpec {
        bandwidth: 2.0,
        ..BakeSpec::default()
    }
}

#[test]
fn the_note_that_sounds_is_the_note_that_was_asked_for() {
    // **The debt from eng-07.** A whole run played at the wrong pitch with 331
    // tests green, because nothing in the suite listened to pitch. This is the
    // assertion that was missing, and it is deliberately across the *whole*
    // keyboard: the defect then was seed-dependent and would have hidden from a
    // single note.
    // A longer window than the sweep uses. At 32768 frames the bins are 1.46 Hz
    // — 45 cents wide at the bottom of the keyboard, which is coarser than the
    // thing being asserted. 2.7 seconds of note puts a bin at 0.37 Hz.
    const LONG: usize = 1 << 17;
    let table = table_set(&BakeSpec::harpington(), 1, band_limit(RATE));
    for hz in keyboard() {
        let sample = play(&table, hz, LONG * 2);
        let measured = analyse(&sample[..LONG], RATE).fundamental_hz(f64::from(hz) * 0.6);
        let cent = 1200.0 * (measured / f64::from(hz)).log2();

        assert!(
            cent.abs() < 15.0,
            "asked for {hz} Hz, got {measured} Hz — {cent:.1} cents out"
        );
    }
}

/// The floor every note must stay under, in dB below its loudest partial.
///
/// **What is left once fold-back is gone.** The band limit removes crossing
/// Nyquist entirely — `a_baked_table_has_no_energy_above_the_band_limit` proves
/// that on the table itself — and what remains is the resampling images of a
/// looped table read at a rate other than 1. They sit at `(r−1)·SR ± k·f0`,
/// which is how they were identified: at 1568 Hz, read at 1.0595, the worst
/// components measured at 9690, 11256 and 12823 Hz, and `(0.0595 · 48000) −
/// k · 1568` reproduces all three exactly.
///
/// They are a property of reading a wavetable with a polynomial interpolator,
/// not of this geometry: swapping linear for Catmull-Rom moved them by 2 dB.
/// The instrument being ported has them too, and worse, because octave spacing
/// reads at rates up to 1.414. Removing them means reading oversampled, which
/// is the conversation dsp-02 §4.4 deferred until something earns it.
///
/// So this number is a **regression floor**, not a certificate of inaudibility.
/// It says the instrument has not got dirtier; it does not say it is clean.
const FLOOR_DB: f64 = -22.0;

#[test]
fn no_note_on_the_keyboard_aliases() {
    let table = table_set(&narrow(), 1, band_limit(RATE));
    let mut worst = (0.0f32, -200.0f64);
    for hz in keyboard() {
        let measured = worst_db(&table, hz);
        if measured > worst.1 {
            worst = (hz, measured);
        }
        assert!(
            measured < FLOOR_DB,
            "{hz} Hz: inharmonic energy at {measured:.1} dB"
        );
    }
    println!("worst note {:.1} Hz at {:.1} dB", worst.0, worst.1);
}

#[test]
fn a_bright_patch_is_the_hard_case_and_still_holds() {
    // A pulse with 256 harmonics puts real energy right up against the limit at
    // every base, which is the content that both folds and images. A saw's 1/k
    // rolloff is comparatively forgiving.
    let spec = BakeSpec {
        source: Source::Pulse,
        shape: 0.7,
        harmonic: 256,
        ..narrow()
    };
    let table = table_set(&spec, 1, band_limit(RATE));
    for hz in keyboard() {
        let measured = worst_db(&table, hz);
        assert!(
            measured < FLOOR_DB,
            "{hz} Hz: inharmonic energy at {measured:.1} dB"
        );
    }
}
