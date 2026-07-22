# rec-02 proposal — two tracks, and the promise that nothing is lost

**Status: approved, implemented, and accepted — rec-02 complete 2026-07-21.**
Accepted at the keyboard: `rev-studio`'s arm → record → watch-it-appear → replay
flow works, and `cargo xtask tmon` proves the kill survives. Two deviations from
the decisions as written, recorded here because the code does not:

- **Decision #4 (two-track), moved to the harness.** The second overdub pass was
  proposed for headless `rev-rec`, but overdub-against-playback is awkward with no
  one to play along; it moved into `rev-studio` (interactive, its natural home)
  plus a deterministic headless test (`two_tracks_record_and_replay_together`).
  `rev-rec` stays single-track.
- **The tmon child records a *tuned* take** (12-ET), so its notes resolve to
  pitches and are viewable on the roll — which is what proved Tier A end to end.
  The durability property itself is tuning-agnostic.

**Original status: proposed 2026-07-21.** Checkpoint per getstarted rule 2: a
**testing-harness architecture** (the `kill -9` durability gate) — the reserved
`xtask tmon` slot, filled. No schema change, no new command, no external
dependency; `xtask` gains the internal `rev-core` path dependency so it can issue
the recording command it kills a process in the middle of.

rec-02 is the **PoC completion test**. It proves the one claim the whole
store-primary design exists to make — *a `kill -9` at any moment loses no
committed gesture* (R-808/R-202/R-1504) — and it records and replays **two
tracks**, so recording is more than a single monophonic pass. The reframing
agreed in discussion holds: the durability gate is the visual-independent core;
"see it" is the read-only roll doing honest work (Tier A, §4); and — added in
discussion — a **throwaway windowed harness** mashes the transport controls onto
the live roll so recording is *operable* and sensible to a person (Tier B, §4-bis),
prototyping a slice of ui-04 without being it. Full multi-lane legibility remains
a **separate, not-yet-planned arrangement view**, not smuggled in here.

---

## 1. What already exists (the delta is small again)

- **WAL durability.** The store opens `journal_mode=WAL` (`project.rs::configure`),
  under which a **committed** transaction survives an application crash: a
  `kill -9` of the process cannot lose an acknowledged commit, and a commit is
  atomic — never torn. rec-02 does not add durability; it **proves** it, and
  guards against a regression where recording buffers instead of committing.
- **The reserved harness slot.** `xtask tmon` already exists as a pre-declared
  command — "the store kill-test" — that today exits nonzero (unimplemented).
  rec-02 implements it. No new layout; an anticipated command lands.
- **Per-frame commits.** rec-01's `Recorder::flush` already journals each frame's
  notes as its own `RecordBatch` gesture — the incremental discipline the gate
  tests. rec-01's unit test proved *reopen*-survival; rec-02 proves *hard-kill*
  survival, which is a stronger, process-level fact.
- **Multi-track compile.** `Compiler::new(tempo, tracks)` already takes a `Vec`
  of tracks (mhall passes one). Two tracks replay by passing both.
- **The roll renders any track.** `Roll::build(&project, cache, track)` draws
  whatever track it is handed — so "see the take" is arg-plumbing, not new
  drawing.

---

## 2. The durability gate — `xtask tmon`

The heart of rec-02. A parent–child process test, run in CI's test job and
runnable by hand (`cargo xtask tmon`):

- **The child** (`xtask tmon --child <project>`) opens a fresh project with one
  track and journals `RecordBatch` gestures in a tight loop — one note per
  gesture, exactly what `Recorder::flush` does — and **prints each committed
  sequence number to stdout** *after* `apply` returns `Ok`. So the child only
  ever claims a commit the store acknowledged.
- **The parent** spawns the child, reads its stdout, lets it commit a few hundred
  notes, then **hard-kills it** with `std::process::Child::kill()` — SIGKILL on
  Unix, `TerminateProcess` on Windows: no unwinding, no destructors, no flush.
  The adversary is a real ungraceful death, not a clean shutdown.
- **On reopen**, the harness asserts:
  1. the project **opens cleanly** — no corruption, no half-written schema;
  2. the journal replays and the track holds **every note the child
     acknowledged** (the highest sequence the parent saw on stdout) — *nothing
     acknowledged is ever lost*;
  3. the count is **at most one more** than that (a gesture may have committed in
     the instant between its commit and the kill, before its line was read) —
     never a torn or partial gesture.

That triple is the whole promise, made executable: **no acknowledged commit
lost, no partial commit survived, reopen always consistent.**

**Scope of the claim, stated honestly.** This proves durability against a
*process* kill, which is what WAL + committed-transaction semantics guarantee.
Power-loss / OS-crash durability is a stronger claim (it turns on `synchronous`
and fsync timing) and is **not** what this tests — naming it so the gate is not
mistaken for more than it is. Process-kill is the right adversary for "the app
crashed mid-take," which is the fear R-808 addresses.

---

## 3. Two tracks — recorded, and heard together

rec-02's second half: record a second track *against* the first, the classic
multitrack overdub. `rev-rec` gains a second pass:

- record onto track 1 as today;
- then **play track 1 back** (compile → `TakeChunk` → `Start`) while **arming
  and recording track 2** — so you overdub against what you already laid down,
  the live thru sounding your new part over the scheduled old one;
- **replay both** by compiling `vec![track1, track2]` into one schedule.

Nothing new is needed below the app: the engine already plays a scheduled chunk
and live thru at once, and the compiler already takes a track list. This proves
the model and the capture path are genuinely multi-track.

**What stays deferred.** Seeing *both* tracks at once — lanes, phrases as blocks
— is the arrangement view, and it is a separate item (§5). Until it exists,
two-track is verified by ear, by the durable journal, and by the roll **one lane
at a time**. That is the honest v0, and it is enough to complete the PoC.

---

## 4. Tier A — see the take (and the survivor) on the roll

`rev-roll` gains `--project FILE [--track N]`: open an existing project and draw
the named track instead of building MHALL. Two payoffs from one flag:

- after a `rev-rec` take (`--project take.revision`), open it and **see the notes
  you played** land where you played them;
- after the durability gate, open the killed-and-reopened project and **see the
  survivor intact** — the durability claim, answered with your eyes.

This is the single-lane visual we already have, earning its keep. It is the
whole of the "Tier A" the discussion folded in.

---

## 4-bis. The operable harness — Control Bar × piano roll (Tier B)

Tier A is a *still photograph*; recording wants to be *operated*. So rec-02 also
builds a **throwaway windowed harness** that mashes the transport controls onto
the live roll — the surface that makes recording sensible to a person: arm, play,
watch notes appear on the roll as you play them, stop, hear it back. **Explicitly
a test harness, not the product Control Bar** — it is a demo binary that borrows
finished parts, kept only as long as it earns its keep.

**Nothing new below the app is required — the parts are built:**

- The widget kit (ui-03) already has a **tri-state `Kind::Record`**
  (`Off`/`Armed`/`Recording`), plus `Button`, `Counter`, `Locator`, and it is
  **clickable**: the kit does hit-testing and reports activation ("the record
  control was operated").
- The roll demo (ui-06, `rev-roll`) is already a full windowed `Host` with a
  painted Kit, live keys, the playhead-follow machine, and space/Home transport.
- The `rev-pane` demo already shows the one wiring the roll demo lacks: route
  `Event::Pointer` to `kit.hit(at)` so widgets are clickable.

**So the harness is composition:**

- a **transport strip** above the roll — Record (tri-state), Play/Stop, and a
  Counter that follows the engine clock — clickable (keys as a bonus);
- **Record arms the `Recorder` and starts the transport**; the record light walks
  `Off → Armed → Recording`; the Counter reads bar·beat from the position;
- **the roll rebuilds each frame** from the recording project (`Roll::build` is
  cheap at PoC note counts), so notes appear *as you play* — the live Tier B;
- **Play** locates to zero, compiles the take, and plays it back, playhead
  following; **Stop** disarms and halts.
- **Two-track lives here**: record track 1, watch it land; arm track 2 and overdub
  while track 1 plays; the roll shows whichever lane is in focus.

**On the doctrine.** This harness deliberately *prototypes* a slice of ui-04
(transport → engine, the record light's states, Counter-follows-clock). That is a
feature, not a violation: it de-risks ui-04 cheaply, and the real Control Bar
slice — the permanent, skinned, laid-out one — remains its own item and will
adopt what the harness proves. The harness is scaffolding with a short life, said
so plainly in its module doc.

---

## 5. Explicitly out of scope — the arrangement view

The multi-lane / graphical-phrases facility that makes multi-track recording
*legible* (lanes = tracks, blocks = phrase-instances, drill in for notes) is
**not** part of rec-02. It is real stage-4 labor, the read-only roll is its
single-lane seed, and it deserves its own discussion and proposal. rec-02
completes the recording *mechanism* and its durability proof; the view that makes
it feel like a multitrack recorder is the next UI conversation, on its own terms.

---

## 6. Tests and what proves it

- **The gate itself** is the test: `cargo xtask tmon` exits nonzero if any of §2's
  three assertions fail. It runs in CI's test job.
- **Two-track**, headless: a test records two synthetic takes onto two tracks and
  asserts `v_realized` (or `query::event_on_track`) shows both, and that a
  compiled schedule over `vec![t1, t2]` contains notes from each.
- **Tier A**, by eye: `rev-rec --project X` then `rev-roll --project X` — the
  user's verification, no UI automation (getstarted stage-4 rule).
- **The harness (Tier B)**, by eye: arm, play the keyboard, watch notes appear on
  the roll live, stop, play back — the user's verification, no UI automation.
- The existing rec-01 suite continues to pass unchanged.

---

## 7. Decisions to approve

1. **The durability gate is `xtask tmon`** (the reserved slot): a child journals
   `RecordBatch` gestures and prints each acknowledged commit; the parent
   hard-kills it with `Child::kill()` mid-loop; reopen must be clean, lose no
   acknowledged commit, and hold no partial one. Runs in CI, runnable by hand.
2. The gate proves **process-kill** durability (WAL's guarantee), **not**
   power-loss durability — named, not conflated.
3. **`xtask` gains the internal `rev-core` dependency** so it can build
   `RecordBatch`. No external dependency, no schema change, no new command.
4. **Two-track via a second overdub pass in `rev-rec`**: record track 1, play it
   back while recording track 2, replay both (`vec![t1, t2]`). Verified by ear,
   journal, and the single-lane roll.
5. **Tier A is `rev-roll --project FILE [--track N]`** — see a take, and see the
   kill survivor, on the read-only roll.
6. **A throwaway windowed harness (Tier B)** mashes the transport strip (Record
   tri-state, Play/Stop, Counter) onto the live roll: arm, play, watch notes
   appear, stop, play back — the surface that makes recording sensible. It reuses
   the finished widget kit and roll, and **prototypes a slice of ui-04** without
   being it (the real Control Bar slice stays its own item and adopts what this
   proves). Verified by eye.
7. **The arrangement / multi-lane view is out of scope** and becomes its own
   future item; two-track legibility waits for it.
