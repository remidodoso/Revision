//! Input routing: platform events in, contract events out.
//!
//! A child module of `mech` so it can reach the driver's internals while keeping
//! `mech.rs` readable. The rules it enforces are ui-01 §7.2 and §7.3:
//!
//! - a press binds its target until release, wherever the pointer goes;
//! - the wheel targets what the pointer is over, not what holds focus;
//! - pressing a control does **not** move focus or raise its window;
//! - raw keys and composed text are two channels, and fields consume the second.

use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::{Key as WinitKey, NamedKey};

use super::{Driver, Host, WindowId};
use crate::geometry::Point;
use crate::input::{Button, Event, Key, KeyCode, Named, Pointer, PointerKind, TargetId, Text};

impl<H: Host> Driver<H> {
    /// Hand an event to the host, which sees the mechanism at the same time.
    fn deliver(&mut self, window: WindowId, target: Option<TargetId>, ev: Event) {
        self.host.event(window, target, &ev, &mut self.mech);
    }

    /// Platform position to logical pixels, through this window's effective scale
    /// (platform DPI composed with the interface scale, R-938).
    fn logical_at(&self, id: WindowId, p: winit::dpi::PhysicalPosition<f64>) -> Point {
        let scale = self
            .mech
            .window
            .get(&id)
            .map_or(1.0, |w| w.effective_scale(self.mech.user_scale));
        Point::new(p.x as f32 / scale, p.y as f32 / scale)
    }

    pub(super) fn pointer_moved(
        &mut self,
        id: WindowId,
        position: winit::dpi::PhysicalPosition<f64>,
    ) {
        let at = self.logical_at(id, position);
        self.mech.pointer_at = at;
        // A relative drag consumes ordinary motion: the pointer is locked in place,
        // and movement arrives as raw device deltas instead.
        if self.mech.dragging() {
            return;
        }
        let modifier = self.mech.modifier;
        let time = self.mech.now();

        // Capture beats hit testing: once a press binds a target, motion goes there
        // however far the pointer has wandered — including outside the window.
        if let Some((window, target)) = self.mech.capture {
            let ev = pointer(PointerKind::Move, at, None, modifier, time);
            self.deliver(window, target, ev);
            return;
        }

        // Hover: leave the old target before entering the new one, so no widget
        // ever sees two enters without a leave between them.
        let now_over = self.host.hit(id, at);
        if self.mech.hover != Some((id, now_over)) {
            if let Some((prev_window, prev)) = self.mech.hover {
                let ev = pointer(PointerKind::Leave, at, None, modifier, time);
                self.deliver(prev_window, prev, ev);
            }
            self.mech.hover = Some((id, now_over));
            let ev = pointer(PointerKind::Enter, at, None, modifier, time);
            self.deliver(id, now_over, ev);
        }
        let ev = pointer(PointerKind::Move, at, None, modifier, time);
        self.deliver(id, now_over, ev);
    }

    pub(super) fn pointer_left(&mut self) {
        let (at, modifier, time) = (self.mech.pointer_at, self.mech.modifier, self.mech.now());
        if let Some((window, target)) = self.mech.hover.take() {
            let ev = pointer(PointerKind::Leave, at, None, modifier, time);
            self.deliver(window, target, ev);
        }
    }

    pub(super) fn pointer_button(
        &mut self,
        id: WindowId,
        state: ElementState,
        button: MouseButton,
    ) {
        let button = match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
            MouseButton::Back => Button::Other(3),
            MouseButton::Forward => Button::Other(4),
            MouseButton::Other(n) => Button::Other(n),
        };
        let (at, modifier, time) = (self.mech.pointer_at, self.mech.modifier, self.mech.now());
        match state {
            ElementState::Pressed => {
                // The press binds its target for the whole gesture. Note what it
                // does *not* do: it does not move focus and does not raise the
                // window. Activation is not focus — the Control Bar's always-active
                // behaviour, generalized to every control.
                let target = self.host.hit(id, at);
                self.mech.capture = Some((id, target));
                let ev = pointer(PointerKind::Down, at, Some(button), modifier, time);
                self.deliver(id, target, ev);
            }
            ElementState::Released => {
                let (window, target) = self.mech.capture.take().unwrap_or((id, None));
                let ev = pointer(PointerKind::Up, at, Some(button), modifier, time);
                self.deliver(window, target, ev);
            }
        }
    }

    pub(super) fn pointer_wheel(&mut self, id: WindowId, delta: MouseScrollDelta) {
        // Both forms arrive; a notch is worth a conventional three lines.
        const LINE: f32 = 16.0;
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x * 3.0 * LINE, y * 3.0 * LINE),
            MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
        };
        let (at, modifier, time) = (self.mech.pointer_at, self.mech.modifier, self.mech.now());
        // Wheel-on-hover: what the pointer is over, not what holds focus.
        let target = self
            .mech
            .hover
            .and_then(|(w, t)| (w == id).then_some(t))
            .flatten();
        let ev = pointer(PointerKind::Wheel { dx, dy }, at, None, modifier, time);
        self.deliver(id, target, ev);
    }

    pub(super) fn pointer_delta(&mut self, window: WindowId, dx: f32, dy: f32) {
        let (at, modifier, time) = (self.mech.pointer_at, self.mech.modifier, self.mech.now());
        let target = self.mech.capture.and_then(|(_, t)| t);
        let ev = pointer(PointerKind::Delta { dx, dy }, at, None, modifier, time);
        self.deliver(window, target, ev);
    }

    pub(super) fn key(&mut self, id: WindowId, event: &winit::event::KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        let code = match &event.logical_key {
            WinitKey::Character(s) => s.chars().next().map(KeyCode::Char),
            WinitKey::Named(named) => Some(KeyCode::Named(translate(*named))),
            _ => Some(KeyCode::Named(Named::Other)),
        };
        let target = self.mech.focus.and_then(|(w, t)| (w == id).then_some(t));

        if let Some(code) = code {
            let ev = Event::Key(Key {
                code,
                pressed,
                repeat: event.repeat,
                modifier: self.mech.modifier,
                time: self.mech.now(),
            });
            self.deliver(id, target, ev);
        }
        // The second channel. Same keystroke, different message: a field consumes
        // this one, because raw keys break under any IME. Control characters are
        // not text — Enter and Tab are keys, and only keys.
        if pressed
            && let Some(text) = &event.text
            && !text.chars().any(char::is_control)
        {
            let ev = Event::Text(Text {
                text: text.to_string(),
            });
            self.deliver(id, target, ev);
        }
    }
}

fn pointer(
    kind: PointerKind,
    at: Point,
    button: Option<Button>,
    modifier: crate::input::Modifier,
    time: crate::input::UiTime,
) -> Event {
    Event::Pointer(Pointer {
        kind,
        at,
        button,
        modifier,
        time,
    })
}

/// Platform key names to ours. Unmapped keys become `Other` rather than being
/// dropped, so a binding can still see that *something* was pressed.
fn translate(named: NamedKey) -> Named {
    match named {
        NamedKey::Enter => Named::Enter,
        NamedKey::Escape => Named::Escape,
        NamedKey::Backspace => Named::Backspace,
        NamedKey::Delete => Named::Delete,
        NamedKey::Tab => Named::Tab,
        NamedKey::Space => Named::Space,
        NamedKey::ArrowLeft => Named::Left,
        NamedKey::ArrowRight => Named::Right,
        NamedKey::ArrowUp => Named::Up,
        NamedKey::ArrowDown => Named::Down,
        NamedKey::Home => Named::Home,
        NamedKey::End => Named::End,
        NamedKey::PageUp => Named::PageUp,
        NamedKey::PageDown => Named::PageDown,
        NamedKey::F1 => Named::Function(1),
        NamedKey::F2 => Named::Function(2),
        NamedKey::F3 => Named::Function(3),
        NamedKey::F4 => Named::Function(4),
        NamedKey::F5 => Named::Function(5),
        NamedKey::F6 => Named::Function(6),
        NamedKey::F7 => Named::Function(7),
        NamedKey::F8 => Named::Function(8),
        _ => Named::Other,
    }
}
