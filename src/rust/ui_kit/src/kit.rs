//! The widget tree: retained, stable-identified, and **made of data**.
//!
//! Two decisions shape everything here.
//!
//! **A widget is a value, not a closure.** ui-01 §8 sketched a `Widget` trait; this
//! is a deliberate departure. R-622 wants panels generated from device profiles and
//! R-1310 wants them assembled by scripts — both need a widget to be something you
//! can *describe*, not something you can only call. A tree of plain data can be
//! built by a script, generated from a profile, serialized, and edited by a layout
//! designer later; a tree of trait objects holding closures can be none of those.
//! The roster is closed on purpose: this is our kit, not an external toolkit, so an
//! enum costs nothing and buys exhaustiveness.
//!
//! **Design and interaction are separate.** The tree holds what a widget *is* —
//! kind, label, value, placement. Hover and press live beside it, keyed by id, and
//! are never written into the design. A layout designer edits the first and must
//! never see the second.

use rev_ui_mech::{CursorShape, Node, Point, Rect, Role as A11yRole, Size, TargetId, Tree};

use crate::skin::{Skin, State};

/// Stable across frames and across relayouts — the same value the mechanism routes
/// input to and an assistive technology addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetId(pub u32);

impl WidgetId {
    fn target(self) -> TargetId {
        TargetId(u64::from(self.0))
    }
}

/// Which parent edges a widget's own edges follow when the parent resizes. Absolute
/// placement with anchors, not a layout engine: the skin is pixel-designed, and a
/// solver would be a large answer to a question nobody asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Anchor {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

impl Anchor {
    pub const TOP_LEFT: Anchor = Anchor {
        left: true,
        top: true,
        right: false,
        bottom: false,
    };
    /// Stretches horizontally with its parent.
    pub const TOP_WIDE: Anchor = Anchor {
        left: true,
        top: true,
        right: true,
        bottom: false,
    };
    pub const FILL: Anchor = Anchor {
        left: true,
        top: true,
        right: true,
        bottom: true,
    };
}

/// What a widget is. Values, so a tree can be described rather than only executed.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// A frame that groups others. Paints the panel face and its bevel.
    Panel,
    /// Static text.
    Label,
    /// A divider between clusters. Reports nothing and takes no input; it exists so
    /// that groups read as groups without a box around each one.
    Rule,
    /// Momentary: fires on release, holds no state of its own.
    Button,
    /// Latching: `on` is its state, and the state is part of the design.
    Toggle { on: bool },
    /// An indicator. Reports, never accepts input.
    Lamp { lit: bool },
    /// A value in the readout window: monospaced, amber, fixed width.
    Readout { value: String },
    /// The transport's record control. **Three intrinsic states**, not two — the
    /// distinction Vision's record light drew and most modern transports lost.
    Record { mode: RecordMode },
    /// One locator: a numbered slot, grey until it holds a position, settable on
    /// the fly during playback.
    Locator { index: u8, at: Option<String> },
    /// A multi-field numeric display. Fields are addressed individually: click one,
    /// then drag or type it. The separator carries the meaning — `|` for
    /// bar|beat|unit, `.` for a decimal tempo, `:` for SMPTE — so one widget serves
    /// all three rather than three widgets differing by a punctuation mark.
    Counter { field: Vec<Field>, separator: char },
    /// A menu that shows its current choice and opens a list when pressed.
    PopUp { option: Vec<String>, chosen: usize },
    /// A continuous value, 0..=1, as a vertical slider — the exhibits' default for
    /// anything continuous. `detent` is the **semantic** zero, which is not always
    /// the middle: a bipolar tilt centres, an organ's spread does not.
    Slider { value: f32, detent: Option<f32> },
    /// A sprung continuous control: scrub while held, return to rest when let go.
    /// `position` runs -1..=1 and is **not** a value the user sets — it is where
    /// the control currently is, and it goes home by itself.
    Shuttle { position: f32 },
}

/// What the transport is doing about recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordMode {
    /// Not recording and not armed.
    Off,
    /// Armed and waiting for the transport. **Flashes** — and the flashing is the
    /// whole point: armed-but-not-yet-recording is a state you must be able to see
    /// across a room, because the cost of misreading it is a lost take.
    Armed,
    Recording,
}

/// One field of a [`Kind::Counter`].
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub value: i64,
    /// Zero-padded to this width, so the display cannot reflow as values change.
    pub digit: u8,
    pub min: i64,
    pub max: i64,
}

impl Field {
    pub fn new(value: i64, digit: u8, min: i64, max: i64) -> Field {
        Field {
            value,
            digit,
            min,
            max,
        }
    }

    fn text(&self) -> String {
        format!("{:0>width$}", self.value, width = self.digit as usize)
    }
}

/// One widget. Plain data throughout — no closures, no handles, nothing that
/// resists being written to a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Widget {
    pub id: WidgetId,
    pub kind: Kind,
    /// What it is called. **Required, not optional**: a control that cannot name
    /// itself cannot be announced, and making the field mandatory is what stops a
    /// kit from growing controls whose identity exists only as painted pixels.
    pub label: String,
    /// Placement within the parent, as designed.
    pub rect: Rect,
    pub anchor: Anchor,
    /// Present and visible but not operable at the current settings — the exhibits'
    /// inert rule. Still painted, still announced.
    pub inert: bool,
    /// Transport semantics, where the widget has any.
    pub state: State,
    pub child: Vec<Widget>,
}

impl Widget {
    pub fn new(id: u32, kind: Kind, label: impl Into<String>, rect: Rect) -> Widget {
        Widget {
            id: WidgetId(id),
            kind,
            label: label.into(),
            rect,
            anchor: Anchor::TOP_LEFT,
            inert: false,
            state: State::Idle,
            child: Vec::new(),
        }
    }

    pub fn with_anchor(mut self, anchor: Anchor) -> Widget {
        self.anchor = anchor;
        self
    }

    pub fn with_state(mut self, state: State) -> Widget {
        self.state = state;
        self
    }

    pub fn with_child(mut self, child: Vec<Widget>) -> Widget {
        self.child = child;
        self
    }

    /// The value an assistive technology should read, where there is one.
    fn value(&self) -> Option<String> {
        match &self.kind {
            Kind::Readout { value } => Some(value.clone()),
            Kind::Locator { at, .. } => at.clone(),
            Kind::Counter { field, .. } => {
                Some(field.iter().map(Field::text).collect::<Vec<_>>().join("|"))
            }
            Kind::Slider { value, .. } => Some(format!("{:.0}%", value * 100.0)),
            Kind::PopUp { option, chosen } => option.get(*chosen).cloned(),
            Kind::Shuttle { position } => Some(format!("{position:+.2}")),
            Kind::Record { mode } => Some(
                match mode {
                    RecordMode::Off => "off",
                    RecordMode::Armed => "armed",
                    RecordMode::Recording => "recording",
                }
                .to_string(),
            ),
            _ => None,
        }
    }

    fn on(&self) -> Option<bool> {
        match &self.kind {
            Kind::Toggle { on } => Some(*on),
            Kind::Lamp { lit } => Some(*lit),
            Kind::Record { mode } => Some(*mode != RecordMode::Off),
            Kind::Locator { at, .. } => Some(at.is_some()),
            _ => None,
        }
    }

    fn a11y_role(&self) -> A11yRole {
        match self.kind {
            Kind::Panel => A11yRole::Group,
            Kind::Label => A11yRole::Label,
            Kind::Rule => A11yRole::Group,
            Kind::Button => A11yRole::Button,
            Kind::Toggle { .. } => A11yRole::Toggle,
            Kind::Lamp { .. } => A11yRole::Label,
            Kind::Readout { .. } => A11yRole::Field,
            Kind::Record { .. } => A11yRole::Toggle,
            Kind::Locator { .. } => A11yRole::Button,
            Kind::Counter { .. } => A11yRole::Field,
            Kind::Slider { .. } => A11yRole::Slider,
            Kind::PopUp { .. } => A11yRole::PopUp,
            Kind::Shuttle { .. } => A11yRole::Slider,
        }
    }

    /// Does this kind accept input at all?
    fn operable(&self) -> bool {
        matches!(
            self.kind,
            Kind::Button
                | Kind::Toggle { .. }
                | Kind::Record { .. }
                | Kind::Locator { .. }
                | Kind::Counter { .. }
                | Kind::PopUp { .. }
                | Kind::Slider { .. }
                | Kind::Shuttle { .. }
        ) && !self.inert
    }
}

/// Interaction constants, transcribed from the exhibits (`revision_skin_inventory`
/// §6) rather than invented. They are the settled feel of the control skin, and
/// changing one is a design decision, not a tuning tweak.
mod feel {
    /// Snap to a detent within this fraction of the range while dragging.
    pub const SNAP_DRAG: f32 = 0.035;
    /// Tighter while nudging: a wheel click should be able to step *past* a detent
    /// rather than being swallowed by it.
    pub const SNAP_WHEEL: f32 = 0.015;
    /// One wheel notch.
    pub const COARSE: f32 = 0.04;
    /// One tilt notch — the horizontal wheel, which is the fine adjustment.
    pub const FINE: f32 = 0.005;
    /// Two presses within this many seconds, close together, are one gesture.
    pub const DOUBLE: f64 = 0.4;
    /// ...and within this many logical pixels.
    pub const DOUBLE_SLOP: f32 = 4.0;
}

/// What happened — never what it means. The application maps `(WidgetId, Intent)`
/// onto a command; the kit has no idea a model exists (R-901).
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Pressed,
    Released,
    /// The press was abandoned: the pointer left the widget before release. **Not**
    /// a release — a control that fires when you drag away and let go is a control
    /// you cannot back out of, and backing out is what the gesture is for.
    Cancelled,
    Toggled(bool),
    /// The record control was operated. The kit reports the press; **what a press
    /// means is the application's** — arm, disarm, or stop is a transport question.
    RecordPressed(RecordMode),
    /// A locator was chosen (it holds a position) or asked to be set (it does not).
    Recalled(u8),
    Store(u8),
    /// A counter field changed: which field, and its new value.
    FieldChanged(usize, i64),
    /// A continuous control moved.
    ValueChanged(f32),
    /// A pop-up's choice changed.
    Chose(usize),
    /// The shuttle moved. Fires continuously while held, and once more at rest
    /// when released — the return to zero is an event too, or the transport never
    /// learns that scrubbing stopped.
    Shuttled(f32),
}

/// Ephemeral interaction state, deliberately separate from the design.
#[derive(Debug, Clone, Default, PartialEq)]
struct Touch {
    hover: Option<WidgetId>,
    press: Option<WidgetId>,
    /// Which counter field the pointer addressed. Interaction, not design.
    field: Option<usize>,
    /// Where a drag started, and the value it started from.
    drag: Option<(f32, i64)>,
    /// Is the pointer still over the widget it pressed? A pressed control tracks
    /// the pointer: it unhighlights when you drag off and highlights again when
    /// you come back, so the cancel is visible before you commit to it.
    /// (Apple HIG 1992, ch. 7, "Button Behavior".)
    press_inside: bool,
    /// Which counter field the pointer is over, as opposed to which one a click
    /// addressed. The wheel acts on this one — the thing under the pointer is the
    /// thing you are aiming at.
    hover_field: Option<(WidgetId, usize)>,
    /// Which item of an open list the pointer is over. Provisional: it says what
    /// *would* be chosen, which is the whole job of a menu you drag through.
    item: Option<usize>,
    /// The pop-up whose list is showing. Ephemeral: an open menu is something the
    /// user is doing, not something the design says.
    open: Option<WidgetId>,
    /// Where a shuttle drag began: pointer x, and the position it started from.
    shuttle: Option<(f32, f32)>,
    /// The counter field being edited, if any: widget and field index.
    focus: Option<(WidgetId, usize)>,
    /// The last press: target, when, and where — for recognising a double-click.
    /// The mechanism reports presses; deciding that two of them are one gesture is
    /// a widget-level judgement, so it is made here.
    tap: Option<(WidgetId, f64, Point)>,
    /// Digits typed so far. `None` means the field shows its value; `Some` means it
    /// shows what is being typed, which is not the same thing and must not be
    /// mistaken for it — an abandoned edit has to leave the value untouched.
    edit: Option<String>,
}

/// A widget tree, laid out and ready to paint.
pub struct Kit {
    root: Widget,
    skin: Skin,
    /// Absolute rects, resolved by [`Kit::layout`]. Kept beside the tree rather than
    /// inside it: the design says where a widget sits within its parent, and this
    /// says where that landed.
    placed: Vec<(WidgetId, Rect)>,
    touch: Touch,
    dirty: Vec<Rect>,
    /// Blink phase for armed controls. Derived from the UI clock, never stored in
    /// the design — a flashing widget must not serialize as "currently bright".
    blink: bool,
}

impl Kit {
    pub fn new(root: Widget, skin: Skin) -> Kit {
        let mut kit = Kit {
            root,
            skin,
            placed: Vec::new(),
            touch: Touch::default(),
            dirty: Vec::new(),
            blink: true,
        };
        kit.layout(Rect::new(0.0, 0.0, 0.0, 0.0));
        kit
    }

    pub fn skin(&self) -> &Skin {
        &self.skin
    }

    /// Resolve every widget's absolute rect for a window of this size. Anchored
    /// edges follow the parent; unanchored ones keep their designed offset.
    pub fn layout(&mut self, window: Rect) {
        self.placed.clear();
        let root = std::mem::replace(
            &mut self.root,
            Widget::new(0, Kind::Panel, "", Rect::default()),
        );
        let designed = Size::new(root.rect.w, root.rect.h);
        place(&root, window, designed, &mut self.placed);
        self.root = root;
        self.dirty.push(window);
    }

    /// Absolute rect of a widget, once laid out.
    pub fn rect(&self, id: WidgetId) -> Option<Rect> {
        self.placed.iter().find(|(w, _)| *w == id).map(|(_, r)| *r)
    }

    /// Topmost widget at a point — children before parents, later siblings first,
    /// so what is drawn on top is what is hit.
    pub fn hit(&self, at: Point) -> Option<TargetId> {
        // An open menu is in front of the whole tree, including widgets drawn
        // after its owner — z-order, not document order.
        if let Some(open) = self.touch.open
            && let Some(rect) = self.rect(open)
            && let Some(Kind::PopUp { option, .. }) = self.find(open).map(|w| &w.kind)
        {
            let list = list_rect(rect, option.len());
            if list.contains(at) || rect.contains(at) {
                return Some(open.target());
            }
            // A press anywhere else is a dismissal, and must not also operate
            // whatever it landed on.
            return Some(open.target());
        }
        self.placed
            .iter()
            .rev()
            .find(|(id, r)| r.contains(at) && self.find(*id).is_some_and(|w| w.operable()))
            .map(|(id, _)| id.target())
    }

    /// Advance time-driven appearance. Returns true when something changed and a
    /// frame is owed.
    ///
    /// The **skin supplies the colour, the kit supplies the flashing** — a widget
    /// does not own a timer, and the phase is derived from the UI clock so two
    /// armed controls can never blink out of step with each other.
    pub fn animate(&mut self, seconds: f64) -> bool {
        // 2 Hz: fast enough to read as urgent, slow enough not to strobe.
        let phase = ((seconds * 2.0) as u64).is_multiple_of(2);
        if phase == self.blink {
            return false;
        }
        self.blink = phase;
        let armed: Vec<WidgetId> = self
            .placed
            .iter()
            .filter(|(id, _)| {
                self.find(*id).is_some_and(|w| {
                    matches!(
                        w.kind,
                        Kind::Record {
                            mode: RecordMode::Armed
                        }
                    )
                })
            })
            .map(|(id, _)| *id)
            .collect();
        for id in &armed {
            self.mark(*id);
        }
        !armed.is_empty()
    }

    /// Is anything time-driven right now? The application asks before requesting a
    /// wake-up, so a still transport lets the loop sleep.
    pub fn animating(&self) -> bool {
        self.placed.iter().any(|(id, _)| {
            self.find(*id).is_some_and(|w| {
                matches!(
                    w.kind,
                    Kind::Record {
                        mode: RecordMode::Armed
                    }
                )
            })
        })
    }

    /// Which target, if any, is editing — what the application hands to
    /// `Mech::set_focus` so the mechanism routes keys and text here.
    ///
    /// The kit never touches the mechanism; it reports what it wants, and the
    /// application decides. Focus moves because a widget asked, which is the only
    /// way it is allowed to move (R-907).
    pub fn editing(&self) -> Option<TargetId> {
        self.touch.focus.map(|(id, _)| id.target())
    }

    /// Mark a pop-up and the area its list occupies.
    fn mark_list(&mut self, id: WidgetId) {
        let Some(rect) = self.rect(id) else { return };
        let count = match self.find(id).map(|w| &w.kind) {
            Some(Kind::PopUp { option, .. }) => option.len(),
            _ => 0,
        };
        self.dirty.push(rect);
        self.dirty.push(list_rect(rect, count));
    }

    /// Which item of an open list a point addresses, if any.
    fn item_at(&self, id: WidgetId, at: Point) -> Option<usize> {
        let rect = self.rect(id)?;
        let count = match &self.find(id)?.kind {
            Kind::PopUp { option, .. } => option.len(),
            _ => return None,
        };
        let list = list_rect(rect, count);
        if !list.contains(at) {
            return None;
        }
        let n = ((at.y - list.y) / rect.h) as usize;
        (n < count).then_some(n)
    }

    /// Is a menu showing?
    pub fn menu_open(&self) -> bool {
        self.touch.open.is_some()
    }

    /// Which counter field sits under a point, if this widget is a counter.
    pub(crate) fn field_under(&self, id: WidgetId, at: Point) -> Option<usize> {
        let rect = self.rect(id)?;
        match &self.find(id)?.kind {
            Kind::Counter { field, .. } => field_at(field, rect, at.x, &self.skin),
            _ => None,
        }
    }

    /// Is this widget pressed *and* still under the pointer? Only then does it
    /// show as pressed.
    pub(crate) fn press_shown(&self, id: WidgetId) -> bool {
        self.touch.press == Some(id) && self.touch.press_inside
    }

    /// Which counter field the pointer is over, if any.
    pub(crate) fn hovered_field(&self, id: WidgetId) -> Option<usize> {
        self.touch
            .hover_field
            .and_then(|(w, n)| (w == id).then_some(n))
    }

    /// What the pointer should look like over whatever it is hovering.
    ///
    /// The kit reports a shape and the application asks the mechanism for it — the
    /// kit still never touches the mechanism. This is the affordance that says a
    /// control can be dragged or wheeled *before* anyone tries it, which a static
    /// panel otherwise leaves you to guess.
    pub fn cursor(&self) -> CursorShape {
        let Some(id) = self.touch.hover else {
            return CursorShape::Default;
        };
        match self.find(id).map(|w| (&w.kind, w.inert)) {
            Some((_, true)) => CursorShape::Default,
            Some((Kind::Slider { .. } | Kind::Counter { .. }, _)) => CursorShape::ResizeVertical,
            Some((Kind::Shuttle { .. }, _)) => CursorShape::ResizeHorizontal,
            Some((
                Kind::Button
                | Kind::Toggle { .. }
                | Kind::Record { .. }
                | Kind::Locator { .. }
                | Kind::PopUp { .. },
                _,
            )) => CursorShape::Hand,
            _ => CursorShape::Default,
        }
    }

    /// The item the pointer is over in an open list, if any.
    pub(crate) fn hovered_item(&self) -> Option<usize> {
        self.touch.item
    }

    /// What state the record control is in.
    pub fn record_mode(&self, id: WidgetId) -> Option<RecordMode> {
        match self.find(id)?.kind {
            Kind::Record { mode } => Some(mode),
            _ => None,
        }
    }

    /// What a locator holds, if anything.
    pub fn locator_text(&self, id: WidgetId) -> Option<String> {
        match &self.find(id)?.kind {
            Kind::Locator { at, .. } => at.clone(),
            _ => None,
        }
    }

    /// A counter's reading as text — what a locator stores when it is set.
    pub fn counter_text(&self, id: WidgetId) -> Option<String> {
        match &self.find(id)?.kind {
            Kind::Counter { field, .. } => {
                Some(field.iter().map(Field::text).collect::<Vec<_>>().join("|"))
            }
            _ => None,
        }
    }

    /// Areas needing repaint since the last call.
    pub fn take_dirty(&mut self) -> Vec<Rect> {
        std::mem::take(&mut self.dirty)
    }

    fn find(&self, id: WidgetId) -> Option<&Widget> {
        fn walk(w: &Widget, id: WidgetId) -> Option<&Widget> {
            if w.id == id {
                return Some(w);
            }
            w.child.iter().find_map(|c| walk(c, id))
        }
        walk(&self.root, id)
    }

    fn find_mut(&mut self, id: WidgetId) -> Option<&mut Widget> {
        fn walk(w: &mut Widget, id: WidgetId) -> Option<&mut Widget> {
            if w.id == id {
                return Some(w);
            }
            w.child.iter_mut().find_map(|c| walk(c, id))
        }
        walk(&mut self.root, id)
    }

    /// The accessibility tree (R-1510), addressed by the same ids input uses.
    pub fn a11y(&self) -> Tree {
        Tree::of(self.a11y_node(&self.root))
    }

    fn a11y_node(&self, w: &Widget) -> Node {
        let rect = self.rect(w.id).unwrap_or(w.rect);
        let mut node = Node::new(w.id.target(), w.a11y_role(), w.label.clone(), rect);
        if let Some(on) = w.on() {
            node = node.with_state(on);
        }
        if let Some(value) = w.value() {
            node = node.with_value(value);
        }
        node.with_child(w.child.iter().map(|c| self.a11y_node(c)).collect())
    }
    fn mark(&mut self, id: WidgetId) {
        if let Some(r) = self.rect(id) {
            self.dirty.push(r);
        }
    }
}

/// Where an open menu's list sits: directly below its button, same width.
fn list_rect(rect: Rect, count: usize) -> Rect {
    Rect::new(rect.x, rect.bottom() + 2.0, rect.w, rect.h * count as f32)
}

/// Which counter field a click at `x` addresses.
///
/// Advance widths are recomputed from the monospaced metric rather than measured:
/// the numeric font role is fixed-width by construction, so a character count is
/// exact, and hit testing must not need a text shaper to answer.
fn field_at(field: &[Field], rect: Rect, x: f32, skin: &Skin) -> Option<usize> {
    // The monospace advance for the readout size. JetBrains Mono's advance is
    // 0.6em; the constant lives here because this is the only place that needs it
    // without a painter to ask.
    let advance = skin.kind.readout * 0.6;
    let mut left = rect.x + 8.0;
    for (n, f) in field.iter().enumerate() {
        let width = advance * f64::from(f.digit) as f32;
        if x < left + width + advance / 2.0 {
            return Some(n);
        }
        left += width + advance;
    }
    field.len().checked_sub(1)
}

/// Resolve absolute rects, parents before children.
///
/// `designed` is the parent's size **as drawn in the design**, not as resolved — a
/// child stretches by however much its parent grew, and comparing a resolved size
/// against itself would always say "not at all".
fn place(w: &Widget, resolved: Rect, designed: Size, out: &mut Vec<(WidgetId, Rect)>) {
    out.push((w.id, resolved));
    for c in &w.child {
        let child = resolve(c.rect, c.anchor, resolved, designed);
        place(c, child, Size::new(c.rect.w, c.rect.h), out);
    }
}

/// Apply anchors: an anchored edge keeps its distance from the parent's
/// corresponding edge, an unanchored one keeps the widget's size.
fn resolve(rect: Rect, anchor: Anchor, parent: Rect, designed: Size) -> Rect {
    let grew = Size::new(parent.w - designed.w, parent.h - designed.h);
    let mut out = Rect::new(parent.x + rect.x, parent.y + rect.y, rect.w, rect.h);
    if anchor.right && anchor.left {
        out.w += grew.w;
    } else if anchor.right {
        out.x += grew.w;
    }
    if anchor.bottom && anchor.top {
        out.h += grew.h;
    } else if anchor.bottom {
        out.y += grew.h;
    }
    out
}

mod draw;
mod event;

#[cfg(test)]
mod test;
