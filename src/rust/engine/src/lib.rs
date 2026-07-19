//! rev-engine — the real-time audio engine: cpal duplex stream (R-301), the
//! engine-owned sample clock (R-302 — the callback is the clock), scheduler
//! consuming sample-stamped compiled schedules, voices, and the graph runtime
//! with Web Audio semantics (R-704). The RT callback is allocation-free by
//! law: pre-allocated state, lock-free rings in/out, garbage shipped back
//! over a ring, allocation guard in debug. Live path stays within one block
//! of the ring (R-304, R-1501).
