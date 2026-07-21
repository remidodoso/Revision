//! Material: phrases, events, tracks, phrase instances, tempo points.
//!
//! A **phrase** is the unit of material (R-401) — a named container of events
//! whose length is a *window* over potentially longer content: events at or
//! beyond the window are retained but do not sound, and the window length is
//! also the loop stride. A **phrase instance** places a phrase in time with its
//! own non-destructive play parameters (R-405), including where in the material
//! the window starts. Editing a phrase affects every instance of it (R-404).

use serde::{Deserialize, Serialize};

use crate::id::{EventId, PhraseId, PhraseInstanceId, ScaleId, TrackId, TuningId};
use crate::note::NoteNumber;
use crate::tick::Tick;

/// Where an event lives: inside a phrase, or directly on a track (R-406).
/// Exactly one, enforced in the schema by a CHECK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Container {
    Phrase(PhraseId),
    Track(TrackId),
}

/// Where an instance lives: on a track, or nested inside a parent phrase
/// (R-407). Exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceContainer {
    Track(TrackId),
    ParentPhrase(PhraseId),
}

/// The event's type. Open by design (R-402): notes today, continuous
/// controllers, articulation and audio later, without a schema change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventKind(pub String);

impl EventKind {
    pub fn note() -> EventKind {
        EventKind("note".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EventKind {
    fn default() -> Self {
        EventKind::note()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhraseSpec {
    pub name: String,
    /// The window (R-401): gates onsets and sets the loop stride.
    pub length_tick: Tick,
    pub tuning_id: Option<TuningId>,
    /// `None` is chromatic — there is no chromatic scale row (R-510).
    pub scale_id: Option<ScaleId>,
    /// The pitch class the scale mask is rooted on. Root semantics overall are
    /// still open (R-517); this is the pragmatic per-phrase binding.
    pub root: i32,
    pub origin: Option<String>,
    pub seed: Option<serde_json::Value>,
    pub parent_phrase_id: Option<PhraseId>,
    pub extra: serde_json::Value,
}

impl PhraseSpec {
    pub fn new(name: impl Into<String>, length_tick: Tick) -> PhraseSpec {
        PhraseSpec {
            name: name.into(),
            length_tick,
            tuning_id: None,
            scale_id: None,
            root: 0,
            origin: None,
            seed: None,
            parent_phrase_id: None,
            extra: serde_json::json!({}),
        }
    }
}

/// A change to a nullable field: leave it, clear it, or set it.
///
/// Three states, not `Option<Option<T>>`: an inverse has to be able to restore
/// a field to NULL, and "leave alone" must stay distinguishable from "set to
/// null" after a round trip through the journal — which the nested-Option
/// encoding does not survive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change<T> {
    #[default]
    Leave,
    Clear,
    Set(T),
}

impl<T> Change<T> {
    /// Does this change touch the field at all?
    pub fn touches(&self) -> bool {
        !matches!(self, Change::Leave)
    }

    /// The value to write, when the field is touched.
    pub fn value(&self) -> Option<&T> {
        match self {
            Change::Set(value) => Some(value),
            _ => None,
        }
    }

    /// The change that restores `prior` — used to build inverses.
    pub fn restoring(prior: Option<T>) -> Change<T> {
        match prior {
            Some(value) => Change::Set(value),
            None => Change::Clear,
        }
    }
}

/// A partial update. `None`/`Change::Leave` means "leave alone".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PhrasePatch {
    pub name: Option<String>,
    pub length_tick: Option<Tick>,
    pub tuning_id: Change<TuningId>,
    pub scale_id: Change<ScaleId>,
    pub root: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Phrase {
    pub id: PhraseId,
    pub spec: PhraseSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventSpec {
    /// `None` on issue, `Some` on replay/redo — the uniform id discipline.
    pub id: Option<EventId>,
    pub kind: EventKind,
    pub at_tick: Tick,
    pub dur_tick: Tick,
    pub note_number: Option<NoteNumber>,
    /// 0..65535: the MIDI 2.0 domain (R-402). Seven-bit values are translated
    /// at the MIDI boundary; the UI presents 0..127 by default.
    pub velocity: Option<i32>,
    pub extra: serde_json::Value,
}

impl EventSpec {
    /// A plain note event — the overwhelmingly common case.
    pub fn note(at_tick: Tick, dur_tick: Tick, note_number: i32, velocity: i32) -> EventSpec {
        EventSpec {
            id: None,
            kind: EventKind::note(),
            at_tick,
            dur_tick,
            note_number: Some(NoteNumber(note_number)),
            velocity: Some(velocity),
            extra: serde_json::json!({}),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub container: Container,
    pub kind: EventKind,
    pub at_tick: Tick,
    pub dur_tick: Tick,
    pub note_number: Option<NoteNumber>,
    pub velocity: Option<i32>,
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackSpec {
    /// The root phrase this track belongs to. Multi-track sub-phrases are
    /// deliberately deferred, not foreclosed.
    pub phrase_id: PhraseId,
    pub name: String,
    pub ord: i32,
    pub extra: serde_json::Value,
}

impl TrackSpec {
    pub fn new(phrase_id: PhraseId, name: impl Into<String>, ord: i32) -> TrackSpec {
        TrackSpec {
            phrase_id,
            name: name.into(),
            ord,
            extra: serde_json::json!({}),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub spec: TrackSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhraseInstanceSpec {
    /// The material being referenced.
    pub phrase_id: PhraseId,
    pub container: InstanceContainer,
    pub at_tick: Tick,
    /// Where the window starts *within the material* — play bars 8 to 16 of 32.
    pub offset_tick: Tick,
    /// The instance's own length, independent of the phrase's (R-405).
    /// `None` means the natural extent of `loop_count` iterations.
    pub length_tick: Option<Tick>,
    pub loop_count: i32,
    /// Chromatic transpose, in note numbers, read in the material's tuning
    /// (R-423). Degree transposition is a separate, scale-relative verb.
    pub transpose: i32,
    pub mute: bool,
    pub extra: serde_json::Value,
}

impl PhraseInstanceSpec {
    pub fn new(phrase_id: PhraseId, container: InstanceContainer, at_tick: Tick) -> Self {
        PhraseInstanceSpec {
            phrase_id,
            container,
            at_tick,
            offset_tick: Tick::ZERO,
            length_tick: None,
            loop_count: 1,
            transpose: 0,
            mute: false,
            extra: serde_json::json!({}),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PhraseInstancePatch {
    pub at_tick: Option<Tick>,
    pub offset_tick: Option<Tick>,
    /// Nullable: clearing it restores the natural extent of the loop.
    pub length_tick: Change<Tick>,
    pub loop_count: Option<i32>,
    pub transpose: Option<i32>,
    pub mute: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhraseInstance {
    pub id: PhraseInstanceId,
    pub spec: PhraseInstanceSpec,
}

/// One point in a phrase's tempo map. Microseconds per quarter is MIDI-exact
/// and integral, so the model never accumulates float drift; seconds are
/// derived at the engine boundary only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoPoint {
    pub at_tick: Tick,
    pub usec_per_quarter: i64,
}

/// One row of the realization view: an event as it will actually sound, with
/// instance placement, transposition and windowing already applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealizedEvent {
    pub track_id: TrackId,
    pub kind: EventKind,
    pub at_tick: Tick,
    pub dur_tick: Tick,
    pub note_number: Option<NoteNumber>,
    pub velocity: Option<i32>,
    pub tuning_id: Option<TuningId>,
    /// `None` for a direct event on the track; `Some` when it came through an
    /// instance.
    pub phrase_instance_id: Option<PhraseInstanceId>,
}
