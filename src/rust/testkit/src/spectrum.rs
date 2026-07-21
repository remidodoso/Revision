//! Listening, for tests.
//!
//! **The debt this repays.** In eng-07 an entire run of MHALL played at the
//! wrong pitch with every test green: bit-identity passes when wrong is
//! reproducible, onsets passed because the timing was genuinely right, and
//! nothing in the suite listened to *pitch*. The defect was heard, not
//! measured. Nothing here is clever; it is the smallest thing that lets a test
//! assert what a note actually sounds like.

use realfft::RealFftPlanner;

/// Half-width of the Blackman-Harris main lobe, in bins. What a window costs
/// for its quiet sidelobes: a single sine occupies nine bins, not one.
pub const MAIN_LOBE: usize = 4;

/// A rendered buffer's magnitude spectrum.
pub struct Spectrum {
    pub magnitude: Vec<f64>,
    pub bin_hz: f64,
}

/// Analyse one channel of a rendered buffer.
///
/// **Blackman-Harris windowed**, not rectangular and not Hann. The point of
/// most of these measurements is to find quiet things next to loud ones —
/// aliasing 60 dB below a partial — and a rectangular window's leakage would
/// bury that in the skirts of the partial itself. Blackman-Harris puts its
/// sidelobes near −92 dB, comfortably below anything asserted on.
pub fn analyse(sample: &[f32], sample_rate: u32) -> Spectrum {
    // A power of two at or below the input length: the transform is happiest
    // there, and the caller's slice is arbitrary.
    let len = sample.len().next_power_of_two() / if sample.len().is_power_of_two() { 1 } else { 2 };
    let mut planner = RealFftPlanner::<f64>::new();
    let forward = planner.plan_fft_forward(len);
    let mut input = forward.make_input_vec();
    for (slot, (index, value)) in input.iter_mut().zip(sample.iter().enumerate()) {
        let t = index as f64 / (len - 1) as f64 * std::f64::consts::TAU;
        let window =
            0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos();
        *slot = f64::from(*value) * window;
    }
    let mut output = forward.make_output_vec();
    forward
        .process(&mut input, &mut output)
        .expect("the buffers come from the plan itself");
    Spectrum {
        magnitude: output.iter().map(|c| c.norm()).collect(),
        bin_hz: f64::from(sample_rate) / len as f64,
    }
}

impl Spectrum {
    pub fn hz_of(&self, bin: usize) -> f64 {
        bin as f64 * self.bin_hz
    }

    pub fn bin_of(&self, hz: f64) -> usize {
        (hz / self.bin_hz).round() as usize
    }

    pub fn peak(&self) -> f64 {
        self.magnitude.iter().cloned().fold(0.0, f64::max)
    }

    /// The frequency of the loudest bin, interpolated to sub-bin accuracy.
    ///
    /// A parabola through the peak and its two neighbours — without it the
    /// answer is only as precise as the bin spacing, which is far too coarse to
    /// tell a right note from a slightly wrong one.
    pub fn loudest_hz(&self) -> f64 {
        let Some(bin) = (1..self.magnitude.len() - 1)
            .max_by(|a, b| self.magnitude[*a].total_cmp(&self.magnitude[*b]))
        else {
            return 0.0;
        };
        let (left, mid, right) = (
            self.magnitude[bin - 1],
            self.magnitude[bin],
            self.magnitude[bin + 1],
        );
        let denominator = left - 2.0 * mid + right;
        let offset = if denominator.abs() > f64::EPSILON {
            0.5 * (left - right) / denominator
        } else {
            0.0
        };
        (bin as f64 + offset) * self.bin_hz
    }

    /// The lowest partial carrying real energy — the note's pitch.
    ///
    /// Not simply the loudest bin: a bright patch, or a formant, can easily put
    /// more energy in the second or third partial than in the first.
    pub fn fundamental_hz(&self, floor_hz: f64) -> f64 {
        let peak = self.peak();
        let from = self.bin_of(floor_hz).max(MAIN_LOBE);
        for bin in from..self.magnitude.len() - MAIN_LOBE {
            let m = self.magnitude[bin];
            // The maximum of its whole neighbourhood, not merely of its two
            // neighbours. A partial is a lobe several bins wide — wider still
            // once PADsynth has smeared it into a band — and the first local
            // wobble on its rising skirt is not the partial. Taking it reported
            // high notes a flat 28 cents low.
            let local = (bin - MAIN_LOBE..=bin + MAIN_LOBE)
                .map(|b| self.magnitude[b])
                .fold(0.0f64, f64::max);
            if m > peak * 0.15 && m >= local {
                return self.centroid_near(self.hz_of(bin));
            }
        }
        0.0
    }

    /// The energy-weighted centre of the partial nearest `hz`.
    ///
    /// **Not its loudest bin.** A PADsynth partial is a narrow *noise band* —
    /// that is the whole method — and every bin in it carries an independent
    /// random phase, so the band's peak lands anywhere inside it and wanders
    /// from bake to bake. Reading the peak reported a note 16 cents flat, which
    /// was the realization, not the pitch. The centroid is what the ear hears
    /// and what the bake actually places.
    pub fn centroid_near(&self, hz: f64) -> f64 {
        // ±2 % is about 35 cents: wider than the default 25-cent band, narrower
        // than the gap to the next partial.
        let window = (hz * 0.02).max(MAIN_LOBE as f64 * self.bin_hz);
        let mut weight = 0.0;
        let mut moment = 0.0;
        for (bin, magnitude) in self.magnitude.iter().enumerate() {
            let f = self.hz_of(bin);
            if (f - hz).abs() <= window {
                let energy = magnitude * magnitude;
                weight += energy;
                moment += energy * f;
            }
        }
        if weight > 0.0 { moment / weight } else { hz }
    }

    /// The loudest bin that is **not** near a partial of `f0`, as a ratio to the
    /// loudest bin that is.
    ///
    /// This is the aliasing measurement. `tolerance_cent` has to be wide enough
    /// to contain the Gaussian band a partial is smeared into — PADsynth's
    /// partials are bands, not lines, and calling the band's own shoulders
    /// "inharmonic" would measure the instrument's whole point as a defect.
    ///
    /// The exclusion is the **wider** of that tolerance and [`MAIN_LOBE`] bins.
    /// A window has a main lobe as well as sidelobes, and Blackman-Harris pays
    /// for its −92 dB skirts with a wide one: four bins either side, which at a
    /// low fundamental is several hundred cents. The first version of this
    /// measured a mathematically pure harmonic series as 6 dB dirty, because a
    /// tolerance in cents alone was narrower than the lobe of the very partial
    /// it was excluding.
    pub fn worst_inharmonic(&self, f0: f64, tolerance_cent: f64, floor_hz: f64) -> f64 {
        let harmonic = self.peak();
        if harmonic <= 0.0 || f0 <= 0.0 {
            return 0.0;
        }
        let guard_hz = MAIN_LOBE as f64 * self.bin_hz;
        let mut worst = 0.0f64;
        for (bin, magnitude) in self.magnitude.iter().enumerate() {
            let f = self.hz_of(bin);
            if f < floor_hz {
                continue;
            }
            let k = (f / f0).round().max(1.0);
            let near = k * f0;
            let cent = 1200.0 * (f / near).log2().abs();
            if cent > tolerance_cent && (f - near).abs() > guard_hz {
                worst = worst.max(*magnitude);
            }
        }
        worst / harmonic
    }

    /// Total energy in a band, as a fraction of the whole.
    pub fn share_between(&self, lo_hz: f64, hi_hz: f64) -> f64 {
        let total: f64 = self.magnitude.iter().map(|m| m * m).sum();
        if total == 0.0 {
            return 0.0;
        }
        let inside: f64 = self
            .magnitude
            .iter()
            .enumerate()
            .filter(|(bin, _)| (lo_hz..hi_hz).contains(&self.hz_of(*bin)))
            .map(|(_, m)| m * m)
            .sum();
        inside / total
    }
}

/// Ratio to decibels, floored so that silence prints rather than diverging.
pub fn db(ratio: f64) -> f64 {
    20.0 * ratio.max(1e-12).log10()
}

#[cfg(test)]
mod test;
