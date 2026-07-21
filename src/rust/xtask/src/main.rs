//! rev-xtask — repo automation, invoked as `cargo xtask <command>` (alias in
//! .cargo/config.toml). Plain binary, no CLI framework.
//!
//! Commands:
//!   schema [path] [--check]   generate doc/revision_schema.json from a live
//!                             database; with a path, document that project
//!   filemap                   verify doc/revision_file_map.json both ways
//!   tmon                      the store kill-test (lands with stage 1's exit)
//!   perf                      ledger recorder (lands with stage 2)
//!
//! Unimplemented commands exit nonzero so they can never green a pipeline by
//! accident.

mod filemap;
mod schema;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let argument: Vec<String> = std::env::args().skip(1).collect();
    let name = argument.first().map(String::as_str);
    let rest: Vec<&str> = argument.iter().skip(1).map(String::as_str).collect();

    let outcome = match name {
        Some("schema") => schema::run(&repo_root(), &rest),
        Some("filemap") => filemap::run(&repo_root()),
        Some(pending @ ("tmon" | "perf")) => {
            eprintln!("xtask {pending}: not implemented yet (lands with its consumer stage)");
            return ExitCode::FAILURE;
        }
        _ => {
            eprintln!("usage: cargo xtask <schema [path] [--check]|filemap|tmon|perf>");
            return ExitCode::FAILURE;
        }
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// The workspace root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("xtask lives at <root>/src/rust/xtask")
        .to_path_buf()
}
