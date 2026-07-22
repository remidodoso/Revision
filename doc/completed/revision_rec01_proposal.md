# rec-01 proposal — the capture path: MIDI in, journaled as it is played

**Status: approved, implemented, and accepted — rec-01 complete 2026-07-21**
(8 decisions). Proven live: `rev-rec` runs the record→replay round trip headless
against a real device and an Oxygen Pro Mini (11 notes captured and replayed).
Two deviations from the decisions as written, both recorded here because the code
does not:

- **Decision #1 (shared origin), realized at the seam, not app-main.** Rather
  than app-main minting the `Instant` and passing it into the session
  constructor, `session_with_thru` mints it and seeds *both* ends — the engine
  reads it from its `RtPort`, the app reads it back via `EngineSession::origin()`
  and hands it to `Keys`. Same one-origin guarantee, and it dodges the ordering
  concern §3 used to reject "engine exposes its origin": the origin exists at the
  seam *before* either the engine or the fork, so nothing is downstream of the
  engine. Cleaner than the proposed shape; the intent is unchanged.
- **Held notes at disarm** are dropped and counted (as §5 anticipated), rather
  than finalized to a stop point — the honest v0.

**Original status: proposed 2026-07-21.** Checkpoint per getstarted rule 2: one
public API change across the engine↔app seam (a shared clock origin), and the
shape of the capture subsystem that rec-02 (two tracks + kill-mid-take) and
rec-03 (the 16-ET trick) build on. No new dependency, no schema change, no new
command.

Aimed at one milestone: **arm a track, play the keyboard while the transport
runs, and have the notes land in the journal where they were played** — so that
replaying the project sounds back what was performed, and a `kill -9` mid-take
loses nothing that was already committed (R-807, R-808, R-810).

---

## 1. What already exists (so this is a small delta)

rec-01 is mostly *wiring parts that were built for it*. Before proposing new
code, the inventory:

- **`Command::RecordBatch { track_id, event }`** — already in `rev-core`, already
  executed by `store::exec::material::record_batch`, which writes the events as
  ordinary direct events on the track and returns `RemoveEvent` as its inverse.
  Capture is legible in the journal and undoable, today.
- **`Correlation`** (`rev-midi`) — the rolling least-squares fit of
  `(sample, nanos)` → sample position. Built and unit-tested in midi-01,
  *exercised* here for the first time (midi-01 §4's explicit promise).
- **`TempoMap`** (`rev-sched`) — integer-exact `sample → tick` via `tick_at`.
- **`Captured { message, nanos }`** on the event ring, already drained each frame
  by `Keys::drain` (today it feeds only the log).
- **`Position`** already publishes the correlation pair (`correlate_at`,
  `correlate_nanos`) every block over the seqlock.
- **`rev-app` already depends on** `rev-store`, `rev-sched`, `rev-midi`,
  `rev-engine`, `rev-log` — so the recorder needs **no new dependency**.

What is missing is: the two clock origins are not the same `Instant` yet; there
is no object that pairs note-ons with note-offs and places them on the tick grid;
and nothing issues `RecordBatch`. That is the whole of rec-01.

---

## 2. Scope

**rec-01 implements.**

- **One shared clock origin** across engine and the input fork (§3) — the
  midi-02 deferral now paid off, because a timestamp finally decides something.
- **The `Recorder`** (a new app-side module): arm/disarm, record mode
  (replace/overdub), note-on/off pairing into placed notes, and journaling them
  as `RecordBatch` **incrementally, as the take runs** (§4, §5).
- **A headless demo binary, `rev-rec`** (§8): open a device, open/create a
  project, run the transport, arm a track, play, stop — then replay, proving the
  round trip without any Control Bar pixels.
- **Tests** that prove placement, pairing, mode, and the durability granularity
  without hardware (§7).

**Deferred to rec-02.** The two-track demonstration and the actual `kill -9`
mid-take TMON test as an executable gate (the durability *design* is rec-01's, §5;
the *kill test* is rec-02's, matching how midi-01 designed correlation and midi-03
exercised it).

**Deferred to ui-04.** The record-arm button, the record light's three states,
and the Control Bar transport driving the engine. rec-01 deliberately does **not**
wait on ui-04: capture is a headless mechanism, demonstrated by `rev-rec`, exactly
as midi-01/02 were proven before any transport widget existed. The arm state
rec-01 builds is what ui-04's button will toggle.

**Deferred / stretch.** Loop-record with keep/discard (poc stage 5 "if cheap") —
the recorder assumes a monotonic, non-looping transport for v0 (§6). Region/punch
replace — v0 replace clears the whole target track (§4.3).

**Out entirely.** CC/pitch-bend capture (only note-on/off exist on the wire so
far); quantization (capture is faithful; quantize is an editor, R-1405); MIDI
output.

---

## 3. The one seam change — a single shared clock origin

Today the fork stamps `Captured.nanos` from `Keys`' own `Instant::now()`
([app/src/midi.rs](../src/rust/app/src/midi.rs)), while the engine stamps
`correlate_nanos` from a *different* private `Instant` created inside
`Engine::new`. Two origins → the correlation would map a fork timestamp onto the
wrong sample. Live play never noticed because it never used the timestamp;
recording does, so this must close.

**Proposal: the app mints the origin and hands it to both.** The composition root
already owns the wiring; let it own the clock zero. A single
`Instant` is created once in `rev-app`, passed into the engine session
constructor (which stops calling `Instant::now()` itself) and into `Keys::new`
(which stops calling its own). Both the seqlock's `correlate_nanos` and the
fork's `Captured.nanos` are then measured from the *same* zero by construction —
the Fork's own doc already says "the app wires them from one shared value"; this
makes that true.

This is the checkpoint: the engine session constructor
(`session_with_thru`) gains an `origin: Instant` parameter (or a sibling
constructor that takes one). Small, mechanical, and the only cross-crate surface
rec-01 touches.

*Alternative considered:* expose the engine's origin (`session.origin()`) and
have `Keys` read it. Rejected — it makes the engine the clock authority and
forces an ordering (engine must exist before the fork can be stamped correctly),
whereas app-mints-origin has no ordering constraint and reads as what it is.

---

## 4. The `Recorder` — from a stream of `Captured` to placed notes

A new app-side module, `app/src/record.rs`. It is pure app-thread code (allocation
allowed) and holds:

- **arm state** and **mode** (`Replace` | `Overdub`), plus the **target track**;
- a **`Correlation`**, fed each frame from `Position`'s pair;
- an **open-notes map** `(channel, note) → (start_tick, velocity)` for pairing;
- a handle to the **`Project`** to journal into.

### 4.1 Placing a note

Each frame, after `Audio::pump`, the app:

1. reads `Position`, and if running feeds `Correlation::observe((at, nanos))`;
2. drains `Keys` (`Captured` events) into the recorder.

For each `Captured` while armed and running:

- **note-on:** map `nanos → sample` (`Correlation::sample_at`) → **play-position**
  sample (§6) → **tick** (`TempoMap::tick_at`); remember `(start_tick, velocity)`
  in the open-notes map.
- **note-off** (or note-on velocity 0): look up the matching on; if found, form
  `EventSpec::note(start_tick, dur_tick, note_number, velocity)` where
  `dur_tick = off_tick − start_tick` (floored to a minimum of one tick so a
  zero-length note cannot exist), and **stage it** for this frame's flush.

An unmatched note-off is ignored (the same tolerance the live pairing already
has). A note still held when recording stops is either finalized at Stop with its
duration up to the stop point, or dropped — see §5.

### 4.2 Overdub

Overdub simply appends: the staged notes are journaled as `RecordBatch` on top of
whatever the track already holds. Nothing is removed.

### 4.3 Replace

Replace must clear what it overwrites. v0 narrowing: **at record start, replace
clears the entire target track's direct events** in one gesture (a `RemoveEvent`
of the track's current event ids), *before* the first captured note is journaled.
It is undoable (RemoveEvent's inverse restores them) and legible. Region/punch
replace — clearing only the recorded tick span — is deferred to when punch exists
(ui-04+); the whole-track rule is the honest v0 and states its own limitation.

---

## 5. Durability granularity — the crux, and rec-02's whole premise

rec-02's headline is *"a `kill -9` mid-take loses nothing committed."* That is a
**design decision made here**, not a property that falls out for free: it dictates
*when* capture journals.

**If a take buffered its notes and committed at Stop, a kill mid-take would lose
the entire take.** So capture must journal **incrementally**. The proposal:

> **Flush once per UI frame:** every note that *completed* (got its note-off)
> during that frame is journaled as one `RecordBatch` gesture at the end of the
> frame. One gesture per frame, containing that frame's finished notes.

Consequences, stated honestly:

- A committed note is durable the instant its frame's gesture commits (the store
  is crash-only; R-202/R-808). A kill loses **at most** the notes still being
  physically held (no note-off yet, so no known duration) plus any events from a
  frame whose transaction had not yet committed. **Nothing finished is lost** —
  which is exactly the claim rec-02 will test.
- Held-at-kill notes are genuinely lost. They were never finished; recording an
  onset with no duration would mean journaling a note the model cannot represent
  (durations are bounded, R-402a). This is the correct behavior, not a gap.
- One gesture per frame keeps the journal readable (a take is a run of
  `record_batch` gestures, timestamped) and keeps each commit tiny, so the write
  never stalls the frame.

*Alternative considered:* one gesture per note (flush on each note-off). Same
durability, more transactions; per-frame batching is the same guarantee at lower
cost, and a frame is short enough (a few ms) that it is a rounding error against
"lose nothing." Recommended: **per-frame flush.**

---

## 6. The transport relationship — session clock vs play position

The correlation yields a **session-clock** sample (the `Position.at` domain, which
advances every block, always). Notes must be placed at **play-position** ticks
(the `Position.play` domain, which advances only while running) so the tempo map
converts them correctly.

While the transport runs forward without looping, the two advance 1:1, so the
offset `session − play` is a constant set at the last Start/Locate. The recorder
reads it from any running-frame `Position` (`at − play`) and computes
`play_sample = session_sample − offset`, then `tick = TempoMap::tick_at(play_sample)`.

**v0 assumes a monotonic, non-looping transport.** A loop wrap or a Locate during
a take would break the constant-offset assumption; loop-record (keep/discard) is
the deferred stretch (poc stage 5), and until it exists the recorder records
against a straight playhead. This limitation is named, not hidden.

---

## 7. Tests (what rec-01 can prove without a stage)

- **Placement:** a synthetic `Correlation` with a known slope + a constant
  `TempoMap`; a `Captured` note-on at a known `nanos` lands at the expected tick
  within tolerance. (The two conversions are already unit-tested in isolation;
  this proves them composed.)
- **Pairing → duration:** note-on then note-off on the same `(channel, note)`
  produces one `EventSpec` with the right `dur_tick`; velocity-0 counts as off; an
  unmatched off is ignored; a held note at Stop is finalized-or-dropped per §5.
- **Overdub vs replace:** overdub appends (existing events survive); replace
  clears the track first (existing events gone, restored by undo).
- **Durability granularity (unit, not the kill test):** after N completed notes,
  the journal already contains N notes across ≥1 `record_batch` gesture *before*
  Stop — proving capture commits as it goes, not at Stop. (The actual `kill -9`
  process test is rec-02.)
- **Shared origin:** the engine session and the fork, constructed from one
  `Instant`, produce `correlate_nanos`/`Captured.nanos` on one monotonic scale
  (a captured event's nanos falls within the published correlation window).
- **Round trip (in `rev-rec`, exercised by hand + a headless offline check):** a
  scripted `Captured` stream recorded into a fresh project, reopened, replays the
  same notes (`v_realized` shows them at the recorded ticks).

---

## 8. The demo — `rev-rec`

A headless binary in the family of `rev-tone`/`rev-mhall`/`rev-roll`: it opens a
device (or degrades to silent), creates or opens a project with one root phrase
and one track, starts the transport, arms the track, and records live MIDI until
stopped — then locates to zero and replays. Flags in the established style
(`--device`, `--tuning`, `--bpm`, `--seconds`, `--overdub`, `--project FILE`).
It is the operable proof the goldens cannot make, and the seed of rec-02's
two-track script and rec-03's tuning swap.

`rev-rec` does **not** draw a Control Bar. It is the mechanism; ui-04 is the face.

---

## 9. Decisions to approve

1. **The app mints one clock origin** and passes it to both the engine session
   constructor and `Keys`/`Fork`, so `correlate_nanos` and `Captured.nanos` share
   a zero. (The one cross-crate API change; alternative — engine exposes its
   origin — rejected in §3.)
2. **rec-01 is headless**, demonstrated by a new `rev-rec` binary, and does **not**
   wait on ui-04. The record button/light are ui-04's; the arm *state* is here.
3. **The `Recorder` lives app-side** (`app/src/record.rs`): arm + mode, note
   pairing into placed `EventSpec`s, journaling via the existing `RecordBatch`.
   No new dependency, no schema change, no new command.
4. **Capture journals incrementally — one `RecordBatch` gesture per UI frame** of
   completed notes — so a kill mid-take loses only unfinished (still-held) notes,
   never a committed one. This is the design rec-02's kill-test will verify.
5. **Notes are placed** by `nanos → sample` (existing `Correlation`) → play
   position → `tick` (existing `TempoMap`); duration is off-tick minus on-tick,
   floored to one tick.
6. **Replace clears the whole target track** at record start (one undoable
   `RemoveEvent` gesture); **overdub appends.** Region/punch replace is deferred.
7. **v0 records against a monotonic, non-looping transport.** Loop-record with
   keep/discard is the deferred stretch (poc stage 5).
8. **rec-02 owns** the two-track demonstration and the executable `kill -9`
   TMON gate; **rec-03 owns** the 16-ET tuning swap. rec-01 builds the mechanism
   all three ride.
