//! rev-midi — MIDI I/O: midir wrapper with hot-plug enumeration (R-601),
//! driver-boundary timestamps and clock-domain correlation to the engine's
//! sample clock (R-603), and the thru fast path (R-605). Input forks at
//! birth: fast path → engine (live), event path → app (capture/journal) —
//! the live/playback classification exists from the first build.
