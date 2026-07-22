//! The input fork: one `midir` port, two rings (midi-01 §3).
//!
//! At the callback, each message is **re-stamped** in the engine's monotonic
//! domain (§4 — `midir`'s own timestamp is a different clock on every platform),
//! resolved through the note→Hz snapshot (§5), and pushed to both rings:
//!
//! - the **thru ring** → engine, carrying frequencies for the voice (the
//!   `ThruSender` from `rev-engine`);
//! - the **event ring** → app, carrying note numbers for the model, stamped for
//!   capture.
//!
//! **The callback allocates nothing and never blocks.** It reads the snapshot
//! (one array index), pushes two rings (each drop-and-count), and returns. The
//! device thread must be as disciplined as the audio thread, for the same
//! reason: a stall here is a stuck note or a dropped take.

use std::sync::Arc;
use std::time::Instant;

use rev_engine::{Live, ThruSender};

use crate::event::{Captured, Message, level_of};
use crate::snapshot::NoteHz;

/// The app's end of the event ring — drained each UI frame for capture and
/// display. Note numbers, not frequencies (R-002).
pub struct Events {
    rx: rtrb::Consumer<Captured>,
    /// Events the ring could not hold. A dropped capture event is caught by the
    /// journal's durability, not by never dropping (midi-01 §3).
    dropped: Arc<std::sync::atomic::AtomicU64>,
}

impl Events {
    /// Take the next captured event, if any. Named `take` rather than `next` so
    /// it is not mistaken for an iterator's — draining is a poll, not iteration.
    pub fn take(&mut self) -> Option<Captured> {
        self.rx.pop().ok()
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Everything the callback needs, moved into it: where physics goes, where the
/// model's events go, the pitch resolution, and the clock origin.
pub struct Fork {
    thru: ThruSender,
    events: rtrb::Producer<Captured>,
    dropped: Arc<std::sync::atomic::AtomicU64>,
    snapshot: NoteHz,
    origin: Instant,
}

impl Fork {
    /// Build the fork and the app's event reader. `origin` must be the **same**
    /// monotonic origin the engine measures its correlation pairs from, so both
    /// live in one clock domain (§4); the app wires them from one shared value.
    pub fn new(thru: ThruSender, snapshot: NoteHz, origin: Instant) -> (Fork, Events) {
        // Room for a burst of a fast passage between UI frames.
        let (tx, rx) = rtrb::RingBuffer::new(1024);
        let dropped = Arc::new(std::sync::atomic::AtomicU64::new(0));
        (
            Fork {
                thru,
                events: tx,
                dropped: Arc::clone(&dropped),
                snapshot,
                origin,
            },
            Events { rx, dropped },
        )
    }

    /// Handle one raw MIDI message — the body of the `midir` callback.
    ///
    /// Kept separate from any `midir` type so it is testable without a device:
    /// feed it bytes, watch both rings.
    pub fn on_message(&mut self, bytes: &[u8]) {
        let Some(message) = Message::parse(bytes) else {
            return;
        };
        let nanos = self.origin.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;

        // --- physics, to the engine (R-312). A key that resolves to nothing is
        // simply not played; a note-off always goes, so nothing is left held.
        match message {
            Message::NoteOn { note, velocity, .. } => {
                if let Some(hz) = self.snapshot.resolve(note.get() as u8) {
                    self.thru.send(Live::NoteOn {
                        hz,
                        level: level_of(velocity),
                        key: message.key(),
                    });
                }
            }
            Message::NoteOff { .. } => {
                self.thru.send(Live::NoteOff { key: message.key() });
            }
        }

        // --- the model's view, to the app (R-002), stamped for capture.
        if self.events.push(Captured { message, nanos }).is_err() {
            self.dropped
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Replace the resolution — a tuning change, or (midi-04) a scale remap.
    /// The whole feature is here: swap the snapshot, and the next key resolves
    /// differently.
    pub fn set_snapshot(&mut self, snapshot: NoteHz) {
        self.snapshot = snapshot;
    }
}

#[cfg(test)]
mod test;
