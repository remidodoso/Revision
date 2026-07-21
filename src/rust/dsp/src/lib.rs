//! rev-dsp — the PADsynth bake (dsp-02).
//!
//! A harmonic amplitude profile is smeared into Gaussian frequency bands, every
//! bin gets a seeded random phase, and one inverse FFT yields a long,
//! seamlessly looping wavetable. Ported from `doc/revision_padlington_inventory.md`
//! §2–§7, a read-only census of the instrument being ported rather than an
//! imagined one.
//!
//! **Pure data-in, data-out.** No engine, no device, no real-time constraint, no
//! workspace dependency: it takes frequencies and returns samples (R-312). That
//! is why the whole thing is testable headless, and why the tests below are
//! identities rather than captured data.
//!
//! **What makes it clean.** The standard is Notorolla's Padlington, which is
//! free of aliasing character across the whole keyboard. Four mechanisms in the
//! source contribute, none of them designed for it (dsp-02 §4.2) — including
//! the engine's *linear* table interpolation, which is a `sinc²` lowpass and is
//! load-bearing. This port keeps all four and adds one guarantee they do not
//! give: half-octave table spacing with the band limit at
//! [`bake::band_limit`], under which no partial can cross Nyquist at any
//! playback rate, so fold-back energy is zero by construction (R-720).

pub mod bake;
pub mod profile;
pub mod rand;
pub mod spec;
pub mod spectrum;

pub use bake::{BASE_COUNT, BASE_HIGH, BASE_LOW, TABLE_LEN, TABLE_RMS, bake, bake_set, band_limit};
pub use spec::{BakeSpec, Source, Vowel};
