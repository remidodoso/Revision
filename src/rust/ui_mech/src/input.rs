//! Input as the contract states it (ui-01 §7.2, §7.3).
//!
//! Three properties are structural here rather than left to widgets:
//!
//! - **Implicit capture.** A press binds its target until release, wherever the
//!   pointer travels. Widgets cannot forget to do it because they are not asked to.
//! - **Keyboard and text are two channels.** Raw keys drive bindings and key
//!   equivalents; composed text drives fields. A field that consumed raw keys would
//!   break under any IME, so the split exists before IME does.
//! - **Activation is not focus.** Pressing a control operates it without taking
//!   focus or raising its window; focus moves only when a target asks.
//!
//! No winit type appears in this module's API — `rev-ui-kit` must not be able to
//! name one.

use crate::geometry::Point;

/// Monotonic seconds since the mechanism started. The **UI clock, not the engine
/// clock**: engine positions arrive over the telemetry ring.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct UiTime(pub f64);

/// An opaque handle to whatever the host hit-tested. The mechanism routes it and
/// never interprets it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetId(pub u64);

/// Why focus moved. Every transfer is one or the other, so "nothing steals focus"
/// (R-907) is checkable rather than aspirational.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    User,
    Programmatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifier {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Windows key / Command.
    pub meta: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerKind {
    Down,
    Move,
    Up,
    /// Positive `dy` scrolls content up, in logical pixels per notch-equivalent.
    Wheel {
        dx: f32,
        dy: f32,
    },
    Enter,
    Leave,
    /// Relative motion during a captured drag (see `Mech::begin_relative_drag`).
    /// `at` is meaningless; the delta is the whole message.
    Delta {
        dx: f32,
        dy: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointer {
    pub kind: PointerKind,
    /// Logical position within the window.
    pub at: Point,
    pub button: Option<Button>,
    pub modifier: Modifier,
    pub time: UiTime,
}

/// A key that is not a character: the ones bindings and key equivalents need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    Enter,
    Escape,
    Backspace,
    Delete,
    Tab,
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Function(u8),
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Named(Named),
}

/// A raw key event — for bindings, key equivalents, and performance input. **Not**
/// for typing into fields; that is [`Text`].
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub code: KeyCode,
    pub pressed: bool,
    pub repeat: bool,
    pub modifier: Modifier,
    pub time: UiTime,
}

/// Composed text, IME-mediated. Numeric fields (Counter, Tempo, In/Out) consume
/// this, never [`Key`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Pointer(Pointer),
    Key(Key),
    Text(Text),
}

/// What the pointer should look like over a widget. Requested per frame; the
/// mechanism applies the last request and resets when nothing asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Default,
    Text,
    Hand,
    ResizeHorizontal,
    ResizeVertical,
    Crosshair,
    /// Hidden — used by relative drag, and by nothing else.
    None,
}
