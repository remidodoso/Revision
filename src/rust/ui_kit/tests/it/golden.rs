//! A screenshot reference for a real widget tree.
//!
//! The point is not that these particular controls look right — it is that a tree
//! *described as data* renders through the skin and the paint list without any
//! imperative drawing code in between. That is the property R-622, R-1310, and a
//! layout designer all depend on.

use rev_testkit::image;
use rev_ui_kit::{Anchor, Field, Kind, Kit, RecordMode, Skin, State, Widget};
use rev_ui_mech::{Canvas, Point, Rect, UiTime};

fn bar() -> Kit {
    let root = Widget::new(
        0,
        Kind::Panel,
        "Control Bar",
        Rect::new(0.0, 0.0, 460.0, 244.0),
    )
    .with_anchor(Anchor::FILL)
    .with_child(vec![
        Widget::new(1, Kind::Button, "Play", Rect::new(16.0, 18.0, 74.0, 32.0)),
        Widget::new(2, Kind::Button, "Stop", Rect::new(98.0, 18.0, 74.0, 32.0)),
        // Latched, so it paints in its Active colour — and would keep doing so
        // under a hovering pointer, which is the rule a bug taught us.
        Widget::new(
            3,
            Kind::Toggle { on: true },
            "Loop",
            Rect::new(180.0, 18.0, 74.0, 32.0),
        ),
        // Inert: present, dimmed, still announced.
        Widget::new(
            4,
            Kind::Toggle { on: false },
            "Punch",
            Rect::new(262.0, 18.0, 74.0, 32.0),
        ),
        Widget::new(
            5,
            Kind::Lamp { lit: true },
            "Record",
            Rect::new(352.0, 28.0, 12.0, 12.0),
        ),
        Widget::new(
            6,
            Kind::Lamp { lit: false },
            "Sync",
            Rect::new(374.0, 28.0, 12.0, 12.0),
        ),
        Widget::new(
            7,
            Kind::Readout {
                value: String::from("012|03|0000"),
            },
            "Counter",
            Rect::new(16.0, 58.0, 190.0, 26.0),
        )
        .with_anchor(Anchor::TOP_WIDE),
        Widget::new(
            8,
            Kind::Label,
            "bar · beat · unit",
            Rect::new(216.0, 62.0, 160.0, 18.0),
        ),
    ]);
    let mut root = root;
    root.child[3].inert = true;
    root.child[0].state = State::Idle;

    // The census. Record appears in all three of its states side by side, which is
    // the point of the widget: two of them would be a toggle.
    for (n, mode) in [RecordMode::Off, RecordMode::Armed, RecordMode::Recording]
        .into_iter()
        .enumerate()
    {
        root.child.push(Widget::new(
            20 + n as u32,
            Kind::Record { mode },
            "Record",
            Rect::new(398.0 - n as f32 * 0.0, 8.0 + n as f32 * 30.0, 46.0, 26.0),
        ));
    }
    root.child.push(Widget::new(
        30,
        Kind::Counter {
            field: vec![
                Field::new(12, 3, 1, 999),
                Field::new(3, 2, 1, 16),
                Field::new(0, 4, 0, 5039),
            ],
            separator: '|',
        },
        "Counter",
        Rect::new(16.0, 96.0, 190.0, 30.0),
    ));
    // A locator bank: filled slots read at a glance against empty ones.
    for n in 0..6u8 {
        let at = matches!(n, 0 | 2 | 5).then(|| String::from("017|01|0000"));
        root.child.push(Widget::new(
            40 + u32::from(n),
            Kind::Locator { index: n + 1, at },
            format!("Locator {}", n + 1),
            Rect::new(216.0 + f32::from(n) * 28.0, 96.0, 24.0, 30.0),
        ));
    }
    // The rest of the census: a pop-up and the shuttle.
    root.child.push(Widget::new(
        50,
        Kind::PopUp {
            option: vec![
                String::from("Replace"),
                String::from("Overdub"),
                String::from("Punch"),
            ],
            chosen: 1,
        },
        "Record Mode",
        Rect::new(16.0, 134.0, 130.0, 28.0),
    ));
    // Sliders: bipolar with a centred detent, one plain, one at an off-centre
    // semantic zero — the detent marks meaning, not the middle.
    for (n, (value, detent)) in [(0.5, Some(0.5)), (0.72, None), (0.3, Some(0.25))]
        .into_iter()
        .enumerate()
    {
        root.child.push(Widget::new(
            70 + n as u32,
            Kind::Slider { value, detent },
            ["Timbre", "Cutoff", "Spread"][n],
            Rect::new(330.0 + n as f32 * 34.0, 130.0, 28.0, 100.0),
        ));
    }
    root.child.push(Widget::new(
        60,
        Kind::Shuttle { position: -0.45 },
        "Shuttle",
        Rect::new(160.0, 136.0, 150.0, 24.0),
    ));
    Kit::new(root, Skin::default())
}

/// The counter in its three conditions: idle, a field addressed, a field being
/// typed. A still image is the only way to judge whether the feedback is visible.
#[test]
fn counter_field_feedback() {
    use rev_ui_mech::{Button, Modifier, Pointer, PointerKind, TargetId, Text, UiTime};

    let make = || {
        let root = Widget::new(0, Kind::Panel, "Panel", Rect::new(0.0, 0.0, 240.0, 46.0))
            .with_child(vec![Widget::new(
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
                Rect::new(10.0, 8.0, 220.0, 30.0),
            )]);
        let mut kit = Kit::new(root, Skin::default());
        kit.layout(Rect::new(0.0, 0.0, 240.0, 46.0));
        kit
    };
    let pointer = |kind, x: f32| {
        rev_ui_mech::Event::Pointer(Pointer {
            kind,
            at: Point::new(x, 20.0),
            button: Some(Button::Left),
            modifier: Modifier::default(),
            time: UiTime(0.0),
        })
    };

    let idle = make();
    let mut addressed = make();
    addressed.event(Some(TargetId(20)), &pointer(PointerKind::Down, 30.0));
    let mut typing = make();
    typing.event(Some(TargetId(20)), &pointer(PointerKind::Down, 70.0));
    typing.event(
        None,
        &rev_ui_mech::Event::Text(Text {
            text: String::from("9"),
        }),
    );

    let mut canvas = Canvas::new(240, 150, 1.0).unwrap();
    canvas.paint(|p| {
        p.clear(idle.skin().panel_lo);
        idle.paint(p);
        p.push_offset(Point::new(0.0, 50.0));
        addressed.paint(p);
        p.pop_offset();
        p.push_offset(Point::new(0.0, 100.0));
        typing.paint(p);
        p.pop_offset();
    });
    if let Err(e) = image::compare_png("counter_states", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

/// An open menu with the pointer over an item. What is *about to* be chosen has to
/// outrank what was chosen last time, or a menu reads as already-decided.
#[test]
fn menu_open_with_a_provisional_choice() {
    use rev_ui_mech::{Button, Modifier, Pointer, PointerKind, TargetId};

    let root =
        Widget::new(0, Kind::Panel, "Panel", Rect::new(0.0, 0.0, 200.0, 160.0)).with_child(vec![
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
                Rect::new(16.0, 16.0, 140.0, 30.0),
            ),
        ]);
    let mut kit = Kit::new(root, Skin::default());
    kit.layout(Rect::new(0.0, 0.0, 200.0, 160.0));
    let pointer = |kind, y: f32| {
        rev_ui_mech::Event::Pointer(Pointer {
            kind,
            at: Point::new(40.0, y),
            button: Some(Button::Left),
            modifier: Modifier::default(),
            time: UiTime(0.0),
        })
    };
    kit.event(Some(TargetId(50)), &pointer(PointerKind::Down, 30.0));
    kit.event(
        Some(TargetId(50)),
        &pointer(PointerKind::Move, 48.0 + 30.0 + 15.0),
    );

    let mut canvas = Canvas::new(200, 160, 1.0).unwrap();
    canvas.paint(|p| {
        p.clear(kit.skin().panel_lo);
        kit.paint(p);
    });
    if let Err(e) = image::compare_png("menu_open", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

#[test]
fn a_described_tree_renders() {
    let mut kit = bar();
    kit.layout(Rect::new(10.0, 10.0, 460.0, 244.0));
    let mut canvas = Canvas::new(480, 264, 1.0).unwrap();
    canvas.paint(|p| kit.paint(p));
    println!("kit 1x: {}", canvas.stat().summary());
    if let Err(e) = image::compare_png("kit_1x", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

#[test]
fn the_same_tree_at_125x() {
    let mut kit = bar();
    kit.layout(Rect::new(10.0, 10.0, 460.0, 244.0));
    let mut canvas = Canvas::new(600, 330, 1.25).unwrap();
    canvas.paint(|p| kit.paint(p));
    if let Err(e) = image::compare_png("kit_125x", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}
