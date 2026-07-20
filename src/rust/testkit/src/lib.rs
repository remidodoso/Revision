//! rev-testkit — shared test support, consumed only via dev-dependencies
//! (Cargo permits the dev-dependency cycle with rev-core): fixture builders
//! (phrases, tunings, seeded projects), golden-master comparators — magnitude
//! spectra with dB tolerance, RMS/peak/centroid meters, bit-identity for
//! determinism gates (R-1402) — proptest strategies for model types, and
//! seeded-PRNG helpers. testdata/ format: raw `.f32` frames + JSON sidecar
//! carrying provenance (source, seed, generator version). Never shipped.
