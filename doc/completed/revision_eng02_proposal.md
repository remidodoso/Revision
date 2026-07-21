# eng-02 proposal — the graph runtime node API

**Status: proposed 2026-07-21.** Checkpoint per getstarted rule 2: the API every
instrument this project ever has will be written against.

Its input is `doc/revision_padlington_inventory.md` — a read-only census of the
voice we are actually porting. That matters, because the honest risk in a node
API is designing for an imagined instrument. This one is designed for a real one
whose node graph, parameter vocabulary and envelope math have already been
written down, and which is deliberately the *cheapest* voice in Notorolla's
roster: two read heads, a gain, a panner, a filter.

**The scope discipline: build what PadSynth needs, arrange it so more can arrive,
implement nothing else.** We are not writing a toolkit.

---

## 1. Scope

**In.** What a node is (§3); how a graph is described and how it is compiled to
something the real-time thread can walk (§4); buffers and channels (§5); the
voice as a graph instance, and the pool that makes note-on allocation-free (§6);
how parameters attach, with the math deferred to eng-04 (§7); sample-accurate
starts (§8); determinism and seeding (§9); the node census we actually implement
(§10).

**Out — deferred, shape not foreclosed.** Every node beyond §10's four. Feedback
loops and delay lines. Sidechaining between voices. Multi-output nodes. Sample
rate conversion inside the graph. Hosted plugins.

**Out entirely.** The Web Audio *API* — see §2. `AudioParam` ramp math (eng-04).
Voice design (eng-05). The bake (dsp-02).

---

## 2. "Web Audio semantics" — what R-704 buys and what it does not

R-704 says the runtime has Web Audio semantics. Taken literally that could mean
anything from "a few node types" to "reimplement a browser". What we take:

- **The node-and-connection model**: nodes with inputs and outputs, connected
  into a directed acyclic graph, processed in dependency order.
- **The parameter model**: values that can be constant, scheduled, or driven by
  another node's output, with the four-method ramp vocabulary the inventory found
  in actual use (§7).
- **`start`/`stop` with times**, and a source that knows its own duration —
  which is exactly the shape a compiled `Note` already has (R-402a).

What we do not take: the JavaScript object model, the `AudioContext` lifecycle,
garbage-collected node graphs, `MediaStream`, worklets, or the 128-frame render
quantum. Those are properties of running inside a browser, and we are not.

**Why take any of it.** Because the voice we are porting is written in it, the
math the inventory recorded is specified against it, and the W3C document is a
precise normative reference for behaviour that is otherwise a matter of taste.
The same reasoning as R-939 adopting the 1992 HIG: use the written specification
rather than re-derive it, and cite it rather than recall it.

---

## 3. A node is data, not a trait object

```rust
/// What a node *is*. A closed enum, so adding a kind is a compile error
/// everywhere it must be handled.
#[derive(Clone, Debug)]
pub enum NodeKind {
    /// A read head over a baked table (§10).
    BufferSource { table: TableId, looping: bool },
    Gain,
    StereoPanner,
    Biquad { mode: BiquadMode },
}
```

**Closed enum rather than `Box<dyn Node>`**, for the same three reasons widgets
became data at ui-01 §8, and one more that is specific here:

- **A patch is data.** R-621 wants device profiles definable as data; R-103 wants
  scripting; a future patch editor must be able to *describe* a graph, not
  construct one out of Rust types. A closed enum serializes; a trait object does
  not.
- **Adding a kind should break the build**, in every place that must learn about
  it — the RT dispatcher, the serializer, the editor, the documentation.
- **No dynamic dispatch in the inner loop.** A `match` per node per block is
  free; the alternative is a vtable indirection that also defeats inlining of the
  small nodes, which are most of them.
- **`Box<dyn Node>` is an allocation**, and the pool (§6) has to pre-build every
  voice. Pre-building boxed trait objects is possible but the enum makes the
  no-allocation property visible rather than argued.

The cost is honestly stated: **a third party cannot add a node type without
editing this crate.** We are not a plugin host at this layer (R-1208's plugins
live out-of-process), and that trade is the right way round for us.

---

## 4. A graph is described, then compiled

```rust
/// The description — serializable, editable, and what a patch actually is.
pub struct GraphSpec {
    pub node: Vec<NodeSpec>,           // kind + initial parameter values
    pub edge: Vec<(NodeRef, NodeRef)>, // from output to input
    pub out: NodeRef,                  // the node whose output leaves the voice
}

/// The compiled form: what the real-time thread walks.
pub struct Graph {
    /// Nodes in dependency order, resolved once. The callback never sorts,
    /// never traverses edges, never looks anything up by name.
    order: Vec<Node>,
    /// Which buffer each node reads and writes, assigned once.
    wiring: Vec<Wiring>,
    buffer: BufferPool,
}
```

**Topological order is computed app-side, at build.** The real-time thread walks
a `Vec` in order. This is the same move the schedule compiler makes for music:
everything that can be decided before the deadline is decided before the deadline.

**Cycles follow the Web Audio rule**: a cycle is legal **if and only if it
contains a delay of at least one render quantum**. The delay breaks the
dependency — its output at *t* depends on its input at *t − delay* — so the graph
can be cut there and what remains is a DAG. That is the same `z⁻ⁿ` element any
signal-flow diagram uses, and it is a better rule than the one this proposal
first invented in ignorance of it.

Since the census (§10) contains no delay node, **every cycle is currently
illegal** — so today's behaviour is what a flat rejection would give, but as a
*specialization of the general rule*. Feedback arrives with the delay node and
needs no redesign.

**One deliberate departure: we error where the browser mutes.** The specification
silences the nodes in an illegal cycle, which is a browser's priority — never
break the page. Silence is the hardest failure to diagnose, and we have taken the
other side of this trade twice already (out-of-range notes are counted and
reported at eng-06 §8; starvation records an observation at eng-01 §10). A patch
that silently plays nothing is worse than a patch that refuses to build.

**Buffer assignment is computed at build too** — which buffer each node writes and
which it reads. A node whose output feeds exactly one input can write in place.
This is a small liveness analysis, done once, on the app thread, and it is why
the pool's memory is bounded and known.

---

## 4a. A fixed render quantum, anchored to absolute sample position

The cycle rule needs "one render quantum" to be a number. Web Audio has one —
128 frames — because it owns its own callback. **We do not**: the device hands us
480 frames on one machine and something else on another, and `max_block` is a
ceiling rather than a size.

Chasing that leads somewhere more important. **k-rate parameters make output
depend on block size.** A filter cutoff updated once per block updates every 480
frames on one device and every 128 on another, so the same project renders
differently depending on the hardware — which breaks R-1402 silently. There is
already a test asserting the opposite (`a_variable_block_size_changes_nothing`);
it passes today only because a sine has no k-rate anything, and it would become a
lie the moment eng-04 lands.

**So the graph processes in a fixed quantum of 128 frames, whose boundaries fall
at multiples of 128 from the session start** — not from the block start. A
480-frame callback covers three whole quanta and part of a fourth; the next
callback continues that partial one. Parameters are evaluated at quantum
boundaries, and those boundaries are identical however the device chops up time.

Three problems, one mechanism:

- **Block-size-independent determinism**, for real rather than accidentally.
- **A defined control rate**: 375 Hz at 48 kHz, which is what the ported voice was
  written against.
- **A number for "one quantum"**, so the feedback minimum delay is enforceable.

The cost is carrying partial-quantum state across callbacks. That is a few lines,
against three correctness properties we otherwise do not have.

## 5. Buffers and channels

Buffers are the engine's existing planar `f32` layout (`PlanarMut`), one block
long, pre-allocated in the pool.

**A node declares its channel count**, derived at build from its kind and its
inputs: sources and gains are mono until something widens them, the panner
outputs two, and everything downstream of it is two. Padlington's graph is
mono until the panner and stereo after it, and that falls out rather than being
configured.

**No implicit up-mixing or down-mixing.** Web Audio's channel-count rules are one
of its genuinely confusing corners; connecting a 2-channel output to a 1-channel
input is a **build error** here, not a silent sum. If mixing is wanted, there is
a node for it.

---

## 6. A voice is a graph instance, and the pool is the point

```rust
pub struct Voice {
    graph: Graph,
    state: VoiceState,   // Free | Held | Releasing
    /// Set at note-on; the note is over when the release finishes, not when the
    /// duration expires (R-402a: `dur` is articulation input, not a hard gate).
    note: Note,
    /// Deterministic per voice (§9).
    seed: u64,
}
```

**The whole pool is built when the instrument is loaded**: N voices × the graph's
nodes × their buffers, allocated once, on the app thread. Note-on takes a voice
off a free list; note-off moves it to `Releasing`; **the voice returns itself to
the free list when its release completes**, not when its duration expires.

That last point is the one that has to be right from the start. It is what makes
a release tail — the DX7 harpsichord's key-up pluck, the damper on a clav —
expressible at all, and it is why R-402a's "no unbounded notes" matters: every
pooled voice eventually returns, with no exceptions, so **a voice sounding with
no scheduled end is by construction a bug** and the observation log can assert on
it.

**Stealing distinguishes `Held` from `Releasing`.** A releasing voice is making a
sound nobody is playing any more and is the right thing to take first. That
distinction is only available because release is a real state rather than a fade.

**Voice count is fixed per instrument**, chosen at load. Exhaustion steals; it
never allocates and never drops silently — it records.

---

## 7. Parameters attach here; the math is eng-04's

```rust
pub struct Param {
    /// The value when nothing is scheduled and nothing is connected.
    pub default: f32,
    /// Scheduled events, evaluated by eng-04.
    pub event: ParamEvents,
    /// An optional audio-rate driver: another node's output added to the value.
    pub driver: Option<NodeRef>,
}
```

The inventory found the vocabulary actually in use, and it is exactly four
methods: `setValueAtTime`, `linearRampToValueAtTime`,
`exponentialRampToValueAtTime`, `setTargetAtTime`. **That is eng-04's whole
scope**, confirmed by a census rather than assumed from the specification.

Two decisions belong here rather than there:

**k-rate versus a-rate is a property of the parameter, fixed at build.** A gain
being driven by an envelope is a-rate (per sample); a filter's Q is k-rate (once
per block). Web Audio decides this per parameter and so do we, because the cost
difference is large and the choice is never dynamic.

**A parameter driven by a node is a graph edge**, so it participates in the
topological order. Otherwise an envelope could be evaluated after the node it
modulates.

---

## 8. Sample-accurate starts, without splitting the block

A note beginning partway through a block is common — at 480 frames, almost every
note does. Two ways: split the block at every event boundary and process the
graph repeatedly, or give each voice a start offset within the block.

**We give the voice an offset.** Splitting means N graph passes for N notes in a
block, each with fixed overhead — so the busiest moment, when the deadline is
closest, becomes the most expensive. That is the wrong way round. A voice that
starts at frame 137 writes silence for 137 frames and then begins: one pass,
sample-accurate, one branch per voice.

**What the engine does today is neither.** A command due anywhere inside a block
is applied at the block's start, so onsets are quantized to the device buffer —
10 ms at current settings. Inaudible for a test tone; fatal for anything with
feel, because it is exactly the small differences that carry groove. A strum
becomes a chord. This decision is how that stops being true, and it is what
R-1502 asks for.

**Live input is the other case, and the offset does not help it.** A key press has
already happened by the time the engine hears about it, so the offset would point
backwards. A live note starts at the next block boundary — as soon as possible,
which is the only thing available — while the *captured* note is timestamped at
the driver boundary (R-603) and lands on the timeline where it was actually
played. Two accuracies for two jobs: what you hear is immediate, what is recorded
is exact. Delaying monitoring to make it "accurate" would spend R-311's latency
budget to fix a problem the capture path does not have.

Two consequences worth recording rather than discovering:

- **Live commands should carry the sample position computed from the driver
  timestamp**, with the engine starting the voice at `max(position, block start)`.
  One mechanism instead of two: it clamps to the block start in the usual case,
  and lands exactly when an interface timestamps ahead. `NOW` remains the sentinel
  for "no timestamp exists" — a button press.
- **Block dispatch makes latency variable, not merely large.** Press just after a
  block begins and you wait nearly a full one; press just before the next and you
  wait almost none. Jitter is more perceptible than constant delay. The trade —
  delay every live note to a fixed one-block offset, buying steadiness with
  latency — has no obvious winner and should be decided against measurements when
  MIDI input exists.

---

## 9. Determinism and seeding

R-706 requires stochastic elements to be seeded and deterministic, and the
inventory names the one place Padlington is not: the two read heads take random
start offsets, unseeded, per note.

**Each voice's seed is a pure function of the note and the instrument's seed** —
not of a global counter, not of the pool slot it happened to get, and not of
wall-clock time. Pool slots are an allocation detail; if the seed depended on
one, the same project would render differently depending on what else was
sounding, and R-1402's render-twice gate would fail intermittently, which is the
worst way for it to fail.

---

## 10. The node census we implement

Exactly what the Padlington voice uses, and nothing else:

| node | why | notes |
|---|---|---|
| `BufferSource` | the read head over the baked table | loop, `playback_rate`, `detune`; two per voice with decorrelated seeded offsets |
| `Gain` | the amplitude envelope, and head mixing | a-rate |
| `StereoPanner` | width from two decorrelated heads | equal-power, the one place mono becomes stereo |
| `Biquad` | the filter, with its own envelope | lowpass first; the other modes are a `match` arm each |

Four kinds. Every other node named in the port plan — oscillator, noise, delay,
convolver, waveshaper, compressor — arrives when something needs it. The
structure is built for N; the implementation is N = 4, which is the same
discipline ui-01 applied to windows.

---

## 11. What this makes possible next

- **eng-04** implements the four ramp methods against the W3C normative formulas
  and unit-tests them against the specification's own examples.
- **eng-05** builds the Padlington voice as a `GraphSpec` — which should be
  roughly the diagram in the inventory §4, transcribed.
- **dsp-02** bakes the table, entirely outside this API: the bake is pure
  data-in/data-out and touches no node.
- **eng-07** plays MHALL through it, and the schedule compiler is already proven
  against hand-checkable positions — so from here on, a wrong note is
  unambiguously the instrument's fault.

---

## 12. Decisions requested

1. **Nodes are a closed enum, not trait objects** (§3) — a patch is data, and
   adding a kind should break the build. Recommended: yes.
2. **Graphs are described (`GraphSpec`) and compiled (`Graph`)**, with
   topological order and buffer assignment resolved app-side at build (§4).
   Recommended: yes.
3. **The Web Audio cycle rule**: a cycle is legal iff it contains a delay of at
   least one render quantum; with no delay node in the census, every cycle is
   currently illegal. **We error where the specification mutes** (§4).
   Recommended: yes.
3a. **A fixed 128-frame render quantum anchored to absolute sample position**
   (§4a) — block-size-independent determinism, a defined k-rate, and a number for
   the feedback minimum. Recommended: yes.
4. **Channel counts are derived at build; mismatched connections are build
   errors, never silent mixing** (§5). Recommended: yes.
5. **A voice is a graph instance from a pre-built pool**; it returns to the free
   list when its **release** completes, not when its duration expires (§6).
   Recommended: yes.
6. **Stealing distinguishes held from releasing** (§6). Recommended: yes.
7. **Parameters carry default, scheduled events, and an optional node driver;
   k-rate versus a-rate is fixed at build; a driven parameter is a graph edge**
   (§7). Recommended: yes.
8. **eng-04's scope is the four methods the census found**, not the whole
   `AudioParam` surface (§7). Recommended: yes.
9. **Note onsets are sample-accurate, implemented as a start offset carried by
   the voice** rather than by splitting the block at each event (§8). Today the
   engine applies events at block boundaries, so timing is quantized to the device
   buffer. Live input is the exception and starts as soon as possible, with the
   capture path carrying the exact time instead. Recommended: yes.
10. **A voice's seed is a pure function of the note and the instrument seed,
    never of its pool slot** (§9). Recommended: yes.
11. **Four node kinds** — `BufferSource`, `Gain`, `StereoPanner`, `Biquad` — and
    no others until something needs them (§10). Recommended: yes.
12. **No new external dependency.** The bake will want an FFT (dsp-02's
    checkpoint, `rustfft`/`realfft` per the port plan); the runtime wants
    nothing. Recommended: yes.
