//! The capture path: live MIDI becoming journaled notes (rec-01).
//!
//! This is the app-side half of recording. The engine plays the keyboard live
//! (the thru fast path, midi-01/03); the [`Recorder`] listens to the *other*
//! branch of the fork — the [`Captured`] event stream, note numbers for the
//! model (R-002) — and turns it into direct events on a track (R-807).
//!
//! **The whole design serves one promise:** a `kill -9` mid-take loses nothing
//! committed (R-808). That is not free — it decides *when* capture journals. A
//! take that buffered its notes and committed at Stop would lose the entire take
//! to a crash, so capture journals **incrementally**: every note that completed
//! during a UI frame is written as one `RecordBatch` gesture at the end of that
//! frame ([`Recorder::flush`]). A committed note is durable the instant its
//! gesture commits; a kill loses only notes still physically held (no note-off,
//! so no known duration) — which were never finished (rec-01 §5).
//!
//! **Placement** rides the plumbing midi-01 built and promised to exercise here:
//! a captured event's driver-boundary instant maps to a sample position through
//! the [`Correlation`] (the engine publishes the pairs every block), then to a
//! tick through the [`TempoMap`]. Note-ons and note-offs are re-paired at this
//! edge into notes with a duration — the mirror of the compiler splitting a
//! note into edges on the way out (R-402a).

use rev_core::phrase::EventSpec;
use rev_core::tick::Tick;
use rev_core::{Command, TrackId};
use rev_engine::{Position, SampleTime};
use rev_midi::{Captured, Correlation, Message};
use rev_sched::TempoMap;
use rev_store::{Project, StoreError, query};

/// Pairs of `(sample, nanos)` history the correlation fits over. A few dozen at
/// the k-rate spans a fraction of a second — long enough to average jitter,
/// short enough to follow drift (midi-01 §4).
const WINDOW: usize = 64;

/// How a take treats material already on the track (rec-01 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Add to what is already there; nothing is removed.
    Overdub,
    /// Clear the track first, then record. v0 clears the **whole** track;
    /// region/punch replace waits for punch to exist (rec-01 §4.3).
    Replace,
}

/// A note whose onset was captured but whose note-off has not arrived. Held
/// notes carry no duration yet, so a crash before the off loses them — and they
/// were never finished, which is the correct behaviour (rec-01 §5).
#[derive(Debug, Clone, Copy)]
struct Held {
    /// The `(channel, note)` pairing key — the same handle the live path uses.
    key: u16,
    start: Tick,
    note_number: i32,
    /// Already translated to the model's 16-bit velocity domain (R-402).
    velocity: i32,
}

/// Captures live MIDI into journaled notes on one track.
///
/// Its inputs each frame are the engine [`Position`] (for the clock correlation)
/// and the drained [`Captured`] stream; its output is `RecordBatch` gestures on
/// a [`Project`]. The placement and pairing are pure and testable without a
/// device or a store; only [`flush`](Recorder::flush) touches the database.
pub struct Recorder {
    track: TrackId,
    tempo: TempoMap,
    correlation: Correlation,
    /// Session-clock minus play-position — constant while the transport runs
    /// forward, and the term that turns a correlated *session* sample into the
    /// *play* sample the tempo map wants (rec-01 §6). `None` until a running
    /// position is seen.
    offset: Option<u64>,
    armed: bool,
    mode: Mode,
    /// Replace clears exactly once per take, at the first flush.
    cleared: bool,
    held: Vec<Held>,
    /// Notes completed this frame, awaiting the frame's `RecordBatch`.
    staged: Vec<EventSpec>,
}

impl Recorder {
    /// A recorder for one track, placing notes through `tempo`. Disarmed until
    /// [`arm`](Recorder::arm); build one `tempo` the same way the compiler does
    /// — from the arrangement's tempo points at the device sample rate.
    pub fn new(track: TrackId, tempo: TempoMap) -> Recorder {
        Recorder {
            track,
            tempo,
            correlation: Correlation::new(WINDOW),
            offset: None,
            armed: false,
            mode: Mode::Overdub,
            cleared: false,
            held: Vec::new(),
            staged: Vec::new(),
        }
    }

    /// Arm the track in a mode. The record light ui-04 will drive reflects this
    /// state; here it simply gates capture. Re-arming starts a fresh take, so
    /// Replace will clear again on the next flush.
    pub fn arm(&mut self, mode: Mode) {
        self.armed = true;
        self.mode = mode;
        self.cleared = false;
    }

    /// Disarm. Any note still physically held is dropped — it never finished —
    /// and the count is returned so the caller can say so honestly (rec-01 §5).
    /// Notes already staged remain, to be flushed.
    pub fn disarm(&mut self) -> usize {
        self.armed = false;
        let dropped = self.held.len();
        self.held.clear();
        dropped
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Feed one frame's engine position. While the transport runs this advances
    /// the clock correlation and refreshes the session→play offset; stopped, it
    /// does nothing, because a note played while stopped has no place to land.
    pub fn observe(&mut self, position: &Position) {
        if !position.running {
            return;
        }
        self.correlation.observe(rev_midi::Pair {
            sample: position.correlate_at.0,
            nanos: position.correlate_nanos,
        });
        // Both clocks advance 1:1 while running forward, so the difference set at
        // the last Start/Locate is constant (rec-01 §6).
        self.offset = Some(position.at.0.saturating_sub(position.play.0));
    }

    /// Take one captured event. Ignored unless armed; an event that cannot yet
    /// be placed (no correlation, or the transport not yet observed running) is
    /// dropped rather than misplaced.
    pub fn capture(&mut self, captured: Captured) {
        if !self.armed {
            return;
        }
        let key = captured.message.key().0;
        match captured.message {
            Message::NoteOn { note, velocity, .. } => {
                let Some(start) = self.tick_at(captured.nanos) else {
                    return;
                };
                self.held.push(Held {
                    key,
                    start,
                    note_number: note.get(),
                    velocity: velocity16(velocity),
                });
            }
            Message::NoteOff { .. } => {
                // Most recent matching onset — a repeated key nests correctly.
                let Some(index) = self.held.iter().rposition(|h| h.key == key) else {
                    return; // an off with no on: ignored, as the live path ignores it
                };
                let Some(off) = self.tick_at(captured.nanos) else {
                    return;
                };
                let held = self.held.remove(index);
                // A note is at least one tick wide; a zero-length note cannot
                // exist, and clamping is cheaper than forbidding it upstream.
                let dur = Tick((off.get() - held.start.get()).max(1));
                self.staged.push(EventSpec::note(
                    held.start,
                    dur,
                    held.note_number,
                    held.velocity,
                ));
            }
        }
    }

    /// The tick a driver-boundary instant maps to: instant → sample (the
    /// correlation) → play sample (minus the offset) → tick (the tempo map).
    /// `None` until both the correlation and the offset exist.
    fn tick_at(&self, nanos: u64) -> Option<Tick> {
        let sample = self.correlation.sample_at(nanos)?;
        let offset = self.offset? as f64;
        let play = (sample - offset).max(0.0);
        Some(self.tempo.tick_at(SampleTime(play as u64)))
    }

    /// The notes completed but not yet journaled — for tests and, later, the
    /// live roll (Tier A/B), which can read a take before it is flushed.
    pub fn staged(&self) -> &[EventSpec] {
        &self.staged
    }

    /// Journal this frame's take: the Replace clear (once), then this frame's
    /// completed notes as one `RecordBatch` gesture. Each is its own gesture, so
    /// a kill loses at most this frame's not-yet-committed notes (rec-01 §5).
    /// Returns how many notes were journaled.
    pub fn flush(&mut self, project: &mut Project) -> Result<usize, StoreError> {
        if self.armed && self.mode == Mode::Replace && !self.cleared {
            let existing = query::event_on_track(project.reader(), self.track)?;
            if !existing.is_empty() {
                let id = existing.iter().map(|e| e.id).collect();
                project.apply(Command::RemoveEvent { id })?;
            }
            // Set even when the track was empty, so we do not re-query every frame.
            self.cleared = true;
        }

        if self.staged.is_empty() {
            return Ok(0);
        }
        let event = std::mem::take(&mut self.staged);
        let count = event.len();
        project.apply(Command::RecordBatch {
            track_id: self.track,
            event,
        })?;
        Ok(count)
    }
}

/// A 7-bit MIDI velocity to the model's 16-bit domain (R-402): the translation
/// the requirement says happens "at the MIDI boundary". Full scale maps to full
/// scale (127 → 65535, 0 → 0), so a fortissimo recorded is a fortissimo stored.
fn velocity16(velocity: u8) -> i32 {
    i32::from(velocity & 0x7F) * 0xFFFF / 0x7F
}

#[cfg(test)]
mod test;
