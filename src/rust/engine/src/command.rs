//! What the app tells the engine.
//!
//! Three properties make this extensible, and all three are deliberate
//! (eng-01 §7):
//!
//! - **Every command carries a time.** A button press sends [`NOW`]; a compiled
//!   event or an arpeggiator carries a sample stamp. Live immediacy and future
//!   scheduling are the same channel with a different number in one field.
//! - **Frequencies, never note numbers.** Below this seam there is only physics
//!   (R-312), so a voice *cannot* assume 12-ET — tuning-awareness is structural
//!   rather than a discipline anyone could forget.
//! - **Commands are values, not closures.** A `Box<dyn FnOnce>` would be an
//!   allocation the real-time thread must free.

use crate::time::{NOW, SampleTime};

/// One instruction, and when to obey it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Command {
    pub at: SampleTime,
    pub what: What,
}

impl Command {
    /// The live path's constructor: do this at the next block boundary.
    pub fn now(what: What) -> Command {
        Command { at: NOW, what }
    }

    pub fn at(at: SampleTime, what: What) -> Command {
        Command { at, what }
    }
}

/// A closed enum, so adding a message is a compile error everywhere it must be
/// handled — the same argument that made widgets data in ui-01 §8.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum What {
    // --- Transport
    Start,
    Stop,
    Locate(SampleTime),
    SetLoop {
        from: SampleTime,
        to: SampleTime,
        on: bool,
    },

    // --- The test tone: audible sound with no graph, no automation, no
    // compiler (eng-01 §15). Superseded, not extended, when voices arrive.
    ToneOn {
        hz: f32,
        gain: f32,
    },
    ToneOff,

    // --- Schedule delivery. Envelope only; the payload is eng-06's (§8).
    TakeChunk(ChunkHandle),
    DropSchedule,

    /// Unconditional silence, always available.
    AllNotesOff,

    /// Trace is a firehose — per-block records at ~100 blocks a second bury
    /// everything — so it is switched on deliberately and off by default.
    SetTraceLevel(crate::obs::Level),
}

/// A compiled schedule chunk.
///
/// **The envelope is settled; the payload is eng-06's.** That deferral is
/// deliberate: the payload is the compiler's contract with `v_realized`, and
/// specifying it before `core-03` finishes would mean honouring a guess long
/// after it stopped being right (eng-01 §8).
///
/// What binds from today is the *ownership discipline*: allocated app-side,
/// immutable once handed over, and returned over the garbage ring — the real-time
/// thread never frees it.
#[derive(Debug)]
pub struct Chunk {
    /// The window this chunk covers. Events outside it are not its business.
    pub from: SampleTime,
    pub to: SampleTime,
    // eng-06: the sample-stamped events. Deliberately absent.
}

/// An owning pointer to a [`Chunk`], sized to cross a ring.
///
/// Raw rather than `Box` because it must be `Copy` to live in a POD command, and
/// because the point is that ownership moves *without* the receiver being able
/// to drop it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkHandle(*mut Chunk);

// SAFETY: a handle is only ever held by one side at a time — the app until it
// sends `TakeChunk`, the engine until it returns the handle over the garbage
// ring. The rings are SPSC and enforce the hand-off; nothing aliases.
unsafe impl Send for ChunkHandle {}

impl ChunkHandle {
    /// Allocate app-side. The only place a chunk is created.
    pub fn new(chunk: Chunk) -> ChunkHandle {
        ChunkHandle(Box::into_raw(Box::new(chunk)))
    }

    /// Read the chunk. Real-time safe: no allocation, no free.
    ///
    /// # Safety
    /// The handle must be one this side currently owns.
    pub unsafe fn get(&self) -> &Chunk {
        // SAFETY: the caller owns the handle, so the pointee is live and
        // unaliased; a chunk is immutable once handed over.
        unsafe { &*self.0 }
    }

    /// Drop it — **app thread only**. The real-time thread returns handles over
    /// the garbage ring and never calls this.
    ///
    /// # Safety
    /// Must be called exactly once, on the app thread, for a handle that has
    /// come home.
    pub unsafe fn release(self) {
        // SAFETY: the caller guarantees sole ownership and a single call.
        drop(unsafe { Box::from_raw(self.0) });
    }
}

/// Things the real-time thread is finished with, going home to be dropped.
///
/// The single mechanism that makes "no allocation on the real-time thread"
/// survive contact with a real feature set: without it, every future subsystem
/// re-invents it badly (eng-01 §4.4).
#[derive(Debug, Clone, Copy)]
pub enum Garbage {
    Chunk(ChunkHandle),
}

#[cfg(test)]
mod test;
