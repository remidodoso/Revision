//! The query catalog: every read, in one place.
//!
//! What this is *not* is a semantic layer that hides the schema and becomes the
//! privileged path — that is the disease §6g diagnoses in other DAWs. What it
//! is: a catalog, so column names live in exactly two places (the DDL and here)
//! and every other crate speaks in `rev-core` types. Four rules keep it honest:
//!
//! 1. One function, one statement. Read logic that recurs becomes a `v_*` view
//!    — shared surface, visible to scripts and the `sqlite3` CLI alike — never a
//!    private helper.
//! 2. Read-only by construction: nothing here mutates. Writes are commands.
//! 3. Named for what it returns, not for who calls it.
//! 4. Every function has a test, because SQL is checked at runtime and a missed
//!    rename must fail a test rather than ship.
//!
//! Capability parity holds through scripting: each entry is one statement a
//! script could write verbatim against the same read-only connection.

use rusqlite::{Connection, OptionalExtension, Row, types::Type};
use serde_json::Value;

use rev_core::id::{
    EventId, MaterializedTuningInstanceId, PhraseId, PhraseInstanceId, ScaleId, TrackId, TuningId,
};
use rev_core::note::NoteNumber;
use rev_core::phrase::{
    Container, Event, EventKind, InstanceContainer, Phrase, PhraseInstance, PhraseInstanceSpec,
    PhraseSpec, RealizedEvent, TempoPoint, Track, TrackSpec,
};
use rev_core::scale::{Scale, ScaleSpec};
use rev_core::tick::Tick;
use rev_core::tuning::{
    MaterializedTuning, Ratio, Tuning, TuningKind, TuningNote, TuningNoteValue, TuningSpec,
};

use crate::error::StoreError;

// ── Row mapping ────────────────────────────────────────────────────────────
// Mechanical deserialization only: no semantics live in these.

fn parse_json(text: &str) -> rusqlite::Result<Value> {
    serde_json::from_str(text)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(e)))
}

fn json_column(row: &Row, name: &str) -> rusqlite::Result<Value> {
    parse_json(&row.get::<_, String>(name)?)
}

fn optional_json_column(row: &Row, name: &str) -> rusqlite::Result<Option<Value>> {
    match row.get::<_, Option<String>>(name)? {
        Some(text) => Ok(Some(parse_json(&text)?)),
        None => Ok(None),
    }
}

fn row_to_tuning(row: &Row) -> rusqlite::Result<Tuning> {
    let kind = match row.get::<_, String>("kind")?.as_str() {
        "equal" => TuningKind::Equal,
        _ => TuningKind::Table,
    };
    let period = match (
        row.get::<_, Option<i64>>("period_num")?,
        row.get::<_, Option<i64>>("period_den")?,
    ) {
        (Some(num), Some(den)) => Some(Ratio { num, den }),
        _ => None,
    };
    Ok(Tuning {
        id: TuningId(row.get("id")?),
        spec: TuningSpec {
            name: row.get("name")?,
            description: row.get("description")?,
            kind,
            period,
            note_per_period: row.get("note_per_period")?,
            anchor_note: NoteNumber(row.get("anchor_note")?),
            anchor_freq: row.get("anchor_freq")?,
            note_min: row.get::<_, Option<i32>>("note_min")?.map(NoteNumber),
            note_max: row.get::<_, Option<i32>>("note_max")?.map(NoteNumber),
            naming: row.get("naming")?,
            origin: row.get("origin")?,
            seed: optional_json_column(row, "seed")?,
            parent_tuning_id: row.get::<_, Option<i64>>("parent_tuning_id")?.map(TuningId),
            extra: json_column(row, "extra")?,
        },
    })
}

fn row_to_tuning_note(row: &Row) -> rusqlite::Result<TuningNote> {
    let value = match (
        row.get::<_, Option<i64>>("ratio_num")?,
        row.get::<_, Option<i64>>("ratio_den")?,
    ) {
        (Some(num), Some(den)) => TuningNoteValue::Ratio(Ratio { num, den }),
        _ => TuningNoteValue::Freq(row.get("freq")?),
    };
    Ok(TuningNote {
        note_number: NoteNumber(row.get("note_number")?),
        value,
    })
}

fn row_to_scale(row: &Row) -> rusqlite::Result<Scale> {
    let mask: Vec<i32> = serde_json::from_value(json_column(row, "mask")?)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(e)))?;
    Ok(Scale {
        id: ScaleId(row.get("id")?),
        spec: ScaleSpec {
            name: row.get("name")?,
            description: row.get("description")?,
            note_per_period: row.get("note_per_period")?,
            tuning_id: row.get::<_, Option<i64>>("tuning_id")?.map(TuningId),
            mask,
            origin: row.get("origin")?,
            seed: optional_json_column(row, "seed")?,
            parent_scale_id: row.get::<_, Option<i64>>("parent_scale_id")?.map(ScaleId),
            extra: json_column(row, "extra")?,
        },
    })
}

fn row_to_phrase(row: &Row) -> rusqlite::Result<Phrase> {
    Ok(Phrase {
        id: PhraseId(row.get("id")?),
        spec: PhraseSpec {
            name: row.get("name")?,
            length_tick: Tick(row.get("length_tick")?),
            tuning_id: row.get::<_, Option<i64>>("tuning_id")?.map(TuningId),
            scale_id: row.get::<_, Option<i64>>("scale_id")?.map(ScaleId),
            root: row.get("root")?,
            origin: row.get("origin")?,
            seed: optional_json_column(row, "seed")?,
            parent_phrase_id: row.get::<_, Option<i64>>("parent_phrase_id")?.map(PhraseId),
            extra: json_column(row, "extra")?,
        },
    })
}

fn row_to_track(row: &Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: TrackId(row.get("id")?),
        spec: TrackSpec {
            phrase_id: PhraseId(row.get("phrase_id")?),
            name: row.get("name")?,
            ord: row.get("ord")?,
            extra: json_column(row, "extra")?,
        },
    })
}

fn row_to_event(row: &Row) -> rusqlite::Result<Event> {
    let container = match (
        row.get::<_, Option<i64>>("phrase_id")?,
        row.get::<_, Option<i64>>("track_id")?,
    ) {
        (Some(id), _) => Container::Phrase(PhraseId(id)),
        (_, Some(id)) => Container::Track(TrackId(id)),
        // Unreachable while the schema CHECK holds; treated as data corruption.
        _ => {
            return Err(rusqlite::Error::InvalidColumnType(
                0,
                "event has neither container".into(),
                Type::Null,
            ));
        }
    };
    Ok(Event {
        id: EventId(row.get("id")?),
        container,
        kind: EventKind(row.get("kind")?),
        at_tick: Tick(row.get("at_tick")?),
        dur_tick: Tick(row.get("dur_tick")?),
        note_number: row.get::<_, Option<i32>>("note_number")?.map(NoteNumber),
        velocity: row.get("velocity")?,
        extra: json_column(row, "extra")?,
    })
}

fn row_to_phrase_instance(row: &Row) -> rusqlite::Result<PhraseInstance> {
    let container = match (
        row.get::<_, Option<i64>>("track_id")?,
        row.get::<_, Option<i64>>("parent_phrase_id")?,
    ) {
        (Some(id), _) => InstanceContainer::Track(TrackId(id)),
        (_, Some(id)) => InstanceContainer::ParentPhrase(PhraseId(id)),
        _ => {
            return Err(rusqlite::Error::InvalidColumnType(
                0,
                "phrase_instance has neither container".into(),
                Type::Null,
            ));
        }
    };
    Ok(PhraseInstance {
        id: PhraseInstanceId(row.get("id")?),
        spec: PhraseInstanceSpec {
            phrase_id: PhraseId(row.get("phrase_id")?),
            container,
            at_tick: Tick(row.get("at_tick")?),
            offset_tick: Tick(row.get("offset_tick")?),
            length_tick: row.get::<_, Option<i64>>("length_tick")?.map(Tick),
            loop_count: row.get("loop_count")?,
            transpose: row.get("transpose")?,
            mute: row.get::<_, i64>("mute")? != 0,
            extra: json_column(row, "extra")?,
        },
    })
}

fn row_to_realized(row: &Row) -> rusqlite::Result<RealizedEvent> {
    Ok(RealizedEvent {
        track_id: TrackId(row.get("track_id")?),
        kind: EventKind(row.get("kind")?),
        at_tick: Tick(row.get("at_tick")?),
        dur_tick: Tick(row.get("dur_tick")?),
        note_number: row.get::<_, Option<i32>>("note_number")?.map(NoteNumber),
        velocity: row.get("velocity")?,
        tuning_id: row.get::<_, Option<i64>>("tuning_id")?.map(TuningId),
        phrase_instance_id: row
            .get::<_, Option<i64>>("phrase_instance_id")?
            .map(PhraseInstanceId),
    })
}

const TUNING_COLUMN: &str = "id, name, description, kind, period_num, period_den, note_per_period, \
     anchor_note, anchor_freq, note_min, note_max, naming, origin, seed, parent_tuning_id, extra";
const SCALE_COLUMN: &str = "id, name, description, note_per_period, tuning_id, mask, origin, seed, \
     parent_scale_id, extra";
const PHRASE_COLUMN: &str = "id, name, length_tick, tuning_id, scale_id, root, origin, seed, \
     parent_phrase_id, extra";
const EVENT_COLUMN: &str =
    "id, phrase_id, track_id, kind, at_tick, dur_tick, note_number, velocity, extra";
const INSTANCE_COLUMN: &str = "id, phrase_id, track_id, parent_phrase_id, at_tick, offset_tick, \
     length_tick, loop_count, transpose, mute, extra";

// ── The catalog ────────────────────────────────────────────────────────────

pub fn meta(conn: &Connection, key: &str) -> Result<Option<String>, StoreError> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

pub fn tuning(conn: &Connection, id: TuningId) -> Result<Option<Tuning>, StoreError> {
    let sql = format!("SELECT {TUNING_COLUMN} FROM tuning WHERE id = ?1");
    Ok(conn.query_row(&sql, [id.get()], row_to_tuning).optional()?)
}

pub fn tuning_by_name(conn: &Connection, name: &str) -> Result<Option<Tuning>, StoreError> {
    let sql = format!("SELECT {TUNING_COLUMN} FROM tuning WHERE name = ?1");
    Ok(conn.query_row(&sql, [name], row_to_tuning).optional()?)
}

pub fn tuning_note(conn: &Connection, tuning_id: TuningId) -> Result<Vec<TuningNote>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT note_number, ratio_num, ratio_den, freq FROM tuning_note \
         WHERE tuning_id = ?1 ORDER BY note_number",
    )?;
    let rows = statement.query_map([tuning_id.get()], row_to_tuning_note)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The most recent materialization of a tuning. Latest-instance-wins is the v0
/// resolution rule; pinning a specific instance is anticipated (the binder id
/// is the dynamic-tuning funnel) but not yet needed.
pub fn latest_materialized_instance(
    conn: &Connection,
    tuning_id: TuningId,
) -> Result<Option<MaterializedTuningInstanceId>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT MAX(id) FROM materialized_tuning_instance WHERE tuning_id = ?1",
            [tuning_id.get()],
            |r| r.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten()
        .map(MaterializedTuningInstanceId))
}

/// A frozen tuning table, reassembled from its rows.
pub fn materialized_tuning(
    conn: &Connection,
    instance_id: MaterializedTuningInstanceId,
) -> Result<Option<MaterializedTuning>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT m.note_number AS note_number, m.freq AS freq, t.note_per_period AS note_per_period \
         FROM materialized_tuning m \
         JOIN materialized_tuning_instance i ON i.id = m.materialized_tuning_instance_id \
         JOIN tuning t ON t.id = i.tuning_id \
         WHERE m.materialized_tuning_instance_id = ?1 ORDER BY m.note_number",
    )?;
    let rows = statement.query_map([instance_id.get()], |r| {
        Ok((
            r.get::<_, i32>("note_number")?,
            r.get::<_, f64>("freq")?,
            r.get::<_, Option<i32>>("note_per_period")?,
        ))
    })?;
    let collected = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if collected.is_empty() {
        return Ok(None);
    }
    let first_note = NoteNumber(collected[0].0);
    let note_per_period = collected[0].2;
    let freq = collected.iter().map(|&(_, f, _)| f).collect();
    Ok(Some(MaterializedTuning::from_rows(
        first_note,
        freq,
        note_per_period,
    )))
}

pub fn scale(conn: &Connection, id: ScaleId) -> Result<Option<Scale>, StoreError> {
    let sql = format!("SELECT {SCALE_COLUMN} FROM scale WHERE id = ?1");
    Ok(conn.query_row(&sql, [id.get()], row_to_scale).optional()?)
}

pub fn scale_by_name(conn: &Connection, name: &str) -> Result<Option<Scale>, StoreError> {
    let sql = format!("SELECT {SCALE_COLUMN} FROM scale WHERE name = ?1");
    Ok(conn.query_row(&sql, [name], row_to_scale).optional()?)
}

/// Masks applicable to a tuning of the given periodicity — the mechanical test
/// (R-509). Idiomatic fit is a separate, advisory question.
pub fn scale_for_period(conn: &Connection, note_per_period: i32) -> Result<Vec<Scale>, StoreError> {
    let sql = format!("SELECT {SCALE_COLUMN} FROM scale WHERE note_per_period = ?1 ORDER BY id");
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([note_per_period], row_to_scale)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn phrase(conn: &Connection, id: PhraseId) -> Result<Option<Phrase>, StoreError> {
    let sql = format!("SELECT {PHRASE_COLUMN} FROM phrase WHERE id = ?1");
    Ok(conn.query_row(&sql, [id.get()], row_to_phrase).optional()?)
}

pub fn phrase_by_name(conn: &Connection, name: &str) -> Result<Option<Phrase>, StoreError> {
    let sql = format!("SELECT {PHRASE_COLUMN} FROM phrase WHERE name = ?1");
    Ok(conn.query_row(&sql, [name], row_to_phrase).optional()?)
}

pub fn track(conn: &Connection, id: TrackId) -> Result<Option<Track>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT id, phrase_id, name, ord, extra FROM track WHERE id = ?1",
            [id.get()],
            row_to_track,
        )
        .optional()?)
}

pub fn track_in_phrase(conn: &Connection, phrase_id: PhraseId) -> Result<Vec<Track>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT id, phrase_id, name, ord, extra FROM track WHERE phrase_id = ?1 ORDER BY ord, id",
    )?;
    let rows = statement.query_map([phrase_id.get()], row_to_track)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn event(conn: &Connection, id: EventId) -> Result<Option<Event>, StoreError> {
    let sql = format!("SELECT {EVENT_COLUMN} FROM event WHERE id = ?1");
    Ok(conn.query_row(&sql, [id.get()], row_to_event).optional()?)
}

pub fn event_in_phrase(conn: &Connection, phrase_id: PhraseId) -> Result<Vec<Event>, StoreError> {
    let sql = format!("SELECT {EVENT_COLUMN} FROM event WHERE phrase_id = ?1 ORDER BY at_tick, id");
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([phrase_id.get()], row_to_event)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn event_on_track(conn: &Connection, track_id: TrackId) -> Result<Vec<Event>, StoreError> {
    let sql = format!("SELECT {EVENT_COLUMN} FROM event WHERE track_id = ?1 ORDER BY at_tick, id");
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([track_id.get()], row_to_event)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn phrase_instance(
    conn: &Connection,
    id: PhraseInstanceId,
) -> Result<Option<PhraseInstance>, StoreError> {
    let sql = format!("SELECT {INSTANCE_COLUMN} FROM phrase_instance WHERE id = ?1");
    Ok(conn
        .query_row(&sql, [id.get()], row_to_phrase_instance)
        .optional()?)
}

/// Where a phrase is used (R-411) — one statement, because the model is
/// relational at heart.
pub fn phrase_instance_of(
    conn: &Connection,
    phrase_id: PhraseId,
) -> Result<Vec<PhraseInstance>, StoreError> {
    let sql =
        format!("SELECT {INSTANCE_COLUMN} FROM phrase_instance WHERE phrase_id = ?1 ORDER BY id");
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([phrase_id.get()], row_to_phrase_instance)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Can `from` reach `target` by following nested instances downward?
///
/// The cycle test for R-407: nesting a phrase inside one it can already reach
/// would make realization non-terminating. A recursive CTE walks the nesting
/// graph, seeded with `from` so a phrase inside itself is caught too.
pub fn phrase_reaches(
    conn: &Connection,
    from: PhraseId,
    target: PhraseId,
) -> Result<bool, StoreError> {
    Ok(conn
        .query_row(
            "WITH RECURSIVE reach(id) AS ( \
               SELECT ?1 \
               UNION \
               SELECT i.phrase_id FROM phrase_instance i \
               JOIN reach r ON i.parent_phrase_id = r.id \
             ) SELECT 1 FROM reach WHERE id = ?2 LIMIT 1",
            [from.get(), target.get()],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .is_some())
}

pub fn tempo_point(conn: &Connection, phrase_id: PhraseId) -> Result<Vec<TempoPoint>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT at_tick, usec_per_quarter FROM tempo_point WHERE phrase_id = ?1 ORDER BY at_tick",
    )?;
    let rows = statement.query_map([phrase_id.get()], |r| {
        Ok(TempoPoint {
            at_tick: Tick(r.get("at_tick")?),
            usec_per_quarter: r.get("usec_per_quarter")?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The arrangement as it will actually sound, in time order — what the stage-2
/// schedule compiler consumes.
pub fn realized(conn: &Connection, track_id: TrackId) -> Result<Vec<RealizedEvent>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT track_id, kind, at_tick, dur_tick, note_number, velocity, tuning_id, \
         phrase_instance_id FROM v_realized WHERE track_id = ?1 ORDER BY at_tick, note_number",
    )?;
    let rows = statement.query_map([track_id.get()], row_to_realized)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
