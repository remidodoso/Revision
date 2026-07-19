# Project "Revision" — discussion notes

**Status: exploratory discussion only. Orthogonal to current Notorolla work; nothing here
changes existing code, plans, or documents.** (Started 2026-07-17.)

Two goals under discussion:

1. **A native, multi-platform DAW** incorporating Notorolla's current and some/all proposed
   functionality.
2. **Incorporating the essence of Opcode Vision** — the (probably still unrivaled) MIDI
   editor/sequencer.

---

## 1. What Notorolla already brings to the table

The codebase is — partly by design, partly by happy accident — unusually well-prepared for
a native future:

- **`core/` is pure data-in/data-out** (no DOM, no Web Audio, node-importable, notch-tested).
  This was explicitly kept WASM-portable. It is the seed of a shared engine core.
- **Time is in beats throughout the model**; seconds derive only at the audio layer. A native
  engine slots in at exactly that seam.
- **Pitch goes through the tuning seam** (`degreeToFreq`, per-tuning `edo`). No native DAW has
  first-class non-12 tuning as a *model* property — this is Revision's differentiator, not
  baggage.
- **Patterns are already Vision-style referenced material** (named, reused, not frozen copies).
  The hard conceptual alignment with Vision's sequence model is already done.
- **Export encoders are pure** (MIDI, WAV, BWF stems) and port trivially.
- What does *not* port: the **Web Audio synth graphs** (Padlington, Vesperia, Boshwick, Tervik,
  …) — a native engine means re-expressing the DSP. Mitigation: the voices are already treated
  as headless-renderable, meterable specifications (`notch/`), so they are *specified*, not just
  wired. Port = re-implement against the existing meters/tests.

## 2. Goal 1 — native multi-platform DAW

### The realistic architecture options

**A. Shell-first (Tauri or Electron around the current app).**
Web UI + Web Audio, wrapped. Gains: real file I/O (actual save, replacing localStorage),
menus, packaging, installers. Days-to-weeks of work. Audio remains Web Audio: fine quality,
but ~10–30 ms latency, no ASIO, no plugin hosting. Tauri preferred over Electron (lighter,
Rust-native — dovetails with the existing Rust/WASM intent), with the caveat that Tauri uses
the OS webview (WebView2/WebKit), so Web Audio behavior varies slightly per platform;
Electron ships a known Chromium if that ever bites.

**B. Hybrid: web UI + native Rust audio/MIDI engine (the recommended trajectory).**
UI stays HTML/canvas (everything learned about the control-skin program carries over
untouched). The engine — clock, scheduler, voices, mixing — moves to Rust: `cpal` for audio
(CoreAudio/WASAPI/ASIO/ALSA), `midir` for MIDI I/O. UI and engine talk over a command
queue (Tauri IPC or shared memory); **the engine owns the clock**, the UI follows it — the
inverse of today. `core/` either stays JS (runs in the webview, sends compiled schedules to
the engine) or gets ported to Rust and compiled *back* to WASM for the web version — one
core, two frontends. That second option means web-Notorolla and native-Revision stay one
project rather than diverging forks.

**C. Full native rewrite (JUCE/C++, or pure-Rust UI).**
JUCE is the industry path (plugin hosting, every platform, battle-tested audio I/O) but is a
total rewrite in C++ and abandons all UI work. Pure-Rust UI (egui/iced/slint) abandons the
UI work too and the ecosystem is younger. Only worth it if plugin *hosting* becomes a core
requirement (see below). Not recommended as a starting move.

### What "DAW" must mean — the key scoping question

A **general** DAW (audio tracks, recording, comping, time-stretch, full plugin hosting,
video sync…) is a decade-scale product. A **pattern-composition DAW** — Notorolla's
generative/tile model + real audio & MIDI I/O + Vision-grade MIDI editing + hardware-synth
sequencing — is a tractable multi-phase project for this codebase, and is also the thing no
one else makes. Working assumption until decided otherwise: **Revision is the latter.**
Open sub-questions:

- **Plugin hosting (VST3/AU/CLAP)?** The single biggest cost driver. Hosting is much harder
  than *being* a plugin. CLAP is the Rust-friendliest. Defer-able: external hardware/soft
  synths can be driven over MIDI first (very Vision, incidentally).
- **Audio recording?** Even one mono input track drags in monitoring, latency compensation,
  file management. Defer-able.
- **Or neither** — internal synthesis + MIDI out, i.e. "Notorolla grown up + Vision", is
  already a complete instrument.

### Phasing sketch (each phase ships something usable)

- **Phase 0 — Shell.** Tauri wrap of the current app. Real save/load to disk, menus,
  installers. No engine change. *(Weeks.)*
- **Phase 1 — Native MIDI.** `midir` bridged into the app: MIDI in (needs the already-planned
  `noteOn/noteOff` voice API) and MIDI **out to hardware** — the first Vision-shaped
  capability, and it makes Revision useful in a studio immediately. *(Weeks–months.)*
- **Phase 2 — Native engine.** Rust audio engine; port voices against the notch meters;
  engine-owned clock; sub-5 ms round trip. The big lift. *(Months.)*
- **Phase 3 — The Vision layer.** Full MIDI editor (see §3), record/loop-record, quantize
  family, sequence triggering. *(Months, overlappable with 2.)*
- **Phase 4 (optional) — Hosting / audio tracks.** Only if scoped in. *(Large.)*

## 3. Goal 2 — the essence of Opcode Vision

**Primary source:** the *Vision 4.5 MIDI Reference Manual* (1999, 499 pp) —
<http://oldschooldaw.com/opcode/uploads/PDF/OpcodeVision-4.5-MIDI-Reference-Manual-1999.pdf>.
The analysis below is grounded in it (chapter refs are to that manual). Worth keeping a
local copy if Revision proceeds.

### 3a. The reference model — sequence events & segments (Ch. 13)

Vision's manual states its identity outright: *"Vision is both a pattern and linear based
sequencer… or even a combination of the two."* The mechanism:

- **Sequence events are aliases** to sequences/segments, living in a parent sequence and
  describing *how to play* the referenced material. **Notorolla's tiles are sequence
  events** — independently reinvented. Vision's per-event parameters that tiles don't have
  yet: **Length independent of the referenced material's length** (play 8 bars of a 32-bar
  sequence), **loop count**, **per-event transpose**, mute, instrument override, Player
  assignment.
- **Make Segment / Unmake Sequence Event** is a *round-trip*: select inline track material →
  it becomes a referenced segment (replaced by an alias); unmake → the referenced material
  is placed back inline (matched by track name/instrument). Capture-to-reference and
  dissolve-to-inline as cheap, reversible verbs.
- **Reference bookkeeping:** the Sequences Window shows a **References column** (every
  parent that uses this material); unreferenced segments **auto-delete** (optioned).
  Command-click a window title for the **"heritage" pop-up** — the nesting ancestry of what
  you're editing. Nested sequence events are fully supported.
- Parent's meter/tempo governs referenced material by default; a sequence can opt out
  (Sync Mode Off = own tempo). A designated **Song Track** can push referenced tempo/meter
  up into the parent, with **Keep Sequences End-to-End** for chained song building.

### 3b. Generated Sequences (Ch. 14) — Vision's algorithmic corner

A special sequence class with a **Note track** and an optional **Rhythm track**, combined
at playback:

- **Attack Mode** (what timing drives the notes): Rhythm track / Note track's own / constant
  rate / random rate. **Duration Mode**: rhythm-track / own / constant / random /
  constant-gap / random-gap / percent / random-percent.
- **Order** per track: forward / reverse / alternate / random.
- **Velocity blend**: 0–100% interpolation between the Rhythm track's velocities and the
  Note track's.
- Explicit use cases: *ostinato patterns, random percussion, "superimpose rhythms from one
  track onto pitches from another."* Length measured in **events, not bars**.

The essence: **pitch material and rhythm material as orthogonal, independently-ordered,
combinable streams**. This is a direct ancestor of Notorolla's generative ambitions and a
concrete architecture for extending them.

### 3c. Players, Queue, and Trigger modes (Ch. 15) — performance of *structure*

- **9 Players**, each with its own **12-deep queue** of sequences; sequences trigger from
  key equivalents or MIDIKeys; per-sequence sync mode = trigger immediately, on-the-beat, or
  free-running at own tempo. Return stops everything; Shift-Return advances one Player's
  queue. (Proto-Session-view, confirmed — with *queueing*, which Ableton still lacks.)
- **Trigger/Transpose modes** (Control Bar or per-key-zone via Input Map, Ch. 34): MIDI
  notes trigger sequences, **transposed by the played note**; Gated mode ties playback to
  key-hold duration. Play a three-note chord → three transposed copies run simultaneously.
- **The killer detail: recording a trigger performance records *sequence events*, not
  notes** — basic / +sequence (additive) / gated / stop / transpose events land in the
  track. **Arranging by performing**; structure captured as structure, editable afterwards
  as aliases.
- **Input Effect**: a recordable arpeggiator/repeater (order modes, octaves, latch,
  grid-or-groove spacing, aftertouch→velocity).

### 3d. Select & Modify (Ch. 16) — selection as a query language

- **Rule-based selection**: attribute lines (position, pitch, velocity, duration, CC value,
  instrument, **position-in-bar**, **position-in-beat**, **between bracketed events**,
  **"position in every group of N is K"**) with conditionals (is / is not / ranges / or),
  combined via **Select / Add To / Refine** (set union & intersection).
- **Modify verbs** on any selection: transpose, change velocity/release-velocity/duration/
  CC-value, quantize (grid or groove), set density, reassign (CC↔bend↔aftertouch↔…), move,
  trim start, set instrument, substitute-from-clipboard, delete. **Double** applies the verb
  to a *copy* (named **Harmonize** under transpose).
- Whole configurations save as **templates**; Do-menu commands are just preconfigurations
  of this one window. Buttons work without focusing the window.
- **Groove quantize** blends timing *and* duration/velocity by weighted percentage
  (`X·D + (100−X)·Q`); grooves are user-creatable from any track (Ch. 20).

### 3e. Transpose types (Ch. 16) — including the non-12 precedent

Nine types: Chromatic, Interval, **Diatonic** (scale-degree, constrain-to-scale),
**Invert** (mirror around a pitch, chromatic or diatonic — dodecaphony-adjacent, resonates
with Triadulator), Key/Scale, **Auto Map — "N notes per octave → M notes per octave"**
(the manual's own examples: 24-per-octave → 12, semitones driving a quarter-tone-tuned
device, 128→1), Octave Map, Manual Map, Drum Map. **Vision already touched the tuning-seam
keyboard-mapping problem** (§4) — as a static pitch map. Revision generalizes it: the map's
codomain becomes tuning *degrees*, and it becomes live and mask-aware.

### 3f. Distilled essence (revised against the manual)

1. **Everything is referenced material with per-reference play parameters**, round-trippable
   between inline and referenced, with visible reference bookkeeping and nesting ancestry.
2. **Structure is performable and performances of structure are recorded as structure.**
3. **Selection is a query language; edits are uniform verbs on any selection**, with
   apply-to-copy and templates.
4. **Pitch and rhythm are separable, recombinable streams** (Generated Sequences).
5. **Multiple synchronized views** (Graphic/List/strip-chart/Notation/Pulse) of one
   selection; never stop the transport.
6. **Full MIDI vocabulary as model data** (CC, bend, aftertouch — drawable, listable,
   quantizable, reassignable). The main model extension Revision needs; today's model is
   notes-only.
7. **Groove** as first-class, user-creatable timing/velocity material.

Tension to resolve: Notorolla's grid is deliberately mono/one-octave and generative; Vision
is full-range polyphonic and performer-facing. These are **complementary, not conflicting** —
the grid stays the *generative* editor, the Vision layer is the *performance/refinement*
editor, both over the same beat-time, tuning-aware model. (And the tuning seam applied to a
Vision-style editor — a microtonal piano roll with per-tuning octave math — is genuinely new
territory; even Vision never had that.)

## 4. The two-instrument identity (2026-07-17)

User input that reframes the project: **the grid is a "Buchla"** — a deliberately alien
composition mechanism whose constraints (mono, one-octave, pattern/generative) drive the
user to create things they otherwise wouldn't. It works *because* it is not a keyboard.
The user's native composition mechanism **is** a keyboard, and Revision accommodates MIDI
in an absolutely first-class way.

Consequences:

- **The grid must not be "upgraded" by MIDI.** Its alien-ness is the value; wiring a
  keyboard into it would erode exactly what makes it generative. Design law: MIDI input
  never feeds the grid.
- **Two capture mechanisms → two kinds of material.** Grid-patterns (composed by
  *construction*, generative, mono/one-octave) and played sequences (composed by
  *performance*, polyphonic, full-range, full MIDI vocabulary). Both are first-class
  material in the same beat-time, tuning-aware arrangement. The Vision layer (§3) is
  precisely the editor for the second kind — the split isn't grid-editor vs. Vision-editor
  so much as **construction-instrument vs. performance-instrument**.
- **Cross-pollination is the interesting frontier** (each direction opt-in, never automatic):
  played material *distilled* into pattern-material (quantize/reduce a performance into the
  grid's constraint space, then let the generative machinery mutate it); grid material
  *performed against* (keyboard over looping patterns — also the live-performance story).
- **Third meeting point, straight from the manual (§3c): the keyboard as a *structure*
  instrument.** Vision's Trigger modes let a MIDI key launch a sequence transposed by the
  played note (chord → simultaneous transposed copies; gated by key-hold), and *recording
  that performance records sequence events, not notes*. Applied to Revision: keys trigger
  Notorolla patterns through the tuning seam, and a recorded trigger-performance lands as
  editable **tile events** in the arrangement. The keyboard plays the grid's *output*
  without ever touching the grid's *interior* — first-class MIDI that leaves the Buchla
  alien. This may be the deepest of the three meeting points.
- **A MIDI keyboard through the tuning seam is an open design question Vision never faced:**
  key number → tuning *degree*, not fixed 12-ET pitch. In 16-ET, 12 physical keys per octave
  ≠ 16 degrees per octave — mapping choices (linear degree-per-key vs. octave-preserving
  subsets vs. scale-mask-aware mapping) materially change what the keyboard *is* in non-12
  tunings.
- **Phasing implication:** MIDI **in** (playing the synths live, low-latency
  `noteOn/noteOff`) likely rises above MIDI-out-to-hardware as the Phase-1 heart — live play
  first, then capture (record/loop/takes), then the Vision editing layer over what was
  captured.

## 5. Product thesis — positioning (2026-07-17)

User framing: pitched as (e.g.) a Kickstarter, this cannot be Re-Ableton or Re-Reaper. It
must **bring back what disappeared, or what is now inconvenient or difficult** — and both
must provide **novel means of composition and production**. The pillars:

### 5a. A wholly microtonal DAW

Not tuning-as-retrofit — which is the entire current landscape (ODDSound MTS-ESP as a
bolt-on ecosystem; Ableton 12's tuning-file *lens*; Bitwig's micro-pitch device; plugins
accepting .scl) — but **degree-native throughout**: model, editors, harmony tools, and
instruments all parameterized by tuning. This is exactly Notorolla's tuning seam grown up:
`edo` is a property of the tuning, scale masks are EDO-tagged, pitch never bottoms out in a
12-ET assumption. **12-ET is just one tuning** — the mainstream case must work flawlessly,
so nothing is lost by the generalization; "12 is just another number."

- **Wholly microtonal instruments** — the Sethares tuning⇄timbre program: instruments whose
  partials track the tuning, so 16-ET (or anything) is *consonant because the timbre
  agrees*. A demoable magic trick no shipping DAW has. The existing synth stable
  (Padlington, Vesperia, …) ports into this role.
- **VSTs accommodated as best as possible** — per-track "tuning transport" tiering:
  (a) internal instruments: degree-native, perfect; (b) MPE-capable or MTS-ESP-aware
  plugins: excellent (per-note pitch); (c) plain 12-ET plugins: channel-rotation pitch-bend
  (the classical workaround; 16-channel polyphony ceiling, bend-range setup); (d) drum
  machines/samplers: often pass-through. The tier should be *visible product design*, not a
  buried compatibility hack.
- **Consequence flagged:** "accommodate VSTs" puts **plugin hosting in scope** — the single
  biggest engineering line-item this thesis adds (supersedes §2's "defer-able" lean; see
  §6). Sub-questions: CLAP-first hosting (Rust-friendly, native per-note expression) with
  VST3 via wrapper/later; plugin GUI embedding per platform (gnarly, unavoidable);
  MTS-ESP *master* licensing terms (ODDSound SDK) need investigating.

### 5b. Note-centric, not audio-clip-centric

The thing that disappeared: pre-audio sequencers (Vision, Performer, early Cubase) put the
**note model** first; modern DAWs are audio-clip engines with MIDI grafted on. Revision's
arrangement is **references to note-material with per-reference play parameters** (§3a),
performance captured **as structure** (§3c), editing power in the note domain (§3b, §3d).
Audio is initially a *rendering* concern — bounce/stems already exist. **Proposed corollary:
audio recording is deliberately out of v1.** A note-centric DAW that renders superbly is a
complete product; audio tracks are an expansion, not a foundation.

### 5b-bis. The Studio Vision precedent — audio's role (researched 2026-07-17)

Studio Vision (1990) was the **first product to integrate MIDI sequencing and digital
audio on one timeline** on a personal computer (using Digidesign hardware — Pro Tools
itself only appeared in 1991). The "anti-Pro Tools" reading holds up historically:

- **Pro Tools** = a tape machine/console emulation; audio as *regions on tape*; built for
  engineers; MIDI grafted on later.
- **Studio Vision** = a sequencer that swallowed audio; **audio events live inside the
  event model** — the Vision 4.5 manual (already reviewed, §3) shows audio events as
  selectable/quantizable peers of note events in Select & Modify.
- Its **patented audio↔MIDI conversion**: select a monophonic audio track → convert to
  MIDI → *edit as notes* → convert back. Pitch correction by MIDI editing — years before
  Auto-Tune/Melodyne. Audio didn't stay audio; it was invited into the note domain.
- Sound On Sound on its market position: it "served **composers and performers** seeking
  comprehensive MIDI+audio with deep editing control," favoring users who "go round and
  round tweaking and listening until it sounds right" — vs. Pro Tools' linear
  record-mix-print model. (This is verbatim the Revision target audience.)
- Late Studio Vision (4.x, 1998) hosted **VST effects and Premiere plug-ins**, supported
  ASIO — it was solving Revision's exact accommodation problem in the earliest VST days.
  Gibson bought Opcode in 1998 and killed everything by 1999; the lineage simply stopped.

**Consequences for Revision's audio scope** (answering "playback certainly; recording
level unsure"):

- **Audio playback: in, as events.** Imported audio (stems, phrases, loops) enters the
  arrangement as *events in the note model* — selectable, quantizable, tile-referenceable —
  never as a parallel tape-world.
- **Audio recording: scoped as phrase capture, not engineering.** Record a take as
  material — a phrase-event immediately available for conversion/distillation — rather
  than multitrack comping/punch console workflows. That's the level Studio Vision actually
  operated at, and it's the level a composer/performer needs.
- **The modern flagship: audio→degree conversion through the tuning seam.** Monophonic
  pitch detection is commodity now; *quantizing detected pitch to arbitrary tunings*
  (sing/play → 16-ET degrees → mutate generatively → render with tuning-matched timbres)
  is Studio Vision's patented trick reborn microtonal. Nobody has this either.

### 5c. Performance orientation — "things Ableton doesn't do"

Not out-Abletoning Ableton; doing what it doesn't: **queues**, not just triggers (Vision's
12-deep per-Player queues); **transposed triggering** from played notes (chord → transposed
copies); **structure recorded as structure** (parametrized aliases, not flattened clips);
nested references with reference bookkeeping; rule-based selection; generated sequences;
never-stop-the-transport. Plus the microtonal foundation under all of it.

### 5d. Hardware posture

Regular plug-and-play MIDI, regular audio drivers — but **modular**: an audio/MIDI HAL with
per-platform backends (WASAPI/**ASIO**/CoreAudio/ALSA), first-class-tested against the
user's own **RME / Yamaha** rig, amenable to whatever else. RME ASIO is the best-case
Windows dev target (and TotalMix already informs the control-skin language — convergent).

### 5e-bis. IP posture (2026-07-17)

User's read, concurred with: aside from literal copyright on the manual (its text/figures),
no significant IP survives around 1990s Vision. The audio↔MIDI **patents expired** years
ago (20-year terms on early-90s filings). Copyright never covered UI concepts, workflows,
or command structures (functional elements — the *Lotus v. Borland* tradition). Only
routine diligence remains: a **trademark clearance** pass on the eventual product name
(the "Vision"/"Studio Vision" marks are presumably abandoned, but check when it's real),
and don't ship their literal assets (text, pixel art). Replicating *anatomy and behavior*
with own styling is clean — and the control-skin restyling means zero trade-dress residue
anyway.

### 5e. The name

**Revision = re-Vision.** The codename is the pitch.

### 5f. Audience (honest read)

Xenharmonic community (passionate, underserved, currently duct-taping MTS-ESP into hosts
that fight them), the tracker/generative/Elektron-adjacent crowd, Vision nostalgics.
Niche-but-passionate is the *right* shape for crowdfunding and the wrong shape for
head-to-head competition — which the thesis explicitly avoids.

**The crowd map (2026-07-19).** Category language settled: **"compositional tool"** (user:
the phrase that always works — not "DAW," not "MIDI editor"). Framing: genre-defining
tools were never the pro tools (TB-303→acid, Auto-Tune→modern pop, FL Studio→trap,
Amiga trackers→jungle); electronica has a proven avant→mainstream wormhole (Aphex);
pitch = *the instrument the next traversal gets made on*. Microtonal-is-hot evidence:
Ableton 12 shipping tuning support (incumbent validation), King Gizzard charting with
quarter-tones, Jacob Collier's Grammy-winning microtonality heard as "goosebumps," and —
the strongest current case — **Angine de Poitrine** (Saguenay duo; quarter-tone
double-neck built by a luthier; KEXP session viral Feb 2026, 16M+ views; Polaris
longlist; NYT: "a marvel of rhythm, repetition, dissonance, surprise and noise"):
16 million listeners heard 24-EDO as *rhythm and surprise*, not as microtonality. Their
own self-description — "electro at the *structural* level, not the timbral level" — is
the loop-structure-plus-experimental-pitch aesthetic verbatim; and the custom guitar is
the hardware analog of the thesis (they needed a luthier; Revision is the luthier for
everyone else). Segments, each with its
door: (1) **xenharmonic community** — wants the degree-native host by name; highest
conversion; evangelists. (2) **generative-curious non-coders** — bounced off TidalCycles;
VCV Rack/Elektron/Fugue Machine prove the market; door: dice-without-code (§5g).
(3) **note-first composers / classic-sequencer veterans** — door: Vision reborn,
never-stop-the-transport. (4) **hardware-synth renaissance** — demonstrably crowdfunds;
door: librarian/editor resurrection ("your knob box becomes the programmer Roland never
shipped"). (5) **ambient/functional listeners** — myNoise crowdfunded successfully;
door: Systems/endless/Revision Radio. (6) **theory-YouTube orbit** — amplifiers, not
buyers; the product is unusually coverable (30-second matched-timbre demo). Funding
shape: software campaigns succeed when run like early-access with hardware-campaign
instincts — free player tier, founder licenses, **physical artifacts** (the printed
16-ET etude book, printed lead sheets of generated pieces). Tension held: category
creation is hard marketing — each segment gets anchored via *its* familiar comparison;
the umbrella stays ours.

### 5g. The algorithmic-composition limb (2026-07-17)

User: another limb of the project. Evidence from practice: **a lot of digestible
EDM-adjacent music from as few as a half-dozen patterns** transposed, reversed, stacked;
**"New Random" now produces genuinely listenable ostinatos** (no small feat).
Phrase-level generation: open question. Live coding (TidalCycles/Strudel/Sonic Pi/Orca
et al.) examined and found **unapproachable** — setup cost + notation cliff — whereas
Notorolla is *approachable*.

**Why New Random works at ostinato scale:** the constraint space does most of the
compositional work — mono, one octave, scale mask, short loop; randomness inside tight
constraints + instant audition = high hit rate. (The Buchla principle again: the
constraints are the co-composer. Vision's Generated Sequences, §3b, embody the same
separation of material from process.)

**The phrase question — key insight from the user's own practice:** the user *already
composes phrases* as arrangements of ostinati (transform + stack + sequence a few
patterns). So phrase generation shouldn't generate longer *material* — it should generate
at the level the user already composes at: **the operations**. "New Random Phrase" =
sample from a small form-grammar over existing patterns — sequences of (pattern,
transform) pairs with form archetypes (AAB, call/response, build/drop for the EDM
register), density/register arcs as targets. Very Vision: the generated thing is a set of
*sequence events*, not raw notes. Further rungs, in order of ambition: contour/tension
curves as generation targets (§3b pitch/rhythm orthogonality helps); constraint search
("a variation ending on degree 0 sharing ≥60% of onsets" — the long-anticipated Rust/WASM
combinatorial-search seam, §"Tech & constraints"); the pattern library as corpus
(Markov/grammar over the user's own kept material); and the **journal as taste data**
(§6e — keep/discard history is selection-pressure statistics; bias generators without ML
grandiosity).

**The live-coding diagnosis (product-relevant):** live coding chose *text as the
interface to process*; Notorolla chooses *direct manipulation as the interface to
process*. The community's own trajectory concedes the friction (Strudel exists precisely
to fix TidalCycles' setup wall — but the notation cliff remains). "Old and lazy" is
miscredited: setup friction is a real adoption cliff, and approachability is a design
achievement, not a compromise. **Positioning: the third leg of the thesis** — wholly
microtonal (§5a) + note-centric/performance (§5b/c) + **approachable generative** ("the
algorithmic DAW for people who won't install Haskell"). ES scripting (§6d) is the
graduation path for users who *want* text — an escape hatch, never the entry fee. Note
also: live coding is algorave/performance culture — Revision's §3c triggers/queues serve
the same desire without typing under stage lights.

**Boundary note:** this limb is the one Revision thread *not* orthogonal to current
Notorolla — phrase-level generation could be prototyped in the web app today (per normal
make-it-so gating), with Revision inheriting whatever the lab proves.

**The ambient/endless register (2026-07-17).** Two reference poles named by the user:

- **65daysofstatic's "Wreckage Systems"** — a years-long continuous stream of procedural
  music of decent quality (grown from their No Man's Sky generative-soundtrack work). The
  lesson: quality came from **musician-authored systems** — dozens of named, hand-tuned
  generative recipes over *composed* material — with the machine supplying combinatorics
  and endurance, not taste. (New Random's lesson at broadcast scale.)
- **myNoise.net** — the opposite pole: near-zero generativity, **exquisite curation and
  calibration** (superb source material + slider calibration + slow animation).

The spectrum: myNoise (all curation) ↔ authored systems (Wreckage) ↔ live coding (all
process). Revision sits natively in the authored-systems middle.

**Product consequence — the Player's second life (§6):** an **endless mode** is nearly
free given the architecture: patterns + transforms + form grammar + triggers/queues +
tuning-matched timbres = a "radio" that plays a project's material forever, mutating,
never repeating. A packaged **System** = material + grammar + tuning/timbre + mix — a
shareable artifact authored in Revision, playable by anyone in the free player. That's
Wreckage Systems *productized*: 65dos built bespoke infrastructure; Revision makes
"run your own Wreckage Systems" a save-as. (Commercial adjacency: the focus/sleep/ambience
market — Endel, brain.fm, lofi streams — is large and pays.)

**Why ambient is the low-hanging genre:** it *prefers* slow, non-teleological form —
drift over cadence — so it sidesteps exactly the directed-phrase problem that makes
generation hard; the user's EDM-adjacent loop register similarly. And **microtonal
ambient is a genuinely novel offer**: spectral consonance is *the* quality lever in
ambience (Sethares again) — tuning-matched 16-ET drones are territory nobody occupies.

**Marketing note:** a 24/7 "Revision Radio" stream generated by the product is
simultaneously a soak test, a dogfood proof, and the Kickstarter's living demo.

**Engineering cost check:** mostly already specced — adds long-run engine stability
(soak), smooth transitions (§3c queues/sync), seeded-vs-drifting determinism choice, and
myNoise-style macro-slider calibration (a control-skin natural).

**Timbre evolution — the loop's lifeblood (2026-07-17).** User's friend's observation:
personal *frisson* at a loop's timbre change; **an ostinato repeated 4–8× unchanged is an
introduction, but with a little variation it replays almost indefinitely.** This has both
practice and literature behind it: EDM's entire build vocabulary is filter movement over
an unchanging loop; dub techno (Basic Channel) is one chord under infinite spectral
evolution; Basinski's Disintegration Loops are timbre decay *as* the composition; and the
musical-chills research consistently ranks timbre/texture change among the top frisson
triggers. Consequences:

- **Timbre is a first-class variation axis**, peer of the note-domain verbs
  (transpose/reverse/stack). The §5g form grammar gains a concrete rule: **variation
  pressure** rises with repetition count (introduction budget ≈ 4–8 unchanged) and is
  dischargeable in *any* domain — note verb, timbre move, density, register. Cheapest
  discharge is usually timbral.
- **Per-reference timbre arcs** (extends §3a/§6g parameters-on-the-reference): "repeat
  this tile 16× with a slow brightness arc" is a property of the *reference*, not 16
  copies with automation lanes. Generated timbre paths = constrained random walks in
  patch-parameter space.
- **Enabling discipline already exists:** the energy-normalization gotcha (a timbre knob
  must not be a loudness knob — Vesperia's tilt is normalized) is precisely what makes
  *automated* timbre walks safe. Corollary: patches should expose **few, calibrated macro
  dimensions** (single-ADSR philosophy; myNoise's calibrated sliders) — low-dimensional
  spaces are what generators can walk musically.
- **The novel microtonal lever — consonance automation:** with tuning-matched partials
  (§5a), a timbre trajectory *is* a tension trajectory — brightness shapes roughness
  deliberately, so the tension curve can live in the spectrum without changing a note.
  Nobody has this; it is Sethares turned from a tuning trick into a *form* device.
- Engine requirement made explicit: block-rate (or better) parameter ramps and patch
  interpolation — patch parameters on continuous, morphable scales.

**"The very best way to roll the dice" (2026-07-17).** User's stated mindset: roll the
dice and listen; the tool's job is to make the dice *excellent*. Doctrine: author great
dice (structured gesture/operation vocabularies), great tables (constraint spaces —
coprime cycles, scale masks, calibrated timbre dimensions), instant audition, cheap
keep/discard (which also feeds the §6e taste journal).

**Seeded determinism — hard requirement (2026-07-17).** User: all "random" generation
must be pseudorandom and repeatable — the "coolest thing I ever heard in my life" that
just happened **must be recyclable**. Consequences:

- **Already forced by the journal, pleasingly:** §6e replay only works if commands are
  deterministic — so a generation event is a journaled command carrying its **seed +
  generator version + parameters + material refs**. Crash recovery and creative
  recycling are the *same mechanism*.
- **Store results, remember recipes:** on "keep," the roll's *output* is committed as
  material (rows) with provenance (seed/version) as metadata. Survival never depends on
  code archaeology; the seed exists for **re-rolling variations**, not for storage.
  Graceful degradation across generator versions: material always survives; exact
  re-rollability is best-effort per version (generator changes are versioned like the
  realizer, §5h-ter).
- **Splittable seed streams** (PCG/Philox-class, seed trees): per-lane, per-axis
  sub-seeds enable **selective replay** — keep the notes, re-roll only the timbre walk;
  same phrase, new articulation. The dice are *labeled*, each rerollable independently.
  (This is the power version of the requirement — vary one axis, hold the rest.)
- **UX: seeds visible and grabbable** — "again" (new seed) / "same" (replay) / "pin";
  seed-as-text, shareable. Seed + lead sheet = a complete generative artifact (the seed
  is part of the §5h-ter program). **Endless mode keeps a timestamped seed schedule** —
  "what was that at 3:41 AM?" is answerable; Revision Radio gets **seed permalinks**.
- **Engineering:** one owned PRNG in core (bit-identical native/WASM — the §6 conformance
  suite covers it); `Math.random`/OS entropy banned from generative paths; generation
  math kept integer/rational where possible so *the notes* are cross-platform exact
  (audio rendering may differ in last-bit float; the material must not).
- **Lab note:** New Random should adopt seeds in current Notorolla too (make-it-so
  gated).

**Case study — the 6-over-5 delay canon.** User discovery: a 6-note ⅛-note ostinato over
a 5×⅛ delay with feedback; articulation (hold one note, legato/staccato, accents) made it
almost endlessly fascinating. Why it works, precisely:

- **Coprime cycle lengths** (gcd(6,5)=1) make every echo return displaced by one step per
  pass — the super-cycle is LCM=30 steps from 6 notes of input. A held note's echo walks
  backward through the pattern, +1 shift per feedback generation: **a rotating canon
  where rotation index = feedback generation** (Reich's phasing by delay line;
  Frippertronics/Discreet Music lineage).
- **Tiny input, huge output:** the dice-space isn't the 6 pitches — it's the
  *articulation gesture* (per-note duration/velocity/rest), a low-dimensional,
  structured, walkable space. Articulation is thereby a **variation axis** peer to note
  verbs and timbre arcs (the §"variation pressure" list gains a fourth domain, arguably
  the cheapest of all).

**Algorithmic treatment "with thought":**

- **Note-domain echo as a model verb**, not only an audio effect: re-emit events at +D
  with velocity scaled by feedback — editable, generative, tuning-aware material (an
  audio delay stays available for the timbre-smear version; both are legitimate). The
  composable one is the verb. Bonus unique to Revision: **per-generation timbre** —
  echo generation n gets a patch-morph step n (canon voices that grow darker/softer/
  rounder as they recede) — ties directly into consonance automation.
- **Coprimality-aware dice:** the generator *knows* gcd/LCM — weights delay/loop pairs
  toward long super-cycles. (Same Z/nZ cyclic-group math the scale/triad machinery
  already lives in — rhythm cycles and pitch classes are the same algebra.) Multiple
  delays (5 and 7 against 6), or per-lane different D = polyrhythmic canon lattice.
- **Gesture vocabulary as the dice faces:** hold-position-k, accent set (Euclidean
  patterns are natural candidates), legato↔staccato ramps, rest insertion (a dropped
  note walks too). Dice weighted by variation pressure.
- **Performance link (§3c):** the discovery was made by *playing* — live articulation
  over a running loop is Input-Effect-adjacent, and recording it should capture
  *articulation-gesture events* (structure as structure, again).

**Lab note:** note-domain echo + coprime dice are prototypable in current Notorolla
(lane delay exists; the pattern/verb machinery exists) — same make-it-so boundary as the
rest of §5g.

**Articulation templates — Groove Quantize generalized (2026-07-18).** User: articulation
is incredibly important to phrasing, yet step entry ignores it — and EDM mostly works
without it, *which is why* one staccato bass note is shocking. Vision's Groove Quantize
(now ubiquitously imitated) should broaden to **more axes** — accent, time
(duration, early/late), **timbre** — and the user must **not be forced to notate
articulation at pitch-entry time**: apply existing templates, randomize templates, etc.

- **Vision's groove already blended three axes with per-axis weights** (timing +
  duration + velocity, `X·D + (100−X)·Q` — manual Ch. 16/20, grooves creatable from any
  track). The broadening adds: structured **accent patterns**, per-note **timbre
  offsets** (macro-dimension nudges — brightness accents; MPE-class per-note expression
  made *composable* rather than performed), articulation ratios (staccato/legato).
- **Doctrine: articulation is material.** Templates are named, reusable objects (rows);
  application is a **non-destructive per-reference parameter** (§3a lineage — Vision's
  play-quantize was already per-reference): tile = pattern ref + articulation-template
  ref + weight + seed. Blendable by percentage, bakeable, journaled, seeded.
- **Sources:** hand-authored; idiom presets (swing, push, laid-back, staccato-bass);
  **randomized under seeds** (§5g dice); and — the killer — **extracted from
  performance**: play the phrase once with feel, *discard the pitches, keep the
  articulation*. A new §4 membrane: the performer's feel becomes reusable material
  without their notes — keyboard-you phrases what Buchla-you constructed.
- **Coprime articulation cycles:** template length deliberately ≠ pattern length (5-slot
  articulation over a 6-note ostinato) = **phrasing super-cycles** — the delay-canon
  arithmetic applied to feel; the "little variation" that makes an ostinato replay
  indefinitely, mechanized at zero note-domain cost.
- **Anti-humanize doctrine:** uniform random jitter ("humanization") is noise, not
  phrasing — articulation is *information*, and in sparse genres it's scarce currency
  (the staccato bass shock = a rare articulation event carrying maximal information —
  scarcity economics in the articulation account). Templates support **sparse, placed
  application**: mostly-neutral with a few strong slots — not blanket wobble.

**The Mutator / Evolver (2026-07-18).** User naming for the variation building block —
with the defining principle: **it remembers the original. Musical evolution is not
stateless — a listener always expects the possibility of a return, while also weighing
the possibility that return will not happen.**

- **Two stances of one tool:** the **Mutator** is the point operation — one seeded roll
  along chosen axes (notes / articulation / timbre / density), one variation out. The
  **Evolver** is the Mutator run as a *process* — a trajectory of mutations over
  repetitions, scheduled by the pressure economy.
- **Lineage, not replacement:** every mutation references its parent — original at the
  root, variations as children, seeds + parameters on the edges. A browsable,
  revertible **family tree of material** (the §6e provenance machinery gives this nearly
  free; keep/discard = selection, taste journal = fitness memory — evolutionary
  computation with the user as fitness function, *anchored* rather than drifting).
- **The home vector:** the Evolver tracks **distance from the original, per axis**
  (note-domain edit distance, timbre distance in macro space, articulation distance).
  Departure opens a **return account** — the general form of the killing part's
  recurrence ambiguity: *any* departure creates return-uncertainty, and the uncertainty
  is the attention annuity. Return is a first-class move with variants: full return
  (the recap — payoff scales with distance × time away, same exchange law), partial
  return, **transformed return** (the original in new light — sonata recapitulation),
  and the **honorable non-return** (the account closed by ending — ambient's
  prerogative).
- **The governors differ by home-retention:** song keeps home in earshot (the account
  stays open; EDM always returns); ambient lets home dissolve (the original is *allowed*
  to be forgotten — homeless drift is the point). Theme-and-variations is the Evolver's
  oldest ancestor; rondo is periodic return (the hook stance); sonata is
  departure-development-return as architecture.
- **UI note:** distance-from-home is displayable (a home "compass"/elastic indicator on
  an evolving lane) — the performer sees how far out they are, which is exactly what a
  performing improviser tracks by ear.
- **Naming hygiene (§5e-bis):** fine as internal building-block names; note "Evolver" is
  a Dave Smith synth (and Mutator a filter product) — check marks if either ever
  surfaces as a product-level name.

**The pressure economy (2026-07-18).** User: "variation pressure" can carry a lot of
weight — *when* will the variation happen? *what* will it be? *should one you wait longer
for be bigger?* (Reference: the SNL "When Will the Bass Drop?" skit — comedy that works
only because the audience shares the expectation model.) Formalized:

- **Grounding:** Meyer (*Emotion and Meaning in Music*): affect arises from inhibited
  expectation. Huron (*Sweet Anticipation*/ITPRA): prediction confidence builds with
  repetition; tension is the anticipation account. Dopamine studies: the peak is at
  *anticipation*, not arrival. Pressure is real, just located in the listener; the
  generator keeps a proxy state variable.
- **The accounting model:** pressure is a **vector, one account per variation axis**
  (notes, timbre, density, register, harmony). It *accrues* with repetition × pattern
  predictability; it *leaks* through micro-variations; it can be *pumped* by explicit
  build gestures (risers/rolls are advertised accumulation — pressure made audible);
  it *discharges* through variation, sized against the balance.
- **WHEN:** discharge probability rises with pressure, timing snapped per idiom — EDM
  discharges on the countable 8/16/32 grid (**when-surprise ≈ 0; what-surprise carries
  the payload**); Haydn's Surprise Symphony is the reverse axis; ambient marks neither.
- **WHAT / exchange rate:** magnitude scales ~monotonically with accumulated pressure
  (the skit's law: a 10-minute tease owes an enormous drop) — **up to a ceiling, beyond
  which subversion becomes the honest payment**: the false drop, the cut to silence,
  half-time, a mask shift instead of the awaited return. The **deceptive cadence** is
  the classical proof that deliberate underpayment is *valuable precisely because the
  debt is real*. Deceptive discharges belong in the vocabulary — and only work atop
  genuine accounting.
- **Cross-domain discharge is the elegant move:** accrue in one account, pay from
  another — pressure built by *note* repetition discharged by a *timbre* move is exactly
  the friend's frisson (§ timbre evolution). Same-account payment reads obvious;
  cross-account reads sophisticated.
- **The two governors are pressure-economy settings, unified:** ambient = high leak,
  low accrual, unmarked timing (pressure never accumulates — why drift works);
  EDM = low leak, active pumping, grid-locked timing, large quanta. The Fugenator sits
  at scheduled-entries + stretto-as-pressure-trick.
- **Calibration:** exchange rates are genre parameters, and the §6e taste journal
  (keep/discard on seeded rolls) is the empirical calibrator — the user's own frisson
  as training signal, no ML grandiosity required.

**The rehearing correction (2026-07-18).** User: Huron headlines surprise, but most music
is heard more than once — **the anticipation of an *expected* event is what matters.**
Concurred; the cognition literature supports the reweighting (Bharucha's
schematic-vs-veridical expectation — the genre-statistical module never learns the
specific piece, which is why a known deceptive cadence still works; Margulis, *On
Repeat* — repetition converts listening into participation; the dopamine data put the
caudate's anticipation phase *before* the nucleus-accumbens peak: savoring is a distinct
neural stage). Design consequences:

- **Novelty is first-listen currency; craft is re-listen currency.** Optimize discharges
  for *relistenability* — legible approach, landmark-quality events, clean arrival —
  not for maximal first-hearing surprise. A keeper is by definition music that will be
  re-heard (the seeds requirement is this insight in engineering form).
- **Never repeat globally; absolutely repeat locally.** Endless mode has no re-listening
  — *except within the piece*: internal reprise creates veridical expectation inside one
  continuous stream. The form grammar must **plant recurring landmarks** so "here it
  comes again" happens even in music that exists once. Naive never-repeat generation
  forfeits the strongest pleasure available.
- **Predictable timing vindicated:** the EDM grid-locked discharge isn't a compromise on
  surprise — **the countdown is the pleasure**. Builds work *because* you know when.
- **The deep taste signal is replay count, not keep/discard:** how often kept material
  gets *re-played* (§6e journal already records it) measures relistenability directly —
  a strictly better calibrator than the initial keep decision.
- **Performance link:** triggering the drop yourself (§3c) is anticipation-of-known in
  its ultimate form — you *cause* the expected event. The performance layer and the
  pressure economy are the same subject from two sides.
- **The killing part (2026-07-18).** The awaited singular moment that "makes" a piece —
  no canonical Anglo theory term (nearest: *money note/moment*, *the payoff*, *the
  break*; K-pop's **"killing part"** names it precisely; "hook" is wrong — hooks repeat,
  this mustn't). Evidence for the rehearing correction: **listeners don't fast-forward
  to it** though skipping is free — the approach is part of the object. Design
  consequences: a System/form-grammar should support a **designated landmark** with
  *scarcity accounting* — a quota-limited, maximal-discharge event whose value is its
  rarity (loop it and it dies… though Kool Herc looping the break founded a genre, i.e.
  recycling the killing part is *sampling culture* — the §"seeds" requirement at
  culture scale). Generator implication: the dice should occasionally roll one
  extraordinary gesture and *spend it once*, rather than distributing quality uniformly.
- **"Once… or maybe twice… if you keep listening" (2026-07-18).** User's exemplar: the
  processed drum/sample events in Yes's *Owner of a Lonely Heart* (Trevor Horn, 1983,
  Fairlight-era production). Two additions to the model:
  - **Post-landmark vigilance:** the killing part doesn't just discharge accumulated
    pressure — **it opens a new account**: *will it come back?* Certainty either way
    closes the account (definitely-once = closure; every-chorus = hook, wrong register).
    **Recurrence ambiguity is an attention annuity** — quota ∈ {1, 2}, second placement
    late (last chorus = the honorable payment), never periodic.
  - **The killing part is often a *timbre object*, not a phrase** — in a note-centric
    song, the spectacular one-off is spectral (cross-domain discharge at maximum
    stakes). Revision's version: the generator occasionally synthesizes a **reserved
    sound** — a one-off patch/spectral event with quota accounting — not just reserved
    phrases. (Ties §5a instruments + timbre-evolution machinery.)
  - **The ORCH5 cautionary tale:** the Fairlight orchestra hit (sampled from Stravinsky)
    went scarcity → ubiquity → kitsch in ~three years; over-recycling killed the
    culture-scale killing part (the anti-Herc outcome). Scarcity accounting must also
    operate **across the library/System**, or the signature sound becomes the preset
    cliché.

**The pipeline — playing into 99% feedback (2026-07-18).** User technique, not
exhausted: play into a ~99%-feedback delay — soft attack/release via volume pedal — a
note, then a third, wait, a seventh… an ambient vibe accumulates. **Key phenomenology:
after a while you *visualize a pipeline* and start thinking about what to put in it.**
(Lineage: Frippertronics/Discreet Music — the tape loop as accumulator. The
visualization insight is the part with no precedent tooling.)

- **A third capture paradigm:** grid = construction; performance = playing in time;
  pipeline = **placement into a circulating buffer**. The performer's mental model
  shifts from "playing notes" to **curating a rotating population** — insertion,
  persistence, decay, eviction. Melody becomes harmony distributed in time.
- **Note-domain pipeline** (sibling of the note-domain echo verb): a loop of length D
  whose events carry per-pass decay (velocity × feedback each revolution; evicted below
  threshold). A pattern *with mortality* — contents always turning over, which is the
  ambient "uncomplicated AND interesting" property arising structurally (the pipeline
  auto-leaks pressure). The 6-over-5 canon is the special case insertion-length ≠ D.
- **The pipeline view — a novel editor:** show the buffer as a rotating circle/wrapped
  timeline with occupants fading by generation. "What to put in it" becomes *visible* —
  spatial reasoning about temporal music; you see the gaps. Touch/pen-natural
  (player-relevant). No DAW has this view.
- **Population-aware suggestion (the tuning platform cashes in):** the system knows the
  pipeline's current degree-population and can rank candidate insertions by
  consonance/roughness against it — *per tuning + timbre* (Sethares tables, the dual
  heuristic+analysis harmony machinery, applied live). "What should I put in?" gets an
  answer engine — assistive, never automatic.
- **Record insertions, not smear:** capture = insertion events (degree, articulation
  envelope, timestamp) — structure as structure; the session re-renders, edits, seeds.
  Audio-domain version stays available; per-pass filtering (generational darkening) is
  the natural timbre-evolution tie-in.
- **Lab note:** audio-domain version partially exists (lane delay); note-domain
  pipeline + pipeline view = new machinery, prototypable in the lab (make-it-so gated).

**The two governors — Fugenator and ambient evolution (2026-07-17).** User distinction:
**variation-as-evolution is distinct from overt "composition."** Two named ambitions:

- **The Fugenator/Etudenator** (user: perfectly achievable, dying to make it work) — the
  *composition* pole: fugue is the most rule-governed of forms, which is exactly why it's
  generatable. Its machinery is largely already in the model's vocabulary: subject =
  pattern; answer/inversion/augmentation/retrograde/stretto = **the transform verbs**;
  what's added is voice-leading/consonance constraints and *search* (the long-anticipated
  Rust/WASM combinatorial seam — this is its flagship customer). Precedent exists
  (species-counterpoint solvers, Ebcioğlu's CHORAL); the **novel move is microtonal
  counterpoint**: species rules are parameterized by a consonance ordering, and the
  Sethares tuning⇄timbre analysis *computes that ordering per tuning+timbre* — swap the
  consonance table and the Fugenator generalizes to any EDO. (The dual
  heuristic+analysis harmony machinery is precisely this table.) A fugue in 16-ET Mavila
  with matched timbres is unexplored territory in the strongest sense.
- **Ambient evolution** — the *evolution* pole: "music" that lasts very long while being
  **uncomplicated AND interesting**. The paradox resolves by register separation: low
  event-domain complexity, with interest supplied by slow sub-note processes — canon
  displacement, timbre arcs, consonance automation, generational decay. Eno's criterion
  is the acceptance spec verbatim: *"as ignorable as it is interesting"* (Music for
  Airports). And the **form mechanism is the 6-over-5 discovery scaled up**: Airports'
  tape loops were of incommensurate lengths — coprime cycles — so the same LCM machinery
  spans from 30 eighth-notes (frisson) to nested cycles of e.g. 30/41/53 minutes that
  don't realign for days (endless mode's skeleton).

**One machine, two governors:** same material model, same verbs, same dice — the
Fugenator *maximizes constraint density per event* (search-heavy), the ambient engine
*minimizes event rate and maximizes slow-parameter motion* (walk-heavy). The etude and
the EDM loop sit between the poles. Product framing: two presets of one generative
architecture, not two features.

### 5h. Notation view — modest vocabulary, pragmatic microtonality (2026-07-17)

User position: a **Notation view with a modest vocabulary is important**; for non-12-ET,
**"wing it"** — more staff lines, irregular line spacing where the scale's steps are
non-uniform, unconventional spellings, whatever works — because *"the rules with
microtonal notation are the same as with synthesizer notation — there are no rules
actually."*

- **The stance is realism, not philistinism.** Microtonal notation is a zoo of competing
  conventions (Sagittal, HEJI, Johnston, Ups-and-Downs, quarter-tone arrows…) with no
  consensus and active doctrinal wars; electronic-music notation has been ad hoc since
  Stockhausen's patch sheets. Shipping *pragmatic profiles* sidesteps a religious war the
  product cannot win and needn't enter.
- **The degree-native model makes notation pure presentation.** In 12-ET-native apps,
  pitch *spelling* is data (enharmonics agonized over); in Revision the datum is the
  degree — notation is a lens. Vision's own Ch. 30 phrase becomes the doctrine:
  **"changing appearance without changing data."** Any scheme is a skin; none can be
  wrong at the model level.
- **"Winging it" can be systematic without being dogmatic — derive the staff from the
  tuning:** a per-tuning **staff profile** generated from machinery core already has:
  *naturals = the current scale mask's degrees* (generalizing diatonic practice exactly —
  white-note naturals are just the 12-ET mask); *accidentals = off-mask degrees*
  (modest glyph set per tuning); *staff-line geometry = step structure* (irregular line
  spacing proportional to L/s steps — a Mavila[7] staff visibly *is* Mavila, the visual
  sibling of the grid's per-tuning octave rendering). Optional letterless profile with
  degree/hex labels (16-ET's 0–f naming exists already).
- **Engineering asset: SMuFL/Bravura.** The standard music-font layout already contains
  essentially every microtonal accidental anyone has invented (Sagittal, arrows,
  Stein-Zimmermann, …) as a free, well-maintained glyph library — "wing it" with
  professional glyphs.
- **Scope: a synchronized peer view (§3f.5), not an engraver.** Notes, rests,
  accidentals, barlines, ties, dynamics/articulation marks (the articulation axis §5g
  gets its natural display) — no engraving spacing, no lyrics, no Dorico ambitions.
  Same selection, same verbs. Interchange later via MusicXML/MEI *export* — export,
  don't compete.
- **Why it matters here specifically:** the audience is composers (§5f); and **the
  Fugenator's output begs for staves** — counterpoint is read, checked, and *printed* as
  notation (a book of 16-ET etudes is a real Etudenator artifact).
- **Ambition calibrated (2026-07-17):** a **playable score for someone who reads music,
  at the "basic" level most DAWs support** (Cubase/Logic score-view class — which is also
  exactly what Vision's Notation window was). Not engraving. The derived-staff profiles
  above are optional experiments on top of that baseline, not the baseline.

### 5h-bis. Isomorphic hardware & alternative idioms (2026-07-17)

User: doesn't own a **Lumatone** but sees one in their future if this proceeds; open to an
alternative notation idiom "hitting" them.

- **The Lumatone is the §4 keyboard-mapping question answered at the high end.** Its
  hexagonal Bosanquet–Wilson layout was designed (in the 1870s!) *specifically for non-12
  tunings*: isomorphic — same interval = same shape, any tuning, any key — so the
  12-physical-keys-vs-16-degrees problem simply dissolves into a lattice vector. Key→
  degree maps fall straight out of the tuning seam.
- **Per-key RGB lights make it a display surface, not just a controller:** light the
  current scale mask (mask degrees = "naturals," home row tinted) — **the grid's
  per-tuning rendering language, mapped onto hardware**. And the convergence is already
  in the codebase: **Notorolla's HEX-keyboard visualizer is unknowingly a Lumatone
  simulator** — the hardware is the visualizer made physical.
- Cheaper rungs on the same ladder: LinnStrument, Exquis, Striso, hex layouts on pad
  grids — all MPE, all lit. Lumatone-class support is disproportionately visible
  marketing: its owners ≈ the xenharmonic community ≈ the Kickstarter audience (§5f).
- **Alternative idioms are cheap experiments under the §5h doctrine** (notation =
  presentation, never data): isomorphic **tablature** (lattice/vector notation — "where
  on the Lumatone," the way guitarists read frets), degree-number notation (jianpu-style
  — the 16-ET hex naming 0–f *is* one), Klavarskribo-style keyboard-mapped staves, the
  step-proportional staff above. Whichever idiom "hits" costs a rendering profile, not a
  model change — so the lab can try them all.

### 5h-ter. The EDM lead sheet (2026-07-17)

User musing: "if I wrote my own lead sheets, what would I put on them?" → redirected to
**"how would I write an EDM lead sheet?"**

A lead sheet is a *minimal sufficient specification for realization within a shared
idiom* — melody + changes + form, with the genre's conventions supplying the rest. The
EDM translation changes what goes in each slot, because **EDM articulates form through
instrumentation and spectrum where common practice articulates it through harmony**:

- **The material slot:** a *pattern glossary* — the half-dozen ostinati (riff, bass,
  hook), a few bars each, in staff/grid notation (§5h profiles).
- **The changes slot:** harmony is often static/modal, so chord symbols become **mask +
  root** ("Mavila[7] on 0") — and the *changes* line instead tracks **texture states**:
  which lanes are in, bass register, filter state, plus event glyphs for spectral
  gestures (riser ↗, sweep, cut-to-break, sidechain depth). Timbre symbols occupy the
  chord-symbol slot because timbre is the form-bearing parameter (§ timbre evolution).
- **The form slot:** the block grid — 8/16/32-bar sections × lanes with in/out marks,
  under a single **energy curve**. Which is to say: **the tile arrangement already *is*
  the EDM lead sheet's form chart** — a print stylesheet away from paper.
- **Degree-native resonance:** the Nashville Number System is the proof that
  working musicians prefer *degree-based* charts (transposition-independent) — the
  EDM lead sheet's harmonic language is Nashville numbers generalized to any tuning.

**The unification: lead sheet = human-readable System spec, and the machine is the
continuo player.** Figured bass was exactly this — a compressed spec the player realizes
by convention. An EDM lead sheet in Revision is the printable serialization of a
form-grammar/System (§5g): readable by a producer, *realizable by the generative engine*
("lead sheet as prompt"). One artifact, two performers — human or Fugenator-class
realizer. (Also a superb physical Kickstarter artifact: printed lead sheets of generated
tracks.)

**The DSL framing (2026-07-17).** User: this is a better "music programming language"
than *cough* Haskell — the **"EDM Lead Sheet" as the domain-specific language.** The
position, named: *the best DSL for a domain is the domain's own working notation, made
executable.* The proof case is the **spreadsheet** — accountants' ledger notation made
executable produced more end-user programmers than every "real" language combined,
where "financial programming languages" all died. The lead sheet is music's spreadsheet;
TidalCycles is music's financial programming language.

- **It's already a language.** Glossary entries = named definitions; tiles = parameterized
  invocations (transpose, loop, length — §3a reference params are the argument list);
  the form chart = the top-level program; the §5g grammar/realization conventions = the
  standard library. The arrangement *is* the program; the lead sheet is its
  pretty-printed source.
- **Declarative, with progressive disclosure:** a sparse sheet realizes via defaults
  (the continuo player's training); density adds control. Sparse→dense is the learning
  curve — no cliff.
- **Bidirectional — the feature no live-coding environment has:** Tidal is write→hear
  only. Here, direct manipulation ⇄ sheet ⇄ engine are views of one model (the §6g
  dogfooding rule guarantees it). Generated music *emits its own source*; edited source
  realizes back. Roll the dice, read the program the dice wrote.
- **Syntax last:** the language is primarily visual (glossary + grid + curves + symbols);
  a *textual* serialization can follow for git/forum-sharing if wanted — the print view
  is the syntax. (Live coding failed approachability by being syntax-first;
  this is semantics-first.)
- **Discipline it implies:** realization conventions are part of the language spec —
  versioned (a sheet that realizes differently next year is a broken program), seeded
  where determinism matters. The §6d/§6 versioning obligations extend to the realizer's
  defaults.

## 6. The product family — code-sharing spectrum (2026-07-17)

User's vision, a spectrum of related code-sharing projects:

1. **The web-hosted platform continues** (possibly re-engineered, possibly mostly Rust —
   form open).
2. **A "player" version** — minimal UI, touch/pen operable, mobile/tablet.
3. **Mobile proper** — back-burner (UI decisions hard and not toward the primary goal),
   but **must not be blocked out** by architectural choices.
4. **First-class platform = Windows + Mac + Linux, all simultaneous.** Windows-first today;
   hardware for test/dev acquirable as needed.

### Architectural consequence: the core owns everything musical

The spectrum is only affordable if it is **one core, many thin frontends** — the §2
"evolution" lean, promoted to a product-family constitution:

- **Core (Rust)** — model (patterns, tuning, arrangement, event vocabulary, selection +
  verbs — i.e. the Vision-layer *semantics*), compiled **native** for desktop/mobile and
  **to WASM** for web. Today's `core/` (JS) is its spec; the **notch suite becomes the
  cross-implementation conformance suite** — port module by module with notch parity as
  the gate.
- **Engine (Rust)** — scheduler, voices, DSP. Native via `cpal` (WASAPI/ASIO/CoreAudio/
  ALSA-PipeWire; AAudio/CoreAudio on mobile later); on web, same DSP compiled to WASM in an
  AudioWorklet. One DSP source = the synths sound identical everywhere.
- **I/O + hosting layer** — audio/MIDI HAL, plugin hosting. **Native desktop only**; web
  and player builds simply omit it. Never let core or engine assume its presence (this is
  the rule that keeps mobile unblocked — iOS has AUv3, not VST; the player may host
  nothing).
- **Frontends (thin, per form factor)** — desktop UI (webview/Tauri), web UI, player UI,
  someday mobile UI. **No musical logic in any frontend, ever** — the discipline that makes
  each new frontend a UI project instead of a fork.
- **The project file format is the family's constitution.** With N frontends it is the
  shared contract; design it deliberately (versioned, forward-compatible, documented)
  *early* — currently "version 1" JSON, adequate for now, but it graduates from
  convenience to product surface.

### The player, reframed

A player = core + engine + transport — architecturally almost free. But given §5c
(performance orientation), the player shouldn't be passive: **the natural minimal UI *is*
the performance surface** — trigger patterns, queue sequences (Players & Queue), tilt/morph
controls; touch/pen suits triggering better than it suits editing anyway. "Player" ≈
*performance terminal* for Revision projects. Kickstarter shape: free player = distribution
and virality tier; the web build doubles as the try-before-anything demo funnel.

**Player UI identified (2026-07-19):** user call — Vision's **Players & Queue window,
replicated in full, is the player app's primary UI**. Its anatomy (phrase palette, N
player lanes, tap-to-queue, playing-vs-waiting display, stop-all / per-player advance)
is already a touch interface that happened to ship on a 1990 Macintosh; queue-centric
where Session view is launch-centric — the right model for conducting an arrangement
live and for the film-scoring pad (queues + states). Desktop gets it as a window
(requirements R-1014); the player *is* the view, full screen, touch-first per R-915.

### Platform notes

- **Windows-first is starting on the hardest terrain** (audio on Windows is the messiest:
  WASAPI vs ASIO, exclusive-mode quirks) — Mac (CoreAudio) and Linux (PipeWire) get
  *easier* from there, not harder. RME quality on Windows (§5d) makes the dev seat
  best-case while it lasts.
- **Linux as first-class is itself a differentiator** (no Ableton on Linux; audio-Linux
  users are chronically underserved and vocal — Kickstarter-relevant).
- Simultaneity risks to watch: webview divergence if Tauri (WebView2/WKWebView/WebKitGTK —
  test audio+UI on all three *early*, or fall back to Electron's bundled Chromium); ASIO
  SDK licensing (Windows); plugin-GUI embedding differs per OS (the hosting layer's
  gnarliest corner); CI cross-builds are cheap for Rust, but audio/MIDI *device* testing
  needs real hardware per OS.

### 6b. UI strategy — bespoke, minimal-dependency, low-level (2026-07-17)

User inclination: with AI agents able to reimplement the relevant slice of UI-library
functionality without the 75–98% that isn't relevant, go **minimal dependency and build
cross-platform UI again in relatively low-level code**.

**The industry's revealed preference agrees.** Every long-lived DAW ends up bespoke:
Reaper (WDL/Swell — an in-house mini-framework incl. a tiny Cocoa shim), Ableton, Bitwig,
Renoise, u-he and Serum on the plugin side. Ardour (GTK) is the cautionary counterexample.
The reason: a DAW's UI is **one big custom-drawn surface** — frameworks contribute window/
input/text plumbing and then get out of the way or get in the way. Notorolla's UI is
*already* fully bespoke (canvas grid/roll/tiles + the control-skin program); no widget
toolkit would express it anyway.

**Refining the AI-agent argument:** the leverage isn't extracting code from existing
libraries (licensing/entanglement); it's that agents collapse the historical cost of
bespoke — the **grinding 70%**: platform shims written 3×, API churn chased, the
two-hundred-line glue files (drag-drop, clipboard formats, per-monitor DPI, IME hookup).
That maintenance headcount was always the argument *against* rolling your own; it's the
part agents do best. Where agents help least: subjective feel (user eyeballs that anyway —
established workflow) and discovering undocumented platform quirks (paid for in calendar
time regardless of who types).

**Guardrails (the difference between bold and doomed):**

1. **"Minimal dependency" = no framework, not no crates.** Use the thin, boring,
   high-quality primitives the Rust ecosystem has already factored out — window/input
   (winit or an owned shim), GPU surface (wgpu or softbuffer), file dialogs (rfd). These
   *are* the "relevant 25%," pre-extracted, and individually replaceable later precisely
   because they're thin.
2. **Never hand-roll text.** Shaping, font fallback, IME, emoji — the classic bespoke-UI
   death march. Use cosmic-text (or harfbuzz). A project name in Japanese will happen.
3. **Accessibility decided early, not retrofitted:** AccessKit (Rust) exists exactly for
   bespoke UIs — exposes a platform accessibility tree (UIA/NSAccessibility/AT-SPI)
   without a framework. Reaper's accessibility reputation shows bespoke can be *good* at
   this when it's designed in.
4. **The owned layer is the widget/drawing kit** — where the control-skin laws live
   (lights glow / text never; mono-color readouts; vertical sliders; wheel-on-hover).
   That's product identity; owning it pays forever.
5. **Renderer abstraction with two backends: native (wgpu) and browser canvas.** The web
   platform is a family member regardless (§6), so the widget kit written against a small
   Canvas2D-ish drawing interface runs in *both* — turning "do UI all over again" into
   "do it once, portably." No commercial framework spans browser+native this way at this
   weight; the bespoke route is the *only* one that unifies the family's UI code.
6. **Native-native where it's cheap and mandatory:** menus, file dialogs, and plugin-GUI
   embedding (raw HWND/NSView — bespoke actually makes hosting embedding *easier*; every
   framework fights it).
7. **Sequenced by platform, like everything else:** Windows backend first; Mac/Linux shims
   are agent-portable later against the same widget kit and conformance eyeballs.

**Honest budget:** the widget kit is the fun 30%; platform glue is the grinding 70%. The
70% is deferrable (Windows-first) and agent-heavy, but it is calendar-real: DPI,
multi-window, Wayland-vs-X11, keyboard layouts, driver quirks — forty one-week problems.
The mitigation is that they're *independent* one-week problems, ideal for agent
parallelism, gated by hardware-in-hand testing.

**The level, precisely (2026-07-17):** *mechanism below, policy above* — the library
provides window/surface/input/text/a11y **mechanism**; widget *style and identity* belong
to the **application**, which may also elect a compatible native widget where it wants
one. Not lowest-level (not raw Win32 3×), but below any framework that owns widget
identity (Qt, Flutter — where scrollbar physics is *their* policy).

- **Native-widget interop is granular, and the granularity matters.** Cheap and correct at
  *window/transient* granularity: file dialogs, menus, IME, tooltips, and plugin editors
  (which are native-child embedding by definition). Expensive *inline*: a native control
  inside the custom-drawn surface hits the classic **airspace problem** (native children
  z-fight the drawn surface — famously unfixable in WPF), plus focus/tab-order and DPI
  mismatches. Expect "the app can choose native" to mean *at popup/window granularity*;
  inline mixing is the escape hatch most likely to stay unused.
- **The DOM observation, unpacked:** the browser converged on exactly this level
  (`appearance: none`, custom elements, canvas/WebGPU, CSS-as-own-policy) — mechanism
  without widget identity — which is *why* the control-skin program was buildable in it.
  And the Rust ecosystem has been quietly **unbundling the browser** into exactly the
  needed pieces: taffy (flexbox layout alone), cosmic-text (text stack), AccessKit (a11y
  tree), winit (window/input). "The relevant parts without the 75–98%" largely already
  exists as crates; agents stitch, fill gaps, and replace any piece that misbehaves.
- **Agreed endpoint: no browser linked into the executable.** CEF/Electron drags the 98%
  back in (weight, divergence, the uncanny web-app feel — wrong for a precision-audio
  product). The DOM's *relevant* export is its **contract**, not its engine — and the
  browser-canvas backend (§6b.5) keeps the web family member running on the real DOM
  while native runs the owned mechanism.
- **The mechanism contract should be written down early** — the services every widget
  assumes, so no widget invents its own: event routing (capture/bubble or simpler), focus
  model, hit testing, scroll semantics (the Scroll law lives *here*, enforced once),
  text-input-vs-keystroke distinction (IME), clipboard, drag-drop, a11y tree, layout
  helper, vsync/animation timing. This one document is the difference between a widget
  *kit* and a widget *pile*.
- **The "hands off the user's body" clause (2026-07-17).** Historical prompt: Vision
  *moved the mouse pointer* (to compensate for immature Mac submenus, c. 1990) — anathema
  to the platform; users eventually got an opt-out. The modern lesson, unified with
  Notorolla's Scroll Annoyance law: the app never moves the user's pointer, never steals
  focus, never scrolls unbidden — one contract clause covering all three. The *acceptable*
  modern descendant of pointer warping is **capture-hide-restore during drags** (infinite
  knob/slider drag via relative pointer mode) — opt-in/optioned, never surprise. Platform
  note: Windows/macOS permit warping (SetCursorPos / CGWarpMouseCursorPosition);
  **Wayland restricts it hard** (relative-pointer + pointer-constraints protocols only) —
  design to the Wayland-shaped hole and the other platforms follow for free.

### 6b-ter. UI notes from requirements capture (2026-07-18)

- **Terminology registers settled** (requirements §5): spec says *phrase / instance*;
  "pattern" = informal role-term for repetition-intent phrases (one model type — the
  anti-segment/sequence rule); UI labels map 1:1 to spec terms but may differ.
- **The tile idea, preserved:** whether or not the word "tile" survives into Revision,
  the *rendering* should — instances drawn as draggable tiles bearing **minified note
  glyphs** (mahjong-like; recognize your bassline at a glance). Vision's single-track
  sequence-event blocks showed miniature track data for the same reason. Candidate
  visual-identity element.

### 6c-bis. PoC architecture v0 (2026-07-19)

**Moved to [revision_poc.md](revision_poc.md)** (architecture, stages, lab/platform
posture) — that file is now the PoC's working document.

### 6c. Desktop PoC — the Control Bar slice (2026-07-17)

**Narrative moved to [revision_poc.md](revision_poc.md)** ("Why the Control Bar
slice"). Historical discussion below retained for the record.

User proposal: begin the desktop PoC by replicating, fairly closely, the basic controls of
manual **Ch. 3 (Vision Basics)** — e.g. the transport cluster. Ch. 23 (The Control Bar,
manual pp. 183–196) read in full; it strengthens the idea considerably:

**The Control Bar is a complete widget-archetype census** — the whole widget kit's
alphabet in one strip: pop-ups (Record Mode, Current Sequence/Track, Thru Instrument,
Current Patch, Sync, Marker); a **tri-state button** (Record: highlighted = recording,
*flashing* = armed for punch-in, grayed = nothing enabled); toggles (Begin Record
Wait-for-Note/Countoff, Punch, Loop); **editable multi-field numeric displays** (Counter:
bar•beat•unit large + SMPTE small, swappable, click-type *or* click-drag, editable during
playback; Tempo Display, conditionally editable; In/Out Points, grayed when inapplicable);
a **continuous controller** (Shuttle bar: variable-speed, center-relative, scrubs audio
when stopped); a **stateful button bank** (8 Locators: gray = undefined, Option-click set
— *on the fly during playback* — Option-Shift clear); window-opener buttons. Every key
equivalent documented (spacebar/return/tab/semicolon/comma/period/`[`/`]`/digits) —
keyboard-first throughout.

**Embedded behavioral laws worth replicating as mechanism-contract rules:**

- *"Buttons in the Control Bar are always active. Clicking one will not move the Control
  Bar in front of another window"* — non-focus-stealing global controls. This is exactly
  Notorolla's hard-won pointerdown-vs-click gotcha (the Set Reference bug), elevated by
  Vision to a design principle. In Revision it becomes mechanism-level physics.
- Loop mode = the never-stop-the-transport workflow: edit while looping; loop-*record*
  with **Enter = keep take, Delete = discard since last keep**, Cmd-Shift-↑/↓ = hop
  record-enable between tracks. (Takes ergonomics live in the transport, not a dialog.)
- The **Thru Instrument pop-up smuggles the §3c performance modes** (Transpose / Trigger /
  Cont Trig / Gated) into the corner of the transport — replicating the Control Bar's
  anatomy puts sequence-triggering at the UI's front door from day one.
- 480 ppq resolution; counter follows engine, is editable mid-playback.

**Not worth replicating:** Memory Display (an OS-9 artifact — its modern descendant is a
CPU/voices meter), SMPTE/MMC/external-sync guts (keep the Sync pop-up's *shape* as a
placeholder; defer the machinery).

**Why it's the right PoC: it forces the whole §6 architecture in one thin vertical
slice.** Transport ⇒ engine-owned clock; Counter ⇒ beat-time model rendering; Play/Stop/
Loop/In-Out ⇒ engine command protocol; Record indicator blink ⇒ MIDI-in monitoring;
widgets ⇒ mechanism contract + control-skin kit. Milestones: **(a)** mechanism layer opens
a window, draws the bar, routes input (Windows first); **(b)** widgets live — counter
edits, shuttle drags, locators store on the fly; **(c)** the slice closes — Rust engine
clock + one ported voice plays a Notorolla pattern; counter follows engine time; loop
In/Out works. Deliverable: **Vision's cockpit, restyled per control-skin law, actually
playing 16-ET Notorolla material** — which is also the Kickstarter teaser image.

### 6d. Language & runtime decisions (2026-07-17)

**Systems language: Rust, confirmed** (user: the only acceptable low-ish-level choice for
new greenfield work, especially real-time). One refinement to the rationale: in a
pro-audio engine the RT thread's goal is to allocate **zero** times per second — pre-allocated
voice pools, lock-free queues (rtrb/crossbeam), no locks/allocs/syscalls in the callback.
Rust's real win is that this discipline becomes *enforceable structure* (ownership +
Send/Sync partition the RT world; `assert_no_alloc`-style guards in debug) rather than
C++ team folklore. The 10^n-allocations-per-second world — model edits, undo, generation,
UI — lives off-thread, where Rust gives that speed *without* a GC whose pauses would need
explaining to a deadline-bound engine.

**Plugin hosting, ecosystem state (more advanced than assumed, but asymmetric):**

- *Being* a plugin in Rust is mature — nih-plug (CLAP+VST3 framework), clack/clap-sys,
  vst3 bindings. *Hosting* is the thin side: **CLAP hosting is viable in Rust today**
  (CLAP is a clean C ABI, MIT-licensed; clack has host support). **VST3 hosting is where
  the C++ glue goes** — the SDK is C++, bindings are immature, and licensing is
  GPLv3-or-Steinberg-agreement (a real diligence item).
- **Recommendation: host plugins out-of-process from day one.** The C++ VST3 glue becomes
  a small separate bridge executable (shared-memory audio + IPC events); the Rust engine
  process stays pure and *crash-proof* — a misbehaving plugin can't take down the
  transport (Bitwig's beloved per-plugin sandboxing, and exactly the §5a "accommodated as
  best as possible" posture). Crash isolation, language isolation, and license isolation
  from one architectural choice.

**Scripting language: ES over Python** (user preference; concurred, with reasons beyond
the Adobe precedent):

- **Family coherence is the clincher:** the web platform member runs user scripts on the
  browser's own ES engine *natively*; the current codebase is JS, so the model's spec
  language is already ES-shaped; one scripting dialect across web/desktop/player.
- **Music-software precedent is specifically strong:** Bitwig's controller API is JS;
  Logic's MIDI Scripter is JS; Adobe (ExtendScript→UXP) per user. (Lua is the usual
  counterargument — tiny, fast — but loses on family coherence; Python loses on embedding
  weight and distribution.)
- **Engine candidates:** QuickJS(-ng) — tiny, interpreter-only, easy to sandbox; Boa —
  pure Rust, younger; V8/JSC — heavy, only if perf demands. Note: **interpreter-only is a
  feature** — iOS forbids JIT, so a QuickJS-class engine keeps the mobile family member
  unblocked (§6 rule).
- **Where scripting plugs in:** user-defined Select & Modify verbs and selection rules
  (§3d), generative operators (§3b), controller-integration scripts (Bitwig-style), export
  processors. **Never on the RT thread** — scripts emit beat-domain data ahead of the
  clock (the beats-not-seconds seam makes this natural); the engine consumes, scripts
  never touch the callback.

### 6e. Persistence — store-primary, crash-only (2026-07-17)

User requirement, from lived experience (Vision crash c. 1990 → dropped into TMON, walked
the PC back to the event loop, saved the recovered heap — song forever named "Saved by
TMON"): the current project is **continually saved to something durable (SQLite)**; the
web app's always-reloadable localStorage behavior is a feature to *keep and promote*, not
a testing convenience to outgrow.

**Principle: store-primary.** The ground truth of a project is a durable store,
continuously updated; there is no "unsaved state." In-memory model = cache; files =
interchange. "Save As" becomes *naming/exporting a version*, not preservation. (Most DAWs
bolt autosave/recovery onto a file-centric model; making the store primary is rarer and
better — and on mobile, where the OS kills apps freely, it's mandatory: another
don't-block-mobile rule honoring itself.)

**SQLite is the right store**, and officially so — "SQLite as application file format" is
a documented, encouraged pattern (sqlite.org; Fossil precedent). WAL mode gives atomic,
power-loss-safe commits; writes scale with *edit size*, not project size (vs. rewriting a
JSON blob); rusqlite is mature. **Family win: official SQLite WASM + OPFS runs the same
store in the browser** — one persistence layer across native and web members.

**Natural architecture: command journal + snapshots.** Model mutations are already
commands (the undo system). Persist the journal continuously (append-only, transaction
per gesture, batched off the UI and RT threads); snapshot/compact periodically. Cascading
benefits:

- Crash recovery = replay journal since snapshot — boring, invisible, total.
- **Undo history survives restarts** (persistent undo — rare and loved).
- Session time-travel ("the project as of yesterday") nearly free.
- For a generative DAW the journal is *creative* material — "how did I get here" replay.
- Rule: journal stores model-domain commands (beat-domain, tuning-degree terms), never
  derived state.

**Two-layer split (ties to §6 file-format-as-constitution):** the working store (SQLite,
implementation detail, schema migrations allowed) is distinct from the **interchange
format** (the versioned, documented project file — the family contract). Store is private
truth; interchange is public promise.

**The anti-pattern, named (2026-07-17): Cubase autosave.** File-centric autosave sprays
`.bak` copies that the user must trim; done wrong it litters dozens per project, with no
clear "which is truth." The pathology is structural: when the file is truth and file
writes aren't atomic, *safety = making copies*, and copies = clutter + ambiguity.
Store-primary removes the **reason** copies exist: the journal *is* the safety, compaction
is automatic and invisible, retention is a bounded policy rather than a user chore, and
the only user-visible artifacts are **deliberately named versions** — queryable rows, not
directory litter. Corollary rules: the user never sees or manages an autosave artifact;
backup = copy one file (SQLite's hot-backup API even allows it mid-session).

**On "SQLite is probably more reliable than the filesystem it's built on"** — genuinely
defensible, not hyperbole: 100% MC/DC-tested, billions of test cases with crash and
power-loss injection, and its own docs ("How To Corrupt An SQLite Database File") show
the residual risks are almost all filesystem/hardware sins that SQLite defends against
more carefully than application code ever does. Single-file also eliminates the
multi-file-consistency problem that autosave-copy schemes *are*.

**Open sub-question:** is the SQLite store (a) a hidden per-project working DB with the
interchange file as the user-facing artifact, or (b) *the project file itself* (the full
sqlite-as-app-file-format doctrine — one durable artifact users move around), with
interchange export alongside for the web member and third parties? Lean (b) for
user-model simplicity; web/OPFS silo is the complication to think through.

**Acceptance test, named for the occasion — the TMON test:** `kill -9` at any moment,
during playback, mid-gesture: reopen loses zero completed gestures and at most a bounded
few seconds, undo stack intact. No song shall ever again be named "Saved by TMON."

### 6f. Notes as rows — the queryable model (2026-07-17)

User observation: with SQLite as the project store, notes-as-rows makes the store a
**high-performance query surface**, not just persistence — an interesting way to read and
manipulate the material.

**The headline convergence: Vision's Select & Modify (§3d) was a query language without a
database; Revision has the database.** The mapping is nearly verbatim:

- Attribute lines → `WHERE` clauses (pitch/velocity/duration/position ranges).
- "Position in every group of N is K" → window functions (`ROW_NUMBER() % N`).
- "Between bracketed events" → self-joins / window frames.
- Select / Add To / Refine → set ops over a **selection table** (event-id temp table).
- Modify verbs → `UPDATE … WHERE id IN (selection)`; **Double/Harmonize** → `INSERT …
  SELECT` with the transform applied. Templates → stored, parameterized queries.
- Scripted verbs (§6d) → ES generating parameterized queries through a safe API; power
  users could eventually get raw read-only SQL over their own song.

**What SQL adds that Vision never had:** analytical queries over the whole project —
pitch-class histograms per section, density maps, "every place two lanes collide within 2
degrees," tuning-aware aggregates. A DAW has never offered this because its model lives
in RAM structs; here it falls out of the store. External inspectability too: `sqlite3` CLI
on your own song (very much in the Notorolla plain-files ethos).

**Schema sketch:** `event(id, track, kind, at_ticks INTEGER, dur_ticks, degree, velocity,
…extra JSON)` — **integer ticks (480 ppq per Vision) not floats**; indices on
`(track, at_ticks)`; SQLite's **R-tree module** for beat×degree viewport/hit queries; FTS
on names for free.

**The honest tension and its resolution:**

- The **RT engine never queries the store** — it consumes compiled, cache-friendly
  schedules (already the §6 model/engine split; unchanged).
- The **editor hot path** (drag feedback at display rate) doesn't round-trip SQL per
  frame; transient gesture state lives wherever cheap. The elegant unification:
  **gesture = transaction** — mid-gesture state is uncommitted; commit on release;
  Escape = `ROLLBACK`. The DB is authoritative at every commit boundary; caches (spatial
  hit-test structures) are rebuilt/invalidated via SQLite's update hooks.
- Dual-representation drift is *the* risk (memory model vs rows). Rule: mutations flow
  through commands (§6e journal) which are the *only* writers; reads may hit SQL freely.
  One writer path = no drift.

**Precedent check:** Audacity's .aup3 uses SQLite as project store (audio blobs — mixed
reception, different problem); **notes-as-queryable-rows in a DAW appears to be genuinely
novel.** Scale reality: 10³ events trivial, 10⁵–10⁶ comfortably within SQLite query
performance; the costly path (recompile-to-schedule on edit) exists regardless of SQL.

### 6f-bis. Transforms as views — SQL as executable specification (2026-07-19)

User: views/CTEs/window functions could be high-leverage — many transforms are
notionally views; performance unknown. Resolution: **the two-tier strategy** — SQL
views are the *executable specification*; hand-optimized Rust twins replace them only
where the planner groans, **property-tested against the views forever** (the view
remains the oracle after it stops being the implementation). Performance never has to
be decided up front.

- **Instance realization is a query:** transpose = expression; looping = join against
  `generate_series`; length = WHERE clip; **nesting = recursive CTE** — and R-422's
  composition law is the CTE's accumulator while R-407's cycle prohibition is its
  termination guarantee. The model's structural laws are exactly what make the
  realization query well-defined: the model is relational at heart.
- **Bake = `INSERT … SELECT` from the realization view** — apply-vs-bake equivalence
  (R-422 testing) compares a view with itself.
- **Window functions as the analysis drawer:** `LAG/LEAD` = melodic intervals/contour
  (home-vector distances); `ROW_NUMBER() % N` = group selections; running sums =
  density/pressure proxies; accent-periodicity queries = R-420's meter inference.
  Articulation templates join on `position % template_length` (coprime super-cycles
  from the modulo). Consonance tables (R-512) as literal tables joined for triad
  detection (R-516). Where-used (R-411) and taste stats (R-1113) are one-liners.
- **Honest boundaries:** seeded generation stays in Rust (R-1101 — dice out of SQL;
  output lands as rows); RT thread never queries (unchanged); gesture-rate caches
  remain caches; deep view stacks mitigate via temp-table waypoints (SQLite has no
  materialized views).
- **Bonus:** introspectability now includes the *realized* arrangement — `SELECT` what
  will actually sound.

### 6g. Why Cubase/Reaper power feels model-less — and the dogfooding rule (2026-07-17)

User observation: Cubase's MIDI scripting/editing is sophisticated (Reaper's too), but the
underlying model doesn't feel clean. Diagnosis — two distinct diseases, same root:

- **Cubase: four overlapping mechanisms, none composing.** Logical Editor (Select &
  Modify's Atari-era sibling), *Project* Logical Editor (a separate near-duplicate for the
  project domain), Input Transformer (a third near-duplicate at the input stage), MIDI
  Modifiers/plugins (non-destructive, but a different paradigm again), and real scripting
  only for MIDI Remote controllers. Each powerful; no shared selection model, no shared
  verb set, results don't flow between them. Power by accretion of islands.
- **Reaper: one mechanism exposing raw guts.** ReaScript is thousands of API functions
  over internal C++ structs — index-based note access, and famously **state chunks**
  (string blobs you *parse* to reach some state). Immensely capable, brittle by design;
  the "model" is whatever the structs happen to be this version. Power by surface area.
- **Common root: the scripting/editing surface was bolted onto a private in-RAM object
  soup that was never designed as a public contract.** Both prove *demand* for deep
  programmable editing; both show the cost of retrofitting it.

**Revision's structural answer — the dogfooding rule:** there is **one model surface**
(rows + selection queries + command verbs), and the **built-in editors are clients of it,
with no privileged backdoor**. The Select & Modify UI compiles to the same queries scripts
issue; the piano roll's drag emits the same commands a script would; undo, persistence,
and sync all speak the identical vocabulary. Cleanliness stops being an aesthetic and
becomes structural: the surface stays honest *because the app itself must live on it*.
(Bonus: the SQL schema is self-documenting in a way a thousand accessors never are.)

**Where non-destructive transforms live** (the Cubase MIDI-Modifiers question): Vision's
answer is the clean one — **parameters on the reference** (§3a: per-sequence-event
transpose/length/loop/quantize), not effect-stacks on the track. Same power, but the
transform is attached to the thing it transforms, visible where you'd look for it.

**Cost acknowledged:** a public-contract model demands schema/API versioning discipline
(already flagged for the file format §6 and scripting API §6d). Reaper's mess is partly
why it ships fast; the bet is that contract discipline pays over a decade what it costs
in a quarter.

## 7. Decision points for discussion

1. **Fork or evolution?** Answered by §6: neither — a **family**. One Rust core, many thin
   frontends (web, desktop, player, someday mobile); notch as the conformance suite during
   the port; the project file format as the shared contract.
2. **Engine language:** answered (§6d) — **Rust**, with C++ confined to an out-of-process
   VST3 bridge; CLAP hosted natively; ES (QuickJS-class) for scripting.
3. **Scope of "DAW":** partially answered by §5 — plugin hosting is **in** (as microtonal
   accommodation, tiered); audio recording **proposed out of v1**. Remaining: confirm the
   audio-recording call; CLAP-first vs VST3-first hosting order.
4. **What is *your* Vision essence?** Which of §3's decomposition mattered most in practice —
   and what did it have that isn't listed?
5. **Shell choice:** Tauri (lean, Rust) vs. Electron (bundled Chromium, predictable Web
   Audio). Current lean: Tauri, validated early by an audio smoke test in each OS webview.

---

*Notes maintained as the discussion evolves. No implementation implied by anything above.*
