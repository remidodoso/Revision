//! Engine time: samples, and nothing else.
//!
//! There is no timer anywhere in this system. The callback is the clock
//! (R-302): a `SampleTime` is advanced only by [`Engine::process`], and it is
//! the only authoritative time the engine has.
//!
//! Musical position — bars, beats, ticks — is derived *above* the seam through
//! the tempo map. The engine never learns that tempo exists, which is what makes
//! polytempo (R-416) free: N tempo streams compile to N sample-stamped lists
//! merged in sample time.

use std::ops::{Add, Sub};

/// Frames since the engine session started.
///
/// `u64` and never reset within a session: at 96 kHz it overflows in six
/// million years.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct SampleTime(pub u64);

/// "At the start of the next block processed" — what a button press carries.
///
/// A sentinel rather than a separate field, so that every command has exactly
/// one time and the dispatch path has one shape (eng-01 §7).
pub const NOW: SampleTime = SampleTime(u64::MAX);

impl SampleTime {
    pub fn is_now(self) -> bool {
        self == NOW
    }

    /// Seconds, for display and for logging. Never used in scheduling — the
    /// whole point of sample time is that scheduling does not round-trip
    /// through a float.
    pub fn seconds(self, sample_rate: u32) -> f64 {
        self.0 as f64 / f64::from(sample_rate)
    }

    pub fn from_seconds(seconds: f64, sample_rate: u32) -> SampleTime {
        SampleTime((seconds * f64::from(sample_rate)).max(0.0) as u64)
    }

    /// Saturating throughout: a locate to before zero is zero, not a wrap into
    /// six million years.
    pub fn saturating_sub(self, other: SampleTime) -> SampleTime {
        SampleTime(self.0.saturating_sub(other.0))
    }
}

impl Add<u64> for SampleTime {
    type Output = SampleTime;
    fn add(self, frames: u64) -> SampleTime {
        SampleTime(self.0.saturating_add(frames))
    }
}

impl Sub<SampleTime> for SampleTime {
    type Output = u64;
    fn sub(self, other: SampleTime) -> u64 {
        self.0.saturating_sub(other.0)
    }
}

#[cfg(test)]
mod test;
