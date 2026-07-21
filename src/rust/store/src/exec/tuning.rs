//! Execution for project settings, tunings and scales.

use rusqlite::{Connection, params};

use rev_core::Command;
use rev_core::id::{MaterializedTuningInstanceId, ScaleId, TuningId};
use rev_core::note::NoteNumber;
use rev_core::scale::ScaleSpec;
use rev_core::tuning::{self, TuningKind, TuningNote, TuningNoteValue, TuningSpec};

use super::{json_text, missing, now_ms, optional_json_text};
use crate::error::StoreError;
use crate::query;

type Outcome = Result<(Command, Vec<Command>), StoreError>;

pub(super) fn set_meta(conn: &Connection, key: String, value: Option<String>) -> Outcome {
    let prior = query::meta(conn, &key)?;
    match &value {
        Some(v) => {
            conn.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, v],
            )?;
        }
        None => {
            conn.execute("DELETE FROM meta WHERE key = ?1", params![key])?;
        }
    }
    let inverse = vec![Command::SetMeta {
        key: key.clone(),
        value: prior,
    }];
    Ok((Command::SetMeta { key, value }, inverse))
}

pub(super) fn create_tuning(conn: &Connection, id: Option<TuningId>, spec: TuningSpec) -> Outcome {
    let kind = match spec.kind {
        TuningKind::Equal => "equal",
        TuningKind::Table => "table",
    };
    // A NULL id lets SQLite allocate; an explicit one reproduces history exactly.
    conn.execute(
        "INSERT INTO tuning (id, name, description, kind, period_num, period_den, \
         note_per_period, anchor_note, anchor_freq, note_min, note_max, naming, origin, seed, \
         parent_tuning_id, extra) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            id.map(|i| i.get()),
            spec.name,
            spec.description,
            kind,
            spec.period.map(|p| p.num),
            spec.period.map(|p| p.den),
            spec.note_per_period,
            spec.anchor_note.get(),
            spec.anchor_freq,
            spec.note_min.map(|n| n.get()),
            spec.note_max.map(|n| n.get()),
            spec.naming,
            spec.origin,
            optional_json_text(&spec.seed)?,
            spec.parent_tuning_id.map(|i| i.get()),
            json_text(&spec.extra)?,
        ],
    )?;
    let new_id = TuningId(conn.last_insert_rowid());
    Ok((
        Command::CreateTuning {
            id: Some(new_id),
            tuning: spec,
        },
        vec![Command::RemoveTuning { id: vec![new_id] }],
    ))
}

/// Removes a tuning and its note rows together, since the notes have no meaning
/// without their tuning. Does not cascade to anything referencing the tuning —
/// a foreign key will refuse, which is the intended protection.
pub(super) fn remove_tuning(conn: &Connection, id: Vec<TuningId>) -> Outcome {
    let mut inverse = Vec::new();
    for &tuning_id in &id {
        let existing =
            query::tuning(conn, tuning_id)?.ok_or_else(|| missing("tuning", tuning_id.get()))?;
        let note = query::tuning_note(conn, tuning_id)?;
        conn.execute(
            "DELETE FROM tuning_note WHERE tuning_id = ?1",
            params![tuning_id.get()],
        )?;
        conn.execute("DELETE FROM tuning WHERE id = ?1", params![tuning_id.get()])?;
        inverse.push(Command::CreateTuning {
            id: Some(tuning_id),
            tuning: existing.spec,
        });
        if !note.is_empty() {
            inverse.push(Command::SetTuningNote { tuning_id, note });
        }
    }
    Ok((Command::RemoveTuning { id }, inverse))
}

pub(super) fn set_tuning_note(
    conn: &Connection,
    tuning_id: TuningId,
    note: Vec<TuningNote>,
) -> Outcome {
    // Partition by what was there before: overwritten rows are restored by
    // value, newly created ones by removal.
    let existing = query::tuning_note(conn, tuning_id)?;
    let mut overwritten = Vec::new();
    let mut created = Vec::new();
    for row in &note {
        match existing.iter().find(|e| e.note_number == row.note_number) {
            Some(prior) => overwritten.push(*prior),
            None => created.push(row.note_number),
        }
    }

    for row in &note {
        let (ratio_num, ratio_den, freq) = match row.value {
            TuningNoteValue::Ratio(r) => (Some(r.num), Some(r.den), None),
            TuningNoteValue::Freq(f) => (None, None, Some(f)),
        };
        conn.execute(
            "INSERT INTO tuning_note (tuning_id, note_number, ratio_num, ratio_den, freq) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(tuning_id, note_number) DO UPDATE SET \
             ratio_num = excluded.ratio_num, ratio_den = excluded.ratio_den, freq = excluded.freq",
            params![
                tuning_id.get(),
                row.note_number.get(),
                ratio_num,
                ratio_den,
                freq
            ],
        )?;
    }

    let mut inverse = Vec::new();
    if !overwritten.is_empty() {
        inverse.push(Command::SetTuningNote {
            tuning_id,
            note: overwritten,
        });
    }
    if !created.is_empty() {
        inverse.push(Command::RemoveTuningNote {
            tuning_id,
            note_number: created,
        });
    }
    Ok((Command::SetTuningNote { tuning_id, note }, inverse))
}

pub(super) fn remove_tuning_note(
    conn: &Connection,
    tuning_id: TuningId,
    note_number: Vec<NoteNumber>,
) -> Outcome {
    let existing = query::tuning_note(conn, tuning_id)?;
    let removed: Vec<TuningNote> = existing
        .into_iter()
        .filter(|e| note_number.contains(&e.note_number))
        .collect();
    for number in &note_number {
        conn.execute(
            "DELETE FROM tuning_note WHERE tuning_id = ?1 AND note_number = ?2",
            params![tuning_id.get(), number.get()],
        )?;
    }
    let mut inverse = Vec::new();
    if !removed.is_empty() {
        inverse.push(Command::SetTuningNote {
            tuning_id,
            note: removed,
        });
    }
    Ok((
        Command::RemoveTuningNote {
            tuning_id,
            note_number,
        },
        inverse,
    ))
}

pub(super) fn materialize_tuning(
    conn: &Connection,
    id: Option<MaterializedTuningInstanceId>,
    tuning_id: TuningId,
    ts: Option<i64>,
) -> Outcome {
    let definition =
        query::tuning(conn, tuning_id)?.ok_or_else(|| missing("tuning", tuning_id.get()))?;
    let note = query::tuning_note(conn, tuning_id)?;
    let table = tuning::materialize(&definition.spec, &note)?;

    let stamp = ts.unwrap_or_else(now_ms);
    conn.execute(
        "INSERT INTO materialized_tuning_instance (id, tuning_id, ts) VALUES (?1, ?2, ?3)",
        params![id.map(|i| i.get()), tuning_id.get(), stamp],
    )?;
    let instance_id = MaterializedTuningInstanceId(conn.last_insert_rowid());

    let mut statement = conn.prepare(
        "INSERT INTO materialized_tuning (materialized_tuning_instance_id, note_number, freq) \
         VALUES (?1, ?2, ?3)",
    )?;
    for (note_number, freq) in table.rows() {
        statement.execute(params![instance_id.get(), note_number.get(), freq])?;
    }

    Ok((
        Command::MaterializeTuning {
            id: Some(instance_id),
            tuning_id,
            ts: Some(stamp),
        },
        vec![Command::RemoveMaterializedTuning {
            id: vec![instance_id],
        }],
    ))
}

/// The inverse re-runs the builder rather than restoring rows verbatim. That is
/// exact because materialization is deterministic (tested), and it keeps the
/// vocabulary from needing a raw row-insert command.
pub(super) fn remove_materialized_tuning(
    conn: &Connection,
    id: Vec<MaterializedTuningInstanceId>,
) -> Outcome {
    let mut inverse = Vec::new();
    for &instance_id in &id {
        let (tuning_id, ts): (i64, i64) = conn.query_row(
            "SELECT tuning_id, ts FROM materialized_tuning_instance WHERE id = ?1",
            params![instance_id.get()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        conn.execute(
            "DELETE FROM materialized_tuning WHERE materialized_tuning_instance_id = ?1",
            params![instance_id.get()],
        )?;
        conn.execute(
            "DELETE FROM materialized_tuning_instance WHERE id = ?1",
            params![instance_id.get()],
        )?;
        inverse.push(Command::MaterializeTuning {
            id: Some(instance_id),
            tuning_id: TuningId(tuning_id),
            ts: Some(ts),
        });
    }
    Ok((Command::RemoveMaterializedTuning { id }, inverse))
}

pub(super) fn create_scale(conn: &Connection, id: Option<ScaleId>, spec: ScaleSpec) -> Outcome {
    spec.validate()?;
    conn.execute(
        "INSERT INTO scale (id, name, description, note_per_period, tuning_id, mask, origin, \
         seed, parent_scale_id, extra) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id.map(|i| i.get()),
            spec.name,
            spec.description,
            spec.note_per_period,
            spec.tuning_id.map(|i| i.get()),
            serde_json::to_string(&spec.mask)?,
            spec.origin,
            optional_json_text(&spec.seed)?,
            spec.parent_scale_id.map(|i| i.get()),
            json_text(&spec.extra)?,
        ],
    )?;
    let new_id = ScaleId(conn.last_insert_rowid());
    Ok((
        Command::CreateScale {
            id: Some(new_id),
            scale: spec,
        },
        vec![Command::RemoveScale { id: vec![new_id] }],
    ))
}

pub(super) fn remove_scale(conn: &Connection, id: Vec<ScaleId>) -> Outcome {
    let mut inverse = Vec::new();
    for &scale_id in &id {
        let existing =
            query::scale(conn, scale_id)?.ok_or_else(|| missing("scale", scale_id.get()))?;
        conn.execute("DELETE FROM scale WHERE id = ?1", params![scale_id.get()])?;
        inverse.push(Command::CreateScale {
            id: Some(scale_id),
            scale: existing.spec,
        });
    }
    Ok((Command::RemoveScale { id }, inverse))
}
