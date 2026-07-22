# midi-01 proposal — the MIDI crate API, clock correlation, and the input fork

**Status: approved and implemented 2026-07-21** (9 decisions). Proven on hardware: a real `midir` open lists the Oxygen Pro Mini. Enumeration is midi-02; playthrough is midi-03.

**Original status: proposed 2026-07-21.** Checkpoint per getstarted rule 2: a new
dependency, a new crate's public surface, a new engine command, and the shapes
that midi-02/03, recording, and the live-remap trick all build on.

Aimed at one milestone: **play Padlington from the Oxygen.** Recording (rec-01)
and live scale remap (midi-04) reuse everything here, so the shapes are chosen
for all three, but only playing has to *work* at the end of midi-03.

`rev-midi` already exists as a stub whose doc comment commits to the posture:
"input forks at birth: fast path → engine (live), event path → app
(capture/journal)." This proposal makes that concrete.

---

## 1. Scope

**midi-01 implements.** The `rev-midi` public API (event types, the fork, device
identity) — designed and typed, backed by a real `midir` open so the types are
proven against the library, but *not* the full enumeration/hot-plug story. The
**clock-correlation module** with its own tests (correlation is arithmetic and
tests without hardware). The two ring types of the fork. The **note→Hz snapshot**
type. The engine's **live-note command** shape.

**Deferred to midi-02.** Runtime enumeration, hot-plug arrival/removal (R-601),
persistent device identity (R-602), and wiring the thru path end to end.

**Deferred to midi-03.** Actual live playthrough and the honest end-to-end
latency print (R-307 v0).

**Out entirely.** MIDI output and external destinations (R-604). External clock
sync in/out — a separate transport feature; input events are stamped in our
domain and nothing else (see §4). Recording (rec-01). The remap (midi-04).

---

## 2. The dependency

**`midir`** — cross-platform (WinMM / CoreMIDI / ALSA), MIT, and the settled
Rust choice; there is no serious alternative. `rev-midi` depends on `midir` and
`rev-core` (for note numbers and the tuning it resolves against) and nothing
else. Dependency checkpoint.

---

## 3. The fork — two rings, and why not the command ring

Each input event is pushed, at the `midir` callback, to **two SPSC rings**:

- a **thru ring** → engine, for live sound, minimum latency;
- an **event ring** → app, for capture, journalling and display.

**Live notes do not travel the command ring.** Its producer is the app thread;
MIDI arrives on `midir`'s own thread, and rtrb is single-producer. Routing live
notes through the app would add a UI frame of latency, which is fatal for
playing. The thru ring keeps MIDI-thread → engine direct — the "thru ring to
engine" eng-01 anticipated (§4). The engine drains it at the top of `process`,
the same place it drains commands.

Both rings are **drop-tolerant with a count**, like the observation ring: a
dropped live note is a missed key (rare, and the count surfaces it via eng-08);
a dropped capture event is caught by the journal's own durability, not by never
dropping.

---

## 4. Clock correlation (R-603)

MIDI events arrive with a `midir` timestamp whose clock domain varies by
platform and is not guaranteed to be the engine's monotonic clock. So:

**Re-stamp at the callback boundary** with the same monotonic clock the engine
reads (`Instant` against the shared origin). Both domains become one by
construction, and the driver-boundary instant is authoritative (R-603) — order
of arrival is not.

**Map instant → sample position** by a least-squares line fit over the
`(correlate_at, correlate_nanos)` pairs the position seqlock *already publishes*
every block. The slope is the observed sample rate (drift and all), which is
also a number worth displaying (R-814). A short rolling window; the fit is pure
arithmetic and is where midi-01's tests live.

**The asymmetry that scopes this.** Live play barely needs correlation — a note
is played as soon as possible, not placed precisely. Correlation earns its keep
at *recording*, where the timestamp decides where a note lands (rec-01, R-810).
So the module is built and unit-tested here, and *exercised* at recording;
sub-millisecond device-timestamp refinement is a later enhancement, not a
blocker.

**No external clock.** Input events are stamped in our sample-clock domain and
nothing else. Incoming MIDI clock is a tempo-sync pulse, not a timestamp —
coarse (24 PPQN), jittery, and usually absent (controllers do not send it); if
Revision ever slaves tempo, that drives the `TempoMap`, never the event stamps.
Timestamps and tempo are orthogonal axes and stay that way.

---

## 5. R-312 on the hot path — the note→Hz snapshot

The engine must never see note numbers (R-312), but MIDI *is* note numbers, so
resolution happens **above** the engine, on the fast path, and must be RT-safe
on the MIDI thread.

A **note→Hz snapshot**: a 128-entry table, `note number → frequency`, for the
current tuning, published to the MIDI thread behind an atomic pointer. The fast
path does one array read — no lock, no allocation, no store access. When the
tuning changes, the snapshot is rebuilt app-side and swapped atomically; the old
one is freed once no reader holds it (the `Arc`/return discipline again). The
thru ring therefore carries **frequencies**, and R-312 holds without the engine
learning what a tuning is.

**This is also midi-04's hook.** The snapshot is "incoming key → Hz"; normally
it is a plain tuning, but nothing stops it being a tuning composed with a
keyboard map. So the live scale remap is *this type*, rebuilt by different logic,
swapped by the same atomic swap. Designing the snapshot as the resolution seam
now is what makes the remap nearly free later.

---

## 6. Live note-on/off pairing, and how it meets the transport (R-402a)

The engine today plays notes only from a compiled `Chunk`, whose notes carry
their duration. A live note has no known duration — you are holding the key. So
the engine gains a small live-note path:

- **note-on** starts a **held** voice — the pool already has `Held` and
  `release()` — with no scheduled end;
- a `(channel, note) → voice` map remembers where it went;
- **note-off** looks it up and calls `release()`.

This is exactly R-402a's "pairs at the edges, entities in the middle": MIDI
decomposes a note because the wire is serial, and the engine **re-pairs at the
input boundary** into a held voice.

### 6.1 Live play is not the transport

The keyboard is a monitored instrument, playable whether or not the sequence
runs (R-1512/1513) — so the transport and the keyboard are two independent
controllers of one voice pool. The rule that keeps them from fighting is a
single principle, and it needs no new machinery:

> **The transport controls time; only All Notes Off controls sound.**

- **Stop** stops the playhead advancing, so no new scheduled note-ons fire — and
  it does **nothing** to sounding voices. That is the whole behavior: it is a
  no-op on the pool, and the engine's existing `running` flag already gates
  scheduled dispatch, so there is nothing to add. Everything sounding ends the
  way it naturally would — a scheduled note by its written duration, a held live
  note by its note-off. A note you are physically holding when you hit Stop keeps
  sounding, because you are still on the key and the sequencer has no say over
  that.
- **A MIDI note-off** ends the one live voice it names — the normal way a held
  note ends, your finger.
- **All Notes Off** releases every voice and clears the pairing map — the input
  escape for the note you never released or a lost note-off (a dropped thru-ring
  slot, a yanked cable, a flaky key). It is the *release* semantics of MIDI CC
  123, not the sledgehammer: tails still ring, and it touches neither effects nor
  external gear. The true **Panic** (CC 120 "All Sound Off" and more — fast-fade
  every voice, flush delay and reverb, and send all-notes-off/all-sound-off to
  every external destination, with a brute-force per-note variant for stubborn
  gear) is a **future superset**, gated on effects and MIDI-out existing. The two
  stay distinct commands from the start so they are never conflated into one
  half-right one.

So there is no origin tag and no `release_scheduled`: the transport never touches
a voice, the human lets go, and if they don't, there is the panic button.

**Why pairing lives in the engine, not `rev-midi`.** It has to sit where voices
live, so a held voice is frequency-native and slot-addressed; pairing before the
thru ring would make `rev-midi` track engine voice slots across a ring, which
inverts the ownership.

### 6.2 One edge deferred to midi-02

If the pool runs dry and **steals a held live voice**, a later note-off for that
key would point at a voice now playing something else. So the map keys on a
voice **identity/generation** that stealing invalidates, and a stale note-off is
ignored. Rare with an adequate pool, real in a dense moment — its handling
belongs to midi-02, but the map is shaped for it here.

---

## 7. What crosses each ring

**Thru ring** (MIDI thread → engine), the minimum a voice needs:

```rust
enum Thru {
    NoteOn  { hz: f32, level: f32, key: LiveKey },  // key = (channel, note), for off-pairing
    NoteOff { key: LiveKey },
    // CC and pitch-bend arrive later (midi-02+); play-time CCs will ride this ring,
    // bake CCs will not (they take the app path — midi-02 note, R-715a).
}
```

**Event ring** (MIDI thread → app), the material for capture and display:
timestamped, and closer to the wire — a note number, velocity, channel, and the
sample-position stamp from §4, so recording lands events where they were played
(R-810) and the roll can show live input later.

The two are deliberately different shapes: the thru ring is *frequencies for the
voice* (R-312), the event ring is *note numbers for the model* (R-002). The fork
is where music and physics part, on the input side — the mirror of the compiler
on the output side.

---

## 8. Tests (what midi-01 can prove without a stage)

- **Correlation:** a synthetic stream of `(sample, nanos)` pairs with a known
  slope and injected jitter recovers the slope and maps an instant to the right
  sample within tolerance; a drifting clock is tracked.
- **The snapshot:** for a known tuning, `snapshot[note]` equals the tuning's
  frequency for that note — bit-identical to `TuneCache`, the same
  what-you-hear-is-what-resolves guarantee the roll has.
- **The fork:** an event pushed at the callback appears on both rings, with the
  thru side carrying the resolved frequency and the event side the note number.
- **Pairing:** note-on then note-off on the same `(channel, note)` starts and
  releases one voice; note-off with no matching on is ignored; `AllNotesOff`
  clears the map.
- **A real `midir` open** succeeds against whatever port exists (or degrades
  cleanly when none does), proving the types against the library — the only part
  that needs the machine, and it needs no *playing* yet.

---

## 9. Decisions to approve

1. Depend on **`midir`** (+ `rev-core`); `rev-midi` gains no other dependency.
2. The input **forks at the callback into two SPSC rings** — thru → engine,
   event → app — both drop-tolerant with a count. Live notes never use the
   command ring.
3. Input events are **re-stamped in the engine's monotonic domain**; the
   instant→sample fit uses the seqlock's existing correlation pairs. External
   clock is not a timestamp source.
4. Correlation is built and unit-tested in midi-01, **exercised at recording** —
   device-timestamp refinement deferred.
5. **R-312 on the hot path via a lock-free note→Hz snapshot**, atomically
   swapped; the thru ring carries frequencies. (This is also midi-04's hook.)
6. **Live note-on/off pairing in the engine**: a held voice plus a
   `(channel, note) → voice` map — R-402a re-pairing at the input edge.
7. **The transport controls time; only All Notes Off controls sound.** Stop is a
   no-op on the voice pool (it only halts the playhead, so no new scheduled
   note-ons fire); a held live note ends on its own MIDI note-off; All Notes Off
   is the sole active silencer and clears the map — the panic for a note never
   released or a lost note-off. No origin tag, no `release_scheduled`.
8. The **thru ring carries frequencies for the voice** (R-312), the **event
   ring carries note numbers for the model** (R-002); the fork is where music
   and physics part on input.
9. midi-01 implements the API, the correlation module, the ring/snapshot types,
   and the engine live-note command; **enumeration/hot-plug is midi-02**, live
   playthrough and the latency print are midi-03.
