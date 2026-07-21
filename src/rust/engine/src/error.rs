//! Engine failures.
//!
//! All of them are *opening* failures. Once a stream is running nothing in the
//! callback returns an error to anyone — there is no one to return it to, and
//! the release policy is silence plus a record, never a panic (eng-01 §10).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("no audio device{}", match wanted {
        Some(name) => format!(" matching {name:?}"),
        None => " available".to_string(),
    })]
    NoDevice { wanted: Option<String> },

    #[error("cannot read configuration of {device}: {detail}")]
    Config { device: String, detail: String },

    #[error("cannot open a stream on {device}: {detail}")]
    Build { device: String, detail: String },
}
