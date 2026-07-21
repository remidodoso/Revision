//! The bake: spectrum → seeded phase → one inverse FFT → a looping table.
//!
//! And the geometry that goes with it, which is the part that decides whether
//! the instrument sounds clean.

use realfft::RealFftPlanner;
use realfft::num_complex::Complex;

use crate::spec::BakeSpec;

/// Table length, ~2.7 s at 48 kHz. The IFFT is a one-time cost at bake time.
pub const TABLE_LEN: usize = 1 << 17;

/// Every table is normalized to this RMS, so Source, Harmonics and Bandwidth
/// change colour and never loudness (R-713). **The bake's constant, so it lives
/// here**; the play-time levels stay with the instrument.
pub const TABLE_RMS: f32 = 0.25;

/// Bases are spaced a **half octave** apart, so a table is never read further
/// than a quarter octave from where it was baked.
///
/// The alternative — octave spacing, as the source uses — puts the lowest
/// frequency any fold-back can reach at `SR·(1 − √2/2) ≈ 14.1 kHz`, which is
/// inside ordinary adult hearing. Half-octave spacing moves that to 19.5 kHz
/// *and*, more importantly, makes [`band_limit`] affordable (dsp-02 §4.4).
pub const RATE_MAX: f64 = 1.189_207_115_002_721; // 2^(1/4)

/// Base index range, relative to the anchor, in half-octaves: `2^(-7/2)` to
/// `2^(8/2)`. At a middle-C anchor that is 23 Hz to 4186 Hz — the piano, and
/// then some.
pub const BASE_LOW: i32 = -7;
pub const BASE_HIGH: i32 = 8;
pub const BASE_COUNT: usize = (BASE_HIGH - BASE_LOW + 1) as usize;

/// The highest frequency any partial may occupy: Nyquist divided by the widest
/// rate a table is ever read at.
///
/// This is the whole aliasing argument in one line. A partial placed below this
/// cannot cross Nyquist when the table is played up, so **fold-back energy is
/// zero by construction** — not 30 dB down, not moved somewhere inaudible,
/// absent.
///
/// It costs the harmonic content between 20.2 kHz and Nyquist at 48 kHz. That
/// is a real loss and worth naming: nobody hears it directly, and what it would
/// have contributed through a nonlinear reproduction chain is *harmonic*
/// intermodulation, which fuses with the note. What it prevents is *inharmonic*
/// content, which a nonlinearity translates downward as inharmonic — audible,
/// and the reason aliasing has a character at all (R-720).
pub fn band_limit(sample_rate: u32) -> f64 {
    f64::from(sample_rate) / 2.0 / RATE_MAX
}

/// The base frequency `n` half-octaves from the anchor.
pub fn base_hz(anchor_hz: f64, n: i32) -> f64 {
    anchor_hz * 2f64.powf(f64::from(n) / 2.0)
}

/// Bake one table.
///
/// All arithmetic in `f64`; only the result is `f32`. The source is `f64`
/// throughout and matching it costs nothing at bake time.
pub fn bake(spec: &BakeSpec, base: f64, sample_rate: u32, len: usize) -> Vec<f32> {
    bake_with_limit(spec, base, sample_rate, len, band_limit(sample_rate))
}

/// Bake at an explicit band limit.
///
/// Exists so that the aliasing sweep can bake the way the *source* does — every
/// partial up to Nyquist, one table per octave — and measure the difference
/// rather than take §4.4's word for it.
pub fn bake_with_limit(
    spec: &BakeSpec,
    base: f64,
    sample_rate: u32,
    len: usize,
    limit_hz: f64,
) -> Vec<f32> {
    let amplitude = crate::profile::profile(spec, base);
    let mag = crate::spectrum::spectrum(spec, &amplitude, base, sample_rate, len, limit_hz);

    // Seeded uniform phase per bin. Phase is where the source is deliberately
    // random and where we are deliberately not: the same patch bakes the same
    // table forever (R-706, R-1402).
    let mut random = crate::rand::Random::new(spec.seed(base, sample_rate, len));
    let mut planner = RealFftPlanner::<f64>::new();
    let inverse = planner.plan_fft_inverse(len);
    let mut bin = inverse.make_input_vec();
    // DC and Nyquist stay zero: both are real-only in a Hermitian spectrum, so
    // giving them a random phase would be meaningless, and a DC offset in a
    // looping table is a click waiting to happen.
    for (index, slot) in bin.iter_mut().enumerate().take(len / 2).skip(1) {
        let m = mag[index];
        if m == 0.0 {
            continue;
        }
        let phase = random.next_f64() * std::f64::consts::TAU;
        *slot = Complex::new(m * phase.cos(), m * phase.sin());
    }

    let mut sample = inverse.make_output_vec();
    inverse
        .process(&mut bin, &mut sample)
        .expect("the buffers come from the plan itself");

    // RMS normalization absorbs the transform's missing 1/N along with
    // everything else, which is why the inverse is left unscaled.
    let sum: f64 = sample.iter().map(|s| s * s).sum();
    let rms = (sum / len as f64).sqrt();
    let gain = if rms > 0.0 {
        f64::from(TABLE_RMS) / rms
    } else {
        0.0
    };
    sample.iter().map(|s| (s * gain) as f32).collect()
}

/// Bake the whole set: one table every half octave, in ascending order.
///
/// Returned as `(base_hz, sample)` pairs for the caller to make tables of —
/// this crate has never heard of `rev_engine::Table`.
///
/// **Eagerly**, all of them, at instrument load. Sixteen tables of 2^17 `f32`
/// is 8 MB and a few hundred milliseconds once. Lazy per-octave baking would
/// save the memory and cost the thing that matters: a table would have to reach
/// a *running* engine, which is a whole hot-swap problem this avoids having.
pub fn bake_set(spec: &BakeSpec, anchor_hz: f64, sample_rate: u32) -> Vec<(f32, Vec<f32>)> {
    (BASE_LOW..=BASE_HIGH)
        .map(|n| {
            let base = base_hz(anchor_hz, n);
            (base as f32, bake(spec, base, sample_rate, TABLE_LEN))
        })
        .collect()
}

#[cfg(test)]
mod test;
