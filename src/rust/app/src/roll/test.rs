use super::*;

use rev_testkit::{TempProject, fixture};

fn mhall() -> (TempProject, fixture::Mhall, Roll) {
    let mut temp = TempProject::create().expect("project");
    let built = fixture::mhall(temp.project_mut()).expect("mhall");
    let mut cache = TuneCache::new();
    let roll = Roll::build(temp.project(), &mut cache, built.track).expect("roll");
    (temp, built, roll)
}

#[test]
fn every_note_of_mhall_is_drawable() {
    let (_temp, _built, roll) = mhall();
    assert_eq!(roll.note.len(), 26, "the whole tune");
    assert!(roll.note.iter().all(|n| n.hz > 0.0 && n.length > 0.0));
    // Eight bars at four beats.
    assert!(
        (roll.beat_extent - 32.0).abs() < 0.01,
        "extent {}",
        roll.beat_extent
    );
}

#[test]
fn the_roll_draws_what_the_engine_plays() {
    // **R-312 made checkable.** The engine is handed frequencies; if the roll
    // resolved pitch its own way the picture and the sound could disagree and
    // nothing would notice. They share `TuneCache`, so this is exact rather
    // than approximate — and it is exact in both tunings, which is the case
    // where a separate resolution path would drift first.
    for sixteen in [false, true] {
        let mut temp = TempProject::create().expect("project");
        let built = fixture::mhall(temp.project_mut()).expect("mhall");
        if sixteen {
            temp.project_mut()
                .apply(rev_core::Command::SetPhrase {
                    id: built.melody,
                    patch: rev_core::phrase::PhrasePatch {
                        tuning_id: rev_core::phrase::Change::Set(built.tuning_16et),
                        ..Default::default()
                    },
                })
                .expect("retune");
        }

        let mut cache = TuneCache::new();
        let roll = Roll::build(temp.project(), &mut cache, built.track).expect("roll");

        // The compiler's own resolution, for the same material.
        let mut compiler = rev_sched::Compiler::new(
            rev_sched::TempoMap::new(
                [(
                    rev_core::tick::Tick(0),
                    rev_core::tick::bpm_to_usec_per_quarter(120.0),
                )],
                48_000,
            ),
            vec![built.track],
        );
        let chunk = compiler
            .chunk(
                temp.project(),
                rev_engine::SampleTime(0),
                rev_engine::SampleTime(48_000 * 20),
            )
            .expect("compile");

        assert_eq!(chunk.note.len(), roll.note.len(), "same notes");
        let mut drawn: Vec<f64> = roll.note.iter().map(|n| n.hz).collect();
        let mut played: Vec<f64> = chunk.note.iter().map(|n| f64::from(n.hz)).collect();
        drawn.sort_by(f64::total_cmp);
        played.sort_by(f64::total_cmp);
        for (a, b) in drawn.iter().zip(&played) {
            // The engine takes `f32`; the roll keeps `f64`. Equal once narrowed,
            // which is the only equality that means anything here.
            assert_eq!(*a as f32, *b as f32, "drawn {a} vs played {b}");
        }
    }
}

#[test]
fn an_octave_is_an_octave_at_any_zoom() {
    // The property that makes R-941 free: y is log2(hz), so a doubling is
    // always the same distance, and no code anywhere divides an octave by a
    // number of degrees.
    let (_temp, _built, roll) = mhall();
    let low = roll.note.iter().map(|n| n.hz).fold(f64::MAX, f64::min);
    let a = roll.content_y(low.log2());
    let b = roll.content_y((low * 2.0).log2());
    let c = roll.content_y((low * 4.0).log2());
    assert!(
        ((a - b) - (b - c)).abs() < 1e-4,
        "octaves should be equidistant: {} vs {}",
        a - b,
        b - c
    );
}

#[test]
fn retuning_moves_notes_and_not_onsets() {
    // R-942, seen rather than heard: the party trick from eng-07, now visible.
    let render = |sixteen: bool| {
        let mut temp = TempProject::create().expect("project");
        let built = fixture::mhall(temp.project_mut()).expect("mhall");
        if sixteen {
            temp.project_mut()
                .apply(rev_core::Command::SetPhrase {
                    id: built.melody,
                    patch: rev_core::phrase::PhrasePatch {
                        tuning_id: rev_core::phrase::Change::Set(built.tuning_16et),
                        ..Default::default()
                    },
                })
                .expect("retune");
        }
        let mut cache = TuneCache::new();
        Roll::build(temp.project(), &mut cache, built.track).expect("roll")
    };

    let twelve = render(false);
    let sixteen = render(true);
    let onset = |r: &Roll| r.note.iter().map(|n| n.beat).collect::<Vec<_>>();
    assert_eq!(onset(&twelve), onset(&sixteen), "not one onset moved");
    assert_ne!(
        twelve
            .note
            .iter()
            .map(|n| n.y.to_bits())
            .collect::<Vec<_>>(),
        sixteen
            .note
            .iter()
            .map(|n| n.y.to_bits())
            .collect::<Vec<_>>(),
        "and the heights genuinely changed"
    );
}

#[test]
fn the_ladder_labels_degrees_and_has_a_rung_per_degree() {
    let (_temp, _built, roll) = mhall();
    assert!(!roll.rung.is_empty(), "a ladder");
    // Ascending, and each labelled — a degree without a conventional name is an
    // ordinary case (R-943), so the only requirement is that it says something.
    for pair in roll.rung.windows(2) {
        assert!(pair[1].y > pair[0].y, "ascending");
    }
    assert!(roll.rung.iter().all(|r| !r.label.is_empty()));
    // 12-ET: a twelfth of an octave between rungs.
    assert!(
        (roll.degree_step - 1.0 / 12.0).abs() < 1e-6,
        "step {}",
        roll.degree_step
    );
}

#[test]
fn the_grid_ladder_lands_on_positions_music_can_hold() {
    // Rungs are divisors of the tick resolution, not powers of ten: 5040 is
    // 2^4*3^2*5*7, so each of these divides exactly and every line falls on a
    // real tick.
    for div in SUBDIVISION {
        assert_eq!(PPQ % div, 0, "{div} does not divide the tick resolution");
    }
    // Finer zoom asks for a finer step, and never a finer one than a tick.
    let coarse = grid_step(1.0, 48.0);
    let fine = grid_step(0.001, 48.0);
    assert!(fine < coarse, "{fine} should be finer than {coarse}");
    assert!(
        fine >= 1.0 / PPQ as f64 - f64::EPSILON,
        "never finer than one tick: {fine}"
    );
    // And at every zoom the lines stay at least as far apart as asked.
    for scale in [0.0001f32, 0.01, 0.1, 1.0, 10.0] {
        let step = grid_step(scale, 48.0);
        assert!(
            (step / f64::from(scale)) as f32 >= 48.0 - 0.001,
            "scale {scale} gave {step}"
        );
    }
}

#[test]
fn thickness_never_fills_the_gap_between_degrees() {
    // It is a visual weight, not a claim about a pitch range: a note has a
    // frequency, not a band. So it stays under the degree spacing at every zoom
    // where the spacing is visible at all.
    let (_temp, _built, roll) = mhall();
    for scale in [0.0005f32, 0.002, 0.01, 0.05] {
        let pane = Pane {
            scale: rev_ui_kit::pane::Scale { x: 1.0, y: scale },
            ..Pane::default()
        };
        let spacing = (roll.degree_step / f64::from(scale)) as f32;
        let t = thickness(&roll, &pane);
        assert!((5.0..=18.0).contains(&t), "clamped: {t}");
        if spacing > 18.0 / 0.6 {
            assert!(t < spacing, "thickness {t} vs spacing {spacing}");
        }
    }
}

/// MHALL on the roll: a look, and a golden master.
///
/// The lesson eng-07 taught in a different costume — a property nothing asserts
/// on is a property nobody knows about. Geometry is testable; whether the tune
/// *reads* as a tune is not, so it gets a picture.
#[test]
fn mhall_on_the_roll() {
    use rev_ui_kit::pane::{BarPolicy, Scale};
    use rev_ui_mech::{Canvas, Size};

    let (_temp, _built, roll) = mhall();
    let (beats, octaves) = roll.extent();
    let frame = Rect::new(0.0, 0.0, 640.0, 300.0);
    let pane = Pane {
        extent: Size::new(beats, octaves),
        // The whole tune across, the whole range down.
        scale: Scale {
            x: beats / (frame.w - 22.0),
            y: octaves / (frame.h - 22.0),
        },
        bar: BarPolicy::Both,
        ..Pane::default()
    };

    let mut canvas = Canvas::new(640, 300, 1.0).unwrap();
    canvas.paint(|p| {
        p.clear(rev_ui_mech::Color::rgba(20, 22, 26, 255));
        let interior = pane.interior(frame);
        p.push_clip(interior);
        paint(&roll, &pane, interior, Some(12.0), p);
        p.pop_clip();
    });
    if let Err(e) = rev_testkit::image::compare_png("roll_mhall", &canvas.png().unwrap()) {
        panic!("{e}");
    }
}

#[test]
fn the_notes_reach_the_accessibility_tree_as_data() {
    // ui-07 section 3 signed this debt on the roll's behalf: the pane says what
    // it is and how far it scrolls, and the interior's meaning is its
    // consumer's to supply. Notes as pitch, onset and duration - not rectangles.
    use rev_ui_kit::{Kit, Skin, Widget, WidgetId};
    use rev_ui_mech::{Size, TargetId};

    let (_temp, _built, roll) = mhall();
    let (beats, octaves) = roll.extent();
    let pane = Pane {
        extent: Size::new(beats, octaves),
        scale: rev_ui_kit::pane::Scale { x: 0.05, y: 0.005 },
        ..Pane::default()
    };
    let frame = Rect::new(0.0, 0.0, 400.0, 300.0);
    let root =
        Widget::new(0, rev_ui_kit::Kind::Panel, "Panel", frame).with_child(vec![Widget::new(
            1,
            rev_ui_kit::Kind::Pane { pane },
            "Roll",
            frame,
        )]);
    let mut kit = Kit::new(root, Skin::default());
    kit.layout(frame);
    let _ = WidgetId(1);

    let mut tree = kit.a11y();
    describe(&roll, &mut tree, TargetId(1), frame, &pane);

    let node = tree.root.as_ref().expect("a root");
    let found = node
        .child
        .iter()
        .find(|c| c.id == TargetId(1))
        .expect("the pane");
    assert_eq!(found.child.len(), 26, "every note is announced");
    let first = &found.child[0];
    assert!(first.label.contains("note "), "labelled: {}", first.label);
    let value = first.value.as_deref().unwrap_or("");
    assert!(value.contains("Hz"), "the pitch it sounds: {value}");
    // R-944: displayed positions honour the origin preference, default 1.
    assert!(
        value.contains("beat 1.00"),
        "counted from the origin: {value}"
    );
}
