//! rev-dsp integration tests: the bake, measured through the thing that reads
//! it.
//!
//! The unit tests inside the crate check the spectrum it *writes*. These check
//! what a rendered note actually sounds like, which needs an engine — a
//! dev-dependency, so no FFT and no bake ever enters the real-time crate.

mod alias;
