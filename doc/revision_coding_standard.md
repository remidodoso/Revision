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
- A screen region needing repaint is **dirty**, never "damaged" — `Dirty`,
  `mark_dirty`, dirty rectangle, dirty region. (One word per concept, in code,
  comments, and docs alike.)

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
- Where a dependency offers a licence choice, the arm adopted is **recorded, not
  inferred** — including every case where a permissive arm is taken over a copyleft
  one (`self_cell`: Apache-2.0 adopted, GPL-2.0-only declined). `cargo xtask license`
  generates the record; R-1514 ships it with the binary.
- Given `MIT OR Apache-2.0`, take **Apache-2.0**: it matches our outbound licence and
  states grant and termination explicitly. (A mild default — audio DSP is not a
  patent-hazardous field, and Apache's grant covers only a contributor's own
  contribution regardless.)
- `BSD-4-Clause` is **explicitly denied**, not merely un-allowed — its advertising
  clause is the one trap in the permissive family, and an explicit denial records
  that we looked. BSD-2/3-Clause, Zlib, 0BSD, and Unicode-3.0 are allowed. In an SPDX
  expression `OR` is a choice and `AND` is not; tooling must distinguish them.
- **Clean room:** algorithms are ported from published literature and our own
  analysis, never from a competitor's binary. (The IP exposure that actually exists
  here is copyright and trade secret, not patents.)
- **Bundled assets are not crates** and `cargo-deny` cannot see them. Every asset
  under `asset/` is declared in the asset manifest with source, version, checksum,
  and licence, and ships its licence text alongside. (R-1514's document is generated
  from the dependency tree *and* that manifest; R-1515 credits the humans.) Assets
  come from upstream project releases, never from a service API that serves
  silently-updated subsets.
- **As dependency-free as practical:** no frameworks; only thin, boring,
  individually replaceable primitives (winit/wgpu/cosmic-text class); everything
  above the mechanism layer is rolled by hand. Every new dependency is a
  checkpoint (getstarted, ground rule 2).
- Proprietary, non-redistributable SDKs (ASIO, MTS-ESP) never enter the repo:
  build-time local opt-in features only, never built by CI.

## Files

- **Keep files under ~1000 lines. Don't write one and then ask — split it.**
  Splitting for size is routine work, not a checkpoint, and needs no permission:
  the seam is usually obvious (drawing, events, queries), and a child module keeps
  privacy intact when the parts must still see each other's internals.
  (Guards against main.js-style accretion — 3,641 lines of "for now". Nobody has
  ever complained of too many 500-line files.)
- Where a file genuinely cannot be split — generated output, or one indivisible
  unit — say so in its header. That is the exception, and it is stated, not
  assumed.

## Bookkeeping

- New, moved, or deleted files update `doc/revision_file_map.json` in the same
  gesture; CI's filemap check fails on any mismatch, in both directions. (The
  map is an agent context-economy device; staleness destroys its credibility.)
- Map desc lines are written for retrieval, not documentation: keyword-bearing
  (key types, subsystem vocabulary, R-numbers), ≤140 characters.
- **Every column in `schema.sql` carries a note**, and every object sits under a
  `-- ##` architecture group; `cargo xtask schema` extracts them into
  `doc/revision_schema.json` and CI fails on a gap. (SQLite has no `COMMENT ON`,
  so the DDL is the only place a note can sit beside its column.)
- Generated documents (`revision_schema.json`, and the file map's checks) are
  committed *and* verified current in CI — a generated artifact nobody checks is
  just a stale one.

## Interaction

- Where our requirements are silent on how something should *behave*, the answer
  comes from the **Macintosh Human Interface Guidelines (1992)** — cite the chapter,
  do not recall it. `doc/revision_hig_inventory.md` holds the extracted rules,
  the departures we make deliberately, and the constructs it predates (R-939, R-940).
  (It is the last complete specification of interaction behaviour anyone wrote, and
  it is right on nearly every question it can still be asked.)
- **A control must never misreport itself.** Intrinsic state supplies the base;
  interaction state only modulates it. A pressed control tracks the pointer, and
  attention state belongs to one place at a time.

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
  rev-testkit and are phase-independent (spectra in dB, meters). UI screenshot
  references live in `testdata/ui/` as PNG and are compared bit-identically —
  which is what CPU rasterization and a bundled font buy. Tests force
  system-font fallback **off**; runtime leaves it on.
- **Golden masters are keyed to a toolchain.** A rasterizer or shaper bump may
  legitimately shift output; regenerating references is a deliberate, reviewed act,
  never a reflex when CI goes red.
- Shared test support: rev-testkit, consumed via dev-dependencies only. Repo
  automation: rev-xtask, via `cargo xtask` (tmon, filemap, perf).
