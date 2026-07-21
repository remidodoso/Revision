//! rev-app — the composition root: wiring, command dispatch, view state.
//! No musical logic lives here or in any frontend, ever (the product-family
//! constitution). Threads: UI/main, MIDI callbacks, async store writer;
//! the RT callback belongs to rev-engine. Everything talks over rings.
//!
//! Currently a bring-up harness for ui-03: a Control Bar **described as data** and
//! rendered by `rev-ui-kit`. The transport it drives does not exist yet — ui-04
//! connects it to the engine — so intents are answered locally here, at exactly the
//! seam that will later carry commands instead.

use rev_ui_kit::{Anchor, Field, Intent, Kind, Kit, RecordMode, Skin, Widget, WidgetId};
use rev_ui_mech::{
    Event, Frame, Host, KeyCode, Mech, Named, Notice, Point, Reason, Rect, Size, TargetId,
    TextStyle, Tree, WindowId, WindowSpec,
};

const PLAY: WidgetId = WidgetId(1);
const STOP: WidgetId = WidgetId(2);
const RECORD: WidgetId = WidgetId(10);
const COUNTER: WidgetId = WidgetId(20);
const TEMPO: WidgetId = WidgetId(23);
const MODE: WidgetId = WidgetId(50);
const SHUTTLE: WidgetId = WidgetId(60);
const SEP1: u32 = 80;
const SEP2: u32 = 81;

/// The Control Bar, as a description.
///
/// Nothing here paints: a layout designer could edit this tree, a device profile
/// could generate one, a script could assemble one. Function follows Vision's
/// Control Bar — the transport cluster, a counter editable during playback,
/// locators set on the fly — and appearance follows the Notorolla exhibits
/// (`revision_poc.md`, "Two sources"). It is not a reconstruction of 1998.
fn control_bar() -> Widget {
    // One row of controls, one row of readouts. Everything is in logical pixels and
    // laid out left to right, because a transport reads left to right.
    const TOP: f32 = 14.0;
    const BOTTOM: f32 = 58.0;
    const H: f32 = 32.0;

    let mut child = Vec::new();
    let mut x = 14.0;

    // Transport cluster.
    for (id, label, w) in [(PLAY.0, "Play", 68.0), (STOP.0, "Stop", 68.0)] {
        child.push(Widget::new(
            id,
            Kind::Button,
            label,
            Rect::new(x, TOP, w, H),
        ));
        x += w + 6.0;
    }
    child.push(Widget::new(
        RECORD.0,
        Kind::Record {
            mode: RecordMode::Off,
        },
        "Record",
        Rect::new(x, TOP, 52.0, H),
    ));
    x += 52.0 + 14.0;
    child.push(Widget::new(
        SEP1,
        Kind::Rule,
        "",
        Rect::new(x, TOP + 2.0, 1.0, H - 4.0),
    ));
    x += 15.0;

    // Mode toggles — the never-stop-the-transport workflow.
    for (n, label) in ["Loop", "Punch", "Wait"].into_iter().enumerate() {
        let w = 64.0;
        child.push(Widget::new(
            3 + n as u32,
            Kind::Toggle { on: false },
            label,
            Rect::new(x, TOP, w, H),
        ));
        x += w + 6.0;
    }
    x += 8.0;
    child.push(Widget::new(
        SEP2,
        Kind::Rule,
        "",
        Rect::new(x, TOP + 2.0, 1.0, H - 4.0),
    ));
    x += 15.0;

    // Record mode, which is where the performance modes will live later.
    child.push(Widget::new(
        MODE.0,
        Kind::PopUp {
            option: vec![
                String::from("Replace"),
                String::from("Overdub"),
                String::from("Punch In"),
            ],
            chosen: 0,
        },
        "Record Mode",
        Rect::new(x, TOP, 118.0, H),
    ));

    // Counter: bar | beat | unit at 5040 ppq (R-003), editable during playback.
    child.push(Widget::new(
        COUNTER.0,
        Kind::Counter {
            field: vec![
                Field::new(1, 3, 1, 999),
                Field::new(1, 2, 1, 16),
                Field::new(0, 4, 0, 5039),
            ],
            separator: '|',
        },
        "Counter",
        Rect::new(14.0, BOTTOM, 172.0, H),
    ));
    child.push(Widget::new(
        21,
        Kind::Label,
        "bar · beat · unit",
        Rect::new(192.0, BOTTOM + 8.0, 130.0, 18.0),
    ));

    // Tempo, in the same numeric role — whole and hundredths.
    child.push(Widget::new(
        TEMPO.0,
        Kind::Counter {
            field: vec![Field::new(120, 3, 20, 400), Field::new(0, 2, 0, 99)],
            // Decimal, not a bar line: bpm is one number with a fraction.
            separator: '.',
        },
        "Tempo",
        Rect::new(318.0, BOTTOM, 108.0, H),
    ));
    child.push(Widget::new(
        22,
        Kind::Label,
        "bpm",
        Rect::new(432.0, BOTTOM + 8.0, 40.0, 18.0),
    ));

    // Shuttle: scrubs while held, springs home on release.
    child.push(Widget::new(
        SHUTTLE.0,
        Kind::Shuttle { position: 0.0 },
        "Shuttle",
        Rect::new(478.0, BOTTOM + 4.0, 132.0, 24.0),
    ));

    // Eight locators, grey until set — the bank reads at a glance.
    for n in 0..8u8 {
        child.push(Widget::new(
            40 + u32::from(n),
            Kind::Locator {
                index: n + 1,
                at: None,
            },
            format!("Locator {}", n + 1),
            Rect::new(626.0 + f32::from(n) * 28.0, BOTTOM, 24.0, H),
        ));
    }

    Widget::new(
        0,
        Kind::Panel,
        "Control Bar",
        Rect::new(0.0, 0.0, 864.0, 104.0),
    )
    .with_anchor(Anchor::FILL)
    .with_child(child)
}

struct Bringup {
    main: Option<WindowId>,
    probe: Option<WindowId>,
    kit: Kit,
    /// Stands in for the transport until ui-04 provides a real one.
    playing: bool,
    /// Beats elapsed on the stand-in transport.
    beat: i64,
    /// Seconds at which the stand-in transport last advanced the counter.
    advanced: f64,
}

impl Default for Bringup {
    fn default() -> Bringup {
        Bringup {
            main: None,
            probe: None,
            kit: Kit::new(control_bar(), Skin::default()),
            playing: false,
            beat: 0,
            advanced: 0.0,
        }
    }
}

impl Bringup {
    /// Push whatever the kit marked dirty into the window that owns it.
    fn flush(&mut self, window: WindowId, mech: &mut Mech) {
        for rect in self.kit.take_dirty() {
            mech.mark_dirty(window, rect);
        }
    }

    /// Give the mechanism whatever the kit asked to focus.
    ///
    /// The kit never touches the mechanism — it reports what it wants and the
    /// application decides. Focus moves because a widget asked for it, which is the
    /// only way it is permitted to move (R-907).
    fn sync_focus(&mut self, window: WindowId, mech: &mut Mech) {
        let want = self.kit.editing().map(|t| (window, t));
        if want != mech.focus() {
            mech.set_focus(want, Reason::User);
        }
    }

    /// The transport's answer to a record press — the decision the kit deliberately
    /// does not make. Stopped, it arms; playing, it records; either way, again stops.
    fn record_pressed(&mut self, was: RecordMode) {
        let next = match (was, self.playing) {
            (RecordMode::Off, false) => RecordMode::Armed,
            (RecordMode::Off, true) => RecordMode::Recording,
            (RecordMode::Armed | RecordMode::Recording, _) => RecordMode::Off,
        };
        self.kit.set_record(RECORD, next);
        println!("record: {was:?} -> {next:?}");
    }
}

impl Host for Bringup {
    fn start(&mut self, mech: &mut Mech) {
        self.main = Some(mech.open_window(WindowSpec {
            title: String::from("Revision — Control Bar"),
            size: Size::new(896.0, 136.0),
            ..WindowSpec::default()
        }));
    }

    fn notice(&mut self, window: WindowId, notice: &Notice, mech: &mut Mech) {
        match notice {
            Notice::CloseRequested => {
                if self.probe == Some(window) {
                    self.probe = None;
                    mech.close_window(window);
                    return;
                }
                mech.close_window(window);
                mech.exit();
            }
            Notice::Resized(size) => {
                if self.probe != Some(window) {
                    // Layout is data: resizing re-resolves it rather than
                    // re-describing it.
                    self.kit
                        .layout(Rect::new(16.0, 16.0, size.w - 32.0, size.h - 32.0));
                }
                mech.mark_dirty_all(window);
            }
            Notice::ScaleChanged(scale) => {
                println!("platform scale: {scale}");
                mech.mark_dirty_all(window);
            }
            Notice::FocusChanged(_) => {}
        }
    }

    fn hit(&self, window: WindowId, at: Point) -> Option<TargetId> {
        if self.probe == Some(window) {
            return None;
        }
        self.kit.hit(at)
    }

    fn a11y(&self, window: WindowId) -> Tree {
        if self.probe == Some(window) {
            return Tree::default();
        }
        self.kit.a11y()
    }

    fn tick(&mut self, mech: &mut Mech) {
        let Some(window) = self.main else { return };
        let now = mech.now().0;
        if self.kit.animate(now) {
            self.flush(window, mech);
        }
        // A stand-in transport at 120 bpm, so the counter moves and the readout can
        // be watched doing it. ui-04 replaces this with the engine's clock.
        if self.playing && now - self.advanced >= 0.5 {
            self.advanced = now;
            self.beat += 1;
            let bar = 1 + self.beat / 4;
            let beat = 1 + self.beat % 4;
            self.kit.set_field(COUNTER, 0, bar);
            self.kit.set_field(COUNTER, 1, beat);
            self.flush(window, mech);
        }
        // Ask to be woken only while something is actually moving. An application
        // that always asks has a busy loop with extra steps.
        if self.kit.animating() || self.playing {
            mech.wake_after(0.05);
        }
    }

    fn event(&mut self, window: WindowId, target: Option<TargetId>, ev: &Event, mech: &mut Mech) {
        if self.probe == Some(window) {
            return;
        }
        // While a field is being edited it owns the keyboard: the application's
        // own shortcuts must not eat digits, and '0' resetting the interface scale
        // mid-edit would be a memorable bug.
        if self.kit.editing().is_some() && matches!(ev, Event::Key(_) | Event::Text(_)) {
            let out = self.kit.event(target, ev);
            self.sync_focus(window, mech);
            self.flush(window, mech);
            if let Some((id, Intent::FieldChanged(field, value))) = out {
                println!("{id:?} field {field} = {value}");
            }
            return;
        }

        // Interface scale and the probe window stay on the keyboard until there are
        // controls for them.
        if let Event::Key(k) = ev
            && k.pressed
        {
            match &k.code {
                KeyCode::Char('=') | KeyCode::Char('+') => {
                    let next = mech.ui_scale() + 0.25;
                    mech.set_ui_scale(next);
                    println!("interface scale: {:.2}", mech.ui_scale());
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    let next = mech.ui_scale() - 0.25;
                    mech.set_ui_scale(next);
                    println!("interface scale: {:.2}", mech.ui_scale());
                }
                KeyCode::Char('0') => mech.set_ui_scale(1.0),
                KeyCode::Char('n') => match self.probe.take() {
                    Some(id) => mech.close_window(id),
                    None => {
                        self.probe = Some(mech.open_window(WindowSpec {
                            title: String::from("Revision — probe"),
                            size: Size::new(420.0, 300.0),
                            scale: Some(1.5),
                            ..WindowSpec::default()
                        }));
                    }
                },
                KeyCode::Named(Named::Escape) => mech.set_focus(None, Reason::User),
                _ => {}
            }
            return;
        }

        let intent = self.kit.event(target, ev);
        // The kit reports the shape it wants; the application asks for it. Same
        // handshake as focus — the kit never touches the mechanism.
        mech.request_cursor(self.kit.cursor());
        self.sync_focus(window, mech);
        let Some((id, intent)) = intent else {
            self.flush(window, mech);
            return;
        };
        // Whether a modifier was held is the application's to interpret; the kit
        // reports what happened, not what it meant.
        let shifted = matches!(ev, Event::Pointer(p) if p.modifier.shift);
        match intent {
            Intent::Toggled(on) if id == PLAY => {
                self.playing = on;
                // Arming and then starting playback is what recording *is*.
                if on && self.kit.record_mode(RECORD) == Some(RecordMode::Armed) {
                    self.kit.set_record(RECORD, RecordMode::Recording);
                } else if !on {
                    self.kit.set_record(RECORD, RecordMode::Off);
                }
                println!("play: {on}");
            }
            Intent::Released if id == STOP => {
                self.playing = false;
                self.beat = 0;
                self.kit.set_toggle(PLAY, false);
                self.kit.set_record(RECORD, RecordMode::Off);
                self.kit.set_field(COUNTER, 0, 1);
                self.kit.set_field(COUNTER, 1, 1);
                self.kit.set_field(COUNTER, 2, 0);
                println!("stop");
            }
            Intent::RecordPressed(was) => self.record_pressed(was),
            // An empty locator stores the counter's current reading — set on the
            // fly, exactly as the Control Bar always did.
            Intent::Store(n) => {
                let at = self.kit.counter_text(COUNTER).unwrap_or_default();
                self.kit.set_locator(id, Some(at.clone()));
                println!("locator {n} <- {at}");
            }
            // Shift-click clears: a locator you cannot empty is a locator you get
            // exactly one chance to place.
            Intent::Recalled(n) if shifted => {
                self.kit.clear_locator(id);
                println!("locator {n} cleared");
            }
            Intent::Recalled(n) => {
                let at = self.kit.locator_text(id).unwrap_or_default();
                println!("locator {n} recalled: {at}");
            }
            Intent::Toggled(on) => println!("{id:?} toggled {on}"),
            Intent::Cancelled => {}
            Intent::FieldChanged(field, value) => println!("counter field {field} = {value}"),
            Intent::ValueChanged(v) => println!("{id:?} = {v:.3}"),
            // Where the transport would scrub; ui-04 makes it real. Zero is the
            // spring returning home, which is news too — it means stop scrubbing.
            Intent::Shuttled(v) if v != 0.0 => println!("scrub {v:+.2}"),
            Intent::Shuttled(_) => println!("scrub end"),
            Intent::Chose(n) => println!("record mode -> {n}"),
            // Confirm the click landed, so the console agrees with the screen.
            Intent::Pressed if id == COUNTER => println!("counter: editing a field"),
            _ => {}
        }
        self.flush(window, mech);
    }

    fn paint(&mut self, window: WindowId, frame: &mut Frame<'_>) {
        if self.probe == Some(window) {
            let size = frame.size;
            let (panel, ink, readout) = {
                let s = self.kit.skin();
                (s.panel, s.ink, s.readout)
            };
            let p = &mut frame.paint;
            p.clear(panel);
            let label = p.shape("probe window", &TextStyle::ui(18.0).bold());
            p.draw_text(&label, Point::new(28.0, 28.0), ink);
            let detail = p.shape(
                &format!("{:.0} x {:.0} at 1.5x window scale", size.w, size.h),
                &TextStyle::numeric(14.0),
            );
            p.draw_text(&detail, Point::new(28.0, 60.0), readout);
            return;
        }
        let background = self.kit.skin().panel_lo;
        frame.paint.clear(background);
        self.kit.paint(&mut frame.paint);
    }
}

fn main() -> Result<(), rev_ui_mech::MechError> {
    rev_ui_mech::run(Bringup::default())
}

#[cfg(test)]
mod test {
    use super::control_bar;
    use rev_ui_kit::{Kit, Skin};
    use rev_ui_mech::{Canvas, Rect};

    /// Render the Control Bar exactly as the application builds it, at the size the
    /// window opens to. Not a golden master — a look at what the user is shown,
    /// which is the only way to check a layout that reasoning says is fine.
    #[test]
    fn the_control_bar_as_shipped() {
        let mut kit = Kit::new(control_bar(), Skin::default());
        kit.layout(Rect::new(16.0, 16.0, 896.0 - 32.0, 136.0 - 32.0));
        let mut canvas = Canvas::new(896, 136, 1.0).unwrap();
        canvas.paint(|p| {
            p.clear(kit.skin().panel_lo);
            kit.paint(p);
        });
        // Into target/, which is ignored: this is a look, not an artifact.
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../target/control_bar.png");
        std::fs::write(&out, canvas.png().unwrap()).unwrap();
        println!("wrote {}", out.display());
    }
}
