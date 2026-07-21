# eng-06 proposal — the schedule compiler

**Status: proposed 2026-07-21.** Checkpoint per getstarted rule 2: a new crate, an
internal format that four later items cross, and the public API between the store
and the engine.

This is where eng-01's deferred debt comes due. §8 of that proposal settled the
*envelope* of a compiled chunk — app-allocated, immutable, returned over the
garbage ring — and deferred the **payload** on the grounds that it is the
compiler's contract with `v_realized`, which did not yet exist to contract with.
It does now.

It is also where **R-312 becomes literal**. "The compiler is the last place music
exists" is a sentence about this crate: note numbers, ticks, tunings and tempo go
in; samples, frequencies and durations come out. Everything below is physics
because everything musical was resolved here.

---

## 1. Scope

**In.** The `rev-sched` crate (§3); tick→sample conversion and the tempo map
(§4); the chunk payload (§5); the look-ahead and refill policy (§6); what happens
when material is edited mid-playback (§7); frequency resolution and the tuning
cache (§8); determinism (§9); testing (§10).

**Out — deferred, with the shape not foreclosed.** Polytempo (R-416) — one tempo
stream in v0, N in the structure. Nested instances — flat realization only, per
the core-03 finding, and the compiler does not care which it is given. MIDI
output. Automation (eng-04). Audio events (R-308).

**Out entirely.** Meter — there is none (R-419), and the compiler never needed
one. Bars. Notation.

---

## 2. The problem in one paragraph

`v_realized` gives rows in ticks: `at_tick`, `dur_tick`, `note_number`,
`velocity`, `tuning_id`. The engine wants events in samples, at frequencies,
delivered ahead of the moment they sound, in blocks small enough that an edit is
audible quickly and large enough that the app thread is not compiling
continuously. Between those two sentences sit four things that are easy to get
quietly wrong: the arithmetic, the ownership, the loop, and the edit.

---

## 3. `rev-sched`, a new crate

```
rev-sched  ←  rev-core, rev-store, rev-engine
```

**Why not `rev-app`.** eng-07 is *MHALL headless* — first musical sound with no
UI. A compiler living in the windowed application cannot be tested without the
windowed application, which drags winit into a headless test. The compiler is
also the piece most in need of exhaustive testing, and the least in need of a
window.

**Why not `rev-store`.** The store's business is persistence and the journal. A
compiler that knows about sample rates does not belong under the same roof as the
schema.

**Why not `rev-engine`.** Because R-312 says so: an engine that speaks only
physics cannot import a model of music. That rule is the whole architecture here,
and this is the first opportunity to break it.

So `rev-sched` sits above all three and is depended on by `rev-app` and by
eng-07's headless binary. No new external dependency.

---

## 4. Tick to sample: integer-exact, monotonic, segment-wise

The model stores tempo as **integer microseconds per quarter** (MIDI-exact,
chosen at core-01 precisely so the model never accumulates float drift). `rev-core`
already has `tick_to_second`, which returns `f64`. **The compiler should not use
it.**

```rust
/// sample = tick × usec_per_quarter × sample_rate ÷ (PPQ × 1_000_000)
```

Done in `i128` this is exact until absurd durations; done in `f64` it is
approximate, and the approximation is *cumulative* across a tempo map. Three
properties matter and all three come from staying in integers:

- **Monotonic.** Two ticks in order must map to samples in order, always. A
  rounding scheme that can invert two adjacent events produces a schedule that
  plays notes out of order — rare, unreproducible, and horrible to diagnose.
- **Deterministic across platforms** (R-1503), which `f64` division is not
  guaranteed to be under different optimization settings.
- **Non-accumulating.** The tempo map is piecewise constant, so a tick's sample
  position is the sum of whole segments before it plus a partial. Each segment
  boundary is computed **once and anchored** — the next segment starts from the
  stored sample position of its start, not from a re-derivation. Error cannot
  compound because nothing downstream re-adds.

```rust
/// A tempo map, resolved to sample positions once.
pub struct TempoMap {
    /// Ascending, each carrying the sample position of its own start.
    segment: Vec<Segment>,   // { from_tick, from_sample, usec_per_quarter }
    sample_rate: u32,
}

impl TempoMap {
    pub fn sample_at(&self, tick: Tick) -> SampleTime;
    pub fn tick_at(&self, at: SampleTime) -> Tick;   // for the counter and the roll
}
```

`tick_at` is the inverse and is needed by the *display* side — the counter
converts the engine's position back into beats, and today `rev-app` does that with
its own `BPM` constant. That constant should die here.

**Rounding: round-half-to-even at each conversion**, stated so it is not
rediscovered. Half-up biases every event a fraction late; banker's rounding does
not bias.

---

## 5. The chunk payload

The debt from eng-01 §8, paid.

```rust
/// One compiled note. Self-contained: everything the engine needs, nothing it
/// must look up.
#[derive(Clone, Copy)]
pub struct Note {
    /// When it starts, in **play-position** samples (§6.3).
    pub at: SampleTime,
    /// How long it sounds, in samples. **Bounded, always** — there is no
    /// sentinel for "forever" and no case that needs one (§5.2). At 48 kHz a
    /// `u32` reaches about 24 hours, so a long note is merely long.
    ///
    /// Carried with the note rather than as a separate note-off event. This is
    /// not an engine convenience: the model already says a note is an entity
    /// with a duration (`event.dur_tick`, R-001), and paired on/off is a wire
    /// encoding, not a concept (§5.1).
    pub dur: u32,
    /// Frequency in hertz, resolved through the tuning at compile time (R-312).
    pub hz: f32,
    /// 0..1, from the model's 16-bit velocity (R-402).
    pub level: f32,
    /// Which track it came from — the engine's routing key, opaque to it.
    pub voice: u16,
    /// Padding to a power of two, reserved. Named rather than anonymous so a
    /// later field does not change the struct's size.
    pub reserved: u16,
}

pub struct Chunk {
    pub from: SampleTime,
    pub to: SampleTime,
    /// Ascending by `at`. The engine dispatches by scanning forward, so sorted
    /// order is a precondition, not a convenience.
    pub note: Vec<Note>,
}
```

Three decisions embedded there, each stated because each has an alternative:

**Notes carry duration, not paired on/off events.** See §5.1 for why this is a
modelling claim rather than an optimization, and §7.2 for what it buys.

**Frequency, not note number.** R-312. The engine cannot mis-tune what it was
never told.

**A `Vec` inside the chunk, allocated app-side.** The engine reads it and never
frees it (eng-01 §4.4). `Chunk` already existed as an envelope with `from`/`to`;
this fills it in without changing the ownership discipline at all — which is the
evidence that deferring the payload was the right call.

### 5.1 Why duration, grounded

The model already says a note is an entity with a duration: `event.dur_tick`
exists, and R-001 makes the note the primary unit. So the question is not whether
to adopt that view in the compiler — it is whether to **abandon** it partway and
rebuild it later. Paired events would mean entity → two fragments → reconstructed
voice, with a stage in the middle where a note does not exist as a thing, and
where every bug is about fragments that lost their partner.

**On/off is a wire encoding, not a concept.** MIDI decomposes notes because a
31,250-baud serial cable had to send something when a key went down and something
else when it came up. That is a property of a transport, not a claim about music
— and the evidence is that the first thing every MIDI file reader does is pair the
events back into rectangles. Everything else that represents notes represents them
as entities: Music-N and Csound score lines (start, duration, pitch, amplitude),
notation and MusicXML, every piano roll ever built, and our own model. The wire
format stands alone, for a bandwidth reason we no longer have.

The principle, stated once:

> **Pairs at the edges, entities in the middle.**

Note that the *input* side already works this way and must: R-810's capture
receives driver events and builds an entity from them. Entity → pair at the MIDI
output boundary is that conversion's exact mirror. Both sit at an edge where a
transport demands them; nothing internal ever handles fragments.

Two things this buys the eventual MIDI output stage specifically. The off is
**queued, not reconciled** — duration is known when the note is emitted, so both
wire events are scheduled together and there is no table of outstanding
obligations to get wrong. And **overlaps become visible**: MIDI cannot express two
simultaneous middle-Cs on one channel, and with entities the output stage can see
the collision before emitting and decide (rotate channels, merge, report) instead
of discovering it as a truncated note.

**Honest limit.** Live input has no duration — a key press is not yet a note, it
becomes one on release — so the engine will need voice tracking and all-notes-off
regardless, once MIDI input exists. Duration-carrying does not remove that
machinery; it means compiled material does not use it, which is where determinism
(R-1402) has to hold.

**The competitive alternative, fairly stated**, is paired events carrying a stable
identity so that a supersede or a boundary crossing can diff old against new. It
wins on uniformity (one mechanism for live and compiled), on release velocity, and
on MIDI output needing pairs anyway. It costs a voice table maintained on the
real-time thread, correct under supersede, boundary crossing, locate, and loop
wrap — four interacting cases in the one place that may not allocate and must
always return, whose failure mode is a stuck note. The trade is one mechanism plus
real-time state against two mechanisms with no shared state; this proposal takes
the second.

### 5.2 There are no unbounded notes

A drone, a held pad, a modular patch that simply runs — none of these are notes.
**A continuously sounding source is instrument state, not an event**, expressed by
parameter automation (eng-04) and never by a note without an end.

That is not a restriction; it is what a patch actually is. In a modular rig the
oscillator is always running and the gate shapes it: "always on" is the patch, and
the notes are the gates.

Three payoffs, none of them cosmetic:

- **The voice pool's invariant stays total.** Every pooled voice eventually
  releases and returns to the free list — no exceptions, no voice that can never
  be reclaimed and quietly shrinks the pool. An always-on source is allocated at
  instrument instantiation and never enters the pool.
- **A stuck note becomes a diagnosable bug rather than a legitimate state.** A
  pooled voice sounding with no scheduled end is, by construction, an error — and
  therefore something the observation log can assert on. An invariant with legal
  exceptions cannot be checked.
- **No sentinel anywhere**: not in the compiler, the roll's rectangle math, the
  MIDI output stage, or the offline renderer's "run until the last voice
  releases". Each of those would have had to know about it.

An extremely long note remains perfectly legal and is merely awkward to draw —
a rendering problem, governed by R-945, not a model problem.

---

## 6. Look-ahead, refill, and the loop

**6.1 Window.** Chunks cover **500 ms** of play position, refilled when the
engine's reported position passes the halfway mark of the current one. Both
numbers are guesses and are stated as guesses; the observation log will make them
measurable, and eng-03's callback timing gives the units.

**6.2 Refill needs no new channel.** The position snapshot already carries `play`.
The app compares it against the current chunk's `to` and compiles the next one
when it must. Nothing is added to the seam.

**6.3 Chunks are stamped in play-position space, not session-clock space.**

This is the decision that makes looping free. The engine already wraps `play`
inside a loop (eng-03), so one tick maps to *many* session-clock samples. Stamping
in session time would force recompilation on every lap; stamping in play position
means the same chunk serves every lap and the engine resolves the mapping it
already computes.

A block that straddles a loop point covers two play ranges, so the engine's
dispatch scan runs twice. That is a small complication in the engine and it
removes an entire class of work from the compiler.

---

## 7. Editing while it plays

R-1508: *a committed edit sounds at the edited material's next occurrence*.

**7.1 Recompile and supersede.** On a committed gesture, the app recompiles from
`play + margin` and sends `TakeChunk`. The engine installs it and returns the old
one over the garbage ring — machinery that exists and is tested. The margin exists
because the currently-playing block cannot be changed retroactively; one chunk
refill period is the obvious value.

**7.2 Why duration-carrying notes matter here.** (Grounding in §5.1.)

With paired on/off events, superseding a schedule orphans note-offs: a note
started from the old chunk has its off event in a list that no longer exists, and
it sounds forever. Every scheduler that works this way needs a reconciliation
mechanism — track sounding voices, diff them against the new schedule, synthesize
the missing offs — and getting it wrong produces the classic stuck note.

With duration carried on the note, **a voice knows when to stop at the moment it
starts.** Superseding a chunk cannot orphan anything, because nothing was waiting
in it. The reconciliation mechanism is not written, not tested, and not able to be
wrong.

The cost is stated plainly: a note whose duration is edited *while it is
sounding* keeps its old duration until its next occurrence. That is exactly what
R-1508 promises, so the cost is zero against the requirement.

**7.3 Retrigger avoidance.** The new chunk starts at `play + margin`, so notes
already sounding are not in it and are not restarted. Notes whose onset falls in
the margin are lost — which is why the margin should be small, and why it is a
measured number rather than a comfortable one.

---

## 8. Frequency resolution and the tuning cache

Each `v_realized` row carries a `tuning_id`. The compiler resolves
`note_number → hz` through that tuning's materialized table, which `rev-store`
already produces (`query::materialized_tuning`).

**Materialize once per tuning, cache by id.** MHALL is one tuning; a mixed-tuning
arrangement (R-418) is a handful. The cache is invalidated when a gesture touches
a tuning — the store already has hooks registered for exactly this kind of
invalidation.

**This is the same resolution the live path performs** (eng-01 §3.1) and the same
one the roll will perform (R-941). One implementation, in this crate, used by all
three — otherwise they will diverge, and the divergence will be inaudible until it
is a bug report.

A row whose note number is outside its tuning's range yields **no note and one
observation record**, not a panic and not a silent skip.

---

## 9. Determinism

Same project state, same sample rate → **byte-identical chunks**. This is what
makes R-1402's render-twice gate meaningful for real material rather than for a
test tone, and it falls out of §4's integer arithmetic plus a stable sort.

**Stable ordering is part of the contract**: `ORDER BY at_tick, note_number`
already, and the compiler must not reorder within equal keys. Two notes at the
same tick on the same track must compile in the same order every time.

---

## 10. Testing

- **MHALL at 120 bpm compiles to hand-computed sample positions.** 5040 ticks per
  quarter, 48 kHz, 120 bpm → 24 000 samples per quarter. The first four onsets are
  0, 24 000, 48 000, 72 000. Arithmetic a human can check.
- **Tempo changes** — a map with three segments; positions after each boundary
  verified against segment-wise summation, and against the property that
  `tick_at(sample_at(t)) == t` for every event tick.
- **Monotonicity as a property test**: for any ascending tick sequence and any
  tempo map, sample positions are non-decreasing. This is the one I most expect to
  find something.
- **Determinism**: compile twice, compare bytes.
- **End to end**: MHALL compiled, rendered offline, rendered again, bit-identical
  — which is eng-07's gate reached through this crate.
- **The 16-ET party trick, audibly**: switch the phrase's tuning, recompile,
  confirm every `hz` moved and no `at` did.

---

## 11. What this does not solve, named so it is not assumed

- **Nested instances** do not realize (core-03 finding). The compiler consumes
  whatever `v_realized` returns and needs no change when that is fixed.
- **Polytempo** (R-416): the structure is N tempo streams merged in sample time;
  v0 compiles one. The merge point is the compiler, and the engine stays ignorant.
- **The margin loses onsets** (§7.3). Acceptable, measured, and revisited.
- **Audio events** (R-308) are not notes and are not in this payload.

---

## 12. Decisions requested

1. **`rev-sched` as a new crate**, depending on rev-core, rev-store and
   rev-engine, with no new external dependency (§3). Recommended: yes.
2. **Integer-exact, segment-anchored tick→sample in `i128`**, with round-half-to-
   even, replacing `f64` conversion on the compile path (§4). Recommended: yes.
3. **`TempoMap` owns both directions**, and `rev-app`'s `BPM` constant dies (§4).
   Recommended: yes.
4. **The chunk payload of §5**: notes carrying `at`, `dur`, `hz`, `level`,
   `voice`. Recommended: yes.
5. **Notes carry duration rather than paired on/off events** (§5.1, §7.2), and
   **there are no unbounded notes** (§5.2) — a continuously sounding source is
   instrument state expressed by automation, never a note without an end.
   Recommended: yes.
5a. **Notes already sounding at a locate are skipped in v0** (§7.3), with a
   start-offset field deferred rather than foreclosed; and `dur` is articulation
   *input*, not a hard gate — a pedal or a long release may sound past it.
   Recommended: yes.
6. **Chunks are stamped in play-position space** (§6.3), so looping needs no
   recompilation and the engine resolves the mapping it already has.
   Recommended: yes.
7. **500 ms window, refill at half, one-window edit margin** (§6.1, §7.1) —
   explicitly provisional, to be measured. Recommended: yes.
8. **Tuning resolution lives here and is used by the engine's live path and the
   roll alike** (§8). Recommended: yes.
9. **Out-of-range note numbers produce an observation, not a panic and not a
   silent skip** (§8). Recommended: yes.
10. **Byte-identical chunks as a tested property** (§9). Recommended: yes.
