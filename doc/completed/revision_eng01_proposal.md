# eng-01 proposal — the engine↔app interface

**Status: proposed 2026-07-20, rewritten the same day** after seven rounds of
discussion covering devices, duplex, the FS1R scenario, latency compensation
scope, MIDI delay, the first instrument, and where music stops and physics
begins. Every one of those is folded into the body; nothing lives only in chat.

Checkpoint per getstarted rule 2: public API between crates, two new
dependencies, one new crate, a file format (the log database), and three new
requirements.

This is the most consequential seam in the project. Everything downstream — the
node runtime (eng-02), automation (eng-04), the schedule compiler (eng-06), the
Control Bar's wiring (ui-04) — crosses it, and it is the one boundary where a
mistake is paid for in *real time*, on a thread that cannot allocate, cannot
lock, and cannot be late.

It also folds in the observation practice discussed on 2026-07-20 (persistent
low-overhead logging), because the real-time half of that is the same ring
machinery, and designing it twice would guarantee two different answers.

**The target that shapes this document: sound, soon.** The plan's first-sound
item is eng-07 (MHALL), four items away. That is the first *musical* sound. The
first *audible* sound should be at the end of eng-03 — a tone, driven by the
transport, with no node graph, no automation, and no schedule compiler.
Everything here is sized so that eng-03 can be built immediately after approval
and be heard.

---

## 1. Scope

**In.** The law that divides music from physics (§3); the channels between the
app thread and the real-time thread and their mechanisms (§4); the
driver-agnostic engine core (§5); the sample clock and clock-domain correlation
(§6); the v0 command set (§7); the *envelope* of a compiled schedule chunk (§8);
the observation path and the `rev-log` crate (§9); the allocation guard (§10);
device and stream policy (§11); latency scope, budget and reporting (§12);
dependencies (§13); crate boundaries and session keying (§14).

**Out — deferred implementation, not deferred shape.** Multiple simultaneous
engine sessions; MIDI into the engine; plugin hosting; audio input capture. Each
is named where it constrains the API, and the API is built so it can arrive
without a redesign.

**Out entirely.** The node/graph API (eng-02). The `AudioParam` math (eng-04).
The schedule chunk *payload* (§8 explains why this is deliberate). Voice
allocation and stealing policy. The latency *model* (R-303) beyond the engine
reporting the terms it knows. The settings file (§11.2). The log viewer (§9.7).

---

## 2. The problem, stated once

The real-time thread runs the audio device's callback. It gets a deadline —
typically 2–10 ms of wall time to produce a block — and if it misses, the user
hears a click. Within that deadline it may not:

- **Allocate or free.** The allocator takes a lock; a lock has no bound.
- **Lock anything the app thread also locks.** Priority inversion turns a 100 ns
  critical section into a scheduling quantum.
- **Do I/O**, including writing a log line.
- **Format a string.** Formatting allocates.
- **Panic.** A panic across the driver's FFI boundary is undefined behaviour at
  worst and process death at best.

Every decision below is downstream of that list. The structural claim of
`revision_poc.md` — that Rust makes this *enforced* rather than reviewed for — is
only true if the seam is built so the compiler can see it: the RT side owns its
state exclusively, and the only paths in or out are lock-free channels.

---

## 3. The law: the compiler is the last place music exists

**Below the seam there is only physics.** The engine's entire vocabulary is
samples, frequencies, channels, gains, and opaque handles. It never sees a note
number, a tick, a tuning, a tempo, a phrase, or a bar. Everything musical is
resolved *above* the seam.

This is the most important sentence in the document, and it buys three things:

- **Tuning-awareness becomes structural rather than conventional.** A voice that
  only ever receives Hz *cannot* assume 12-ET. There is no discipline to forget
  and no code review that has to catch it. (R-002's "12-ET has no privileged
  status" stops being a policy and becomes a property.)
- **The engine is testable with no project machinery** — no database, no model,
  no tempo map. A device and a ring.
- **R-003 is honoured exactly**: "Seconds are derived only at the engine boundary
  via the tempo map." This document is that boundary.

### 3.1 Three paths cross it, not two

**Live.** MIDI arrives → the app resolves it → a command stamped `NOW` → it
sounds in the next block. Nothing is scheduled, quantized, or looked ahead at.
The only things between the key and the sound are one ring push and one block of
device buffer.

The resolution step is where tuning lives, and its cost must stay negligible:
**the active tuning is resident in memory as a lookup table**, refreshed when it
changes, never queried from SQLite on the MIDI thread. Note-number-to-frequency
is then sub-microsecond, and immediacy and the law coexist at no cost.

**Playback.** Compiled chunks, sample-stamped, look-ahead. Music was resolved at
compile time; "baked content" is literally true.

**Transform.** Arpeggiators, note→CC, harmonizers, chord memory — app-side, above
the seam, feeding the same command ring as the live path. **This needs no new
mechanism**: because every command carries a time (§7), a live source can emit
events ahead of the clock, and the RT side dispatches them sample-accurately
without knowing what an arpeggio is. Live immediacy and future scheduling are the
same channel with a different number in one field.

### 3.2 The case that looks like it needs musical intelligence in RT — and doesn't

Launch quantization ("start this phrase at the next bar") is a musical decision
made while the transport runs. Under this law the RT thread cannot make it; the
compiler schedules the start at the computed sample instead.

Checked rather than assumed: this works as long as the look-ahead window is
shorter than the quantization interval. A bar at 120 BPM is 2 seconds;
look-ahead will be tens of milliseconds. The margin is two orders of magnitude.

### 3.3 The stated escape hatch

The class that would strain the law is **real-time musical reaction to audio**:
pitch tracking driving notes, audio-triggered synthesis, envelope following that
becomes a musical decision. It splits cleanly:

- The **analysis** is DSP and belongs below the seam as a node producing control
  values. That is physics, and it is fine. Envelope-following a filter needs no
  round trip at all.
- If those values must become **notes**, they travel up the observation path, are
  interpreted app-side, and come back as commands — a few milliseconds. For
  guitar-to-MIDI that is acceptable, because pitch detection needs several
  milliseconds of signal anyway; the seam is not the bottleneck.

Written down here so the hatch is a decision rather than a discovery.

---

## 4. Four channels, three mechanisms

The instinct is one ring in each direction. That is wrong, and the reason decides
three things at once.

```
   app thread                                     RT thread
   ──────────                                     ─────────
      ├──────────── command ring (SPSC) ────────────▶  drain, act
      │                fixed POD, bounded
      │
      ◀─────────── position snapshot (seqlock) ──────┤  publish, every block
      │             latest value wins
      │
      ◀─────────── observation ring (SPSC) ──────────┤  push, drop on full
      │             fixed POD, drop-and-count
      │
      ◀─────────── return ring (SPSC) ───────────────┤  hand back, never drop
                    owned values going home
```

**4.1 Command ring — app → RT, SPSC, fixed-size POD, bounded.**
Every message is a plain value of one enum, `Copy`, no pointers except the
deliberate ownership transfers of §8. Bounded, because unbounded means
allocating. **Overflow policy: the app thread refuses to send and reports it** —
commands are user intent, and silently dropping "stop" is not acceptable.
Capacity 1024, revisited with measurements.

**4.2 Position snapshot — RT → app, seqlock, latest wins.**
The Control Bar's counter wants "where is the transport *now*". A ring is exactly
the wrong structure for that: if the UI thread stalls for three frames, a ring
makes it read three-frames-stale values and then catch up, so the counter lags
and then lurches. A seqlock — write an odd sequence, write the payload, write an
even sequence; readers retry on a torn read — always gives the reader the newest
value and never blocks the writer. Payload: sample position, transport state,
block count, xrun count, peak level, the clock correlation pair (§6), and the
latency terms (§12.3).

**4.3 Observation ring — RT → app, SPSC, fixed POD, drop-and-count.**
The log records the RT thread produces (§9). Here dropping *is* correct: an
observation is not intent, and blocking the audio thread to preserve a log line
inverts the whole priority order. On overflow the RT side increments a counter;
the drain records "N records dropped", so the gap is visible rather than silent.

**4.4 Return ring — RT → app, SPSC, ownership going home.**
The RT thread never calls `drop` on anything heap-allocated. When it is finished
with a schedule chunk, a voice, or a buffer, it pushes the pointer here and the
app thread drops it. This single mechanism is what makes "no allocation on the RT
thread" survive contact with a real feature set — without it, every future
subsystem re-invents it badly.

**4.5 Why not one ring for 4.2 and 4.3.** They differ in what the reader wants:
position wants the newest value and no history; observation wants every record
and no coalescing. One mechanism serving both serves neither.

**4.6 What we implement and what we take.** The three rings are ordinary SPSC
queues and `rtrb` provides all three unmodified — the differing overflow policies
are decisions at the *call site* (check capacity before pushing, or handle
`Err(Full)`), not modifications to the queue. The seqlock is not a ring at all
and is written here. **We do not reimplement anything we also depend on.**

---

## 5. The engine core is driver-agnostic

```rust
/// Everything the engine needs to know about the world it renders into.
pub struct Format {
    pub sample_rate: u32,
    pub channel_out: u16,
    pub channel_in: u16,     // 0 on an output-only stream
    pub max_block: u32,
}

/// One block of work. Deinterleaved f32, engine-native.
pub struct Block<'a> {
    /// `None` on an output-only stream. Present from day one so that adding
    /// input later is not a signature change through every node that exists.
    pub inp: Option<&'a [&'a [f32]]>,
    pub out: &'a mut [&'a mut [f32]],
    pub frame: u32,
    /// Sample position of frame 0 on the engine timeline.
    pub at: SampleTime,
}

impl Engine {
    pub fn new(format: Format, port: RtPort) -> Engine;
    /// The whole of the real-time contract, in one function.
    /// Allocation-free, lock-free, wait-free, and it always returns.
    pub fn process(&mut self, block: &mut Block<'_>);
}
```

`Engine::process` knows nothing about cpal, WASAPI, or files. Two drivers call
it:

- **`CpalDriver`** — opens a stream and calls `process` from the device callback
  (eng-03).
- **`OfflineDriver`** — calls `process` in a loop, writing to a buffer or a WAV
  file, as fast as the CPU allows.

This is not extra work; it is *less*. It gives eng-07's render-twice
bit-identity gate (R-1402) for free instead of as a parallel implementation, and
every engine test runs headless in CI on a machine with no sound card — which is
the difference between an engine that is tested and one that is auditioned.

**The offline driver runs the allocation guard**, so CI enforces the real-time
discipline on hardware that has no real time.

---

## 6. The clock (R-302)

**The callback is the clock.** There is no timer anywhere in this system. A
`SampleTime` is a `u64` count of frames since the engine session started; it is
advanced only by `process`, and it is the only authoritative time.

```rust
pub struct SampleTime(pub u64);
pub const NOW: SampleTime = SampleTime(u64::MAX);   // "at the next block boundary"
```

Musical position (bar|beat|unit) is derived app-side through the tempo map. The
RT thread never knows tempo exists — which is what makes polytempo (R-416) free:
N tempo streams compile to N sample-stamped lists merged in sample time.

**Clock-domain correlation** gets its own module and its own tests from day one,
because R-603 (MIDI driver timestamps) and R-814 (retrospective capture
alignment) both rest on it. Each callback captures a pair — the sample position
of the block and the OS timestamp the driver reports, or `Instant::now()` where
it does not — and publishes it in the position snapshot. The app converts in
either direction by linear fit over a short history, which also yields observed
sample-clock drift as a number we can log and display.

**Honest note.** cpal's timestamp support varies by host. Where the host gives a
real callback timestamp we use it; where it does not, `Instant::now()` at
callback entry is a worse but usable estimate — and **which one is in force is
recorded in the log at stream open** rather than silently assumed.

---

## 7. The command set, v0

Small on purpose. It has to carry eng-03 and eng-05, and it has to be obviously
extensible — a message set needing restructuring at eng-06 was designed wrong.

```rust
#[derive(Clone, Copy)]
pub struct Command {
    /// When to apply. `NOW` means "at the start of the next block processed".
    pub at: SampleTime,
    pub what: What,
}

#[derive(Clone, Copy)]
pub enum What {
    // Transport (eng-03)
    Start,
    Stop,
    Locate(SampleTime),
    SetLoop { from: SampleTime, to: SampleTime, on: bool },

    // The test tone that makes eng-03 audible with no graph (§15)
    ToneOn { hz: f32, gain: f32 },
    ToneOff,

    // Schedule delivery (envelope only — §8)
    TakeChunk(ChunkHandle),
    DropSchedule,

    // Unconditional silence, always available
    AllNotesOff,

    // Observation control
    SetTraceLevel(Level),
}
```

Four properties, which are the extensible part:

- **Every command carries a time.** A button press sends `NOW`; a compiled event
  or an arpeggiator carries a sample stamp. One mechanism — and sample-accurate
  scheduling (R-1502) becomes a property of the message set rather than a later
  addition.
- **Frequencies, never note numbers** (§3). `ToneOn { hz }` is the pattern every
  future voice command follows.
- **Commands are values, not closures.** A `Box<dyn FnOnce>` would be an
  allocation the RT thread must free.
- **`What` is a closed enum**, so adding a message is a compile error everywhere
  it must be handled — the same argument that made widgets data in ui-01 §8.

### 7.1 A named gap: cancellation

If a transform source (§3.1) has queued notes into the future and the user lifts
their hands, those must not sound. `AllNotesOff` is the blunt instrument; the
general mechanism is an **epoch counter** carried on each command, with the RT
side discarding anything from a superseded epoch.

**Named, not built.** It costs one field to add when a transform layer first
exists, and building it now would be inventing requirements for a feature nobody
has specified.

---

## 8. The schedule envelope — and what is deliberately not decided

The plan text for eng-01 lists "schedule chunk format". **The envelope is settled
here; the payload is deferred to eng-06**, for a specific reason: the payload is
the compiler's contract with `v_realized`, and `core-03` — which finishes
`v_realized` — is not done. Specifying the payload now means guessing at a schema
that exists in three weeks, and the guess would be honoured long after it stopped
being right.

The envelope, which does not depend on the payload:

- A chunk is **allocated app-side**, filled app-side, and **immutable** once
  handed over.
- It crosses as a `ChunkHandle` — a raw owning pointer in a newtype — over the
  command ring, inside `TakeChunk`.
- The RT thread reads it and **never frees it**: when spent or superseded, the
  handle goes back over the return ring and the app thread drops it.
- Chunks are **sample-stamped and ordered**; the RT scheduler finds events falling
  within `[block.at, block.at + frame)` and dispatches them at their intra-block
  offsets.
- A chunk covers a **look-ahead window**; the app keeps the pipeline fed. How far
  ahead, and how re-compilation after an edit splices in (R-1508), is eng-06's.
- **eng-06 may apply a constant per-destination offset when stamping events**
  (§12.4), default zero.

This commits to the *ownership discipline*, which would be expensive to change,
and defers the *layout*, which is cheap.

---

## 9. Observation — and the `rev-log` crate

The practice: **every significant action leaves a record**, cheaply enough that
nobody is tempted to switch it off, in a place the user can look at.

**9.1 The RT half.** The RT thread pushes fixed-size POD records:

```rust
pub struct Obs {
    pub at: SampleTime,
    pub creator: Creator,   // small enum: engine.transport, engine.sched, …
    pub level: Level,       // Trace | Info | Warn | Error
    pub code: Code,         // closed enum; the message identity
    pub arg: [u64; 3],      // whatever the code's format string wants
}
```

**No strings and no formatting on the RT thread.** A code plus three integers is a
few stores; the app-side drain turns the code into a format string and renders
the text. The message catalogue is then a single table — greppable, countable,
and translatable later if that ever matters.

**9.2 The non-RT half.** UI, store and app log directly with ordinary strings —
they may allocate. Same crate, same severities, same creator vocabulary (dotted:
`ui.transport`, `store.command`, `engine.sched`).

**9.3 `rev-log`, a new crate** (`src/rust/log/`). It cannot live in `rev-core`
(WASM-able, R-104) and it is used by app, store and ui, so it sits below them and
depends on `rusqlite` and nothing else new.

A `Log` handle is cheap to clone; `log.info(creator, text)` pushes onto a bounded
channel; **one writer thread** owns the connection and writes in batches. Nothing
that logs ever touches SQLite, and nothing that logs ever blocks — R-1509 already
forbids blocking the UI on store writes, and this is the same rule.

**`rev-engine` does not depend on `rev-log`** (§14). Since RT records are POD
formatted app-side, the engine only owns a ring; the app drains it and hands text
to the log. This keeps bundled SQLite out of the audio engine's dependency tree
entirely.

**9.4 The database.** One forever file in the OS application-data directory, not
one per session — per-session files litter, and worse, they make the
cross-session questions ("has this xrun happened before?", "when did this
start?") impossible.

```sql
-- ## Observation log — a rolling record of what the application did.
CREATE TABLE entry (
  id         INTEGER PRIMARY KEY,        -- monotonic; also the ordering
  session_id INTEGER NOT NULL,           -- which run of the application
  ts         INTEGER NOT NULL,           -- unix microseconds, app clock
  creator    TEXT    NOT NULL,           -- dotted origin: ui.transport, engine.sched
  level      INTEGER NOT NULL,           -- 0 trace, 1 info, 2 warn, 3 error
  text       TEXT    NOT NULL,           -- rendered message
  detail     TEXT,                       -- nullable JSON: structure later, no migration
  keep       INTEGER NOT NULL DEFAULT 0  -- 1 exempts the row from pruning
);
```

- **A session is a column, not a filename**, so "just this run" stays a `WHERE`
  and cross-session questions stay answerable. A `session` table records start
  time, version, platform and build.
- **`journal_mode = WAL`** — a second instance can write safely, and readers (the
  viewer) never block the writer.
- **`synchronous = NORMAL`**, not `FULL`. A log may lose its last few records in a
  power cut; the project journal may not (R-808). Different files, different
  guarantees; copying settings between them would be a bug.
- **Pruning: at ~1 MB, delete oldest back to ~75%**, checked every few thousand
  inserts on the writer thread. Deliberately small and deliberately arbitrary —
  the log will tell us the real volume, and then we pick a real number.
- **The file will not shrink**, by design: SQLite reuses freed pages, so pruning
  stops growth instead of reclaiming disk, and `VACUUM` never stalls the writer.
- **`keep`** exempts a range from pruning — one column, one clause in the delete,
  and it turns a rolling buffer into something you can file a bug from.

**9.5 Messages are prose.** Someone will actually read this. The catalogue says
`stream open: HDMI, 48000 Hz, 480 frames, timestamps from driver`, not
`STREAM_OPEN 48000 480 1`. The POD-plus-catalogue design already allows it; this
is the instruction to write it that way.

**9.6 Trace is a firehose.** Per-block records at ~100 blocks/second bury
everything. Default level is Info; Trace is switched on per creator, which is
what `SetTraceLevel` exists for.

**9.7 Stderr echo now, viewer soon.** Debug builds echo every record to the
console — zero design cost, and it makes the log watchable *while* eng-03 is
being built, which is when the seam is least trustworthy. The real viewer (live
tail, auto-scroll, filter by creator and severity, drained per UI frame) becomes
a **new plan item on the ui track, scheduled after eng-03**.

**9.8 Deferred.** Whether *some* logging belongs in the project file (the user's
UI actions, arguably) stays open. Nothing here forecloses it: `creator` already
namespaces by origin, so routing a subset to a second destination later is a
change of sink, not of shape.

---

## 10. The allocation guard

A global allocator wrapper plus a thread-local "this thread is real-time" flag.
When the flag is set, an allocation panics with a backtrace. About forty lines,
no dependency.

- **On in debug and test builds; compiled out in release.** The guard is a
  development instrument, not a runtime safety net — a panic in the audio
  callback is worse than the allocation it caught.
- Set around `Engine::process`, so it covers every future node automatically.
- The `OfflineDriver` sets it too, so **CI enforces the discipline without a
  sound card**.

**Release policy for a fault in the callback: never panic.** A voice that cannot
start is not started, and the failure is *logged*, not thrown. Starvation
produces silence plus an observation record, never a block and never a partial
buffer. This is the concrete form of the poc's "callback always completes".

---

## 11. Devices and streams

**11.1 Selection is an act, not an inheritance.** "The default device" means
whatever Windows last decided — plug in an interface and it may switch silently.
So: a preference list with an override, and **the resolved device, format, buffer
size and timestamp source all logged at stream open**.

Development default for now: **integrated audio, HDMI out**. Selection override
by environment variable (`REVISION_AUDIO_DEVICE`) until there is a settings file.

**11.2 Settings are deferred.** A real settings file is a file format and
therefore its own checkpoint, and it is too early — but its *shape* is recorded
so nobody builds a single-layer thing: **global settings with project-level
overrides**, per Notorolla's precedent.

**11.3 Duplex, and the single-device rule.** R-301 requires duplex from initial
implementation. That is architecture, not a demand that an input stream exist
where the device has none — an HDMI endpoint has no input, and inventing one
would be worse than not having it.

> The stream is duplex where the device offers input, output-only where it does
> not, and **which one is in force is recorded at open**. The input-carrying code
> path exists from day one and is exercised the moment a device with input is
> selected.

And the rule that keeps §6 honest:

> **Input and output come from a single device.** Two devices are two clocks, and
> they drift; the correlation module assumes one timebase. A split input/output
> pair is out of scope until clock-drift work exists, and choosing one is refused
> with a stated reason rather than silently accepted.

This is not a restriction to be bumped into: routing an external instrument
through the application means input and output are both the interface anyway.

**11.4 Fill our channels and zero the rest.** HDMI endpoints frequently expose 6
or 8 channels when the sink is stereo. Filling 2 of 8 leaves whatever was in the
buffer on the other six — silence if lucky, a loud surprise if not.

**11.5 The stream opens once and never stops.** Silence is written when the
transport is stopped; the device never learns that stop was pressed. Four
reasons, any one sufficient: starting a stream costs milliseconds; the sample
clock's continuity would break; R-1512 says nothing gates the start; R-1513 says
the application is playable on launch. There is also a practical one — many TVs
mute their amplifier during silence and take 100–300 ms to unmute, so a
stream-per-transport design would swallow the first note and look exactly like a
scheduling bug.

**11.6 Device loss is a normal event.** An HDMI endpoint disappears when a
display sleeps or switches input. Policy: **transport stops, silence, an
observation record, and a visible state** — never a panic in the callback, never
a silent hang. Getting this on day one instead of in front of a user is a gift,
not a nuisance.

---

## 12. Latency: scope, budget, reporting

**12.1 What we compensate.** Proposed as **R-310** (§16): the latency model
compensates only delay *the system introduces* — conversion, device buffers,
declared processing latency, and transport of events to their destination. Delay
inherent to an instrument's own response is musical content, not error.

The FS1R's key-down-to-sound delay is the case that settles it. Compensating it
would shift a recording *earlier than the player heard it*, correcting a
performance to something that never happened. Keyboard players have been pressing
early for slow attacks since the 1970s; that is the musician's craft, not a
defect to be measured out. This deletes a feature Cubase has — no ping routine,
no per-instrument delay value, no calibration step — and the rare case is served
by an ordinary journalled edit to the recorded material.

**12.2 The budget.** Proposed as **R-311** (§16): **10 ms round trip, input to
output, excluding the instrument**. Below ~5 ms nobody detects it; ~10 ms still
feels connected; past ~20 ms it feels like playing through something. Achievable
on real hardware — an interface at 128 frames gives 2.7 ms each way plus
conversion, landing near 7–8 ms.

**Not achievable on shared-mode HDMI**, which hands out ~10 ms buffers before we
do anything. Stated rather than quietly avoided: the budget targets a real
interface, and the current development configuration does not meet it.

The useful corollary is that R-305's exclusion threshold need not be invented
separately: the budget is the *total*, so the processing allowance is what remains
after device buffers, and declared latency that would push a live path over 10 ms
is excluded automatically. One number, not two, and it adapts to buffer size.

**12.3 What the engine reports.** Not a latency model — just the terms it knows,
in samples, at stream open and in the position snapshot: input buffer, output
buffer, and internal path delay. If the engine cannot report them, R-303's model
has nowhere to get them, and adding the reporting later means touching the seam.

**12.4 MIDI transmission delay is compensable in principle, and nothing is built
for it.** It is delay the system introduces, so R-310 permits it. But the
congestion that made it worth compensating is gone for USB-class instruments —
what remains is 1–3 ms and roughly *constant*, which is indistinguishable from
the instrument's own response and therefore out of scope by R-310 anyway. The
provision is one sentence in eng-06's contract (§8): a constant per-destination
offset, default zero. No measurement, no UI, no calibration.

**12.5 Two honest notes about DIN MIDI**, recorded so nobody later files them as
defects or "fixes" them:

- USB removes the *host→interface* hop, not *interface→instrument*. A DIN-connected
  instrument still receives at 31,250 baud — ~1 ms per note-on.
- Consequently a ten-note chord is spread over ~10 ms and **its notes are not
  simultaneous**. This is a property of MIDI, not a defect. Nothing here corrects
  it; the only obligations are to add no jitter of our own and not to reorder a
  chord arbitrarily between takes.

**12.6 Where the platform offers scheduled output**, prefer it to offsets. CoreMIDI
takes timestamps on outgoing packets, ALSA's sequencer has scheduled queues, and
Windows' newer MIDI stack supports timestamped output where classic winmm did
not. Handing the driver the stamp our scheduler already computed makes jitter the
driver's problem. Whether `midir` exposes this per platform is a question for the
midi item — but it changes what we would build, so it belongs in the record now.

---

## 13. Dependencies

Two, both narrow.

| Crate | Version | Licence | Why |
|---|---|---|---|
| `cpal` | 0.18.1 | **Apache-2.0** (no choice offered) | The audio device. The only serious cross-platform Rust option; thin, boring, replaceable — WASAPI/CoreAudio/ALSA behind one enumeration and callback API. |
| `rtrb` | 0.3.4 | MIT OR Apache-2.0 → **Apache-2.0 adopted** | The three SPSC rings of §4. ~1k lines, wait-free, no dependencies. What it earns its place with is correct acquire/release ordering and cache-line padding so producer and consumer indices do not false-share — precisely the bug class that reproduces once a week on one machine. |

**Census, measured not guessed** (Windows, this workspace): 114 packages today,
**127 with both added — 13 net-new**:

```
cpal  dasp_sample  rtrb
windows  windows-collections  windows-core  windows-future
windows-implement  windows-interface  windows-numerics
windows-result  windows-strings  windows-threading
```

Eleven of thirteen are the `windows` crate family, all `MIT OR Apache-2.0`,
Microsoft's generated bindings. This is a *different* family from the
`windows-sys` winit already pulls: `windows-sys` is raw FFI, `windows` is the COM
layer WASAPI needs. Both being present is normal and not duplication in any
meaningful sense. `dasp_sample` (MIT OR Apache-2.0) is cpal's sample-format
conversion trait — about 500 lines of `From` impls.

**Not taken:** the `asio` feature (proprietary SDK — the coding standard's "never
enters the repo" rule; a local opt-in build feature later, never in CI) and the
`realtime` feature (thread-priority elevation via `audio_thread_priority`, which
is Linux/dbus-centric; on Windows we call `AvSetMmThreadCharacteristics` directly
if measurement says we need it).

**Not taken, deliberately: a logging framework.** `log`/`tracing` are good crates
solving a different problem — developers reading a console. This is a user-facing
feature with a database, a viewer and a retention policy. Wrapping `tracing`
would mean fighting its subscriber and formatting model for no gain.

**No new dependency for `rev-log`**: `rusqlite` is already approved (core-01).

---

## 14. Crate boundaries and session keying

```
rev-engine   ← cpal, rtrb.  Nothing else — not even rev-core (§3).
rev-log      ← rusqlite.
rev-app      ← both, plus everything.
```

Two rules, stated rather than left to accident:

- **`rev-engine` never depends on `rev-store`.** The engine consumes compiled
  chunks; it never reads the database. That boundary is what keeps the RT thread
  honest.
- **`rev-engine` never depends on `rev-log`.** RT records are POD formatted
  app-side, so the engine owns a ring and nothing more. This keeps bundled SQLite
  out of the audio engine's dependency tree.

The `rev-core` exclusion is a consequence of §3 rather than an independent rule:
an engine that speaks only physics has nothing to import from a model of music.
If eng-05 shows this is wrong, that is a finding worth recording, not a rule to
quietly drop.

**Session keying** (ui-01 §4, invariant 6). The engine interface is obtained per
session, never as a global:

```rust
pub struct EngineSession { /* app-side ends of the four channels */ }
pub struct Host { /* device ownership; hands the stream to one session */ }
```

Only one session holds the audio device — the Cubase model settled in the
multi-document discussion. Others exist, keep state, and render offline if asked.
**Implemented at N = 1**, but there is no static, no singleton and no
`&'static Engine` anywhere, so N > 1 is a policy change rather than a rewrite.

---

## 15. What eng-03 delivers — first sound

Stated here so the exit criteria are agreed before the code:

1. cpal enumerates devices; a stream opens on the selected device with a
   negotiated format; **device, format, buffer size, duplex-or-output-only and
   timestamp source are all logged**.
2. `Engine::process` fills blocks. `ToneOn`/`ToneOff` over the command ring makes
   **an audible sine**, gated by the transport.
3. The Control Bar's Play button sends `Start`; the counter reads the position
   snapshot and moves. (The first real wire between the halves of the
   application; ui-04 proper is more, but this proves the seam.)
4. The position snapshot carries callback timing; the perf ledger gets its first
   real-time entries — **device-keyed as well as machine-keyed**, so nobody later
   reads an HDMI number as a baseline. Callback timing is meaningful on any
   device; end-to-end latency is not, and is not recorded here.
5. The `OfflineDriver` renders the same tone to a file, and a test asserts two
   renders are bit-identical — R-1402's gate working before there is anything
   complicated to render.
6. The allocation guard is active in tests and green.
7. Records are visible: stderr echo in debug, rows in the log database.

That is sound, in one item, without waiting for the graph.

---

## 16. Requirements this proposal proposes

Drafted for the requirements document, subject to the same approval:

> **R-310 [Arch].** The latency model (R-303) compensates only delay the system
> itself introduces: conversion, device buffers, declared processing latency, and
> transport of events to their destination. Delay inherent to an instrument's own
> response — external hardware or hosted plugin — is musical content, not error,
> and is neither measured nor compensated. Where a musician wants such timing
> adjusted, the adjustment is an ordinary edit to the recorded material, not an
> automatic correction.

> **R-311 [Arch].** The live-path budget (R-304) is 10 ms round trip, input to
> output, excluding the instrument's own response. Processing whose declared
> latency would carry a live path beyond the budget is excluded per R-305; the
> allowance is therefore what remains after device buffers, not a separate
> threshold.

> **R-624 [P2].** An external instrument is a named pairing of a MIDI destination
> (R-604) and an audio return. A track addresses it as an ordinary instrument
> target (R-712).

---

## 17. Consequences outside this proposal

- **eng-02/eng-05 input, recorded from the instrument discussion**: the first real
  instrument is a PadSynth port aimed at a harpsichord/clavinet — chosen partly
  because a plucked, decaying sound makes scheduling errors *audible* where a pad
  would smear them. PadSynth's bandwidth profile is a physical model of unison
  detuning (spread in cents, diverging in Hz with partial index), which is why it
  excels at closely-tuned multiple strings; bandwidth wants fine resolution at
  small values, since the useful territory for a single string is near zero. It
  needs a filter envelope and a **multi-segment release stage** — the DX7
  harpsichord's release pluck is an ordinary EG release that rises before it
  falls, and nothing more than that. Release is a *playtime* parameter, so it does
  not enter the bake cache key.
- **Voice lifetime**: a release stage means a voice outlives its note-off by an
  unpredictable tail. Consequences for eng-05: a **voice pool with a free list**,
  reclaimed when release completes rather than at note-off; stealing policy that
  distinguishes held from releasing.
- **eng-07**: an offline render must run until the last voice has released, not
  until the last event. A truncation bug would otherwise produce two *identical*
  truncated renders and pass R-1402's gate while being wrong.
- **eng-06** inherits the chunk envelope (§8) and owes the payload; it may apply a
  constant per-destination offset (§12.4).
- **ui-04** can begin as soon as eng-03 lands.
- **A new ui-track plan item**: the log viewer, scheduled after eng-03 (§9.7).
- **`revision_poc.md` §"Timing engine sketch"** is superseded where it differs;
  the sketch stays as history.

---

## 18. Decisions requested

**The seam**

1. **Four channels, three mechanisms** (§4): command ring, position seqlock,
   observation ring, return ring. Recommended: yes.
2. **Asymmetric overflow: commands refuse and report; observations drop and
   count** (§4.1, §4.3) — intent and observation are different things.
   Recommended: yes.
3. **The law of §3 — the compiler is the last place music exists; the engine
   receives frequencies, never note numbers** — with the escape hatch of §3.3
   written down. Recommended: yes.
4. **Driver-agnostic `Engine::process`** with cpal and offline drivers (§5).
   Recommended: yes.
5. **The callback is the clock; `SampleTime` is the only authoritative time**
   (§6), with a correlation module and tests from day one. Recommended: yes.
6. **Command set v0 as §7**, every command time-stamped, `What` a closed enum,
   epoch cancellation named but not built. Recommended: yes.
7. **Schedule *envelope* settled now, payload deferred to eng-06** (§8), with the
   ownership discipline binding from today. Recommended: yes.

**Observation**

8. **RT observations are POD codes formatted app-side; messages are prose**
   (§9.1, §9.5). Recommended: yes.
9. **`rev-log` as a new crate** (`src/rust/log/`), depending only on rusqlite,
   with `rev-engine` *not* depending on it (§9.3, §14). Recommended: yes.
10. **One forever log file with the schema of §9.4** — session as a column, WAL,
    `synchronous = NORMAL`, prune at ~1 MB, `keep` flag. Recommended: yes.
11. **Stderr echo in debug now; a log viewer as a new ui-track item after
    eng-03** (§9.7). Recommended: yes.

**Real-time discipline**

12. **Hand-rolled allocation guard, debug/test only**, active in offline renders
    so CI enforces it (§10). Recommended: yes.

**Devices and latency**

13. **Device selection is explicit and logged**, env-var override, settings file
    deferred with its two-layer shape recorded (§11.1, §11.2). Recommended: yes.
14. **Single-device duplex; output-only permitted and recorded; a split pair
    refused with a reason** (§11.3). Recommended: yes.
15. **R-310** — compensate only what the system introduces (§12.1, §16).
    Recommended: yes.
16. **R-311** — a 10 ms live-path budget, with R-305's allowance derived from it
    (§12.2, §16). Recommended: yes.
17. **R-624** — external instrument as a MIDI destination paired with an audio
    return (§16). Recommended: yes.

**Dependencies and boundaries**

18. **`cpal` 0.18.1 (Apache-2.0) and `rtrb` 0.3.4 (Apache-2.0 arm)**, 13 net-new
    packages, no logging framework (§13). Recommended: yes.
19. **`rev-engine` depends on cpal and rtrb only** — not rev-store, not rev-log,
    and provisionally not rev-core (§14). Recommended: yes.
20. **Engine sessions are keyed, never global; one session holds the device**
    (§14), implemented at N = 1. Recommended: yes.
21. **eng-03's exit criteria as §15**, including an audible tone, device-keyed
    ledger entries, and the render-twice bit-identity test. Recommended: yes.

**Stated as rules rather than numbered decisions** (§11.4–11.6, §12.4–12.6, §17):
channel zeroing, stream-never-stops, device-loss policy, the DIN chord-serialization
note, scheduled MIDI output preferred over offsets, and the eng-02/eng-05
instrument input. Raise any of them and it becomes a decision.
