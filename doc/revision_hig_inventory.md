# Interaction inventory — Apple Macintosh Human Interface Guidelines (1992)

Status: read-only inventory, taken 2026-07-20. Source: *Macintosh Human Interface
Guidelines*, Apple Computer / Addison-Wesley, 1992 (410pp). Quotations are from the
document; chapter names are given so every claim here is checkable rather than
recalled.

**Why this document.** It is the last complete specification of interaction
*behaviour* anyone wrote. What replaced it is largely platform marketing and API
documentation; none of it tells you what should happen when the pointer leaves a
pressed button. This one does, with the reason. It is right on nearly every question
it can still be asked — the constructs that did not exist in 1992 are the exception,
not the rule.

**Standing.** Proposed as R-939: where Revision's own requirements are silent on
interaction behaviour, this document is the default authority — for **behaviour and
principles**, not appearance, not menu-bar architecture, not its document model.
Precedence: Revision requirements → skin inventory and coding standard → AHIG 1992 →
invention (which then gets written down).

---

## 1. Principles (ch. 1)

Ten, of which these bind us directly:

- **User control.** "Allow the user, not the computer, to initiate and control
  actions." The balance stated is between capability and destroying data — protect
  by *warning and allowing*, not by preventing.
- **Feedback and dialog.** "When a user initiates an action, provide some indicator
  … that your application has received the user's input and is operating on it."
  Immediacy is the requirement, not eventual correctness.
- **Forgiveness.** "Actions on the computer are generally reversible." And the
  sharpest line in the chapter: "frequent alert boxes are a good indication that
  something is wrong with the program design."
- **Perceived stability.** "Even when particular actions are unavailable, they are
  not eliminated from a display but are merely dimmed." — the exhibits' inert rule,
  arrived at independently, stated here first.
- **Modelessness.** Modes are to be avoided; the sanctioned exceptions are
  long-term modes (an application is one), **short-term spring-loaded modes** ("the
  user must constantly do something to maintain the mode"), and alert modes, "kept
  to a minimum". Our shuttle is a spring-loaded mode in exactly this sense.
- **Direct manipulation**, **see-and-point**, **consistency**, **WYSIWYG**,
  **aesthetic integrity**.

## 2. Mouse actions (ch. 10)

The chapter that binds us most, and the one we have been re-deriving.

> "In general, just moving the mouse changes nothing except the location, and
> possibly the shape, of the pointer. **Pressing the mouse button indicates the
> intention to do something, and releasing the mouse button completes the action.**"

> "If the function of the click is to cause an action (such as clicking a button),
> **the selection is made when the button is pressed, and the action takes place when
> the button is released.**"

**Clicking** is press-and-release with the mouse stationary: "If the mouse moves
between button down and button up, dragging — not clicking — is what happens."

**Double-clicking** requires closeness in time *and* place ("usually within one or
two pixels"). Two hard rules: **"Double-clicking must never be the only way to
perform a given action"**, and if only single and double clicks are defined, "a third
click should have no effect."

**Pressing** (holding, stationary) "should have no more effect than clicking has —
except in well-defined areas, such as scroll arrows, where it has the same effect as
repeated clicking."

**Dragging** with boundaries — the rule our cancel behaviour implements:

> "If the user releases the mouse button while the pointer is outside the boundaries,
> the object doesn't move. **However, if the user moves the pointer back within the
> boundaries before releasing the mouse button, the object appears in the new
> location.**"

## 3. Control feedback (ch. 7, "Button Behavior")

> "If the user presses the mouse button while the pointer is over a button, the
> button stays inverted until the user releases the mouse button **or moves the
> pointer away from the button. The button tracks the mouse movement** as long as the
> user keeps the mouse button depressed. If the user moves the pointer back over the
> button, it is highlighted. **If the user releases the mouse button while the pointer
> is not over the button, nothing happens.**"

Also: name a button "with a verb that describes the action that it performs", one
word where possible and never more than three; Escape and Command-period are always
Cancel; do not give a default button when the likely action is dangerous.

## 3a. Scroll bars (ch. 5, "Windows", pp. 158–167) — read 2026-07-21 for ui-kit scroll

The anatomy: "A scroll bar is a light gray rectangle that has an arrow in a box at
each end… Inside the scroll bar is a rectangle called the **scroll box**… The rest of
the scroll bar is called the **gray area**." A vertical bar sits on the right, a
horizontal one along the bottom.

**They are never hidden** (p. 160):

> "If the document is no larger than the window, the scroll bars are **inactive**.
> This means that the rectangles are outlined, but there is no gray area, no scroll
> box, and the arrows are hollow (their outlines appear)."

So a scroll bar with nothing to scroll becomes *empty*, not *absent* — the space is
reserved by construction. Auto-hiding bars have no basis here.

**The scroll box carries position only** (p. 159): "The scroll box represents the
relative **location**, in the whole document, of the portion that can be seen in the
window." Nothing about size, and a page number inside the box is offered as the way
to say more (p. 162) — the 1992 box is fixed.

**Paging** (p. 164): "Clicking in the gray area of the scroll bar advances the
document by a windowful… A windowful equals the height or width of the window,
**minus at least one unit of overlap** to maintain the user's context." Pressing
repeats "until the user releases the mouse button, or until the location of the
scroll box catches up to the location of the pointer."

**Dragging is not live** (p. 159): "If the user drags the scroll box **and releases
the mouse button**, the document 'moves' along with it."

**Dragging out** (p. 165), which is §3's control rule applied to the thumb:

> "If the user starts dragging the scroll box, then moves the pointer out of the
> scroll bar, the scroll box stops following the pointer and **snaps back to its
> original position**. The user can move the pointer out of the scroll bar region by a
> little more than the width of the scroll box before the scroll box snaps back… This
> type of tracking is standard behavior for controls in general."

**Automatic scrolling** (pp. 166–167) — four cases where the application must scroll:
an operation that moves the selection or insertion point; keyboard entry at the edge;
the pointer leaving the window during an extended selection ("the rate of scrolling
can be the same as if the user were pressing on the corresponding scroll arrow…
it makes sense to vary the scrolling speed so that it is faster as the user moves the
pointer farther away"); and an operation on a selection scrolled out of view. The
governing rule is restraint: "avoid unnecessary scrolling… move a document only as
much as necessary… if part of a selection is showing, don't scroll at all", never
scroll both axes when one will do, and prefer the selection "somewhere near the middle
of a window than right up against a corner."

**A scroll bar is not a value control** (ch. 7, p. 214): "Make sure that you don't use
a scroll bar when you really mean to use a slider. Use scroll bars **only** for
representing the relative position of the visible portion of a document and in
scrolling lists… Using a scroll box to change a setting confuses the meaning of the
element." Binding on us: the roll's zoom is not a scroll bar.

## 4. Fields (ch. 10, "Editing Fields")

For an application that is not primarily a text application:

- select the whole field and type a new value; select a substring and replace it;
  double-click to select a word; Undo/Cut/Copy/Paste/Clear must work.
- **Validation timing is specified**: "the application could wait until the user is
  through typing before checking the validity of a field's contents. In this case,
  the appropriate time to check the field is **when the user clicks anywhere other
  than in the field or presses the Return, Enter, or Tab key.**"

That is precisely our counter's commit rule, arrived at independently.

## 5. Where we already agree — and had to find out the hard way

Every one of these we implemented, broke, or fixed this session *before* reading the
document, mostly because the user was looking:

| Our rule | Its source here |
|---|---|
| Release off the pressed widget cancels | ch. 10, dragging boundaries; ch. 7 |
| A pressed control tracks the pointer | ch. 7, verbatim |
| Inert controls are dimmed, not removed | ch. 1, Perceived stability |
| Feedback must be immediate | ch. 1, Feedback and dialog |
| Commit a field on Enter, Tab, or a click away | ch. 10, Editing fields |
| The shuttle springs home | ch. 1, spring-loaded modes |
| Intrinsic state outranks interaction state | implied by ch. 7's tracking rule |

**Two defects this reading found immediately**, neither caught by any test: our
buttons stayed lit when the pointer dragged off them (ch. 7 says they must not), and
a control kept showing attention state after another control was activated.

## 6. What it cannot answer

Hardware and constructs that postdate it. These are ours to decide, and the decision
gets written down:

- **The second mouse button** (contextual menus arrive ~1997) and the **scroll
  wheel** (~1996). Our wheel-coarse / tilt-fine comes from the Notorolla exhibits.
- **Touch and gesture**: no hover state, minimum target sizes, pinch and swipe.
- **High-DPI and interface scale** (R-938): 1992 assumes 72 dpi at 1:1.
- **Assistive technology as a semantic tree** (R-1510). Ch. 2 covers universal
  access thoughtfully but predates screen-reader APIs.
- **Multi-window without a menu bar.** Its menu chapter assumes one screen-wide bar
  owned by the frontmost application; we are cross-platform and multi-window, and
  R-915 wants every function reachable through visible pointer-operable UI anyway.

## 7. Where we knowingly depart

Not gaps — disagreements, and we think we are right:

- **The document model.** It specifies Save / Save As / Revert (ch. 4). **R-201
  abolishes unsaved state entirely.** Its Save Changes alert box has no counterpart
  here and should not acquire one.
- **Undo depth.** It specifies single-level Undo/Redo (ch. 4). Ours is journaled and
  unlimited (R-205), so what a "gesture" is, and what Revert means when nothing is
  unsaved, are ours to define.
- **Modality tolerance.** Its dialog chapters are more permissive than R-905 and
  R-906 allow: our transport never stops for an edit, and an in-progress gesture is
  uncommitted until it completes.
- **Scroll bars: three departures, decided 2026-07-21** (ui-kit scroll checkpoint;
  §3a has what the book says). **Proportional scroll box** — the 1992 box carries
  position alone, ours carries position *and* proportion, because a log of two hundred
  lines and one of a hundred thousand must not look identical. **Live drag** — the
  book redraws on release (p. 159), a concession to 1992 hardware we no longer owe.
  **No scroll arrows** — the arrows are unused furniture on modern hardware; paging by
  clicking the gray area stays, with the book's one-unit overlap rule intact. We keep
  the part of §3a that matters most: bars are never hidden, only inactive.
- **Vertical zoom cluster reads downward** (ch. 7, p. 214: "most people assume that
  moving an indicator up a vertical slider means increasing the value"). Ours is
  Vision's horizontal cluster rotated 90° clockwise — minus above plus — so dragging
  up zooms out. Taken because p. 214 is guarding against *unclear* direction, and the
  magnifier glyphs make it explicit, which is the remedy the same page recommends
  ("give some indication what the user can expect", p. 216). One object in two
  orientations beats two arrangements to learn.
- **Alert boxes as the safety mechanism.** Ch. 1 leans on them for warnings; our
  answer is journaled reversibility, which the same chapter would prefer — "frequent
  alert boxes are a good indication that something is wrong with the program design."

## 8. How to use this

When a question of interaction behaviour arises and our requirements are silent:
cite the chapter, not the memory. If the document has no answer because the
construct did not exist, decide, record the decision here under §6, and say why.
