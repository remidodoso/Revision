//! Errors from the store. Wraps `CoreError` (pure model failures) plus the
//! things only a database can discover.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Core(#[from] rev_core::CoreError),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("no {kind} with id {id}")]
    NotFound { kind: &'static str, id: i64 },

    #[error("phrase {child} cannot be nested in phrase {parent}: it would create a cycle")]
    PhraseCycle { child: i64, parent: i64 },

    #[error("tuning {0} has no materialization; run materialize_tuning first")]
    NotMaterialized(i64),

    #[error("project at {0} is not a Revision store (no schema_version)")]
    NotAProject(String),

    #[error("project schema version {found} is not supported (expected {expected})")]
    SchemaVersion { found: i64, expected: i64 },
}
