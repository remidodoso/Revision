//! The Player — our rendition of Vision's Tracks Window (ui-08).
//!
//! This module is the **view core**: the column model, the fake fixtures, and
//! the two painters (the track-list table and the Track Overview). It holds no
//! window and no engine — the `rev-player` binary mounts it in a window and
//! drives the transport. Everything here is drawable and testable without a
//! device.
//!
//! **A mixture, not a pure mockup** (ui-08 proposal): the transport genuinely
//! plays (the binary reuses rev-studio's compile+play), while the *content* here
//! — track rows and overview blocks — is a hand-built fixture. The fixture types
//! are shaped to what ui-09's store queries will hand the painters, so wiring is
//! a source swap, not a rewrite.
//!
//! **Two panes, one row grid.** The table (left) and the overview (right) share
//! one vertical row layout: a track's row and its overview lane are the same
//! height and the same y, and they scroll vertically together. Only the overview
//! scrolls horizontally (time). `ROW_H` and a shared vertical offset are the
//! whole of that coupling.

use rev_ui_kit::pane::Pane;
use rev_ui_mech::{Color, Painter, Point, Rect, TextStyle};

/// One track row's height, and one overview lane's height. The coupling between
/// the two panes is exactly this shared number (§4 of the proposal).
pub const ROW_H: f32 = 22.0;
/// The table header row and the overview ruler are both this tall, so the two
/// panes' content starts at the same y.
pub const HEAD_H: f32 = 20.0;

/// The x-range `(left, width)` of each R/M/S button within a Marker cell of the
/// given left and width — 2px padding and 2px gaps, so the three read as
/// individual buttons. Shared by the painter and the click hit-testing.
pub fn marker_slots(left: f32, width: f32) -> [(f32, f32); 3] {
    let (pad, gap) = (2.0, 2.0);
    let w = ((width - 2.0 * pad - 2.0 * gap) / 3.0).max(1.0);
    [0usize, 1, 2].map(|i| (left + pad + i as f32 * (w + gap), w))
}

// --- The column model -------------------------------------------------------

/// Which track-column a [`Column`] is. The set is fixed; which are shown, in
/// what order and width, is the user's (the column model, below).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnId {
    /// The selector/mute/solo/record dot cluster is drawn as one narrow column.
    Marker,
    Name,
    Len,
    Instrument,
    Patch,
}

/// One column of the track-list table: what it is, how wide, and whether shown.
/// Columns are **data** — reorderable, hideable, resizable — which is the whole
/// point of the widget (ui-08 §6).
#[derive(Debug, Clone)]
pub struct Column {
    pub id: ColumnId,
    pub header: &'static str,
    pub width: f32,
    pub min: f32,
    pub visible: bool,
}

/// The ordered set of columns. Order is the vector order; visibility is a flag;
/// width is per-column. All three are user-mutable.
#[derive(Debug, Clone)]
pub struct Columns {
    col: Vec<Column>,
}

impl Columns {
    /// The candidate set from Vision Ch. 25, our subset for the mockup. The
    /// Instrument column is the track's *optional default* routing hint (empty =
    /// "—"), not a track property (ui-08 §6).
    pub fn candidate() -> Columns {
        Columns {
            col: vec![
                Column {
                    id: ColumnId::Marker,
                    header: "",
                    width: 52.0,
                    min: 52.0,
                    visible: true,
                },
                Column {
                    id: ColumnId::Name,
                    header: "Track",
                    width: 130.0,
                    min: 60.0,
                    visible: true,
                },
                Column {
                    id: ColumnId::Len,
                    header: "Len",
                    width: 44.0,
                    min: 32.0,
                    visible: true,
                },
                Column {
                    id: ColumnId::Instrument,
                    header: "Instrument",
                    width: 120.0,
                    min: 60.0,
                    visible: true,
                },
                Column {
                    id: ColumnId::Patch,
                    header: "Patch",
                    width: 96.0,
                    min: 48.0,
                    visible: true,
                },
            ],
        }
    }

    /// The visible columns, in order.
    pub fn visible(&self) -> impl Iterator<Item = &Column> {
        self.col.iter().filter(|c| c.visible)
    }

    /// Every column, in order (for a show/hide menu).
    pub fn all(&self) -> &[Column] {
        &self.col
    }

    /// Total width of the visible columns — the table pane's fixed content width.
    pub fn width(&self) -> f32 {
        self.visible().map(|c| c.width).sum()
    }

    /// Show or hide a column by id. Hiding the last visible column is refused, so
    /// the table can never become an empty strip you cannot get back.
    pub fn toggle(&mut self, id: ColumnId) {
        let visible_count = self.col.iter().filter(|c| c.visible).count();
        if let Some(c) = self.col.iter_mut().find(|c| c.id == id) {
            if c.visible && visible_count == 1 {
                return;
            }
            c.visible = !c.visible;
        }
    }

    /// Move the column at `from` to `to`, shifting the rest — a header drag.
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.col.len() || to >= self.col.len() || from == to {
            return;
        }
        let c = self.col.remove(from);
        self.col.insert(to, c);
    }

    /// Resize a column, clamped to its minimum so a column can never vanish by
    /// dragging (hiding is the way to remove one).
    pub fn resize(&mut self, id: ColumnId, width: f32) {
        if let Some(c) = self.col.iter_mut().find(|c| c.id == id) {
            c.width = width.max(c.min);
        }
    }

    /// The column under a table-space x, with its left edge and width — for
    /// hit-testing a header click, a resize grip, or a cell.
    pub fn at_x(&self, x: f32) -> Option<(ColumnId, f32, f32)> {
        let mut left = 0.0;
        for c in self.visible() {
            if x >= left && x < left + c.width {
                return Some((c.id, left, c.width));
            }
            left += c.width;
        }
        None
    }
}

// --- The fixtures (shaped to ui-09's eventual queries) ----------------------

/// One track row. Mirrors a `Track` plus its routing/session view: the model
/// stays a pure agnostic container, so `instrument` here is the track's
/// *optional default* routing hint (ui-08 §6), and mute/solo are session state.
#[derive(Debug, Clone)]
pub struct TrackRow {
    pub name: String,
    /// The optional default instrument (routing hint). `None` renders "—".
    pub instrument: Option<String>,
    pub patch: Option<String>,
    /// Length in measures — a display value here.
    pub len: i32,
    pub looped: bool,
    pub muted: bool,
    pub soloed: bool,
    pub armed: bool,
}

/// What a block on the overview is. The kinds exist so the mockup can prove the
/// painter renders varied material — most importantly the **alias** case, a
/// block that references a structured phrase (the recursive-arrangement unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// A run of note material (drawn as a patterned block).
    Notes,
    /// A reference to another phrase — the recursive block, drawn distinctly.
    Alias,
    /// Continuous-controller data (drawn as an envelope band).
    Controller,
}

/// One block on the Track Overview. Mirrors a realized `PhraseInstance` on a
/// track: `at_tick`/`length` become beats, and an alias is an instance whose
/// referenced phrase is itself structured.
#[derive(Debug, Clone)]
pub struct Block {
    pub lane: usize,
    pub start_beat: f64,
    pub len_beats: f64,
    pub name: String,
    pub kind: BlockKind,
}

/// The demo's fake track rows — the "Verse" of the reference screenshot.
pub fn fixture_rows() -> Vec<TrackRow> {
    let row = |name: &str, inst: Option<&str>, patch: Option<&str>, len: i32| TrackRow {
        name: name.to_string(),
        instrument: inst.map(str::to_string),
        patch: patch.map(str::to_string),
        len,
        looped: false,
        muted: false,
        soloed: false,
        armed: false,
    };
    vec![
        row("Piano", Some("Wavestation"), Some("Grand 16'"), 24),
        row("Bass", Some("MicroWave"), Some("Moog O1"), 24),
        row("String Arpeggios", Some("Prophet 5"), Some("0/1/0"), 24),
        row("Drum Loop", Some("S1000 Kit"), Some("Standard"), 8),
        row("Volume data", None, None, 12),
        row("Pan data", None, None, 8),
    ]
}

/// The demo's fake overview blocks — deliberately varied so the painter is
/// exercised: held notes, alias/reference blocks, and a controller envelope.
pub fn fixture_blocks() -> Vec<Block> {
    let b = |lane, start, len, name: &str, kind| Block {
        lane,
        start_beat: start,
        len_beats: len,
        name: name.to_string(),
        kind,
    };
    vec![
        b(0, 0.0, 32.0, "", BlockKind::Notes),
        b(1, 0.0, 32.0, "", BlockKind::Notes),
        b(2, 8.0, 6.0, "Arp A", BlockKind::Alias),
        b(2, 16.0, 6.0, "Arp A", BlockKind::Alias),
        b(2, 24.0, 6.0, "Arp B", BlockKind::Alias),
        b(3, 0.0, 8.0, "Loop", BlockKind::Alias),
        b(3, 8.0, 8.0, "Loop", BlockKind::Alias),
        b(3, 16.0, 8.0, "Loop", BlockKind::Alias),
        b(4, 0.0, 32.0, "", BlockKind::Controller),
        b(5, 0.0, 24.0, "", BlockKind::Controller),
    ]
}

/// The value shown in a cell — kept next to the row so the painter has no policy.
fn cell_text(row: &TrackRow, id: ColumnId) -> String {
    match id {
        ColumnId::Marker => String::new(),
        ColumnId::Name => row.name.clone(),
        ColumnId::Len => {
            if row.looped {
                format!(":{}:", row.len)
            } else {
                row.len.to_string()
            }
        }
        ColumnId::Instrument => row.instrument.clone().unwrap_or_else(|| "—".to_string()),
        ColumnId::Patch => row.patch.clone().unwrap_or_else(|| "—".to_string()),
    }
}

// --- Painting ---------------------------------------------------------------

/// The player's ink. A handful of flat fills for the mockup; the full skin
/// treatment is a later pass (the block gradients etc.).
struct Ink {
    head: Color,
    /// One neutral background for every track/lane — tracks are told apart by the
    /// thin rules between them, not by candy-striping.
    track: Color,
    rule: Color,
    /// The vertical column dividers. Converged to the same weight as the
    /// horizontal `rule` — identical for now, kept a separate field so either can
    /// be nudged without disturbing the other.
    col_rule: Color,
    label: Color,
    dim: Color,
    grid: Color,
    grid_major: Color,
    notes: Color,
    alias: Color,
    alias_edge: Color,
    controller: Color,
    playhead: Color,
    sel: Color,
}

impl Ink {
    /// Dark mode, perked up: backgrounds lifted off near-black, rules and grid
    /// clearly visible, block kinds in three distinct hues (blue notes / amber
    /// references / violet controllers). Bright and distinctive, not garish.
    fn dark() -> Ink {
        Ink {
            head: Color::hex(0x2b3245),
            track: Color::hex(0x1b2130),
            rule: Color::hex(0x636d8a),
            col_rule: Color::hex(0x636d8a),
            label: Color::hex(0xe8ebf2),
            dim: Color::hex(0xacb4c6),
            grid: Color::hex(0x2b3345),
            grid_major: Color::hex(0x49577f),
            notes: Color::hex(0x74bbff),
            alias: Color::hex(0x3d3620),
            alias_edge: Color::hex(0xe3b552),
            controller: Color::hex(0x9370cc),
            playhead: Color::hex(0xff7d7d),
            sel: Color::hex(0x2a4670),
        }
    }
}

/// The y of lane/row `lane`'s top, given the shared vertical offset (in pixels)
/// and the content-area top (below the header/ruler).
fn row_top(lane: usize, content_top: f32, v_offset: f32) -> f32 {
    content_top + lane as f32 * ROW_H - v_offset
}

/// Draw `text` at `at`, truncating with an ellipsis if it would exceed `max_w` —
/// the standard HI behaviour for a cell narrower than its text (so a column's
/// text never bleeds into its neighbour). Drops whole characters from the end
/// and appends "…" until it fits.
fn draw_fit(p: &mut Painter, text: &str, style: &TextStyle, at: Point, max_w: f32, color: Color) {
    let full = p.shape(text, style);
    if full.size().w <= max_w {
        p.draw_text(&full, at, color);
        return;
    }
    let mut chars: Vec<char> = text.chars().collect();
    while chars.pop().is_some() {
        let mut candidate: String = chars.iter().collect();
        candidate.push('…');
        let shaped = p.shape(&candidate, style);
        if shaped.size().w <= max_w {
            p.draw_text(&shaped, at, color);
            return;
        }
    }
    let dots = p.shape("…", style);
    p.draw_text(&dots, at, color);
}

/// Paint the track-list table into `interior`: a header row, then the rows, with
/// cells laid out by the (visible, ordered) columns. `v_offset` is the shared
/// vertical scroll (pixels); `selected` highlights a row.
pub fn paint_table(
    rows: &[TrackRow],
    columns: &Columns,
    interior: Rect,
    v_offset: f32,
    selected: Option<usize>,
    p: &mut Painter,
) {
    let ink = Ink::dark();
    p.push_clip(interior);

    // Vertical column rules stop at the last populated row, not the pane floor —
    // dividers hanging in the empty space below the tracks read as clutter.
    let content_top = interior.y + HEAD_H;
    let rule_bottom =
        (content_top + rows.len() as f32 * ROW_H - v_offset).clamp(content_top, interior.bottom());

    // Header row.
    let head = Rect::new(interior.x, interior.y, interior.w, HEAD_H);
    p.fill_rect(head, ink.head);
    let mut x = interior.x;
    for c in columns.visible() {
        // The heading row is bold throughout.
        match c.id {
            // R / M / S column headings, centred over the three buttons. The rules
            // dividing them are drawn later, with the other verticals.
            ColumnId::Marker => {
                let slots = marker_slots(x, c.width);
                for (letter, &(x0, w)) in ["R", "M", "S"].into_iter().zip(slots.iter()) {
                    let shaped = p.shape(letter, &TextStyle::ui(11.0).bold());
                    let sz = shaped.size();
                    p.draw_text(
                        &shaped,
                        Point::new(x0 + (w - sz.w) / 2.0, interior.y + 3.0),
                        ink.label,
                    );
                }
            }
            _ if !c.header.is_empty() => {
                draw_fit(
                    p,
                    c.header,
                    &TextStyle::ui(12.0).bold(),
                    Point::new(x + 6.0, interior.y + 3.0),
                    c.width - 8.0,
                    ink.label,
                );
            }
            _ => {}
        }
        x += c.width;
    }
    p.stroke_line(
        Point::new(interior.x, interior.y + HEAD_H),
        Point::new(interior.right(), interior.y + HEAD_H),
        ink.rule,
        1.0,
    );

    // Rows.
    for (lane, row) in rows.iter().enumerate() {
        let top = row_top(lane, content_top, v_offset);
        if top + ROW_H < content_top || top > interior.bottom() {
            continue;
        }
        let rect = Rect::new(interior.x, top, interior.w, ROW_H);
        let bg = if selected == Some(lane) {
            ink.sel
        } else {
            ink.track
        };
        p.fill_rect(rect, bg);
        // A thin rule under each row divides the tracks (no candy-striping).
        p.stroke_line(
            Point::new(interior.x, top + ROW_H),
            Point::new(interior.right(), top + ROW_H),
            ink.rule,
            1.0,
        );

        let mut cx = interior.x;
        for c in columns.visible() {
            match c.id {
                ColumnId::Marker => paint_marker(row, Rect::new(cx, top, c.width, ROW_H), p),
                _ => {
                    let text = cell_text(row, c.id);
                    let color = if row.instrument.is_none() && c.id != ColumnId::Name {
                        ink.dim
                    } else {
                        ink.label
                    };
                    draw_fit(
                        p,
                        &text,
                        &TextStyle::ui(12.0),
                        Point::new(cx + 6.0, top + 4.0),
                        c.width - 8.0,
                        color,
                    );
                }
            }
            cx += c.width;
        }
    }

    // Vertical column rules, painted last so the row backgrounds cannot bury them
    // — a rule dividing two lanes has to sit above both. Before this pass the
    // rules were drawn under the header and then overpainted by every row fill, so
    // only the header band showed them. They span the populated height (header
    // included) in the brighter `col_rule`, with the R/M/S sub-columns dividing
    // their button gaps the same way.
    let mut vx = interior.x;
    for c in columns.visible() {
        if c.id == ColumnId::Marker {
            for &(x0, w) in marker_slots(vx, c.width).iter().take(2) {
                let rx = x0 + w + 1.0;
                p.stroke_line(
                    Point::new(rx, interior.y),
                    Point::new(rx, rule_bottom),
                    ink.col_rule,
                    1.0,
                );
            }
        }
        vx += c.width;
        p.stroke_line(
            Point::new(vx, interior.y),
            Point::new(vx, rule_bottom),
            ink.col_rule,
            1.0,
        );
    }
    p.pop_clip();
}

/// The Marker cell: three labelled R/M/S buttons. When a state is on the button
/// is a solid strong colour with black bold text; off, it is a dark button with
/// the colour as its letter — so on/off reads at a glance. Red record, amber
/// mute, cyan solo.
fn paint_marker(row: &TrackRow, cell: Rect, p: &mut Painter) {
    let slots = marker_slots(cell.x, cell.w);
    let states = [
        (row.armed, "R", Color::hex(0xd6483f)),
        (row.muted, "M", Color::hex(0xe0a52e)),
        (row.soloed, "S", Color::hex(0x33c2cc)),
    ];
    let pad_v = 2.0;
    for ((on, letter, color), (x0, w)) in states.into_iter().zip(slots) {
        let rect = Rect::new(x0, cell.y + pad_v, w, ROW_H - pad_v * 2.0);
        if on {
            // Lit: the strong colour with a black bold letter.
            p.fill_round_rect(rect, 2.0, color);
            let shaped = p.shape(letter, &TextStyle::ui(11.0).bold());
            let sz = shaped.size();
            p.draw_text(
                &shaped,
                Point::new(x0 + (w - sz.w) / 2.0, rect.y + (rect.h - sz.h) / 2.0),
                Color::rgb(0, 0, 0),
            );
        } else {
            // Off: an empty dark button, no letter.
            p.fill_round_rect(rect, 2.0, Color::hex(0x242a3a));
        }
    }
}

/// Paint the Track Overview into `interior`: a ruler, lane backgrounds aligned to
/// the shared row grid, and the blocks. `pane` supplies the horizontal
/// scroll/zoom (beats), `v_offset` the shared vertical scroll, `playhead` the
/// sweeping line in beats.
pub fn paint_overview(
    blocks: &[Block],
    lanes: usize,
    pane: &Pane,
    interior: Rect,
    v_offset: f32,
    playhead: Option<f64>,
    p: &mut Painter,
) {
    let ink = Ink::dark();
    p.push_clip(interior);

    let beat_to_x = |beat: f64| -> f32 {
        interior.x + ((beat - f64::from(pane.offset.x)) / f64::from(pane.scale.x)) as f32
    };
    let content_top = interior.y + HEAD_H;

    // Lane backgrounds (aligned to the table's rows): one neutral fill, divided
    // by a thin rule under each lane — the same treatment as the table.
    for lane in 0..lanes {
        let top = row_top(lane, content_top, v_offset);
        if top + ROW_H < content_top || top > interior.bottom() {
            continue;
        }
        p.fill_rect(Rect::new(interior.x, top, interior.w, ROW_H), ink.track);
        p.stroke_line(
            Point::new(interior.x, top + ROW_H),
            Point::new(interior.right(), top + ROW_H),
            ink.rule,
            1.0,
        );
    }

    // Beat grid + ruler.
    let span = f64::from(interior.w) * f64::from(pane.scale.x);
    // Minor lines at each beat, when beats are wide enough to be worth drawing.
    if f64::from(pane.scale.x) < 0.5 {
        let mut minor = f64::from(pane.offset.x).floor().max(0.0);
        while minor <= f64::from(pane.offset.x) + span {
            let x = beat_to_x(minor);
            p.stroke_line(
                Point::new(x, content_top),
                Point::new(x, interior.bottom()),
                ink.grid,
                1.0,
            );
            minor += 1.0;
        }
    }
    let step = 4.0f64; // a bar; the ruler numbers bars
    let from = (f64::from(pane.offset.x) / step).floor() * step;
    let mut beat = from.max(0.0);
    while beat <= f64::from(pane.offset.x) + span {
        let x = beat_to_x(beat);
        p.stroke_line(
            Point::new(x, content_top),
            Point::new(x, interior.bottom()),
            ink.grid_major,
            1.0,
        );
        let label = format!("{}", (beat / 4.0) as i64 + 1);
        let shaped = p.shape(&label, &TextStyle::numeric(10.0));
        p.draw_text(&shaped, Point::new(x + 3.0, interior.y + 4.0), ink.dim);
        beat += step;
    }
    p.stroke_line(
        Point::new(interior.x, content_top),
        Point::new(interior.right(), content_top),
        ink.rule,
        1.0,
    );

    // Blocks.
    for block in blocks {
        let top = row_top(block.lane, content_top, v_offset) + 2.0;
        if top + ROW_H < content_top || top > interior.bottom() {
            continue;
        }
        let x = beat_to_x(block.start_beat);
        let w = (block.len_beats / f64::from(pane.scale.x)) as f32;
        if x + w < interior.x || x > interior.right() {
            continue;
        }
        let rect = Rect::new(x, top, w.max(2.0), ROW_H - 4.0);
        match block.kind {
            BlockKind::Notes => {
                p.fill_round_rect(rect, 2.0, Color::rgba(116, 187, 255, 95));
                p.stroke_line(
                    Point::new(x, top + (ROW_H - 4.0) / 2.0),
                    Point::new(x + w, top + (ROW_H - 4.0) / 2.0),
                    ink.notes,
                    1.5,
                );
            }
            BlockKind::Controller => {
                p.fill_round_rect(rect, 2.0, ink.controller);
            }
            BlockKind::Alias => {
                p.fill_round_rect(rect, 3.0, ink.alias);
                let shaped = p.shape(&block.name, &TextStyle::ui(11.0));
                p.draw_text(&shaped, Point::new(x + 4.0, top + 3.0), ink.label);
                // A distinct edge marks it as a reference to another phrase.
                p.stroke_line(
                    Point::new(x, top),
                    Point::new(x, top + ROW_H - 4.0),
                    ink.alias_edge,
                    2.0,
                );
            }
        }
    }

    // Playhead.
    if let Some(beat) = playhead {
        let x = beat_to_x(beat);
        if x >= interior.x && x <= interior.right() {
            p.stroke_line(
                Point::new(x, interior.y),
                Point::new(x, interior.bottom()),
                ink.playhead,
                1.0,
            );
        }
    }

    p.pop_clip();
}

#[cfg(test)]
mod test;
