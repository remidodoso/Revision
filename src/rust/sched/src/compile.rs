//! Ticks and note numbers in; samples and frequencies out.
//!
//! The whole of R-312 in one function. Above it, music; below it, physics; and
//! this is the boundary both sides agree on because only one of them computes it.

use rev_core::id::{TrackId, TuningId};
use rev_core::tick::Tick;
use rev_engine::{Chunk, Note, SampleTime};
use rev_store::{Project, query};

use crate::error::SchedError;
use crate::tempo::TempoMap;
use crate::tune::TuneCache;

/// The model's velocity domain (R-402): 16-bit, MIDI 2.0.
const VELOCITY_FULL: f32 = 65_535.0;

/// Compiles windows of a project into chunks the engine can play.
///
/// Holds the tempo map and the tuning cache, because both are expensive to build
/// and constant across the many windows of one playback.
pub struct Compiler {
    tempo: TempoMap,
    tune: TuneCache,
    /// Tracks to compile, in order. A track's index here is the `voice` a note
    /// carries — an opaque routing key the engine never interprets.
    track: Vec<TrackId>,
    /// Rows whose note number falls outside their tuning, since compilation
    /// started. Reported by the caller; never silently swallowed.
    unplayable: u64,
    /// The project's default tuning, read once. A phrase's tuning is optional
    /// (R-414), and an untuned phrase's note numbers are interpreted in the
    /// project default — which is a *project-level* fallback, not inheritance
    /// from a parent phrase. Tuning attaches to events through the phrase that
    /// holds them, at every level of nesting; a structured phrase has as many
    /// tunings as its components do, and no tuning of its own.
    fallback: Option<TuningId>,
}

impl Compiler {
    pub fn new(tempo: TempoMap, track: Vec<TrackId>) -> Compiler {
        Compiler {
            tempo,
            tune: TuneCache::new(),
            track,
            unplayable: 0,
            fallback: None,
        }
    }

    pub fn tempo(&self) -> &TempoMap {
        &self.tempo
    }

    /// Rows that could not be given a frequency. A tuning genuinely may not reach
    /// a note number; what it must not do is fail silently.
    pub fn unplayable(&self) -> u64 {
        self.unplayable
    }

    /// Forget materialized tunings — after a gesture that touched one.
    pub fn retune(&mut self) {
        self.tune.clear();
        self.fallback = None;
    }

    fn default_tuning(&mut self, project: &Project) -> Result<Option<TuningId>, SchedError> {
        if self.fallback.is_none() {
            self.fallback = rev_store::query::meta(
                project.reader(),
                rev_store::schema::META_DEFAULT_TUNING_ID,
            )?
            .and_then(|value| value.parse::<i64>().ok())
            .map(TuningId);
        }
        Ok(self.fallback)
    }

    /// Compile `[from, to)` of play position into a chunk.
    ///
    /// The window is given in **samples**, because that is what the caller knows:
    /// it is chasing the engine's reported position. Ticks are an implementation
    /// detail of the query.
    pub fn chunk(
        &mut self,
        project: &Project,
        from: SampleTime,
        to: SampleTime,
    ) -> Result<Chunk, SchedError> {
        let from_tick = self.tempo.tick_at(from);
        // The tick containing `to` may start before it, so a note at that tick
        // would be admitted by the tick test and fall outside the sample window.
        // Filter on samples after converting, rather than trying to be clever
        // about which tick to ask for.
        let to_tick = Tick(self.tempo.tick_at(to).get() + 1);

        // Resolved before the loop: it is constant for the whole chunk, and
        // reading it inside would borrow `self` mutably while `self.track` is
        // already borrowed for iteration.
        let fallback = self.default_tuning(project)?;

        let mut note = Vec::new();
        let track = self.track.clone();
        for (index, &track_id) in track.iter().enumerate() {
            let voice = index as u16;
            for row in query::realized_between(project.reader(), track_id, from_tick, to_tick)? {
                let Some(number) = row.note_number else {
                    continue; // not a note; other event kinds are not this item's
                };
                let at = self.tempo.sample_at(row.at_tick);
                if at < from || at >= to {
                    continue;
                }
                let Some(tuning_id) = row.tuning_id.or(fallback) else {
                    // No tuning on the phrase and no project default: the note
                    // number has nothing to be interpreted in.
                    self.unplayable += 1;
                    continue;
                };
                let Some(hz) = self.tune.hz(project, tuning_id, number)? else {
                    // A tuning of 24 notes has nothing to say about note 400.
                    // Counted, returned to the caller, never a panic and never a
                    // silent skip.
                    self.unplayable += 1;
                    continue;
                };

                // Duration in samples is the distance between the note's start and
                // its end *as positions*, not a separately converted length. At a
                // tempo change inside the note, those differ — and the difference
                // is the whole reason a tempo map exists.
                let end = self
                    .tempo
                    .sample_at(Tick(row.at_tick.get() + row.dur_tick.get().max(0)));
                note.push(Note {
                    at,
                    dur: (end.0 - at.0).min(u64::from(u32::MAX)) as u32,
                    hz: hz as f32,
                    level: row.velocity.unwrap_or(0).max(0) as f32 / VELOCITY_FULL,
                    voice,
                    reserved: 0,
                });
            }
        }

        // Ascending by `at` is a precondition of the engine's forward scan. Ties
        // keep the order the query produced (at_tick, then note_number, then
        // track), so compiling twice cannot reorder anything — which is what
        // makes byte-identical chunks possible.
        note.sort_by_key(|n| n.at);

        Ok(Chunk { from, to, note })
    }
}
