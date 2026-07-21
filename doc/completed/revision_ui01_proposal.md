# ui-01 proposal — mechanism contract as API + widget kit shape

**Status: approved 2026-07-20** — all fifteen decisions as recommended, plus the
amendments in §13, which were settled in the same discussion and are part of the
approval. Checkpoint per getstarted rule 2: public API between crates, code
organization, four new dependencies, and bundled assets.

Turns `revision_poc.md` §"Mechanism contract v0 sketch" into concrete Rust, and
settles two things the sketch left open: the renderer, and how much of the eventual
multi-window, multi-document application has to be visible in a v0 API.

Two discussions of 2026-07-20 drive this revision, and both are recorded here rather
than left in chat, because they are structural:

1. **The renderer is CPU rasterization** (§2).
2. **The application will be multi-document with pinned and following views.** The
   first pass is single-document — but only in the sense that N = 1. Nothing may be
   *written* single-document and later ripped out. §3 enumerates exactly what that
   costs, and §4 lists the invariants that pay for it.

## 1. Scope

**In:** the `rev-ui-mech` public API (windows, surfaces, frame loop, input routing,
focus, capture, cursor, painting, text, drag sessions, the hands-off clause, the
accessibility slot); the `rev-ui-kit` architecture (slot tree, retained widget tree,
widget trait, intents, skin, anchoring); the dependency set; the boundaries between
mech, kit, and app.

**Out — deferred implementation, but *not* deferred API shape:** more than one window
open at a time, more than one project open at a time, docking, tear-off, tabs. The
API is built for N and exercised at N = 1 (with one deliberate exception, §8.1).

**Out entirely for the PoC:** drag-and-drop between processes, clipboard, touch and
pen, native menus, accessibility population, the browser backend, and every widget in
the Control Bar census (those are ui-03).

**Out of this proposal's authority:** application policy — what "active project"
means, how activation is performed, where workspace state is persisted. §3 records
the settled shape because it constrains this API, but the normative home for it is a
requirements block (R-921+), which does not yet exist and should be written next.

## 2. The renderer: CPU rasterization

**Decision: softbuffer for the window surface, tiny-skia for painting.** GPU
acceleration of 2D buys three things — fill rate on full-frame redraw, smooth zoom
and scroll of a large canvas, and per-pixel math. Only the second and third describe
anything in this application (arrangement, roll, and the spectrally-colored waveform
overview), and none of them describe a Control Bar. Against that:

- **"2D on the GPU" means writing a 2D rasterizer on the GPU.** Rectangles and a
  glyph atlas are easy; antialiased strokes, curves, and clipping are hard enough
  that they remain research projects. tiny-skia is a finished, dependency-light port
  of Skia's rasterizer that draws what it is asked, today.
- **Determinism.** CPU output is bit-identical across machines, so widget rendering
  can be screenshot-tested exactly in CI — a regression net that matters for a
  hand-rolled kit. GPU output varies by vendor and driver, reducing golden masters to
  tolerances or to nothing.
- **No driver surface**: no device-lost path, no shader compilation stall, no
  vendor-specific bug class on the platform where that class is worst.
- **Half the dependency gesture** (§5).

The choice is deliberately reversible, and the seam that makes it so is required
anyway: the kit-facing API is a **paint list**, because the contract already forbids
native handles leaking (a browser-canvas backend is anticipated). When a view earns
GPU, the move is not rewriting the painter but presenting tiny-skia's output as a
texture through wgpu and adding shader-drawn layers only where per-pixel math pays —
the waveform overview gets its shader; no widget notices.

**Consequence, promoted from "later" to v0 design:** the API carries **dirty
rectangles** from the start. It is the difference between a renderer that scales with
content and one that scales with the user's monitor, and retrofitting dirty tracking
into a widget kit that never expected it is unpleasant. ui-02 may paint whole frames;
the API may not assume it.

## 3. The multi-document shape this API must not foreclose

Settled by discussion; stated here as the constraint set, not as a feature list.

**Three orthogonal properties of a project**, deliberately not conflated:

- **Open** — a live `Project` handle exists. Any number.
- **Active** — engine-attached: owns the transport, receives MIDI, makes sound.
  **Exactly one, by construction.** Changed only by explicit, deliberate activation —
  never by window focus.
- **Editable** — read-write or read-only. Orthogonal to both.

A read-only reference project (browse an old sketch, steal its bassline) is
`open + inactive + read-only`; a Cubase-style second document is
`open + inactive + read-write`. **Same architecture, different policy** — which is why
the first pass can ship the restrictive setting without foreclosing the general one.

**Active is not the edit target.** The active project owns the transport; the *focused
view's* project is what verbs apply to. This is a safety property, not a
complication: the undoable operation follows your attention, the one that commits a
take to disk requires a deliberate act. Selection is per-project; R-909's "one
selection, many lenses" holds within a project.

**Views bind, they do not own.** A view instance is either **Follow** (retargets to
the active project) or **Pin(project)**. Both are the same object: a view holds
per-project state keyed by project, and the binding decides which key is read — so
pinning a following view loses nothing. Binding is usually implied by subject (a
phrase editor is pinned by the fact of its subject; the Control Bar follows by
necessity, since only the active project has a transport); the visible pin control
appears only on the small class of views whose subject is *a project as a whole* —
mixer, arrangement, tempo map, tuning list, project settings.

**Cross-project transfer is a fragment** — a selection closed over its dependencies —
carried by drag, clipboard, file, or import alike. It is always a copy: two projects
are two SQLite databases with no shared transaction, so no cross-project move is
atomic, and the design says copy rather than pretending otherwise.

**Two layers of layout state**, neither journaled (nobody wants Undo to move a
window):

- **Workspace** — application-level, plus named templates: window and pane geometry,
  which views exist, their bindings. Global, because a following view belongs to no
  project.
- **Per-project view state** — in the project file: scroll, zoom, visible strips,
  keyed by view identity. Restored into whatever views bind to that project.

## 4. Invariants — the cheap-now, expensive-later list

These are the only things that must be right from the first line of UI code. They
are what "don't code single-document things that get ripped out" reduces to, and each
costs approximately nothing today:

1. **No global project.** Nothing reaches a project except through a handle it was
   given. One global here turns multi-document from a policy flag into a refactor.
2. **No view captures a project at construction.** A view resolves its project
   through its binding *at the point of use*. Violate this once and every following
   view is a dangling reference.
3. **Windows are keyed, never singular.** `WindowId` appears in the mech API from the
   start; DPI, dirty regions, focus, cursor, and the accessibility tree are
   per-window state, never process state.
4. **DPI is per-window and mutable.** A window dragged between monitors changes scale
   factor mid-life. A stored scale factor anywhere outside the backend is a bug.
5. **A view never knows where it lives.** No view constructs a window, asks whether
   it is docked, or holds screen coordinates; it receives a context and paints into
   it. This single rule is what makes window/pane/dock/tear-off placement decisions
   rather than rewrites — including the Control Bar, which ships as a window and may
   later become a dockable pane without being touched.
6. **The engine interface is session-keyed.** One device, therefore one engine, with
   projects as clients. N = 1 sessions for now. *(Not this proposal's to decide —
   flagged for eng-01 before it is drafted.)*

## 5. Dependencies

Four crates, measured rather than recalled (`cargo metadata`, resolved for
`x86_64-pc-windows-msvc`):

| Crate | Version | Role |
|---|---|---|
| `winit` | 0.30 | windows, event loop, DPI, keyboard/pointer source |
| `softbuffer` | 0.4 | CPU framebuffer presented to a window |
| `tiny-skia` | 0.12 | rasterizer: paths, fills, strokes, clips, blending |
| `cosmic-text` | 0.19 | shaping, font enumeration, measurement, line breaking |

**70 packages** resolve on Windows. The same stack with wgpu instead resolves **124**,
and the 54-package difference is precisely the platform-backend and shader-compiler
layer. The distribution is lopsided and worth knowing: winit ~13, softbuffer ~7,
tiny-skia 7 (including a PNG encoder — see §9), and **cosmic-text ~44**. Text is the
bulk of this checkpoint; the window and the painter are cheap.

winit is **already multi-window shaped** — a `WindowId` accompanies every event.
Invariant 3 is therefore not extra work; assuming a single window would mean actively
discarding what the dependency provides.

**Licences: all permissive, none copyleft-only.** Four families are new to the
project and join `deny.toml` with this checkpoint:

- **`Unicode-3.0`** (Unicode character tables).
- **`Zlib`**, **`BSD-2-Clause`**, **`BSD-3-Clause`**, **`0BSD`**.
- `self_cell` is **`Apache-2.0 OR GPL-2.0-only`**. We take the Apache side, so the
  GPL-free standard holds — but a dual licence with a GPL arm is named explicitly in
  the allowlist rather than passing silently, so the audit stays honest.

Default features are taken as-is except where noted; cosmic-text's editor extras
(`syntect`, `vi`, `modit`) are opt-in and stay off — we are importing a text shaper,
not a text editor.

## 6. Crate boundaries

**`rev-ui-mech` owns every external dependency in the UI stack.** Windows, input,
surfaces, rasterizer, and text all live behind its API.

**`rev-ui-kit` depends on `rev-ui-mech` and nothing else** — no winit, no tiny-skia,
no cosmic-text, no `windows-sys`. This is the operative form of "no native handle
ever leaks": the kit *cannot* reach a native handle, because it cannot name one. It
also means the browser backend, when it comes, replaces exactly one crate.

**`rev-app`** depends on both, owns views, bindings, the active project, and the
mapping from intents to commands. No musical logic in either UI crate.

The division of the placement problem follows the same line: **mech owns windows**,
**kit owns the slot tree inside a window** (splits, tabs, splitters — pure geometry
and interaction), **app owns views** and what they are bound to.

## 7. `rev-ui-mech` — the contract as API

Sketched to fix shapes and vocabulary, not to pre-write ui-02. All geometry is in
**logical pixels** as `f32`; device pixels exist only inside the backend, so DPI is
handled in one place.

### 7.1 Windows, frame loop, time

```rust
pub struct WindowId(u32);
pub struct UiTime(pub f64);          // monotonic seconds since start; UI clock only

impl Mech {
    pub fn open_window(&mut self, spec: &WindowSpec) -> WindowId;
    pub fn close_window(&mut self, w: WindowId);
    pub fn scale_factor(&self, w: WindowId) -> f32;     // per-window, mutable
    pub fn mark_dirty(&mut self, w: WindowId, r: Rect);
}

pub trait Host {
    fn hit(&self, w: WindowId, at: Point) -> Option<TargetId>;
    fn event(&mut self, w: WindowId, target: Option<TargetId>, ev: &Event,
             mech: &mut Mech);
    fn paint(&mut self, w: WindowId, painter: &mut Painter, dirty: &Dirty);
    fn a11y(&self, w: WindowId) -> A11yTree;   // v0: structure only
}
```

The mechanism drives; the host (in practice the kit) hit-tests and paints. A frame
happens at vsync **when something is dirty**, never on a timer, and dirtiness is
per-window. The **UI clock is not the engine clock**: engine positions arrive over
the telemetry ring and are consumed at frame start.

`WindowSpec` carries the role — ordinary window, or **palette** (floats above its
owner, never takes activation). The Control Bar is a palette; that it is a window at
all is an app decision, and §4.5 is what keeps it one.

### 7.2 Events

```rust
pub enum Event { Pointer(Pointer), Key(Key), Text(Text), Window(WindowEvent) }

pub struct Pointer { pub kind: PointerKind, pub at: Point,
                     pub button: Button, pub modifier: Modifier, pub time: UiTime }
pub enum PointerKind { Down, Move, Up, Wheel { dx: f32, dy: f32 }, Enter, Leave }
```

- **Implicit capture is structural**: a `Down` binds the hit target, and every
  subsequent `Move`/`Up` routes there until release, regardless of where the pointer
  travels — across window boundaries included. Widgets cannot forget to do this,
  because they are not asked to. (Notorolla's pointerdown-vs-click lesson, made
  physics.)
- **Wheel targets the hovered widget**, not the focused one.
- **Keyboard and text are two channels.** `Key` carries physical key, logical key,
  and modifiers, for bindings and key equivalents; `Text` carries composed,
  IME-mediated characters for fields. The split exists in v0 even though IME hookup
  is deferred — numeric fields (Counter, Tempo, In/Out) consume `Text`.

### 7.3 Focus, activation, and the hands-off clause (R-907)

```rust
pub enum Reason { User, Programmatic }   // every focus move is one or the other
impl Mech {
    pub fn focus(&self) -> Option<(WindowId, TargetId)>;
    pub fn set_focus(&mut self, to: Option<(WindowId, TargetId)>, why: Reason);
    pub fn request_cursor(&mut self, w: WindowId, shape: CursorShape);
    pub fn begin_relative_drag(&mut self) -> RelativeDrag;   // the one sanctioned warp
}
```

- **Activation ≠ focus.** Pointer-down operates a widget *without* moving focus and
  without changing window z-order. Focus moves only if the target asks. This is the
  Control Bar's always-active behavior generalized into a routing property available
  to every control — and it is what allows a palette to be clicked without disturbing
  which document window the user is working in.
- **The mechanism never moves the pointer, never steals focus, never scrolls
  unbidden.** The single exception is offered *by* the mechanism —
  `begin_relative_drag` hides the cursor, delivers deltas, and restores the position
  on drop (infinite knob drag) — so no widget rolls its own warping.
- **The Scroll Law at one chokepoint:** scroll offsets belong to scroll containers
  and change only through a user gesture or a call explicitly marked `Programmatic`.

### 7.4 Painting

```rust
impl Painter {
    pub fn fill_rect(&mut self, r: Rect, color: Color);
    pub fn fill_round_rect(&mut self, r: Rect, radius: f32, color: Color);
    pub fn stroke_line(&mut self, a: Point, b: Point, color: Color, width: f32);
    pub fn fill_path(&mut self, p: &Path, color: Color);
    pub fn draw_text(&mut self, t: &Shaped, at: Point, color: Color);
    pub fn push_clip(&mut self, r: Rect);    pub fn pop_clip(&mut self);
    pub fn push_offset(&mut self, d: Point); pub fn pop_offset(&mut self);
}
```

A small, closed vocabulary — deliberately implementable by a browser canvas without
translation loss. `Dirty` is passed into `paint` so a kit may skip clean subtrees.

### 7.5 Text

```rust
impl Mech {
    pub fn shape(&mut self, s: &str, style: &TextStyle) -> Shaped;
}
impl Shaped {
    pub fn size(&self) -> Size;
    pub fn caret(&self, byte: usize) -> Point;    // editable fields need both
    pub fn byte_at(&self, x: f32) -> usize;
}
```

Kit widgets never touch a font, a face, or a glyph. Measurement and hit-testing ship
in v0 because the Counter is editable from the first widget that matters.

### 7.6 Drag sessions

Designed now, implemented in-process only. A drag carries a payload advertising what
it is; a drop target advertises what it accepts; the mechanism negotiates.

```rust
pub struct DragPayload { pub kind: Vec<TypeTag>, pub data: DragData }
pub enum DragData { InProcess(Rc<dyn Any>), Serialized(Vec<u8>) }
```

The negotiation shape is what makes the in-process and (much later) OS transports
interchangeable, and it is what carries a **fragment** from one project's view to
another's. Worth knowing before anyone plans on it: winit can *receive* dropped files
but cannot *initiate* an OS-level drag portably — that is per-platform work in
`rev-ui-mech`, and choosing one process for N projects means it is never needed to
make cross-project drag work.

### 7.7 Threading and accessibility

The UI is single-threaded on the main thread; `Mech` is `!Send`/`!Sync` and does not
pretend otherwise. Cross-thread input arrives only through rings. The AccessKit node
channel is **declared** (`Host::a11y`, per-window) and minimally populated, so the kit
grows around it rather than against it (R-1510).

## 8. `rev-ui-kit` — shape

**A retained tree with stable ids.** Immediate mode was considered and rejected:
explicit focus ownership (R-907), synchronized lenses over one selection (R-909), and
a persistent accessibility node tree (R-1510) all require durable widget identity.
Rebuilding the world each frame also defeats dirty-rect painting, which §2 promoted
to a design invariant.

```rust
pub struct WidgetId(u32);            // stable across frames; TargetId is its mech face

pub trait Widget {
    fn paint(&self, ctx: &mut PaintCtx);
    fn event(&mut self, ev: &Event, ctx: &mut EventCtx) -> Option<Intent>;
}

pub enum Intent {                    // what happened, never what it means
    Pressed, Released, Toggled(bool), ValueChanged(f64),
    Committed(String), Chose(usize), Cancelled,
}
```

**Widgets emit intents; the application assigns meaning.** The kit has no idea a
model exists — it cannot construct a command, and the `(WidgetId, Intent)` pair is
all `rev-app` needs to build one. That is R-901 ("no privileged editor backdoor")
enforced by the type system rather than by discipline.

**The slot tree.** A window's content is a tree: interior nodes are splits or tab
stacks, leaves are slots holding a view. Panes and windows are then the same
structure seen at different depths — **a window is a root, a pane is an interior
node** — which is what makes docking and tear-off reparenting operations rather than
features. v0 builds the tree and a single leaf; splitters and drop zones are ui-03+
interaction work over an unchanged model.

**Views receive a context, never a window** (§4.5):

```rust
pub struct ViewCtx<'a> { pub size: Size, pub scale: f32,
                         pub focused: bool, pub dirty: &'a mut DirtySink }
```

**Layout is absolute and anchored**, not computed: the control skin is pixel-designed,
so a widget has a rect within its parent plus optional edge anchors for resize. No
layout engine in v0; if the arrangement view later wants one, it is additive.

**One `Skin` value** holds every color, metric, and radius, so the control-skin look
changes in one place and is never sprinkled through widgets.

## 9. ui-02 exit criteria

1. A window opens, resizes, and survives a DPI change with correct scaling.
2. **A second window opens and closes at runtime** — a debug affordance, not a
   feature. It is the only way to know invariants 3 and 4 are real rather than
   aspirational, and it costs almost nothing while the code is young. This is the one
   place the PoC deliberately exceeds N = 1.
3. The paint vocabulary of §7.4 renders: rects, rounded rects, lines, a path, and a
   shaped text run.
4. Pointer and keyboard events route through §7.2, with implicit capture demonstrated
   by a drag that leaves the window and still tracks.
5. Dirty rectangles are plumbed end to end, per window (whole-frame painting is
   acceptable; a frame with nothing dirty must not repaint).
6. The hands-off laws are demonstrably held, including one relative-drag probe.
7. A first **screenshot golden master** is committed and compared bit-identically.
8. The user eyeballs it. (UI is not machine-gradable.)

## 10. Small matters, decided here rather than mid-implementation

- **Screenshot format: PNG.** tiny-skia's PNG encoder is in its default feature set,
  so it costs nothing already counted, and unlike `testdata`'s raw `.f32` frames a
  rendering reference should be *lookable-at* when it fails. Sidecar JSON carries
  provenance as elsewhere.
- **Terminology: "dirty", not "damage"** — `Dirty`, `mark_dirty`, dirty rectangle,
  dirty region. Already recorded in the coding standard.
- New file-map entries land with the code, per the bookkeeping rule.

## 11. Consequences outside this proposal

Recorded so they are not lost, and so the checkpoints that own them see them first:

- **Requirements owe an R-921+ block**: open/active/editable, explicit activation,
  active-vs-edit-target, view binding, the two layout layers, fragments with
  copy-only cross-project semantics, audition-under-context. §3 is a summary of
  settled discussion, not a normative source. Note that R-921–923 were also drafted
  (unapplied) for the spectral waveform overview; one block must renumber.
- **eng-01**: session-keyed engine interface, one device, projects as clients
  (invariant 6) — needed before that proposal is drafted, not after.
- **core-01 revisit**: content identity for shareable definitions (a hash over a
  tuning's or scale's *defining* content, excluding its label), so importing a
  fragment can dedupe instead of accumulating fourteen indistinguishable 12-ETs.
  Cheap now, painful once files exist in the wild — the AUTOINCREMENT lesson again.
- **Per-project view state** needs an unjournaled table in the project schema.

## 12. Decisions requested

1. **CPU rasterization** — softbuffer + tiny-skia, paint-list seam preserved for a
   later GPU presentation path (§2). Recommended: yes.
2. **Dirty rectangles in the v0 API**, per window (§2, §7.1). Recommended: yes.
3. **The six invariants of §4** adopted as binding on all UI code — the operative
   form of "no single-document code that has to be ripped out". Recommended: yes.
4. **Windows keyed from day one** (`WindowId` in the API), implementation N = 1 plus
   the §9.2 probe (§4.3, §7.1). Recommended: yes.
5. **Views receive `ViewCtx`, never a window; placement lives outside the view**
   (§4.5, §8). Recommended: yes.
6. **Slot tree in the kit: windows are roots, panes are interior nodes** — model in
   v0, interaction later (§8). Recommended: yes.
7. **Four dependencies** — winit, softbuffer, tiny-skia, cosmic-text (§5).
   Recommended: yes.
8. **`deny.toml` additions** — Unicode-3.0, Zlib, BSD-2-Clause, BSD-3-Clause, 0BSD,
   and an explicit note for `self_cell`'s Apache-or-GPL dual (§5). Recommended: yes.
9. **`rev-ui-kit` depends only on `rev-ui-mech`** — every external UI dependency
   confined to the mechanism crate (§6). Recommended: yes.
10. **Retained widget tree with stable ids**, not immediate mode (§8).
    Recommended: yes.
11. **Widgets emit intents; only `rev-app` maps them to commands** (§8).
    Recommended: yes.
12. **Absolute/anchored layout, no layout engine in v0** (§8). Recommended: yes.
13. **Drag-session API with type negotiation designed now, in-process transport
    only** (§7.6). Recommended: yes.
14. **Screenshot golden masters as PNG**, compared bit-identically (§9.7, §10).
    Recommended: yes.
15. **The second-window probe as a standing ui-02 exit criterion** (§9.2) — the
    anti-rot test for the invariants. Recommended: yes.

## 13. Amendments, settled at approval

Recorded rather than folded silently, so the reasoning survives.

### 13.1 A correction to §5

§5 claimed four licence families "must join `deny.toml` with this checkpoint."
**They were already there** — boot-02's allowlist already carried `BSD-2-Clause`,
`BSD-3-Clause`, `Zlib`, `0BSD`, `Unicode-3.0`, plus `OFL-1.1` and `CC0-1.0`. The
claim was asserted from the census without reading the file. What actually remains is
smaller: an explicit **denial** of `BSD-4-Clause` (its advertising clause is the one
trap in the permissive family, and an explicit denial records that we looked) and the
`self_cell` choice recorded rather than inferred. `cargo-deny` is **not installed
locally**; the licence gate runs in CI only unless that changes.

Verified rather than assumed: **`raw-window-handle` unifies at 0.6.2** across winit
0.30 and softbuffer 0.4 — the classic winit-ecosystem split-version hazard is absent.

### 13.2 Bundled assets, and why they force a requirement change

Decision 14 (bit-identical screenshot golden masters) is **impossible with system
fonts**: two machines enumerate different font sets and rasterize different glyphs.
Bundling is therefore not a preference but a consequence.

**Typefaces** (both `OFL-1.1`, already allowed):

- **Source Sans 3** — Regular, Italic, Bold, Bold Italic. Humanist, open, generously
  spaced. Real italics ship because the fallback is a *synthesized oblique* — the
  upright skewed by the shaping stack — which is worse than absent.
- **JetBrains Mono** — Regular, Bold. Monospace makes tabular figures structural
  rather than optional, so the Counter cannot jitter, and `byte_at` in numeric fields
  becomes arithmetic instead of an advance-width walk. Italic omitted: its role is
  numeric and data display, where italics are not wanted. One file to add if that
  changes.

Both are taken from their upstream project releases (versioned, checksummed, licence
text alongside), never from a font-service API that serves silently-updated subsets.
Their CJK siblings (Source Han Sans, and Noto for coverage gaps) are the eventual
fallback path; DejaVu Sans was considered for its single-file breadth and set aside
because it would introduce the `Bitstream-Vera` licence family for a dated design.

**Icons: Lucide** (`ISC`, already allowed). Consumed as **build-time generated Rust
path constants** — an xtask reads a manifest of the icons actually used and converts
their SVG into paths, following the `schema`/`filemap`/`license` pattern. This is
deliberate avoidance: the natural reach is `usvg`/`resvg`, which is **MPL-2.0**, a
licence family we have not adopted and do not need for a fixed, known icon set.
Lucide's SVGs are regular enough (`path`, `line`, `circle`, `rect`, `polyline`) that
the converter is small.

**Transport glyphs are hand-drawn**, not Lucide. Play, stop, and record are
traditionally solid — a filled triangle, square, and circle — where Lucide draws thin
rounded outlines that read wrong against an instrument-style control skin. They cost
nothing to draw and match the skin exactly. Lucide serves the long tail, where its
consistency is the whole point.

**Layout:** `asset/font/` and `asset/icon/` (singular, per the naming standard), each
asset accompanied by its licence text and declared in an **asset manifest** carrying
source, version, and checksum.

**Requirement consequence:** `cargo-deny` audits crates and cannot see a bundled asset
at all, so R-1514 as written would have silently omitted every one of them. R-1514 now
reads "the dependency tree **and the bundled-asset manifest**", and **R-1515** is new:
an About view crediting the authors of bundled creative work by name, *independent of
whether the licence requires it*.

### 13.3 Font policy

- **Role-based selection from day one**: `FontRole::Ui` and `FontRole::Numeric`. Both
  resolve through the same table; no widget names a file. (Build for N, ship with the
  faces we have — the same discipline as windows and projects.)
- **Ligatures off.** JetBrains Mono is a coding face; left enabled, `!=` in a track
  name renders as `≠`.
- **System-font fallback is allowed at runtime and forced off in tests.** Users get
  their glyphs; golden masters stay bit-identical because the harness pins the bundle.
- **Golden masters are keyed to a toolchain.** A cosmic-text or swash bump can
  legitimately shift antialiasing and invalidate every reference. Regenerating them is
  a deliberate, reviewed act — never a reflex when CI goes red.

### 13.4 §7.1 overclaims vsync

softbuffer blits; it has no present-timing or vsync mechanism, and true vsync would
mean `DwmFlush` on Windows — legal inside `rev-ui-mech`, but per-platform work nobody
needs yet. The v0 policy is therefore: **redraw when dirty, coalesced once per
event-loop wakeup, capped at the display refresh interval on a best-effort basis.**
True vsync waits for tearing to actually matter.

### 13.5 A borrow wrinkle in `Host`

`fn event(&mut self, …, mech: &mut Mech)` cannot work if `Mech` owns and drives the
`Host` — that is two mutable borrows of one graph. Resolution belongs in ui-02, where
the compiler participates: either `Mech` splits into a driver plus a services handle,
or the host returns requests rather than being handed the mechanism. Recorded here
because the §7 sketch as written does not compile, and a reader deserves to know that
before trying.

### 13.6 Golden-master location

`testdata/ui/` for screenshot references, alongside the audio references `testdata/`
was created for; the file-map description stops claiming audio-only when the first
one lands.
