//! rev-app — the composition root: wiring, command dispatch, view state.
//! No musical logic lives here or in any frontend, ever (the product-family
//! constitution). Threads: UI/main, MIDI callbacks, async store writer;
//! the RT callback belongs to rev-engine. Everything talks over rings.

fn main() {}
