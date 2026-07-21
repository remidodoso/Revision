//! What input does to the tree.
//!
//! Split from `kit.rs` to keep both files inside the size rule; the seam is real,
//! because everything here reaches the tree only through `find`, `find_mut`, `rect`
//! and `mark`. `kit.rs` keeps the tree, its layout and its queries.
//!
//! The rules obeyed here are the exhibits' settled interaction, not invented:
//! sliders are absolute, detents snap wider under a drag than under a wheel notch,
//! the tilt wheel is the fine adjustment, and a double-click goes home. The
//! constants live in [`super::feel`] so they read as design rather than as tuning.

use rev_ui_mech::{Event, KeyCode, Named, Point, PointerKind, TargetId};

use super::{Field, Intent, Kind, Kit, RecordMode, WidgetId, feel, field_at};

impl Kit {
    /// Move a slider's cap to where the pointer is, snapping near a detent.
    fn slide(&mut self, id: WidgetId, at: Point) -> Option<(WidgetId, Intent)> {
        let rect = self.rect(id)?;
        // Up is more: the cap's height is dead travel at both ends, so the value
        // runs over what is left.
        let raw = 1.0 - ((at.y - rect.y) / rect.h.max(1.0));
        self.set_continuous(id, raw, feel::SNAP_DRAG)
    }

    /// Nudge a continuous control, or a counter field, by a fraction of its range.
    fn nudge(
        &mut self,
        id: WidgetId,
        by: f32,
        field_index: Option<usize>,
    ) -> Option<(WidgetId, Intent)> {
        match self.find(id)?.kind {
            Kind::Slider { value, .. } => self.set_continuous(id, value + by, feel::SNAP_WHEEL),
            Kind::Counter { .. } => {
                // A counter has no fractional range: a notch is a step of one, on
                // whichever field the pointer last addressed.
                let index = field_index.or(self.touch.field).unwrap_or(0);
                let step = if by > 0.0 { 1 } else { -1 };
                let widget = self.find_mut(id)?;
                let Kind::Counter { field, .. } = &mut widget.kind else {
                    return None;
                };
                let f = field.get_mut(index)?;
                let next = (f.value + step).clamp(f.min, f.max);
                if next == f.value {
                    return None;
                }
                f.value = next;
                self.mark(id);
                Some((id, Intent::FieldChanged(index, next)))
            }
            _ => None,
        }
    }

    /// Set a slider, clamped, snapped to its detent when near enough.
    fn set_continuous(&mut self, id: WidgetId, to: f32, snap: f32) -> Option<(WidgetId, Intent)> {
        let widget = self.find_mut(id)?;
        let Kind::Slider { value, detent } = &mut widget.kind else {
            return None;
        };
        let mut next = to.clamp(0.0, 1.0);
        if let Some(d) = *detent
            && (next - d).abs() < snap
        {
            next = d;
        }
        if (next - *value).abs() < 0.0005 {
            return None;
        }
        *value = next;
        self.mark(id);
        Some((id, Intent::ValueChanged(next)))
    }

    /// Return a control to its detent — the double-click gesture.
    fn recentre(&mut self, id: WidgetId) -> Option<(WidgetId, Intent)> {
        let widget = self.find_mut(id)?;
        match &mut widget.kind {
            Kind::Slider { value, detent } => {
                let d = (*detent)?;
                if (*value - d).abs() < 0.0005 {
                    return None;
                }
                *value = d;
                self.mark(id);
                Some((id, Intent::ValueChanged(d)))
            }
            Kind::Shuttle { position } => {
                if *position == 0.0 {
                    return None;
                }
                *position = 0.0;
                self.mark(id);
                Some((id, Intent::Shuttled(0.0)))
            }
            _ => None,
        }
    }

    /// Set a slider from the application.
    pub fn set_value(&mut self, id: WidgetId, to: f32) {
        if let Some(w) = self.find_mut(id)
            && let Kind::Slider { value, .. } = &mut w.kind
        {
            let next = to.clamp(0.0, 1.0);
            if (next - *value).abs() >= 0.0005 {
                *value = next;
                self.mark(id);
            }
        }
    }

    /// Composed text — the **second** keyboard channel. Fields consume this and
    /// never raw keys, because raw keys break under any input method.
    fn typed(&mut self, text: &str) -> Option<(WidgetId, Intent)> {
        let (id, index) = self.touch.focus?;
        let digit = self.field_digits(id, index)?;
        let mut buffer = self.touch.edit.take().unwrap_or_default();
        for c in text.chars().filter(char::is_ascii_digit) {
            if buffer.len() < digit as usize {
                buffer.push(c);
            }
        }
        self.touch.edit = Some(buffer);
        self.mark(id);
        // Nothing is reported until the edit is committed: a half-typed number is
        // not a value anyone downstream should act on.
        None
    }

    /// Raw keys — the **first** channel: commit, cancel, correct, move on.
    fn key(&mut self, code: &KeyCode) -> Option<(WidgetId, Intent)> {
        let (id, index) = self.touch.focus?;
        match code {
            KeyCode::Named(Named::Enter) => {
                let out = self.commit();
                self.touch.focus = None;
                out
            }
            KeyCode::Named(Named::Escape) => {
                // Cancel restores nothing, because typing never changed anything:
                // the buffer is dropped and the value was never touched.
                self.touch.edit = None;
                self.touch.focus = None;
                self.mark(id);
                None
            }
            KeyCode::Named(Named::Backspace) => {
                if let Some(buffer) = &mut self.touch.edit {
                    buffer.pop();
                    self.mark(id);
                }
                None
            }
            KeyCode::Named(Named::Tab) => {
                let out = self.commit();
                let count = self.field_count(id)?;
                self.touch.focus = Some((id, (index + 1) % count));
                self.touch.edit = None;
                self.mark(id);
                out
            }
            _ => None,
        }
    }

    /// Apply the typed digits, if any. Out-of-range input clamps rather than being
    /// refused: a rejected keystroke leaves the user guessing which one it was.
    fn commit(&mut self) -> Option<(WidgetId, Intent)> {
        let (id, index) = self.touch.focus?;
        let buffer = self.touch.edit.take()?;
        let typed: i64 = buffer.parse().ok()?;
        let widget = self.find_mut(id)?;
        let Kind::Counter { field, .. } = &mut widget.kind else {
            return None;
        };
        let f = field.get_mut(index)?;
        let value = typed.clamp(f.min, f.max);
        let changed = value != f.value;
        f.value = value;
        self.mark(id);
        changed.then_some((id, Intent::FieldChanged(index, value)))
    }

    fn field_digits(&self, id: WidgetId, index: usize) -> Option<u8> {
        match &self.find(id)?.kind {
            Kind::Counter { field, .. } => field.get(index).map(|f| f.digit),
            _ => None,
        }
    }

    fn field_count(&self, id: WidgetId) -> Option<usize> {
        match &self.find(id)?.kind {
            Kind::Counter { field, .. } => Some(field.len()),
            _ => None,
        }
    }

    /// What a counter field is showing right now: the typed buffer while an edit is
    /// in progress, its value otherwise.
    pub(crate) fn field_text(&self, id: WidgetId, index: usize, f: &Field) -> (String, bool) {
        if self.touch.focus == Some((id, index))
            && let Some(buffer) = &self.touch.edit
        {
            return (format!("{buffer:_>width$}", width = f.digit as usize), true);
        }
        (f.text(), false)
    }

    /// Is this field the one being edited?
    pub(crate) fn field_focused(&self, id: WidgetId, index: usize) -> bool {
        self.touch.focus == Some((id, index))
    }

    /// Route one already-targeted event. Returns what happened, if anything.
    ///
    /// The mechanism has already decided *who* — capture during a drag, hover for a
    /// wheel, focus for a key. The kit decides *what*.
    pub fn event(&mut self, target: Option<TargetId>, ev: &Event) -> Option<(WidgetId, Intent)> {
        let target = target.and_then(|t| u32::try_from(t.0).ok()).map(WidgetId);
        match ev {
            Event::Text(t) => return self.typed(&t.text),
            Event::Key(k) if k.pressed => return self.key(&k.code),
            Event::Key(_) => return None,
            Event::Pointer(_) => {}
        }
        let Event::Pointer(p) = ev else {
            return None;
        };
        match p.kind {
            PointerKind::Enter => {
                if let Some(old) = self.touch.hover {
                    self.mark(old);
                }
                self.touch.hover = target;
                if let Some(id) = target {
                    self.mark(id);
                }
                None
            }
            // Wheel is coarse, tilt is fine. The horizontal wheel is not a second
            // scroll axis on a control — it is the fine adjustment, which is the
            // exhibits' settled answer and the reason a mouse with a tilt wheel is
            // worth having in front of a synthesizer.
            PointerKind::Wheel { dx, dy } => {
                let target = target?;
                let fine = dx.abs() > dy.abs();
                let notch = if fine { dx } else { dy };
                let step = if fine { feel::FINE } else { feel::COARSE };
                let by = step * notch.signum();
                // On the field under the pointer, not the one a click last
                // addressed: the wheel aims where you are looking.
                let field = self.field_under(target, p.at);
                self.nudge(target, by, field)
            }
            PointerKind::Leave => {
                if let Some(old) = self.touch.hover.take() {
                    self.mark(old);
                }
                if let Some((id, _)) = self.touch.hover_field.take() {
                    self.mark(id);
                }
                None
            }
            PointerKind::Down => {
                self.touch.press = target;
                self.touch.press_inside = true;
                self.touch.field = None;
                self.touch.drag = None;
                // Attention belongs to one place at a time: pressing a control
                // erases whatever another control was showing about the pointer.
                if let Some((w, _)) = self.touch.hover_field
                    && Some(w) != target
                {
                    self.touch.hover_field = None;
                    self.mark(w);
                }
                let id = target?;
                self.mark(id);
                // A press with a menu open dismisses it, and chooses only if it
                // landed on an item.
                if let Some(open) = self.touch.open.take() {
                    self.mark_list(open);
                    self.touch.press = None;
                    self.touch.item = None;
                    let chosen = self.item_at(open, p.at);
                    if let Some(n) = chosen
                        && let Some(w) = self.find_mut(open)
                        && let Kind::PopUp {
                            chosen: current, ..
                        } = &mut w.kind
                        && *current != n
                    {
                        *current = n;
                        self.mark(open);
                        return Some((open, Intent::Chose(n)));
                    }
                    return None;
                }
                // A counter is addressed field by field: the press picks one, and
                // everything after it belongs to that field alone.
                let addressed = match (self.rect(id), self.find(id)) {
                    (Some(rect), Some(widget)) => match &widget.kind {
                        Kind::Counter { field, .. } => {
                            field_at(field, rect, p.at.x, &self.skin).map(|n| (n, field[n].value))
                        }
                        _ => None,
                    },
                    _ => None,
                };
                self.touch.field = addressed.map(|(n, _)| n);
                self.touch.drag = addressed.map(|(_, v)| (p.at.y, v));

                // Pressing somewhere else commits whatever was being typed. An edit
                // abandoned by clicking away is still an edit the user made.
                let committed = match addressed {
                    Some((n, _)) => {
                        let same = self.touch.focus == Some((id, n));
                        let out = if same { None } else { self.commit() };
                        self.touch.focus = Some((id, n));
                        if !same {
                            self.touch.edit = None;
                        }
                        out
                    }
                    None => {
                        let out = self.commit();
                        self.touch.focus = None;
                        out
                    }
                };
                if committed.is_some() {
                    return committed;
                }

                // Two presses on one target, close in time and place, are one
                // gesture: return to the detent. The exhibits' answer to "how do I
                // get back to neutral", and the reason a detent is worth having.
                let double = matches!(
                    self.touch.tap,
                    Some((was, when, at)) if was == id
                        && p.time.0 - when < feel::DOUBLE
                        && (at.x - p.at.x).abs() < feel::DOUBLE_SLOP
                        && (at.y - p.at.y).abs() < feel::DOUBLE_SLOP
                );
                self.touch.tap = Some((id, p.time.0, p.at));
                if double && let Some(out) = self.recentre(id) {
                    return Some(out);
                }

                match self.find(id).map(|w| &w.kind) {
                    Some(Kind::PopUp { .. }) => {
                        self.touch.open = Some(id);
                        self.touch.item = None;
                        self.mark_list(id);
                        return None;
                    }
                    Some(Kind::Shuttle { position }) => {
                        self.touch.shuttle = Some((p.at.x, *position));
                    }
                    // A slider jumps to the pointer and tracks from there — the cap
                    // goes where you pressed, which is what "absolute" means and
                    // what every hardware fader does not do but every screen one
                    // should.
                    Some(Kind::Slider { .. }) => {
                        if let Some(out) = self.slide(id, p.at) {
                            return Some(out);
                        }
                        return Some((id, Intent::Pressed));
                    }
                    _ => {}
                }
                Some((id, Intent::Pressed))
            }
            PointerKind::Move => {
                // A pressed control tracks the pointer (Apple HIG 1992, ch. 7):
                // it stops showing as pressed when the pointer leaves it, and shows
                // again on return, so the cancel is visible before it happens.
                if let Some(pressed) = self.touch.press {
                    let inside = self.rect(pressed).is_some_and(|r| r.contains(p.at));
                    if inside != self.touch.press_inside {
                        self.touch.press_inside = inside;
                        self.mark(pressed);
                    }
                }
                // An open list tracks the pointer whether or not a button is held,
                // so both gestures work: press-drag-release, and click-then-click.
                // Which counter field is under the pointer, so the wheel target is
                // visible before anyone spins it.
                let over_field = target.and_then(|id| self.field_under(id, p.at).map(|n| (id, n)));
                if over_field != self.touch.hover_field {
                    if let Some((id, _)) = self.touch.hover_field {
                        self.mark(id);
                    }
                    self.touch.hover_field = over_field;
                    if let Some((id, _)) = over_field {
                        self.mark(id);
                    }
                }
                if let Some(open) = self.touch.open {
                    let over = self.item_at(open, p.at);
                    if over != self.touch.item {
                        self.touch.item = over;
                        self.mark_list(open);
                    }
                    return None;
                }
                let pressed = self.touch.press?;
                // The shuttle tracks the pointer across its own width, so how far
                // you have pulled it is how fast you are going.
                if let Some((start_x, start)) = self.touch.shuttle
                    && let Some(rect) = self.rect(pressed)
                {
                    let travel = (rect.w / 2.0).max(1.0);
                    let next = (start + (p.at.x - start_x) / travel).clamp(-1.0, 1.0);
                    let widget = self.find_mut(pressed)?;
                    let Kind::Shuttle { position } = &mut widget.kind else {
                        return None;
                    };
                    if (*position - next).abs() < 0.001 {
                        return None;
                    }
                    *position = next;
                    self.mark(pressed);
                    return Some((pressed, Intent::Shuttled(next)));
                }
                if matches!(
                    self.find(pressed).map(|w| &w.kind),
                    Some(Kind::Slider { .. })
                ) {
                    return self.slide(pressed, p.at);
                }
                // A drag abandons any typing: you are setting the value now, not
                // spelling it.
                self.touch.edit = None;
                // Dragging a counter field: up is more, at one step per 4 logical
                // pixels — slow enough to land on a value, quick enough to travel.
                let (start_y, start_value) = self.touch.drag?;
                let index = self.touch.field?;
                let step = ((start_y - p.at.y) / 4.0) as i64;
                let widget = self.find_mut(pressed)?;
                let Kind::Counter { field, .. } = &mut widget.kind else {
                    return None;
                };
                let target_value = start_value + step;
                let f = field.get_mut(index)?;
                let clamped = target_value.clamp(f.min, f.max);
                if clamped == f.value {
                    return None;
                }
                f.value = clamped;
                self.mark(pressed);
                Some((pressed, Intent::FieldChanged(index, clamped)))
            }
            PointerKind::Up => {
                // Releasing over an item chooses it — the drag-through gesture.
                // Releasing anywhere else leaves the menu open, so a plain click on
                // the button opens it and stays open.
                if let Some(open) = self.touch.open
                    && let Some(n) = self.item_at(open, p.at)
                {
                    self.touch.press = None;
                    self.touch.open = None;
                    self.touch.item = None;
                    self.mark_list(open);
                    let widget = self.find_mut(open)?;
                    let Kind::PopUp { chosen, .. } = &mut widget.kind else {
                        return None;
                    };
                    if *chosen == n {
                        return None;
                    }
                    *chosen = n;
                    self.mark(open);
                    return Some((open, Intent::Chose(n)));
                }
                let pressed = self.touch.press.take()?;
                self.touch.drag = None;
                // Let go of the shuttle and it springs home — and says so, because
                // otherwise nothing downstream learns that scrubbing has stopped.
                if self.touch.shuttle.take().is_some()
                    && let Some(w) = self.find_mut(pressed)
                    && let Kind::Shuttle { position } = &mut w.kind
                {
                    *position = 0.0;
                    self.mark(pressed);
                    return Some((pressed, Intent::Shuttled(0.0)));
                }
                self.mark(pressed);
                // A release only acts if it lands on the widget that was pressed —
                // dragging off and letting go cancels, which is what every control
                // on every platform has always done.
                if target != Some(pressed) {
                    return Some((pressed, Intent::Cancelled));
                }
                let widget = self.find_mut(pressed)?;
                match &mut widget.kind {
                    Kind::Toggle { on } => {
                        *on = !*on;
                        let now = *on;
                        Some((pressed, Intent::Toggled(now)))
                    }
                    // The kit reports that record was pressed and in what state it
                    // was pressed; **it does not decide what happens next**. Whether
                    // a press arms, disarms, or stops is a transport question, and
                    // the transport is not here (R-901).
                    Kind::Record { mode } => Some((pressed, Intent::RecordPressed(*mode))),
                    // A locator that holds a position recalls it; an empty one asks
                    // to be filled. Set on the fly, during playback, exactly as the
                    // Control Bar always did.
                    Kind::Locator { index, at } => {
                        let index = *index;
                        if at.is_some() {
                            Some((pressed, Intent::Recalled(index)))
                        } else {
                            Some((pressed, Intent::Store(index)))
                        }
                    }
                    _ => Some((pressed, Intent::Released)),
                }
            }
            _ => None,
        }
    }

    /// Set a toggle's state from the application — the other direction, for when
    /// the model changes rather than the pointer.
    pub fn set_toggle(&mut self, id: WidgetId, on: bool) {
        if let Some(w) = self.find_mut(id)
            && let Kind::Toggle { on: current } = &mut w.kind
            && *current != on
        {
            *current = on;
            self.mark(id);
        }
    }

    pub fn set_readout(&mut self, id: WidgetId, value: impl Into<String>) {
        let value = value.into();
        if let Some(w) = self.find_mut(id)
            && let Kind::Readout { value: current } = &mut w.kind
            && *current != value
        {
            *current = value;
            self.mark(id);
        }
    }

    /// Set the record control's state — the application's answer to
    /// [`Intent::RecordPressed`].
    pub fn set_record(&mut self, id: WidgetId, to: RecordMode) {
        if let Some(w) = self.find_mut(id)
            && let Kind::Record { mode } = &mut w.kind
            && *mode != to
        {
            *mode = to;
            self.mark(id);
        }
    }

    /// Clear every locator — for a "clear all" gesture.
    pub fn clear_locator(&mut self, id: WidgetId) {
        self.set_locator(id, None);
    }

    /// Fill or clear a locator.
    pub fn set_locator(&mut self, id: WidgetId, at: Option<String>) {
        if let Some(w) = self.find_mut(id)
            && let Kind::Locator { at: current, .. } = &mut w.kind
            && *current != at
        {
            *current = at;
            self.mark(id);
        }
    }

    /// Set a counter field from the model — the other direction, and the one that
    /// runs on every frame of playback.
    pub fn set_field(&mut self, id: WidgetId, index: usize, value: i64) {
        let mut changed = false;
        if let Some(w) = self.find_mut(id)
            && let Kind::Counter { field, .. } = &mut w.kind
            && let Some(f) = field.get_mut(index)
        {
            let clamped = value.clamp(f.min, f.max);
            changed = clamped != f.value;
            f.value = clamped;
        }
        if changed {
            self.mark(id);
        }
    }
}
