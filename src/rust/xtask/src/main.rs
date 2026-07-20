//! rev-xtask — repo automation, invoked as `cargo xtask <command>` (alias in
//! .cargo/config.toml). Commands per the boot-03 proposal: `tmon` (seeded
//! kill-loop against the store, stage 1), `filemap` (two-way file-map checker,
//! mismatches are failures), `perf` (ledger recorder into perf/ledger.jsonl,
//! stage 2). Plain binary, no CLI framework; each command's implementation
//! lands with its consumer, and unimplemented commands exit nonzero so they
//! can never green a pipeline by accident.

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some(name @ ("tmon" | "filemap" | "perf")) => {
            eprintln!("xtask {name}: not implemented yet (lands with its consumer stage)");
            std::process::exit(1);
        }
        _ => {
            eprintln!("usage: cargo xtask <tmon|filemap|perf>");
            std::process::exit(1);
        }
    }
}
