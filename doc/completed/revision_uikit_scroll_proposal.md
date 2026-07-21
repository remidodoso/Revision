# ui-kit scroll proposal — panes, scroll bars, and zoom

**Status: approved and implemented 2026-07-21** (all nineteen decisions). Findings, including two things decided but *not* built, are in §11.

**Original status: proposed 2026-07-21.** Checkpoint per getstarted rule 2: a new widget
kind, a new intent, and — the part that actually matters — the first place the
kit lets an application paint.

Its input is `doc/revision_hig_inventory.md` **§3a**, added today from a fresh
read of ch. 5 "Windows", pp. 158–167. Everything attributed to the book below is
quoted there; nothing here is recalled.

**Two items need this, which is why it is a checkpoint and not an inline
detail.** ui-05 is a scrolling log window; ui-06 is a piano roll. Built twice
they would differ, and the difference would be arbitrary.

---

## 1. Scope

**In.** A scrollable pane: what it is, how it clips, how it is scrolled, and how
its interior is drawn (§3, §4). Scroll bars — geometry, states, and the three
departures from 1992 (§5). Zoom, which the book does not have (§6). The wheel
(§7). What the pane does *not* do (§8).

**Out — deferred, shape not foreclosed.** Elastic/rubber-band overscroll.
Scroll-bar split controls (the book's one acceptable addition, p. 161).
Kinetic or inertial scrolling. Nested scrollable panes. Zoom animation.

**Out entirely.** The piano roll itself (ui-06) and the log viewer (ui-05). This
is the surface they are both built on, and it is designed for both rather than
for either.

---

## 2. The problem this has to solve

A read-only MHALL roll is 26 notes. A real arrangement is tens of thousands, and
a log window is unbounded. **Content at that scale cannot be widgets.** Layout
walks the tree, hit-testing walks the tree, the accessibility tree mirrors the
tree; putting a hundred thousand notes in it breaks all three at once, and it
breaks them quietly — as slowness, not as an error.

So the kit has to admit content it does not model. That is the decision this
proposal exists for. Everything else is consequence.

---

## 3. `Kind::Pane` — the kit owns the frame, the application owns the interior

A new variant of the closed `Kind` enum:

```rust
Pane {
    /// The size of the content, in content units. What the bars are relative to.
    extent: Size,
    /// Where the viewport sits within it, in content units.
    offset: Point,
    /// Content units per point, per axis. 1.0 is one unit to one point.
    scale: Scale,
    /// Which bars the pane reserves space for, whether or not they are active.
    bar: BarPolicy,
}
```

The kit owns: the pane's rect, its bevel, the reserved gutters, both scroll
bars, the clip rectangle, the offset and scale arithmetic, hit-testing of the
bars, and the pane's node in the accessibility tree. **All of that is still the
kit's**, and none of it is duplicated per consumer.

The application owns exactly one thing: what appears inside the clip. The kit
emits a paint-list entry that names the pane, and the application fills it —
already the shape of the backend seam from ui-01, since the kit's output is a
paint list rather than immediate drawing calls.

**Why not a `Custom(Box<dyn Widget>)` escape hatch.** Because that is a hole in
the wall rather than a door: it would let any widget be anything, and the closed
`Kind` enum is what makes the a11y tree, the skin and the paint list total
functions over a known set. A pane is one named kind with one named hole, and
the hole is *inside a clip rectangle the kit controls*.

**The interior is not in the accessibility tree** as geometry, and pretending
otherwise would be worse than admitting it. The pane exposes itself as a
scrollable region with position and extent (R-1510), and the application
supplies a **semantic** description of what is inside — for the roll, notes as
data, not rectangles as pixels. That is a real obligation on ui-06 and it is
written down here so it is not discovered later.

---

## 4. Coordinates

Three spaces, named, because confusing them is the classic source of
off-by-a-scroll-offset bugs:

- **content** — what the application thinks in. Ticks and hertz for the roll,
  lines for the log.
- **pane** — points inside the viewport, origin at its top-left, after offset
  and scale.
- **window** — what the mechanism layer uses.

The kit converts, and exposes the conversion both ways (`Pane::to_content`,
`Pane::to_pane`), because hit-testing inside the interior is the application's
job and it must not re-derive the arithmetic.

**Offsets are in content units, not points.** A zoom must not move the view, and
it does not if the offset is expressed in the space that zoom does not change.

---

## 5. Scroll bars

### 5.1 Always there

Space is reserved for a scrollable pane's bars whether or not there is anything
to scroll. When there is not, the bar is drawn **inactive** — outlined, empty,
no thumb — which is exactly the book's behaviour (§3a, p. 160), so this is
faithfulness rather than departure.

It also removes a defect class rather than an annoyance. A bar that appears on
demand narrows the content; narrower content can be taller; taller content can
demand the other bar; which narrows it again. At one particular content size
that loop does not settle. Reserving the gutter makes it unreachable, and makes
layout a pure function of window size and content extent — which the kit's
`take_dirty` incremental repaint quietly depends on.

`BarPolicy` says which axes reserve: `Both`, `Vertical`, `Horizontal`, `None`.
The log window is vertical-only; the roll is both.

### 5.2 The thumb is proportional — departure

`thumb_length = track_length × (viewport / extent)`, floored at `MIN_THUMB`.

The 1992 box carries position alone (§3a, p. 159). Ours carries position **and**
proportion, because "how much of this am I seeing" is information the user
needs and cannot otherwise get: two hundred log lines and a hundred thousand are
indistinguishable under a fixed thumb.

Two consequences that must be got right, and are the usual bugs:

- **`MIN_THUMB` is in interface-scale units** (R-938), not raw pixels, and is
  large enough to grab on a 4K panel at a distance.
- **Travel is `track − thumb`, not `track − MIN_THUMB`.** Once the floor is
  active the two differ, and using the wrong one makes the end of a long
  document unreachable — the bug is invisible in short content and permanent in
  long.

### 5.3 Live drag — departure

The book redraws on release (§3a, p. 159); that was 1992's hardware speaking. We
track live.

**With an honest fallback.** If a pane's interior cannot be repainted inside the
frame budget, the pane falls back to dragging the thumb alone and committing the
offset on release — the book's behaviour, arrived at for the book's reason. The
policy is per-pane and measured, not guessed: the pane records its own paint
cost, and the fallback is a *recorded observation*, never a silent degradation.
Expected never to fire; kept because "it will always be fast enough" is a
prediction, and predictions belong behind a measurement.

### 5.4 No arrows — departure

The book's bar has an arrow box at each end, one content unit per click,
continuous while held (§3a, p. 163). We omit them: they are unused furniture on
hardware with a wheel, and the space is better given to the track.

**What survives, deliberately:** clicking the gray area pages, and a page is
"the height or width of the window, **minus at least one unit of overlap**"
(§3a, p. 164). The overlap rule is the good part — it is what keeps a reader's
context across a page — and it does not depend on arrows existing. Press-repeat
in the gray area also survives, including its stop condition: repeat until
release, *or until the thumb reaches the pointer*.

### 5.5 Dragging out of the bar — and why it is not a cancel

The book snaps the thumb back when the pointer strays more than about a
thumb-width out of the bar, resumes if it comes back while still held, and does
nothing on a release outside (§3a, p. 165), calling it "standard behavior for
controls in general."

**The kit already does exactly this**, and an earlier draft of this proposal got
it wrong by calling the snap-back a cancel. It is not. `Intent::Cancelled` is
emitted **at release** (`ui_kit/src/kit/event.rs`), never on leaving; while a
control is held, its highlight tracks out and back in continuously. The book,
the kit, and every current application agree.

So the model is simpler than an abort:

- A thumb drag emits offset updates continuously.
- **The snap-back is just another offset update**, back to where the drag began.
- Coming back into range resumes from the pointer.
- Releasing outside emits nothing further — the offset is *already* the original
  value, because the snap-back put it there live.

There is nothing to revert, nothing uncommitted, and no cancel event that
*causes* anything. `Intent::Cancelled` may still be reported at release-outside
for consistency, but it must not be load-bearing.

**The tolerance is a named constant** in interface-scale units, starting at the
book's "a little more than the width of the scroll box". Current applications use
a visibly larger one, so this is expected to want widening — after being used,
not before.

### 5.6 Escape does not apply

The kit's rule is that Escape and Command-period are always Cancel (inventory
§3, ch. 7). That rule is about operations that **commit** something, and it
would be strange applied to a scroll — there is nothing uncommitted to abandon.
Stated generally, so the next case is answered before it is asked:

> **Escape cancels an uncommitted change to the document. Navigation changes no
> document state, so Escape does not apply to it.**

That covers scrolling and zoom together, and leaves Escape intact everywhere it
means something. It is R-905's uncommitted-until-complete posture seen from the
other side.

---

## 6. Zoom

The book has no content zoom — its zoom box resizes a *window* (ch. 5, p. 167),
a different thing entirely, and it explicitly forbids reusing a scroll bar as a
value control (§3a, ch. 7 p. 214). So everything here is ours, recorded as
invention per §8 of the inventory.

### 6.1 Zoom is a control you can see, not only a gesture

The shape to imitate is Cubase's and Resolve's: a **continuous zoom control per
axis**, sitting at the end of that axis's scroll bar, that you can grab and
creep. A modifier-wheel gesture alone is not enough — it is invisible, it is
undiscoverable, and it cannot be operated slowly.

Two things make that placement right rather than merely familiar:

- **The book permits exactly one addition there** (p. 161): "It's OK to add one
  control, like a split bar... to the top of the vertical scroll bar. But if you
  add more than one control to this area, it's hard for people to distinguish
  controls." One zoom control per axis spends that budget precisely, and the
  warning tells us nothing else may join it.
- **`Kind::Slider` already exists**, and ch. 7 p. 214 says a slider is the
  correct control for a setting where a scroll bar is not. The form is already
  in the kit.

There is an obvious way to get this wrong — a zoom slider directly against a
scroll bar reading as a *second* scroll bar, which ch. 7 p. 214 warns
"confuses the meaning of the element". §6.4's control removes the risk rather
than asking the skin to paint around it.

### 6.2 What zoom does

**Axes are independent.** Time and pitch have nothing to do with each other; a
roll zoomed to one bar of time may want five octaves or one.

**Vertical zoom is linear in log-frequency.** The roll's axis is continuous
log-frequency (R-943), so scaling is a multiply in log space — octaves stay
evenly spaced at every zoom, and no tuning needs a special case. The requirement
pays for itself here.

**The slider is logarithmic too**: position maps to `log(scale)`, so equal travel
is equal *ratio*. That is what makes creeping the control feel even instead of
exploding at one end, and it is the same principle as the pitch axis.

**Steps are fine.** Near `2^(1/8)` per wheel detent or key press — small enough
to creep, large enough that crossing the range is not tedious.

**What stays fixed while zooming** — the part that decides whether it feels
right:

- **Wheel or pinch zoom anchors the pointer.** The content under the pointer
  does not move; that is what makes it a lens rather than a jump.
- **Keyboard, slider or command zoom anchors the playhead** when it is in view,
  and the viewport centre when it is not. You are zooming to look at where the
  music is.

Both are one function with a different anchor.

**Limits are content-relative**, not absolute: never less than one tick per
pixel, never more than the whole piece plus a margin. A zoom that reaches a
blank void is a zoom the user has to recover from.

### 6.3 Reserved, not built

**Zoom to fit** and **zoom to selection**. Both DAWs have them, both are reached
for constantly once there is content, and both are trivial given §6.2 — they are
an extent and an anchor. They are named here so the API does not foreclose them,
and left to their consumer.

### 6.4 The control itself: Vision's cluster

Two buttons at the far end of the bar — a magnifying glass with a minus, a
magnifying glass with a plus — with the slider **between them when there is room
for it**:

```
[-][+]                 cramped
[-]----[]---[+]        with room
```

Copied from Vision, which is contemporary with the book, and it is better than
the two-tier swap an earlier draft proposed for three reasons:

- **Degradation is subtractive, not substitutive.** Nothing is replaced; the
  middle is simply absent. The zoom-in button is in the same place at every
  window size, which is the entire value of a fixed target.
- **The magnifiers settle §6.1's confusion risk by construction.** A trough
  flanked by magnifying glasses cannot be read as a second scroll bar, so the
  skin no longer has to work around a design that invites the mistake. The book
  asks for exactly this (p. 216): "If possible, give some indication what the
  user can expect by using the up arrow and the down arrow." A bare arrow does
  not say what it increments; a magnifier does.
- **Behaviour comes from the book unchanged** (p. 216, little arrows): a click
  steps one unit — here `2^(1/8)` — and a press repeats until release.

**Placement.** The cluster sits at the **far end** of its bar: the right end of a
horizontal bar, the bottom of a vertical one. The vertical cluster is the
horizontal one **rotated 90° clockwise**, so minus is above plus. One object in
two orientations rather than two arrangements to learn.

**This departs from ch. 7, p. 214**, which observes that "most people assume that
moving an indicator up a vertical slider means increasing the value" — with minus
above, dragging up zooms *out*. Recorded in inventory §7, and taken deliberately:
the assumption p. 214 is protecting against is *unclear direction*, and the
glyphs make the direction explicit, which is the remedy the same chapter
recommends. Rotational consistency then buys more than the convention does.

**Both clusters converge near the bottom-right corner** when a pane has both
bars. That is Vision's layout and it works: every zoom control in one place,
rather than one at each end of the window.

### 6.5 It spends the whole scroll-bar budget

p. 161 permits **one** addition to the scroll-bar region, warning that with more
"it's hard for people to distinguish controls, and to click exactly the desired
control."

The cluster survives that, and the reason belongs on the record rather than in
someone's head: the book's concern is telling *unrelated* controls apart, and
two magnifiers with a trough between them are **one control with three parts** —
a single function offering coarse and fine access. Its split-bar example is a
genuinely different function competing for the same strip.

But it does spend the budget entirely, so the consequence is a rule:
**nothing else ever goes in the scroll-bar region.** No split bar, no page
indicator, no status strip (p. 161 offers all three). Decided here, once.

---

## 7. The wheel

The inventory already settled the principle — 1992 has no wheel, and "our
wheel-coarse / tilt-fine comes from the Notorolla exhibits" (§6). Applied here:

- **wheel** scrolls the pane's primary axis;
- **tilt** (horizontal wheel) scrolls the cross axis;
- **wheel with the zoom modifier** zooms, anchored at the pointer per §6.

One line per behaviour, all three consistent with the exhibits.

---

## 8. What a pane deliberately does not do

**It does not scroll itself.** The book's automatic-scrolling rules (§3a,
pp. 166–167) are about following a *selection*, and a pane has no idea what a
selection is. Auto-scroll is the application's, and the roll's
playhead-following belongs to ui-06 — where the book's restraint is the
governing rule: "avoid unnecessary scrolling… if part of a selection is showing
in the window, don't scroll at all", and never scroll both axes when one will
do.

**It does not know what is inside it.** No content model, no note, no log line.
That is what keeps one pane serving both consumers.

---

## 9. Tests

- **Geometry**: thumb length is proportional; the floor engages for long content
  and travel still reaches both ends (the §5.2 bug, as a test).
- **Reserved space**: a pane with nothing to scroll has the same interior rect
  as the same pane with content, and its bars report inactive.
- **No thrash**: a pane at the exact size where an on-demand bar would oscillate
  settles in one layout pass.
- **Round-trip**: `to_content(to_pane(p)) == p` across offsets and scales.
- **Zoom anchoring**: the content point under the anchor is invariant across a
  zoom, at both anchors, in log space for the vertical axis.
- **Drag-out**: the thumb snaps back past the tolerance, **resumes** when the
  pointer returns while still held, and releasing outside leaves the offset at
  its original value — asserted as offset updates, not as a cancel.
- **Escape**: a pane in mid-drag ignores it, and the drag continues.
- **Zoom slider**: equal travel is equal ratio, across the whole range.
- **Cluster degradation**: below the slider threshold the two buttons remain, in
  the same positions, and still step and repeat.
- **Paging**: a page moves by a windowful minus the overlap unit, and
  press-repeat stops when the thumb reaches the pointer.

---

## 10. Decisions to approve

1. `Kind::Pane` in the closed enum, with the kit owning frame, bars, clip,
   offset and scale.
2. **The application paints the interior**, inside a kit-owned clip rect,
   through the paint list — the first such hole, and named rather than general.
3. The pane exposes itself to the a11y tree as a scrollable region; **the
   interior's semantics are an obligation on its consumer** (ui-05, ui-06).
4. Three coordinate spaces, converted by the kit both ways; offsets in content
   units.
5. Bars are **never hidden and always reserved** (faithful to §3a); `BarPolicy`
   per axis.
6. **Proportional thumb**, `MIN_THUMB` in interface-scale units, travel computed
   from actual thumb length. *Departure, recorded.*
7. **Live drag**, with a measured per-pane fallback to commit-on-release that
   records an observation when it fires. *Departure, recorded.*
8. **No scroll arrows**; gray-area paging survives **with the one-unit overlap
   rule** and press-repeat. *Departure, recorded.*
9. Drag-out keeps the book's snap-back and resume, expressed as **ordinary
   offset updates** — not a cancel. `Intent::Cancelled` may be reported at
   release-outside but is never load-bearing. Tolerance is a named constant,
   expected to widen after use.
10. **Escape does not apply to navigation** — it cancels uncommitted changes to
    the document, and scrolling and zooming are not that.
11. Zoom is a **visible continuous control per axis**, at the end of that axis's
    scroll bar — the book's one permitted addition — built on `Kind::Slider`,
    logarithmic in position. The skin must keep it plainly distinct from a
    scroll bar.
12. Zoom: independent axes, log-space vertically, `2^(1/8)` steps,
    pointer-anchored for the wheel and playhead-anchored for keyboard and
    slider, content-relative limits.
13. The zoom control is **Vision's cluster**: `[-]` and `[+]` magnifier buttons
    at the far end of the bar, slider between them when it fits, buttons flush
    when it does not. Click steps, press repeats (ch. 7 p. 216).
14. The vertical cluster is the horizontal one **rotated 90° clockwise** — minus
    above plus. *Departure from ch. 7 p. 214's up-means-more, recorded.*
15. The cluster spends the whole scroll-bar-region budget: **nothing else ever
    goes there.**
16. **Zoom to fit** and **zoom to selection** are reserved, not built.
17. Wheel coarse / tilt fine / modifier-zoom, per the exhibits.
18. A pane never scrolls itself; auto-scroll belongs to its consumer.
19. **A demo bin** (`app/src/bin/pane.rs`), permanent, alongside golden
    screenshots. The goldens pin geometry; the demo is the only thing that can
    say whether the snap-back tolerance, the zoom anchoring and the drag feel
    right — and eng-07's lesson is that a green suite proves nothing about
    properties nothing asserts on. It carries a ruled grid with its coordinates
    drawn in (so pointer-anchored zoom is verifiable by eye), an absurd extent
    (so `MIN_THUMB` and the travel arithmetic engage), and both bar policies
    side by side. Being a *third*, synthetic consumer is the point: it proves
    the pane is not accidentally shaped for whichever of ui-05 and ui-06 gets
    built first. §5.3's paint-cost measurement lives here too.


---

## 11. Findings (written after the fact)

**1. Press-repeat is not implemented.** Clicking the gray area pages, and
clicking a magnifier steps — but *holding* either does nothing, where the book
repeats until release (§3a, pp. 163–164). The reason is structural rather than
an oversight: repeating needs a tick that can emit an `Intent`, and the kit's
`animate()` returns a bool. Inventing an event channel to get it would have been
a larger change than this checkpoint approved. It should arrive with whichever
consumer first wants it, and it is a real gap: press-repeat is what makes the
buttons usable when the cluster has collapsed to `[-][+]`.

**2. §5.3's measured live-drag fallback is not implemented either.** Drag is
always live. The fallback was specified against a *measurement* the pane does
not yet take, and building the fallback before the measurement would have meant
guessing the threshold — which is the thing §5.3 was written to avoid. The demo
is where that measurement belongs.

**3. The slider must not take the gutter.** A first version gave the zoom slider
whatever room was left after the two buttons, which at ordinary sizes left the
scroll track exactly **zero pixels** long. Caught by the thumb-floor test, which
suddenly had no track to sit in. The gutter belongs to the scroll bar; the
cluster is a guest with a fixed length, and when the two cannot both fit, the
slider is what goes.

**4. The magnifier had to be looked at.** The first glyph was invented and drew
the lens as four straight strokes; it rendered as a *box* with a dash in it,
which promises nothing — and §6.4 leans on the glyph to justify the vertical
cluster reading downward. No test noticed, because no test can. It now uses
**Lucide's `zoom-in`/`zoom-out` geometry**, transcribed rather than approximated
(ISC, nothing bundled, recorded in `asset/asset.json`).

**5. Clamping beats the anchor at the edges, correctly.** Zooming out at a point
near the origin cannot hold the content under the pointer still — doing so would
require a negative offset. The clamp wins. A test asserted the anchor property
without that caveat and failed; the behaviour was right and the assertion was
wrong.


---

## 12. Amendments (2026-07-21, after operating it)

**A. Bar width is the skin's, and larger.** `BAR` moved to
`Skin::metric::scroll_bar` beside `slider_width` and `touch_min`, and went from
15 to **22** logical pixels — with `scroll_thumb_min` at `bar × 1.5`, since a
short thumb in a wide bar reads as a stub. Bars shrank industry-wide because
phone conventions leaked to the desktop, not because anyone measured a desk; the
skin inventory already made the same call for type, whose scale is "larger than
control-skin convention" for this display. A scroll bar is a long thin target
and the thin dimension is the one that costs. 30 is worth trying next.

**B. Bars do *not* adapt to window size.** Considered and rejected. The
saving is real — 22 pixels is 1.5 % of a wide document pane and 10 % of a narrow
utility one — but a target that changes size between windows is exactly what
muscle memory punishes, and §6.4's whole argument for the cluster was that its
buttons *do not move*. Space in small panes is already recovered the right way:
the slider is dropped and the buttons stay. (A bar width that varied with the
*window* could not reintroduce §5.1's thrash loop, which depends on content —
so the objection is feel, not correctness.)

**C. What the pointer is over decides what the wheel does.** Not a new rule: the
kit already says it for counter fields — "the wheel aims where you are looking".
Applied to the pane it gives three consistent cases: over a **zoom cluster**,
zoom *that cluster's axis* with no modifier; over a **scroll bar**, scroll *that
bar's axis*; over the **interior**, scroll, or zoom with the modifier, anchored
at the pointer. The interesting one is the second and third: rolling over the
horizontal cluster zooms *time* even though the wheel is the vertical input. The
control names the axis; the wheel only supplies the amount.

**D. Furniture highlights under the pointer; content never does.** A pane is one
widget containing several controls, so the kit gained sub-widget hover
(`Part::{Thumb, Track, ZoomIn, ZoomOut, ZoomSlider}`) — the only place it needs
it. The interior is deliberately not a `Part`: a control lights up because it is
about to do something, and content is not offering to do anything.

**E. The zoom modifier is unsettled.** `ctrl` for now. Cubase and Resolve
disagree, and choosing between them is a decision nobody has made — recorded as
open rather than as settled by default.

**F. Three numbers, changed after operating them** — which is the order §5.5 and
§6.2 promised, and worth recording as a pattern rather than three tweaks:

- **Drag tolerance doubled**, `bar × 1.5` → `bar × 3.0`. The book's "a little
  more than the width of the scroll box" is what it was, and it proved too tight
  in the hand. Widened after use, not guessed at in advance.
- **The zoom step doubled**, `2^(1/8)` → `2^(1/4)`. An eighth of an octave was
  too fine to get anywhere with. One constant still, shared by the magnifier
  buttons, the wheel over a cluster and the keyboard, so all three agree about
  what a step is.
- **Bar width stays at 22.** 30 was on offer and not wanted.

**G. The thumb lights for its whole track.** Hovering anywhere in a bar
highlights its thumb, not only hovering the thumb itself — because once the
pointer is in a track the wheel scrolls *that* axis, so the thumb is the thing
about to move. Highlighting only the thumb would leave a bar looking inert at
the exact moment it became the wheel's target. Amendment C is what made this
necessary: the rule about what the wheel is over creates an obligation to *show*
what it is over.
