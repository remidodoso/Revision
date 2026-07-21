//! Log failures.
//!
//! Deliberately few. Logging is a side channel: once a [`Log`](crate::Log) is
//! open, nothing it does can fail in a way a caller should handle — a record
//! that cannot be written is dropped and counted, never returned as an error to
//! code that was doing something else. Only *opening* can fail.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LogError {
    #[error("log database: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("cannot create log directory {path}: {source}")]
    Directory {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// No application-data directory could be determined from the environment.
    /// The caller's recourse is [`Log::hush`](crate::Log::hush) — running
    /// without a log is degraded, not fatal.
    #[error("no application-data directory: {0}")]
    NoHome(&'static str),
}
