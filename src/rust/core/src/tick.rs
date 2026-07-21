//! Musical time (R-003): integer ticks at a fixed 5040 per quarter note.
//! Seconds are derived only at the engine boundary, via the tempo map.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Ticks per quarter note. 5040 = 2^4·3^2·5·7, so every duple, triple,
/// quintuple and septuple subdivision of a quarter is an exact integer —
/// tuplets never round (Vision used 480; R-003 raises it).
pub const PPQ: i64 = 5040;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tick(pub i64);

impl Tick {
    pub const ZERO: Tick = Tick(0);

    pub fn get(self) -> i64 {
        self.0
    }

    /// Ticks per whole note, per beat-fraction: `Tick::per_note_value(4)` is a
    /// quarter, `(8)` an eighth, `(3)` a half-note triplet's parent, and so on.
    pub fn per_note_value(denominator: i64) -> Tick {
        Tick(PPQ * 4 / denominator)
    }
}

impl From<i64> for Tick {
    fn from(value: i64) -> Self {
        Tick(value)
    }
}

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Seconds elapsed over `tick` ticks at a constant tempo. Tempo is stored as
/// integer microseconds per quarter (MIDI-exact; no float drift in the model),
/// so the division happens here, once, deterministically.
pub fn tick_to_second(tick: Tick, usec_per_quarter: i64) -> f64 {
    (tick.get() as f64) * (usec_per_quarter as f64) / (PPQ as f64 * 1_000_000.0)
}

/// The inverse of [`tick_to_second`], rounded to the nearest tick.
pub fn second_to_tick(second: f64, usec_per_quarter: i64) -> Tick {
    let tick = second * PPQ as f64 * 1_000_000.0 / usec_per_quarter as f64;
    Tick(tick.round() as i64)
}

/// Microseconds per quarter note for a tempo in beats per minute.
pub fn bpm_to_usec_per_quarter(bpm: f64) -> i64 {
    (60_000_000.0 / bpm).round() as i64
}

#[cfg(test)]
mod test;
