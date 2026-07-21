# Skin inventory — the Notorolla control-skin exhibits (read-only pass)

Status: read-only inventory, taken 2026-07-20 for ui-03. Source is
`../Notorolla/future/ui_skin/`, which is **strictly read-only**; this document is
the Revision-side transcription so the port has a checklist and the lab is never
touched.

**Scope: the chosen designs only.** The Round-1→4 studies (`skin-a-stage`,
`skin-b-roland`, `skin-c-rackmix`, `skin-d-machined`) are the historical derivation
and are *not* inventoried — rejected candidates describe roads not taken. What is
here is `skin-e-composite` (the Round-5 endpoint) plus the seven locked instrument
exhibits and the FM-algorithm control.

Notorolla's own README marks the instrument exhibits as **retained permanently** as
the living visual spec. Where its CSS says `LOCKED`, that is recorded below as a
decision, not a preference.

Related: `revision_poc.md` §"Two sources, and what each governs" — Vision supplies
the Control Bar's *function*, these exhibits supply the *look*.

---

## 1. Standing laws

Stated verbatim in `skin-e-composite`'s header, and observed by every exhibit:

- **Lights glow, text never.** Glow is a lamp property. No glowing type, anywhere.
- **Mono readouts in fixed windows.** Values are monospaced, tabular, and sit in a
  fixed-width field so a changing value cannot reflow its neighbours.
- **One weight.** The exhibits set `font-weight: 400` globally and never vary it;
  emphasis comes from colour and size, not from bold.
- **Density comes from removing dead space**, not from shrinking things.

> **Port note.** The single-weight law is worth deliberate re-examination rather
> than blind adoption: it was set for a browser rendering Tahoma at 14px, and our
> floor is 14px on a 4K panel at desk distance with a face that has a real bold. It
> should be a decision, not an inheritance.

## 2. Tokens

One scalar drives the panel; everything else is derived from it by `calc()`.

| Token | Value | Meaning |
|---|---|---|
| `--pl` | 20% (exhibits) / 22% (composite) | **panel lightness — the master dial** |
| `--panel` | `hsl(220 7% pl)` | panel face |
| `--panel-hi` | `pl + 6%` | raised edge / top highlight |
| `--panel-lo` | `pl − 6%` | lower edge |
| `--slot` | `hsl(222 10% pl−10%)` | recessed groove |
| `--tick` | `pl + 26%` | minor tick |
| `--tick-maj` | `pl + 38%` | major tick |
| `--boxln` | `pl + 16%` | group frame |
| `--ink` | `hsl(220 12% 88%)` | primary text |
| `--ink-dim` | `hsl(220 8% 62%)` | labels, secondary text |
| `--ro` | `hsl(41 100% 62%)` | **readout amber** — every value, everywhere |
| `--acc` | `hsl(358 78% 56%)` | accent red — lit lamps |
| `--arc` | `hsl(200 70% 55%)` | value arc blue |
| `--cap-a/b/c` | `42% / 20% / 30%` L | slider cap gradient stops |
| `--glow` | 0.7 | lamp glow strength |
| `--tgap` | .10em (exhibits) | gap between adjacent tick ladders |

Hue is **220** almost everywhere: the whole chrome is one cool grey family, and
colour is reserved for meaning.

## 3. Role hues (LOCKED)

The band colour of a group is its **role**, never its instrument. A role keeps its
colour on every panel; **an absent role leaves a gap in the spectrum**, and the gap
is the feature — the panel reads "this one has no LFO" at a glance.

| Role | Hue / sat |
|---|---|
| LFO | 15° 45% — muted rust |
| Oscillator | 30° 48% — muted orange |
| Filter | 120° 30% — muted green |
| Envelope | 190° 40% — muted cyan |
| Effects | 215° 35% — muted blue |

Reserved for future splits: yellow 48/45 (osc №2, pitch), aqua 165/35 (filter №2),
mauve 285/22 (output). Bands are black text on the colour, at 54% lightness.

**Canonical group order, panel-wide:** LFO → Oscillator → Filter → Envelope →
Effects. Fixed, so instruments read against one another.

**Roles are a parameter taxonomy and do not transfer to the transport** — the
Control Bar has no LFO. Its semantics are states (armed, recording, looping), using
the role-independent tokens: `--ro` for values, `--acc` for record, `--arc` for
active.

## 4. Layout rules (LOCKED)

- **Groups are fieldsets**: a 1px frame with the band as a legend tab centred *on*
  the border line, notched out of it by panel-coloured side rectangles.
- **Subgroup chrome is label-only.** Hairline and boxed variants were tried and
  retired. **Every** subgroup carries a label even when it is the only one, so
  slider tops stay uniform across the whole panel.
- **Banks are top-aligned**, so sliders start a uniform distance below every band
  even beside a taller rotary stack.
- **Speculative** elements are dashed + `†`, at group or subgroup level.
- **Inert** controls (invalid at the current settings) dim to **0.35 opacity and
  stop accepting input**, live, driven by another control's value.

## 5. Widget archetypes

Chosen by the **shape of the parameter**, not by taste:

| Parameter shape | Widget |
|---|---|
| continuous | vertical slider |
| bipolar / detented | knob |
| 3–5-way enum | rotary switch with position labels |
| 6+-way enum | rotary switch, value in a readout window, no radial labels |
| stepped ratios | stepped slider (snaps to exact values) |
| registration (organ) | drawbar tabs |
| graph / diagram | backlit LCD + rotary encoder |

**Vertical slider** — 1.8em wide, 7.4em tall. Slot .3em wide, recessed
(`inset 0 1px 3px rgba(0,0,0,.7)` plus a `panel-hi` bottom hairline). Tick ladders
both sides, 1px minor / 2px major. **Cap is wide** (1.62em — near column width,
Jupiter-8 style), .92em tall, a three-stop vertical gradient (`cap-a → cap-b 46% →
cap-c`) with a 1.5px near-white centre line, a drop shadow and a 1px inner top
highlight. Bipolar sliders mark the detent by colouring **that tick amber** — no
extra bar. Below: name in `ink`, value in `ro`, unit-less, 5.5ch fixed window.

**Knob** — 3.1em, radial gradient lit from 32%/28%, dark rim, drop shadow, inner
top highlight. Pointer is a 2.5px near-white bar from 8% to 40% of the radius.
Sweep is **270°** (−135° to +135°). A small variant (`.sm`) inverts to a bright
chrome cap with dark pointer. Optional value **arc** in `--arc`, a ring at 1.16×
diameter masked to a 58–72% annulus.

**Rotary switch** — 2.3em, same body as the knob. Position ticks around the dial,
the selected one amber; **upright position labels** (text or glyph) at each angle;
for 6+ positions, labels are dropped and the value goes to a readout window.

**Toggle** — a pill with a vertical `panel-hi → panel-lo` gradient, 1px dark
border, drop shadow, inner top highlight, and a **lamp**: .55em disc, dark and
recessed when off, `--acc` with a glow when on. Pressing translates it 1px down.
Mute/Solo are the exception, colouring the whole pill (cyan 190°, orange 32°).

**Meter** — .8em × 12em, slot background, recessed, with a gradient bar (green to
24%, yellow to 8%, red above) and a **16-segment overlay** drawn as repeating
panel-coloured 1px lines.

**Drawbar** — a separate species from sliders: no tick ladders, packed tight like a
manual, chrome stem, white/black numbered pull-tabs, 9 positions (0–8), **up =
louder**.

**Backlit LCD** — positive LCD: *dark graphics on lit glass* (amber/green,
FS1R-style), with brightness/tint/saturation as meta parameters, and a machined
selector below it, clear of the glass.

## 6. Interaction (LOCKED)

- **Pointer capture on press** — already structural in `rev-ui-mech`.
- **Slider: absolute** — the cap jumps to the pointer, then tracks.
- **Knob: relative** — vertical drag, full range over ~140px of travel.
- **Detent snap** while dragging: within 3.5% of the detent, snap to it.
- **Wheel = coarse** (3–4% per notch); **tilt (horizontal wheel) = fine** (0.5%).
  Wheel snaps within a tighter 1.5% window.
- **Double-click a control** → return to its detent.
- **Double-click a readout** → type an exact value. **Typed values do not snap.**
- **Hover tooltip** carries the full name and the formatted value with units —
  which is why the on-panel readout can be unit-less and narrow.

## 7. What this implies for our port

1. **Paint vocabulary** — done in ui-03 step 1: linear gradients, outer and inset
   blurred shadows. Still missing for full fidelity: **radial gradients** (knob
   bodies), and **conic or annular arcs** (knob value rings). Neither has a caller
   until step 4, so neither is built yet.
2. **The lamp glow is an outer shadow** with zero offset — no new primitive.
3. **Most "inset shadows" are 1px hairlines**, not blurs, and need nothing new.
4. **Tick ladders, meter segments and the LCD grid are hairline fills** — pixel
   snapping (step 1) is what keeps them crisp at fractional interface scale.
5. **The 5.5ch fixed readout window** is exactly why the numeric font role is
   monospaced. Already satisfied.
6. **Inert dimming** wants a widget-level enabled/valid state in the kit, and an
   a11y consequence: an inert control is still present and must still report itself.
7. **`--pl` as a master dial** argues for the Skin being *derived* rather than a
   flat table of literals — one lightness in, every panel value out.

## 8. Open questions for step 2

- **Single weight, or use the real bold?** §1's port note. Our display target and
  font differ from the exhibits'.
- **Type scale.** The exhibits are relative (`em`) throughout, anchored to a 14px
  panel with a Size meta at 125% — an effective ~17.5px. That lands close to the
  user's stated 14px floor / 18px comfortable, which is reassuring, but our skin
  should state absolute sizes rather than inherit a cascade we do not have.
- **Does the transport get a band?** Groups have coloured role bands; a Control Bar
  has no roles. Either it has no bands, or it uses one neutral band. Unresolved.
