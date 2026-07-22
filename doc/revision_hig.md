# Human-interface decisions — Revision

**What this is.** The register of interaction *rulings* Revision has actually made:
the concrete "here is what happens when…" decisions, each with its grounding. It is
the counterpart to `revision_hig_inventory.md` — that document is the **source** (what
the 1992 *Apple Human Interface Guidelines* say, quoted and chapter-cited); this one is
the **application** (what we decided for Revision, and why). When you settle an
interaction question during HI work, record the answer here.

**Standing and precedence.** Where Revision's own requirements are silent on interaction
*behaviour*, the AHIG 1992 is the default authority (R-939), cited by chapter rather
than recalled (R-940 — `revision_hig_inventory.md` has the quotations). Precedence:

> Revision requirements → skin inventory & coding standard → AHIG 1992 → invention
> (which then gets written down **here**).

**How to use this document.** Consult it before deciding any pointer/keyboard/feedback
behaviour, so the workspace stays consistent and you don't re-derive a settled
question. When a new behaviour is decided — in discussion, or forced by
implementation — add a row: the ruling, its grounding (an AHIG chapter/page, an
R-number, or an honest "invention"), and where it lives in code. UI is eyeball-verified
(getstarted stage 4), so this register is the only durable memory of these choices.

---

## Principles that bind (AHIG ch. 1)

The ten principles, of which these recur in our decisions. Full text in the inventory.

- **Feedback and immediacy.** "When a user initiates an action, provide some indicator
  that your application has received the input and is operating on it." Immediacy is
  the requirement — an indicator now, not eventual correctness.
- **User control.** The user, not the computer, initiates and controls actions; protect
  data by *warning and allowing*, not by preventing.
- **Perceived stability.** Unavailable actions are **dimmed, not removed** — the display
  does not rearrange itself under the user.
- **Modelessness.** Avoid modes; the sanctioned exceptions are the long-term mode (an
  application), the **spring-loaded** mode (held only while the user keeps acting), and
  the alert mode.
- **Direct manipulation / WYSIWYG / consistency / aesthetic integrity.**

## Interaction rulings

- **Press indicates intent; release completes the action.** A click is press-and-release
  with the pointer stationary; if the pointer moves between down and up, it is a *drag*,
  not a click. *AHIG ch. 10.* (See also the carried-over scar: **activation ≠ focus,
  act on press** — a control's press must not be swallowed by an ancestor.)

- **The pointer shape announces what a spot does.** Moving the mouse "changes nothing
  except the location, and possibly **the shape**, of the pointer" (ch. 10). A draggable
  divider therefore shows the resize cursor on hover: in the Player, both the pane
  splitter and every column edge switch to the horizontal-resize shape.

- **The pointer shape is stable through a gesture.** A cursor requested for a drag must
  survive the frames *within* that drag (repaints, playback ticks). The mechanism
  reconciles the cursor only after genuine pointer events, never on a plain repaint —
  otherwise a resize cursor flickers back to the arrow between mouse moves.
  *Implementation: `rev-ui-mech` `window_event`.*

- **Dragging a control out snaps it back; returning resumes it.** "If the user starts
  dragging the scroll box, then moves the pointer out of the scroll bar, the scroll box
  stops following the pointer and **snaps back to its original position**… by a little
  more than the width of the scroll box before it snaps back… **standard behavior for
  controls in general.**" *AHIG ch. 5, "Windows", p. 165.* Applied to the Player's
  **column-width drag**. The gesture belongs to the **column heading** — the heading row
  plus a little vertical slop on either side — not the tracks below it: that band is
  where the edge is grabbable *and* where the drag tracks. Stray off it (down into the
  tracks, or above the header) and the column snaps back to its grab width, resuming the
  instant the pointer re-enters the band. It is a **suspend, not a cancel** — release
  in-band commits the new width, release out-of-band keeps the original.

- **A palette never takes front (R-907).** Operating a palette window (the Control Bar)
  does not activate it or steal focus from the document being worked in, so a
  document-scoped action (space plays the *focused* Player) stays stable while the user
  reaches for the transport. *AHIG palette/window-layer behaviour; R-907.*

- **Independent toggles are not a mode.** Record / Mute / Solo per track are independent
  toggles; arming one track does not disarm the others (record is **not** mutually
  exclusive). Making it a mutex would be an unasked-for mode (ch. 1, modelessness).

- **Double-clicking a column heading acts on the whole column.** Double-clicking an
  R/M/S heading clears that state on every track ("all mutes off", "all solos off", "all
  record off"). Double-click is the gesture for an action beyond selection (AHIG ch. 10);
  the specific "clear the column" mapping is a Revision convenience. *Invention;
  `rev-player`. (Double-click is detected app-side for now — the mechanism does not yet
  model it; that belongs in the mechanism contract when a second widget needs it.)*

- **Text that overflows its container is truncated with an ellipsis** ("Tex…"), never
  allowed to bleed into the neighbouring column. Applied to every table cell and column
  heading in the Player. *Revision decision, consistent with direct-manipulation and the
  Finder's column behaviour; no specific AHIG page — recorded here as invention.*

## Departures from AHIG (deliberate, recorded)

- **Live dragging where AHIG defers to release.** AHIG's scroll box is **not live** —
  it updates the document only on release (p. 159). Revision's **splitter and column
  resize track live** (the divider follows the pointer continuously), because on modern
  hardware the continuous redraw is free and the immediate feedback is better. The
  snap-back ruling above is how we keep the *tracking* rule (p. 165) even though the
  *liveness* differs.

- **Scroll bars — three departures (decided 2026-07-21; inventory §3a has the source).**
  Proportional scroll box (the 1992 box carried position only); **no scroll arrows**
  (unused furniture on modern hardware; paging covers it); and the roll's zoom is a
  **slider, not a scroll bar** ("a scroll bar is not a value control", ch. 7 p. 214).

## Appearance rulings with interaction weight

- **Rows are divided by rules, not candy-striping.** Every track/lane sits on one
  neutral background, told apart by thin rules between rows and columns — not by
  alternating fills. Vertical and horizontal rules are converged to one weight.
  *Revision decision (aesthetic integrity); the Notorolla skin is the visual authority
  it serves.*

- **Legibility floor: 14 px.** The smallest UI text must clear ~14 logical px on the
  target display (a 50-inch 4K panel); 18 px is comfortable-small. The per-window
  interface scale (R-938) is the knob that lifts a whole window's content to clear the
  floor without re-specifying every size.

- **Pace redraws to the display; feedback stays immediate.** Continuous animation (a
  playing playhead) is paced to the refresh rate rather than free-run — free-running
  full repaints burns the frame budget on a 30 Hz panel and starves input. This serves
  the feedback principle (responsive input) rather than fighting it. *Implementation:
  `rev-player` `tick`; the shaped-run cache in `rev-ui-mech` is what makes a paced
  full-window repaint cheap enough.*
