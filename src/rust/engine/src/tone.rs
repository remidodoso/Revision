//! The test tone.
//!
//! **A placeholder, and honestly labelled as one.** It exists so eng-03 makes an
//! audible sound with no node graph, no automation and no schedule compiler —
//! which means device, ring, clock and transport can be proved before anything
//! complicated depends on them. It is superseded, not extended, when real voices
//! arrive (eng-05).
//!
//! It does carry one thing forward that is not a placeholder: **it is given a
//! frequency, never a note number** (R-312). Every voice after it inherits that
//! shape.

use std::f64::consts::TAU;

/// Ramp length for gain changes, in seconds. A hard gate on a sine is a click,
/// and a click is indistinguishable from a scheduling bug when you are trying to
/// hear whether the scheduler works.
const RAMP: f64 = 0.005;

pub struct Tone {
    phase: f64,
    /// Radians per sample. Derived from frequency and rate, never from a
    /// hard-coded rate.
    step: f64,
    gain: f32,
    target: f32,
    /// Gain change per sample while ramping.
    slew: f32,
}

impl Tone {
    pub fn new(sample_rate: u32) -> Tone {
        Tone {
            phase: 0.0,
            step: 0.0,
            gain: 0.0,
            target: 0.0,
            slew: (1.0 / (RAMP * f64::from(sample_rate))) as f32,
        }
    }

    pub fn on(&mut self, hz: f32, gain: f32, sample_rate: u32) {
        self.step = TAU * f64::from(hz.max(0.0)) / f64::from(sample_rate);
        self.target = gain.clamp(0.0, 1.0);
    }

    pub fn off(&mut self) {
        self.target = 0.0;
    }

    pub fn is_silent(&self) -> bool {
        self.gain == 0.0 && self.target == 0.0
    }

    /// Add into a segment — *add*, not assign, so a voice never assumes it is
    /// the only thing in the buffer.
    pub fn render(&mut self, out: &mut [f32]) {
        for sample in out.iter_mut() {
            if self.gain < self.target {
                self.gain = (self.gain + self.slew).min(self.target);
            } else if self.gain > self.target {
                self.gain = (self.gain - self.slew).max(self.target);
            }
            *sample += (self.phase.sin() as f32) * self.gain;
            self.phase += self.step;
            // Wrap rather than letting the phase grow: a large f64 phase loses
            // precision in `sin`, and the loss is audible as slow detuning.
            if self.phase >= TAU {
                self.phase -= TAU;
            }
        }
    }
}

#[cfg(test)]
mod test;
