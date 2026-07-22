//! What a MIDI message becomes on each side of the fork (midi-01 §7).
//!
//! The two rings carry deliberately different shapes, because they serve
//! different masters. The thru ring carries **frequencies for the voice**
//! (physics, R-312); the event ring carries **note numbers for the model**
//! (music, R-002). The fork is where music and physics part on the input side —
//! the mirror of the compiler on the output side.

use rev_core::NoteNumber;

/// A parsed MIDI channel-voice message — the subset midi-01 handles. Raw bytes
/// become this at the callback; CC, pitch-bend and the rest arrive in midi-02+.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Message {
    NoteOn {
        channel: u8,
        note: NoteNumber,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: NoteNumber,
    },
}

impl Message {
    /// Parse a status+data MIDI message. `None` for anything midi-01 does not
    /// handle yet, and for a malformed one — a short buffer is not a crash.
    ///
    /// A note-on with velocity zero is a note-off; every keyboard uses this, and
    /// treating it otherwise strands notes (the classic stuck-note bug).
    pub fn parse(bytes: &[u8]) -> Option<Message> {
        let &[status, data1, data2, ..] = bytes else {
            return None;
        };
        let channel = status & 0x0F;
        let note = NoteNumber(i32::from(data1 & 0x7F));
        match status & 0xF0 {
            0x90 if data2 & 0x7F > 0 => Some(Message::NoteOn {
                channel,
                note,
                velocity: data2 & 0x7F,
            }),
            0x90 | 0x80 => Some(Message::NoteOff { channel, note }),
            _ => None,
        }
    }

    /// The opaque pairing key for this note: `(channel, note)`, which the engine
    /// treats as a handle and never as a pitch (R-312).
    pub fn key(&self) -> rev_engine::LiveKey {
        let (channel, note) = match self {
            Message::NoteOn { channel, note, .. } | Message::NoteOff { channel, note } => {
                (*channel, *note)
            }
        };
        rev_engine::LiveKey((u16::from(channel) << 8) | (note.get() as u16 & 0x7F))
    }
}

/// One event on the **event ring**: the model's view, stamped for capture.
///
/// A note number (R-002), a velocity, a channel, and the sample-position stamp
/// from the correlation (§4) — so recording lands a note where it was played
/// (R-810) and the roll can show live input later. Note numbers, not
/// frequencies: this side feeds the model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Captured {
    pub message: Message,
    /// Monotonic instant at the driver boundary, in the engine's clock domain.
    /// Mapped to a sample position through the correlation when it is recorded.
    pub nanos: u64,
}

/// Velocity 0..=127 to the engine's 0..=1 level. Square-ish would be more
/// musical; linear is honest for now and the curve is a play-time concern.
pub fn level_of(velocity: u8) -> f32 {
    f32::from(velocity & 0x7F) / 127.0
}
