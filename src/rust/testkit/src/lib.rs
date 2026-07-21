//! rev-testkit — shared test support, consumed only via dev-dependencies
//! (Cargo permits the dev-dependency cycle with rev-core and rev-store):
//! fixture builders, throwaway projects, state comparison, screenshot golden
//! masters, and — since dsp-02 — a spectrum, so that a test can listen to a
//! rendered note instead of only counting its samples. Never shipped.

pub mod fixture;
pub mod image;
pub mod spectrum;
pub mod state;
pub mod temp;

pub use temp::TempProject;
