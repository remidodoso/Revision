# boot-03 proposal — testing stack

**Status: checkpoint proposal, awaiting approval.** Covers the grown scope from the
plan: test frameworks, golden-master harness, TMON kill-test scripting, bench
framework, soak/deadline harness placement, and the perf ledger format. Placement
policy (sibling test modules, umbrella integration binaries, `#[ignore]` for device
tests) is already settled in the coding standard and not revisited here. Decisions
requested are gathered in §8.

## 1. Unit & integration tests — no framework

The std `#[test]` harness via `cargo test`, exactly as Rust ships it. No test
framework dependency exists that earns its place; the default-convention clause
applies. Assertion style: std `assert!`/`assert_eq!`.

## 2. Property testing — proptest

**proptest** (dual MIT/Apache) as a dev-dependency: strategy-based generation with
integrated shrinking, and — the decisive feature — **failure persistence**: failing
cases are written to `proptest-regressions/` files which we commit, so every
shrunk counterexample becomes a permanent regression test automatically. Fits the
seeded-determinism ethos (failures are replayable by construction). First
customers: realization-view-vs-optimized-twin equivalence (§6f-bis oracle testing),
tuning math, tick arithmetic. (quickcheck considered: simpler but less maintained,
no failure persistence.)

## 3. Golden-master harness — custom, in rev-testkit

No off-the-shelf snapshot crate (insta et al. solve string/JSON snapshots; our
masters are spectra and meters). The harness is small custom code:

- **testdata/ layout:** raw audio as `.f32` (little-endian f32 frames) with a JSON
  sidecar (`sample_rate`, `channel`, provenance: source, seed, generator version);
  spectra and meter references as JSON. No container-format dependency; everything
  inspectable and diffable.
- **Comparators (phase-independent, per the Padlington plan):** magnitude-spectrum
  comparison with dB tolerance; meter comparison (RMS/peak/centroid) with stated
  tolerances; exact bit-identity helper for determinism gates (render twice →
  identical, R-1402).
- **Notch relationship:** notch stays the JS-side oracle. Read-only visits export
  notch-computed references into testdata/; the Rust harness compares against
  those files. Zero Notorolla changes.

## 4. rev-testkit — the shared test-support crate

New workspace member `src/rust/testkit/` (package `rev-testkit`, publish = false),
consumed only via dev-dependencies. Contents: fixture builders (phrases, tunings,
seeded projects), the golden-master comparators (§3), proptest strategies for
model types, seeded-PRNG helpers. Cargo explicitly permits the dev-dependency
cycle (testkit depends on rev-core; rev-core dev-depends on testkit).

## 5. xtask — repo automation

The cargo-xtask pattern: workspace member `src/rust/xtask/` (package `rev-xtask`,
a plain binary — no CLI framework; match on `std::env::args`) plus a
`.cargo/config.toml` alias so `cargo xtask <command>` works. First commands:

- `cargo xtask tmon` — the kill-test (§6).
- `cargo xtask filemap` — file-map checker, **both directions**: every entry's
  path exists, and every file/directory of consequence (crates, root files,
  doc files) is covered by an entry. Mismatches are failures, never warnings —
  the map's agent-facing credibility depends on it. Runs in CI's lint job.
- `cargo xtask perf` — perf recorder (§7); lands with eng-03 per doctrine.

## 6. TMON kill-test approach

A small writer binary (lives in rev-store's `tests/` support or `examples/`) opens
a store and applies journaled gestures in a loop, printing each committed sequence
number. `xtask tmon` spawns it, waits a **seeded** random interval, hard-kills it
(TerminateProcess — the Windows kill -9), reopens the store, and verifies: intact
database, journal replay succeeds, nothing at-or-before the last printed commit is
lost. Repeat N iterations (N and seed printed for replay). Runs in CI (no hardware
needed) once stage 1 exists — it is a correctness test and may gate.
**Honest scope:** this proves crash consistency (process death), not power loss —
torn-write defense is SQLite's own territory, tested by them far beyond our reach.

## 7. Bench framework & perf ledger

- **criterion** (dual MIT/Apache) as the bench framework, in per-crate `benches/`.
  Chosen over divan for one reason: **structured output** (JSON estimates under
  `target/criterion/`) that `xtask perf` can harvest into the ledger; divan is
  lighter but stdout-only. iai-callgrind (instruction counts, CI-stable,
  Linux-only) deferred — noted as a possible later series source.
- **Ledger:** `perf/ledger.jsonl` at repo root, append-only, committed (the public
  performance history; a Pages viewer `doc/revision_perf.html` arrives with the
  first real data). One record per measurement:
  `{ ts, commit, machine, os, toolchain, build (profile/opt/features), test,
  metric { … }, seed? }`. Series identity = (test, machine, build) — the
  metadata that makes graphs honest. Machine name comes from a `--machine` flag
  or `REVISION_MACHINE` env; `xtask perf` refuses to record without it.
- **Soak/deadline harnesses** are xtask-adjacent binaries driving the engine's
  telemetry ring (eng-03 onward), recording into the same ledger. Per doctrine:
  everything here tracks; nothing gates.

## 8. Decisions requested

1. std test harness, no test framework — recommended: yes.
2. proptest as dev-dependency; `proptest-regressions/` committed — recommended: yes.
3. Custom golden-master harness; `.f32` + JSON sidecar format for testdata/ —
   recommended: yes.
4. rev-testkit workspace member (dev-dep only) — recommended: yes.
5. rev-xtask member + `cargo xtask` alias; `filemap` check joins CI lint job —
   recommended: yes.
6. TMON approach per §6 (seeded kill-loop, may gate in CI) — recommended: yes.
7. criterion as bench framework (structured output for the ledger) — recommended:
   yes.
8. Perf ledger as committed `perf/ledger.jsonl`, machine-keyed, format per §7 —
   recommended: yes.

On approval: rev-testkit and rev-xtask crates scaffolded (empty, doc-commented,
file map updated); proptest/criterion enter workspace.dependencies as
dev-dependencies when their first consumer lands (stage 1 / stage 2), not before;
coding-standard Tests section gains the settled entries.
