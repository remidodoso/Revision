//! rev-app — the composition root: wiring, command dispatch, view state.
//!
//! The library half exists so the application and its diagnostic binaries share
//! one wiring rather than two. `rev-tone` (the headless first sound) and the
//! windowed application both open the engine through [`audio::Audio`], so
//! whatever is proved by one is true of the other.

pub mod audio;
pub mod follow;
pub mod latency;
pub mod mhall;
pub mod midi;
pub mod pad;
pub mod record;
pub mod roll;
