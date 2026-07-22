//! The honest live-path latency estimate (midi-03, R-307 v0).
//!
//! **Honest, not precise** — the difference matters. The true MIDI-to-sound
//! delay has a term the platform will not tell us: on Windows shared-mode
//! WASAPI the audio engine buffers beyond our callback by an amount `cpal` does
//! not expose (eng-01 §15 already noted end-to-end latency on a shared device
//! "means nothing"). So we report the part we *can* measure — a floor — and say
//! plainly that the real figure is larger.
//!
//! The floor is two block periods:
//!
//! - **scheduling** — a live note pushed to the thru ring waits up to one block
//!   for the next callback to drain it;
//! - **output** — the block the callback fills then drains to the converter,
//!   another block period at least.
//!
//! Both are the *observed* block size, not an assumed one: the host may hand a
//! different size each callback, so the number tracks reality (R-307).

use rev_engine::Position;

/// A latency estimate, in milliseconds, with its terms kept apart so the print
/// can show what it is made of rather than a bare number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    /// One block: the worst-case wait for the thru ring to be drained.
    pub scheduling_ms: f32,
    /// One block at least: the filled buffer draining to the converter.
    pub output_ms: f32,
    /// The device output latency the engine could measure, if any. Zero on a
    /// device that does not report it (most), which is why the floor exists.
    pub device_ms: f32,
}

impl Estimate {
    /// From a position snapshot. `None` until a block has been observed — before
    /// the stream's first callback there is nothing honest to say.
    pub fn from(position: &Position) -> Option<Estimate> {
        if position.block_frames == 0 || position.sample_rate == 0 {
            return None;
        }
        let ms = |frames: u32| frames as f32 / position.sample_rate as f32 * 1000.0;
        Some(Estimate {
            scheduling_ms: ms(position.block_frames),
            output_ms: ms(position.block_frames),
            device_ms: ms(position.latency_out),
        })
    }

    /// The measurable floor. The real figure is larger by the platform's own
    /// buffering, which is why callers should render it as "≥".
    pub fn floor_ms(&self) -> f32 {
        self.scheduling_ms + self.output_ms + self.device_ms
    }

    /// A one-line human summary — the honest print.
    pub fn summary(&self, block_frames: u32, sample_rate: u32) -> String {
        format!(
            "live MIDI latency ≥ {:.1} ms (scheduling {:.1} + output {:.1}{}); \
             block {block_frames} frames @ {sample_rate} Hz; \
             shared-mode buffering adds an unmeasured amount",
            self.floor_ms(),
            self.scheduling_ms,
            self.output_ms,
            if self.device_ms > 0.0 {
                format!(" + device {:.1}", self.device_ms)
            } else {
                String::new()
            },
        )
    }
}

#[cfg(test)]
mod test;
