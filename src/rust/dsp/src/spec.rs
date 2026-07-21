//! What a bake is a function of — and nothing else.
//!
//! The partition matters more than the fields. Everything here re-bakes a table
//! when it changes; everything *not* here (envelopes, filter, width, pitch
//! attack) is play-time and must never re-bake. The inventory validated that
//! split against the source, and it is what makes the type a cache key.

/// The raw carrier, before the formant mask.
///
/// Pulse and Saw are one-parameter morphs of the same family, `|sin(πkx)|/kᵉ` —
/// because the bake randomizes phase, only magnitudes matter, so a waveshape
/// *is* its harmonic-magnitude profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Source {
    #[default]
    Saw,
    Pulse,
    /// A glottal `1/k^1.1` rolloff — the vocal carrier.
    Voice,
    /// A bare `1/kᵉ`, the abstract profile.
    Tilt,
}

/// Formant centres, in hertz. `None` is a bypass, not a vowel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Vowel {
    #[default]
    None,
    Ooh,
    Oh,
    Ah,
    Eh,
    Ee,
}

impl Vowel {
    /// F1, F2, F3. `None` has none, which is what makes the mask flat.
    pub fn formant(self) -> Option<[f64; 3]> {
        match self {
            Vowel::None => None,
            Vowel::Ooh => Some([350.0, 600.0, 2400.0]),
            Vowel::Oh => Some([430.0, 820.0, 2600.0]),
            Vowel::Ah => Some([800.0, 1150.0, 2900.0]),
            Vowel::Eh => Some([500.0, 1800.0, 2550.0]),
            Vowel::Ee => Some([300.0, 2300.0, 3010.0]),
        }
    }
}

/// The twelve bake-relevant fields. Defaults are the inventory's, unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakeSpec {
    pub source: Source,
    /// 0..1. Saw→triangle morph, or pulse duty 0.5→0.03.
    pub shape: f64,
    /// The exponent of [`Source::Tilt`].
    pub tilt: f64,
    /// How many partials the profile has.
    pub harmonic: u32,
    pub vowel: Vowel,
    /// Formant-centre scale — vocal tract size.
    pub size: f64,
    pub formant_q: f64,
    /// 0..1 air blend, an energy-matched crossfade.
    pub noise: f64,
    /// The air high-pass corner, in hertz.
    pub air_cut: f64,
    /// Gaussian smear, in cents. The lushness.
    pub bandwidth: f64,
    /// How the smear grows up the series. 1 = constant cents.
    pub bw_scale: f64,
    /// Partial `k` lands at `f0·k^(1+stretch)` — the Sethares hook.
    pub stretch: f64,
}

impl Default for BakeSpec {
    fn default() -> BakeSpec {
        BakeSpec {
            source: Source::Saw,
            shape: 0.0,
            tilt: 1.5,
            harmonic: 64,
            vowel: Vowel::None,
            size: 1.0,
            formant_q: 9.0,
            noise: 0.0,
            air_cut: 30.0,
            bandwidth: 25.0,
            bw_scale: 1.0,
            stretch: 0.0,
        }
    }
}

// A bake key is an identity, so it has to be `Eq` and `Hash` — and its fields
// are floats, which are neither. Comparing bit patterns is the honest reading:
// two patches share a table when they are the same patch, and a difference too
// small to hear is still a difference we were told about.
impl Eq for BakeSpec {}

impl std::hash::Hash for BakeSpec {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.vowel.hash(state);
        self.harmonic.hash(state);
        for value in [
            self.shape,
            self.tilt,
            self.size,
            self.formant_q,
            self.noise,
            self.air_cut,
            self.bandwidth,
            self.bw_scale,
            self.stretch,
        ] {
            value.to_bits().hash(state);
        }
    }
}

impl BakeSpec {
    /// **Harpington** — the first real timbre, from Notorolla's patch catalog
    /// rather than invented here.
    ///
    /// Two fields carry the character, and neither is a default:
    ///
    /// - `bandwidth` 19.83 cents, a little tighter than the stock 25, which is
    ///   what keeps a plucked sound from blooming into a pad;
    /// - `bw_scale` **0.706**, well under 1. The smear grows *slower* than
    ///   constant-cents up the series, so the upper partials stay comparatively
    ///   narrow instead of merging into a continuum — the reason this reads as
    ///   a struck string with a defined top rather than a wash.
    ///
    /// `harmonics` arrives as 37.79 from a continuous control and rounds to 38,
    /// exactly as the source does. `tilt` is carried but unused: it belongs to
    /// [`Source::Tilt`], and this is a saw.
    pub fn harpington() -> BakeSpec {
        BakeSpec {
            source: Source::Saw,
            shape: 0.0,
            tilt: 0.892_5,
            harmonic: 38,
            vowel: Vowel::None,
            size: 0.937_5,
            formant_q: 9.0,
            noise: 0.0,
            air_cut: 30.0,
            bandwidth: 19.833_944_677_736_9,
            bw_scale: 0.706,
            stretch: 0.0,
        }
    }

    /// The bake seed: this patch, at this base, at this rate, at this length.
    ///
    /// Every one of those belongs in it. Two tables of the same patch at
    /// adjacent bases must not share phases, or the seam between them would be
    /// a correlation rather than a change of colour.
    pub fn seed(&self, base_hz: f64, sample_rate: u32, len: usize) -> u64 {
        let mut hasher = crate::rand::Hasher::default();
        hasher
            .write(&[self.source as u8, self.vowel as u8])
            .write_u64(u64::from(self.harmonic))
            .write_f64(self.shape)
            .write_f64(self.tilt)
            .write_f64(self.size)
            .write_f64(self.formant_q)
            .write_f64(self.noise)
            .write_f64(self.air_cut)
            .write_f64(self.bandwidth)
            .write_f64(self.bw_scale)
            .write_f64(self.stretch)
            .write_f64(base_hz)
            .write_u64(u64::from(sample_rate))
            .write_u64(len as u64);
        hasher.finish()
    }
}
