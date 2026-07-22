//! rev-roll — MHALL, visible and playing (ui-06).
//!
//! The arc that began with "what do we do now to make sound soon" ends here:
//! the same material, resolved through the same tuning cache, drawn where it is
//! heard. Space plays and stops; Home locates to the beginning, which re-arms
//! following.
//!
//! **Watch the two things this item is about.** Scroll horizontally during
//! playback and following stops, permanently, because you have taken charge —
//! Home gives it back. And notice that the view is *stationary* between jumps:
//! the only thing moving is the playhead (R-947).

use rev_app::audio::Audio;
use rev_app::follow::Follow;
use rev_app::latency::Estimate;
use rev_app::mhall::build;
use rev_app::midi::Keys;
use rev_app::roll::{self, Roll};
use rev_core::tick::bpm_to_usec_per_quarter;
use rev_dsp::BakeSpec;
use rev_engine::driver::Request;
use rev_engine::{Chunk, ChunkHandle, Command, Patch, SampleTime, What};
use rev_log::{Log, creator};
use rev_midi::NoteHz;
use rev_sched::{Compiler, TempoMap, TuneCache};
use rev_store::{Project, query};
use rev_ui_kit::pane::{Axis, BarPolicy, Pane, Scale};
use rev_ui_kit::{Intent, Kind, Kit, PaneArtist, Skin, Widget, WidgetId};
use rev_ui_mech::{
    Event, Frame, Host, KeyCode, Mech, Named, Notice, Painter, Point, Rect, Size, TargetId,
    TextStyle, Tree, WindowId, WindowSpec,
};

const ROLL: WidgetId = WidgetId(1);
const BPM: f64 = 120.0;

fn scene(size: Size, roll: &Roll) -> Widget {
    let (beats, octaves) = roll.extent();
    Widget::new(0, Kind::Panel, "", Rect::new(0.0, 0.0, size.w, size.h)).with_child(vec![
        Widget::new(
            ROLL.0,
            Kind::Pane {
                pane: Pane {
                    extent: Size::new(beats, octaves),
                    // Eight beats across and the material's range down, to
                    // start. Both are content units: beats and log2(hz).
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
            Rect::new(12.0, 12.0, size.w - 24.0, size.h - 46.0),
        )
        .with_anchor(rev_ui_kit::Anchor::FILL),
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

struct Demo {
    kit: Kit,
    roll: Roll,
    follow: Follow,
    audio: Audio,
    keys: Keys,
    log: Log,
    window: Option<WindowId>,
    playing: bool,
    playhead: f64,
    said: String,
    live: String,
    /// The latency floor, once a block has been observed — printed once and
    /// shown live (midi-03, R-307).
    latency: Option<Estimate>,
}

impl Demo {
    /// Beats from the engine's sample position — the one place seconds are
    /// involved at all.
    fn beat_at(&self, sample: u64) -> f64 {
        sample as f64 / f64::from(self.audio.sample_rate()) * (BPM / 60.0)
    }

    fn pane_mut(&mut self) -> Option<&mut Pane> {
        match self.kit.kind_mut(ROLL) {
            Some(Kind::Pane { pane }) => Some(pane),
            _ => None,
        }
    }
}

impl Host for Demo {
    fn start(&mut self, mech: &mut Mech) {
        self.window = Some(mech.open_window(WindowSpec {
            title: String::from("Revision — MHALL"),
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
        if let Event::Key(key) = ev
            && key.pressed
        {
            match key.code {
                KeyCode::Named(Named::Space) => {
                    self.playing = !self.playing;
                    let what = if self.playing {
                        What::Start
                    } else {
                        What::Stop
                    };
                    self.audio.send(what);
                    self.said = String::from(if self.playing { "playing" } else { "stopped" });
                }
                KeyCode::Named(Named::Home) => {
                    // An explicit locate: the one thing that gives following
                    // back, along with the control itself.
                    self.audio.send(What::Locate(SampleTime(0)));
                    self.playhead = 0.0;
                    self.follow.located();
                    self.said = String::from("located to the beginning — following again");
                }
                _ => {}
            }
        }

        if let Some((_, intent)) = self.kit.event(target, ev) {
            match intent {
                // **The user took over.** An Intent is by construction a user
                // act — the kit emits them only from input — so nothing has to
                // guess whether this scroll was ours.
                Intent::Scrolled(_) => {
                    let armed = self.follow.armed();
                    self.follow.user_scrolled(Axis::Horizontal);
                    if armed && !self.follow.armed() {
                        self.said = String::from("you scrolled — following off (Home to resume)");
                    }
                }
                Intent::Zoomed(scale) => {
                    self.said = format!("zoom {:.4} beats/px, {:.5} oct/px", scale.x, scale.y);
                }
                _ => {}
            }
        }
        mech.mark_dirty_all(window);
    }

    fn tick(&mut self, mech: &mut Mech) {
        let Some(window) = self.window else { return };
        self.audio.pump();

        // Hot-plug and auto-open: the Oxygen plays Padlington as soon as it is
        // seen, the fast path straight to the voice pool.
        // The honest live-path latency, the moment the stream has run a block.
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

        self.keys.poll(&self.log);
        let mut last = None;
        self.keys.drain(|captured| last = Some(captured));
        if let Some(captured) = last {
            self.live = format!("{:?}", captured.message);
            mech.mark_dirty_all(window);
        }
        if let Some(port) = self.keys.open_port()
            && self.live.is_empty()
        {
            self.live = format!("playing from {port}");
        }

        if self.playing {
            let position = self.audio.position();
            self.playhead = self.beat_at(position.play.0);
            let rect = self.kit.rect(ROLL);
            let follow = self.follow;
            let head = self.playhead;
            if let (Some(rect), Some(pane)) = (rect, self.pane_mut()) {
                follow.advance(rect, pane, head);
            }
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
            Some(port) => format!("MIDI: {port}  {}", self.live),
            None => String::from("MIDI: (plug in a keyboard)"),
        };
        let latency = match self.latency {
            Some(e) => format!("  ·  latency ≥ {:.1} ms", e.floor_ms()),
            None => String::new(),
        };
        let line = format!(
            "space: play/stop · home: locate · {}   |   beat {:.2} · follow {} · {midi}",
            self.said,
            self.playhead,
            if self.follow.armed() { "on" } else { "OFF" }
        );
        let line = format!("{line}{latency}");
        let shaped = frame.paint.shape(&line, &TextStyle::numeric(13.0));
        frame
            .paint
            .draw_text(&shaped, Point::new(14.0, frame.size.h - 18.0), ink);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log = Log::open_default().unwrap_or_else(|_| Log::hush());

    let directory = std::env::temp_dir().join(format!("revision_roll_{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let mut project = Project::create(directory.join("roll.revision"))?;
    let (_arrangement, track) = build(&mut project, BPM, "12-ET")?;
    let mut cache = TuneCache::new();
    let roll = Roll::build(&project, &mut cache, track)?;
    log.info(
        creator::APP,
        format!(
            "MHALL: {} notes, {} rungs",
            roll.note.len(),
            roll.rung.len()
        ),
    );

    let mut audio = Audio::open_with(log, &Request::default(), |format| {
        rev_app::pad::instrument(
            Patch::harpington(),
            &BakeSpec::harpington(),
            16,
            format.sample_rate,
        )
    });

    // The whole tune in one chunk, as eng-07 does.
    let rate = audio.sample_rate();
    let mut compiler = Compiler::new(
        TempoMap::new(
            [(rev_core::tick::Tick(0), bpm_to_usec_per_quarter(BPM))],
            rate,
        ),
        vec![track],
    );
    let span = (roll.beat_extent + 8.0) * 60.0 / BPM * f64::from(rate);
    let chunk = compiler.chunk(&project, SampleTime(0), SampleTime(span as u64))?;
    audio.send_command(Command::now(What::TakeChunk(ChunkHandle::new(Chunk {
        from: chunk.from,
        to: chunk.to,
        note: chunk.note,
    }))));

    // Resolve the roll's tuning to a note→Hz snapshot, so a played key sounds
    // the same pitch the roll draws — the same `freq` table, so live input and
    // the picture cannot disagree (R-312).
    let snapshot = query::tuning_by_name(project.reader(), "12-ET")
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
        .unwrap_or_else(NoteHz::silent);
    let keys = match audio.take_thru() {
        Some(thru) => Keys::new(thru, snapshot),
        None => unreachable!("audio always yields its thru sender once"),
    };
    let demo_log = audio.log().clone();

    let kit = Kit::new(scene(Size::new(1200.0, 720.0), &roll), Skin::default());
    rev_ui_mech::run(Demo {
        kit,
        roll,
        keys,
        log: demo_log,
        follow: Follow::default(),
        audio,
        window: None,
        playing: false,
        playhead: 0.0,
        said: String::from("ready"),
        live: String::new(),
        latency: None,
    })?;
    let _ = std::fs::remove_dir_all(directory);
    Ok(())
}
