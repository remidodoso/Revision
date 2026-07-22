//! `rev-player` — the drivable Player + Control Bar workspace mockup (ui-08).
//!
//! Opens a **Control Bar palette** and one or more **Player** windows (our
//! rendition of Vision's Tracks Window) and lets you drive the workspace. It is a
//! **mixture**: the transport genuinely works (each Player is backed by a real
//! fixture phrase it compiles and plays), while the Player's *content* — the
//! track rows and the overview blocks — is the fake fixture from `rev_app::player`.
//!
//! What you can do:
//!   space          play / stop the FOCUSED Player (the current Playable)
//!   n              open another Player window
//!   p              pin the transport to the focused Player (edit-while-playing)
//!   1..5           show / hide the track columns (prove the column model)
//!   drag divider   move the split between the columns and the overview
//!   drag col edge  resize a column
//!   click a row    select it
//!   wheel          scroll the overview (ctrl = zoom time)
//!
//! The Control Bar is a `WindowRole::Palette`, so operating it never steals
//! front from the Player you are working in (R-907) — which is what keeps
//! "space plays the focused Player" stable.

use std::time::Instant;

use rev_app::audio::Audio;
use rev_app::mhall::build;
use rev_app::player::{
    Block, ColumnId, Columns, HEAD_H, ROW_H, TrackRow, fixture_blocks, fixture_rows, marker_slots,
    paint_overview, paint_table,
};
use rev_core::PhraseId;
use rev_core::TrackId;
use rev_core::tick::Tick;
use rev_dsp::BakeSpec;
use rev_engine::driver::Request;
use rev_engine::{Chunk, ChunkHandle, Patch, Position, SampleTime, What};
use rev_log::{Log, creator};
use rev_sched::{Compiler, TempoMap};
use rev_store::{Project, query};
use rev_ui_kit::pane::{Pane, Scale};
use rev_ui_kit::{Field, Intent, Kind, Kit, RecordMode, Skin, Widget, WidgetId};
use rev_ui_mech::{
    Color, CursorShape, Event, Frame, Host, KeyCode, Mech, Named, Notice, Point, PointerKind, Rect,
    Size, TargetId, TextStyle, Tree, WindowId, WindowRole, WindowSpec,
};

const REC: WidgetId = WidgetId(1);
const PLAY: WidgetId = WidgetId(2);
const STOP: WidgetId = WidgetId(3);
const COUNTER: WidgetId = WidgetId(4);

/// The current edit tool — the Arrow/Marquee/I-beam palette. Visual state only
/// in the mockup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Arrow,
    Marquee,
    Ibeam,
}

/// A manual pointer gesture in a Player window (the kit does not model these).
#[derive(Debug, Clone, Copy)]
enum Drag {
    Splitter,
    /// Resizing a column. `start_width` is the width at grab time, so the column
    /// can snap back to it while the pointer strays off the heading band — the
    /// scroll-box "dragging out" tracking rule (HIG ch. 5, "Windows", p. 165),
    /// which the book calls "standard behavior for controls in general."
    Column {
        id: ColumnId,
        start_width: f32,
    },
}

/// One Player window: our Tracks Window, fully app-painted, backed by a real
/// fixture phrase so its transport actually sounds.
struct Player {
    window: WindowId,
    title: String,
    columns: Columns,
    rows: Vec<TrackRow>,
    blocks: Vec<Block>,
    /// The overview's horizontal geometry (time). Vertical scroll is `v_offset`,
    /// shared with the table.
    overview: Pane,
    v_offset: f32,
    split_x: f32,
    tool: Tool,
    selected: Option<usize>,
    size: Size,
    drag: Option<Drag>,
    /// Slot and time of the last R/M/S heading click, to spot a double-click.
    header_click: Option<(usize, f64)>,

    // --- transport (its own, per Playable)
    project: Project,
    arrangement: PhraseId,
    track: TrackId,
    bpm: f64,
    playing: bool,
    playhead: f64,
    /// Whether the last painted frame showed this Player playing. When it flips
    /// false we owe one repaint to wipe the frozen playhead — otherwise stopping
    /// from the Control Bar (which repaints the bar, not the Players) leaves a
    /// ghost line behind.
    shown_playing: bool,
}

impl Player {
    /// The y where the two panes begin (below the header strip).
    fn body_top(&self) -> f32 {
        8.0 + 26.0 // a header strip band, then the panes
    }

    /// Compile this Player's fixture phrase into a chunk for the engine.
    fn chunk(&self, rate: u32) -> Option<Chunk> {
        let point: Vec<(Tick, i64)> = query::tempo_point(self.project.reader(), self.arrangement)
            .ok()?
            .into_iter()
            .map(|p| (p.at_tick, p.usec_per_quarter))
            .collect();
        let mut compiler = Compiler::new(TempoMap::new(point, rate), vec![self.track]);
        let seconds = 40.0;
        let frames = (seconds * f64::from(rate)) as u64;
        compiler
            .chunk(&self.project, SampleTime(0), SampleTime(frames))
            .ok()
    }

    fn beat_at(&self, sample: u64, rate: u32) -> f64 {
        sample as f64 / f64::from(rate) * (self.bpm / 60.0)
    }
}

/// The Control Bar — a palette window with a real, working transport.
struct Bar {
    /// `None` until `start` opens it.
    window: Option<WindowId>,
    kit: Kit,
}

fn bar_scene(size: Size) -> Widget {
    Widget::new(0, Kind::Panel, "", Rect::new(0.0, 0.0, size.w, size.h)).with_child(vec![
        Widget::new(
            REC.0,
            Kind::Record {
                mode: RecordMode::Off,
            },
            "Record",
            Rect::new(10.0, 8.0, 84.0, 28.0),
        ),
        Widget::new(
            PLAY.0,
            Kind::Button,
            "Play",
            Rect::new(102.0, 8.0, 60.0, 28.0),
        ),
        Widget::new(
            STOP.0,
            Kind::Button,
            "Stop",
            Rect::new(168.0, 8.0, 60.0, 28.0),
        ),
        Widget::new(
            COUNTER.0,
            Kind::Counter {
                field: vec![Field::new(1, 3, 1, 9999), Field::new(1, 1, 1, 4)],
                separator: '|',
            },
            "Counter",
            Rect::new(240.0, 8.0, 140.0, 28.0),
        ),
    ])
}

struct App {
    players: Vec<Player>,
    bar: Bar,
    audio: Audio,
    log: Log,
    /// Which Player is frontmost (the current Playable). `None` before any focus.
    focused: Option<usize>,
    /// A pinned transport target overrides focus (edit one, hear another).
    pinned: Option<usize>,
    started: Instant,
    /// When the last animated frame was presented, to pace continuous repaints
    /// (the playing playhead) instead of free-running the loop as fast as it can.
    last_frame: Instant,
    next_bpm: f64,
    next_tuning: usize,
}

impl App {
    fn player_ix(&self, window: WindowId) -> Option<usize> {
        self.players.iter().position(|p| p.window == window)
    }

    fn is_bar(&self, w: WindowId) -> bool {
        self.bar.window == Some(w)
    }

    /// The current transport target: the pin if set, else the focused Player,
    /// else the first — so space always plays *something*.
    fn target(&self) -> Option<usize> {
        self.pinned.or(self.focused).or(if self.players.is_empty() {
            None
        } else {
            Some(0)
        })
    }

    fn stop_all(&mut self) {
        for p in &mut self.players {
            p.playing = false;
        }
        self.audio.send(What::AllNotesOff);
        self.audio.send(What::Stop);
    }

    /// Play the target Player's phrase from its own playhead. Switching target
    /// stops whatever was sounding; the old Player keeps its playhead.
    fn play_target(&mut self) {
        let Some(ix) = self.target() else { return };
        let rate = self.audio.sample_rate();
        for (j, p) in self.players.iter_mut().enumerate() {
            if j != ix {
                p.playing = false;
            }
        }
        let Some(chunk) = self.players[ix].chunk(rate) else {
            self.log.warn(creator::APP, "player: nothing to compile");
            return;
        };
        let head_sample = {
            let p = &self.players[ix];
            (p.playhead / (p.bpm / 60.0) * f64::from(rate)) as u64
        };
        self.audio.send(What::TakeChunk(ChunkHandle::new(Chunk {
            from: chunk.from,
            to: chunk.to,
            note: chunk.note,
        })));
        self.audio.send(What::Locate(SampleTime(head_sample)));
        self.audio.send(What::Start);
        self.players[ix].playing = true;
        self.set_light(RecordMode::Off);
        self.log.info(
            creator::APP,
            format!("playing {} (target)", self.players[ix].title),
        );
    }

    fn toggle_play(&mut self) {
        let anyone_playing = self.players.iter().any(|p| p.playing);
        if anyone_playing {
            self.stop_all();
        } else {
            self.play_target();
        }
    }

    fn set_light(&mut self, mode: RecordMode) {
        if let Some(Kind::Record { mode: m }) = self.bar.kit.kind_mut(REC) {
            *m = mode;
        }
    }

    /// Open a new Player window backed by a fresh fixture phrase, so several
    /// Players sound distinct (the point of "space plays the focused one").
    fn open_player(&mut self, mech: &mut Mech) {
        let tunings = ["12-ET", "16-ET", "Just (5-limit)"];
        let tuning = tunings[self.next_tuning % tunings.len()];
        self.next_tuning += 1;
        let bpm = self.next_bpm;
        self.next_bpm += 12.0;

        let dir = std::env::temp_dir().join(format!(
            "revision_player_{}_{}",
            std::process::id(),
            self.players.len()
        ));
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let mut project = match Project::create(dir.join("player.revision")) {
            Ok(p) => p,
            Err(e) => {
                self.log
                    .error(creator::APP, format!("player: no project: {e}"));
                return;
            }
        };
        let (arrangement, track) = match build(&mut project, bpm, tuning) {
            Ok(t) => t,
            Err(e) => {
                self.log
                    .error(creator::APP, format!("player: no fixture: {e}"));
                return;
            }
        };

        let n = self.players.len() + 1;
        // Embiggened 50% via the per-window interface scale (R-938): everything
        // in a Player renders 1.5x, so the smallest text (was ~10-12 logical px)
        // clears the 14 px legibility floor. The window opens 1.5x larger so the
        // same content still fits. This is the one knob to tune the whole scale.
        const PLAYER_SCALE: f32 = 1.5;
        let window = mech.open_window(WindowSpec {
            title: format!("Player {n} — {tuning}"),
            size: Size::new(1000.0 * PLAYER_SCALE, 560.0 * PLAYER_SCALE),
            role: WindowRole::Document,
            resizable: true,
            scale: Some(PLAYER_SCALE),
        });
        self.players.push(Player {
            window,
            title: format!("Player {n}"),
            columns: Columns::candidate(),
            rows: fixture_rows(),
            blocks: fixture_blocks(),
            overview: Pane {
                extent: Size::new(48.0, 8.0),
                scale: Scale { x: 0.08, y: 1.0 },
                scale_min: 0.01,
                scale_max: 1.0,
                bar: rev_ui_kit::pane::BarPolicy::None,
                ..Pane::default()
            },
            v_offset: 0.0,
            split_x: 388.0,
            tool: Tool::Arrow,
            selected: None,
            size: Size::new(1000.0, 560.0),
            drag: None,
            header_click: None,
            project,
            arrangement,
            track,
            bpm,
            playing: false,
            playhead: 0.0,
            shown_playing: false,
        });
    }

    // --- painting a Player window ------------------------------------------

    fn paint_player(&mut self, ix: usize, frame: &mut Frame<'_>) {
        let bg = Color::hex(0x14171f);
        frame.paint.clear(bg);
        let p = &self.players[ix];
        let size = frame.size;

        // Header strip (fake readouts, real-ish layout).
        let head = Rect::new(0.0, 0.0, size.w, 30.0);
        frame.paint.fill_rect(head, Color::hex(0x252c3e));
        let readout = |paint: &mut rev_ui_mech::Painter, x: f32, label: &str, val: &str| {
            let l = paint.shape(label, &TextStyle::ui(10.0));
            paint.draw_text(&l, Point::new(x, 4.0), Color::hex(0xacb4c6));
            let v = paint.shape(val, &TextStyle::numeric(12.0));
            paint.draw_text(&v, Point::new(x, 14.0), Color::hex(0xffcf6a));
        };
        readout(&mut frame.paint, 10.0, "Meter", "4/4");
        readout(&mut frame.paint, 70.0, "Tempo", &format!("{:.0}", p.bpm));
        readout(&mut frame.paint, 140.0, "Seq Len", "24");
        // The tool palette (visual).
        for (i, (tool, glyph)) in [
            (Tool::Arrow, "\u{2196}"),
            (Tool::Marquee, "\u{2b1a}"),
            (Tool::Ibeam, "I"),
        ]
        .into_iter()
        .enumerate()
        {
            let x = size.w - 120.0 + i as f32 * 30.0;
            let on = p.tool == tool;
            frame.paint.fill_round_rect(
                Rect::new(x, 5.0, 26.0, 20.0),
                3.0,
                if on {
                    Color::hex(0x3a5990)
                } else {
                    Color::hex(0x2e3648)
                },
            );
            let g = frame.paint.shape(glyph, &TextStyle::ui(12.0));
            frame
                .paint
                .draw_text(&g, Point::new(x + 8.0, 7.0), Color::hex(0xe8ebf2));
        }

        // The two panes.
        let body = Rect::new(0.0, p.body_top(), size.w, size.h - p.body_top() - 20.0);
        let split = p.split_x.clamp(120.0, size.w - 160.0);
        let table_rect = Rect::new(body.x, body.y, split, body.h);
        let overview_rect = Rect::new(split + 4.0, body.y, body.x + body.w - split - 4.0, body.h);

        paint_table(
            &p.rows,
            &p.columns,
            table_rect,
            p.v_offset,
            p.selected,
            &mut frame.paint,
        );
        let playhead = if p.playing { Some(p.playhead) } else { None };
        paint_overview(
            &p.blocks,
            p.rows.len(),
            &p.overview,
            overview_rect,
            p.v_offset,
            playhead,
            &mut frame.paint,
        );

        // The splitter.
        frame
            .paint
            .fill_rect(Rect::new(split, body.y, 4.0, body.h), Color::hex(0x4a5268));

        // Status line.
        let target = self.target() == Some(ix);
        let pinned = self.pinned == Some(ix);
        let line = format!(
            "space: play/stop · n: new · p: pin · 1-5: columns · {} {}",
            if target { "[transport target]" } else { "" },
            if pinned { "[pinned]" } else { "" },
        );
        let s = frame.paint.shape(&line, &TextStyle::ui(11.0));
        frame
            .paint
            .draw_text(&s, Point::new(8.0, size.h - 16.0), Color::hex(0xacb4c6));

        // A distinct window edge so the window is easy to find and grab against a
        // sea of other dark windows — brighter when this Player is the transport
        // target, which doubles as a focus cue.
        let edge = if target {
            Color::hex(0x8aa2d6)
        } else {
            Color::hex(0x545c74)
        };
        let t = 2.0;
        frame.paint.fill_rect(Rect::new(0.0, 0.0, size.w, t), edge);
        frame
            .paint
            .fill_rect(Rect::new(0.0, size.h - t, size.w, t), edge);
        frame.paint.fill_rect(Rect::new(0.0, 0.0, t, size.h), edge);
        frame
            .paint
            .fill_rect(Rect::new(size.w - t, 0.0, t, size.h), edge);
    }

    /// The cursor for a pointer position in a Player — a resize cursor over the
    /// splitter or a column edge, so a draggable divider announces itself (#3).
    fn player_cursor(&self, ix: usize, at: Point) -> CursorShape {
        let p = &self.players[ix];
        if matches!(p.drag, Some(Drag::Splitter) | Some(Drag::Column { .. })) {
            return CursorShape::ResizeHorizontal;
        }
        let (split, body_top) = (p.split_x, p.body_top());
        let near_split = (at.x - split).abs() <= SPLIT_GRIP && at.y >= body_top;
        if near_split || column_edge(&p.columns, at, split, body_top).is_some() {
            return CursorShape::ResizeHorizontal;
        }
        CursorShape::Default
    }

    // --- pointer in a Player window ----------------------------------------

    /// Handle a pointer in a Player. Returns whether anything *changed* — a pure
    /// hover returns false, so we do not queue a redraw for it. That matters: a
    /// redraw re-runs `apply_cursor` with a stale Default and would undo the
    /// resize cursor we just set (the #3 instability).
    fn player_pointer(&mut self, ix: usize, pt: &rev_ui_mech::Pointer) -> bool {
        let p = &mut self.players[ix];
        let body_top = p.body_top();
        let split = p.split_x;
        match pt.kind {
            PointerKind::Down => {
                if (pt.at.x - split).abs() <= SPLIT_GRIP && pt.at.y >= body_top {
                    p.drag = Some(Drag::Splitter);
                } else if let Some(id) = column_edge(&p.columns, pt.at, split, body_top) {
                    // A column boundary, grabbable within the heading band.
                    let start_width = column_left(&p.columns, id).map_or(0.0, |(_, _, w)| w);
                    p.drag = Some(Drag::Column { id, start_width });
                } else if let Some(which) = header_marker_hit(&p.columns, pt.at, body_top) {
                    // Double-clicking an R/M/S heading clears that state on every
                    // track ("all mutes off", "all solos off", …); a single click
                    // just records the time so a second within the interval reads as
                    // the double-click.
                    let now = pt.time.0;
                    if matches!(p.header_click, Some((w, t)) if w == which && now - t <= DOUBLE_CLICK)
                    {
                        clear_rms(&mut p.rows, which);
                        p.header_click = None;
                    } else {
                        p.header_click = Some((which, now));
                    }
                } else if pt.at.x < split && pt.at.y >= body_top + HEAD_H {
                    let lane = ((pt.at.y - body_top - HEAD_H + p.v_offset) / ROW_H) as usize;
                    if lane < p.rows.len() {
                        // An R/M/S button, or the row itself?
                        if let Some(which) = dot_hit(&p.columns, pt.at.x) {
                            toggle_rms(&mut p.rows, lane, which);
                        } else {
                            p.selected = Some(lane);
                        }
                    }
                }
                true
            }
            PointerKind::Move => match p.drag {
                Some(Drag::Splitter) => {
                    p.split_x = pt.at.x.clamp(120.0, p.size.w - 160.0);
                    true
                }
                Some(Drag::Column { id, start_width }) => {
                    if let Some((_, left, _)) = column_left(&p.columns, id) {
                        // Track the pointer while it stays within the heading band;
                        // stray above or below it — down into the tracks, say — and
                        // the column snaps back to its grab width, resuming the moment
                        // the pointer returns (HIG p. 165 — a suspend, not a cancel).
                        let (band_top, band_bottom) = heading_band(body_top);
                        let in_band = pt.at.y >= band_top && pt.at.y <= band_bottom;
                        let width = if in_band { pt.at.x - left } else { start_width };
                        p.columns.resize(id, width);
                    }
                    true
                }
                None => false, // a pure hover changes nothing
            },
            PointerKind::Up => {
                let had = p.drag.is_some();
                p.drag = None;
                had
            }
            PointerKind::Wheel { dx, dy } => {
                if pt.at.x > split {
                    if pt.modifier.ctrl {
                        let f = if dy > 0.0 { 0.9 } else { 1.0 / 0.9 };
                        p.overview.scale.x = (p.overview.scale.x * f)
                            .clamp(p.overview.scale_min, p.overview.scale_max);
                    } else {
                        p.overview.offset.x = (p.overview.offset.x
                            - dx * p.overview.scale.x
                            - dy * p.overview.scale.x)
                            .max(0.0);
                    }
                } else {
                    p.v_offset = (p.v_offset - dy).max(0.0);
                }
                true
            }
            _ => false,
        }
    }
}

/// The left edge (and width) of a column by id, in table space.
fn column_left(cols: &Columns, id: ColumnId) -> Option<(ColumnId, f32, f32)> {
    let mut left = 0.0;
    for c in cols.visible() {
        if c.id == id {
            return Some((id, left, c.width));
        }
        left += c.width;
    }
    None
}

/// How close (px) a pointer must be to the splitter / a column edge to grab it.
/// A couple of pixels each so the target is easy to hit.
const SPLIT_GRIP: f32 = 7.0;
const COL_GRIP: f32 = 6.0;
/// The vertical slop above and below the column heading — the only region the
/// resize gesture lives in. It bounds both where the edge is grabbable and how
/// far the pointer may stray mid-drag before the column snaps back to its grab
/// width ("a little more than the width of the scroll box", HIG p. 165). Kept
/// tight, so leaving the heading springs the column back promptly.
const SNAP_TOL: f32 = 8.0;

/// The vertical band (top, bottom) the column-resize gesture occupies: the
/// heading row grown by `SNAP_TOL` on each side. Both the initial grab and the
/// mid-drag snap-back test against this one band, so they can never disagree.
fn heading_band(body_top: f32) -> (f32, f32) {
    (body_top - SNAP_TOL, body_top + HEAD_H + SNAP_TOL)
}

/// Interval (seconds) within which two clicks on the same R/M/S heading count as
/// a double-click. The platform's real value is a user setting we can't read
/// here, so this is a conventional default.
const DOUBLE_CLICK: f64 = 0.4;

/// The column whose right edge a pointer is grabbing. The gesture belongs to the
/// column *heading*, not the tracks below it: active only within the heading row
/// plus a little vertical slop on either side (`SNAP_TOL`). Below the heading a
/// click is for the cells, so the edge is not grabbable there.
fn column_edge(cols: &Columns, at: Point, split: f32, body_top: f32) -> Option<ColumnId> {
    let (band_top, band_bottom) = heading_band(body_top);
    if at.x >= split || at.y < band_top || at.y > band_bottom {
        return None;
    }
    let mut left = 0.0f32;
    for c in cols.visible() {
        let right = left + c.width;
        if (at.x - right).abs() <= COL_GRIP {
            return Some(c.id);
        }
        left = right;
    }
    None
}

/// Which record/mute/solo button (0=R, 1=M, 2=S) a table-space x lands on, if any.
fn dot_hit(cols: &Columns, x: f32) -> Option<usize> {
    let (_, left, width) = column_left(cols, ColumnId::Marker)?;
    marker_slots(left, width)
        .iter()
        .position(|&(x0, w)| x >= x0 && x < x0 + w)
}

/// Toggle a track's record/mute/solo. All three are independent toggles — record
/// mode is **not** mutually exclusive here; several tracks may be armed at once.
fn toggle_rms(rows: &mut [TrackRow], lane: usize, which: usize) {
    match which {
        0 => rows[lane].armed = !rows[lane].armed,
        1 => rows[lane].muted = !rows[lane].muted,
        2 => rows[lane].soloed = !rows[lane].soloed,
        _ => {}
    }
}

/// Which R/M/S heading (0=R, 1=M, 2=S) a point falls on, if it is in the header
/// row and over one of the three heading letters.
fn header_marker_hit(cols: &Columns, at: Point, body_top: f32) -> Option<usize> {
    if at.y < body_top || at.y > body_top + HEAD_H {
        return None;
    }
    let (_, left, width) = column_left(cols, ColumnId::Marker)?;
    marker_slots(left, width)
        .iter()
        .position(|&(x0, w)| at.x >= x0 && at.x < x0 + w)
}

/// Clear one of record/mute/solo across every track — the header double-click
/// ("all record off", "all mutes off", "all solos off").
fn clear_rms(rows: &mut [TrackRow], which: usize) {
    for r in rows.iter_mut() {
        match which {
            0 => r.armed = false,
            1 => r.muted = false,
            2 => r.soloed = false,
            _ => {}
        }
    }
}

impl Host for App {
    fn start(&mut self, mech: &mut Mech) {
        self.bar.window = Some(mech.open_window(WindowSpec {
            title: String::from("Control Bar"),
            size: Size::new(400.0, 46.0),
            role: WindowRole::Palette,
            resizable: false,
            scale: None,
        }));
        self.open_player(mech);
        self.open_player(mech);
    }

    fn notice(&mut self, window: WindowId, notice: &Notice, mech: &mut Mech) {
        match notice {
            Notice::CloseRequested => {
                if self.is_bar(window) || self.players.len() <= 1 {
                    mech.exit();
                } else {
                    mech.close_window(window);
                    if let Some(ix) = self.player_ix(window) {
                        self.players.remove(ix);
                        self.focused = None;
                    }
                }
            }
            Notice::Resized(size) => {
                if let Some(ix) = self.player_ix(window) {
                    self.players[ix].size = *size;
                } else if self.is_bar(window) {
                    self.bar.kit.layout(Rect::new(0.0, 0.0, size.w, size.h));
                }
                mech.mark_dirty_all(window);
            }
            Notice::FocusChanged(gained) => {
                // Only a Player (a Document) becomes the transport target; the
                // palette never does (R-907).
                if *gained && let Some(ix) = self.player_ix(window) {
                    self.focused = Some(ix);
                }
                // Repaint EVERY Player: the target moved, so the window that was
                // the target must redraw to drop its bright border.
                for p in &self.players {
                    mech.mark_dirty_all(p.window);
                }
            }
            _ => {}
        }
    }

    fn hit(&self, window: WindowId, at: Point) -> Option<TargetId> {
        if self.is_bar(window) {
            self.bar.kit.hit(at)
        } else {
            None
        }
    }

    fn a11y(&self, window: WindowId) -> Tree {
        if self.is_bar(window) {
            self.bar.kit.a11y()
        } else {
            Tree::default()
        }
    }

    fn event(&mut self, window: WindowId, target: Option<TargetId>, ev: &Event, mech: &mut Mech) {
        // Keyboard — space and the workspace keys, from any window.
        if let Event::Key(key) = ev
            && key.pressed
        {
            match key.code {
                KeyCode::Named(Named::Space) => self.toggle_play(),
                KeyCode::Char('n') | KeyCode::Char('N') => self.open_player(mech),
                KeyCode::Char('p') | KeyCode::Char('P') => {
                    self.pinned = match self.pinned {
                        Some(_) => None,
                        None => self.focused,
                    };
                }
                KeyCode::Char(c @ '1'..='5') => {
                    if let Some(ix) = self.player_ix(window) {
                        let col = [
                            ColumnId::Marker,
                            ColumnId::Name,
                            ColumnId::Len,
                            ColumnId::Instrument,
                            ColumnId::Patch,
                        ][(c as u8 - b'1') as usize];
                        self.players[ix].columns.toggle(col);
                    }
                }
                _ => {}
            }
            mech.mark_dirty_all(window);
        }

        // Pointer — the Control Bar routes to its kit; a Player is manual.
        if self.is_bar(window) {
            if let Some((id, intent)) = self.bar.kit.event(target, ev) {
                match (id, intent) {
                    (PLAY, Intent::Released) => self.play_target(),
                    (STOP, Intent::Released) => self.stop_all(),
                    (REC, Intent::RecordPressed(_)) => {} // arm is ui-04; a no-op here
                    _ => {}
                }
            }
            mech.mark_dirty_all(window);
        } else if let (Some(ix), Event::Pointer(pt)) = (self.player_ix(window), ev) {
            let changed = self.player_pointer(ix, pt);
            mech.request_cursor(self.player_cursor(ix, pt.at));
            // Only repaint on a real change — a hover must not queue a redraw, or
            // the redraw's apply_cursor would clobber the resize cursor (#3).
            if changed {
                mech.mark_dirty_all(window);
            }
        }
    }

    fn tick(&mut self, mech: &mut Mech) {
        self.audio.pump();

        // Pace the animated repaints. The playhead moves continuously while a
        // Player plays, but re-presenting faster than the display refreshes only
        // burns the frame budget (this workspace runs on a 30 Hz panel) and
        // starves input. Cap it: if the last frame is too recent, ask the loop to
        // wake when the budget is up and skip this tick's repaint.
        const FRAME: std::time::Duration = std::time::Duration::from_micros(16_667); // ~60 Hz
        if self.players.iter().any(|p| p.playing) {
            let since = self.last_frame.elapsed();
            if since < FRAME {
                mech.wake_after((FRAME - since).as_secs_f64());
                return;
            }
            self.last_frame = Instant::now();
        }

        let rate = self.audio.sample_rate();
        let position: Position = self.audio.position();
        // Advance the playing Player's own playhead and the Counter.
        let mut counter = (1i64, 1i64);
        for p in &mut self.players {
            if p.playing {
                p.playhead = p.beat_at(position.play.0, rate);
                counter = ((p.playhead / 4.0) as i64 + 1, (p.playhead as i64 % 4) + 1);
                mech.mark_dirty_all(p.window);
                p.shown_playing = true;
            } else if p.shown_playing {
                // Just stopped — one repaint to wipe the frozen playhead line.
                mech.mark_dirty_all(p.window);
                p.shown_playing = false;
            }
        }
        if let Some(Kind::Counter { field, .. }) = self.bar.kit.kind_mut(COUNTER) {
            field[0].value = counter.0;
            field[1].value = counter.1;
        }
        if self.bar.kit.animate(self.started.elapsed().as_secs_f64())
            && let Some(w) = self.bar.window
        {
            mech.mark_dirty_all(w);
        }
        if self.players.iter().any(|p| p.playing)
            && let Some(w) = self.bar.window
        {
            mech.mark_dirty_all(w);
        }
    }

    fn paint(&mut self, window: WindowId, frame: &mut Frame<'_>) {
        if self.is_bar(window) {
            let bg = self.bar.kit.skin().panel_lo;
            frame.paint.clear(bg);
            self.bar.kit.paint(&mut frame.paint);
        } else if let Some(ix) = self.player_ix(window) {
            self.paint_player(ix, frame);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log = Log::open_default().unwrap_or_else(|_| Log::hush());
    let audio = Audio::open_with(log.clone(), &Request::default(), |format| {
        rev_app::pad::instrument(
            Patch::harpington(),
            &BakeSpec::harpington(),
            16,
            format.sample_rate,
        )
    });
    if !audio.is_audible() {
        eprintln!("rev-player: no audio device — the workspace still drives, silently");
    }

    let bar_kit = Kit::new(bar_scene(Size::new(400.0, 46.0)), Skin::default());
    let app = App {
        players: Vec::new(),
        bar: Bar {
            window: None,
            kit: bar_kit,
        },
        audio,
        log,
        focused: None,
        pinned: None,
        started: Instant::now(),
        last_frame: Instant::now(),
        next_bpm: 96.0,
        next_tuning: 0,
    };
    rev_ui_mech::run(app)?;
    Ok(())
}
