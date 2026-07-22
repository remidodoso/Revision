//! `rev-studio` — the throwaway record-onto-the-roll harness (rec-02 §4-bis).
//!
//! A test harness, **not** the product Control Bar: it mashes a transport strip
//! (a tri-state Record light, Play, Stop, a Counter) onto the live piano roll so
//! recording is *operable* and *visible* — arm, play the keyboard, watch the
//! notes appear on the roll as you play, stop, hear it back. It borrows finished
//! parts (the ui-03 widget kit, the ui-06 roll, the rec-01 `Recorder`) and is
//! kept only as long as it earns its keep. The permanent, skinned Control Bar
//! slice is ui-04's job; this deliberately *prototypes* a sliver of it.
//!
//! ```text
//! cargo run -p rev-app --bin rev-studio
//! cargo run -p rev-app --bin rev-studio -- --project take.revision --bpm 100
//! ```
//!
//! Transport, by click or key:
//!   Record / `r`   arm the focused track (Off→Armed); press again to disarm
//!   Play  / space  if armed, roll and **record** (Armed→Recording); else play back
//!   Stop           stop; a recording take is finalized and the held notes reported
//!   `1` / `2`      focus track one or two — two-track overdub lives here
//!   `x`            toggle Replace / Overdub for the next take
//!   `t`            cycle the tuning — the 16-ET party trick (rec-03): the same
//!                  take, retuned degree-native with one command
//!   Home           locate to the beginning

use std::time::{Duration, Instant};

use rev_app::audio::Audio;
use rev_app::follow::Follow;
use rev_app::latency::Estimate;
use rev_app::midi::Keys;
use rev_app::record::{Mode, Recorder};
use rev_app::roll::{self, Roll};
use rev_core::phrase::{Change, PhrasePatch, PhraseSpec, TempoPoint, TrackSpec};
use rev_core::tick::{PPQ, Tick, bpm_to_usec_per_quarter};
use rev_core::{Command as ModelCommand, PhraseId, TrackId, TuningId};
use rev_dsp::BakeSpec;
use rev_engine::driver::Request;
use rev_engine::{Chunk, ChunkHandle, Patch, SampleTime, What};
use rev_log::{Log, creator};
use rev_midi::NoteHz;
use rev_sched::{Compiler, TempoMap, TuneCache};
use rev_store::{Project, StoreError, query};
use rev_ui_kit::pane::{Axis, BarPolicy, Pane, Scale};
use rev_ui_kit::{
    Anchor, Field, Intent, Kind, Kit, PaneArtist, RecordMode, Skin, Widget, WidgetId,
};
use rev_ui_mech::{
    Event, Frame, Host, KeyCode, Mech, Named, Notice, Painter, Point, Rect, Size, TargetId,
    TextStyle, Tree, WindowId, WindowSpec,
};

const ROLL: WidgetId = WidgetId(1);
const RECORD: WidgetId = WidgetId(2);
const PLAY: WidgetId = WidgetId(3);
const STOP: WidgetId = WidgetId(4);
const COUNTER: WidgetId = WidgetId(5);

/// The transport strip over the roll. Explicit rects, top-anchored, so a resize
/// keeps the controls put and stretches only the roll.
fn scene(size: Size, roll: &Roll) -> Widget {
    let (beats, octaves) = roll.extent();
    let strip = 44.0;
    Widget::new(0, Kind::Panel, "", Rect::new(0.0, 0.0, size.w, size.h)).with_child(vec![
        Widget::new(
            RECORD.0,
            Kind::Record {
                mode: RecordMode::Off,
            },
            "Record",
            Rect::new(12.0, 8.0, 92.0, 30.0),
        ),
        Widget::new(
            PLAY.0,
            Kind::Button,
            "Play",
            Rect::new(112.0, 8.0, 66.0, 30.0),
        ),
        Widget::new(
            STOP.0,
            Kind::Button,
            "Stop",
            Rect::new(186.0, 8.0, 66.0, 30.0),
        ),
        Widget::new(
            COUNTER.0,
            Kind::Counter {
                field: vec![Field::new(1, 3, 1, 9999), Field::new(1, 1, 1, 4)],
                separator: '|',
            },
            "Counter",
            Rect::new(266.0, 8.0, 150.0, 30.0),
        ),
        Widget::new(
            ROLL.0,
            Kind::Pane {
                pane: Pane {
                    extent: Size::new(beats, octaves),
                    scale: Scale {
                        x: beats / 8.0 / 100.0,
                        y: octaves / 400.0,
                    },
                    bar: BarPolicy::Both,
                    scale_min: 0.000_01,
                    scale_max: 1.0,
                    ..Pane::default()
                },
            },
            "Piano roll",
            Rect::new(12.0, strip + 8.0, size.w - 24.0, size.h - strip - 42.0),
        )
        .with_anchor(Anchor::FILL),
    ])
}

struct Artist<'a> {
    roll: &'a Roll,
    playhead: Option<f64>,
}

impl PaneArtist for Artist<'_> {
    fn paint(&mut self, _: WidgetId, pane: &Pane, interior: Rect, p: &mut Painter) {
        roll::paint(self.roll, pane, interior, self.playhead, p);
    }
}

struct Studio {
    kit: Kit,
    roll: Roll,
    follow: Follow,
    audio: Audio,
    keys: Keys,
    log: Log,
    cache: TuneCache,
    window: Option<WindowId>,

    project: Project,
    arrangement: PhraseId,
    track: [TrackId; 2],
    focus: usize,
    recorder: Recorder,
    mode: Mode,
    /// The tunings the party trick cycles (rec-03): name and id, in order.
    tunings: Vec<(String, TuningId)>,
    tuning_ix: usize,

    bpm: f64,
    started: Instant,
    playing: bool,
    recording: bool,
    playhead: f64,
    said: String,
    latency: Option<Estimate>,
}

impl Studio {
    fn beat_at(&self, sample: u64) -> f64 {
        sample as f64 / f64::from(self.audio.sample_rate()) * (self.bpm / 60.0)
    }

    fn pane_mut(&mut self) -> Option<&mut Pane> {
        match self.kit.kind_mut(ROLL) {
            Some(Kind::Pane { pane }) => Some(pane),
            _ => None,
        }
    }

    fn set_light(&mut self, mode: RecordMode) {
        if let Some(Kind::Record { mode: m }) = self.kit.kind_mut(RECORD) {
            *m = mode;
        }
    }

    /// A tempo map built the way the compiler builds it — the project's own
    /// points, so recorder and playback agree with the roll.
    fn tempo(&self) -> TempoMap {
        let point: Vec<(Tick, i64)> = query::tempo_point(self.project.reader(), self.arrangement)
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.at_tick, p.usec_per_quarter))
            .collect();
        TempoMap::new(point, self.audio.sample_rate())
    }

    /// Point the recorder at the focused track, warm and disarmed.
    fn retarget(&mut self) {
        self.recorder = Recorder::new(self.track[self.focus], self.tempo());
    }

    /// Rebuild the roll from what is on the focused track right now — the live
    /// redraw that makes a note appear the instant it is journaled.
    fn redraw_roll(&mut self) {
        if let Ok(roll) = Roll::build(&self.project, &mut self.cache, self.track[self.focus]) {
            let (beats, octaves) = roll.extent();
            if let Some(pane) = self.pane_mut() {
                pane.extent = Size::new(beats, octaves);
            }
            self.roll = roll;
        }
    }

    /// Compile some tracks into one chunk and hand it to the engine. A compile
    /// failure is logged, not fatal — the harness keeps running.
    fn schedule(&mut self, tracks: Vec<TrackId>) {
        let rate = self.audio.sample_rate();
        let mut compiler = Compiler::new(self.tempo(), tracks);
        let span = ((self.roll.beat_extent + 8.0) * 60.0 / self.bpm * f64::from(rate)) as u64;
        match compiler.chunk(
            &self.project,
            SampleTime(0),
            SampleTime(span.max(u64::from(rate))),
        ) {
            Ok(chunk) => self.audio.send(What::TakeChunk(ChunkHandle::new(Chunk {
                from: chunk.from,
                to: chunk.to,
                note: chunk.note,
            }))),
            Err(error) => self
                .log
                .warn(creator::APP, format!("compile failed: {error}")),
        }
    }

    /// Press Play: record if armed, otherwise play the whole arrangement back.
    fn play(&mut self, mech_dirty: &mut bool) {
        if self.playing {
            return;
        }
        self.audio.send(What::Locate(SampleTime(0)));
        self.playhead = 0.0;
        self.follow.located();
        if self.recorder.is_armed() {
            // Overdub against the *other* track, if it has anything, so you play
            // along with what is already down.
            let other = self.track[1 - self.focus];
            if query::event_on_track(self.project.reader(), other).is_ok_and(|e| !e.is_empty()) {
                self.schedule(vec![other]);
            }
            self.recording = true;
            self.set_light(RecordMode::Recording);
            self.said = format!("recording track {} ({:?})", self.focus + 1, self.mode);
        } else {
            self.schedule(vec![self.track[0], self.track[1]]);
            self.said = String::from("playing back");
        }
        self.audio.send(What::Start);
        self.playing = true;
        *mech_dirty = true;
    }

    /// Press Stop: halt, and finalize a take if one was running.
    fn stop(&mut self) {
        if self.recording {
            let held = self.recorder.disarm();
            let _ = self.recorder.flush(&mut self.project);
            self.recording = false;
            self.set_light(RecordMode::Off);
            self.redraw_roll();
            self.said = if held > 0 {
                format!("stopped — {held} held note(s) dropped")
            } else {
                String::from("stopped")
            };
        } else {
            self.said = String::from("stopped");
        }
        self.audio.send(What::AllNotesOff);
        self.audio.send(What::Stop);
        self.playing = false;
    }

    /// Press Record: arm or disarm the focused track. While recording, it stops.
    fn arm(&mut self) {
        if self.recording {
            self.stop();
            return;
        }
        if self.recorder.is_armed() {
            self.recorder.disarm();
            self.set_light(RecordMode::Off);
            self.said = String::from("disarmed");
        } else {
            self.retarget();
            self.recorder.arm(self.mode);
            self.set_light(RecordMode::Armed);
            self.said = format!("armed track {} ({:?})", self.focus + 1, self.mode);
        }
    }

    fn focus_track(&mut self, which: usize) {
        if which == self.focus || self.recording {
            return;
        }
        self.focus = which;
        self.retarget();
        self.recorder.arm(self.mode);
        // Re-arm is only cosmetic here — show the light armed if it was.
        self.set_light(RecordMode::Off);
        self.recorder.disarm();
        self.redraw_roll();
        self.said = format!("focus track {}", which + 1);
    }

    /// The party trick (rec-03): swap the arrangement's tuning with one command.
    /// The recorded degrees do not move — the *physics* under them does — so the
    /// same performance is heard, and drawn, in a new tuning. Proof the pipeline
    /// is degree-native (R-002), not 12-ET with tuning bolted on.
    fn retune(&mut self) {
        if self.recording || self.tunings.len() < 2 {
            return;
        }
        self.tuning_ix = (self.tuning_ix + 1) % self.tunings.len();
        let (name, id) = self.tunings[self.tuning_ix].clone();
        let patch = PhrasePatch {
            tuning_id: Change::Set(id),
            ..PhrasePatch::default()
        };
        if let Err(error) = self.project.apply(ModelCommand::SetPhrase {
            id: self.arrangement,
            patch,
        }) {
            self.log
                .warn(creator::APP, format!("retune failed: {error}"));
            return;
        }
        self.redraw_roll();
        // Heard, not just seen: if a take is playing back, recompile so the flip
        // happens under your ears, not on the next Play.
        if self.playing && !self.recording {
            self.schedule(vec![self.track[0], self.track[1]]);
        }
        self.said = format!("retuned to {name} — same performance, new degrees");
    }

    /// The current tuning's name, for the status line.
    fn tuning_name(&self) -> &str {
        self.tunings
            .get(self.tuning_ix)
            .map(|(name, _)| name.as_str())
            .unwrap_or("?")
    }
}

impl Host for Studio {
    fn start(&mut self, mech: &mut Mech) {
        self.window = Some(mech.open_window(WindowSpec {
            title: String::from("Revision — Studio (test harness)"),
            size: Size::new(1200.0, 720.0),
            ..WindowSpec::default()
        }));
    }

    fn notice(&mut self, window: WindowId, notice: &Notice, mech: &mut Mech) {
        match notice {
            Notice::CloseRequested => {
                mech.close_window(window);
                mech.exit();
            }
            Notice::Resized(size) => {
                self.kit.layout(Rect::new(0.0, 0.0, size.w, size.h));
                mech.mark_dirty_all(window);
            }
            _ => {}
        }
    }

    fn hit(&self, _: WindowId, at: Point) -> Option<TargetId> {
        self.kit.hit(at)
    }

    fn a11y(&self, _: WindowId) -> Tree {
        let mut tree = self.kit.a11y();
        if let (Some(rect), Some(Kind::Pane { pane })) = (self.kit.rect(ROLL), self.kit.kind(ROLL))
        {
            roll::describe(
                &self.roll,
                &mut tree,
                TargetId(u64::from(ROLL.0)),
                rect,
                pane,
            );
        }
        tree
    }

    fn event(&mut self, window: WindowId, target: Option<TargetId>, ev: &Event, mech: &mut Mech) {
        let mut dirty = false;
        if let Event::Key(key) = ev
            && key.pressed
        {
            match key.code {
                KeyCode::Named(Named::Space) => self.play(&mut dirty),
                KeyCode::Char('r') | KeyCode::Char('R') => self.arm(),
                KeyCode::Char('x') | KeyCode::Char('X') => {
                    self.mode = match self.mode {
                        Mode::Overdub => Mode::Replace,
                        Mode::Replace => Mode::Overdub,
                    };
                    self.said = format!("next take: {:?}", self.mode);
                }
                KeyCode::Char('1') => self.focus_track(0),
                KeyCode::Char('2') => self.focus_track(1),
                KeyCode::Char('t') | KeyCode::Char('T') => self.retune(),
                KeyCode::Named(Named::Home) => {
                    self.audio.send(What::Locate(SampleTime(0)));
                    self.playhead = 0.0;
                    self.follow.located();
                    self.said = String::from("located");
                }
                _ => {}
            }
        }

        if let Some((id, intent)) = self.kit.event(target, ev) {
            match (id, intent) {
                (RECORD, Intent::RecordPressed(_)) => self.arm(),
                (PLAY, Intent::Released) => self.play(&mut dirty),
                (STOP, Intent::Released) => self.stop(),
                (_, Intent::Scrolled(_)) => {
                    let armed = self.follow.armed();
                    self.follow.user_scrolled(Axis::Horizontal);
                    if armed && !self.follow.armed() {
                        self.said = String::from("scrolled — following off (Home to resume)");
                    }
                }
                _ => {}
            }
        }
        let _ = dirty;
        mech.mark_dirty_all(window);
    }

    fn tick(&mut self, mech: &mut Mech) {
        let Some(window) = self.window else { return };
        self.audio.pump();

        if self.latency.is_none() {
            let position = self.audio.position();
            if let Some(estimate) = Estimate::from(&position) {
                self.log.info(
                    creator::APP,
                    estimate.summary(position.block_frames, position.sample_rate),
                );
                self.latency = Some(estimate);
            }
        }

        // Live input: play it (thru is automatic) and, if recording, capture it.
        self.keys.poll(&self.log);
        let mut captured = Vec::new();
        self.keys.drain(|c| captured.push(c));
        let position = self.audio.position();
        self.recorder.observe(&position);
        for c in captured {
            self.recorder.capture(c);
        }
        if self.recording && self.recorder.flush(&mut self.project).unwrap_or(0) > 0 {
            self.redraw_roll();
            mech.mark_dirty_all(window);
        }

        if self.playing {
            self.playhead = self.beat_at(position.play.0);
            let bar = (self.playhead / 4.0) as i64 + 1;
            let beat = (self.playhead as i64 % 4) + 1;
            if let Some(Kind::Counter { field, .. }) = self.kit.kind_mut(COUNTER) {
                field[0].value = bar;
                field[1].value = beat;
            }
            let rect = self.kit.rect(ROLL);
            let follow = self.follow;
            let head = self.playhead;
            if let (Some(rect), Some(pane)) = (rect, self.pane_mut()) {
                follow.advance(rect, pane, head);
            }
            mech.mark_dirty_all(window);
        }

        // The armed light flashes on the UI clock — the kit owns the phase.
        if self.kit.animate(self.started.elapsed().as_secs_f64()) {
            mech.mark_dirty_all(window);
        }
    }

    fn paint(&mut self, _: WindowId, frame: &mut Frame<'_>) {
        let (background, ink) = {
            let s = self.kit.skin();
            (s.panel_lo, s.ink)
        };
        frame.paint.clear(background);
        let mut artist = Artist {
            roll: &self.roll,
            playhead: Some(self.playhead),
        };
        self.kit.paint_with(&mut frame.paint, &mut artist);

        let midi = match self.keys.open_port() {
            Some(port) => format!("MIDI: {port}"),
            None => String::from("MIDI: (plug in a keyboard)"),
        };
        let latency = match self.latency {
            Some(e) => format!("  ·  latency ≥ {:.1} ms", e.floor_ms()),
            None => String::new(),
        };
        let line = format!(
            "r: arm · space: play/record · 1/2: track · x: mode · t: tune · home: locate   |   \
             track {} · {} · beat {:.2} · {} · {midi}{latency}",
            self.focus + 1,
            self.tuning_name(),
            self.playhead,
            self.said,
        );
        let shaped = frame.paint.shape(&line, &TextStyle::numeric(13.0));
        frame
            .paint
            .draw_text(&shaped, Point::new(14.0, frame.size.h - 18.0), ink);
    }
}

/// Build an arrangement with two empty tracks, at the given tempo and tuning.
fn build(
    project: &mut Project,
    bpm: f64,
    tuning: &str,
) -> Result<(PhraseId, [TrackId; 2]), StoreError> {
    let tuning_id = query::tuning_by_name(project.reader(), tuning)?.map(|t| t.id);
    project.gesture(|g| {
        let mut phrase = PhraseSpec::new("Take", Tick(PPQ * 4 * 256));
        phrase.tuning_id = tuning_id;
        let arrangement = match g.exec(ModelCommand::CreatePhrase { id: None, phrase })? {
            ModelCommand::CreatePhrase { id: Some(id), .. } => id,
            _ => unreachable!(),
        };
        g.exec(ModelCommand::SetTempo {
            phrase_id: arrangement,
            point: vec![TempoPoint {
                at_tick: Tick::ZERO,
                usec_per_quarter: bpm_to_usec_per_quarter(bpm),
            }],
        })?;
        let mut track = [TrackId(0); 2];
        for (i, slot) in track.iter_mut().enumerate() {
            *slot = match g.exec(ModelCommand::CreateTrack {
                id: None,
                track: TrackSpec::new(arrangement, format!("Track {}", i + 1), i as i32),
            })? {
                ModelCommand::CreateTrack { id: Some(id), .. } => id,
                _ => unreachable!(),
            };
        }
        Ok((arrangement, track))
    })
}

fn snapshot(project: &Project, tuning: &str) -> NoteHz {
    query::tuning_by_name(project.reader(), tuning)
        .ok()
        .flatten()
        .and_then(|t| {
            query::latest_materialized_instance(project.reader(), t.id)
                .ok()
                .flatten()
        })
        .and_then(|inst| {
            query::materialized_tuning(project.reader(), inst)
                .ok()
                .flatten()
        })
        .map(|t| NoteHz::from_tuning(&t))
        .unwrap_or_else(NoteHz::silent)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (bpm, tuning, project_path) = parse();
    let log = Log::open_default().unwrap_or_else(|_| Log::hush());

    let mut cleanup = None;
    let path = match project_path {
        Some(path) => std::path::PathBuf::from(path),
        None => {
            let dir = std::env::temp_dir().join(format!("revision_studio_{}", std::process::id()));
            std::fs::create_dir_all(&dir)?;
            cleanup = Some(dir.clone());
            dir.join("studio.revision")
        }
    };
    let mut project = Project::create(&path)?;
    let (arrangement, track) = build(&mut project, bpm, &tuning)?;

    let mut cache = TuneCache::new();
    let roll = Roll::build(&project, &mut cache, track[0])?;

    let mut audio = Audio::open_with(log.clone(), &Request::default(), |format| {
        rev_app::pad::instrument(
            Patch::harpington(),
            &BakeSpec::harpington(),
            16,
            format.sample_rate,
        )
    });
    if !audio.is_audible() {
        eprintln!("rev-studio: no audio device — you can still record, but silently");
    }

    let origin = audio.origin();
    let snap = snapshot(&project, &tuning);
    let keys = match audio.take_thru() {
        Some(thru) => Keys::new(thru, snap, origin),
        None => unreachable!("audio always yields its thru sender once"),
    };
    let rate = audio.sample_rate();
    let tempo = TempoMap::new(
        query::tempo_point(project.reader(), arrangement)?
            .into_iter()
            .map(|p| (p.at_tick, p.usec_per_quarter))
            .collect::<Vec<_>>(),
        rate,
    );
    let recorder = Recorder::new(track[0], tempo);

    // The tunings the `t` key cycles (rec-03), those genesis seeds that exist.
    let mut tunings = Vec::new();
    for name in ["12-ET", "16-ET", "Just (5-limit)"] {
        if let Ok(Some(t)) = query::tuning_by_name(project.reader(), name) {
            tunings.push((name.to_string(), t.id));
        }
    }
    let tuning_ix = tunings.iter().position(|(n, _)| n == &tuning).unwrap_or(0);

    let kit = Kit::new(scene(Size::new(1200.0, 720.0), &roll), Skin::default());
    rev_ui_mech::run(Studio {
        kit,
        roll,
        follow: Follow::default(),
        audio,
        keys,
        log,
        cache,
        window: None,
        project,
        arrangement,
        track,
        focus: 0,
        recorder,
        mode: Mode::Overdub,
        tunings,
        tuning_ix,
        bpm,
        started: Instant::now(),
        playing: false,
        recording: false,
        playhead: 0.0,
        said: String::from("ready — press r to arm, space to record"),
        latency: None,
    })?;

    if let Some(dir) = cleanup {
        // Give the OS a beat to release the file handles before removing.
        std::thread::sleep(Duration::from_millis(50));
        let _ = std::fs::remove_dir_all(dir);
    }
    Ok(())
}

fn parse() -> (f64, String, Option<String>) {
    let mut bpm = 120.0;
    let mut tuning = String::from("12-ET");
    let mut project = None;
    let mut argument = std::env::args().skip(1);
    while let Some(flag) = argument.next() {
        match flag.as_str() {
            "--bpm" => {
                bpm = argument
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(120.0)
            }
            "--tuning" => tuning = argument.next().unwrap_or(tuning),
            "--project" => project = argument.next(),
            "--help" | "-h" => {
                println!(
                    "rev-studio [--bpm N] [--tuning 12-ET|16-ET|JI] [--project FILE.revision]"
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument {other:?}; try --help");
                std::process::exit(2);
            }
        }
    }
    (bpm, tuning, project)
}
