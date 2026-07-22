//! The read-only piano roll (ui-06).
//!
//! **Almost all of this is choosing coordinates and then not doing very much.**
//! Content is `log2(hz)` vertically and *beats* horizontally, so R-941's
//! continuous logarithmic pitch axis is not implemented at all — it is the
//! coordinate system. Octaves come out evenly spaced by construction, unequal
//! tunings draw unequal with no special case, and material in different tunings
//! overlays honestly (R-942). Nothing here ever divides an octave by a number of
//! degrees.
//!
//! Beats rather than ticks for two reasons that agree: the pane's offsets are
//! `f32`, and at 5040 PPQ ticks leave exact integer representation after about
//! 27 minutes at 120 bpm — a long piece would quantize the *view*. And beats is
//! the axis's meaning, making both axes the continuous musical quantity with the
//! model's integers underneath.
//!
//! Pitch resolves through the same [`TuneCache`] the schedule compiler uses, so
//! *what you see is what you hear* is structural rather than a coincidence that
//! holds until somebody edits one of the two (R-312).

use rev_core::tick::{PPQ, Tick};
use rev_core::tuning::MaterializedTuning;
use rev_core::{NoteNumber, TrackId, TuningId};
use rev_sched::TuneCache;
use rev_store::{Project, StoreError, query};
use rev_ui_kit::pane::Pane;
use rev_ui_mech::{Color, Node, Painter, Point, Rect, Role, TargetId, TextStyle, Tree};

/// One note, ready to draw: where it starts, how long it lasts, and what pitch
/// it *actually sounds*.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drawn {
    /// Onset, in beats.
    pub beat: f64,
    /// Duration, in beats. A note is an entity with a duration (R-402a).
    pub length: f64,
    /// The resolved frequency — the same number the engine is handed.
    pub hz: f64,
    /// `log2(hz)`: the vertical content coordinate, and the whole reason the
    /// pitch axis needs no special handling.
    pub y: f64,
    pub note: NoteNumber,
    pub velocity: i32,
}

/// A rung of the degree ladder: a line to draw, and what to call it.
#[derive(Debug, Clone, PartialEq)]
pub struct Rung {
    pub y: f64,
    pub hz: f64,
    pub note: NoteNumber,
    pub label: String,
}

/// Everything the roll draws, resolved once from the model.
#[derive(Debug, Clone, Default)]
pub struct Roll {
    pub note: Vec<Drawn>,
    pub rung: Vec<Rung>,
    /// The material's extent, as content: beats across, `log2(hz)` down.
    pub beat_extent: f64,
    pub low: f64,
    pub high: f64,
    /// Median spacing between adjacent degrees, in `log2(hz)`. What note
    /// thickness is derived from — the median rather than the local spacing, so
    /// an unequal tuning does not make notes randomly fat and thin.
    pub degree_step: f64,
}

/// Vertical margin above and below the material, in octaves.
const MARGIN: f64 = 0.25;

impl Roll {
    /// Resolve a track into something drawable.
    ///
    /// **Through the compiler's own cache.** If the roll resolved pitch its own
    /// way, the picture and the sound could disagree and nothing would notice.
    pub fn build(
        project: &Project,
        cache: &mut TuneCache,
        track: TrackId,
    ) -> Result<Roll, StoreError> {
        let event = query::realized(project.reader(), track)?;
        let mut roll = Roll::default();
        let mut tuning: Option<TuningId> = None;

        for e in &event {
            let (Some(note), Some(id)) = (e.note_number, e.tuning_id) else {
                continue;
            };
            // A tuning that cannot reach a note is a real answer, not an error
            // (rev-sched's rule) — the note simply is not drawable, and the
            // engine will not sound it either.
            let Ok(Some(hz)) = cache.hz(project, id, note) else {
                continue;
            };
            tuning.get_or_insert(id);
            roll.note.push(Drawn {
                beat: beat_of(e.at_tick),
                length: beat_of(e.dur_tick),
                hz,
                y: hz.log2(),
                note,
                velocity: e.velocity.unwrap_or(0),
            });
        }

        roll.beat_extent = roll
            .note
            .iter()
            .map(|n| n.beat + n.length)
            .fold(0.0, f64::max);
        let (lowest, highest) = roll.note.iter().fold((f64::MAX, f64::MIN), |(lo, hi), n| {
            (lo.min(n.y), hi.max(n.y))
        });
        if roll.note.is_empty() {
            (roll.low, roll.high) = (log2_hz(220.0), log2_hz(880.0));
        } else {
            roll.low = lowest - MARGIN;
            roll.high = highest + MARGIN;
        }

        // The ladder comes from the tuning of the track being displayed. When a
        // view holds several tunings (R-942 says it may), the rule is chosen
        // then, with material in hand — stated here rather than assumed.
        if let Some(id) = tuning {
            roll.rung = ladder(project, id, roll.low, roll.high)?;
            roll.degree_step = median_step(&roll.rung);
        }
        Ok(roll)
    }

    /// The pane extent this material wants.
    pub fn extent(&self) -> (f32, f32) {
        (
            self.beat_extent.max(1.0) as f32,
            (self.high - self.low).max(0.1) as f32,
        )
    }

    /// Pane-space y for a frequency. The pane's origin is the top, and pitch
    /// rises upward, so the axis is inverted here and nowhere else.
    pub fn content_y(&self, y: f64) -> f32 {
        (self.high - y) as f32
    }
}

/// The display origin for counted fields: a user preference whose default is 1
/// (R-944). Applied only when *displaying* a position, never to a stored one.
pub const ORIGIN: f64 = 1.0;

/// Contribute the notes to the accessibility tree, under the pane's node.
///
/// **The debt ui-07 §3 signed.** The pane exposes itself as a scrollable region
/// with position and extent, and says outright that the interior's semantics
/// belong to its consumer. This is the consumer.
///
/// Notes as *data* — pitch, onset and duration, in the user's counting — not
/// rectangles and not pixels. Bounds are supplied because a reader may want to
/// point at one, but they are not what is being communicated.
pub fn describe(roll: &Roll, tree: &mut Tree, pane_id: TargetId, rect: Rect, pane: &Pane) {
    let Some(root) = tree.root.as_mut() else {
        return;
    };
    let Some(node) = find(root, pane_id) else {
        return;
    };
    let interior = pane.interior(rect);
    node.child = roll
        .note
        .iter()
        .enumerate()
        .map(|(index, note)| {
            let x = interior.x
                + ((note.beat - f64::from(pane.offset.x)) / f64::from(pane.scale.x)) as f32;
            let y = interior.y
                + ((roll.content_y(note.y) as f64 - f64::from(pane.offset.y))
                    / f64::from(pane.scale.y)) as f32;
            let w = (note.length / f64::from(pane.scale.x)) as f32;
            let mut child = Node::new(
                TargetId(A11Y_BASE + index as u64),
                Role::Label,
                format!("note {}", note.note.get()),
                Rect::new(x, y - 4.0, w.max(1.0), 8.0),
            );
            child.value = Some(format!(
                "{:.3} Hz, beat {:.2}, {:.2} beats long",
                note.hz,
                note.beat + ORIGIN,
                note.length
            ));
            child
        })
        .collect();
}

/// Where the roll's note ids start. Far above any widget id, so a note and a
/// control can never collide.
const A11Y_BASE: u64 = 1 << 32;

fn find(node: &mut Node, id: TargetId) -> Option<&mut Node> {
    if node.id == id {
        return Some(node);
    }
    node.child.iter_mut().find_map(|c| find(c, id))
}

/// Ticks to beats: exact, and **tempo-independent** — only *seconds* involve
/// the tempo map.
pub fn beat_of(tick: Tick) -> f64 {
    tick.get() as f64 / PPQ as f64
}

fn log2_hz(hz: f64) -> f64 {
    hz.log2()
}

/// Every degree of a tuning that falls inside the view, labelled.
fn ladder(project: &Project, id: TuningId, low: f64, high: f64) -> Result<Vec<Rung>, StoreError> {
    let conn = project.reader();
    let Some(latest) = query::latest_materialized_instance(conn, id)? else {
        return Ok(Vec::new());
    };
    let Some(table) = query::materialized_tuning(conn, latest)? else {
        return Ok(Vec::new());
    };
    Ok(table
        .rows()
        .filter(|(_, hz)| *hz > 0.0)
        .map(|(note, hz)| (note, hz, hz.log2()))
        .filter(|(_, _, y)| *y >= low && *y <= high)
        .map(|(note, hz, y)| Rung {
            y,
            hz,
            note,
            label: label_of(&table, note),
        })
        .collect())
}

/// What to call a degree (R-943).
///
/// A tuning's own degree names where it has them, the degree index otherwise. A
/// degree without a conventional name is an ordinary case, not an error, and
/// nearest-12-ET-with-cents is an orientation aid that is never the primary
/// reading — so it is not what this returns.
fn label_of(_table: &MaterializedTuning, note: NoteNumber) -> String {
    // **The degree index, which is the note number** — a signed integer position
    // in a tuning (R-002). Nothing invented.
    //
    // A first version wrote `index/period`, e.g. "4/5" for note 64 of 12-ET.
    // That reads as *4 steps of 5-EDO*, which is the xenharmonic convention and
    // is not what it meant — a label that says something false about the tuning
    // is worse than a plain number. Found by looking at the screenshot.
    //
    // Real degree names arrive when tunings carry them; `TuningNote` has no
    // name field today, so "as well as they can be" (R-943) is this.
    format!("{}", note.get())
}

/// The median gap between adjacent rungs, in `log2(hz)`.
fn median_step(rung: &[Rung]) -> f64 {
    if rung.len() < 2 {
        return 1.0 / 12.0;
    }
    let mut gap: Vec<f64> = rung.windows(2).map(|w| (w[1].y - w[0].y).abs()).collect();
    gap.sort_by(f64::total_cmp);
    gap[gap.len() / 2]
}

/// Note thickness in pixels: fat like a real roll when zoomed in, never filling
/// the gap between degrees — so it never implies that a note occupies a pitch
/// *range*. It has a frequency.
pub fn thickness(roll: &Roll, pane: &Pane) -> f32 {
    let spacing = (roll.degree_step / f64::from(pane.scale.y)) as f32;
    (0.6 * spacing).clamp(5.0, 18.0)
}

/// The grid ladder's rungs: **divisors of the tick resolution**, not powers of
/// ten.
///
/// 5040 is 2⁴·3²·5·7, which is why it was chosen, so each of these divides
/// exactly and every gridline lands on a position music can actually hold. The
/// last rung is a single tick, which is where "ticks at extreme magnification"
/// lives.
pub const SUBDIVISION: [i64; 11] = [1, 2, 3, 4, 6, 8, 12, 16, 24, 48, PPQ];

/// The finest subdivision whose lines are at least `apart` pixels apart.
pub fn grid_step(beats_per_pixel: f32, apart: f32) -> f64 {
    for div in SUBDIVISION {
        let step = 1.0 / div as f64;
        if (step / f64::from(beats_per_pixel)) as f32 >= apart {
            return step;
        }
    }
    // Coarser than a beat: whole beats, then powers of two of them, so the grid
    // never disappears at low zoom.
    let mut step = 1.0;
    while ((step / f64::from(beats_per_pixel)) as f32) < apart {
        step *= 2.0;
    }
    step
}

/// Paint the roll's interior. Called by the kit with the clip already set.
pub fn paint(roll: &Roll, pane: &Pane, interior: Rect, playhead: Option<f64>, p: &mut Painter) {
    let skin = Ink::default();
    let thickness = thickness(roll, pane);

    // --- time grid
    let step = grid_step(pane.scale.x, 48.0);
    let from = (f64::from(pane.offset.x) / step).floor() * step;
    let to = f64::from(pane.offset.x) + f64::from(interior.w) * f64::from(pane.scale.x);
    let mut beat = from;
    while beat <= to {
        let x = interior.x + ((beat - f64::from(pane.offset.x)) / f64::from(pane.scale.x)) as f32;
        // Periodic emphasis is a display grouping, never a claim about meter
        // (R-946).
        let emphasis = (beat / (step * f64::from(EMPHASIS))).fract().abs() < 1e-9;
        p.stroke_line(
            Point::new(x, interior.y),
            Point::new(x, interior.bottom()),
            if emphasis { skin.grid_major } else { skin.grid },
            1.0,
        );
        beat += step;
    }

    // --- degree ladder, pinned against horizontal scroll
    for rung in &roll.rung {
        let y = interior.y
            + ((roll.content_y(rung.y) as f64 - f64::from(pane.offset.y)) / f64::from(pane.scale.y))
                as f32;
        if y < interior.y || y > interior.bottom() {
            continue;
        }
        p.stroke_line(
            Point::new(interior.x, y),
            Point::new(interior.right(), y),
            skin.rung,
            1.0,
        );
        // Pinned: drawn at a fixed x, ignoring the horizontal offset entirely.
        // On a strip of its own, because a label drawn straight over the
        // material is unreadable exactly where the material is densest — which
        // the first screenshot showed and no assertion could.
        let shaped = p.shape(&rung.label, &TextStyle::numeric(11.0));
        let size = shaped.size();
        p.fill_rect(
            Rect::new(interior.x, y - size.h - 1.0, size.w + 8.0, size.h + 2.0),
            skin.gutter,
        );
        p.draw_text(
            &shaped,
            Point::new(interior.x + 4.0, y - size.h - 1.0),
            skin.label,
        );
    }

    // --- notes
    for note in &roll.note {
        let x =
            interior.x + ((note.beat - f64::from(pane.offset.x)) / f64::from(pane.scale.x)) as f32;
        let y = interior.y
            + ((roll.content_y(note.y) as f64 - f64::from(pane.offset.y)) / f64::from(pane.scale.y))
                as f32;
        let width = (note.length / f64::from(pane.scale.x)) as f32;
        if x > interior.right() || x + width.max(thickness) < interior.x {
            continue;
        }
        let top = y - thickness / 2.0;
        if width < 1.0 {
            // **A circle, deliberately.** Below a pixel a capsule would read as
            // some particular short length, which is a thing reading as
            // something it is not (R-945's principle in another medium). A
            // circle declines to represent the duration and says so by looking
            // different; the onset stays truthful, which is the coordinate that
            // matters at this scale. It is also simply what a percussion part
            // looks like.
            p.fill_round_rect(
                Rect::new(x, top, thickness, thickness),
                thickness / 2.0,
                skin.note,
            );
        } else {
            // A thick line with round end caps: two abutting notes pinch
            // between their caps instead of merging into one long bar, which is
            // what makes legato legible.
            p.fill_round_rect(
                Rect::new(x, top, width, thickness),
                thickness / 2.0,
                skin.note,
            );
        }
    }

    // --- playhead, the only thing that moves during playback
    if let Some(beat) = playhead {
        let x = interior.x + ((beat - f64::from(pane.offset.x)) / f64::from(pane.scale.x)) as f32;
        if x >= interior.x && x <= interior.right() {
            p.stroke_line(
                Point::new(x, interior.y),
                Point::new(x, interior.bottom()),
                skin.playhead,
                1.0,
            );
        }
    }
}

/// How many grid steps between emphasised lines. View state, user-settable,
/// carrying no model meaning (R-946).
pub const EMPHASIS: u32 = 4;

/// The roll's colours. Not the control skin — this is content, and content has
/// its own ink.
struct Ink {
    gutter: Color,
    grid: Color,
    grid_major: Color,
    rung: Color,
    label: Color,
    note: Color,
    playhead: Color,
}

impl Default for Ink {
    fn default() -> Ink {
        Ink {
            gutter: Color::rgba(24, 27, 32, 220),
            grid: Color::rgba(56, 62, 74, 255),
            grid_major: Color::rgba(84, 92, 108, 255),
            rung: Color::rgba(44, 49, 58, 255),
            label: Color::rgba(120, 130, 148, 255),
            note: Color::rgba(210, 170, 80, 255),
            playhead: Color::rgba(230, 226, 216, 255),
        }
    }
}

#[cfg(test)]
mod test;
