//! The seam itself: four channels, three mechanisms.
//!
//! ```text
//!    app thread                                     RT thread
//!    ──────────                                     ─────────
//!       ├──────────── command ring (SPSC) ────────────▶  drain, act
//!       ◀─────────── position snapshot (seqlock) ──────┤  publish, every block
//!       ◀─────────── observation ring (SPSC) ──────────┤  push, drop on full
//!       ◀─────────── return ring (SPSC) ───────────────┤  hand back, never drop
//! ```
//!
//! The overflow policies are **asymmetric on purpose** (eng-01 §4): commands are
//! user intent, so the app refuses to send and reports it — silently dropping
//! "stop" is not acceptable. Observations are not intent, so they drop and
//! count — blocking the audio thread to preserve a log line inverts the whole
//! priority order.

use std::sync::Arc;

use rtrb::{Consumer, Producer, RingBuffer};

use crate::command::{Command, Garbage};
use crate::live::Live;
use crate::obs::Obs;
use crate::position::{Position, PositionCell};

/// Command ring capacity. Revisited with measurements, as eng-01 §4.1 says.
pub const COMMAND_CAPACITY: usize = 1024;
pub const OBS_CAPACITY: usize = 1024;
pub const GARBAGE_CAPACITY: usize = 256;
/// The thru ring: MIDI thread → engine, for live notes (midi-01 §3). Sized for a
/// generous burst of a fast passage between one audio callback and the next;
/// like the observation ring it drops-and-counts rather than blocking, because a
/// missed live key is a missed key, not a leak.
pub const THRU_CAPACITY: usize = 256;

/// A command that could not be sent, handed back to its sender.
///
/// The type exists so that "the ring was full" cannot be ignored the way a
/// `bool` can.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Refused(pub Command);

/// The real-time side of the seam. Owned by the engine, touched by no one else.
pub struct RtPort {
    pub(crate) command: Consumer<Command>,
    pub(crate) obs: Producer<Obs>,
    pub(crate) garbage: Producer<Garbage>,
    pub(crate) position: Arc<PositionCell>,
    /// Live notes from the MIDI thread. A different producer than `command`
    /// (which is the app thread's), so it is its own ring — rtrb is
    /// single-producer, and routing live notes through the app would cost a UI
    /// frame of latency (midi-01 §3).
    pub(crate) thru: Consumer<Live>,
    /// Observations the ring could not hold. Reported on the next successful
    /// push, so a gap in the log is itself visible.
    pub(crate) dropped: u64,
}

impl RtPort {
    /// Push an observation, or count it as lost. Never blocks, never allocates.
    pub(crate) fn observe(&mut self, obs: Obs) {
        if self.obs.push(obs).is_err() {
            self.dropped += 1;
        }
    }

    /// Send something home to be dropped. If the ring is full the value stays
    /// with the engine and is retried — **the return ring never drops**, because
    /// dropping here would leak.
    pub(crate) fn release(&mut self, garbage: Garbage) -> Result<(), Garbage> {
        self.garbage
            .push(garbage)
            .map_err(|rtrb::PushError::Full(unsent)| unsent)
    }

    pub(crate) fn publish(&self, position: Position) {
        self.position.publish(position);
    }

    /// Take the next live note, if any. Drained in `process`, the same place
    /// commands are — and public so a driver (or a test standing in for one)
    /// can prove what physics reached the engine.
    pub fn next_live(&mut self) -> Option<Live> {
        self.thru.pop().ok()
    }
}

/// The MIDI thread's end of the thru ring. Held by `rev-midi`'s input callback,
/// which resolves note→Hz and pushes physics (midi-01 §5, §7). **Drops and
/// counts** rather than blocking, so a full ring under an implausible burst
/// costs a note, never the audio thread.
pub struct ThruSender {
    thru: Producer<Live>,
    /// Live notes the ring could not hold — surfaced via eng-08.
    dropped: u64,
}

impl ThruSender {
    /// Push a live note, or count it as lost. Never blocks, never allocates.
    pub fn send(&mut self, live: Live) {
        if self.thru.push(live).is_err() {
            self.dropped += 1;
        }
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

/// The app side of the seam. One per engine session — never a global, so that
/// N > 1 is a policy change rather than a rewrite (ui-01 §4, invariant 6).
pub struct EngineSession {
    command: Producer<Command>,
    obs: Consumer<Obs>,
    garbage: Consumer<Garbage>,
    position: Arc<PositionCell>,
}

impl EngineSession {
    /// Send a command. **Refuses rather than drops**: commands are intent.
    pub fn send(&mut self, command: Command) -> Result<(), Refused> {
        self.command
            .push(command)
            .map_err(|rtrb::PushError::Full(refused)| Refused(refused))
    }

    /// Where the transport is, right now.
    pub fn position(&self) -> Position {
        self.position.read()
    }

    /// Drain what the engine has said. Call every UI frame; formatting happens
    /// here, on this thread, where allocating is allowed.
    pub fn drain_obs(&mut self, mut sink: impl FnMut(Obs)) {
        while let Ok(obs) = self.obs.pop() {
            sink(obs);
        }
    }

    /// Drop what the engine has finished with. Call every UI frame — this is
    /// the only place engine-side allocations are freed.
    pub fn collect(&mut self) {
        while let Ok(garbage) = self.garbage.pop() {
            match garbage {
                // SAFETY: the handle has come home over the return ring, so the
                // engine no longer holds it, and each handle is returned once.
                Garbage::Chunk(handle) => unsafe { handle.release() },
            }
        }
    }
}

impl Drop for EngineSession {
    fn drop(&mut self) {
        // Anything still in flight is ours to free. Without this, tearing down a
        // session would leak every chunk the engine had not yet returned.
        self.collect();
    }
}

/// Build both ends of one session's seam, plus the thru sender for live input.
pub fn session_with_thru() -> (EngineSession, RtPort, ThruSender) {
    let (command_tx, command_rx) = RingBuffer::new(COMMAND_CAPACITY);
    let (obs_tx, obs_rx) = RingBuffer::new(OBS_CAPACITY);
    let (garbage_tx, garbage_rx) = RingBuffer::new(GARBAGE_CAPACITY);
    let (thru_tx, thru_rx) = RingBuffer::new(THRU_CAPACITY);
    let position = Arc::new(PositionCell::new());

    (
        EngineSession {
            command: command_tx,
            obs: obs_rx,
            garbage: garbage_rx,
            position: Arc::clone(&position),
        },
        RtPort {
            command: command_rx,
            obs: obs_tx,
            garbage: garbage_tx,
            position,
            thru: thru_rx,
            dropped: 0,
        },
        ThruSender {
            thru: thru_tx,
            dropped: 0,
        },
    )
}

/// Build a session with no live input — the common case for playback-only.
///
/// Keeps the two-value signature every existing caller uses; the thru ring
/// still exists (the engine always drains it) but its producer is dropped, so
/// the ring is simply always empty.
pub fn session() -> (EngineSession, RtPort) {
    let (app, rt, _thru) = session_with_thru();
    (app, rt)
}

#[cfg(test)]
mod test;
