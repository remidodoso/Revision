//! Player column-model tests (ui-08). The painters are eyeballed via `rev-player`
//! (getstarted stage-4 rule); the *logic* — reorder, show/hide, resize, layout —
//! is unit-tested here.

use super::{ColumnId, Columns, fixture_blocks, fixture_rows};

#[test]
fn the_candidate_set_is_all_visible_in_order() {
    let cols = Columns::candidate();
    let order: Vec<ColumnId> = cols.visible().map(|c| c.id).collect();
    assert_eq!(
        order,
        vec![
            ColumnId::Marker,
            ColumnId::Name,
            ColumnId::Len,
            ColumnId::Instrument,
            ColumnId::Patch
        ]
    );
}

#[test]
fn hiding_a_column_removes_it_from_the_visible_order() {
    let mut cols = Columns::candidate();
    cols.toggle(ColumnId::Patch);
    assert!(cols.visible().all(|c| c.id != ColumnId::Patch));
    // Still present in the full list (for a show/hide menu), just not visible.
    assert!(
        cols.all()
            .iter()
            .any(|c| c.id == ColumnId::Patch && !c.visible)
    );
}

#[test]
fn the_last_visible_column_cannot_be_hidden() {
    let mut cols = Columns::candidate();
    for id in [
        ColumnId::Marker,
        ColumnId::Len,
        ColumnId::Instrument,
        ColumnId::Patch,
    ] {
        cols.toggle(id);
    }
    // Only Name remains; hiding it is refused so the table cannot go empty.
    cols.toggle(ColumnId::Name);
    assert_eq!(cols.visible().count(), 1);
}

#[test]
fn reorder_moves_a_column_and_shifts_the_rest() {
    let mut cols = Columns::candidate();
    // Move Instrument (index 3) to the front (index 0).
    cols.reorder(3, 0);
    let order: Vec<ColumnId> = cols.visible().map(|c| c.id).collect();
    assert_eq!(order[0], ColumnId::Instrument);
    assert_eq!(order[1], ColumnId::Marker);
}

#[test]
fn resize_clamps_to_the_minimum() {
    let mut cols = Columns::candidate();
    cols.resize(ColumnId::Name, 5.0);
    let name = cols.all().iter().find(|c| c.id == ColumnId::Name).unwrap();
    assert_eq!(
        name.width, name.min,
        "a column cannot be dragged below its minimum"
    );
}

#[test]
fn width_is_the_sum_of_visible_columns() {
    let mut cols = Columns::candidate();
    let full = cols.width();
    let patch = cols
        .all()
        .iter()
        .find(|c| c.id == ColumnId::Patch)
        .unwrap()
        .width;
    cols.toggle(ColumnId::Patch);
    assert_eq!(
        cols.width(),
        full - patch,
        "hiding a column narrows the table by its width"
    );
}

#[test]
fn at_x_hit_tests_columns_by_position() {
    let cols = Columns::candidate();
    // Marker is 52 wide, then Name — so x=60 lands in Name at left edge 52.
    let (id, left, _w) = cols.at_x(60.0).expect("inside the table");
    assert_eq!(id, ColumnId::Name);
    assert_eq!(left, 52.0);
    // A hit inside the Marker column resolves to it.
    assert_eq!(cols.at_x(10.0).map(|(id, _, _)| id), Some(ColumnId::Marker));
    // Far past the right edge is nothing.
    assert!(cols.at_x(10_000.0).is_none());
}

#[test]
fn the_fixtures_are_shaped_for_the_painters() {
    let rows = fixture_rows();
    assert_eq!(rows.len(), 6);
    // "Volume data" / "Pan data" are instrument-less control tracks (the dim
    // "—" case) — a track is a pure agnostic container.
    assert!(rows.iter().any(|r| r.instrument.is_none()));
    // Blocks reference at least one lane per row, including alias blocks.
    let blocks = fixture_blocks();
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b.kind, super::BlockKind::Alias))
    );
    assert!(blocks.iter().all(|b| b.lane < rows.len()));
}
