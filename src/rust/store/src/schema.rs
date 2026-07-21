//! The schema: the model surface itself (§6g — built-in editors are clients of
//! this, with no privileged back door).
//!
//! The data definition lives in `schema.sql` rather than in a Rust string, so
//! it reads as SQL everywhere SQL is read — the editor, GitHub, `sqlite3` — and
//! carries its own annotations, which `cargo xtask schema` extracts into the
//! browsable schema document. Approved at core-01; annotation convention and
//! generation at misc-02.

use rusqlite::Connection;

use crate::error::StoreError;

/// Bumped only by a migration, which is a checkpoint in its own right.
pub const SCHEMA_VERSION: i64 = 1;

pub const META_SCHEMA_VERSION: &str = "schema_version";
pub const META_PPQ: &str = "ppq";
pub const META_CREATED: &str = "created";
pub const META_DEFAULT_TUNING_ID: &str = "default_tuning_id";
pub const META_ROOT_PHRASE_ID: &str = "root_phrase_id";

/// The data definition, verbatim from `schema.sql`.
pub const DDL: &str = include_str!("schema.sql");

/// The model tables, in dependency order — used by state comparison and, later,
/// by the interchange serializer. History (`journal`, `snapshot`) is not model
/// state: replay reproduces the model, not the history it was replayed from.
pub const MODEL_TABLE: &[&str] = &[
    "meta",
    "tuning",
    "tuning_note",
    "materialized_tuning_instance",
    "materialized_tuning",
    "scale",
    "phrase",
    "track",
    "event",
    "phrase_instance",
    "tempo_point",
];

pub fn create(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(DDL)?;
    Ok(())
}
