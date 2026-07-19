# Revision — Proof of Concept plan ("revision_poc")

Status: planning notes. Companion to `revision.md` (discussion/rationale) and
`revision_requirements_v1.md` (normative requirements). Same ground rules as both:
completely orthogonal to Notorolla work; discussion precedes changes; implementation
only on "make it so."

## Target

A Windows desktop application that:

1. brings up a window containing the **Control Bar slice** (transport cluster);
2. plays **"Mary Had a Little Lamb"** through a genuinely ported voice via the Rust
   engine;
3. **records two MIDI tracks** from a hardware controller and replays them;
4. party trick: the same tune with a one-line tuning swap to **16-ET Mavila** —
   audible proof the plumbing is degree-native (R-002) from the first build.

The PoC honors every [Arch] requirement in miniature. Its code is intended as the
product's actual foundation, not a throwaway — the [Arch] discipline is the point.

## Why the Control Bar slice

(Consolidated from revision.md §6c.)

- **A complete widget-archetype census** in one strip: pop-ups (Record Mode, Current
  Sequence/Track, Thru Instrument, Sync, Marker); a tri-state button (Record:
  highlighted / flashing-armed / disabled); toggles (Wait-for-Note/Countoff, Punch,
  Loop); editable multi-field numeric displays (Counter: bar•beat•unit + SMPTE,
  swappable, click-type or click-drag, editable during playback; Tempo; In/Out
  points); a continuous controller (Shuttle bar, variable speed, scrubs when
  stopped); a stateful button bank (8 Locators: gray = unset, set on the fly during
  playback); window-opener buttons. Building it validates the widget kit's whole
  alphabet against controls that shipped for a decade.
- **Embedded behavioral laws worth replicating as mechanism-contract rules:**
  - *"Buttons in the Control Bar are always active; clicking one will not move the
    Control Bar in front of another window"* — non-focus-stealing global controls
    (Notorolla's hard-won pointerdown-vs-click lesson, elevated to physics).
  - Loop mode is the never-stop-the-transport workflow: edit while looping;
    loop-record with Enter = keep take, Delete = discard since last keep.
  - The Thru Instrument pop-up carries the performance modes (Transpose / Trigger /
    Cont Trig / Gated) — sequence-triggering sits at the UI's front door from day one
    (shape ships in PoC; behavior later, R-1005).
  - 480 ppq in Vision; 5040 in Revision (R-003). Counter follows the engine clock and
    is editable mid-playback.
- **Not replicated:** Memory Display (OS-9 artifact; its modern slot becomes a
  CPU/voices meter later), SMPTE/MMC/external-sync machinery (Sync pop-up keeps the
  *shape* as a placeholder; R-614 is [Later]).
- **Why it is the right PoC:** it forces the whole architecture through one thin
  vertical slice — transport ⇒ engine-owned clock; Counter ⇒ beat-time rendering;
  Play/Stop/Loop/In-Out ⇒ command protocol; record light ⇒ MIDI-in monitoring;
  widgets ⇒ mechanism contract + control-skin kit.

## Architecture v0

**Crate map** (Cargo workspace; boundaries mirror the product-family constitution,
revision.md §6):

| Crate | Contents | Key requirements |
|---|---|---|
| `core` | pure model: events, phrases, instances, tracks, ticks/tempo map, tunings, commands | R-001–003, R-4xx, R-5xx; WASM-able (R-104) |
| `engine` | cpal duplex, engine-owned clock, scheduler, voices, graph runtime (Web-Audio-semantics nodes), zero-alloc RT | R-301–305, R-704, R-1501 |
| `store` | rusqlite, journal, snapshots, TMON behavior | R-201–205, R-808 |
| `midi` | midir wrapper: enumeration, driver-boundary timestamps, thru | R-601–607 |
| `ui-mech` | winit + wgpu + cosmic-text; the mechanism contract | §6b; R-907 |
| `ui-kit` | control-skin widgets | R-712 rendering side |
| `app` | composition root: wiring, command dispatch, view state | — |

**Threads:** RT audio callback (zero-alloc), UI/main, MIDI callbacks, async store
writer (R-1509). Lock-free rings between (rtrb-class).

**The MIDI fork at birth:** input forks immediately into a fast path → engine (thru →
voice: the *live path*, R-304) and an event path → app (capture → journal). The
live/playback classification exists from the first build so it is never retrofitted.

**Dataflow.** Playback: store rows → app compiles schedule (ticks → seconds via tempo
map) → engine consumes → graph voice renders. Record: timestamped MIDI → thru path
sounds now; event path journals as commands → direct events on a track (R-807) →
recompile → replay. Capture is journaling, so recording is TMON-safe (R-808) by
construction.

## Timing engine sketch (2026-07-19)

- **The callback is the clock:** no timers; authoritative time = sample count advanced
  by the cpal callback (R-302 concretely). Musical position derives via tempo map.
- **Callback duties, allocation-free:** drain command ring (bounded) → advance
  scheduler → render voices → publish telemetry. All state pre-allocated; SPSC rings
  (rtrb) in/out; **garbage ships back over a ring** (RT never frees); debug builds run
  under an allocation guard.
- **Scheduler consumes sample-stamped events** compiled by the app (ticks → samples
  via tempo map), dispatched sample-accurately within blocks (block splitting /
  intra-block voice offsets). **Polytempo is free here:** N tempo streams = N compiled
  lists merged in sample time; the RT side never knows tempo exists (R-416).
- **Live path = ring + ≤1 block** (R-1501 falls out of the topology).
- **Clock-domain correlation** (OS timestamps ↔ sample time) is the fiddly bit —
  dedicated module + tests from day one; it underwrites R-603 and R-814 alignment.
- **Callback always completes:** budget accounting, voice stealing before deadline
  miss, starvation → silence + telemetry, never a block. Transport is a small RT state
  machine; loops arrive as loop-aware compiled windows.
- Rust's contribution: the discipline is *structurally enforced* (ownership/Send,
  ring-only communication), not code-reviewed for.

## Stages

Ordered sound-before-pixels: risk (timing, audio) front-loaded; labor (widgets)
deferred. Each stage independently demoable.

**Stage 1 — `core` + `store`, headless.**
Phrase/track/command model skeleton; journal; snapshot; JSON serialization.
*Exit criteria:* create material headlessly; kill the process mid-write; reopen
intact (TMON v0, R-202/R-1504). SQLite↔JSON round-trip test green (R-203/204 seed).

**Stage 2 — `engine`, headless. First sound.**
cpal duplex stream opens (R-301 even though input is unused); graph runtime v0
(oscillator/PeriodicWave, gain, parameter ramps per W3C math); one voice; schedule
compiler.
*Exit criteria:* MHALL sounds from a hardcoded schedule with no UI; offline render of
the same schedule matches live structurally; RT callback verified allocation-free.

**Stage 3 — `midi` thru. Live playthrough.**
Enumeration, timestamps, thru routing to the engine voice; dual-path fork in place
(capture path logging even before recording exists).
*Exit criteria:* play the voice from the Code/Oxygen; honest end-to-end latency
measured and printed (R-307 v0). Stretch: one knob mapped to a voice macro (early
R-917 taste).

**Stage 4 — `ui-mech` + `ui-kit`. The Control Bar.**
The mechanism contract v0 is *written as a document first* (§6b's artifact: event
routing, focus, hit testing, scroll law, text-input distinction, a11y posture), then
window + widgets, then the slice.
*Exit criteria:* transport buttons drive the engine; counter follows the engine clock
and edits mid-playback; key equivalents live; always-active behavior holds (no focus
steal, no pointer warp, no unbidden scroll — R-907).

**Stage 5 — Record/replay. PoC complete.**
Arm, capture (replace + overdub), journaled during capture, replay; loop record with
keep/discard if cheap.
*Exit criteria:* two MIDI tracks recorded from hardware and replayed; kill -9
mid-take loses nothing committed (R-808); the 16-ET party trick demonstrated.

## Core schema sketch v0 (2026-07-19)

Stage 1's blueprint; designed alongside its realization view (revision.md §6f-bis).

**Tables:** `meta` · `tuning` (embedded — PoC resolves R-506 toward self-containment;
product question stays open) · `phrase` (explicit length R-401; nullable tuning
R-414; provenance columns origin/seed/parent now — cheap, R-413) · `event` (one
container: phrase XOR track, CHECK-enforced; kind + JSON `extra` for R-402 growth) ·
`instance` (one container: track XOR parent-phrase — R-407 nesting; R-405 params +
`extra`) · `track` (belongs to a root phrase; multi-track sub-phrases deliberately
deferred, not foreclosed) · `tempo_map` · `journal` (seq, gesture, redo/undo payloads —
the only write path; R-205/R-808) · `snapshot` (compaction waypoints).
Indices on (container, at_ticks); R-tree waits for editors.

**Realization view v0:** direct events UNION instanced events (loop placement via
`generate_series`, per-instance transpose, mute filter, R-405 length clip); recursive
CTE upgrade when nesting material arrives. The stage-2 schedule compiler is a consumer
of `realized ORDER BY at` — and the view is the specification any later optimized
realization must match (§6f-bis two-tier strategy).

**Command vocabulary v0:** CreatePhrase, AddEvents, RemoveEvents, CreateInstance,
SetInstanceParams, CreateTrack, SetTempo, RecordBatch — journaled with redo+undo
payloads inside per-gesture transactions (R-905 literally).

**Judgment calls logged:** container duality via nullable FKs + CHECK; current-state
tables + journal (not pure event sourcing); JSON `extra` escape valves; provenance
columns from day one; tracks-in-root-phrase-only as PoC narrowing.

## Padlington port plan (2026-07-19)

The first voice port; chosen because **PADsynth is precompute-heavy, playback-trivial**
— the split de-risks the port and defines the graph runtime's v0 node menu.

- **Precompute (app thread, pure, seeded):** harmonic profile (waveform morphs =
  profile interpolation) → formant shaping → Gaussian partial spread → seeded random
  phases → IFFT (rustfft/realfft) → looping wavetable. Tables ship to RT as immutable
  Arc buffers via the command ring.
- **RT voice (allocation-free):** phase-accumulator table playback w/ interpolation
  (pitch ratio = freq/base), amp envelope, seeded noise component, gain.
- **Node menu v0 defined by this voice:** buffer source (loop + playbackRate), gain,
  the **AudioParam automation module** (W3C spec math — setTargetAtTime/ramps; shared
  by all future voices; unit-tested against spec formulas), biquad if the JS inventory
  demands it.
- **Seeding upgrade:** phases + noise seeded → bit-identical renders (R-706); the port
  is *more correct than its source* (the ±1 dB metering wobble gotcha dies here).
- **Golden masters, phase-independent:** compare table *magnitude spectra* (dB
  tolerance) + notch-style metered render comparison (RMS/peak/centroid on a reference
  phrase). Requires zero Notorolla-side changes. (Optional later: seed-injection hook
  in notch for near-exact comparison — a lab change, make-it-so gated.) Rust-side
  gate: render twice → bit-identical (R-1402).
- **Steps:** (a) read-only JS inventory (patch fields, topology, envelope timings);
  (b) spectral builder + spectra tests; (c) param-automation module; (d) RT nodes;
  (e) assembled voice + meter certification; (f) live pool + real noteOn/noteOff
  (fire-and-forget retires — R-711) + 16-ET degree→freq check (playbackRate is
  continuous; per-region tables if top-octave stretch colors).

## Mechanism contract v0 sketch (2026-07-19)

The stage-4 entry gate: the services every widget may assume, stated once so no widget
invents its own. Written to be implementable by both backends (native wgpu now,
browser canvas later): **no native handle ever leaks into the kit-facing API.**

1. **Window & surface.** GPU surface per window; per-monitor DPI reported; redraw =
   full frame at vsync when dirty (damage regions later). Monotonic UI time provided
   for animation (Record-button flash); **UI clock ≠ engine clock** — engine positions
   arrive via telemetry ring, consumed at frame start.
2. **Pointer events** (down/move/up/wheel, hover enter/leave) with **implicit capture**:
   press captures the hit widget until release (the Notorolla pointerdown-vs-click
   lesson, made structural). Hit testing lives kit-side over the widget tree; mech
   delivers routed raw events. Wheel targets the hovered widget (wheel-on-hover).
3. **Activation ≠ focus.** Widgets act on pointer-down without acquiring focus or
   changing window z-order — the Control Bar's always-active behavior as a routing
   property, available to any control.
4. **Keyboard vs text are two channels:** raw keystrokes (for bindings, key
   equivalents, performance focus R-1015) and composed TextInput (IME-mediated, for
   fields). The distinction exists in v0 even though IME hookup is deferred; numeric
   fields (Counter, Tempo, In/Out) consume TextInput.
5. **Focus model:** explicit ownership; focus moves only by user action or explicit
   programmatic transfer marked as such. Nothing steals it (R-907).
6. **The hands-off clause (R-907):** the mechanism never moves the pointer, never
   steals focus, never scrolls unbidden. The one sanctioned exception is provided *by*
   the mechanism as an API — capture-hide-restore relative drag (infinite knob drag) —
   so widgets cannot roll their own warping. Scroll offsets are owned by scroll
   containers and mutate only via user gesture or calls marked programmatic (the
   Scroll Law enforced at one chokepoint).
7. **Cursor shape** requested through the mech API per widget.
8. **Text stack:** cosmic-text for shaping/measurement; kit widgets never touch fonts
   directly.
9. **Accessibility slot declared:** the contract reserves the AccessKit node-tree
   channel (R-1510); v0 ships the structure, minimally populated — reserved now so the
   widget kit grows around it, not against it.
10. **Threading:** UI is single-threaded on main; mech APIs are not thread-safe and
    don't pretend to be; cross-thread input arrives only via rings.
11. **v0 omissions (deliberate):** multi-window, drag-drop, clipboard, touch/pen
    gestures, native menus, a11y population, the browser-canvas backend itself — all
    anticipated by the API shape, none implemented for the PoC.

## Lab & platform posture

- **Dev hardware (2026-07-19):** integrated sound or **Yamaha AG06** (preferred:
  class-compliant 48 kHz; hardware direct monitoring on its knobs = free test rig for
  the don't-double-monitor posture; loopback useful for R-815 later). M-Audio
  Code/Oxygen controllers (Code's pads/knobs = R-917 fodder; several controllers →
  R-610 testing later). Expected WASAPI-shared round trip ~20–30 ms: laggy, fine —
  the PoC makes the latency number *true and visible*, not small.
- **The Cubase machine** (RME Babyface, MIDI fleet, 01v/ADAT) is the future
  integration/acceptance lab (RME ASIO, the R-306 RD-2000 loop) — never the dev box.
- **Windows-only lab now; portability by seam:** all platform touchpoints behind
  cpal/midir/winit/wgpu/rusqlite/cosmic-text; **no Win32 outside `ui-mech`**; CI
  cross-compiles Mac/Linux targets from day one (compile checks, honestly not tests).
  WASAPI shared first; ASIO deferred (RME polish, not a PoC gate). Mac/Linux hardware
  arrives with funding; Linux-as-future-primary stays free.

## Deliberate omissions

No editors (material hardcoded or via a crude Notorolla-JSON converter — R-1405's
seed); one ported voice (the simplest Notorolla voice, ported *properly* — the port
methodology is itself PoC subject matter); mixing = master gain; no plugins, no
notation, no performance layer beyond the Thru pop-up's shape; persistence UI = "it
never loses anything."

## Open zoom-ins (next design discussions, any order)

1. **Engine ↔ app interface** — what a compiled schedule is; command-ring vocabulary;
   clock reporting. The most consequential seam.
2. **Graph runtime node set v0** — node list, fidelity order, which voice ports
   first, golden-master methodology (notch-derived).
3. **Mechanism contract v0** — the document itself (stage 4 entry gate).
4. **Core schema** — SQLite tables + command vocabulary (stage 1 blueprint).
