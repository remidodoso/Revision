//! rev-sched — the schedule compiler: the last place music exists.
//!
//! Note numbers, ticks, tunings and tempo go in; samples, frequencies and
//! durations come out (R-312). Everything below this crate is physics, and it is
//! physics *because* everything musical was resolved here.
//!
//! Three things it is careful about, each with a reason recorded where it is
//! implemented:
//!
//! - **[`TempoMap`] converts in integers**, segment-anchored, so conversion is
//!   monotonic, deterministic across platforms (R-1503), and non-accumulating.
//! - **Notes carry duration** (R-402a), so nothing downstream holds a pending
//!   obligation that a chunk boundary or a supersede could orphan.
//! - **Frequencies are resolved here**, through the same tuning tables the live
//!   path and the roll use, so the three cannot disagree.
//!
//! Approved at eng-06; see `doc/completed/revision_eng06_proposal.md`.

pub mod compile;
pub mod error;
pub mod tempo;
pub mod tune;

pub use compile::Compiler;
pub use error::SchedError;
pub use tempo::TempoMap;
pub use tune::TuneCache;
