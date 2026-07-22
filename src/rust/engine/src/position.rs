//! Where the transport is, published every block.
//!
//! **A seqlock, not a ring, and the difference matters.** The Control Bar's
//! counter wants "where is the transport *now*". If the UI thread stalls for
//! three frames, a ring would make it read three-frames-stale values and then
//! catch up — so the counter would lag and then lurch. A seqlock always gives
//! the reader the newest value and never blocks the writer (eng-01 §4.2).
//!
//! This is the one structure eng-01 writes rather than takes from `rtrb`,
//! because it is a different problem: one slot, latest wins, no queueing. We do
//! not reimplement anything we also depend on.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU32, Ordering, fence};

use crate::time::SampleTime;

/// Everything the app can learn about the engine without asking it.
///
/// `Copy` and POD: the whole value is written and read as one unit.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Position {
    /// The engine session clock — advances every block, always.
    pub at: SampleTime,
    /// The transport position — advances only while running. What the counter
    /// displays.
    pub play: SampleTime,
    pub running: bool,
    pub loop_on: bool,
    pub loop_from: SampleTime,
    pub loop_to: SampleTime,

    pub sample_rate: u32,
    /// Blocks processed since the stream opened.
    pub block: u64,
    /// The most recent callback's block size, in frames. Not fixed — the host
    /// may hand a different size each time — so it is observed, not assumed. The
    /// live-path latency floor is built from it (R-307).
    pub block_frames: u32,
    /// Callbacks that did not make their deadline.
    pub xrun: u64,
    /// Peak output since the last block, per channel, linear.
    pub peak: [f32; 2],

    /// Last and worst callback duration, microseconds. Meaningful on any device
    /// — unlike end-to-end latency, which on shared-mode HDMI means nothing
    /// (eng-01 §15).
    pub callback_us: u32,
    pub callback_worst_us: u32,

    /// The clock-domain correlation pair: this sample position was observed at
    /// this monotonic instant (nanoseconds since an arbitrary origin). The app
    /// fits a line over a short history of these, which also yields observed
    /// sample-clock drift as a number we can display (R-603, R-814).
    pub correlate_at: SampleTime,
    pub correlate_nanos: u64,

    /// Latency terms the engine knows, in frames — **ours only** (R-310): the
    /// device buffers and our own path. Not a latency model; the numbers the
    /// model has nowhere else to get.
    pub latency_in: u32,
    pub latency_out: u32,
}

/// The published slot.
///
/// # Safety posture
/// A seqlock is read while the writer may be writing, which the abstract machine
/// calls a data race. The accepted mitigation, used here, is volatile access on
/// both sides plus acquire/release fences around the sequence counter: the
/// compiler may not fuse or reorder the payload access across the fences, and a
/// torn read is detected by the counter and retried. This is the standard
/// pragmatic construction; it is stated rather than glossed.
pub struct PositionCell {
    seq: AtomicU32,
    cell: UnsafeCell<Position>,
}

// SAFETY: all access goes through `publish`/`read`, which coordinate via `seq`.
// Exactly one writer (the real-time thread) is permitted.
unsafe impl Sync for PositionCell {}
unsafe impl Send for PositionCell {}

impl Default for PositionCell {
    fn default() -> PositionCell {
        PositionCell::new()
    }
}

impl PositionCell {
    pub fn new() -> PositionCell {
        PositionCell {
            seq: AtomicU32::new(0),
            cell: UnsafeCell::new(Position::default()),
        }
    }

    /// Publish. **Real-time thread only, and only one of them.** Wait-free:
    /// two stores and a copy, no branch that can spin.
    pub fn publish(&self, position: Position) {
        let seq = self.seq.load(Ordering::Relaxed);
        // Odd: a write is in progress.
        self.seq.store(seq.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        // SAFETY: sole writer; readers detect the odd counter and retry.
        unsafe { std::ptr::write_volatile(self.cell.get(), position) };
        fence(Ordering::Release);
        // Even: the value is whole again.
        self.seq.store(seq.wrapping_add(2), Ordering::Relaxed);
    }

    /// Read the newest whole value. Retries on a torn read, which the writer's
    /// wait-freedom bounds to a couple of attempts.
    pub fn read(&self) -> Position {
        loop {
            let before = self.seq.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            // SAFETY: volatile, and validated by the counter check below.
            let position = unsafe { std::ptr::read_volatile(self.cell.get()) };
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Acquire) == before {
                return position;
            }
            std::hint::spin_loop();
        }
    }
}

#[cfg(test)]
mod test;
