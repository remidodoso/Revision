//! rev-core — the pure model: event, phrase, instance, track, tick/tempo map,
//! tuning, and the command vocabulary. No I/O, no platform dependencies; the
//! Vision-layer semantics live here. Compiled native for desktop and to WASM
//! for the web family member (R-104). Pitch is degree-native through the
//! tuning seam (R-002); time is integer ticks at 5040 ppq (R-003).
