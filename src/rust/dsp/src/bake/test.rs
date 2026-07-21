use super::*;

use crate::spec::{Source, Vowel};
use realfft::RealFftPlanner;

const RATE: u32 = 48_000;
/// Short tables: the identities hold at any length, and 2^14 keeps the suite
/// quick. `the_full_size_bake_is_sane` covers the real one — which turns out to
/// take ten milliseconds, so the length here is habit rather than necessity.
const LEN: usize = 1 << 14;

fn rms(sample: &[f32]) -> f32 {
    (sample.iter().map(|s| s * s).sum::<f32>() / sample.len() as f32).sqrt()
}

/// The magnitude spectrum of a baked table, for asking where its energy is.
fn analyse(sample: &[f32]) -> Vec<f64> {
    let mut planner = RealFftPlanner::<f64>::new();
    let forward = planner.plan_fft_forward(sample.len());
    let mut input: Vec<f64> = sample.iter().map(|s| f64::from(*s)).collect();
    let mut output = forward.make_output_vec();
    forward
        .process(&mut input, &mut output)
        .expect("the buffers come from the plan");
    output.iter().map(|c| c.norm()).collect()
}

#[test]
fn baking_twice_gives_the_same_table() {
    // R-1402 at its source. The one thing the JS leaves random is phase, and
    // seeding it is the upgrade R-706 asks of a port.
    let spec = BakeSpec::default();
    let first = bake(&spec, 261.0, RATE, LEN);
    let second = bake(&spec, 261.0, RATE, LEN);
    assert_eq!(
        first.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        second.iter().map(|s| s.to_bits()).collect::<Vec<_>>()
    );
}

#[test]
fn a_different_patch_bakes_a_different_table() {
    let saw = bake(&BakeSpec::default(), 261.0, RATE, LEN);
    let pulse = bake(
        &BakeSpec {
            source: Source::Pulse,
            ..BakeSpec::default()
        },
        261.0,
        RATE,
        LEN,
    );
    assert_ne!(saw, pulse);
}

#[test]
fn neighbouring_bases_do_not_share_phases() {
    // The base is in the seed for this reason: two tables of the same patch a
    // half octave apart must be independent, or the seam between them would be
    // a correlation rather than a change of colour.
    let spec = BakeSpec::default();
    let low = bake(&spec, 261.0, RATE, LEN);
    let high = bake(&spec, base_hz(261.0, 1), RATE, LEN);
    let correlation: f32 = low.iter().zip(&high).map(|(a, b)| a * b).sum::<f32>()
        / (low.len() as f32 * rms(&low) * rms(&high));
    assert!(correlation.abs() < 0.1, "correlated: {correlation}");
}

#[test]
fn every_table_is_normalized_to_the_same_level() {
    // Source, Harmonics and Bandwidth change colour, never loudness (R-713).
    for spec in [
        BakeSpec::default(),
        BakeSpec {
            source: Source::Pulse,
            shape: 0.8,
            ..BakeSpec::default()
        },
        BakeSpec {
            harmonic: 8,
            ..BakeSpec::default()
        },
        BakeSpec {
            harmonic: 256,
            bandwidth: 120.0,
            ..BakeSpec::default()
        },
        BakeSpec {
            vowel: Vowel::Ee,
            source: Source::Voice,
            ..BakeSpec::default()
        },
        BakeSpec {
            noise: 1.0,
            ..BakeSpec::default()
        },
    ] {
        let table = bake(&spec, 220.0, RATE, LEN);
        let level = rms(&table);
        assert!(
            (level - TABLE_RMS).abs() < 1e-4,
            "{spec:?} baked at {level}, not {TABLE_RMS}"
        );
        assert!(table.iter().all(|s| s.is_finite()), "not finite: {spec:?}");
    }
}

#[test]
fn a_baked_table_has_no_energy_above_the_band_limit() {
    // **The guarantee this whole design exists for**, measured on the finished
    // table rather than argued about: nothing above the limit means nothing can
    // cross Nyquist when the table is played up, so no fold-back is possible.
    let ceiling = band_limit(RATE);
    let bin_hz = f64::from(RATE) / LEN as f64;
    for spec in [
        BakeSpec::default(),
        BakeSpec {
            harmonic: 400,
            noise: 0.5,
            bandwidth: 200.0,
            ..BakeSpec::default()
        },
    ] {
        for base in [110.0, 880.0, 3_000.0] {
            let mag = analyse(&bake(&spec, base, RATE, LEN));
            let peak = mag.iter().cloned().fold(0.0f64, f64::max);
            for (bin, value) in mag.iter().enumerate() {
                let f = bin as f64 * bin_hz;
                if f > ceiling * 1.02 {
                    assert!(
                        *value < peak * 1e-6,
                        "base {base}: energy at {f} Hz is {value}, peak {peak}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_set_is_half_octave_spaced_and_ascending() {
    let set = bake_set(&BakeSpec::default(), 261.625_58, RATE);
    assert_eq!(set.len(), BASE_COUNT);
    assert_eq!(set.len(), 16, "sixteen tables, 8 MB at full size");
    for pair in set.windows(2) {
        let ratio = f64::from(pair[1].0) / f64::from(pair[0].0);
        assert!(
            (ratio - std::f64::consts::SQRT_2).abs() < 1e-4,
            "spacing {ratio}, wanted √2"
        );
    }
    // A note is never further than a quarter octave from a base, which is what
    // bounds the playback rate and makes the band limit affordable.
    let lowest = f64::from(set[0].0);
    let highest = f64::from(set[set.len() - 1].0);
    assert!(lowest < 25.0 && highest > 4_000.0, "{lowest}..{highest}");
}

#[test]
fn the_full_size_bake_is_sane() {
    let table = bake(&BakeSpec::default(), 261.625_58, RATE, TABLE_LEN);
    assert_eq!(table.len(), TABLE_LEN);
    assert!((rms(&table) - TABLE_RMS).abs() < 1e-4);
    assert!(table.iter().all(|s| s.is_finite()));
}
