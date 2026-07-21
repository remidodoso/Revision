//! Baked wavetables, and the registry a graph refers to them through.
//!
//! The bake itself is dsp-02's and touches nothing here: it is pure
//! data-in/data-out, seeded, and headless-testable, which is the whole reason
//! PADsynth splits the way it does. This crate only reads the result.

use std::sync::Arc;

/// Which table, within one instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableId(pub u16);

/// One baked table: a long, seamlessly looping waveform.
///
/// `Arc` because voices share it — sixteen voices of one instrument read the
/// same half-megabyte table, and copying it per voice would be absurd. Cloning
/// an `Arc` is app-side; the real-time thread only ever reads through one.
#[derive(Debug, Clone)]
pub struct Table {
    sample: Arc<[f32]>,
    /// The frequency the table was baked at. A read head plays it at
    /// `note_hz / base_hz`, so resampling stays within about a half octave.
    base_hz: f32,
}

impl Table {
    pub fn new(sample: impl Into<Arc<[f32]>>, base_hz: f32) -> Table {
        Table {
            sample: sample.into(),
            base_hz: base_hz.max(f32::MIN_POSITIVE),
        }
    }

    pub fn len(&self) -> usize {
        self.sample.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sample.is_empty()
    }

    pub fn base_hz(&self) -> f32 {
        self.base_hz
    }

    pub fn sample(&self) -> &[f32] {
        &self.sample
    }

    /// Linear interpolation at a fractional position, wrapping.
    ///
    /// Linear rather than something better on purpose, and the reason is worth
    /// stating carefully, because it changed once already.
    ///
    /// The triangle kernel is itself a lowpass — `sinc²`, about −8 dB on a
    /// near-Nyquist partial and −27 dB on the reconstruction images. In the
    /// geometry this voice was ported from, where a table's content runs all
    /// the way to Nyquist and is then read up to 1.41×, that attenuation is
    /// **load-bearing**: it is one of the four reasons the instrument sounds
    /// clean (dsp-02 §4.2), and "upgrading" to cubic there would have removed a
    /// filter and made it dirtier.
    ///
    /// Under Revision's geometry the argument is weaker, and honesty requires
    /// saying so: half-octave bases and a band limit at `Nyquist/r_max` mean
    /// nothing crosses Nyquist, so a higher-order interpolator would be weakly
    /// *better*. It stays linear because the measurement says it barely
    /// matters — Catmull-Rom moved the residual inharmonic energy by 2 dB
    /// (dsp-02 §13.2), which is not the dominant term. What dominates is the
    /// resampling images, and only oversampled reading removes those.
    pub fn read(&self, position: f64) -> f32 {
        if self.sample.is_empty() {
            return 0.0;
        }
        let len = self.sample.len();
        let wrapped = position.rem_euclid(len as f64);
        let index = wrapped as usize;
        let fraction = (wrapped - index as f64) as f32;
        let a = self.sample[index];
        let b = self.sample[(index + 1) % len];
        a + (b - a) * fraction
    }
}

/// Every table one instrument owns. Built app-side, read by voices.
#[derive(Debug, Clone, Default)]
pub struct TableSet {
    table: Vec<Table>,
}

impl TableSet {
    pub fn new() -> TableSet {
        TableSet::default()
    }

    pub fn add(&mut self, table: Table) -> TableId {
        self.table.push(table);
        TableId((self.table.len() - 1) as u16)
    }

    pub fn get(&self, id: TableId) -> Option<&Table> {
        self.table.get(id.0 as usize)
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// The table baked closest to `hz`, measured in octaves.
    ///
    /// **Why the engine chooses and not the instrument.** A set is baked every
    /// half octave so that no table is ever read more than a quarter octave
    /// from where it was made — which is what bounds the playback rate, and the
    /// bound is what lets the bake band-limit hard enough that no partial can
    /// cross Nyquist (dsp-02 §4.4). Reading the wrong table does not sound
    /// wrong, it sounds *aliased*, so the choice belongs next to the reading.
    ///
    /// Distance in octaves, not in hertz: the tables are geometrically spaced,
    /// so a linear nearest would pick the one above almost every time.
    pub fn nearest(&self, hz: f32) -> Option<TableId> {
        if hz <= 0.0 {
            return None;
        }
        self.table
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let distance = |t: &Table| (hz / t.base_hz()).log2().abs();
                distance(a).total_cmp(&distance(b))
            })
            .map(|(index, _)| TableId(index as u16))
    }
}
