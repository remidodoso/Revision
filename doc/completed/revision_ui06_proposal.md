# ui-06 proposal — the read-only piano roll

**Status: approved and implemented 2026-07-21** (all fifteen decisions). See §12 for what the screenshot caught that no test could.

**Original status: proposed 2026-07-21.** Checkpoint per getstarted rule 2: one new
requirement, and the first view that reads model, tuning and engine together.

Its predecessors did the hard parts. core-03 gives `v_realized`; eng-06 gives
the tick arithmetic; eng-07 makes it sound; ui-07 gives a pane that scrolls and
zooms a region the application paints. **This is mostly a matter of choosing
coordinates well and then not doing very much.**

---

## 1. Scope

**In.** Where the roll lives (§2). What a content unit is (§3) — the decision
everything else falls out of. How a note becomes a shape, resolved exactly as the
engine resolves it (§4). The degree ladder (§5). Time gridlines (§6).
The playhead, and the follow state machine (§7, §8). What the pane owes the
accessibility tree (§9).

**Out — deferred, shape not foreclosed.** Editing of every kind: no drag, no
snapping, no row model, no keyboard widget, no selection. Multiple tracks in one
view. Velocity display. Continuous-controller lanes. The spectral overview
R-942 promises to align with.

**Out entirely.** Anything that raises "which tuning does a dragged note belong
to". That question is the whole reason editing is a later item.

---

## 2. Where it lives

`rev-app`, as `src/rust/app/src/roll.rs`, with a demo bin alongside `pane.rs`.

Not a new crate: it needs `rev-store` (the events), `rev-sched` (the tuning
resolution and tick arithmetic), `rev-engine` (the position) and `rev-ui-kit`
(the pane) simultaneously, and a crate that depends on all four *is* the
application.

---

## 3. Coordinates — the decision the rest follows from

The pane is linear and knows nothing about music, so the roll chooses what a
content unit means. Choose well and the requirements stop being work:

- **y is `log2(hz)`.** Then R-941's continuous logarithmic axis is not
  implemented at all — it *is* the coordinate system. Octaves are evenly spaced
  by construction, unequal tunings draw unequal without a special case, 16-ET
  and 12-ET material overlay honestly (R-942), and the pane's existing
  logarithmic zoom lands exactly right. Nothing in the roll ever divides an
  octave by a number of degrees.
- **x is beats.** Not ticks, for two reasons that agree. The pane's offsets are
  `f32`, and at 5040 PPQ ticks leave exact integer representation after about
  27 minutes at 120 bpm — a two-hour piece is 72 million ticks and the *view*
  would start quantizing. In beats that piece is 14,400, with sub-tick precision
  to spare. And beats is the axis's meaning: it makes the horizontal the exact
  analogue of the vertical, both being the continuous musically real quantity
  with the model's integers underneath. `log2(hz)` is not a note number; a beat
  is not a tick.

  The model stays in ticks (R-003); the roll divides by PPQ, which is exact and
  **tempo-independent** — beats↔ticks is linear, and only *seconds* involve the
  tempo map. The engine's sample position is the one thing converted through it.

The vertical extent is the material's pitch range plus a margin, in log space;
the horizontal extent is the arrangement's length. Both are content, so the
pane's scroll and zoom limits are already right.

---

## 4. A note becomes a shape

`query::realized` gives `(at_tick, dur_tick, note_number, tuning_id)`. The
frequency comes from **`rev_sched::TuneCache`** — the same cache, on the same
path, that the compiler uses to produce what the engine plays.

That is the point, not an economy. R-312 says the engine receives frequencies
and never note numbers; if the roll resolved pitch its own way, the drawing and
the sound could disagree and nothing would notice. Sharing the resolution makes
**what you see is what you hear** structural rather than a coincidence that
holds until someone edits one of the two.

### 4.1 The shape

A note is drawn from its onset over its **duration** — which is what a note is
here (eng-06 §5.1) — as a **thick line with round end caps**: mechanically a
rounded rectangle whose radius is half its thickness, which the painter already
does.

**Round caps make legato legible.** Two notes where one ends exactly as the next
begins show a visible pinch between their caps rather than merging into a single
long bar. That is a virtue, not a side effect: it distinguishes two notes from
one, which a square-ended bar cannot.

**Thickness follows the ladder, without claiming a band:**

```text
thickness = clamp(0.6 × median degree spacing in pixels, 5, 18)
```

Zoomed in, notes get satisfyingly fat as on a traditional roll; they never fill
the gap between degrees, so they never imply that a note occupies a pitch
*range* — it has a frequency (§3). The **median** spacing rather than the local
one, so that thickness is uniform across a view: in an unequal tuning, per-degree
spacing would make notes randomly fat and thin for no reason the reader could
use.

### 4.2 Notes shorter than a pixel become circles

Below one pixel of drawn length, a note is drawn as a **circle** of the
thickness's diameter, its left edge at the onset.

This is the honest form rather than a compromise, and it is worth saying why. A
stubby capsule would read as *some particular short length*, which is exactly
what R-945 forbids of a positional readout — a thing that reads as something it
is not. A circle does not misrepresent the duration; it **declines to represent**
it, and says so by looking different. What stays truthful is the onset, which is
the coordinate that matters at that scale.

It is also a perfectly ordinary picture, not a degenerate case to apologise for:
percussion and other one-shot material is *mostly* notes like this, and a row of
dots is what such a part should look like. A user who wants to edit a length
zooms until lengths are visible, which is their business and not the view's.

---

## 5. The degree ladder (R-943)

Horizontal lines at each degree of the applicable tuning, labelled with the
tuning's own degree names where it has them and the degree index otherwise. A
degree without a conventional name is an ordinary case. Nearest-12-ET-with-cents
is an orientation aid and never the primary label.

**Pinned by the artist, not by the kit.** The ladder must stay put while the
view scrolls horizontally, and the pane has one interior with no frozen column.
The application paints the interior, so it can simply draw the ladder ignoring
the x offset. This is a deliberate choice rather than a discovery: adding frozen
columns to the pane would be a second layout system inside the first, for one
customer.

**Which tuning, when material has several** (R-942 says one view may hold
several) — the honest answer today is that MHALL has one, and a rule invented
now would be invented without a case. **The placeholder is stated rather than
implied:** the ladder shows the tuning of the track being displayed; when a view
holds several, the rule is chosen then, with material in hand.

---

## 6. Time gridlines

A **grid ladder**, the time analogue of §5's degree ladder: the subdivision gets
finer as the view zooms in, and at the deepest zoom one line is one tick. That
is where "ticks at extreme magnification" lives — as the finest rung, not as the
coordinate system.

**The rungs are divisors of the tick resolution**, not powers of ten. 5040 is
2⁴·3²·5·7, which is *why* it was chosen, so 2, 3, 4, 6, 8, 12, 16, 24, 32, 48 and
the rest all divide exactly and every gridline lands on a real tick. A
power-of-ten ladder — which is what the pane demo uses, correctly, for an
abstract grid — would put lines between the positions music can actually hold.

Periodic emphasis every N is **view state**: user-settable, carrying no model
meaning, and never a claim about meter (R-946). Default 4, because most music
most of the time.

**No bars.** The counter reads beats and subdivisions, and the display origin is
the user's preference (R-944), applied to counted fields and not to remainders.

---

## 7. The playhead

Driven by the engine's position snapshot — the seqlock, read once per frame,
latest-wins — converted from samples to ticks through the `TempoMap`.

**It is the only thing that moves during playback.** With a stationary view the
damage between frames is two thin columns, old position and new, so a frame of
playback costs a couple of thousand pixels instead of a couple of million. That
is what §8 is protecting.

---

## 8. Follow

### 8.1 The rule

The transport keeps the playhead in view. **A manual scroll takes that over**,
permanently, until the user says otherwise. This is ch. 1's first principle —
*"Allow the user, not the computer, to initiate and control actions"* — applied
to a conflict between the two: the user's scroll is an act, the follow is not,
and the act wins.

### 8.2 What makes "did the user scroll?" answerable

The trap is that follow moves the view too, so a naive implementation switches
itself off on the first frame of playback.

There is already an invariant that settles it: **the kit emits an `Intent` only
from input.** So the roll moves the pane by mutating it directly, and treats
*every* `Intent::Scrolled` as a user act. The distinction is structural, not a
flag someone must remember to set.

The same shape as `Reason::{User, Programmatic}`, which exists so that "nothing
steals focus" (R-907) is "checkable rather than aspirational". Same problem,
same answer — and no third mechanism invented for it.

### 8.3 The state machine

- **Armed** by default, and while armed the transport keeps the playhead in view.
- **Disarmed** by a *horizontal* scroll from the user. A vertical scroll does
  not: follow governs time, and looking at a high note makes no claim about
  where you are in the piece. (The book is adjacent: "if you can scroll in one
  orientation to reveal the selection, don't scroll in both.")
- **Not disarmed by zoom.** Zooming while following is normal. Instead,
  **while armed, all zoom anchors the playhead** — §6.2 already says keyboard
  and slider zoom do; following extends it to the wheel. Follow mode changes
  what stays still, rather than being defeated by it.
- **Re-armed** by an explicit *locate* — return-to-zero, a locator recall, any
  "take me there" — and by the toggle. **Not** by plain Play: pressing Play
  where you are looking must not yank the view. **Not** by Stop: you may have
  stopped precisely in order to go and look at something.

Follow is **view state** — per view, user-set, never stored in the project,
exactly as R-946 says of gridline emphasis.

### 8.4 How it follows, and the two numbers

**It pages. It never slides.** See §8.5.

- **`FOLLOW_TRIGGER = 0.80`** — the fraction across the viewport at which the
  view jumps. Its real job is that the playhead never reaches the edge: at 1.0
  you would watch it hit the wall and then stumble.
- **`FOLLOW_LAND = 0.50`** — where the playhead sits afterwards. Its job is
  history: half a viewport of what was just played stays visible.

Both are fractions of the viewport, **not durations** — a fixed time margin
would behave differently at every zoom, and a proportion is what the eye reads.

**The arithmetic, stated so nobody is surprised.** The jump is
`(trigger − land) × viewport` = **30 % of a view** at these defaults, not a
windowful, so it scrolls about 3.3× more often than "page scroll" usually
implies: roughly every 1.2 s for an 8-beat view at 120 bpm. Against 72 repaints
a second for smooth scrolling, this is still nothing.

**They are a constrained pair, not two sliders.** If `land ≥ trigger` the
playhead lands on or past the trigger and fires again immediately — the view
thrashes forever. A minimum separation of **0.15** is enforced where they are
set, not discovered during playback.

These supersede the one-beat paging overlap for this view: that mitigation was
designed for a playhead landing flush at the left edge, and half a viewport of
history is strictly more context.

**At the end of the piece** the content cannot scroll far enough to place the
playhead at `land`, so it clamps and the playhead runs to the right within the
final view. Correct, not a defect.

**They are the preference system's second known customer**, after R-944's
display origin. Carried here as named constants with these defaults, shaped to
lift into the settings store unchanged when it exists — including the *relation*
between them, which is the interesting requirement they impose.

### 8.5 The transport never animates the view (R-947)

Proposed as a requirement because it will otherwise be re-litigated by whoever
finds smooth scrolling pretty:

> No view scrolls itself smoothly. Motion the user did not ask for is motion the
> user pays for, in cycles and in attention.

Three reasons, in increasing order of importance:

1. **The renderer is a CPU rasterizer** (ui-01). A full-panel repaint measured
   18.6 ms in dev and 3.6 ms in release — the reason `rev-ui-mech` carries
   `opt-level = 2` in dev builds. Smooth scrolling makes every frame a
   full-panel repaint, at 4K.
2. **It defeats the damage tracking.** `take_dirty` exists so that a frame costs
   what changed. A stationary view during playback means the damage is the
   playhead — two thin columns. A sliding view means the damage is everything.
3. **The audio callback has a deadline and the UI does not.** A UI thread
   rasterizing the whole window at 60 Hz competes for memory bandwidth with the
   one thread that must not be late. During playback the correct posture for the
   interface is *nearly idle*.

It generalizes past the roll: ui-05's log tail advances by whole lines for the
same reason.

---

## 9. What the roll owes the accessibility tree

ui-07 §3 signed a debt here: the pane exposes itself as a scrollable region with
position and extent, and **the interior's semantics belong to its consumer**.
This is where it comes due.

The roll contributes notes **as data** — pitch label (§5's naming), onset and
duration in the user's positional convention (R-944) — not rectangles, and not
pixels. It is a small list for MHALL and a windowed one later; the obligation is
that it exists and is honest, not that it is complete for a million notes.

---

## 10. Tests

- **Coordinates**: a note's y is `log2(hz)` for its resolved frequency; two notes
  an octave apart are equidistant at every zoom.
- **Same-as-the-engine**: the frequency the roll draws at is bit-identical to
  the one the compiler hands the engine, for every note of MHALL, in 12-ET and
  in 16-ET. This is R-312's guarantee made checkable.
- **Retuning moves notes** (R-942): the 16-ET render draws at different heights
  and identical x positions — the party trick, seen instead of heard.
- **The follow state machine**, driven deterministically: armed → follows;
  horizontal scroll → stops; vertical scroll → still follows; zoom → still
  follows, anchored on the playhead; locate → follows again; Play alone → does
  not re-arm.
- **The jump lands where it should**: after a follow page, the playhead is at
  `FOLLOW_LAND ± a pixel`, and never at the edge.
- **The pair is constrained**: settings closer than the minimum separation are
  rejected, and a rejected pair cannot produce a repeating jump.
- **MHALL is too short to trigger follow at default zoom**, which is a fact
  about the fixture and not a limitation of the test: the test zooms until the
  viewport is a couple of beats and drives every transition explicitly. That is
  a better test than long material would give.
- **A golden screenshot** of MHALL on the roll, at 1× and 1.25×.

---

## 11. Decisions to approve

1. The roll lives in `rev-app` (`roll.rs`), with a demo bin. Not a new crate.
2. **Content coordinates are `log2(hz)` vertically and beats horizontally** —
   the continuous musical quantity on both axes, with the model's ticks
   underneath. Beats also keeps the view's `f32` offsets exact for pieces hours
   long, which ticks would not.
3. Pitch resolves through **`rev_sched::TuneCache`**, the same path the compiler
   uses — what you see is what you hear, structurally.
4. A note is a **thick line with round end caps** from its onset over its
   duration, thickness `clamp(0.6 × median degree spacing, 5, 18)` — fat like a
   real roll when zoomed in, never filling the gap between degrees, so never
   implying a pitch band.
5. **Under one pixel of length a note is a circle**, left edge on the onset. It
   declines to represent a duration rather than misrepresenting one, and it is
   the ordinary picture for percussion rather than a degenerate case.
6. The degree ladder is drawn by the artist, pinned against horizontal scroll;
   the kit gains no frozen-column machinery.
7. Multi-tuning ladders are an **explicit placeholder**: the displayed track's
   tuning today, the rule chosen when there is material that needs one.
8. A **grid ladder** whose rungs are divisors of the tick resolution (2, 3, 4,
   6, 8, 12…), finest rung one tick — which is where ticks are exposed. Periodic
   emphasis as view state (R-946), default 4. No bars.
9. The playhead is the engine's position through the `TempoMap`, and is the only
   thing that moves during playback.
10. Follow is armed by default; **a horizontal user scroll disarms it**, a
   vertical one does not, and an `Intent::Scrolled` is what "user" means.
11. **Zoom does not disarm follow; while armed, zoom anchors the playhead.**
12. Re-arm on an explicit locate or the toggle — not on Play, not on Stop.
13. `FOLLOW_TRIGGER = 0.80`, `FOLLOW_LAND = 0.50`, fractions of the viewport,
    with a minimum separation of 0.15 enforced where they are set.
14. New requirement **R-947**: the transport never animates the view.
15. The roll contributes notes as data to the accessibility tree.


---

## 12. Findings

**Two defects the golden caught and no assertion could** — which is exactly why
decision 19 of ui-07 insisted a view have a picture, and why this one does too.

1. **The degree labels lied.** The first version wrote `index/period` — "4/5"
   for note 64 of 12-ET. That is the xenharmonic notation for *four steps of
   5-EDO*, a specific and wrong claim about the tuning. A label that says
   something false is worse than a plain number, so labels are now the note
   number (a degree index, R-002) until `TuningNote` carries real names. R-943's
   "as well as they can be" is a number today because the model holds nothing
   better yet.
2. **The labels sat on top of the notes**, unreadable precisely where the
   material is densest. They moved to a pinned strip drawn behind them. Both were
   invisible to every geometry test and obvious at a glance.

**`mhall.rs` was extracted.** The tune moved out of the `rev-mhall` binary into
`app/src/mhall.rs` so `rev-mhall` and `rev-roll` share one definition rather than
two that drift. It could not come from `rev-testkit`'s fixture: that crate is
dev-only and a shipping binary cannot link it — the same rule that keeps the FFT
out of production.
