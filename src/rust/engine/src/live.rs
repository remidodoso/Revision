//! Live notes — a keyboard played *now*, off the schedule (midi-01 §6).
//!
//! This is the input side of R-312's boundary. A note arrives from a MIDI
//! keyboard as a note number, but that number is resolved to a **frequency**
//! above the engine (`rev-midi`'s note→Hz snapshot), so what crosses the thru
//! ring is physics: a frequency, a level, and an **opaque handle** for pairing
//! the eventual note-off — exactly the vocabulary R-312 permits.
//!
//! The engine never interprets the handle. It is `(channel, note)` at the
//! source, but here it is only an identity: "the off that matches this on."

/// Opaque identity for pairing a note-off to its note-on.
///
/// `(channel << 8 | note)` at the source, but the engine treats it as a handle
/// and never as a pitch — which is what keeps live input inside R-312.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveKey(pub u16);

/// One live event, resolved to physics, crossing the thru ring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Live {
    /// Begin a held voice at this frequency and level.
    NoteOn { hz: f32, level: f32, key: LiveKey },
    /// Release the held voice this key started.
    NoteOff { key: LiveKey },
}
