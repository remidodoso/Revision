//! Profile → magnitude spectrum: each partial smeared into a Gaussian band.
//!
//! This is where PADsynth's lushness comes from — a harmonic is not a line but
//! a narrow noise band, which is a supersaw with infinite unison and no beating.
//! It is also where the band limit is enforced, and that is the part of this
//! module that decides whether the instrument sounds clean (dsp-02 §4).

use crate::profile::formant_mask;
use crate::spec::BakeSpec;

/// Where partial `k` lands. Stretch 0 is exactly harmonic; positive stretches
/// the upper partials sharp (bell, gamelan), negative compresses them flat.
pub fn partial_hz(k: u32, base_hz: f64, stretch: f64) -> f64 {
    base_hz * f64::from(k).powf(1.0 + stretch)
}

/// The magnitude spectrum, `len/2` bins, everything above `limit_hz` left at
/// zero.
///
/// **`limit_hz` is not Nyquist.** It is Nyquist divided by the widest playback
/// rate the table will ever be read at, so that no partial can cross Nyquist
/// when the table is played up, and fold-back energy is zero by construction
/// rather than merely attenuated (dsp-02 §4.4).
///
/// The limit is applied three times, and all three are needed:
///
/// - to the partial centres, as the source does;
/// - to the Gaussian **skirts**, which extend ±4·bw beyond the centre and would
///   otherwise leak a partial's energy across the limit it was placed under;
/// - to the **air**, which is broadband and would otherwise put noise at every
///   frequency up to Nyquist — the one component that aliases at *every* note
///   rather than only at high ones.
pub fn spectrum(
    spec: &BakeSpec,
    amplitude: &[f64],
    base_hz: f64,
    sample_rate: u32,
    len: usize,
    limit_hz: f64,
) -> Vec<f64> {
    let half = len / 2;
    let mut mag = vec![0.0f64; half];
    let bin_hz = f64::from(sample_rate) / len as f64;
    let bw_fraction = 2f64.powf(spec.bandwidth / 1200.0) - 1.0;
    // The highest bin any energy may occupy.
    let ceiling = ((limit_hz / bin_hz).floor() as usize).min(half.saturating_sub(1));

    for (index, &a) in amplitude.iter().enumerate() {
        let k = index as u32 + 1;
        if a <= 0.0 {
            continue;
        }
        let fk = partial_hz(k, base_hz, spec.stretch);
        if fk >= limit_hz {
            // Partial frequency grows with k, so there is nothing above.
            break;
        }
        // Floored at one bin: with no width at all the partial would snap to a
        // bin, and the pitch with it. The smear is what keeps pitch continuous
        // on a table whose bins are 0.366 Hz apart (R-943).
        let bw_hz = (bw_fraction * base_hz * f64::from(k).powf(spec.bw_scale)).max(bin_hz);
        // Full-width-half-maximum → σ, and 1/√σ so a harmonic carries the same
        // energy at any bandwidth. Without it the Bandwidth knob would retilt
        // the spectrum, which makes it a tone control in disguise (R-713).
        let sigma = bw_hz / 2.355;
        let scale = a / sigma.sqrt();
        let lo = (((fk - 4.0 * bw_hz) / bin_hz).floor() as isize).max(1) as usize;
        let hi = (((fk + 4.0 * bw_hz) / bin_hz).ceil() as usize).min(ceiling);
        for (bin, slot) in mag.iter_mut().enumerate().take(hi + 1).skip(lo) {
            let d = bin as f64 * bin_hz - fk;
            *slot += scale * (-(d * d) / (2.0 * sigma * sigma)).exp();
        }
    }

    let noise = spec.noise.clamp(0.0, 1.0);
    if noise > 0.0 {
        // Air: pink (1/√f) through a one-pole high-pass (which also tames
        // pink's blow-up at DC), shaped by the same formant mask so a vowel
        // makes the air breathy. Crossfaded at constant energy, so Air is a
        // colour control and not a level control.
        let fc = spec.air_cut.max(1.0);
        let mut air = vec![0.0f64; half];
        let tone_energy: f64 = mag.iter().map(|m| m * m).sum();
        let mut air_energy = 0.0;
        for (bin, slot) in air.iter_mut().enumerate().take(ceiling + 1).skip(1) {
            let f = bin as f64 * bin_hz;
            let r = f / fc;
            let value = (r / (1.0 + r * r).sqrt()) / f.sqrt()
                * formant_mask(f, spec.vowel, spec.size, spec.formant_q);
            *slot = value;
            air_energy += value * value;
        }
        let scale = if air_energy > 0.0 {
            (if tone_energy > 0.0 {
                tone_energy
            } else {
                air_energy
            } / air_energy)
                .sqrt()
        } else {
            0.0
        };
        for (slot, air) in mag.iter_mut().zip(&air) {
            *slot = (1.0 - noise) * *slot + noise * scale * air;
        }
    }

    mag
}

#[cfg(test)]
mod test;
