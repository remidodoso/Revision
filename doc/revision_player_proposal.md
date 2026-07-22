# ui-08 proposal — The Player and Control Bar: a drivable workspace mockup

**Status: approved 2026-07-22 (decisions 1–12); building.** One scope trim on
approval: **both** of Vision's "info" areas are cut from the mockup — the top
**Information toggle** (SMPTE Offset / Sync Mode / Sequence Start) *and* the
bottom **Expanded Info Area + Strip Chart**. Block-parameter editing (a selected
instance's transpose/loop/length) returns later as a **modern inspector**, not a
Vision-style drawer; controller editing (the Strip Chart) is later still.

**Original status: proposed 2026-07-22.** Checkpoint per getstarted rule 2: a new
subsystem and window (**The Player**), new widgets and their public shapes, new
code organization, and the first real exercise of **multi-window** policy. No
model/store binding and no schema change — every piece of Player *content* is
fake data in this item; ui-09 wires it to the store.

**Aim.** Stand up a **drivable workspace** — one or more Player windows (our
rendition of Vision's Tracks Window, Ch. 25) plus a **Control Bar palette** — on
the real `Kit`/`Pane`/`Host`, and use it to (a) settle the Player's anatomy and
its new widgets against a known-good reference, and (b) get the "lots of windows"
model right *early*: focus/raise, palette-doesn't-steal-front, and
**space-plays-the-focused-Playable**, with each window keeping its own transport.

**A mixture, not a pure mockup.** The **transport genuinely works** — we reuse
`rev-studio`'s Start/Stop/Locate/`TakeChunk` path, so pressing space actually
plays the *focused* Playable and switching focus switches what you hear. Only the
Player's *content* (its track rows and overview blocks) is mocked, with fake data
shaped to the eventual real queries so ui-09 is a source swap, not a rewrite.

---

## 1. Scope

**Implements (real):** the Player window shell and its two-pane layout; the new
widgets (§5); a Control Bar **palette** window re-housing `rev-studio`'s working
transport; the multi-window workspace rig (§9); and a `rev-player` demo binary
you can open several of.

**Mocked (fake data, to the eventual shape):** the Player's track rows and the
Track Overview blocks. A hand-built fixture, not the store.

**Deferred to later items:** wiring the Player to real material (ui-09); the Grid
Editor (ui-10); drag-to-arrange (ui-11); recursive realization for nested
playback (a core/sched item, when nested playback is wanted); the Control Bar's
full *wired* function (ui-04).

**Out entirely for now:** the Strip Chart's controller-editing internals; SMPTE
Offset / Sync Mode machinery (the fields may appear as dead chrome, or be cut —
§4); real drag-drop.

---

## 2. Sources, and what each governs

The same three-source recipe the Control Bar slice used, applied to the Player:

- **Vision Ch. 25 "The Tracks Window"** supplies the **anatomy and behaviour** —
  the two-pane split, the column set, the Track Overview's blocks and display
  modes, the tools, the menu. Read from the source (no_git/), not recalled.
- **Notorolla's Tile Player** supplies **interaction discipline** — references
  not copies, one shared time axis, and *preview == commit* (a drag's ghost runs
  the same primitives it will commit).
- **Our kit + `Pane`** (ui-03/07) and the **HIG** (windows; R-939) supply the
  **look and the window behaviour**. The gray 1999 chrome is *not* the target
  (§11); the screenshot is a layout reference.

Recipe: **Vision's model, modern ergonomics.** Keep the recursive/two-pane
structure; drop the dated interactions (single-window-replace, one-record-arm,
modifier-click overload); paint in our skin.

---

## 3. What already exists (so this is mostly assembly)

- **Widgets** (ui-03): `Counter`, `PopUp`, `Toggle`, `Button`, `Readout`,
  tri-state `Record`, `Lamp`. The header strip and the Control Bar are largely
  these.
- **`Pane` + `PaneArtist`** (ui-07): a scrollable/zoomable region the kit frames
  and the app paints — exactly how `roll::paint` draws notes. The Track Overview
  is a `Pane` with a new painter.
- **Working transport** (`rev-studio`): compile a phrase → `TakeChunk` → Start;
  the Counter-follows-clock and playhead machinery. The Control Bar reuses this.
- **Multi-window `Mech` API** (ui-01): `open_window`/`close_window`/`window_id`,
  all `WindowId`-keyed, `WindowRole::{Document, Palette}`, focus/capture/hover as
  keyed tuples, activation ≠ focus. Opening N windows + a palette is supported
  today; the **gap** is document z-order / raise-on-click policy (§9).

The genuinely new engineering is two widgets — the **track-list table** and the
**Track Overview block painter** — plus a **splitter** and a **tool palette**.

---

## 4. The Player window anatomy (our rendition of Ch. 25)

Three bands, top to bottom:

1. **Header strip** — Meter / Tempo / Seq-Len readouts, the In/Out point
   Counters, Display-mode and Silence pop-ups, the cursor-position readout, and
   the **tool palette** (Arrow / Marquee / I-beam). Almost all existing widgets.
2. **Body — two panes, one window.** Left: the **track-list table** (columns).
   Right: the **Track Overview** (`Pane` + block painter). A draggable
   **splitter** between them; a toggle to hide the overview. **The two panes
   share one vertical row layout** — a track's table row and its overview lane
   are the same height and the same y, and the two scroll vertically *together*;
   only the overview scrolls horizontally (time). This shared-row coupling is the
   "two windows in one" made concrete, and it is a real layout constraint the
   design has to honour.
**Cut on approval (both "info" areas):** the top **Information toggle** (Sequence
Information — SMPTE Offset, Sync Mode, Start Point) and the bottom **Expanded Info
Area + Strip Chart** are both omitted from the mockup — not even stubbed. Sync
Mode is really our polytempo (R-416) and Start Point a pickup, but neither belongs
in this window now; block-parameter editing returns later as a **modern
inspector**, not a Vision drawer, and controller editing (Strip Chart) is later
still. The mockup is the two panes + the header strip.

---

## 5. New widgets

### 5.1 The track-list table (the big one)

A **table of widgets**, not a monolithic painted grid — so cells reuse the kit
(a `Toggle` for Mute/Solo, a `PopUp` for Instrument/Patch, a `Counter` for Len)
and inherit a11y for free. The kit owns:

- a **header row** whose columns can be **reordered** (drag a header),
  **resized** (drag a header edge), and **shown/hidden** (a menu);
- the **column model** — columns are *data*: `Column { id, header, width, min,
  visible, order }`, a `Vec` the app supplies and the user mutates;
- laying each row's cells into the visible columns in order, aligned to the
  shared row grid (§4).

This is the widget the "columns are variable, hideable, reorderable" requirement
lives in, and it is the largest new piece. For the mockup, cells hold simple
fake content; the *shape* (a cell hosts a kit widget) is real so ui-09 drops real
widgets in.

### 5.2 The pane splitter

A draggable vertical divider between the two panes; double-click snaps it fully
left/right (Vision's behaviour). Small, self-contained, reusable for any future
two-pane view.

### 5.3 The tool palette

A radio group (Arrow / Marquee / I-beam) — exactly one active. Either a small new
`Kind` or a convention over `Button`; it reports the chosen tool. Trivial.

### 5.4 The Track Overview block painter

Not a new widget — a new **`PaneArtist`**, the multi-lane cousin of
`roll::paint`. Over one `Pane` (free horizontal scroll/zoom, vertical synced to
the table), it paints a **ruler** and, per lane, the lane's **blocks**. The
render input is a flat list of:

```
Block { lane, start_beat, len_beats, name, color, kind, is_alias }
```

For the mockup, a varied fixture: a held-note lane, an **alias/reference block**
(the recursive-arrangement case, drawn distinctly), and a controller envelope —
enough variety to prove the painter. Detailed-vs-patterned rendering (Ch. 25's
toggle) can start as patterned rectangles with a name.

---

## 6. The column model (per your requirement)

Columns are data and user-arrangeable. The **candidate set** from Ch. 25, of
which the mockup shows a subset with fake values:

`Select/Move · Record · Mute · Solo · Name · Len (+loop) · Drum · Instrument ·
Patch · Comments`

Reconciled with our model (from the design discussion): the track is a **pure
agnostic container**, so **Instrument/Patch are a routing view, not track
fields** — the Instrument column shows a track's *optional default instrument*
(Notorolla's lane-instrument idea), "—" when unset, "Multi" reserved for later
per-event routing. Mute/Solo read as session state, not schema. None of this adds
a track field in this item (it's all mocked), but the columns are designed to
reflect that model so ui-09 binds cleanly.

The mockup **demonstrates** hiding a column, dragging a header to reorder, and
resizing — the interactions, on fake rows.

---

## 7. Fake data to the eventual shape

Two fixture structs, designed to be exactly what ui-09's queries will hand the
widgets, so wiring is a source swap:

- `TrackRow { name, default_instrument: Option<..>, len, looped, muted, soloed,
  … }` — mirrors a `Track` + its routing/session view.
- `Block { lane, start_beat, len_beats, name, color, kind, is_alias }` — mirrors
  a realized `PhraseInstance` on a track (at_tick/length/loop → beats; is_alias =
  it references a structured phrase).

Mock to the eventual shape, never a convenient shape.

---

## 8. The Control Bar palette

A **palette window** (`WindowRole::Palette`) re-housing `rev-studio`'s transport
strip: the tri-state Record light, Play/Stop, the Counter. Its transport
**works** — it drives the engine for the focused Playable (§9). It obeys the
Control Bar's founding law (R-907): **always active, and operating it never
raises it in front of the document you're working on** — which is exactly what
makes "space plays the focused Player" stable when you click a transport button.
Its full wired function (record arm routing, tempo, loop, locators) is ui-04; here
it is transport + the palette behaviour.

---

## 9. Multi-window and the Playable model

The reason to build the workspace early is to get this right while it is cheap.

**Playability is a capability, not a window kind.** The transport targets "the
current **Playable**," never a Player specifically — a Player is Playable, and so
will be the Grid Editor and future things, each by exposing the same tiny
contract (give the transport a phrase/schedule; own a transport state). For this
item the Player is the only Playable, but the seam is drawn so it never
special-cases Player.

**The rules to prove (with several Players + the Control Bar on screen):**
- clicking a **document** window raises it and makes it the current Playable;
- clicking the **Control Bar** (a palette) operates it **without** stealing front
  or changing the target — so the target is "the frontmost *document* Playable,"
  palettes excluded (R-907);
- **space** always plays the current Playable, which always exists;
- each Playable keeps **its own transport state** (playhead, loop), so switching
  focus resumes that thing where it was;
- an optional **pin** freezes the target (edit one phrase while another keeps
  playing) — modelled as `transport_target: Option<WindowId>`, `None` = follow
  focus.

**What the `Mech` has vs. needs.** It has the windows, roles, and focus tuples;
it lacks a **document z-order / raise-on-click** policy. Proposal: lean on
winit/OS native raising first, and add a *minimal* `Mech` rule only if the native
behaviour is insufficient — "clicking a Document raises it and sets it current;
Palettes never become the current document." The mockup is precisely the rig that
tells us whether the native behaviour suffices; any needed `Mech` addition is
identified here and kept to that one rule.

---

## 10. Does the transport make sound in the mockup?

To make "space plays the focused Playable" a *real* test rather than a moving
line, each Player window is backed by a **real fixture phrase** (e.g. MHALL, or a
small purpose-built arrangement) that the transport compiles and plays — so
focus-switching between two Players is *audible*. The displayed columns/blocks
stay fake (the mixture). **Recommended: yes, real sound** — it reuses
`rev-studio`'s compile+play and is the whole payoff. (Alternative: playhead-only,
simpler but unconvincing — a decision, §13.)

---

## 11. Skin

The mockup paints in the **real kit's skin** (`Skin::default`); the screenshot is
a **layout/proportions reference, not a pixel target**. A gray-faithful-first
pass was considered and declined: building on the real kit gives us our look for
free, and a throwaway gray surface would be exactly the throwaway we've avoided
everywhere else. (If you want gray-faithful for one region to check proportions,
that's a local call, not the item's posture.)

---

## 12. Verification

- **UI by eyeball** (getstarted stage-4 rule; no UI automation): open several
  `rev-player` windows + the Control Bar, drive the divider, tools, column
  show/hide/reorder/resize, scroll/zoom, block-select, and confirm the
  multi-window rules of §9 (raise, palette-no-steal, space-plays-focused, per-
  Playable transport).
- **Widget *logic* by unit test**, as `pane.rs` did: the column model
  (reorder/show-hide/resize math), splitter geometry, the shared-row layout, and
  the block layout (beats→x, lane→y). The *painting* is eyeballed; the *math* is
  tested.
- **Gate**: fmt, clippy, filemap/plan, the suite — all green.

---

## 13. Decisions to approve

1. **Scope**: a drivable *workspace* mockup — Player window(s) + Control Bar
   palette — a **mixture** (transport works, content mocked). Model binding is
   ui-09.
2. **Two panes share one vertical row layout** (columns and overview scroll
   together vertically; only the overview scrolls horizontally). The splitter
   divides them; an overview toggle hides the right pane.
3. **New widgets**: a **track-list table** (a table *of kit widgets* with a
   data-driven, reorderable/hideable/resizable **column model**); a **pane
   splitter**; a **tool palette** radio; and a **Track Overview `PaneArtist`**
   (not a new widget) rendering blocks/aliases/envelopes.
4. **Columns are data** and the mockup demonstrates reorder/hide/resize;
   **Instrument/Patch are a routing view** (optional per-track default), not track
   fields — nothing added to the schema in this item.
5. **Fake fixtures (`TrackRow`, `Block`) are shaped to ui-09's eventual queries**
   so wiring is a source swap.
6. **The Control Bar is a palette window** re-housing `rev-studio`'s **working**
   transport, obeying R-907 (always-active, non-focus-stealing). Its wired
   function stays ui-04.
7. **Playability is a capability**; the transport targets the frontmost
   **document** Playable (palettes excluded), space always plays it, each Playable
   keeps its own transport state, with an optional pin.
8. **Multi-window policy**: lean on native raise first; add at most one minimal
   `Mech` rule (Document raises + becomes current on click; Palette never becomes
   the current document) if native isn't enough. The mockup is the rig that
   decides.
9. **The transport makes real sound** (each Player backed by a fixture phrase) —
   recommended over playhead-only.
10. **Skin is the real kit's**; the screenshot is a layout reference; gray-faithful
    declined.
11. **Both "info" areas are cut** from the mockup — the top SMPTE Information
    toggle and the bottom Expanded Info Area + Strip Chart. Block-parameter
    editing returns later as a modern inspector; controller editing later still.
12. Delivered as a **`rev-player`** binary; UI eyeballed, widget logic unit-tested.
