//! Execution for material: phrases, tracks, events, instances, tempo maps.

use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use rev_core::Command;
use rev_core::id::{EventId, PhraseId, PhraseInstanceId, TrackId};
use rev_core::phrase::{
    Change, Container, EventSpec, InstanceContainer, PhraseInstancePatch, PhraseInstanceSpec,
    PhrasePatch, PhraseSpec, TempoPoint, TrackSpec,
};

use super::{json_text, missing, optional_json_text};
use crate::error::StoreError;
use crate::query;

type Outcome = Result<(Command, Vec<Command>), StoreError>;

/// Removed events grouped by the container they came from, keyed by the
/// (phrase_id, track_id) pair so the ordering is stable.
type EventByContainer = BTreeMap<(Option<i64>, Option<i64>), (Container, Vec<EventSpec>)>;

pub(super) fn create_phrase(conn: &Connection, id: Option<PhraseId>, spec: PhraseSpec) -> Outcome {
    conn.execute(
        "INSERT INTO phrase (id, name, length_tick, tuning_id, scale_id, root, origin, seed, \
         parent_phrase_id, extra) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id.map(|i| i.get()),
            spec.name,
            spec.length_tick.get(),
            spec.tuning_id.map(|i| i.get()),
            spec.scale_id.map(|i| i.get()),
            spec.root,
            spec.origin,
            optional_json_text(&spec.seed)?,
            spec.parent_phrase_id.map(|i| i.get()),
            json_text(&spec.extra)?,
        ],
    )?;
    let new_id = PhraseId(conn.last_insert_rowid());
    Ok((
        Command::CreatePhrase {
            id: Some(new_id),
            phrase: spec,
        },
        vec![Command::RemovePhrase { id: vec![new_id] }],
    ))
}

/// Does not cascade: events, tracks and instances belonging to the phrase must
/// be removed first, and a foreign key refuses otherwise. Unreferenced phrases
/// persist until explicitly deleted (R-412), so silent cascades would be wrong.
pub(super) fn remove_phrase(conn: &Connection, id: Vec<PhraseId>) -> Outcome {
    let mut inverse = Vec::new();
    for &phrase_id in &id {
        let existing =
            query::phrase(conn, phrase_id)?.ok_or_else(|| missing("phrase", phrase_id.get()))?;
        conn.execute("DELETE FROM phrase WHERE id = ?1", params![phrase_id.get()])?;
        inverse.push(Command::CreatePhrase {
            id: Some(phrase_id),
            phrase: existing.spec,
        });
    }
    Ok((Command::RemovePhrase { id }, inverse))
}

pub(super) fn set_phrase(conn: &Connection, id: PhraseId, patch: PhrasePatch) -> Outcome {
    let existing = query::phrase(conn, id)?.ok_or_else(|| missing("phrase", id.get()))?;
    // The inverse mirrors the patch's shape: only the fields this patch touches
    // are restored, each to what it was.
    let prior = PhrasePatch {
        name: patch.name.as_ref().map(|_| existing.spec.name.clone()),
        length_tick: patch.length_tick.map(|_| existing.spec.length_tick),
        tuning_id: if patch.tuning_id.touches() {
            Change::restoring(existing.spec.tuning_id)
        } else {
            Change::Leave
        },
        scale_id: if patch.scale_id.touches() {
            Change::restoring(existing.spec.scale_id)
        } else {
            Change::Leave
        },
        root: patch.root.map(|_| existing.spec.root),
    };

    if let Some(name) = &patch.name {
        conn.execute(
            "UPDATE phrase SET name = ?2 WHERE id = ?1",
            params![id.get(), name],
        )?;
    }
    if let Some(length) = patch.length_tick {
        conn.execute(
            "UPDATE phrase SET length_tick = ?2 WHERE id = ?1",
            params![id.get(), length.get()],
        )?;
    }
    if patch.tuning_id.touches() {
        conn.execute(
            "UPDATE phrase SET tuning_id = ?2 WHERE id = ?1",
            params![id.get(), patch.tuning_id.value().map(|i| i.get())],
        )?;
    }
    if patch.scale_id.touches() {
        conn.execute(
            "UPDATE phrase SET scale_id = ?2 WHERE id = ?1",
            params![id.get(), patch.scale_id.value().map(|i| i.get())],
        )?;
    }
    if let Some(root) = patch.root {
        conn.execute(
            "UPDATE phrase SET root = ?2 WHERE id = ?1",
            params![id.get(), root],
        )?;
    }

    Ok((
        Command::SetPhrase { id, patch },
        vec![Command::SetPhrase { id, patch: prior }],
    ))
}

pub(super) fn create_track(conn: &Connection, id: Option<TrackId>, spec: TrackSpec) -> Outcome {
    conn.execute(
        "INSERT INTO track (id, phrase_id, name, ord, extra) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            id.map(|i| i.get()),
            spec.phrase_id.get(),
            spec.name,
            spec.ord,
            json_text(&spec.extra)?,
        ],
    )?;
    let new_id = TrackId(conn.last_insert_rowid());
    Ok((
        Command::CreateTrack {
            id: Some(new_id),
            track: spec,
        },
        vec![Command::RemoveTrack { id: vec![new_id] }],
    ))
}

pub(super) fn remove_track(conn: &Connection, id: Vec<TrackId>) -> Outcome {
    let mut inverse = Vec::new();
    for &track_id in &id {
        let existing =
            query::track(conn, track_id)?.ok_or_else(|| missing("track", track_id.get()))?;
        conn.execute("DELETE FROM track WHERE id = ?1", params![track_id.get()])?;
        inverse.push(Command::CreateTrack {
            id: Some(track_id),
            track: existing.spec,
        });
    }
    Ok((Command::RemoveTrack { id }, inverse))
}

fn insert_event(
    conn: &Connection,
    container: Container,
    spec: &EventSpec,
) -> Result<EventId, StoreError> {
    let (phrase_id, track_id) = match container {
        Container::Phrase(id) => (Some(id.get()), None),
        Container::Track(id) => (None, Some(id.get())),
    };
    conn.execute(
        "INSERT INTO event (id, phrase_id, track_id, kind, at_tick, dur_tick, note_number, \
         velocity, extra) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            spec.id.map(|i| i.get()),
            phrase_id,
            track_id,
            spec.kind.as_str(),
            spec.at_tick.get(),
            spec.dur_tick.get(),
            spec.note_number.map(|n| n.get()),
            spec.velocity,
            json_text(&spec.extra)?,
        ],
    )?;
    Ok(EventId(conn.last_insert_rowid()))
}

pub(super) fn add_event(conn: &Connection, container: Container, event: Vec<EventSpec>) -> Outcome {
    let mut resolved = Vec::with_capacity(event.len());
    let mut id = Vec::with_capacity(event.len());
    for spec in event {
        let new_id = insert_event(conn, container, &spec)?;
        id.push(new_id);
        resolved.push(EventSpec {
            id: Some(new_id),
            ..spec
        });
    }
    Ok((
        Command::AddEvent {
            container,
            event: resolved,
        },
        vec![Command::RemoveEvent { id }],
    ))
}

/// The inverse carries the removed rows themselves — they cannot be re-derived
/// later, which is exactly why undo payloads are computed at execution time.
pub(super) fn remove_event(conn: &Connection, id: Vec<EventId>) -> Outcome {
    // Group by container so each inverse command is a well-formed AddEvent.
    let mut by_container: EventByContainer = BTreeMap::new();
    for &event_id in &id {
        let existing =
            query::event(conn, event_id)?.ok_or_else(|| missing("event", event_id.get()))?;
        let key = match existing.container {
            Container::Phrase(p) => (Some(p.get()), None),
            Container::Track(t) => (None, Some(t.get())),
        };
        by_container
            .entry(key)
            .or_insert_with(|| (existing.container, Vec::new()))
            .1
            .push(EventSpec {
                id: Some(existing.id),
                kind: existing.kind,
                at_tick: existing.at_tick,
                dur_tick: existing.dur_tick,
                note_number: existing.note_number,
                velocity: existing.velocity,
                extra: existing.extra,
            });
        conn.execute("DELETE FROM event WHERE id = ?1", params![event_id.get()])?;
    }
    let inverse = by_container
        .into_values()
        .map(|(container, event)| Command::AddEvent { container, event })
        .collect();
    Ok((Command::RemoveEvent { id }, inverse))
}

pub(super) fn create_phrase_instance(
    conn: &Connection,
    id: Option<PhraseInstanceId>,
    spec: PhraseInstanceSpec,
) -> Outcome {
    // Cyclic reference is prohibited at the model level (R-407): nesting a
    // phrase inside one it can already reach would make realization diverge.
    if let InstanceContainer::ParentPhrase(parent) = spec.container
        && query::phrase_reaches(conn, spec.phrase_id, parent)?
    {
        return Err(StoreError::PhraseCycle {
            child: spec.phrase_id.get(),
            parent: parent.get(),
        });
    }
    let (track_id, parent_phrase_id) = match spec.container {
        InstanceContainer::Track(id) => (Some(id.get()), None),
        InstanceContainer::ParentPhrase(id) => (None, Some(id.get())),
    };
    conn.execute(
        "INSERT INTO phrase_instance (id, phrase_id, track_id, parent_phrase_id, at_tick, \
         offset_tick, length_tick, loop_count, transpose, mute, extra) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id.map(|i| i.get()),
            spec.phrase_id.get(),
            track_id,
            parent_phrase_id,
            spec.at_tick.get(),
            spec.offset_tick.get(),
            spec.length_tick.map(|t| t.get()),
            spec.loop_count,
            spec.transpose,
            i64::from(spec.mute),
            json_text(&spec.extra)?,
        ],
    )?;
    let new_id = PhraseInstanceId(conn.last_insert_rowid());
    Ok((
        Command::CreatePhraseInstance {
            id: Some(new_id),
            phrase_instance: spec,
        },
        vec![Command::RemovePhraseInstance { id: vec![new_id] }],
    ))
}

pub(super) fn remove_phrase_instance(conn: &Connection, id: Vec<PhraseInstanceId>) -> Outcome {
    let mut inverse = Vec::new();
    for &instance_id in &id {
        let existing = query::phrase_instance(conn, instance_id)?
            .ok_or_else(|| missing("phrase_instance", instance_id.get()))?;
        conn.execute(
            "DELETE FROM phrase_instance WHERE id = ?1",
            params![instance_id.get()],
        )?;
        inverse.push(Command::CreatePhraseInstance {
            id: Some(instance_id),
            phrase_instance: existing.spec,
        });
    }
    Ok((Command::RemovePhraseInstance { id }, inverse))
}

pub(super) fn set_phrase_instance_param(
    conn: &Connection,
    id: PhraseInstanceId,
    patch: PhraseInstancePatch,
) -> Outcome {
    let existing =
        query::phrase_instance(conn, id)?.ok_or_else(|| missing("phrase_instance", id.get()))?;
    let prior = PhraseInstancePatch {
        at_tick: patch.at_tick.map(|_| existing.spec.at_tick),
        offset_tick: patch.offset_tick.map(|_| existing.spec.offset_tick),
        length_tick: if patch.length_tick.touches() {
            Change::restoring(existing.spec.length_tick)
        } else {
            Change::Leave
        },
        loop_count: patch.loop_count.map(|_| existing.spec.loop_count),
        transpose: patch.transpose.map(|_| existing.spec.transpose),
        mute: patch.mute.map(|_| existing.spec.mute),
    };

    if let Some(at_tick) = patch.at_tick {
        conn.execute(
            "UPDATE phrase_instance SET at_tick = ?2 WHERE id = ?1",
            params![id.get(), at_tick.get()],
        )?;
    }
    if let Some(offset_tick) = patch.offset_tick {
        conn.execute(
            "UPDATE phrase_instance SET offset_tick = ?2 WHERE id = ?1",
            params![id.get(), offset_tick.get()],
        )?;
    }
    if patch.length_tick.touches() {
        conn.execute(
            "UPDATE phrase_instance SET length_tick = ?2 WHERE id = ?1",
            params![id.get(), patch.length_tick.value().map(|t| t.get())],
        )?;
    }
    if let Some(loop_count) = patch.loop_count {
        conn.execute(
            "UPDATE phrase_instance SET loop_count = ?2 WHERE id = ?1",
            params![id.get(), loop_count],
        )?;
    }
    if let Some(transpose) = patch.transpose {
        conn.execute(
            "UPDATE phrase_instance SET transpose = ?2 WHERE id = ?1",
            params![id.get(), transpose],
        )?;
    }
    if let Some(mute) = patch.mute {
        conn.execute(
            "UPDATE phrase_instance SET mute = ?2 WHERE id = ?1",
            params![id.get(), i64::from(mute)],
        )?;
    }

    Ok((
        Command::SetPhraseInstanceParam { id, patch },
        vec![Command::SetPhraseInstanceParam { id, patch: prior }],
    ))
}

/// Replaces the phrase's whole tempo map, so the inverse is simply the prior
/// map — total, and free of merge semantics nobody has asked for yet.
pub(super) fn set_tempo(conn: &Connection, phrase_id: PhraseId, point: Vec<TempoPoint>) -> Outcome {
    let prior = query::tempo_point(conn, phrase_id)?;
    conn.execute(
        "DELETE FROM tempo_point WHERE phrase_id = ?1",
        params![phrase_id.get()],
    )?;
    for p in &point {
        conn.execute(
            "INSERT INTO tempo_point (phrase_id, at_tick, usec_per_quarter) VALUES (?1, ?2, ?3)",
            params![phrase_id.get(), p.at_tick.get(), p.usec_per_quarter],
        )?;
    }
    Ok((
        Command::SetTempo { phrase_id, point },
        vec![Command::SetTempo {
            phrase_id,
            point: prior,
        }],
    ))
}

/// Capture (R-807). Distinct from AddEvent so a take is legible in the journal
/// and can carry capture metadata later; the rows it writes are ordinary direct
/// events on the track.
pub(super) fn record_batch(conn: &Connection, track_id: TrackId, event: Vec<EventSpec>) -> Outcome {
    let container = Container::Track(track_id);
    let mut resolved = Vec::with_capacity(event.len());
    let mut id = Vec::with_capacity(event.len());
    for spec in event {
        let new_id = insert_event(conn, container, &spec)?;
        id.push(new_id);
        resolved.push(EventSpec {
            id: Some(new_id),
            ..spec
        });
    }
    Ok((
        Command::RecordBatch {
            track_id,
            event: resolved,
        },
        vec![Command::RemoveEvent { id }],
    ))
}
