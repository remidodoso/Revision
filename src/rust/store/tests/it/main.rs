//! Integration tests for rev-store, as one umbrella binary: N test files would
//! link N times, and the link line already carries bundled SQLite.

mod catalog;
mod genesis;
mod history;
mod property;
mod realize;
