# Revision — Getting Started (agent handoff)

**Read first, in order:** `revision.md` (rationale and history), `revision_requirements_v1.md`
(normative requirements; R-numbers cited below are defined there), `revision_poc.md` (the
plan this file operationalizes), `revision_coding_standard.md` (how the code is written).
This file tells you *how to work*; those tell you *what to build and why*.

**Repo note:** Revision gets a **new repository** (imminent), hosted **publicly on
GitHub**. These documents migrate there at bootstrap. Until then they live in the
Notorolla repo as guests — and Notorolla itself is strictly off-limits (see ground
rule 3). Public-hosting particulars: the **license is decided (2026-07-19): dual
MIT OR Apache-2.0** (Rust convention; see revision_coding_standard.md "Dependencies &
licensing" — GPL-free tree; the R-1203 VST3 bridge may be GPLv3 in out-of-process
isolation; ASIO/MTS-ESP SDKs are never vendored);
GitHub **Pages** serves `revision_plan.html` as a live public dashboard (add a
`.nojekyll` file so Pages serves the `.md` files verbatim — Jekyll processing would
break the viewer's requirement rollovers).

## Ground rules

1. **Discussion precedes implementation.** The user gates work with the phrase
   "make it so." Do not implement without it.
2. **Checkpoint protocol.** The following always require a concrete written proposal and
   explicit human approval *before* implementation:
   - table schemas (DDL) and any later migration;
   - names and responsibilities of large subsystems (crates, major modules);
   - code organization and directory layout;
   - testing framework choices and harness architecture;
   - every new external dependency;
   - public API shapes between crates (engine↔app vocabulary, mechanism contract);
   - file formats;
   - anything expensive to rename or reverse later.
   Within an approved design, routine implementation detail proceeds without
   re-approval. When in doubt, ask — a wasted question is cheaper than a wasted
   subsystem.
3. **Orthogonality is absolute:** never modify Notorolla code or documents. Read-only
   visits (e.g., the Padlington inventory) are fine.
4. **Commits are the user's, alone, always.** Never commit, never suggest you will.
5. **Report honestly:** failing tests are reported as failing; skipped steps are
   reported as skipped; no green-washing.

## Conventions

Coding conventions live in **`revision_coding_standard.md`**. Its default is ordinary
Rust convention for a project expected to grow large; the document records only
deviations and decisions (naming, layout, file size, comments, test placement).

## The work cycle and the plan document

All work — major architectural projects, minor odd jobs, and everything between —
follows one cycle, tracked in **`revision_plan.json`** (rendered by
**`revision_plan.html`**; serve the directory with `python -m http.server` and open
`/revision_plan.html`):

1. **Describe/plan** — the item's `plan` field is written and discussed; status
   `described`.
2. **Execute** — status `executing`. (No item enters `executing` with an empty `plan`.)
3. **Verify** — status `verifying`; the `verify` field records *how* it was verified.
   (No item reaches `complete` with an empty `verify`.)
4. **Complete** — status `complete`, `completed` timestamp set.

Keep the plan current as changes happen: statuses, notes (timestamped), the top-level
`now` field (a few sentences: what's in flight, what's blocked, what changed — the
"what's going on at the moment" register). **Roll-off discipline from birth:** entries
are timestamped; on a periodic pass, `complete` items whose details no longer inform
current work move to `revision_plan_archive.json` (their calendar history is preserved
there and the viewer's History section still shows them). Prose that outgrows an item
graduates to `revision.md`.

## Work plan (stages; details in revision_poc.md)

- **Stage 0 — Bootstrap:** repo creation; [checkpoint] workspace layout, toolchain/CI
  skeleton (fmt, clippy, test, Mac/Linux cross-compile *checks*); [checkpoint] testing
  stack (unit, property, golden-master harness, TMON kill-test scripting).
- **Stage 1 — `core` + `store`:** [checkpoint] concrete DDL (from the revision_poc
  schema sketch), command vocabulary, journal payload format → model types → store
  open/create/journal/replay → realization view with fixture tests → JSON round-trip
  test (R-203/204) → TMON v0 kill-and-reopen (R-202).
- **Stage 2 — `engine`:** [checkpoint] engine↔app interface (command ring, schedule
  chunks, telemetry); [checkpoint] graph-runtime node API → cpal duplex bring-up →
  AudioParam automation module (W3C spec math, unit-tested) → nodes → sine voice →
  schedule compiler reading `realized` → **MHALL headless** → allocation-guard
  verification → offline render path.
- **Stage 3 — `midi`:** [checkpoint] API + clock-correlation approach → enumeration and
  driver-boundary timestamps → thru fast path → honest latency printed (R-307 v0).
  Stretch: one knob → voice macro.
- **Stage 4 — `ui`:** [checkpoint] mechanism contract as API + widget-kit API shape →
  window/surface/text bring-up → widgets per the Control Bar census → slice wired to
  the engine. **UI verification is by the user's eyeball** — no UI automation
  frameworks; provide runnable builds and screenshots.
- **Stage 5 — Record/replay:** capture → journal (`record_batch`), arm/replace/overdub,
  replay; kill-mid-take test (R-808); Padlington port certified (may run in parallel
  from stage 2 exit); **16-ET party trick** demonstrated. PoC complete.

## Testing doctrine

Pure modules get unit tests; DSP gets phase-independent golden masters (magnitude
spectra + meters); determinism gates everywhere (render twice → bit-identical); the
realization view is the oracle for any optimized twin; all tests green before any
checkpoint review is requested.

## Gotchas carried over (Notorolla scar tissue; learn free)

- **EOL-aware file rewriting.** Repos may be CRLF (`core.autocrlf`). Any tool that
  rewrites files must detect and preserve line endings or it produces a giant noisy
  diff. (Cost real time once; never again.)
- **Locate code by name, never by line number.** Every edit shifts lines; names are
  authoritative; re-search each time.
- **ripgrep, not grep,** for anything with lookarounds — GNU grep `-E` silently
  ignores lookbehinds and returns bogus zero-counts that can trick you into deleting
  live code.
- **A level lives in one place.** Scaling a source by `peak` *and* ramping its bus to
  `peak` = peak² (~−20 dB). Amp envelopes ramp to 1; the level has one home. Re-meter
  after any voicing change.
- **A "timbre" control that changes summed energy is a loudness control in disguise.**
  Energy-normalize (now law: R-713).
- **Offline render always uses the full voice** — never wire a live CPU-relief flag
  into the offline path (now law: R-715). Non-finite math kills offline rendering
  silently; floor and guard.
- **Activation ≠ focus; act on press.** A control whose enabled state depends on
  selection can have its click silently swallowed by an ancestor's pointer-down
  handler. Now mechanism-contract physics, but recognize the symptom: button enables
  visibly, click does nothing, no error.
