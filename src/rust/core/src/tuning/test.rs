use super::*;

const MIDDLE_C: f64 = 261.625_565_300_598_6;

fn equal_spec(name: &str, note_per_period: i32) -> TuningSpec {
    let mut spec = TuningSpec::new(name, TuningKind::Equal, 60, MIDDLE_C);
    spec.period = Some(Ratio::OCTAVE);
    spec.note_per_period = Some(note_per_period);
    spec.note_min = Some(NoteNumber(0));
    spec.note_max = Some(NoteNumber(127));
    spec
}

fn just_5limit() -> (TuningSpec, Vec<TuningNote>) {
    let mut spec = TuningSpec::new("just", TuningKind::Table, 60, MIDDLE_C);
    spec.period = Some(Ratio::OCTAVE);
    spec.note_per_period = Some(12);
    spec.note_min = Some(NoteNumber(0));
    spec.note_max = Some(NoteNumber(127));
    let ratio = [
        (1, 1),
        (16, 15),
        (9, 8),
        (6, 5),
        (5, 4),
        (4, 3),
        (45, 32),
        (3, 2),
        (8, 5),
        (5, 3),
        (9, 5),
        (15, 8),
    ];
    // One canonical period starting at the anchor: notes 60..71.
    let note = ratio
        .iter()
        .enumerate()
        .map(|(i, &(num, den))| TuningNote {
            note_number: NoteNumber(60 + i as i32),
            value: TuningNoteValue::Ratio(Ratio::new(num, den)),
        })
        .collect();
    (spec, note)
}

#[test]
fn twelve_equal_gives_concert_pitch_and_exact_octaves() {
    let table = materialize(&equal_spec("12-et", 12), &[]).unwrap();
    let a4 = table.freq(NoteNumber(69)).unwrap();
    assert!((a4 - 440.0).abs() < 1e-9, "A4 = {a4}");
    let c4 = table.freq(NoteNumber(60)).unwrap();
    let c5 = table.freq(NoteNumber(72)).unwrap();
    assert!((c5 / c4 - 2.0).abs() < 1e-12, "octave = {}", c5 / c4);
}

#[test]
fn sixteen_equal_steps_are_seventy_five_cents() {
    let table = materialize(&equal_spec("16-et", 16), &[]).unwrap();
    let c4 = table.freq(NoteNumber(60)).unwrap();
    let next = table.freq(NoteNumber(61)).unwrap();
    let cents = 1200.0 * equal::log2(next / c4);
    assert!((cents - 75.0).abs() < 1e-9, "step = {cents} cents");
    // The anchor holds across tunings: switching keeps the home pitch.
    assert!((c4 - MIDDLE_C).abs() < 1e-12);
}

#[test]
fn just_intonation_realizes_exact_ratios() {
    let (spec, note) = just_5limit();
    let table = materialize(&spec, &note).unwrap();
    let c4 = table.freq(NoteNumber(60)).unwrap();
    let g4 = table.freq(NoteNumber(67)).unwrap();
    // A just fifth is exactly 3/2 — the point of storing ratios (R-504).
    assert!((g4 / c4 - 1.5).abs() < 1e-12, "fifth = {}", g4 / c4);
    // And the period extends the canonical rows in both directions.
    let g3 = table.freq(NoteNumber(55)).unwrap();
    assert!((g4 / g3 - 2.0).abs() < 1e-12);
}

#[test]
fn aperiodic_table_uses_its_rows_as_the_domain() {
    // A "cross"-style tuning: a sorted frequency list with no octave at all.
    let mut spec = TuningSpec::new("sparse", TuningKind::Table, 10, 100.0);
    spec.note_min = None;
    spec.note_max = None;
    let note: Vec<_> = [100.0, 120.0, 150.0, 190.0]
        .iter()
        .enumerate()
        .map(|(i, &f)| TuningNote {
            note_number: NoteNumber(10 + i as i32),
            value: TuningNoteValue::Freq(f),
        })
        .collect();
    let table = materialize(&spec, &note).unwrap();

    assert_eq!(table.note_range(), (NoteNumber(10), NoteNumber(13)));
    assert!(
        !table.has_period(),
        "aperiodic tunings gate octave features off"
    );
    assert_eq!(table.freq(NoteNumber(12)), Some(150.0));
    // Out of domain is None, never a clamped neighbour.
    assert_eq!(table.freq(NoteNumber(9)), None);
    assert_eq!(table.freq(NoteNumber(14)), None);
}

#[test]
fn nearest_note_brackets_in_log_space() {
    let table = materialize(&equal_spec("12-et", 12), &[]).unwrap();
    assert_eq!(table.nearest_note(440.0), Some(NoteNumber(69)));
    assert_eq!(table.nearest_note(MIDDLE_C), Some(NoteNumber(60)));
    // A quarter-tone below A4 is still nearest A4's neighbour below it.
    let quarter_flat = 440.0 * equal::exp2(-0.6 / 12.0);
    assert_eq!(table.nearest_note(quarter_flat), Some(NoteNumber(68)));
    // Off the ends, clamp to the domain rather than returning nothing.
    assert_eq!(table.nearest_note(1.0), Some(NoteNumber(0)));
    assert_eq!(table.nearest_note(1e9), Some(NoteNumber(127)));
}

#[test]
fn materialization_is_bit_identical_when_repeated() {
    // The determinism gate (R-1402) at the tuning layer.
    let spec = equal_spec("16-et", 16);
    let first = materialize(&spec, &[]).unwrap();
    let second = materialize(&spec, &[]).unwrap();
    for (a, b) in first.rows().zip(second.rows()) {
        assert_eq!(a.1.to_bits(), b.1.to_bits(), "note {}", a.0);
    }
}

#[test]
fn rows_round_trip_through_from_rows() {
    let table = materialize(&equal_spec("12-et", 12), &[]).unwrap();
    let (first, _) = table.note_range();
    let freq: Vec<f64> = table.rows().map(|(_, f)| f).collect();
    let reloaded = MaterializedTuning::from_rows(first, freq, table.note_per_period());
    assert_eq!(reloaded, table);
}

#[test]
fn equal_without_a_period_is_rejected() {
    let mut spec = equal_spec("broken", 12);
    spec.period = None;
    assert!(matches!(
        materialize(&spec, &[]),
        Err(CoreError::TuningIncomplete { .. })
    ));
}

#[test]
fn periodic_table_with_wrong_row_count_is_rejected() {
    let (spec, mut note) = just_5limit();
    note.pop();
    assert!(matches!(
        materialize(&spec, &note),
        Err(CoreError::TuningNoteCount {
            expected: 12,
            found: 11,
            ..
        })
    ));
}

#[test]
fn out_of_order_frequencies_are_rejected() {
    // A mistyped ratio surfaces here rather than as a silently wrong pitch.
    let (spec, mut note) = just_5limit();
    note[3].value = TuningNoteValue::Ratio(Ratio::new(1, 2));
    assert!(matches!(
        materialize(&spec, &note),
        Err(CoreError::TuningNotMonotone { .. })
    ));
}

#[test]
fn aperiodic_gaps_are_rejected() {
    let spec = TuningSpec::new("gappy", TuningKind::Table, 0, 100.0);
    let note = vec![
        TuningNote {
            note_number: NoteNumber(0),
            value: TuningNoteValue::Freq(100.0),
        },
        TuningNote {
            note_number: NoteNumber(2),
            value: TuningNoteValue::Freq(200.0),
        },
    ];
    assert!(matches!(
        materialize(&spec, &note),
        Err(CoreError::TuningNoteGap { .. })
    ));
}
