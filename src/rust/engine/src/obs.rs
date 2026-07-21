//! What the engine says about itself.
//!
//! **No strings and no formatting on the real-time thread.** Formatting
//! allocates, and the callback may not. So the engine pushes a `code` plus three
//! integers — a few stores — and the app-side drain turns the code into prose.
//!
//! That makes the message catalogue a single table: greppable, countable, and
//! translatable later if it ever matters. It also means the prose can be written
//! for a human to read, because [someone will actually be reading
//! it](crate::obs::Obs::render) — the log is a feature, not a developer console
//! (eng-01 §9.5).
//!
//! This crate does not depend on `rev-log`: the engine owns a ring, the app
//! drains it and hands text to the log. That is what keeps bundled SQLite out of
//! the audio engine's dependency tree (eng-01 §14).

use crate::time::SampleTime;

/// How serious a record is. Mirrors `rev_log::Level` without depending on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Level {
    Trace = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

/// Which part of the engine spoke. Dotted names, matching `rev_log::creator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Creator {
    Stream,
    Transport,
    Sched,
    Timing,
}

impl Creator {
    pub fn as_str(self) -> &'static str {
        match self {
            Creator::Stream => "engine.stream",
            Creator::Transport => "engine.transport",
            Creator::Sched => "engine.sched",
            Creator::Timing => "engine.timing",
        }
    }
}

/// The message catalogue. One variant per thing the engine can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// arg0: sample position the transport started from.
    TransportStart,
    /// arg0: sample position it stopped at.
    TransportStop,
    /// arg0: destination.
    Locate,
    /// arg0: frequency in millihertz (integers only cross the ring).
    ToneOn,
    ToneOff,
    AllNotesOff,
    /// arg0: how many. The callback did not produce a block in time.
    Xrun,
    /// arg0: chunk window start, arg1: end.
    ChunkTaken,
    ChunkReleased,
    /// arg0: how many commands could not be held. The pending set is fixed size
    /// because the callback may not allocate; overflow is a real event.
    PendingFull,
    /// arg0: how many observations the ring could not hold.
    ObsDropped,
    /// arg0: block frames, arg1: worst callback microseconds so far.
    BlockTrace,
}

/// One thing the engine said. Fixed size, `Copy`, no pointers — a few stores on
/// the real-time thread.
#[derive(Debug, Clone, Copy)]
pub struct Obs {
    pub at: SampleTime,
    pub creator: Creator,
    pub level: Level,
    pub code: Code,
    pub arg: [u64; 3],
}

impl Obs {
    pub fn new(creator: Creator, level: Level, code: Code) -> Obs {
        Obs {
            at: SampleTime(0),
            creator,
            level,
            code,
            arg: [0; 3],
        }
    }

    pub fn at(mut self, at: SampleTime) -> Obs {
        self.at = at;
        self
    }

    pub fn arg0(mut self, value: u64) -> Obs {
        self.arg[0] = value;
        self
    }

    pub fn arg1(mut self, value: u64) -> Obs {
        self.arg[1] = value;
        self
    }

    /// Turn the record into prose. **App thread only** — this allocates, which
    /// is exactly why the real-time side does not do it.
    ///
    /// `sample_rate` is passed rather than stored because an `Obs` is sized to
    /// be cheap, and the drain knows the format anyway.
    pub fn render(&self, sample_rate: u32) -> String {
        let seconds = self.at.seconds(sample_rate);
        let a = self.arg[0];
        let b = self.arg[1];
        match self.code {
            Code::TransportStart => format!("transport start at sample {a}"),
            Code::TransportStop => format!("transport stop at sample {a}"),
            Code::Locate => format!("locate to sample {a}"),
            Code::ToneOn => format!("tone on: {:.3} Hz", a as f64 / 1000.0),
            Code::ToneOff => "tone off".to_string(),
            Code::AllNotesOff => "all notes off".to_string(),
            Code::Xrun => format!("xrun: {a} so far (at {seconds:.3} s)"),
            Code::ChunkTaken => format!("schedule chunk taken, samples {a}..{b}"),
            Code::ChunkReleased => "schedule chunk released".to_string(),
            Code::PendingFull => {
                format!("{a} scheduled commands dropped: the pending set is full")
            }
            Code::ObsDropped => {
                format!("{a} engine records dropped: the observation ring was full")
            }
            Code::BlockTrace => format!("block of {a} frames, worst callback {b} us"),
        }
    }
}

#[cfg(test)]
mod test;
