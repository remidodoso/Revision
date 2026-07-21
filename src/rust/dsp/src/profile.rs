//! The harmonic amplitude profile: `A_k = source(k) × formant_mask(k·f0)`.
//!
//! Pure functions of the spec and the base frequency — the formants live at
//! fixed hertz, so the mask has to know where the partials landed.

use crate::spec::{BakeSpec, Source, Vowel};

/// Relative gains of F1, F2, F3.
pub const FORMANT_GAIN: [f64; 3] = [1.0, 0.6, 0.4];

/// The glottal rolloff exponent of [`Source::Voice`].
pub const GLOTTAL_TILT: f64 = 1.1;

/// The thinnest pulse duty. Kept off zero so the pulse never collapses to
/// silence — a skinny, bright, buzzy band-limited pulse instead.
pub const PULSE_MIN_DUTY: f64 = 0.03;

/// Magnitude of a bandpass resonator at `f`: unity at the centre, skirts
/// falling by `q`. The analytic stand-in for running the source through a
/// formant bank.
pub fn resonator(f: f64, fc: f64, q: f64) -> f64 {
    if f <= 0.0 || fc <= 0.0 {
        return 0.0;
    }
    let x = f / fc - fc / f;
    1.0 / (1.0 + q * q * x * x).sqrt()
}

/// The universal formant mask — a spectral envelope that shapes *any* source's
/// harmonics, and the air noise with them.
///
/// `Vowel::None` returns a flat 1.0, so a saw is unshaped by default, and
/// `Source::Voice` times a vowel is a choir.
pub fn formant_mask(f: f64, vowel: Vowel, size: f64, q: f64) -> f64 {
    let Some(formant) = vowel.formant() else {
        return 1.0;
    };
    (0..3)
        .map(|i| FORMANT_GAIN[i] * resonator(f, formant[i] * size, q))
        .sum()
}

/// `A_1 .. A_n` for a spec, at a base frequency.
pub fn profile(spec: &BakeSpec, base_hz: f64) -> Vec<f64> {
    let n = spec.harmonic.max(1) as usize;
    let shape = spec.shape.clamp(0.0, 1.0);
    // Pulse duty, and the saw→triangle symmetry, from the one Shape control.
    let duty = 0.5 - shape * (0.5 - PULSE_MIN_DUTY);
    let symmetry = shape * 0.5;

    (1..=n)
        .map(|k| {
            let k64 = k as f64;
            let raw = match spec.source {
                Source::Pulse => (std::f64::consts::PI * k64 * duty).sin().abs() / k64,
                Source::Voice => 1.0 / k64.powf(GLOTTAL_TILT),
                Source::Tilt => 1.0 / k64.powf(spec.tilt),
                Source::Saw => {
                    if shape <= 0.0 {
                        1.0 / k64
                    } else {
                        // The 1/(s(1−s)) scale of a real triangle is
                        // k-independent, so it drops out under RMS
                        // normalization and is omitted.
                        (std::f64::consts::PI * k64 * symmetry).sin().abs() / (k64 * k64)
                    }
                }
            };
            raw * formant_mask(base_hz * k64, spec.vowel, spec.size, spec.formant_q)
        })
        .collect()
}

#[cfg(test)]
mod test;
