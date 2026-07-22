//! The note→Hz snapshot: R-312 on the hot path (midi-01 §5).
//!
//! The engine must never see a note number; MIDI *is* note numbers. So the
//! resolution happens here, **above** the engine, and it must be RT-safe on the
//! MIDI thread — no lock, no allocation, no store access. A snapshot is a plain
//! 128-entry table, `note number → frequency`, built app-side from a tuning and
//! read by the fast path with one array index.
//!
//! When the tuning changes the snapshot is rebuilt and swapped atomically (the
//! app holds an `ArcSwap`-style pointer; that plumbing is midi-02's). What
//! matters here is the *type*: a value the fast path can read without touching
//! anything shared and mutable.
//!
//! **This is also midi-04's hook.** The snapshot is "incoming key → Hz." Nothing
//! says it has to be a plain tuning — composed with a keyboard map it becomes a
//! live scale remap, swapped by the same atomic swap. Building resolution as a
//! flat table now is what makes the remap nearly free later.

use rev_core::NoteNumber;
use rev_core::tuning::MaterializedTuning;

/// MIDI notes are 0..=127.
pub const NOTES: usize = 128;

/// A frozen note→Hz table for the 128 MIDI keys. `0.0` means "this key resolves
/// to nothing" — a note the tuning does not reach — and the fast path treats a
/// zero as *do not sound*, the same answer the engine would give.
#[derive(Debug, Clone)]
pub struct NoteHz {
    hz: [f32; NOTES],
}

impl NoteHz {
    /// The silent snapshot: every key resolves to nothing. What you have before
    /// a tuning is loaded, and a safe default — an unresolved key makes no sound
    /// rather than a wrong one.
    pub fn silent() -> NoteHz {
        NoteHz { hz: [0.0; NOTES] }
    }

    /// Build from a materialized tuning: each MIDI key resolves exactly as the
    /// engine resolves it, because it is the same table (`freq`) the compiler
    /// and the roll read. That identity is what makes *what you play is what you
    /// hear* structural rather than hopeful (R-312).
    pub fn from_tuning(tuning: &MaterializedTuning) -> NoteHz {
        let mut hz = [0.0f32; NOTES];
        for (key, slot) in hz.iter_mut().enumerate() {
            *slot = tuning
                .freq(NoteNumber(key as i32))
                .map(|f| f as f32)
                .unwrap_or(0.0);
        }
        NoteHz { hz }
    }

    /// Resolve a key. The whole hot-path cost: one array read, no branch worth
    /// naming, no allocation. A key out of range or unresolved returns `None`,
    /// which the fast path renders as silence.
    #[inline]
    pub fn resolve(&self, key: u8) -> Option<f32> {
        match self.hz.get(key as usize).copied() {
            Some(hz) if hz > 0.0 => Some(hz),
            _ => None,
        }
    }
}

#[cfg(test)]
mod test;
