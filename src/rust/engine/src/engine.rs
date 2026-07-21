//! The whole of the real-time contract, in one function.
//!
//! [`Engine::process`] is allocation-free, lock-free, wait-free, and it always
//! returns. It knows nothing about cpal, WASAPI, or files — two drivers call it
//! (a device and an offline renderer), which is what gives eng-07's
//! render-twice bit-identity gate (R-1402) for free instead of as a parallel
//! implementation, and what lets every engine test run headless in CI.
//!
//! **Release policy for a fault here: never panic.** A voice that cannot start
//! is not started, and the failure is recorded, not thrown. Starvation produces
//! silence plus an observation, never a block and never a partial buffer.

use std::time::Instant;

use crate::command::{Command, Garbage, What};
use crate::format::{Block, Format};
use crate::guard::RtScope;
use crate::obs::{Code, Creator, Level, Obs};
use crate::port::RtPort;
use crate::position::Position;
use crate::time::SampleTime;
use crate::tone::Tone;

/// Commands held for a future block. Fixed size because the callback may not
/// allocate; overflow is recorded rather than silently absorbed.
const PENDING: usize = 256;

pub struct Engine {
    format: Format,
    port: RtPort,
    tone: Tone,

    at: SampleTime,
    play: SampleTime,
    running: bool,
    loop_on: bool,
    loop_from: SampleTime,
    loop_to: SampleTime,

    /// Scheduled commands not yet due. Kept unsorted and scanned — 256 entries
    /// is a handful of cache lines, and a heap would cost more than it saves.
    pending: [Option<Command>; PENDING],
    pending_lost: u64,

    chunk: Option<crate::command::ChunkHandle>,
    /// A chunk the return ring could not take. Retried next block rather than
    /// dropped, because dropping it would leak.
    returning: Option<Garbage>,

    block: u64,
    xrun: u64,
    worst_us: u32,
    trace: Level,
    peak: [f32; 2],
    /// Device buffer latency in frames, input and output. **Ours only** — never
    /// an instrument's own response (R-310).
    latency: (u32, u32),
    /// The origin the clock-correlation pair is measured from. Arbitrary and
    /// monotonic, which is all a correlation needs.
    origin: Instant,
}

impl Engine {
    pub fn new(format: Format, port: RtPort) -> Engine {
        Engine {
            tone: Tone::new(format.sample_rate),
            format,
            port,
            at: SampleTime(0),
            play: SampleTime(0),
            running: false,
            loop_on: false,
            loop_from: SampleTime(0),
            loop_to: SampleTime(0),
            pending: [None; PENDING],
            pending_lost: 0,
            chunk: None,
            returning: None,
            block: 0,
            xrun: 0,
            worst_us: 0,
            trace: Level::Info,
            peak: [0.0; 2],
            latency: (0, 0),
            origin: Instant::now(),
        }
    }

    pub fn format(&self) -> Format {
        self.format
    }

    /// Record the device's own latency, in frames, so R-303's model has
    /// somewhere to get it. Ours only — never an instrument's response (R-310).
    pub fn set_device_latency(&mut self, input: u32, output: u32) {
        self.latency = (input, output);
    }

    /// Frames rendered since the session opened. The offline driver reads this;
    /// so does anything asking "has it started yet".
    pub fn at(&self) -> SampleTime {
        self.at
    }

    /// One block. Everything the real-time thread ever does.
    pub fn process(&mut self, block: &mut Block<'_>) {
        let _rt = RtScope::enter();
        let started = Instant::now();

        let frame = block.frame();
        block.out.silence();

        self.take_commands(frame);
        self.render(block, frame);

        self.at = self.at + frame as u64;
        if self.running {
            self.play = self.advance_play(frame as u64);
        }
        self.block += 1;

        self.retry_return();
        self.report_dropped();

        let elapsed = started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        self.worst_us = self.worst_us.max(elapsed);
        if self.trace <= Level::Trace {
            self.port.observe(
                Obs::new(Creator::Timing, Level::Trace, Code::BlockTrace)
                    .at(self.at)
                    .arg0(frame as u64)
                    .arg1(u64::from(self.worst_us)),
            );
        }

        self.publish(elapsed, started);
    }

    /// The transport missed its deadline — the driver saw a gap. Recorded, not
    /// corrected: there is nothing to correct, only something to know.
    pub fn note_xrun(&mut self) {
        self.xrun += 1;
        self.port.observe(
            Obs::new(Creator::Stream, Level::Warn, Code::Xrun)
                .at(self.at)
                .arg0(self.xrun),
        );
    }

    /// Drain the command ring, applying what is due now and holding the rest.
    fn take_commands(&mut self, frame: usize) {
        let block_end = self.at + frame as u64;
        while let Ok(command) = self.port.command.pop() {
            if command.at.is_now() || command.at < block_end {
                self.apply(command.what);
            } else {
                self.hold(command);
            }
        }

        // Anything held that has come due. Scanned rather than sorted: see
        // `pending`. Indexed rather than iterated, because applying a command
        // needs `&mut self` and an iterator would still be holding the array.
        for index in 0..self.pending.len() {
            let Some(command) = self.pending[index] else {
                continue;
            };
            if command.at < block_end {
                self.pending[index] = None;
                self.apply(command.what);
            }
        }
    }

    fn hold(&mut self, command: Command) {
        if let Some(slot) = self.pending.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(command);
        } else {
            self.pending_lost += 1;
            self.port.observe(
                Obs::new(Creator::Sched, Level::Error, Code::PendingFull)
                    .at(self.at)
                    .arg0(self.pending_lost),
            );
        }
    }

    fn apply(&mut self, what: What) {
        match what {
            What::Start => {
                self.running = true;
                self.port.observe(
                    Obs::new(Creator::Transport, Level::Info, Code::TransportStart)
                        .at(self.at)
                        .arg0(self.play.0),
                );
            }
            What::Stop => {
                self.running = false;
                self.port.observe(
                    Obs::new(Creator::Transport, Level::Info, Code::TransportStop)
                        .at(self.at)
                        .arg0(self.play.0),
                );
            }
            What::Locate(to) => {
                self.play = to;
                self.port.observe(
                    Obs::new(Creator::Transport, Level::Info, Code::Locate)
                        .at(self.at)
                        .arg0(to.0),
                );
            }
            What::SetLoop { from, to, on } => {
                self.loop_from = from;
                self.loop_to = to;
                self.loop_on = on && to > from;
            }
            What::ToneOn { hz, gain } => {
                self.tone.on(hz, gain, self.format.sample_rate);
                self.port.observe(
                    Obs::new(Creator::Transport, Level::Info, Code::ToneOn)
                        .at(self.at)
                        .arg0((f64::from(hz) * 1000.0) as u64),
                );
            }
            What::ToneOff => {
                self.tone.off();
                self.port
                    .observe(Obs::new(Creator::Transport, Level::Info, Code::ToneOff).at(self.at));
            }
            What::TakeChunk(handle) => {
                // SAFETY: ownership has just crossed to this side, and a chunk
                // is immutable once handed over.
                let (from, to) = unsafe {
                    let chunk = handle.get();
                    (chunk.from, chunk.to)
                };
                self.replace_chunk(Some(handle));
                self.port.observe(
                    Obs::new(Creator::Sched, Level::Info, Code::ChunkTaken)
                        .at(self.at)
                        .arg0(from.0)
                        .arg1(to.0),
                );
            }
            What::DropSchedule => self.replace_chunk(None),
            What::AllNotesOff => {
                self.tone.off();
                self.port.observe(
                    Obs::new(Creator::Transport, Level::Info, Code::AllNotesOff).at(self.at),
                );
            }
            What::SetTraceLevel(level) => self.trace = level,
        }
    }

    /// Install a chunk, sending any previous one home. **The real-time thread
    /// never frees** (eng-01 §4.4).
    fn replace_chunk(&mut self, next: Option<crate::command::ChunkHandle>) {
        if let Some(old) = self.chunk.take() {
            let garbage = Garbage::Chunk(old);
            if let Err(unsent) = self.port.release(garbage) {
                // The ring is full. Hold it and retry — dropping it would leak,
                // and freeing it here would allocate.
                self.stash(unsent);
            } else {
                self.port.observe(
                    Obs::new(Creator::Sched, Level::Info, Code::ChunkReleased).at(self.at),
                );
            }
        }
        self.chunk = next;
    }

    fn stash(&mut self, garbage: Garbage) {
        // Only one slot: a second unreturnable chunk while the first is still
        // stuck means the app thread has stopped collecting entirely, which is a
        // bug elsewhere. Keeping the older one loses less.
        if self.returning.is_none() {
            self.returning = Some(garbage);
        }
    }

    fn retry_return(&mut self) {
        if let Some(garbage) = self.returning.take()
            && let Err(unsent) = self.port.release(garbage)
        {
            self.returning = Some(unsent);
        }
    }

    fn report_dropped(&mut self) {
        if self.port.dropped > 0 {
            let lost = self.port.dropped;
            self.port.dropped = 0;
            // If *this* push also fails the count simply resumes, which is the
            // correct behaviour: the gap stays visible.
            self.port.observe(
                Obs::new(Creator::Stream, Level::Warn, Code::ObsDropped)
                    .at(self.at)
                    .arg0(lost),
            );
        }
    }

    fn advance_play(&self, frames: u64) -> SampleTime {
        let next = self.play + frames;
        if self.loop_on && next >= self.loop_to {
            // Wrap by the loop length rather than snapping to the start, so a
            // block boundary that straddles the loop point does not lose or
            // repeat samples.
            let length = self.loop_to - self.loop_from;
            if length > 0 {
                return SampleTime(self.loop_from.0 + (next - self.loop_to) % length);
            }
        }
        next
    }

    fn render(&mut self, block: &mut Block<'_>, frame: usize) {
        if !self.tone.is_silent() {
            // Channel 0 carries the tone; every other channel copies it.
            // `silence()` has already zeroed every plane, so the copy is a
            // choice about *this* voice — the guarantee that we never leave a
            // channel unwritten (eng-01 §11.4) is already kept.
            self.tone.render(block.out.segment(0, 0, frame));
            for channel in 1..block.out.channel() {
                block.out.copy_plane(0, channel);
            }
        }
        for (channel, slot) in self.peak.iter_mut().enumerate() {
            *slot = if channel < block.out.channel() {
                block.out.peak(channel)
            } else {
                0.0
            };
        }
    }

    fn publish(&mut self, elapsed: u32, started: Instant) {
        self.port.publish(Position {
            at: self.at,
            play: self.play,
            running: self.running,
            loop_on: self.loop_on,
            loop_from: self.loop_from,
            loop_to: self.loop_to,
            sample_rate: self.format.sample_rate,
            block: self.block,
            xrun: self.xrun,
            peak: self.peak,
            callback_us: elapsed,
            callback_worst_us: self.worst_us,
            // The correlation pair: this sample position was observed at this
            // instant. The app fits a line over a short history of these, which
            // is where observed sample-clock drift comes from (R-603, R-814).
            correlate_at: self.at,
            correlate_nanos: started.duration_since(self.origin).as_nanos() as u64,
            latency_in: self.latency.0,
            latency_out: self.latency.1,
        });
    }
}

#[cfg(test)]
mod test;
