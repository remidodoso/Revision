//! rev-testkit — shared test support, consumed only via dev-dependencies
//! (Cargo permits the dev-dependency cycle with rev-core and rev-store):
//! fixture builders, throwaway projects, state comparison, and screenshot
//! golden masters. Golden-master comparators for DSP arrive with dsp-02.
//! Never shipped.

pub mod fixture;
pub mod image;
pub mod state;
pub mod temp;

pub use temp::TempProject;
