# Revision — Requirements v1

Status: capture in progress. Content is added only after discussion.
Language convention: brief, plain, declarative. Rationale lives in revision.md, not here.

Tags: [P1] initial scope · [P2] subsequent · [Later] undetermined future ·
[Arch] binds architecture from the start regardless of feature exposure ·
[Open] decision pending. Untagged requirements are constitutional (always in force).

## 1. Identity & scope

- R-001. Revision is a note-based composition and playback system. The note, organized
  into phrases, is the primary unit of the model. All other media and views serve the
  note model.
- R-002. Pitch shall be represented as note numbers: signed integer positions in a
  tuning. 12-ET is one supported tuning with no privileged status in the model.
- R-003. Time shall be represented in musical units throughout the model: integer ticks
  at a fixed resolution of 5040 ticks per quarter note. Seconds are derived only at the
  engine boundary via the tempo map.
- R-003a [Open]. Per-object linear (wall-clock) time anchoring is deferred pending audio
  recording design.
- R-004 [P1]. Target platforms are Windows, macOS, and Linux desktop, first-class and
  simultaneous.
- R-005 [Arch]. The architecture shall anticipate the possibility of web, tablet/player,
  and mobile variants.

## 2. Implementation constraints

- R-101 [P1]. The system shall be implemented primarily in Rust.
- R-102 [P1]. Components in other languages are permitted where required. Each shall be
  isolated behind a defined interface.
- R-103 [P1]. Where embedded scripting is provided, the language shall be ECMAScript.
- R-104 [Arch]. Core model logic shall be platform-independent and compilable to WASM.
- R-105 [Arch]. Hardware compute acceleration (GPU or equivalent) may be used for
  precomputation, offline rendering, and analysis. The CPU implementation is
  normative. Any result whose determinism is required (R-706, R-1402, R-1503) is
  either computed on the CPU or computed once and stored in the project. No
  accelerated path exists in the real-time audio callback.

## 3. Formats & persistence

- R-201 [P1]. The project file shall be an SQLite database. It is the authoritative,
  continuously updated project state; there is no unsaved state.
- R-202 [P1]. After abnormal termination at any point, reopening a project shall lose no
  completed user gestures.
- R-203 [P1]. A JSON text format shall serialize the complete project model.
  SQLite↔JSON round-trip shall be lossless.
- R-204 [P1]. Round-trip losslessness shall be verified by automated tests run routinely
  during development.
- R-205 [P1]. The store shall retain a command journal sufficient to reconstruct project
  state. Undo history shall persist across sessions.
- R-206 [Later]. Additional interchange formats (MIDI file, DAW project, notation,
  tuning) are to be determined.
- R-207 [Open]. Storage of recorded audio (in-database vs. referenced files) is deferred
  pending design.

## 4. Audio & MIDI engine architecture

- R-301 [Arch]. The audio engine shall be full-duplex from initial implementation.
- R-302 [Arch]. A single timebase shall govern playback scheduling, MIDI input, and
  audio input.
- R-303 [Arch]. The engine shall maintain a latency model of all input and output
  paths. Recorded material shall be aligned to the timeline using it.
- R-304 [Arch]. Signal paths shall be classified as live (monitored) or playback.
  Playback paths are latency-compensated; live paths are latency-minimized within a
  defined budget.
- R-305 [Arch]. Processing whose latency exceeds the live-path budget shall be
  excludable from live paths automatically, per path, without interrupting playback.
- R-306 [P2]. The system shall support live playthrough: MIDI in, routed MIDI out to an
  external instrument, returned audio monitored through the engine.
- R-307 [P1]. The system shall report the actual end-to-end latency of live paths.
- R-308 [P1]. The system shall play back audio material within arrangements.
- R-309 [P2]. The system shall record audio input, aligned via the latency model.

## 5. Sequencing model

Definitions. *Phrase*: a named container of events; the unit of material;
origin-agnostic. *Pattern*: informal term for a phrase created with the intent of
repetition (typically via a grid editor); every pattern is a phrase; the model does not
distinguish them. *Instance*: a placement of a phrase in time, carrying its own play
parameters. *Structured instance*: an instance of a phrase that itself contains
instances. *Track*: an ordered container of instances and direct events.
*Arrangement*: a phrase serving as a playback root. *Library*: the project's phrases,
collectively. *Direct events*: events on a track outside any phrase. "Phrase" is the
preferred user-facing term; UI labels may differ from spec terms but shall map to them
one-to-one.

- R-401. The reusable unit of material is the phrase: a named container of events.
  Phrase length is an explicit attribute, defaulting to content extent at creation.
  Length is a window: events beyond it are retained but do not sound.
- R-402. Note events shall carry at minimum: position (ticks), duration (ticks),
  note number, and velocity (16-bit, the MIDI 2.0 domain; translated at the MIDI 1.0
  boundary per spec). The event vocabulary shall be extensible (continuous
  controllers, articulation, audio, and other types).
- R-403. Phrases are polyphonic. Editors may impose stricter constraints; the model
  does not.
- R-404. An instance places a phrase in time. All instances of a phrase share its
  material: editing the phrase affects every instance.
- R-405. An instance shall carry its own play parameters, applied non-destructively:
  window offset (into the material), length (independent of the phrase's length),
  loop count, transpose (chromatic: in note numbers), mute. The parameter set shall
  be extensible (articulation template, timbre arc).
- R-406. A track is an ordered container of events and instances. Tracks may hold both
  direct events and instances simultaneously.
- R-407. Phrases may contain instances of other phrases (nesting). An instance of a
  phrase that contains nested instances is a structured instance. Cyclic reference is
  prohibited and shall be rejected at the model level.
- R-408. An arrangement is a phrase serving as the root of playback. Any phrase may be
  a root.
- R-409. Reciprocal verbs shall exist: make-phrase (selected material becomes a phrase,
  replaced by an instance) and unmake (an instance is replaced by the material it
  references). Round-trip shall be lossless.
- R-410. A bake verb shall exist: an instance's play parameters are applied
  destructively, producing plain material.
- R-411. The model shall track where each phrase is instantiated. Where-used shall be
  queryable.
- R-412. Unreferenced phrases persist in the project library. They are removed only by
  explicit deletion.
- R-413 [P2]. Material may carry provenance metadata: origin (recorded, generated,
  derived), generation seed and parameters, and parent material.
- R-414. A phrase may carry its own attributes: meter map, tempo map, tuning, and
  instrument bindings. All are optional.
- R-415 [Open]. Every phrase and instance attribute shall have a defined inheritance
  rule, and rules may differ per attribute class. The candidate chain (instance
  override, phrase attribute, parent, project default) and per-attribute governance
  (e.g., tempo parent-governed, tuning phrase-owned) are working assumptions, not yet
  settled; the model shall not hard-code a single inheritance scheme.
- R-416 [Arch]. An instanced phrase may play at its own tempo concurrently with its
  parent (polytempo). Engine scheduling shall support multiple concurrent tempo streams
  over the single timebase (R-302).
- R-417. A phrase shall be playable standalone, without embedding in an arrangement.
- R-418 [P1]. Mixed-tuning arrangements shall be supported: a phrase's note numbers
  are interpreted in the phrase's tuning.
- R-419. Meter is optional. No feature shall require a meter to be defined.
- R-420. Where no meter is defined: bar display derives from phrase boundaries; the
  metronome clicks unaccented beats; notation and interchange formats requiring time
  signatures obtain them at render or export time — supplied, or inferred from phrase
  length and accent structure, user-overridable.
- R-421. Metric accent is expressed as material (a cyclic accent template), not as a
  property of the timeline.
- R-422. Transformations compose through nesting: applying a transformation to a
  structured instance shall yield the same result as baking the instance first and then
  applying the transformation.
- R-423. Transpose operates in the tuning of the transposed material. Two
  transpositions are first-class: transpose by degree (scale-relative, requiring a
  scale and root) and transpose chromatically (by note number — equivalently, degree
  transposition under the null scale). For structured
  instances spanning multiple tunings, resulting transposability is determined by rule
  [Open: rule to be defined]; "not transposable" is a permitted outcome.
- R-424. The model permits overlapping instances on a track. Editors shall not
  facilitate creating overlaps. [Flagged for review: remove permission if no use case
  emerges.]
- R-425. There is a single kind of track. Note events, audio events, and instances may
  coexist on any track.

## 6. Tuning model

Definitions. *Note number*: the model's pitch datum — a signed integer position in a
tuning. *Pitch class*: a note number reduced modulo notes-per-period (periodic tunings
only). *Degree*: a position within a scale — the scale-relative, user-facing term.
*Midi note*: the 0–127 MIDI wire value, appearing only at the MIDI I/O boundary.

- R-501. A tuning defines the mapping from note number (signed integer) to frequency.
  The mapping shall be deterministic and identical across platforms.
- R-502. A tuning may declare a period (interval of equivalence) with a number of
  notes per period. The period is typically the octave but need not be (e.g.,
  Bohlen-Pierce). Tunings with no period are fully supported; pitch-class logic (note
  number modulo notes-per-period) applies only to periodic tunings.
- R-503. A tuning declares an anchor: one note number bound to a reference frequency.
- R-504. Tunings whose intervals are ratio-defined shall use exact rational
  representation where appropriate, not approximated cents or floats.
- R-505. 12-ET, just intonation, and 16-ET shall be provided. The tuning set is
  extensible; user-defined tunings are first-class.
- R-506. Interchange serializations shall embed every tuning the project uses; playback
  from interchange shall never depend on an external tuning registry. [Open: whether
  the project store embeds tunings or references a shared library.]
- R-507 [P2]. Tuning interchange: import and export of Scala (.scl/.kbm) files.
- R-508. A tuning may define a naming scheme for its notes and pitch classes
  (letters for 12-ET, hexadecimal for 16-ET). Absent a scheme, notes are named
  numerically. Naming is presentation; the stored datum is always the note number.
- R-509. A scale is a named subset of pitch classes (periodic tunings; stored
  root-relative) or of note numbers (aperiodic tunings). A periodic scale is
  applicable to every tuning sharing its notes-per-period; aperiodic scales are
  specific to their tuning. Idiomatic fit over a particular tuning and root is
  advisory — curated or computed, never enforced. The full set is always available.
- R-510. Scales constrain editors and generators; they do not restrict the model. Any
  note number may be stored regardless of the active scale.
- R-511. Withdrawn (superseded by R-517).
- R-512 [P1]. The system shall provide a consonance analysis service: given a tuning
  and a timbre, rank intervals and chords by computed roughness. Available to harmony
  tools, generators, and editors.
- R-513 [P2]. Instruments may declare tuning awareness: adapting their partial
  structure to the active tuning. (Details in the Instruments section.)
- R-514 [P2]. Table-based tunings (measured or arbitrary frequency lists) may be
  supported.
- R-515 [Arch]. Tuning may change dynamically over time. The engine and model shall not
  assume a static tuning for the duration of playback; note-number-to-frequency
  resolution is time-aware. [Open: change semantics — discrete change events vs. continuous
  interpolation between tunings; behavior of sounding notes across a change (hold pitch
  vs. retune).]
- R-516 [P1]. The system shall provide recognition and elucidation of triads within
  material, per tuning; recognition of more general chords [Later]. Available to
  editors, analysis views, and generators.
- R-517 [Open]. Root (reference pitch class) semantics — the relationship among anchor,
  root, transpose, and the R-415 inheritance model — are not yet settled.

## 7. MIDI I/O

- R-601 [P1]. The system shall support MIDI input and output devices, enumerated at
  runtime, with hot-plug arrival and removal handled without restart.
- R-602 [P1]. Devices shall have persistent identity across sessions. Projects and
  settings referencing an absent device shall degrade gracefully and rebind when it
  returns.
- R-603 [Arch]. MIDI input shall be timestamped at the driver boundary and mapped onto
  the engine timebase (R-302). Timestamps, not arrival order, are authoritative.
- R-604 [P1]. External MIDI destinations shall be first-class instrument targets: a
  track may address an external device/channel as its instrument.
- R-605 [P1]. Thru routing shall exist: input routed to any instrument target
  (internal or external), with transforms (channel, transpose, mapping) applied in the
  live path, subject to the live-path latency budget (R-304).
- R-606 [P1]. Input notes shall be mapped to note numbers at capture time via a
  definable input mapping (midi note → note number). The default mapping is 12-ET
  identity.
  [Open: non-12 input mappings (scale-mask-aware, isomorphic layouts) require
  experimental design — further work.]
- R-607. Recorded material stores note numbers (R-508). Raw performance data as
  received — original midi note, channel, velocity, controller values, unprocessed
  timing — shall be retained as capture metadata.
- R-608 [P1]. Output shall map note numbers to MIDI per destination. 12-ET note
  numbers emit plain midi notes. [Open: non-12 output strategies (per-channel pitch
  bend, MPE, MTS) and
  behavior for unrepresentable material require experimental design — further work.]
- R-609 [P2]. MPE shall be supported as a transport on input and output, mapping to
  and from the model's per-note expression representation (R-617).
- R-610 [P2]. Multiple simultaneous inputs shall merge, with source identity retained
  per event.
- R-611 [P2]. An input event filter shall exist (record/thru by event type).
- R-612 [P1]. A panic action shall exist covering both domains: all-notes-off/reset to
  all MIDI destinations, and audio panic — silence all internal voices and reset
  self-sustaining audio state (feedback and delay lines).
- R-613 [Later]. Per-key illuminated controllers (Lumatone, LinnStrument class) may be
  driven as display surfaces (scale mask, degree lighting).
- R-614 [Later]. MIDI clock and MTC synchronization, in and out.
- R-615 [Later]. Virtual MIDI ports (routing to and from other applications).
- R-616 [Later]. MIDI 2.0/UMP is anticipated; per-note pitch is a natural output
  strategy for tuning transport when device support matures.
- R-617 [P2]. Note events may carry attached continuous expression curves (pitch
  offset, pressure, timbre dimensions) as properties of the note, editable
  independently of their source. External protocols (MPE, VST3 Note Expression) map to
  and from this representation.
- R-618 [P1]. Program change and bank select are first-class events: recordable,
  editable, and sent to external destinations.
- R-619 [P1]. Per-device patch name lists shall be supported: user-editable,
  importable, and displayed wherever a patch is shown or selected (tracks, instrument
  assignment, auditioning). Patches are selectable by name.
- R-620 [P2]. SysEx shall be captured, stored, and retransmitted: bulk dumps archived
  as project or library assets and sendable to restore device state. [Storage location
  shares the R-506 open question.]
- R-621 [P2]. Device profiles shall be definable as data (patch name charts, parameter
  maps, dump formats), user-creatable and shareable; profiles may be script-extended
  (R-103).
- R-622 [Later]. Per-device patch editor panels generated from device profiles.
- R-623 [P2]. Patch auditioning: step through a device's patches by name during
  playback.

## 8. Instruments & audio processing

Abstraction:

- R-701 [Arch]. There is a single instrument abstraction with multiple target kinds:
  internal instruments, external MIDI devices (R-604), and hosted plugins [P2]. Track
  instrument bindings do not distinguish kinds.
- R-702 [Arch]. Patch binding is unified across kinds: one binding concept, realized as
  preset (internal), program/bank change (external, R-618), or plugin state (hosted).
  Bindings are project state; phrase- and arrangement-level bindings follow
  R-414/R-415.
- R-703 [Arch]. Internal instruments and effects are self-contained processors against
  a narrow host interface: events in (pitch delivered through the interface as note
  number with tuning context, or frequency), declared parameters, audio out, state
  serialization. No access to engine globals. This interface shall not preclude
  packaging processors as CLAP/VST3 plugins [Later].

Graph runtime:

- R-704 [P1]. A DSP graph runtime shall be provided, implementing selected Web Audio
  node semantics accurately per the W3C specification: oscillator/PeriodicWave, gain,
  biquad filter, delay, buffer source, waveshaper, and the parameter automation
  curves. Instruments and effects are definable as graph descriptions.
- R-705 [P1]. Notorolla's instruments shall be ported as graph descriptions and
  verified against golden-master renders and meters (notch-derived conformance suite).
- R-706 [P1]. All stochastic elements (noise sources) are seeded and deterministic:
  identical renders from identical state.
- R-707 [Arch]. The runtime shall support dynamic graph topology (no fixed-voice
  assumption).

Mixing & effects framework:

- R-708 [P1]. A mixing framework shall exist on the same runtime: tracks route through
  per-track processing chains to buses and a master output. Sends, bus structure, and
  mixer detail are deferred to a later section.
- R-709 [P1]. Effects are processors under the R-703 interface discipline, insertable
  per track and per bus.
- R-710 [Later]. A user-facing modular synthesis environment (user-patchable graphs
  from runtime nodes) may be provided. R-707 is its architectural prerequisite.

Voices, parameters, behavior:

- R-711 [P1]. Voice lifecycle: noteOn/noteOff with release tails; defined voice
  allocation and stealing policies; per-voice expression input (R-617).
- R-712 [P1]. Parameters are formally declared: name, range, unit, curve. Parameters
  may be enumerated: an ordered list of named values. Enumerated parameters support
  stepping operations (next, previous, wrap) as first-class operations. Instruments
  should present few, calibrated macro dimensions as their primary surface.
- R-713 [P1]. Timbre parameters shall be loudness-neutral (energy-normalized): a
  timbre control is not a volume control.
- R-714 [P1]. Instruments receive tuning context through the interface; tuning-aware
  instruments may adapt partial structure (R-513). Note-number-to-frequency resolution
  is time-aware (R-515).
- R-715 [Arch]. Live-performance CPU-relief variants (reduced voices) shall never
  affect offline rendering: offline render always uses the full voice.
- R-716 [P1]. Internal instruments add no buffering beyond the engine block in the
  live path (R-304).
- R-717 [P2]. Hosted plugin instruments and effects (CLAP, VST3) via an isolated
  bridge process. Detailed requirements deferred to a hosting section.

Scope of processing:

- R-718 [Later]. Pitch tracking and pitch manipulation of monophonic material are
  anticipated.
- R-719 [P2]. Spatial processing beyond stereo is provided by hosted plugins (R-717).
  Built-in spatial functions, if any, are limited to techniques free of active patent
  claims.
- R-720. Source separation and audio restoration are out of scope. Such material is
  prepared by other tools before import.

## 9. Recording

Shared transport behavior (domain-agnostic):

- R-801 [P1]. Any track may be record-armed; multiple tracks may be armed and capture
  simultaneously.
- R-802 [P1]. Record modes: replace and overdub, in real time.
- R-803 [P1]. Punch recording: in/out points bound the recorded range; punch in/out on
  the fly (during playback) is supported.
- R-804 [P1]. Loop recording: capture over a repeating range with take management —
  keep and discard gestures operate during looping without stopping the transport.
- R-805 [P1]. Begin-record behaviors: count-in, and wait-for-first-event.
- R-806 [P1]. Recording never interrupts playback: arming, disarming, punching, and
  take gestures occur with the transport running.
- R-807 [P1]. Recorded material lands as direct events on the target track.
  Conversion to phrases is the make-phrase verb (R-409), not an automatic behavior.
- R-808 [P1]. Capture is journaled as it occurs, including retrospective capture
  buffers: material recorded — or retained retrospectively — up to the moment of an
  abnormal termination is recoverable (extends R-202).

MIDI capture:

- R-810 [P1]. Real-time MIDI capture: driver timestamps authoritative (R-603), note
  numbers at capture (R-606), raw performance metadata retained (R-607).
- R-811 [P1]. Record quantize: input quantization applied non-destructively at
  capture; underlying raw timing is retained and the quantization is revisable after
  the fact.
- R-812 [P1]. Step entry: notes entered stepwise with step size, duration, and
  velocity controls; chords supported; input from MIDI or on-screen.
- R-813 [P2]. Continuous controller and expression capture into R-617 curves, subject
  to the input filter (R-611).
- R-814 [P1]. Retrospective capture: an always-listening MIDI input buffer, durable
  per R-808, with a configurable retention window. Contents are retrievable as
  recorded material. Material captured during playback carries timeline alignment and
  is insertable in place; material captured while stopped retains relative timing and
  lands at a user-chosen point.
- R-815 [P2]. Rolling audio capture: designated audio points (master output;
  optionally live inputs) are continuously recorded into a bounded rolling window
  (order of 5–15 minutes, configurable). Contents are retrievable as audio events.
  Primary purpose: preserving audio that is not re-derivable from project state
  (external instruments, live input). [Open: tap points, format, retention details.]

Audio capture:

- R-820 [P2]. Audio input is recordable, placed on the timeline via the latency model
  (R-303, R-309).
- R-821 [P2]. Captured audio lands as audio events on ordinary tracks (R-425), subject
  to the same phrase machinery as notes.
- R-822 [P2]. Audio recording under loop and punch follows the shared transport
  semantics above.
- R-823 [Open]. Audio capture design: storage (R-207), monitoring interplay while
  recording, simultaneous input count, and capture granularity (phrase-capture vs.
  multitrack) — deferred pending audio design.

## 10. Editing & views

Doctrines:

- R-901 [Arch]. Built-in editors are clients of the public model surface (selection
  queries and command verbs). There is no privileged editor backdoor; any operation an
  editor performs is expressible through the same surface scripts and generators use.
- R-902 [P1]. Selection is a first-class object: it may span event types, phrases, and
  tracks; it is constructible by direct manipulation and by rule (attribute queries);
  rule results combine with set operations (select, add, refine).
- R-903 [P1]. Editing operations are uniform verbs applied to a selection, independent
  of which view invoked them. The verb set is extensible.
- R-904 [P1]. Every applicable verb offers an apply-to-copy variant (operate on a
  duplicate, leaving the original intact).
- R-905 [P1]. Editing gestures are transactional: in-progress gestures are
  uncommitted, completion commits, Escape cancels. Committed gestures are journaled
  (R-205).
- R-906 [P1]. All editing is available during playback; edits are audible on the next
  occurrence of the edited material. The transport never stops for an edit.
- R-907 [Arch]. Interaction laws, enforced at the mechanism level: the application
  never steals focus, never moves the pointer (outside sanctioned drag capture), never
  scrolls unbidden.
- R-908 [P1]. Verbs are keyboard-assignable; common editing flows are operable
  entirely from the keyboard.

Views:

- R-909 [Arch]. Views are synchronized lenses over one model and one selection: a
  selection made in any view is the selection in every view; a change from any source
  (editor, script, generator, recording) is reflected in all views live.
- R-910 [P1]. Initial view roster: arrangement (instances on tracks), grid (pattern
  editor; may impose constraints per R-403), roll (graphic note editing), and list
  (event detail).
- R-911 [P2]. Second-tier views: expression/strip-chart lanes (R-617), basic notation
  (R-420), tracker, and analysis displays (consonance R-512, triad elucidation R-516).
- R-912 [P2]. Verb and selection configurations are savable as named templates.
- R-913 [Later]. Additional view concepts (pipeline view, home-distance display, and
  other performance/generative surfaces) are anticipated; the view architecture shall
  not assume a fixed roster.
- R-914 [P1]. Several views of the same phrase may be open and editable
  simultaneously (e.g., grid, roll, list; later notation, tracker). Changes made in
  any view are immediately reflected in all others (per R-909).
- R-915 [P1]. Every function is reachable through visible, pointer-operable UI.
  Modifier keys and keyboard bindings are accelerators, never the sole path to a
  function.
- R-916 [P1]. Keyboard, modifier, and gesture bindings are remappable as data; binding
  maps are exportable, importable, and shareable.
- R-917 [P1/P2]. Hardware control surfaces (MIDI and USB) shall be supported: controls
  mappable to transport, verbs, and declared parameters (R-712) [P1]; mapping includes
  relative/endless encoder semantics and detented, hysteresis-guarded stepping of
  enumerated parameters [P1]; device feedback, surface profiles as shareable data, and
  script-extended surfaces (R-103) [P2].
- R-918 [P1/P2]. Full-surface control: a controller with sufficient controls shall be
  able to operate the entirety of an embedded instrument's declared parameters.
  Mapping layouts pair an instrument with a surface: auto-generated from declared
  parameters and groups, user-refinable, shareable as assets. Banking/paging covers
  surfaces with fewer controls than parameters; absolute controls use takeover
  strategies (pickup, scaled) [P1]. State feedback to capable surfaces — values,
  names, LED/motor/display — and the same machinery applied to hosted plugins over
  preserved metadata (R-1206) [P2].
- R-919 [P2]. Screen-optional operation of embedded instruments: patch selection,
  parameter editing, and compare/store operable entirely from a mapped surface.
- R-920 [Later]. Full-surface control extends to external MIDI devices: a device
  profile (R-621) may declare parameters with their wire formats (CC, NRPN, SysEx
  address maps with scaling and checksums). Profile-declared parameters join the
  declared-parameter model as a third provider (with internal R-712 and plugin
  R-1206), receiving the same layout, stepping, takeover, and mapping machinery
  (R-918). Feedback applies where the profile declares state query support. Parameter
  edits to external devices are journaled; state is re-sendable.

Windows, workspaces, and documents:

- R-921 [Arch]. A project has three independent properties: **open** (a live handle
  exists), **active** (attached to the engine — owns the transport, receives MIDI
  input, produces sound), and **editable** (read-write or read-only). Any number of
  projects may be open; at most one is active.
- R-922 [Arch]. Activation is explicit. No project becomes active as a consequence of
  window focus, view focus, or pointer activity.
- R-923 [P1]. The identity of the active project is displayed wherever transport or
  record state is displayed.
- R-924 [Arch]. The target of an editing verb is the project of the focused view,
  which is not necessarily the active project.
- R-925 [P1]. Selection is per-project. R-909 holds within a project; a selection does
  not span projects.
- R-926 [P2]. Several projects may be open simultaneously. Inactive projects remain
  editable and journaled.
- R-927 [P2]. A project may be opened read-only: browsed, auditioned, and copied from
  without being activated. A project's mode may be changed without closing the
  application.
- R-928 [Arch]. Views are placement-agnostic. A view does not know whether it occupies
  a window, a pane, or a tab, and holds no window state. Placement is assigned to a
  view, never chosen by it.
- R-929 [Arch]. Window and pane placement are one structure: a tree whose leaves hold
  views, whose interior nodes are splits and tab stacks, and whose roots are windows.
  Splitting, docking, tabbing, and tear-off as user operations are [P2].
- R-930 [Arch]. Every view instance is bound either to the active project (*following*)
  or to a designated project (*pinned*). A view holds its state per project; changing a
  binding does not discard state.
- R-931 [P1]. A view whose subject is a specific object is pinned by that subject; a
  view whose subject is the application is following. The binding is user-selectable
  for views whose subject is a project as a whole.
- R-932 [Arch]. Window and pane layout is never journaled and never undoable.
- R-933 [P1]. Layout state has two layers: workspace state (window and pane geometry,
  view roster, bindings) is application-level and savable as named workspaces (R-912);
  per-view display state (scroll, zoom, visible content) is stored in the project and
  restored into whichever views bind to it.
- R-934 [P1]. Closing a project closes the views pinned to it. A following view
  displays an empty state when no project is active.

Spectral waveform overview:

- R-935 [P2]. Audio waveform overviews are spectrally colored: the display color at
  each horizontal position encodes the frequency content of the material there.
- R-936 [P2]. The overview is computed by band analysis of the source material — low,
  mid, and high bands mapped to red, green, and blue — with color expressing spectral
  balance and brightness expressing level. Band edges, normalization, and mapping are
  adjustable; defaults are chosen for legibility at a glance.
- R-937 [P2]. Overview analysis is cached alongside the material it describes and
  recomputed only when that material changes.

Interaction authority:

- R-939 [Arch]. Where these requirements are silent on interaction behaviour, the
  *Macintosh Human Interface Guidelines* (Apple, 1992) are the default authority —
  for behaviour and principles, not for appearance, menu-bar architecture, or its
  document model. Precedence: these requirements, then the coding standard and skin
  inventory, then that document, then invention — and an invention is recorded in
  `revision_hig_inventory.md` with its reason.
- R-940 [Arch]. Departures from R-939's authority are deliberate and recorded, not
  incidental. The standing departures are: no unsaved state and therefore no
  Save/Revert model (R-201); journaled unlimited undo rather than single-level
  (R-205); and stricter modelessness than its dialog chapters allow (R-905, R-906).

Interface scale:

- R-938 [P1]. The interface provides a user-settable scale, applied uniformly to all
  interface geometry and independent of platform DPI, with which it composes. Scale
  is workspace state (R-933) and is settable per window. Content zoom within a view
  is a separate control and is unaffected by it.

## 11. Performance

Triggering & queues:

- R-1001 [P1]. Any phrase may be assigned a trigger: computer key, MIDI note, or
  control-surface pad. Triggering starts the phrase subject to launch quantization.
- R-1002 [P1]. Launch quantization: immediate, next beat, next bar, or next phrase
  boundary; configurable per trigger.
- R-1003 [P1]. Queues: successive triggers accumulate into an ordered queue — several
  keystrokes queue an on-the-fly arrangement. Queues are visible and editable; advance,
  stop, and clear have dedicated gestures, including stop-all.
- R-1004 [P1]. Multiple slots: several phrases play simultaneously in independent
  slots, each with its own queue. Slot count and queue depth are configurable
  (defaults in the spirit of the original: 9 slots, 12-deep queues). (This is the
  performance-layer meaning of model-level overlap, R-424.)
- R-1005 [P2]. Trigger modes (Vision lineage): transpose (a played note transposes
  playing/triggerable phrases), trigger (note restarts), continuous trigger
  (additive), gated (plays while held). Chords produce simultaneous transposed copies.
- R-1006 [P1]. Performance actions are recorded as structure: trigger, queue, and
  state-change performances land as instance and state events — editable,
  re-renderable, never flattened.

States & morphing:

- R-1007 [Arch]. A performance state is a named musical target: texture (lanes
  in/out), parameter targets (density, register, timbre macros, consonance), and/or
  material targets. A launch is the degenerate state change — single-phrase target,
  zero glide. The state concept underlies launch, queue, and morph uniformly.
- R-1008 [P2]. Morph transitions: state changes commit within a bounded musical
  interval (configurable; order of one to two beats), at musically legal boundaries.
  Parameters glide; texture changes stage in and out; composed transition material is
  optionally invoked.
- R-1009 [Later]. Material morphing: a state change may re-anchor generative material
  (Evolver home-vector retargeting) so that playing material evolves into the target
  rather than being replaced.
- R-1010 [P1]. Performance playback is deterministic: identical trigger and state
  input at identical positions yields identical output (rehearsable takes; ties R-706
  and the seed doctrine). Scope: internally generated sound; external instruments are
  outside the determinism boundary (R-815 covers them).

Surfaces & input processing:

- R-1011 [P2]. Triggers and states are mappable to control surfaces (R-917), including
  pad pages for state sets (the scoring pad).
- R-1012 [P2]. Input effect: a latchable arpeggiator/repeater on the live input path —
  order modes, grid-or-groove spacing, period extension — recordable per R-1006's
  structure doctrine where it emits notes.
- R-1013 [Later]. Timecode-locked performance (scoring against picture) rides on
  synchronization (R-614).
- R-1014 [P1]. A Players & Queue view shall exist, replicating the Vision original in
  full: per-slot queue display distinguishing playing from waiting phrases; a
  queue-mode toggle (accumulate vs. switch); slot selection by pointer or number;
  stop-all (clearing all queues) and per-slot stop/advance gestures. This view
  satisfies R-1003's visibility mandate, is designed touch-first (R-915), and is
  anticipated as the primary UI of the player variant (R-005). Phrases with asserted
  own tempo (R-416) play at that tempo within a slot.
- R-1015 [P1]. An explicit performance focus governs when keyboard input acts as
  triggers; trigger keys shall never conflict with text entry or editing bindings
  outside that focus.

## 12. Generative tools

Determinism & provenance:

- R-1101 [Arch]. All generation is seeded and reproducible: one owned PRNG in core; no
  ambient entropy (system random, OS entropy, time) in any generative path. Identical
  seed, parameters, inputs, and generator version yield identical output on every
  platform.
- R-1102 [Arch]. Seed streams are splittable and labeled per axis (notes, rhythm,
  articulation, timbre): selective re-roll varies one axis while holding the others.
- R-1103 [P1]. Kept results are committed as material with provenance (R-413): seed,
  parameters, generator version, input references. Material survives generator
  evolution; exact re-roll across versions is best-effort (generators are versioned).
- R-1104 [P1]. Seeds are user-visible and shareable: repeat (same seed), re-roll (new
  seed), pin; seed representable as text.

Generators:

- R-1105 [P1]. Pattern generation at parity with the proven Notorolla generators (New
  Random class): constrained randomness within tuning, scale, length, and template
  constraints.
- R-1106 [P1]. Generation accepts material as input: existing phrases serve as
  templates and donors (accent, articulation, rhythm — pitch and rhythm as separable,
  recombinable streams).
- R-1107 [P2]. Phrase-level generation operates on operations, not raw notes: sampled
  sequences of (phrase, transform) pairs over form archetypes. Output is instances and
  structure — editable — never flattened material.
- R-1108 [P2]. Generation is cycle-aware: cycle-length relationships (coprimality,
  super-cycle length) are computable by and usable in generators.
- R-1109 [P2]. Generative transform verbs produce ordinary editable material:
  note-domain echo/canon (delay-as-verb with per-generation decay and timbre step) and
  pipeline (circulating buffer with per-pass decay and eviction).

Variation:

- R-1110 [P2]. The Mutator: a point operation producing a seeded variation of selected
  material along chosen axes. The Evolver: the Mutator run as a scheduled process over
  repetitions.
- R-1111 [P2]. Variation is stateful: mutations record lineage (parent chains,
  browsable and revertible); the Evolver tracks per-axis distance from origin; return —
  full, partial, transformed — is a first-class move.
- R-1112 [Later]. Variation scheduling may be governed by a pressure model: per-axis
  accounts, accrual with repetition, discharge sized and placed by policy,
  genre-parameterized.
- R-1113 [P2]. A taste record: keep/discard decisions and replay counts for generated
  material are journaled and available to bias generation.

Long-form & analysis:

- R-1114 [Later]. Systems: packaged generative recipes (material, grammar,
  tuning/timbre, mix) as shareable artifacts; an endless playback mode with a
  timestamped seed schedule making any heard moment retrievable.
- R-1115 [Later]. Directed-form generators (Fugenator/Etudenator class): constraint
  search over the transform vocabulary using tuning-derived consonance tables (R-512).
- R-1116 [P2]. Suggestion services: consonance-ranked candidates against the current
  material context (R-512, R-516), assistive only — never automatic.

The performable lead sheet:

- R-1117 [P2]. A lead sheet representation shall exist: a compact, human-readable
  rendering of a piece comprising a pattern glossary (phrases, in notation per R-420),
  a form chart (sections and texture states over the arrangement — R-1007's states),
  and a harmonic/texture line (tuning, mask and root, texture states, spectral event
  marks). Lead sheets are printable.
- R-1118 [P2]. The lead sheet is executable: a lead sheet plus seeds (R-1104) is
  sufficient input for realization — the system performs the piece from the sheet. A
  sparse sheet realizes through defaults; added detail adds control. Realization
  conventions are versioned (with R-1101, a sheet realizes identically given identical
  versions).
- R-1119 [Later]. The relationship is bidirectional: generated or arranged music can
  emit its own lead sheet; an edited sheet re-realizes. One artifact, two performers —
  human or system.
- R-1120 [Later]. A textual serialization of the lead sheet may be provided
  (shareable, diffable). The visual and printed form is primary; syntax follows
  semantics.

## 13. Plugin hosting

Architecture:

- R-1201 [Arch]. Plugins are hosted out-of-process: bridge processes isolate all
  plugin code from the engine. A plugin crash shall not interrupt the engine,
  transport, or session — the affected plugin drops out, the session continues, and
  reload is offered.
- R-1202 [P1]. CLAP hosting, instruments and effects.
- R-1203 [P2]. VST3 hosting via the bridge; the C++ glue lives only there. [Open: SDK
  licensing path.]
- R-1204 [Later]. Additional formats (AU on macOS) as warranted.

Integration with existing abstractions:

- R-1205 [P1]. Hosted plugins are instrument and effect kinds under the R-701/R-703
  abstractions: kind-agnostic track binding; patch binding realized as plugin state
  (R-702); insertable per R-709.
- R-1206 [P1]. Plugin parameters map into the declared-parameter model (R-712):
  automatable, control-surface-mappable (R-917), journaled. Parameter discreteness,
  enumeration, and grouping metadata from plugin formats (step counts, stepped flags,
  value names, groups) is preserved, never flattened.
- R-1207 [P1]. Plugin GUIs embed in application-owned windows (raw native handles).
  GUI-less operation is always available via a generic parameter panel.
- R-1208 [P1]. Plugin latency reports into the latency model (R-303): playback paths
  are delay-compensated; live paths enforce the budget with automatic per-path
  exclusion (R-304/R-305). There is no global "constrain" mode.
- R-1212 [P1]. Offline render parity: hosted plugins render offline through the same
  processing as live (no live-only shortcuts — R-715's spirit). Nondeterminism inside
  a plugin is outside the determinism boundary, like external hardware (R-1010).
- R-1213 [P2]. Plugin state is project state: journaled snapshots, TMON-recoverable
  (R-202) — reopening restores plugin states as of the last commit.
- R-1214 [P1]. Latency is attributed, never just totaled: every processor's
  contribution to a path's latency is inspectable. When a live path approaches or
  exceeds its budget, the offending processors are identified explicitly — and when
  automatic exclusion (R-305) acts, what was excluded and why is visibly indicated.
  Nothing about latency handling is silent.

Microtonal accommodation:

- R-1209 [P2]. The tuning transport tier is visible per plugin: (a) MPE, (b) MTS
  master role, (c) channel-rotation pitch bend, (d) 12-ET only. The user always knows
  what pitch fidelity a plugin can achieve. Ties the R-608 open work; MTS master
  licensing [Open].
- R-1210 [P2]. Per-note expression (R-617) maps to hosted plugins where supported
  (CLAP note expression, VST3 Note Expression, MPE).

Safety & hygiene:

- R-1211 [P2]. Plugin scanning and validation run out-of-process; a failing plugin
  cannot damage the application or a project. Scan results are cached; per-plugin
  enable/disable exists.

## 14. Scripting

Foundation:

- R-1301 [P2]. An embedded ECMAScript runtime (R-103): interpreter-class, sandboxed,
  no JIT dependency (preserves R-005's mobile anticipation).
- R-1302 [Arch]. Scripts operate exclusively through the public model surface (R-901):
  selection queries, verbs, generation and analysis APIs. There are no private script
  hooks — anything a script can do, the application does through the same surface, and
  vice versa within capability grants.
- R-1303 [Arch]. Scripts never execute on the real-time thread. Script output is
  beat-domain data delivered ahead of the clock; the engine consumes it as ordinary
  material.
- R-1305 [Arch]. The scripting API is versioned, and compatibility is a product
  promise: a user's script collection is part of their instrument. Breaking changes
  require explicit migration support.

Extension points:

- R-1304 [P2]. Scripts may define: custom verbs (selection in, edits out), custom
  selection rules, generative operators, device profiles (R-621), control-surface
  integrations (R-917), and export processors. Generative script operators receive
  seeded PRNG streams (R-1101); ambient entropy is unavailable to scripts. Kept script
  output is committed with provenance like any generation (R-1103).
- R-1306 [P2]. Scripts run sandboxed with explicit capabilities: model access by
  default; file, network, and device access only by grant. Scripts arriving with
  shared projects run sandboxed and are visible before they run.
- R-1307 [P2]. Script actions are journaled as commands like any other edit: undoable,
  TMON-recoverable, provenance-carrying.
- R-1308 [P2]. Scripts are assets: shareable, versioned, attachable to projects (a
  project requiring a script for realization carries it — the R-506 embed-vs-library
  nuance applies).
- R-1311 [P2]. Control mappings may be scripted functions: input gestures (absolute,
  relative, buttons, combinations) with mapping state (layers, conditions) transform
  to parameter operations (set, step, wrap, select-by-name) with acceleration and
  hysteresis under script control. Scripted mappings are shareable assets (R-916,
  R-1308).

Development experience:

- R-1309 [Later]. Development affordances: console, logging, error surfacing. A
  misbehaving script is terminable and can never take down the application; script
  errors are reported, never silent.
- R-1310 [Later]. Script-defined panels: custom UI assembled from the
  declared-parameter widget set (the R-622 device-panel machinery generalized).

## 15. Interchange

- R-1401 [P1]. Audio export: mixdown (WAV) and per-track stems (BWF). Export sample
  rate is caller-chosen at export time, never inherited from the output device.
  Offline renders use full processing (R-715, R-1212).
- R-1402 [P1]. Offline renders are deterministic: identical project state renders
  bit-identically (R-706, R-1101; hosted-plugin nondeterminism excepted per R-1212).
- R-1403 [P1]. Standard MIDI File export: 5040 PPQ division; time signatures supplied
  or derived (R-420); 12-ET material exports as plain notes. [Open: non-12 SMF
  representation — rides R-608's further work.]
- R-1404 [P2]. Standard MIDI File import: material lands as direct events, phrase-able
  per R-409; 12-ET mapping assumed unless an input mapping is designated (R-606).
- R-1405 [P1]. Notorolla project import: a migration path from the web lab's project
  format into Revision projects.
- R-1406 [Later]. MusicXML export for notation interchange (meter per R-420).
- R-1407 [Later]. DAWproject support, pending evaluation.
- R-1408. Cross-references: tuning interchange per R-507 (Scala); complete project
  text interchange per R-203 (JSON).
- R-1409 [Arch]. The unit of interchange is a **fragment**: a selection together with
  the closure of everything it depends on. Drag, clipboard, file export and import,
  and script output all carry fragments.
- R-1410 [P1]. Importing a fragment is a single journaled gesture in the receiving
  project.
- R-1411 [Arch]. Transfer between projects is a copy. No operation is atomic across
  two projects.
- R-1412 [Arch]. Shareable definitions — tunings, scales, and comparable named
  resources — carry a content identity derived from their defining content, excluding
  name and description. Import reuses a content-identical definition rather than
  duplicating it.
- R-1413 [P2]. Material may be auditioned before import, under either its own context
  or the receiving project's context (tuning, tempo, instrument).

## 16. Non-functional

- R-1501 [Arch]. Live paths add no buffering beyond the configured device buffers:
  system-added latency on a live path is the device round trip plus at most one engine
  block. (The measurable form of R-304/R-716.)
- R-1502 [P1]. Internal event scheduling is sample-accurate against the tempo map;
  MIDI output jitter shall not exceed 1 ms under normal load. [Provisional figure.]
- R-1503 [P1]. Core model and generation are bit-identical across platforms and build
  targets (native, WASM), verified by the conformance suite as a release gate (R-104,
  R-1101).
- R-1504 [P1]. The TMON test, formalized: forced termination at any moment — including
  mid-recording, mid-playback, mid-gesture — loses no committed gesture and at most 5
  seconds of journal on reopen. [Provisional figure.]
- R-1505 [P1]. Soak: 24 hours of continuous playback and generation without audio
  degradation, timing drift, or unbounded memory growth. Endless mode targets
  multi-day operation [P2].
- R-1506 [P1]. Scale: projects of 10^5 events remain fully responsive in editing,
  queries, and views; 10^6 events remain usable. [Provisional figures.]
- R-1507 [P1]. Cold start to first sound in under 3 seconds on target hardware.
  [Provisional figure.]
- R-1508 [P1]. Edit-to-audible: a committed edit sounds at the edited material's next
  occurrence (R-906); gesture feedback renders at display rate.
- R-1509 [P1]. Persistence is imperceptible: journaling and store writes never block
  the UI or real-time threads.
- R-1510 [P2]. Accessibility: the UI exposes a platform accessibility tree; core
  workflows are screen-reader operable.
- R-1511 [P1]. Quick change: creating a new project and forking the current project
  are near-instant operations (target: under one second [provisional]). A fork is a
  complete copy of current state, provenance-linked to its origin, taken without
  interrupting playback.
- R-1512 [Arch]. Nothing gates the start: startup and project opening are never
  blocked by licensing checks, network access, plugin scanning, or content
  registration. Scans and validations run in the background against caches (R-1211);
  any licensing this product ever has shall never delay or interrupt creative work.
- R-1513 [P1]. Capture before commitment: on launch the application is playable — a
  default instrument sounding, retrospective capture running (R-814) — before and
  regardless of any project decision. Material played before a project exists is
  retained and attachable to a new or existing project.
- R-1514 [P1]. Distributed builds carry a complete third-party attribution document:
  every dependency and every bundled asset, its licence, and — where the licence
  offers a choice — the terms adopted. It is generated from the dependency tree and
  the bundled-asset manifest, never maintained by hand.
- R-1515 [P1]. The application provides an About view that credits by name the authors
  of bundled creative work — typefaces, icon sets, and comparable assets —
  independent of whether their licence requires attribution. The complete third-party
  attribution document (R-1514) is reachable from it.
