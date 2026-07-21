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

/// One compiled note. Self-contained: everything the engine needs, nothing it
/// must look up (eng-06 §5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    /// When it starts, in **play-position** samples — not session-clock samples,
    /// so a loop needs no recompilation (eng-06 §6.3).
    pub at: SampleTime,
    /// How long it sounds, in samples. **Bounded, always** (R-402a): there is no
    /// sentinel for "forever", because a continuously sounding source is
    /// instrument state, not a note. At 48 kHz a `u32` reaches about 24 hours.
    ///
    /// Carried with the note rather than sent as a separate note-off. That is a
    /// modelling claim, not an optimization — the model already says a note has a
    /// duration, and paired on/off is a wire encoding. It also means a chunk
    /// boundary, a loop wrap, or a superseded schedule cannot orphan anything,
    /// because nothing was waiting.
    ///
    /// It is articulation *input*, not a hard gate: a pedal or a long release may
    /// sound past it.
    pub dur: u32,
    /// Frequency, resolved through the tuning at compile time (R-312). The engine
    /// cannot mis-tune what it was never told.
    pub hz: f32,
    /// 0..1, from the model's 16-bit velocity (R-402).
    pub level: f32,
    /// Which track it came from — a routing key, opaque to the engine.
    pub voice: u16,
    /// Reserved, and named rather than anonymous so that filling it later does
    /// not change the struct's size.
    pub reserved: u16,
}

/// A compiled schedule chunk.
///
/// The *envelope* was settled at eng-01 §8 and the payload deferred to eng-06,
/// on the grounds that it is the compiler's contract with `v_realized` and that
/// specifying it before `core-03` existed would mean honouring a guess. Both
/// halves are now here, and the ownership discipline never changed: allocated
/// app-side, immutable once handed over, returned over the garbage ring — the
/// real-time thread never frees it.
#[derive(Debug)]
pub struct Chunk {
    /// The window this chunk covers, in play-position samples.
    pub from: SampleTime,
    pub to: SampleTime,
    /// **Ascending by `at`.** The engine dispatches by scanning forward, so
    /// sorted order is a precondition rather than a convenience.
    ///
    /// A note may extend past `to`: durations are not truncated by a window
    /// (R-405a), and carrying the duration is what makes that expressible.
    pub note: Vec<Note>,
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
