//! The accessibility slot (ui-01 §7.7, R-1510).
//!
//! **Declared now, bridged later.** No AccessKit dependency yet and no platform
//! publication — what ships here is the *shape*: a per-window tree of nodes with
//! stable ids, roles, labels and bounds, which a host can already build and a
//! bridge can later walk.
//!
//! Declaring it before the widget kit exists is the whole point. A kit grown
//! without an accessibility model acquires one only by retrofit — controls whose
//! identity lives in closures, labels that exist only as pixels, state that is
//! implied by colour. Asking a widget for a node from the first day makes those
//! mistakes visible while they are still cheap.

use crate::geometry::Rect;
use crate::input::TargetId;

/// What a node *is*, as an assistive technology would classify it. Deliberately
/// small: the Control Bar's census plus the containers it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    #[default]
    Group,
    Window,
    Button,
    /// A button with a latched state — Loop, Punch, Wait.
    Toggle,
    /// A value that can be typed or dragged — Counter, Tempo, In/Out.
    Field,
    /// A continuous control — the Shuttle.
    Slider,
    PopUp,
    Label,
}

/// One node. `id` is the same [`TargetId`] the mechanism routes input to, so an
/// assistive technology and a pointer address the same thing — which is the
/// property that stops the two drifting apart.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: TargetId,
    pub role: Role,
    /// What it is called. A control whose label exists only as painted pixels
    /// cannot answer this, which is exactly the design smell worth catching early.
    pub label: String,
    /// Logical bounds within the window.
    pub bounds: Rect,
    /// Latched, pressed, checked — whatever the role's state means.
    pub on: Option<bool>,
    /// Current value for `Field` and `Slider`, already formatted for reading.
    pub value: Option<String>,
    pub child: Vec<Node>,
}

impl Node {
    pub fn new(id: TargetId, role: Role, label: impl Into<String>, bounds: Rect) -> Node {
        Node {
            id,
            role,
            label: label.into(),
            bounds,
            on: None,
            value: None,
            child: Vec::new(),
        }
    }

    pub fn with_state(mut self, on: bool) -> Node {
        self.on = Some(on);
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Node {
        self.value = Some(value.into());
        self
    }

    pub fn with_child(mut self, child: Vec<Node>) -> Node {
        self.child = child;
        self
    }

    /// Depth-first walk, parents before children — the order a bridge publishes in
    /// and a reader announces in.
    pub fn walk<'n>(&'n self, visit: &mut impl FnMut(&'n Node)) {
        visit(self);
        for c in &self.child {
            c.walk(visit);
        }
    }

    /// Find a node by the target id the mechanism would route to.
    pub fn find(&self, id: TargetId) -> Option<&Node> {
        if self.id == id {
            return Some(self);
        }
        self.child.iter().find_map(|c| c.find(id))
    }
}

/// One window's tree. Empty until a host populates it; v0 hosts may leave it so.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tree {
    pub root: Option<Node>,
}

impl Tree {
    pub fn of(root: Node) -> Tree {
        Tree { root: Some(root) }
    }

    pub fn empty(&self) -> bool {
        self.root.is_none()
    }

    /// Every node, parents first.
    pub fn node(&self) -> Vec<&Node> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            root.walk(&mut |n| out.push(n));
        }
        out
    }
}

#[cfg(test)]
mod test;
