# Revision — Coding Standard

**Default: ordinary Rust convention**, as practiced in a project expected to grow
large (>100k, perhaps >1M lines): rustfmt defaults, standard idioms, a workspace
of focused crates. Where this document is silent, do the conventional thing;
where the conventional thing seems wrong, raise it for discussion — the
resolution becomes an entry here. This document records only deviations and
decisions.

## Naming

- snake_case for functions and variables in every language — including ES,
  deliberately overriding JS camelCase — except where glue requires otherwise
  (foreign trait impls, external API callbacks). (One convention across a
  polyglot family.)
- Table, column, and variable names are singular, never plural: `cat[5]`, not
  `cats[5]`; `event`, not `events`. (English prose pluralizes freely.)
- Packages are `rev-*` (`rev-core`, `rev-engine`); crate directories are
  unprefixed (`src/rust/core/`). (Cargo rejects bare `core`; prefix uniformly.)

## SQL

- ANSI join syntax only: conditions in `ON` (prefer `ON` over `USING`);
  `CROSS JOIN` written explicitly when intended; comma joins never. (A
  forgotten join condition degrades silently into a cartesian product; ANSI
  makes it unwritable.)
- PKs are `id`; FKs are `<table>_id`, role-qualified when duplicated or
  self-referential (`parent_phrase_id`).
- Views are `v_*`. (Performance triage: know it's a view before opening the
  schema.) Indexes are `[object]_[column…]_i` — the object name inherited
  verbatim, so view-derived objects carry `v_` by composition; unique indexes
  use plain `_i` too.
- Overloaded concept words are always qualified — no bare `instance`
  (`phrase_instance`, `materialized_tuning_instance`).

## Arithmetic

- Note numbers are signed; pitch-class math uses euclidean modulo
  (`rem_euclid`), never bare `%`. (Bare `%` fails only below the anchor —
  silently.)

## Layout

- Virtual workspace manifest at the repo root; crates under `src/rust/*`; later
  `src/es/`, `src/cpp/`. (Language-keyed layout for a known-polyglot repo.)
- Per-crate inner `src/` is kept — the `src/rust/core/src/` stutter is accepted.
  (Toolchain defaults are cheaper than fighting them forever.)
- Cargo-fixed plural directory names (`tests/`, `benches/`) are foreign API.

## Dependencies & licensing

- The project is dual-licensed **MIT OR Apache-2.0** (the Rust convention):
  `LICENSE-MIT` and `LICENSE-APACHE` at the repo root, `license = "MIT OR
  Apache-2.0"` in every crate manifest; contributions are dual-licensed likewise
  unless stated otherwise.
- **The dependency tree is GPL-free** — audited, not assumed. (Preserves the
  permissive posture and commercial optionality. The VST3 bridge, if GPLv3, is
  out-of-process and outside this rule's scope by design.)
- **As dependency-free as practical:** no frameworks; only thin, boring,
  individually replaceable primitives (winit/wgpu/cosmic-text class); everything
  above the mechanism layer is rolled by hand. Every new dependency is a
  checkpoint (getstarted, ground rule 2).
- Proprietary, non-redistributable SDKs (ASIO, MTS-ESP) never enter the repo:
  build-time local opt-in features only, never built by CI.

## Files

- Around 1000 lines, flag the file to the user as a refactor candidate — never
  silently split; some huge files legitimately have to exist. (Guards against
  main.js-style accretion — 3,641 lines of "for now".)

## Bookkeeping

- New, moved, or deleted files update `doc/revision_file_map.json` in the same
  gesture; CI's filemap check fails on any mismatch, in both directions. (The
  map is an agent context-economy device; staleness destroys its credibility.)
- Map desc lines are written for retrieval, not documentation: keyword-bearing
  (key types, subsystem vocabulary, R-numbers), ≤140 characters.

## Comments

- Code is maintained with human-friendly comments.
- Dense functional style is encouraged *and* must carry a comment saying what is
  being mapped/filtered into what. (The chain is write-optimized; the comment
  re-optimizes it for reading.)
- Where a requirement motivates a design, cite its R-number in the doc comment.

## Tests

- Unit tests live in sibling child modules — `#[cfg(test)] mod test;` resolving
  to `foo/test.rs` — never inline in source files. (Separation of files without
  separation of access; `mod test` singular per the naming rule.)
- Integration tests: one umbrella binary per crate (`tests/it/main.rs`).
  (N test files as N binaries link N times; the umbrella links once.)
- Tests requiring audio/MIDI hardware are `#[ignore]`-marked so `cargo test`
  passes on deviceless machines. (CI has no sound card.)
- Property tests: proptest; `proptest-regressions/` files are committed — every
  shrunk counterexample becomes a permanent regression test. (Seeded
  determinism, applied to failures.)
- Benches: criterion, in per-crate `benches/`; `cargo xtask perf` harvests
  results into `perf/ledger.jsonl`. Perf tests track, never gate (getstarted
  doctrine).
- Golden masters: `testdata/` holds raw `.f32` frames + JSON sidecars carrying
  provenance (source, seed, generator version); comparators live in
  rev-testkit and are phase-independent (spectra in dB, meters).
- Shared test support: rev-testkit, consumed via dev-dependencies only. Repo
  automation: rev-xtask, via `cargo xtask` (tmon, filemap, perf).
