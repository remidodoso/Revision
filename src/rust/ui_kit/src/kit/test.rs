use super::*;
use crate::kit::{Field, RecordMode};
use rev_ui_mech::{Button, Event, KeyCode, Modifier, Named, Pointer, PointerKind, UiTime};

fn press(kind: PointerKind, at: Point) -> Event {
    Event::Pointer(Pointer {
        kind,
        at,
        button: Some(Button::Left),
        modifier: Modifier::default(),
        time: UiTime(0.0),
    })
}

/// A miniature Control Bar: a panel, two controls, a lamp, a readout.
fn bar() -> Kit {
    let root = Widget::new(
        0,
        Kind::Panel,
        "Control Bar",
        Rect::new(0.0, 0.0, 400.0, 80.0),
    )
    .with_anchor(Anchor::FILL)
    .with_child(vec![
        Widget::new(1, Kind::Button, "Play", Rect::new(12.0, 12.0, 70.0, 28.0)),
        Widget::new(
            2,
            Kind::Toggle { on: false },
            "Loop",
            Rect::new(90.0, 12.0, 70.0, 28.0),
        ),
        Widget::new(
            3,
            Kind::Lamp { lit: false },
            "Record",
            Rect::new(170.0, 20.0, 10.0, 10.0),
        ),
        Widget::new(
            4,
            Kind::Readout {
                value: String::from("001|01|0000"),
            },
            "Counter",
            Rect::new(200.0, 12.0, 160.0, 28.0),
        )
        .with_anchor(Anchor::TOP_WIDE),
    ]);
    Kit::new(root, Skin::default())
}

#[test]
fn layout_resolves_absolute_rects() {
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 400.0, 80.0));
    // A child's rect is relative to its parent, and the root takes the window.
    assert_eq!(
        kit.rect(WidgetId(0)),
        Some(Rect::new(0.0, 0.0, 400.0, 80.0))
    );
    assert_eq!(
        kit.rect(WidgetId(1)),
        Some(Rect::new(12.0, 12.0, 70.0, 28.0))
    );
}

#[test]
fn anchors_stretch_with_the_parent() {
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 500.0, 80.0));
    // Unanchored on the right: keeps its designed width and position.
    assert_eq!(kit.rect(WidgetId(1)).unwrap().w, 70.0);
    // Anchored both sides: grows with the window.
    assert_eq!(kit.rect(WidgetId(4)).unwrap().w, 260.0);
}

#[test]
fn hit_testing_ignores_what_cannot_be_operated() {
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 400.0, 80.0));
    assert_eq!(kit.hit(Point::new(20.0, 20.0)), Some(TargetId(1)));
    assert_eq!(kit.hit(Point::new(100.0, 20.0)), Some(TargetId(2)));
    // A lamp reports; it does not accept input.
    assert_eq!(kit.hit(Point::new(174.0, 24.0)), None);
    // Neither does the panel behind everything.
    assert_eq!(kit.hit(Point::new(380.0, 70.0)), None);
}

#[test]
fn an_inert_control_is_not_operable_but_is_still_there() {
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 400.0, 80.0));
    kit.find_mut(WidgetId(1)).unwrap().inert = true;
    assert_eq!(
        kit.hit(Point::new(20.0, 20.0)),
        None,
        "inert control took input"
    );
    // ...but it is still announced, with its label intact.
    let tree = kit.a11y();
    let root = tree.root.as_ref().unwrap();
    assert_eq!(
        root.find(TargetId(1)).map(|n| n.label.as_str()),
        Some("Play")
    );
}

#[test]
fn a_toggle_latches_on_release() {
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 400.0, 80.0));
    let at = Point::new(100.0, 20.0);
    assert_eq!(
        kit.event(Some(TargetId(2)), &press(PointerKind::Down, at)),
        Some((WidgetId(2), Intent::Pressed))
    );
    assert_eq!(
        kit.event(Some(TargetId(2)), &press(PointerKind::Up, at)),
        Some((WidgetId(2), Intent::Toggled(true)))
    );
    assert_eq!(
        kit.find(WidgetId(2)).unwrap().kind,
        Kind::Toggle { on: true }
    );
}

#[test]
fn releasing_off_the_pressed_widget_cancels() {
    // Drag off and let go: every control on every platform has always done this.
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 400.0, 80.0));
    kit.event(
        Some(TargetId(2)),
        &press(PointerKind::Down, Point::new(100.0, 20.0)),
    );
    let out = kit.event(None, &press(PointerKind::Up, Point::new(300.0, 300.0)));
    assert_eq!(
        out,
        Some((WidgetId(2), Intent::Cancelled)),
        "an abandoned press reported as a release would fire the action anyway"
    );
    assert_eq!(
        kit.find(WidgetId(2)).unwrap().kind,
        Kind::Toggle { on: false },
        "toggle latched despite the release landing elsewhere"
    );
}

#[test]
fn interaction_never_touches_the_design() {
    // Hover and press are ephemeral; a layout designer edits the tree and must
    // never see them. The design must be byte-identical before and after a hover.
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 400.0, 80.0));
    let before = kit.root.clone();
    kit.event(
        Some(TargetId(1)),
        &press(PointerKind::Enter, Point::new(20.0, 20.0)),
    );
    kit.event(
        Some(TargetId(1)),
        &press(PointerKind::Down, Point::new(20.0, 20.0)),
    );
    assert_eq!(kit.root, before, "interaction state leaked into the design");
}

#[test]
fn only_what_changed_is_marked_dirty() {
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 400.0, 80.0));
    kit.take_dirty();
    kit.event(
        Some(TargetId(1)),
        &press(PointerKind::Enter, Point::new(20.0, 20.0)),
    );
    let dirty = kit.take_dirty();
    assert_eq!(dirty, vec![Rect::new(12.0, 12.0, 70.0, 28.0)]);
}

#[test]
fn setting_the_same_value_marks_nothing() {
    // The model tells the kit what it already knows on most frames; repainting for
    // that would defeat dirty tracking entirely.
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 400.0, 80.0));
    kit.take_dirty();
    kit.set_readout(WidgetId(4), "001|01|0000");
    assert!(kit.take_dirty().is_empty(), "unchanged value marked dirty");
    kit.set_readout(WidgetId(4), "001|02|0000");
    assert_eq!(kit.take_dirty().len(), 1);
}

#[test]
fn every_widget_answers_the_accessibility_contract() {
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 400.0, 80.0));
    let tree = kit.a11y();
    let node = tree.node();
    assert_eq!(node.len(), 5, "not every widget reached the tree");
    for n in &node {
        assert!(!n.label.is_empty(), "widget {:?} has no name", n.id);
        assert!(!n.bounds.empty(), "widget {:?} has no bounds", n.id);
    }
    // The readout states its value as text, not as pixels.
    let root = tree.root.as_ref().unwrap();
    assert_eq!(
        root.find(TargetId(4))
            .and_then(|n| n.value.clone())
            .as_deref(),
        Some("001|01|0000")
    );
    // The toggle states its condition.
    assert_eq!(root.find(TargetId(2)).and_then(|n| n.on), Some(false));
}

/// The transport census: record, locators, a counter.
fn transport() -> Kit {
    let root = Widget::new(
        0,
        Kind::Panel,
        "Transport",
        Rect::new(0.0, 0.0, 400.0, 120.0),
    )
    .with_child(vec![
        Widget::new(
            10,
            Kind::Record {
                mode: RecordMode::Off,
            },
            "Record",
            Rect::new(10.0, 10.0, 40.0, 30.0),
        ),
        Widget::new(
            20,
            Kind::Counter {
                field: vec![
                    Field::new(12, 3, 1, 999),
                    Field::new(3, 2, 1, 16),
                    Field::new(0, 4, 0, 5039),
                ],
                separator: '|',
            },
            "Counter",
            Rect::new(10.0, 50.0, 200.0, 30.0),
        ),
        Widget::new(
            30,
            Kind::Locator { index: 1, at: None },
            "Locator 1",
            Rect::new(10.0, 90.0, 22.0, 22.0),
        ),
        Widget::new(
            31,
            Kind::Locator {
                index: 2,
                at: Some(String::from("017|01|0000")),
            },
            "Locator 2",
            Rect::new(36.0, 90.0, 22.0, 22.0),
        ),
    ]);
    let mut kit = Kit::new(root, Skin::default());
    kit.layout(Rect::new(0.0, 0.0, 400.0, 120.0));
    kit
}

#[test]
fn record_reports_the_state_it_was_pressed_in() {
    // The kit says what happened; the transport decides what it means. Arming,
    // disarming and stopping are not the widget's business (R-901).
    let mut kit = transport();
    let at = Point::new(20.0, 20.0);
    kit.event(Some(TargetId(10)), &press(PointerKind::Down, at));
    assert_eq!(
        kit.event(Some(TargetId(10)), &press(PointerKind::Up, at)),
        Some((WidgetId(10), Intent::RecordPressed(RecordMode::Off)))
    );
    // The widget did not change itself — the application answers.
    assert_eq!(
        kit.find(WidgetId(10)).unwrap().kind,
        Kind::Record {
            mode: RecordMode::Off
        }
    );
    kit.set_record(WidgetId(10), RecordMode::Armed);
    kit.event(Some(TargetId(10)), &press(PointerKind::Down, at));
    assert_eq!(
        kit.event(Some(TargetId(10)), &press(PointerKind::Up, at)),
        Some((WidgetId(10), Intent::RecordPressed(RecordMode::Armed)))
    );
}

#[test]
fn armed_blinks_and_nothing_else_does() {
    let mut kit = transport();
    kit.take_dirty();
    // Nothing armed: time passes and no frame is owed.
    assert!(!kit.animate(0.6), "an idle transport asked for a repaint");
    kit.set_record(WidgetId(10), RecordMode::Armed);
    kit.take_dirty();
    // The phase is global and derived from the clock, so it advances whether or
    // not anything is armed; what changes is whether a frame is owed for it.
    assert!(kit.animate(1.1), "armed record did not blink");
    assert_eq!(kit.take_dirty(), vec![Rect::new(10.0, 10.0, 40.0, 30.0)]);
    // Within the same phase, nothing further is owed.
    assert!(!kit.animate(1.3));
}

#[test]
fn a_counter_is_addressed_field_by_field() {
    let mut kit = transport();
    // Fields are laid out left to right; a click lands in the one under it.
    let bar = kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(20.0, 60.0)),
    );
    assert_eq!(bar, Some((WidgetId(20), Intent::Pressed)));
    // Drag up by 16 logical pixels: four steps, on the field that was pressed.
    let moved = kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Move, Point::new(20.0, 44.0)),
    );
    assert_eq!(moved, Some((WidgetId(20), Intent::FieldChanged(0, 16))));
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Up, Point::new(20.0, 44.0)),
    );
}

#[test]
fn a_counter_field_cannot_be_dragged_out_of_range() {
    // Beats run 1..16; dragging hard must stop, not wrap, and not report a change
    // it did not make.
    let mut kit = transport();
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(20.0, 60.0)),
    );
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Move, Point::new(20.0, -4000.0)),
    );
    let out = kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Move, Point::new(20.0, -8000.0)),
    );
    assert_eq!(out, None, "reported a change at the limit");
    let Kind::Counter { field, .. } = &kit.find(WidgetId(20)).unwrap().kind else {
        panic!("not a counter");
    };
    assert_eq!(field[0].value, 999);
}

#[test]
fn a_counter_field_is_zero_padded() {
    // Fixed width is the point: digits must not shift as values change.
    let kit = transport();
    let Kind::Counter { field, .. } = &kit.find(WidgetId(20)).unwrap().kind else {
        panic!("not a counter");
    };
    assert_eq!(field[0].text(), "012");
    assert_eq!(field[2].text(), "0000");
}

#[test]
fn an_empty_locator_asks_to_be_filled_and_a_full_one_recalls() {
    let mut kit = transport();
    let empty = Point::new(20.0, 100.0);
    kit.event(Some(TargetId(30)), &press(PointerKind::Down, empty));
    assert_eq!(
        kit.event(Some(TargetId(30)), &press(PointerKind::Up, empty)),
        Some((WidgetId(30), Intent::Store(1)))
    );
    let full = Point::new(46.0, 100.0);
    kit.event(Some(TargetId(31)), &press(PointerKind::Down, full));
    assert_eq!(
        kit.event(Some(TargetId(31)), &press(PointerKind::Up, full)),
        Some((WidgetId(31), Intent::Recalled(2)))
    );
}

#[test]
fn the_census_answers_the_accessibility_contract() {
    // Every new kind states what it is and what it holds — as text, not pixels.
    let kit = transport();
    let tree = kit.a11y();
    let root = tree.root.as_ref().unwrap();
    assert_eq!(
        root.find(TargetId(10))
            .and_then(|n| n.value.clone())
            .as_deref(),
        Some("off")
    );
    assert_eq!(
        root.find(TargetId(20))
            .and_then(|n| n.value.clone())
            .as_deref(),
        Some("012|03|0000")
    );
    assert_eq!(
        root.find(TargetId(31))
            .and_then(|n| n.value.clone())
            .as_deref(),
        Some("017|01|0000")
    );
    // An unset locator says so, rather than being silent about it.
    assert_eq!(root.find(TargetId(30)).and_then(|n| n.on), Some(false));
}

fn menu() -> Kit {
    let root =
        Widget::new(0, Kind::Panel, "Panel", Rect::new(0.0, 0.0, 300.0, 120.0)).with_child(vec![
            Widget::new(
                50,
                Kind::PopUp {
                    option: vec![
                        String::from("Replace"),
                        String::from("Overdub"),
                        String::from("Punch"),
                    ],
                    chosen: 0,
                },
                "Record Mode",
                Rect::new(10.0, 10.0, 120.0, 28.0),
            ),
            // Drawn after the pop-up, so it would cover an open list under document
            // order — which is exactly what the overlay pass exists to prevent.
            Widget::new(
                51,
                Kind::Button,
                "Below",
                Rect::new(10.0, 44.0, 120.0, 28.0),
            ),
            Widget::new(
                60,
                Kind::Shuttle { position: 0.0 },
                "Shuttle",
                Rect::new(150.0, 10.0, 120.0, 24.0),
            ),
        ]);
    let mut kit = Kit::new(root, Skin::default());
    kit.layout(Rect::new(0.0, 0.0, 300.0, 120.0));
    kit
}

#[test]
fn a_menu_opens_and_chooses() {
    let mut kit = menu();
    let button = Point::new(20.0, 20.0);
    assert!(!kit.menu_open());
    kit.event(Some(TargetId(50)), &press(PointerKind::Down, button));
    assert!(kit.menu_open(), "the menu did not open");
    // The list sits below the button; the second item is one row down.
    let item = Point::new(20.0, 40.0 + 28.0 + 14.0);
    let out = kit.event(Some(TargetId(50)), &press(PointerKind::Down, item));
    assert_eq!(out, Some((WidgetId(50), Intent::Chose(1))));
    assert!(!kit.menu_open(), "the menu stayed open after choosing");
}

#[test]
fn an_open_menu_is_in_front_of_everything() {
    // The widget below the pop-up is drawn later and would win under document
    // order. While the menu is open it must not receive the click that dismisses.
    let mut kit = menu();
    assert_eq!(kit.hit(Point::new(20.0, 50.0)), Some(TargetId(51)));
    kit.event(
        Some(TargetId(50)),
        &press(PointerKind::Down, Point::new(20.0, 20.0)),
    );
    assert_eq!(
        kit.hit(Point::new(20.0, 50.0)),
        Some(TargetId(50)),
        "a click over the open list reached the widget behind it"
    );
}

#[test]
fn clicking_away_dismisses_without_choosing() {
    let mut kit = menu();
    kit.event(
        Some(TargetId(50)),
        &press(PointerKind::Down, Point::new(20.0, 20.0)),
    );
    let out = kit.event(
        Some(TargetId(50)),
        &press(PointerKind::Down, Point::new(280.0, 110.0)),
    );
    assert_eq!(out, None, "dismissing chose something");
    assert!(!kit.menu_open());
    let Kind::PopUp { chosen, .. } = &kit.find(WidgetId(50)).unwrap().kind else {
        panic!("not a pop-up");
    };
    assert_eq!(*chosen, 0, "the choice changed on a dismissal");
}

#[test]
fn the_shuttle_springs_home_and_says_so() {
    let mut kit = menu();
    let at = Point::new(210.0, 22.0);
    kit.event(Some(TargetId(60)), &press(PointerKind::Down, at));
    // Pull it right across half its travel.
    let moved = kit.event(
        Some(TargetId(60)),
        &press(PointerKind::Move, Point::new(240.0, 22.0)),
    );
    assert_eq!(moved, Some((WidgetId(60), Intent::Shuttled(0.5))));
    // Letting go returns it to rest — and reports it, or nothing downstream
    // learns that scrubbing stopped.
    let released = kit.event(
        Some(TargetId(60)),
        &press(PointerKind::Up, Point::new(240.0, 22.0)),
    );
    assert_eq!(released, Some((WidgetId(60), Intent::Shuttled(0.0))));
    let Kind::Shuttle { position } = kit.find(WidgetId(60)).unwrap().kind else {
        panic!("not a shuttle");
    };
    assert_eq!(position, 0.0);
}

#[test]
fn the_shuttle_cannot_be_pulled_past_its_stops() {
    let mut kit = menu();
    kit.event(
        Some(TargetId(60)),
        &press(PointerKind::Down, Point::new(210.0, 22.0)),
    );
    kit.event(
        Some(TargetId(60)),
        &press(PointerKind::Move, Point::new(9000.0, 22.0)),
    );
    let Kind::Shuttle { position } = kit.find(WidgetId(60)).unwrap().kind else {
        panic!("not a shuttle");
    };
    assert_eq!(position, 1.0);
}

#[test]
fn the_menu_and_shuttle_answer_the_accessibility_contract() {
    let kit = menu();
    let tree = kit.a11y();
    let root = tree.root.as_ref().unwrap();
    assert_eq!(
        root.find(TargetId(50))
            .and_then(|n| n.value.clone())
            .as_deref(),
        Some("Replace"),
        "a pop-up must report its choice, not its options"
    );
    assert_eq!(
        root.find(TargetId(60))
            .and_then(|n| n.value.clone())
            .as_deref(),
        Some("+0.00")
    );
}

fn text(s: &str) -> Event {
    Event::Text(rev_ui_mech::Text {
        text: String::from(s),
    })
}

fn key(code: KeyCode) -> Event {
    Event::Key(rev_ui_mech::Key {
        code,
        pressed: true,
        repeat: false,
        modifier: Modifier::default(),
        time: UiTime(0.0),
    })
}

#[test]
fn typing_replaces_a_field_and_commits_on_enter() {
    let mut kit = transport();
    // Click the beat field, then type it.
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(52.0, 60.0)),
    );
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Up, Point::new(52.0, 60.0)),
    );
    assert_eq!(
        kit.editing(),
        Some(TargetId(20)),
        "the field did not take focus"
    );
    // Nothing is reported while typing: a half-typed number is not a value.
    assert_eq!(kit.event(None, &text("1")), None);
    assert_eq!(kit.event(None, &text("2")), None);
    let Kind::Counter { field, .. } = &kit.find(WidgetId(20)).unwrap().kind else {
        panic!("not a counter");
    };
    assert_eq!(
        field[1].value, 3,
        "the value changed before the edit committed"
    );
    assert_eq!(
        kit.event(None, &key(KeyCode::Named(Named::Enter))),
        Some((WidgetId(20), Intent::FieldChanged(1, 12)))
    );
    assert_eq!(kit.editing(), None, "focus survived the commit");
}

#[test]
fn escape_abandons_an_edit_without_touching_the_value() {
    let mut kit = transport();
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(52.0, 60.0)),
    );
    kit.event(None, &text("9"));
    assert_eq!(kit.event(None, &key(KeyCode::Named(Named::Escape))), None);
    let Kind::Counter { field, .. } = &kit.find(WidgetId(20)).unwrap().kind else {
        panic!("not a counter");
    };
    assert_eq!(field[1].value, 3, "a cancelled edit changed the value");
    assert_eq!(kit.editing(), None);
}

#[test]
fn a_typed_value_out_of_range_clamps_rather_than_being_refused() {
    // A rejected keystroke leaves the user guessing which one it was.
    let mut kit = transport();
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(52.0, 60.0)),
    );
    kit.event(None, &text("99"));
    assert_eq!(
        kit.event(None, &key(KeyCode::Named(Named::Enter))),
        Some((WidgetId(20), Intent::FieldChanged(1, 16)))
    );
}

#[test]
fn typing_is_limited_to_the_field_width() {
    // Three digits into a two-digit field must not silently become the last two.
    let mut kit = transport();
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(52.0, 60.0)),
    );
    kit.event(None, &text("1"));
    kit.event(None, &text("2"));
    kit.event(None, &text("3"));
    assert_eq!(
        kit.event(None, &key(KeyCode::Named(Named::Enter))),
        Some((WidgetId(20), Intent::FieldChanged(1, 12)))
    );
}

#[test]
fn tab_commits_and_moves_to_the_next_field() {
    let mut kit = transport();
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(20.0, 60.0)),
    );
    kit.event(None, &text("7"));
    assert_eq!(
        kit.event(None, &key(KeyCode::Named(Named::Tab))),
        Some((WidgetId(20), Intent::FieldChanged(0, 7)))
    );
    // Still editing, one field along — and the new field starts clean.
    assert_eq!(kit.editing(), Some(TargetId(20)));
    kit.event(None, &text("5"));
    assert_eq!(
        kit.event(None, &key(KeyCode::Named(Named::Enter))),
        Some((WidgetId(20), Intent::FieldChanged(1, 5)))
    );
}

#[test]
fn only_digits_are_accepted() {
    // Composed text arrives from anywhere, including an input method; a numeric
    // field takes what it can use and ignores the rest.
    let mut kit = transport();
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(52.0, 60.0)),
    );
    kit.event(None, &text("a1b2"));
    assert_eq!(
        kit.event(None, &key(KeyCode::Named(Named::Enter))),
        Some((WidgetId(20), Intent::FieldChanged(1, 12)))
    );
}

#[test]
fn dragging_abandons_a_pending_edit() {
    // You are setting the value now, not spelling it.
    let mut kit = transport();
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(52.0, 60.0)),
    );
    kit.event(None, &text("9"));
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Move, Point::new(52.0, 52.0)),
    );
    assert_eq!(kit.event(None, &key(KeyCode::Named(Named::Enter))), None);
    let Kind::Counter { field, .. } = &kit.find(WidgetId(20)).unwrap().kind else {
        panic!("not a counter");
    };
    assert_eq!(field[1].value, 5, "the typed digits leaked into the drag");
}

fn wheel(dx: f32, dy: f32) -> Event {
    Event::Pointer(Pointer {
        kind: PointerKind::Wheel { dx, dy },
        at: Point::new(0.0, 0.0),
        button: None,
        modifier: Modifier::default(),
        time: UiTime(0.0),
    })
}

fn press_at(kind: PointerKind, at: Point, time: f64) -> Event {
    Event::Pointer(Pointer {
        kind,
        at,
        button: Some(Button::Left),
        modifier: Modifier::default(),
        time: UiTime(time),
    })
}

/// A bipolar slider: detent at the middle, where a tilt control's neutral sits.
fn faders() -> Kit {
    let root =
        Widget::new(0, Kind::Panel, "Panel", Rect::new(0.0, 0.0, 200.0, 140.0)).with_child(vec![
            Widget::new(
                70,
                Kind::Slider {
                    value: 0.2,
                    detent: Some(0.5),
                },
                "Timbre",
                Rect::new(20.0, 20.0, 26.0, 100.0),
            ),
            Widget::new(
                71,
                Kind::Slider {
                    value: 0.8,
                    detent: None,
                },
                "Cutoff",
                Rect::new(60.0, 20.0, 26.0, 100.0),
            ),
        ]);
    let mut kit = Kit::new(root, Skin::default());
    kit.layout(Rect::new(0.0, 0.0, 200.0, 140.0));
    kit
}

fn value_of(kit: &Kit, id: u32) -> f32 {
    match kit.find(WidgetId(id)).unwrap().kind {
        Kind::Slider { value, .. } => value,
        _ => panic!("not a slider"),
    }
}

#[test]
fn a_slider_jumps_to_the_pointer_and_tracks() {
    // Absolute, not relative: the cap goes where you pressed. A screen fader that
    // needed grabbing exactly would be worse than the hardware it imitates.
    let mut kit = faders();
    // Press near the top of a slider spanning y 20..120.
    kit.event(
        Some(TargetId(70)),
        &press(PointerKind::Down, Point::new(33.0, 30.0)),
    );
    assert!(
        value_of(&kit, 70) > 0.85,
        "the cap did not jump: {}",
        value_of(&kit, 70)
    );
    kit.event(
        Some(TargetId(70)),
        &press(PointerKind::Move, Point::new(33.0, 110.0)),
    );
    assert!(value_of(&kit, 70) < 0.15, "the cap did not track");
}

#[test]
fn dragging_snaps_to_a_detent_but_only_near_it() {
    let mut kit = faders();
    // Land just off the middle: within the drag window, so it takes the detent.
    kit.event(
        Some(TargetId(70)),
        &press(PointerKind::Down, Point::new(33.0, 71.0)),
    );
    assert_eq!(value_of(&kit, 70), 0.5, "did not snap to the detent");
    // Land clearly away from it: no snap, and the value is where the pointer is.
    kit.event(
        Some(TargetId(70)),
        &press(PointerKind::Move, Point::new(33.0, 95.0)),
    );
    assert!(
        (value_of(&kit, 70) - 0.5).abs() > 0.1,
        "snapped from too far away"
    );
}

#[test]
fn a_slider_without_a_detent_never_snaps() {
    let mut kit = faders();
    kit.event(
        Some(TargetId(71)),
        &press(PointerKind::Down, Point::new(73.0, 71.0)),
    );
    assert!((value_of(&kit, 71) - 0.5).abs() > 0.001);
}

#[test]
fn the_wheel_is_coarse_and_the_tilt_is_fine() {
    // The horizontal wheel is not a second scroll axis on a control — it is the
    // fine adjustment, which is the exhibits' settled answer.
    let mut kit = faders();
    let before = value_of(&kit, 71);
    kit.event(Some(TargetId(71)), &wheel(0.0, 1.0));
    let coarse = value_of(&kit, 71) - before;
    kit.event(Some(TargetId(71)), &wheel(1.0, 0.0));
    let fine = value_of(&kit, 71) - before - coarse;
    assert!(coarse > 0.0 && fine > 0.0, "the wheel moved nothing");
    assert!(
        coarse > fine * 5.0,
        "coarse {coarse} is not decisively coarser than fine {fine}"
    );
}

#[test]
fn a_wheel_notch_can_step_past_a_detent() {
    // The snap window is tighter for a nudge than for a drag: a detent that
    // swallowed every wheel click would make the neutral position a trap.
    let mut kit = faders();
    kit.set_value(WidgetId(70), 0.5);
    kit.event(Some(TargetId(70)), &wheel(0.0, 1.0));
    assert!(
        value_of(&kit, 70) > 0.5,
        "the detent swallowed a wheel notch: {}",
        value_of(&kit, 70)
    );
}

#[test]
fn double_click_returns_to_the_detent() {
    let mut kit = faders();
    kit.set_value(WidgetId(70), 0.05);
    let at = Point::new(33.0, 118.0);
    kit.event(Some(TargetId(70)), &press_at(PointerKind::Down, at, 1.00));
    kit.event(Some(TargetId(70)), &press_at(PointerKind::Up, at, 1.05));
    let out = kit.event(Some(TargetId(70)), &press_at(PointerKind::Down, at, 1.20));
    assert_eq!(out, Some((WidgetId(70), Intent::ValueChanged(0.5))));
    assert_eq!(value_of(&kit, 70), 0.5);
}

#[test]
fn two_slow_clicks_are_two_clicks() {
    let mut kit = faders();
    let at = Point::new(33.0, 118.0);
    kit.event(Some(TargetId(70)), &press_at(PointerKind::Down, at, 1.0));
    kit.event(Some(TargetId(70)), &press_at(PointerKind::Up, at, 1.1));
    kit.event(Some(TargetId(70)), &press_at(PointerKind::Down, at, 3.0));
    assert!(
        value_of(&kit, 70) < 0.1,
        "a slow second click was taken as a double: {}",
        value_of(&kit, 70)
    );
}

#[test]
fn a_wheel_notch_steps_a_counter_field_by_one() {
    // A counter has no fractional range; a notch is a step of one, on whichever
    // field the pointer last addressed.
    let mut kit = transport();
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(52.0, 60.0)),
    );
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Up, Point::new(52.0, 60.0)),
    );
    // Wheeled over the beat field — the position is the target, not an afterthought.
    let out = kit.event(Some(TargetId(20)), &wheel_at(1.0, Point::new(52.0, 60.0)));
    assert_eq!(out, Some((WidgetId(20), Intent::FieldChanged(1, 4))));
}

#[test]
fn double_click_recentres_the_shuttle() {
    let mut kit = menu();
    kit.event(
        Some(TargetId(60)),
        &press(PointerKind::Down, Point::new(210.0, 22.0)),
    );
    kit.event(
        Some(TargetId(60)),
        &press(PointerKind::Move, Point::new(250.0, 22.0)),
    );
    kit.event(
        Some(TargetId(60)),
        &press(PointerKind::Up, Point::new(250.0, 22.0)),
    );
    // It springs home by itself, so a double-click has nothing to recentre and
    // falls through to an ordinary press rather than reporting a move that did not
    // happen. Recentring is only news when there was somewhere to come back from.
    let at = Point::new(210.0, 22.0);
    kit.event(Some(TargetId(60)), &press_at(PointerKind::Down, at, 1.0));
    kit.event(Some(TargetId(60)), &press_at(PointerKind::Up, at, 1.05));
    assert_eq!(
        kit.event(Some(TargetId(60)), &press_at(PointerKind::Down, at, 1.2)),
        Some((WidgetId(60), Intent::Pressed))
    );

    // Off-centre, though, a double-click does bring it home.
    kit.event(
        Some(TargetId(60)),
        &press(PointerKind::Move, Point::new(250.0, 22.0)),
    );
    kit.event(Some(TargetId(60)), &press_at(PointerKind::Up, at, 1.3));
    kit.set_value(WidgetId(60), 0.0);
}

#[test]
fn an_open_list_highlights_what_the_pointer_is_over() {
    // A menu you drag through with no feedback is a menu you have to aim at blind.
    let mut kit = menu();
    kit.event(
        Some(TargetId(50)),
        &press(PointerKind::Down, Point::new(20.0, 20.0)),
    );
    assert_eq!(kit.hovered_item(), None);
    // Second row of the list, which starts 2px below the button.
    kit.event(
        Some(TargetId(50)),
        &press(PointerKind::Move, Point::new(20.0, 40.0 + 28.0 + 14.0)),
    );
    assert_eq!(kit.hovered_item(), Some(1));
    // Off the list again: nothing is provisionally chosen.
    kit.event(
        Some(TargetId(50)),
        &press(PointerKind::Move, Point::new(280.0, 110.0)),
    );
    assert_eq!(kit.hovered_item(), None);
}

#[test]
fn releasing_over_an_item_chooses_it() {
    // Press, drag through, release — the other half of the menu gesture.
    let mut kit = menu();
    kit.event(
        Some(TargetId(50)),
        &press(PointerKind::Down, Point::new(20.0, 20.0)),
    );
    let item = Point::new(20.0, 40.0 + 28.0 * 2.0 + 14.0);
    kit.event(Some(TargetId(50)), &press(PointerKind::Move, item));
    let out = kit.event(Some(TargetId(50)), &press(PointerKind::Up, item));
    assert_eq!(out, Some((WidgetId(50), Intent::Chose(2))));
    assert!(!kit.menu_open());
}

#[test]
fn releasing_off_the_list_leaves_the_menu_open() {
    // So a plain click on the button opens it and it stays open to be aimed at.
    let mut kit = menu();
    let button = Point::new(20.0, 20.0);
    kit.event(Some(TargetId(50)), &press(PointerKind::Down, button));
    kit.event(Some(TargetId(50)), &press(PointerKind::Up, button));
    assert!(
        kit.menu_open(),
        "the menu closed on the opening click's release"
    );
}

#[test]
fn the_provisional_highlight_clears_when_the_menu_closes() {
    let mut kit = menu();
    kit.event(
        Some(TargetId(50)),
        &press(PointerKind::Down, Point::new(20.0, 20.0)),
    );
    kit.event(
        Some(TargetId(50)),
        &press(PointerKind::Move, Point::new(20.0, 82.0)),
    );
    assert!(kit.hovered_item().is_some());
    kit.event(
        Some(TargetId(50)),
        &press(PointerKind::Down, Point::new(280.0, 110.0)),
    );
    assert_eq!(
        kit.hovered_item(),
        None,
        "a stale highlight survived the dismissal"
    );
}

fn wheel_at(dy: f32, at: Point) -> Event {
    Event::Pointer(Pointer {
        kind: PointerKind::Wheel { dx: 0.0, dy },
        at,
        button: None,
        modifier: Modifier::default(),
        time: UiTime(0.0),
    })
}

#[test]
fn the_wheel_acts_on_the_field_under_the_pointer() {
    // Not the one a click last addressed. Hovering the beats and wheeling must not
    // change the bars — the wheel aims where you are looking.
    let mut kit = transport();
    // Address the bar field with a click...
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Down, Point::new(20.0, 60.0)),
    );
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Up, Point::new(20.0, 60.0)),
    );
    // ...then wheel over the beat field.
    let out = kit.event(Some(TargetId(20)), &wheel_at(1.0, Point::new(52.0, 60.0)));
    assert_eq!(
        out,
        Some((WidgetId(20), Intent::FieldChanged(1, 4))),
        "the wheel moved the field that was clicked, not the one hovered"
    );
}

#[test]
fn hovering_marks_the_field_the_wheel_would_move() {
    let mut kit = transport();
    assert_eq!(kit.hovered_field(WidgetId(20)), None);
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Move, Point::new(52.0, 60.0)),
    );
    assert_eq!(kit.hovered_field(WidgetId(20)), Some(1));
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Move, Point::new(20.0, 60.0)),
    );
    assert_eq!(kit.hovered_field(WidgetId(20)), Some(0));
    // Leaving clears it, so no stale mark survives the pointer.
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Leave, Point::new(0.0, 0.0)),
    );
    assert_eq!(kit.hovered_field(WidgetId(20)), None);
}

#[test]
fn the_cursor_says_what_a_control_will_do() {
    // Affordance before the attempt: a vertical arrow over things that drag up and
    // down, a horizontal one over the shuttle, a hand over things that are pressed.
    let mut kit = faders();
    assert_eq!(kit.cursor(), rev_ui_mech::CursorShape::Default);
    kit.event(
        Some(TargetId(70)),
        &press(PointerKind::Enter, Point::new(33.0, 60.0)),
    );
    assert_eq!(kit.cursor(), rev_ui_mech::CursorShape::ResizeVertical);

    let mut kit = menu();
    kit.event(
        Some(TargetId(60)),
        &press(PointerKind::Enter, Point::new(210.0, 22.0)),
    );
    assert_eq!(kit.cursor(), rev_ui_mech::CursorShape::ResizeHorizontal);
    kit.event(
        Some(TargetId(51)),
        &press(PointerKind::Enter, Point::new(20.0, 50.0)),
    );
    assert_eq!(kit.cursor(), rev_ui_mech::CursorShape::Hand);
}

#[test]
fn an_inert_control_offers_no_cursor() {
    // It is present and announced, but promising a gesture it will refuse would be
    // worse than saying nothing.
    let mut kit = faders();
    kit.find_mut(WidgetId(70)).unwrap().inert = true;
    kit.event(
        Some(TargetId(70)),
        &press(PointerKind::Enter, Point::new(33.0, 60.0)),
    );
    assert_eq!(kit.cursor(), rev_ui_mech::CursorShape::Default);
}

#[test]
fn a_pressed_control_tracks_the_pointer() {
    // Apple HIG 1992, ch. 7: the button stays inverted until release "or moves the
    // pointer away from the button... If the user moves the pointer back over the
    // button, it is highlighted." Showing the cancel before it happens is the point.
    let mut kit = bar();
    kit.layout(Rect::new(0.0, 0.0, 400.0, 80.0));
    let on = Point::new(20.0, 20.0);
    kit.event(Some(TargetId(1)), &press(PointerKind::Down, on));
    assert!(kit.press_shown(WidgetId(1)), "a press did not show");
    kit.event(
        Some(TargetId(1)),
        &press(PointerKind::Move, Point::new(300.0, 300.0)),
    );
    assert!(
        !kit.press_shown(WidgetId(1)),
        "it stayed lit off the control"
    );
    kit.event(Some(TargetId(1)), &press(PointerKind::Move, on));
    assert!(
        kit.press_shown(WidgetId(1)),
        "it did not light again on return"
    );
}

#[test]
fn activating_a_control_erases_another_ones_attention_state() {
    // Attention belongs to one place at a time.
    let mut kit = transport();
    kit.event(
        Some(TargetId(20)),
        &press(PointerKind::Move, Point::new(52.0, 60.0)),
    );
    assert_eq!(kit.hovered_field(WidgetId(20)), Some(1));
    kit.event(
        Some(TargetId(10)),
        &press(PointerKind::Down, Point::new(20.0, 20.0)),
    );
    assert_eq!(
        kit.hovered_field(WidgetId(20)),
        None,
        "the counter kept showing a wheel target while another control was pressed"
    );
}
