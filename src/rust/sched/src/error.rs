//! Compilation failures.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchedError {
    #[error("reading the project: {0}")]
    Store(#[from] rev_store::StoreError),

    /// A tuning a row referred to is not in the project. The store's foreign keys
    /// make this close to impossible; it is an error rather than a silent skip
    /// because silence is the one outcome nobody can diagnose.
    #[error("tuning {0} is missing")]
    MissingTuning(i64),
}
