# boot-02 proposal — workspace layout, toolchain, CI skeleton

**Status: approved and implemented 2026-07-19.** Retained as the record of what was
decided and why; the plan (`revision_plan.json`, boot-02) records completion.

*Original header:* **checkpoint proposal, awaiting approval.** Nothing here is
implemented until approved (getstarted, ground rule 2). Decisions already settled in discussion are
restated for completeness and marked *(settled)*; genuinely open knobs are gathered
in §8.

## 1. Repository scaffold

```
Revision/
├── Cargo.toml                  # virtual workspace manifest (§3)
├── rust-toolchain.toml         # toolchain pin (§4)
├── deny.toml                   # license-audit config (§5)
├── .gitignore                  # /target
├── .nojekyll                   # present already
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md                   # one paragraph + license notice incl. the
│                               #   contributions-dual-licensed sentence
├── .github/workflows/ci.yml    # §5
├── doc/                        # existing documents + file map (§6)
├── src/
│   └── rust/                   # (settled) all seven crates scaffolded empty now:
│       ├── core/               #   rev-core    pure model; WASM-able (R-104)
│       ├── engine/             #   rev-engine  RT audio, graph runtime
│       ├── store/              #   rev-store   SQLite, journal, snapshots
│       ├── midi/               #   rev-midi    midir wrapper
│       ├── ui_mech/            #   rev-ui-mech mechanism contract impl
│       ├── ui_kit/             #   rev-ui-kit  control-skin widgets
│       └── app/                #   rev-app     composition root (binary)
└── testdata/                   # golden-master data (empty at bootstrap)
```

Each crate: `Cargo.toml` + `src/lib.rs` (or `main.rs` for rev-app) containing only a
`//!` doc comment stating the crate's responsibility and key R-numbers. Rationale
for scaffolding all seven now: names and boundaries are pinned, CI exercises the
real workspace shape from day 1, and empty crates cost nothing. Contents arrive
stage by stage.

## 2. Member manifests

```toml
[package]
name = "rev-core"               # etc.
version = "0.1.0"
edition.workspace = true
license.workspace = true
rust-version.workspace = true
publish = false                 # not on crates.io; prevents accidents; revisit if ever publishing
```

## 3. Root workspace manifest

```toml
[workspace]
resolver = "3"
members = ["src/rust/*"]

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"
rust-version = "1.95"
repository = "https://github.com/remidodoso/Revision"

[workspace.dependencies]
# empty at bootstrap — every entry arrives via a dependency checkpoint,
# version stated once here, inherited by members

# Build profiles (settled in discussion):
[profile.dev.package."*"]       # external dependencies: optimized once, cached
opt-level = 3

[profile.dev.package.rev-engine] # RT code optimized even in dev builds;
opt-level = 3                    # debug-assertions/overflow-checks stay on

# workspace members otherwise default opt-level 0; raise to 1 empirically if
# iteration feels sluggish (standing note from discussion). --release reserved
# for latency measurement and real use.
```

## 4. Toolchain pin

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.95.0"              # exact stable pin; upgrades are deliberate commits
components = ["rustfmt", "clippy"]
targets = ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "wasm32-unknown-unknown"]
```

Pin-exact rationale: reproducible builds, and toolchain upgrades become visible
commits — which the perf ledger wants (a toolchain change is a series boundary).
Edition 2024; `rust-version` mirrors the pin.

## 5. CI skeleton (GitHub Actions)

On push and PR, four jobs:

| Job | Runner | Does |
|---|---|---|
| lint | ubuntu | `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings` |
| test | windows | `cargo test --workspace` (device tests `#[ignore]`d, per standard) |
| cross | ubuntu | `cargo check --workspace --target x86_64-unknown-linux-gnu` and `--target aarch64-apple-darwin`; `cargo check -p rev-core --target wasm32-unknown-unknown` |
| license | ubuntu | `cargo deny check licenses` against `deny.toml` (allowlist: MIT/Apache/BSD/ISC/Zlib class; GPL family denied) — the GPL-free standard, enforced |

Notes: `cargo check` doesn't link, so Mac cross-*checks* run fine from a Linux
runner with the rustup target installed (honest caveat: if a future dependency's
build script probes for Apple SDKs, that job moves to a macos runner — cheap fix,
noted now so it isn't a surprise). Clippy at `-D warnings` on the default lint set;
finer lint policy accrues to the coding standard as case law. cargo-deny is the
first dev-tool dependency (checkpoint-gated like any other — approval of this
proposal covers it).

## 6. File map scaffold

Adopting the JSON-viewed-through-HTML pattern (discussion of 2026-07-19):

- `doc/revision_file_map.json` — directory/module-granularity entries:
  `{ "path", "desc", "layer"?, "generated": false }` under a `meta` header. Array
  key is `entry` (singular, per the naming standard). v0 is hand-maintained;
  schema anticipates `generated: true` for future xtask extraction from `//!`
  doc comments; existence-checker arrives with xtask (boot-03+).
- `doc/revision_file_map.html` — minimal viewer (grouped table), Pages-served
  beside the plan dashboard.
- Born populated with the scaffold's own entries.

## 7. Explicitly out of scope

Testing stack and harness architecture (boot-03); any real code; schema DDL
(core-01); engine/app interface (eng-01); perf ledger format (boot-03); Pages
enablement confirmation (boot-01 remainder, user-side).

## 8. Decisions requested

1. Exact-pin toolchain `1.95.0` (vs floating `stable`) — recommended: pin.
2. Edition 2024 — recommended: yes.
3. Scaffold all seven crates empty now (vs create-on-demand) — recommended: now.
4. Clippy `-D warnings` in CI — recommended: yes.
5. cargo-deny as the license auditor (first dev-tool dependency) — recommended: yes.
6. File map JSON + minimal viewer at scaffold time — recommended: yes.
7. Mac cross-check from the Linux runner, macos-runner fallback — recommended: yes.

On approval: I create everything above; you commit.
