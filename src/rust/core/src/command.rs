//! The command vocabulary: the only way project state changes.
//!
//! One enum serves three jobs — the API callers issue, the journal payload
//! format (serde-serialized), and the seed of the R-203 interchange serializer.
//! That is deliberate: a single vocabulary is what keeps undo, persistence and
//! scripting speaking the same language (the dogfooding rule, §6g).
//!
//! Two disciplines run through it. **Ids are uniform**: every creating command
//! carries `Option<Id>`, where `None` means "allocate" and `Some` means "use
//! exactly this" — replay and redo pass `Some`, so history reproduces without
//! renumbering. **Inverses are closed under the vocabulary**: undoing a command
//! produces other commands, never a private back door, which is why replaying
//! history and undoing an edit are the same machinery. That closure is why
//! every create has a matching remove (which R-412 wants regardless).

use serde::{Deserialize, Serialize};

use crate::id::{
    EventId, MaterializedTuningInstanceId, PhraseId, PhraseInstanceId, ScaleId, TrackId, TuningId,
};
use crate::note::NoteNumber;
use crate::phrase::{
    Container, EventSpec, PhraseInstancePatch, PhraseInstanceSpec, PhrasePatch, PhraseSpec,
    TempoPoint, TrackSpec,
};
use crate::scale::ScaleSpec;
use crate::tuning::{TuningNote, TuningSpec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    /// Project settings: schema version, ppq, default tuning, root phrase.
    /// `value: None` removes the key, so the inverse is always another SetMeta.
    SetMeta {
        key: String,
        value: Option<String>,
    },

    CreateTuning {
        id: Option<TuningId>,
        tuning: TuningSpec,
    },
    RemoveTuning {
        id: Vec<TuningId>,
    },
    /// "This frequency is this note", one or many at a time — the hand-authored
    /// path, and how generated tunings land their rows.
    SetTuningNote {
        tuning_id: TuningId,
        note: Vec<TuningNote>,
    },
    RemoveTuningNote {
        tuning_id: TuningId,
        note_number: Vec<NoteNumber>,
    },
    /// Compile a definition into a frozen table. The resulting binder id is the
    /// funnel everything downstream resolves through.
    ///
    /// `ts` follows the same discipline as ids: `None` on issue (the executor
    /// stamps the clock), `Some` on replay. Anything the executor supplies must
    /// end up in the resolved payload, or history would not reproduce.
    MaterializeTuning {
        id: Option<MaterializedTuningInstanceId>,
        tuning_id: TuningId,
        ts: Option<i64>,
    },
    RemoveMaterializedTuning {
        id: Vec<MaterializedTuningInstanceId>,
    },

    CreateScale {
        id: Option<ScaleId>,
        scale: ScaleSpec,
    },
    RemoveScale {
        id: Vec<ScaleId>,
    },

    CreatePhrase {
        id: Option<PhraseId>,
        phrase: PhraseSpec,
    },
    RemovePhrase {
        id: Vec<PhraseId>,
    },
    /// The 16-ET party trick is literally this command with a tuning_id patch.
    SetPhrase {
        id: PhraseId,
        patch: PhrasePatch,
    },

    CreateTrack {
        id: Option<TrackId>,
        track: TrackSpec,
    },
    RemoveTrack {
        id: Vec<TrackId>,
    },

    AddEvent {
        container: Container,
        event: Vec<EventSpec>,
    },
    RemoveEvent {
        id: Vec<EventId>,
    },

    CreatePhraseInstance {
        id: Option<PhraseInstanceId>,
        phrase_instance: PhraseInstanceSpec,
    },
    RemovePhraseInstance {
        id: Vec<PhraseInstanceId>,
    },
    SetPhraseInstanceParam {
        id: PhraseInstanceId,
        patch: PhraseInstancePatch,
    },

    /// Replaces the phrase's whole tempo map, so the inverse is the prior map.
    SetTempo {
        phrase_id: PhraseId,
        point: Vec<TempoPoint>,
    },

    /// The capture path (R-807): recorded events land as direct events on a
    /// track. Separate from AddEvent so recording is legible in the journal and
    /// can carry capture metadata later.
    RecordBatch {
        track_id: TrackId,
        event: Vec<EventSpec>,
    },
}

impl Command {
    /// The command's name, denormalized into the journal's `command` column so
    /// history is filterable in plain SQL ("every set_tempo ever").
    pub fn name(&self) -> &'static str {
        match self {
            Command::SetMeta { .. } => "set_meta",
            Command::CreateTuning { .. } => "create_tuning",
            Command::RemoveTuning { .. } => "remove_tuning",
            Command::SetTuningNote { .. } => "set_tuning_note",
            Command::RemoveTuningNote { .. } => "remove_tuning_note",
            Command::MaterializeTuning { .. } => "materialize_tuning",
            Command::RemoveMaterializedTuning { .. } => "remove_materialized_tuning",
            Command::CreateScale { .. } => "create_scale",
            Command::RemoveScale { .. } => "remove_scale",
            Command::CreatePhrase { .. } => "create_phrase",
            Command::RemovePhrase { .. } => "remove_phrase",
            Command::SetPhrase { .. } => "set_phrase",
            Command::CreateTrack { .. } => "create_track",
            Command::RemoveTrack { .. } => "remove_track",
            Command::AddEvent { .. } => "add_event",
            Command::RemoveEvent { .. } => "remove_event",
            Command::CreatePhraseInstance { .. } => "create_phrase_instance",
            Command::RemovePhraseInstance { .. } => "remove_phrase_instance",
            Command::SetPhraseInstanceParam { .. } => "set_phrase_instance_param",
            Command::SetTempo { .. } => "set_tempo",
            Command::RecordBatch { .. } => "record_batch",
        }
    }
}
