//! rev-store — store-primary persistence: the SQLite project store, command
//! journal, and snapshots (R-201..R-205). The journal is the only write path;
//! gesture = transaction. Crash-only by design: kill -9 at any moment loses
//! no committed gesture (the TMON test, R-202/R-808/R-1504). The realization
//! view is the model's executable specification.

mod exec;
mod genesis;

pub mod error;
pub mod journal;
pub mod project;
pub mod query;
pub mod replay;
pub mod schema;

pub use error::StoreError;
pub use project::{Gesture, Project};
