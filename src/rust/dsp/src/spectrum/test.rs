use super::*;

use crate::bake::band_limit;
use crate::profile::profile;

const RATE: u32 = 48_000;
const LEN: usize = 1 << 14;

fn mag(spec: &BakeSpec, base: f64) -> Vec<f64> {
    let amplitude = profile(spec, base);
    spectrum(spec, &amplitude, base, RATE, LEN, band_limit(RATE))
}

fn bin_hz() -> f64 {
    f64::from(RATE) / LEN as f64
}

/// Energy in a window around a frequency, as a fraction of the whole.
fn share(mag: &[f64], hz: f64, window_hz: f64) -> f64 {
    let total: f64 = mag.iter().map(|m| m * m).sum();
    if total == 0.0 {
        return 0.0;
    }
    let near: f64 = mag
        .iter()
        .enumerate()
        .filter(|(bin, _)| ((*bin as f64 * bin_hz()) - hz).abs() <= window_hz)
        .map(|(_, m)| m * m)
        .sum();
    near / total
}

#[test]
fn a_partial_lands_where_the_stretch_puts_it() {
    for stretch in [-0.05, 0.0, 0.05] {
        let spec = BakeSpec {
            stretch,
            harmonic: 8,
            ..BakeSpec::default()
        };
        let mag = mag(&spec, 200.0);
        for k in 1..=6u32 {
            let expect = partial_hz(k, 200.0, stretch);
            assert!(
                share(&mag, expect, 30.0) > 0.01,
                "stretch {stretch}, partial {k}: nothing near {expect}"
            );
        }
    }
}

#[test]
fn the_energy_sits_at_the_partials() {
    // Most of a harmonic spectrum's energy is within a narrow window of the
    // first few partials. If the smear were leaking, this would sag.
    let spec = BakeSpec::default();
    let mag = mag(&spec, 200.0);
    let concentrated: f64 = (1..=4)
        .map(|k| share(&mag, 200.0 * f64::from(k), 25.0))
        .sum();
    assert!(concentrated > 0.7, "diffuse: {concentrated}");
}

#[test]
fn bandwidth_widens_a_band_without_retilting_the_spectrum() {
    // The gotcha this guards: a timbre knob that changes summed energy is a
    // loudness control in disguise (R-713). Each harmonic is scaled by 1/√σ so
    // that widening spreads its energy without changing how much there is.
    // A longer table than the other tests use, on purpose. The 1/√σ scaling is
    // derived from an integral, and the spectrum is a discrete sum: a band only
    // a bin or two wide is sampled too coarsely for the two to agree, and the
    // one-bin floor in `spectrum` pins the narrowest bands right there. At 2^14
    // a 60-cent band measures 5 % light against a 300-cent one — a property of
    // the measurement, not of the knob, though the first version of this test
    // blamed the knob. At 2^16 both are several σ wide and they agree.
    //
    // And **one harmonic**, which is what the claim is about. A full series
    // does gain energy as the bands widen, because neighbouring bands begin to
    // overlap and magnitudes add coherently where they do — that is the sound
    // of the knob, not a leak. Measuring the series instead of the harmonic
    // conflates the two, and the second version of this test did.
    const FINE: usize = 1 << 16;
    let fine_bin = f64::from(RATE) / FINE as f64;
    let energy = |bandwidth: f64| {
        let spec = BakeSpec {
            bandwidth,
            harmonic: 1,
            ..BakeSpec::default()
        };
        let amplitude = profile(&spec, 300.0);
        let mag = spectrum(&spec, &amplitude, 300.0, RATE, FINE, band_limit(RATE));
        let total: f64 = mag.iter().map(|m| m * m).sum();
        let near: f64 = mag
            .iter()
            .enumerate()
            .filter(|(bin, _)| ((*bin as f64 * fine_bin) - 300.0).abs() <= 10.0)
            .map(|(_, m)| m * m)
            .sum();
        (total, near / total)
    };

    let (narrow_total, narrow_share) = energy(60.0);
    let (wide_total, wide_share) = energy(300.0);

    assert!(
        (wide_total / narrow_total - 1.0).abs() < 0.02,
        "energy moved: {narrow_total} → {wide_total}"
    );
    assert!(
        wide_share < narrow_share * 0.8,
        "the band did not widen: {narrow_share} → {wide_share}"
    );
}

#[test]
fn nothing_is_placed_above_the_band_limit() {
    // The guarantee, at the spectrum stage. Three things had to be limited and
    // this fails if any one of them was missed: the partial centres, the
    // Gaussian skirts that extend past them, and the broadband air.
    for spec in [
        BakeSpec::default(),
        BakeSpec {
            harmonic: 512,
            ..BakeSpec::default()
        },
        BakeSpec {
            noise: 1.0,
            ..BakeSpec::default()
        },
        BakeSpec {
            bandwidth: 600.0,
            harmonic: 256,
            ..BakeSpec::default()
        },
    ] {
        let ceiling = band_limit(RATE);
        for base in [55.0, 440.0, 3_520.0] {
            let mag = mag(&spec, base);
            for (bin, value) in mag.iter().enumerate() {
                let f = bin as f64 * bin_hz();
                if f > ceiling {
                    assert_eq!(*value, 0.0, "energy at {f} Hz, above {ceiling}: {spec:?}");
                }
            }
        }
    }
}

#[test]
fn air_fills_the_spectrum_and_still_respects_the_limit() {
    let spec = BakeSpec {
        noise: 1.0,
        ..BakeSpec::default()
    };
    let mag = mag(&spec, 200.0);
    let occupied = mag.iter().filter(|m| **m > 0.0).count();
    let ceiling = (band_limit(RATE) / bin_hz()) as usize;
    // Broadband: nearly every bin below the limit, and none above it.
    assert!(
        occupied > ceiling / 2,
        "not broadband: {occupied}/{ceiling}"
    );
    assert!(occupied <= ceiling + 1, "past the limit: {occupied}");
}

#[test]
fn the_air_crossfade_is_matched_at_its_ends() {
    // "Energy-matched" means the two things being crossfaded carry the same
    // energy — so all air is as loud as no air, and Noise is a colour control.
    //
    // It does **not** mean the mixture holds energy constant on the way across.
    // Magnitudes are non-negative and the two spectra barely overlap, so the
    // midpoint sums to about half the energy of either end: −3 dB at Noise 0.5.
    // The first version of this test asserted the stronger claim and failed on
    // it. The level guarantee that actually matters is downstream — the bake
    // RMS-normalizes every table, at every Noise setting.
    let energy = |noise: f64| {
        let spec = BakeSpec {
            noise,
            ..BakeSpec::default()
        };
        mag(&spec, 200.0).iter().map(|m| m * m).sum::<f64>()
    };
    let dry = energy(0.0);
    let wet = energy(1.0);
    assert!(
        (wet / dry - 1.0).abs() < 0.02,
        "the ends are not matched: {dry} → {wet}"
    );
}
