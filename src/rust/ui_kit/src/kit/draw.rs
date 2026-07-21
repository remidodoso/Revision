//! How each widget looks.
//!
//! Split from `kit.rs` when it passed the ~1000-line refactor tripwire: appearance
//! is the natural seam, because it depends on nothing but the skin, the painter and
//! the widget, while the tree, layout, hit testing and events stay next door.
//!
//! A child module, so it reaches `Kit`'s private state — which it needs, because
//! **hover and press modulate appearance without belonging to the design**. The one
//! rule this file exists to keep: intrinsic state supplies the base colour and
//! interaction state only lifts it. A control painted with a flat hover colour
//! reports the wrong state for exactly as long as the pointer rests on it.

use rev_ui_mech::{Color, Fill, Outline, Painter, Point, Rect, Shadow};

use super::{Kind, Kit, RecordMode, Widget, WidgetId, list_rect};
use crate::skin::State;

impl Kit {
    /// Paint the tree, then anything that floats above it.
    pub fn paint(&self, p: &mut Painter) {
        self.paint_widget(&self.root, p);
        // An open menu is painted last so it covers widgets drawn after its owner.
        // One overlay pass, because one thing floats; a general z-order system for
        // a single case would be machinery in search of a problem.
        if let Some(open) = self.touch.open {
            self.paint_list(open, p);
        }
    }

    fn paint_list(&self, id: WidgetId, p: &mut Painter) {
        let (Some(rect), Some(widget)) = (self.rect(id), self.find(id)) else {
            return;
        };
        let Kind::PopUp { option, chosen } = &widget.kind else {
            return;
        };
        let s = &self.skin;
        let list = list_rect(rect, option.len());
        p.shadow_round_rect(
            list,
            s.metric.control_radius,
            &Shadow::outer(Point::new(0.0, 3.0), 8.0, Color::rgba(0, 0, 0, 170)),
        );
        p.fill_round_rect(list, s.metric.control_radius, s.panel_hi);
        for (n, text) in option.iter().enumerate() {
            let item = Rect::new(list.x, list.y + n as f32 * rect.h, list.w, rect.h);
            // **Exactly one bar.** While the pointer is in the list the question is
            // "what am I about to pick", so the highlight follows the pointer and
            // the current choice steps back to an amber label. Two bars would ask
            // the reader to work out which one means what.
            let hovered = self.hovered_item();
            let bar = hovered.unwrap_or(*chosen) == n;
            if bar {
                p.fill_rect(item, s.arc);
            }
            let shaped = p.shape(text, &s.label_style());
            let ink = if bar {
                s.band_ink()
            } else if n == *chosen {
                s.readout
            } else {
                s.ink
            };
            p.draw_text(
                &shaped,
                Point::new(item.x + 8.0, item.y + (item.h - shaped.size().h) / 2.0),
                ink,
            );
        }
    }

    fn paint_widget(&self, w: &Widget, p: &mut Painter) {
        let Some(rect) = self.rect(w.id) else {
            return;
        };
        // A widget outside the dirty region costs nothing but a comparison.
        if !rect.intersect(p.clip()).empty() {
            self.draw(w, rect, p);
        }
        for c in &w.child {
            self.paint_widget(c, p);
        }
    }

    /// One widget's appearance.
    ///
    /// **Intrinsic state supplies the base; interaction only modulates it.** A
    /// control painted with a flat hover colour reports the wrong state for exactly
    /// as long as the pointer rests on it — which is the moment it matters most.
    fn draw(&self, w: &Widget, rect: Rect, p: &mut Painter) {
        let s = &self.skin;
        let hovered = self.touch.hover == Some(w.id);
        // Pressed *and* still under the pointer — a control that stayed lit while
        // you dragged off it would promise an action it is not going to perform.
        let pressed = self.press_shown(w.id);

        let modulate = |base: Color| {
            if !w.operable() {
                base
            } else if pressed {
                s.lift(base, 52)
            } else if hovered {
                s.lift(base, 22)
            } else {
                base
            }
        };

        match &w.kind {
            Kind::Panel => {
                p.shadow_round_rect(
                    rect,
                    s.metric.panel_radius,
                    &Shadow::outer(Point::new(0.0, 4.0), 10.0, Color::rgba(0, 0, 0, 130)),
                );
                p.fill_round_rect(rect, s.metric.panel_radius, s.panel);
                // The bevel: a hairline, not a blur.
                p.fill_rect(
                    Rect::new(
                        rect.x + s.metric.panel_radius,
                        rect.y,
                        rect.w - s.metric.panel_radius * 2.0,
                        1.0,
                    ),
                    s.panel_hi,
                );
            }
            Kind::Rule => {
                // A hairline, snapped — the thing pixel snapping exists for.
                p.fill_rect(rect, s.frame);
            }
            Kind::Label => {
                let shaped = p.shape(&w.label, &s.label_style());
                let ink = if w.inert { s.dim(s.ink_dim) } else { s.ink_dim };
                p.draw_text(&shaped, Point::new(rect.x, rect.y), ink);
            }
            Kind::Button | Kind::Toggle { .. } => {
                let latched = matches!(w.kind, Kind::Toggle { on: true });
                let base = if w.inert {
                    s.dim(s.state(State::Idle))
                } else if latched {
                    s.state(if w.state == State::Idle {
                        State::Active
                    } else {
                        w.state
                    })
                } else {
                    s.state(State::Idle)
                };
                let face = modulate(base);
                p.shadow_round_rect(
                    rect,
                    s.metric.control_radius,
                    &Shadow::outer(Point::new(0.0, 1.0), 2.0, Color::rgba(0, 0, 0, 120)),
                );
                p.fill_round_rect(
                    rect,
                    s.metric.control_radius,
                    Fill::vertical(0.0, rect.h, vec![(0.0, s.lift(face, 14)), (1.0, face)]),
                );
                let shaped = p.shape(&w.label, &s.label_style());
                let ink = if w.inert { s.dim(s.ink) } else { s.ink };
                p.draw_text(
                    &shaped,
                    Point::new(
                        rect.x + (rect.w - shaped.size().w) / 2.0,
                        rect.y + (rect.h - shaped.size().h) / 2.0,
                    ),
                    ink,
                );
            }
            Kind::Lamp { lit } => {
                let d = s.metric.lamp_diameter;
                let body = Rect::new(rect.x, rect.y, d, d);
                if *lit {
                    // Lights glow, text never — a standing law of the exhibits.
                    p.shadow_round_rect(
                        body,
                        d / 2.0,
                        &Shadow::outer(Point::new(0.0, 0.0), 5.0, s.accent),
                    );
                    p.fill_round_rect(body, d / 2.0, s.accent);
                } else {
                    p.fill_round_rect(body, d / 2.0, s.slot);
                    p.shadow_round_rect(
                        body,
                        d / 2.0,
                        &Shadow::inset(Point::new(0.0, 1.0), 1.5, Color::rgba(0, 0, 0, 200)),
                    );
                }
            }
            Kind::Readout { value } => {
                p.fill_round_rect(rect, 2.0, s.slot);
                p.shadow_round_rect(
                    rect,
                    2.0,
                    &Shadow::inset(Point::new(0.0, 1.0), 2.0, Color::rgba(0, 0, 0, 190)),
                );
                let shaped = p.shape(value, &s.readout_style());
                let ink = if w.inert { s.dim(s.readout) } else { s.readout };
                p.draw_text(
                    &shaped,
                    Point::new(
                        rect.right() - shaped.size().w - 6.0,
                        rect.y + (rect.h - shaped.size().h) / 2.0,
                    ),
                    ink,
                );
            }
            Kind::Record { mode } => {
                // Three intrinsic states, and interaction only modulates them.
                // Armed blinks; the phase is the kit's, so every armed control in
                // the application pulses together.
                let lit = match mode {
                    RecordMode::Off => false,
                    RecordMode::Armed => self.blink,
                    RecordMode::Recording => true,
                };
                let base = match mode {
                    RecordMode::Off => s.state(State::Idle),
                    RecordMode::Armed | RecordMode::Recording if lit => s.accent,
                    _ => s.state(State::Idle),
                };
                let face = if w.inert { s.dim(base) } else { modulate(base) };
                p.shadow_round_rect(
                    rect,
                    s.metric.control_radius,
                    &Shadow::outer(Point::new(0.0, 1.0), 2.0, Color::rgba(0, 0, 0, 120)),
                );
                p.fill_round_rect(
                    rect,
                    s.metric.control_radius,
                    Fill::vertical(0.0, rect.h, vec![(0.0, s.lift(face, 14)), (1.0, face)]),
                );
                // The dot: solid red when the button itself is dark, dark when the
                // button is lit, so the control reads at a glance either way.
                let d = (rect.h * 0.42).min(rect.w * 0.42);
                let dot = Rect::new(
                    rect.x + (rect.w - d) / 2.0,
                    rect.y + (rect.h - d) / 2.0,
                    d,
                    d,
                );
                let ink = if lit { s.panel_lo } else { s.accent };
                if *mode == RecordMode::Recording {
                    p.shadow_round_rect(
                        rect,
                        s.metric.control_radius,
                        &Shadow::outer(Point::new(0.0, 0.0), 6.0, s.accent),
                    );
                }
                p.fill_round_rect(dot, d / 2.0, if w.inert { s.dim(ink) } else { ink });
            }
            Kind::Locator { index, at } => {
                // Grey until it holds a position — the bank shows at a glance which
                // slots are in use, which is the whole reason it is a bank.
                let base = if at.is_some() {
                    s.arc
                } else {
                    s.state(State::Idle)
                };
                let face = if w.inert { s.dim(base) } else { modulate(base) };
                p.fill_round_rect(rect, 2.0, face);
                // Read, not scanned — so the label size, not the band size. Band
                // labels are the only sanctioned exception to the legibility floor.
                let shaped = p.shape(&index.to_string(), &s.label_style());
                let ink = if at.is_some() {
                    s.band_ink()
                } else {
                    s.ink_dim
                };
                p.draw_text(
                    &shaped,
                    Point::new(
                        rect.x + (rect.w - shaped.size().w) / 2.0,
                        rect.y + (rect.h - shaped.size().h) / 2.0,
                    ),
                    ink,
                );
            }
            Kind::Counter { field, separator } => {
                p.fill_round_rect(rect, 2.0, s.slot);
                p.shadow_round_rect(
                    rect,
                    2.0,
                    &Shadow::inset(Point::new(0.0, 1.0), 2.0, Color::rgba(0, 0, 0, 190)),
                );
                let style = s.readout_style();
                let mut x = rect.x + 8.0;
                let y = rect.y + (rect.h - s.kind.readout * 1.3) / 2.0;
                for (n, f) in field.iter().enumerate() {
                    let (text, typing) = self.field_text(w.id, n, f);
                    let shaped = p.shape(&text, &style);
                    // The addressed field is marked by its ground, not by its ink:
                    // a value that changes colour when you touch it is harder to
                    // read at the moment you are reading it most carefully.
                    // The addressed field is marked by its ground, not by its ink:
                    // a value that changes colour when you touch it is harder to
                    // read at the moment you are reading it most carefully. It has
                    // to be *visible* against the slot, though — a selection nobody
                    // can see is the same as no selection at all.
                    // The field under the pointer, marked faintly: this is what the
                    // wheel will act on, and a control whose target is invisible is
                    // one you have to experiment with to use.
                    if self.hovered_field(w.id) == Some(n) && !self.field_focused(w.id, n) {
                        p.fill_round_rect(
                            Rect::new(x - 4.0, rect.y + 3.0, shaped.size().w + 8.0, rect.h - 6.0),
                            2.0,
                            s.panel_hi,
                        );
                    }
                    if self.field_focused(w.id, n) {
                        // The exhibits' idiom for an editable window under the
                        // pointer: an amber outline, not a swapped ground. Amber
                        // digits on near-black stay the most legible thing in the
                        // panel, which is what a counter is for — and a selection
                        // nobody can see is the same as no selection at all.
                        let ground =
                            Rect::new(x - 4.0, rect.y + 3.0, shaped.size().w + 8.0, rect.h - 6.0);
                        p.fill_round_rect(ground, 2.0, s.readout);
                        p.fill_round_rect(
                            Rect::new(
                                ground.x + 1.0,
                                ground.y + 1.0,
                                ground.w - 2.0,
                                ground.h - 2.0,
                            ),
                            2.0,
                            if typing { s.panel_lo } else { s.slot },
                        );
                    }
                    let ink = if w.inert { s.dim(s.readout) } else { s.readout };
                    p.draw_text(&shaped, Point::new(x, y), ink);
                    x += shaped.size().w;
                    if n + 1 < field.len() {
                        let sep = p.shape(&separator.to_string(), &style);
                        p.draw_text(&sep, Point::new(x, y), s.ink_dim);
                        x += sep.size().w;
                    }
                }
            }
            Kind::PopUp { option, chosen } => {
                let face = if w.inert {
                    s.dim(s.state(State::Idle))
                } else {
                    modulate(s.state(State::Idle))
                };
                p.shadow_round_rect(
                    rect,
                    s.metric.control_radius,
                    &Shadow::outer(Point::new(0.0, 1.0), 2.0, Color::rgba(0, 0, 0, 120)),
                );
                p.fill_round_rect(
                    rect,
                    s.metric.control_radius,
                    Fill::vertical(0.0, rect.h, vec![(0.0, s.lift(face, 14)), (1.0, face)]),
                );
                let text = option.get(*chosen).map_or("", String::as_str);
                let shaped = p.shape(text, &s.label_style());
                let ink = if w.inert { s.dim(s.ink) } else { s.ink };
                p.draw_text(
                    &shaped,
                    Point::new(rect.x + 8.0, rect.y + (rect.h - shaped.size().h) / 2.0),
                    ink,
                );
                // The mark that says "there is more here": a small solid triangle,
                // not a glyph, so it cannot depend on a font having one.
                let size = 5.0;
                let cx = rect.right() - 12.0;
                let cy = rect.y + rect.h / 2.0;
                let mut o = Outline::new();
                o.move_to(Point::new(cx - size, cy - size / 2.0));
                o.line_to(Point::new(cx + size, cy - size / 2.0));
                o.line_to(Point::new(cx, cy + size / 2.0));
                o.close();
                if let Some(path) = o.finish() {
                    p.fill_path(&path, if w.inert { s.dim(s.ink_dim) } else { s.ink_dim });
                }
            }
            Kind::Slider { value, detent } => {
                // The exhibits' locked slider: a narrow recessed slot, tick ladders
                // either side, and a WIDE cap — near column width, Jupiter-8 style.
                let slot_w = s.metric.slot_width;
                let slot = Rect::new(rect.x + (rect.w - slot_w) / 2.0, rect.y, slot_w, rect.h);
                p.fill_round_rect(slot, slot_w / 2.0, s.slot);
                p.shadow_round_rect(
                    slot,
                    slot_w / 2.0,
                    &Shadow::inset(Point::new(0.0, 1.0), 2.0, Color::rgba(0, 0, 0, 190)),
                );

                let cap_h = s.metric.cap_height;
                let travel = rect.h - cap_h;
                // Ticks: eleven rungs, with the detent's own tick amber — the
                // accent marks neutral without adding a second bar to read.
                for n in 0..=10 {
                    let t = n as f32 / 10.0;
                    let y = rect.y + cap_h / 2.0 + (1.0 - t) * travel;
                    let major = n % 5 == 0;
                    let at_detent = detent.is_some_and(|d| (d - t).abs() < 0.05);
                    let ink = if at_detent {
                        s.readout
                    } else if major {
                        s.tick_major
                    } else {
                        s.tick
                    };
                    let len = if major { 5.0 } else { 3.0 };
                    p.fill_rect(Rect::new(rect.x, y, len, 1.0), ink);
                    p.fill_rect(Rect::new(rect.right() - len, y, len, 1.0), ink);
                }

                let cap = Rect::new(
                    rect.x + (rect.w - s.metric.cap_width) / 2.0,
                    rect.y + (1.0 - value) * travel,
                    s.metric.cap_width,
                    cap_h,
                );
                p.shadow_round_rect(
                    cap,
                    2.0,
                    &Shadow::outer(Point::new(0.0, 2.0), 3.0, Color::rgba(0, 0, 0, 150)),
                );
                let top = if w.inert {
                    s.dim(s.cap[0])
                } else {
                    modulate(s.cap[0])
                };
                p.fill_round_rect(
                    cap,
                    2.0,
                    Fill::vertical(
                        0.0,
                        cap.h,
                        vec![(0.0, top), (0.46, s.cap[1]), (1.0, s.cap[2])],
                    ),
                );
                // The cap's centre line: what the eye actually reads the value off.
                p.fill_rect(
                    Rect::new(cap.x + 2.0, cap.y + cap.h / 2.0 - 0.75, cap.w - 4.0, 1.5),
                    if w.inert { s.dim(s.ink) } else { s.ink },
                );
            }
            Kind::Shuttle { position } => {
                // A sprung control reads as sprung: a recessed track, a marked
                // centre, and a cap that is obviously away from home when it is.
                p.fill_round_rect(rect, rect.h / 2.0, s.slot);
                p.shadow_round_rect(
                    rect,
                    rect.h / 2.0,
                    &Shadow::inset(Point::new(0.0, 1.0), 2.0, Color::rgba(0, 0, 0, 190)),
                );
                let centre = rect.x + rect.w / 2.0;
                p.fill_rect(
                    Rect::new(centre - 0.5, rect.y + 3.0, 1.0, rect.h - 6.0),
                    s.tick_major,
                );
                let cap_w = 18.0;
                let travel = (rect.w - cap_w) / 2.0;
                let cap = Rect::new(
                    centre - cap_w / 2.0 + position * travel,
                    rect.y + 2.0,
                    cap_w,
                    rect.h - 4.0,
                );
                p.shadow_round_rect(
                    cap,
                    2.0,
                    &Shadow::outer(Point::new(0.0, 1.0), 2.0, Color::rgba(0, 0, 0, 150)),
                );
                let face = if w.inert {
                    s.dim(s.cap[0])
                } else {
                    modulate(s.cap[0])
                };
                p.fill_round_rect(
                    cap,
                    2.0,
                    Fill::vertical(
                        0.0,
                        cap.h,
                        vec![(0.0, face), (0.46, s.cap[1]), (1.0, s.cap[2])],
                    ),
                );
                p.fill_rect(
                    Rect::new(cap.x + 3.0, cap.y + cap.h / 2.0 - 0.5, cap.w - 6.0, 1.0),
                    s.ink,
                );
            }
        }
    }
}
