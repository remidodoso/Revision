//! rev-ui-mech — the UI mechanism layer: window/surface (winit + wgpu), text
//! stack (cosmic-text), input routing per the mechanism contract — implicit
//! pointer capture, activation ≠ focus, keyboard-vs-text channels, and the
//! hands-off clause (R-907): never move the pointer, never steal focus, never
//! scroll unbidden. Mechanism only; widget style and identity belong above.
//! No native handle leaks into the kit-facing API; no Win32 outside this crate.
