//! The Padlington instrument, assembled.
//!
//! The join between a bake that knows nothing about engines and an engine that
//! knows nothing about bakes: `rev-dsp` returns samples, `rev-engine` wants
//! tables, and this is the six lines in between. It lives app-side because
//! **this is where allocating is allowed** — sixteen tables of half a megabyte,
//! built once, before the instrument is handed across the seam.

use rev_dsp::BakeSpec;
use rev_dsp::bake::{BASE_HIGH, BASE_LOW, TABLE_LEN, bake, base_hz};
use rev_engine::{Instrument, Patch, Table, TableSet};

/// The pitch the table set is anchored to, and the patch's filter reference.
///
/// Middle C for now. The inventory calls this the one 12-ET-flavoured constant
/// in the port; in Revision it becomes the tuning's anchor frequency, and when
/// it does, nothing here moves — the bases are `anchor · 2^(n/2)` either way.
pub const ANCHOR: f64 = rev_engine::instrument::MIDDLE_C as f64;

/// Bake the whole half-octave set for a patch.
///
/// **Sixteen tables, 8 MB, a few hundred milliseconds, once.** The spacing is
/// what bounds the playback rate, and the bound is what lets the bake
/// band-limit hard enough that no partial can cross Nyquist (dsp-02 §4.4).
pub fn table(spec: &BakeSpec, sample_rate: u32) -> TableSet {
    let mut set = TableSet::new();
    for n in BASE_LOW..=BASE_HIGH {
        let base = base_hz(ANCHOR, n);
        set.add(Table::new(
            bake(spec, base, sample_rate, TABLE_LEN),
            base as f32,
        ));
    }
    set
}

/// The instrument MHALL plays: a plucked Padlington.
pub fn instrument(
    patch: Patch,
    spec: &BakeSpec,
    voices: usize,
    sample_rate: u32,
) -> Option<Instrument> {
    Instrument::new(patch, table(spec, sample_rate), voices, sample_rate).ok()
}
