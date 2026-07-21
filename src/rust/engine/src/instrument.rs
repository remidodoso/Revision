//! The first instrument: the Padlington voice, as a graph description.
//!
//! Transcribed from `doc/revision_padlington_inventory.md` §4, which censused
//! the voice being ported rather than imagining one:
//!
//! ```text
//! 2 × [ BufferSource(loop, rate, detune) → Gain(1/√2) → StereoPanner(±width) ]
//!     → Gain(amp envelope) → Biquad(lowpass, Q, cutoff envelope) → out
//! ```
//!
//! Eight nodes. The two read heads take **decorrelated seeded start offsets** —
//! the trick that gets a wide stereo image out of a single bake — and the
//! panners place them at opposite sides.
//!
//! **The bake is not here.** PADsynth's table is pure data-in/data-out and
//! belongs to dsp-02; this module takes whatever table it is given, which is why
//! it can be tested against a synthetic one long before the real bake exists.

use crate::graph::{BiquadMode, Graph, GraphSpec, NodeKind, NodeSpec, ParamId};
use crate::table::TableSet;
use crate::voice::VoicePool;

/// Middle C, the reference the filter's key tracking is measured against.
///
/// The inventory calls this the one 12-ET-flavoured constant in the port, and
/// notes that **in Revision it becomes the tuning's anchor frequency**. It is a
/// field on the patch rather than a constant here so that the day a tuning
/// supplies it, nothing else moves.
pub const MIDDLE_C: f32 = 261.625_58;

/// Play-time parameters — the ones that never re-bake.
///
/// The bake-relevant fields (source, harmonics, bandwidth, stretch, vowel…) are
/// deliberately absent: they belong to dsp-02 and to the table's cache key, and
/// keeping the two sets apart is the partition the inventory found and validated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Patch {
    /// Amplitude envelope, in seconds; sustain is a fraction of peak.
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,

    /// Stereo spread of the two read heads, 0..1.
    pub width: f32,

    /// Filter cutoff in hertz at the reference pitch, its resonance, how many
    /// octaves the envelope opens it, and how much it follows pitch.
    pub cutoff: f32,
    pub resonance: f32,
    pub filter_env: f32,
    pub key_track: f32,
    /// The pitch the cutoff and key tracking are measured from.
    pub reference_hz: f32,

    /// Pitch attack: cents away at the start, and how long it takes to arrive.
    pub pitch_attack: f32,
    pub pitch_attack_time: f32,
}

impl Default for Patch {
    /// The inventory's defaults, unchanged.
    fn default() -> Patch {
        Patch {
            attack: 0.4,
            decay: 1.0,
            sustain: 0.9,
            release: 1.2,
            width: 0.7,
            cutoff: 7_000.0,
            resonance: 0.5,
            filter_env: 0.0,
            key_track: 0.3,
            reference_hz: MIDDLE_C,
            pitch_attack: 0.0,
            pitch_attack_time: 0.08,
        }
    }
}

impl Patch {
    /// A plucked patch — the harpsichord/clavinet direction.
    ///
    /// A short, decaying sound rather than a pad, chosen deliberately: note
    /// boundaries are *audible*, so a scheduling error can be heard rather than
    /// smeared. It is also the harder case for PADsynth, which is why it is
    /// worth doing first.
    pub fn plucked() -> Patch {
        Patch {
            attack: 0.002,
            decay: 0.35,
            sustain: 0.0,
            release: 0.12,
            filter_env: 2.5,
            cutoff: 2_400.0,
            resonance: 0.9,
            ..Patch::default()
        }
    }

    /// **Harpington** — the play-time half of Notorolla's catalog preset, the
    /// first patch here that was tuned by ear rather than chosen by default.
    ///
    /// A 4 ms attack onto a half-second decay, sustaining at 0.089 — nearly
    /// nothing, so the note is essentially all decay, which is what a plucked
    /// string is. The 0.44 s release is long against that: the sound keeps
    /// ringing after the key lifts, the way a damper takes a moment to land.
    ///
    /// The filter is the other half of the character: 1560 Hz opening 1.5
    /// octaves on the envelope, tracking pitch at 0.678 — enough that high
    /// notes stay bright without the top of the keyboard turning glassy.
    ///
    /// Its bake half is [`rev_dsp::BakeSpec::harpington`], which is where the
    /// timbre proper lives.
    pub fn harpington() -> Patch {
        Patch {
            attack: 0.003_963_372,
            decay: 0.558_454_5,
            sustain: 0.089,
            release: 0.442_967_34,
            width: 0.467_567_95,
            cutoff: 1_560.506_4,
            resonance: 0.5,
            filter_env: 1.544,
            key_track: 0.678,
            reference_hz: MIDDLE_C,
            pitch_attack: 0.0,
            pitch_attack_time: 0.146_554_78,
        }
    }
}

/// Which node is which, once the graph is built. Held so the envelope knows
/// where to schedule rather than searching by kind.
#[derive(Debug, Clone, Copy)]
struct Layout {
    head: [usize; 2],
    panner: [usize; 2],
    amp: usize,
    filter: usize,
}

/// One instrument: a patch, its tables, and the voices that play it.
pub struct Instrument {
    patch: Patch,
    table: TableSet,
    pool: VoicePool,
    layout: Layout,
    sample_rate: u32,
}

impl Instrument {
    /// Build the graph and the whole voice pool. **Everything that allocates
    /// happens here**, on the app thread, once.
    pub fn new(
        patch: Patch,
        table: TableSet,
        voices: usize,
        sample_rate: u32,
    ) -> Result<Instrument, crate::graph::BuildError> {
        let (spec, described) = Self::describe(&patch);
        let graph = Graph::build(&spec)?;
        // Translate description indices into wiring positions. The topological
        // sort reorders, so a spec index used against a voice's parameters would
        // land on whichever node happened to sort into that slot — which is a
        // silent wrong answer, not an error.
        let place = |node: usize| {
            graph
                .position_of(crate::graph::NodeRef(node as u16))
                .expect("every described node is in the compiled graph")
        };
        let layout = Layout {
            head: [place(described.head[0]), place(described.head[1])],
            panner: [place(described.panner[0]), place(described.panner[1])],
            amp: place(described.amp),
            filter: place(described.filter),
        };
        let mut pool = VoicePool::new(graph, voices, sample_rate);
        // Five time constants is the usual reading of "inaudible" for an
        // exponential approach, and the pool needs a length rather than an
        // asymptote to decide when a voice has finished.
        pool.set_release((patch.release * 5.0 * sample_rate as f32) as u64);

        Ok(Instrument {
            patch,
            table,
            pool,
            layout,
            sample_rate,
        })
    }

    /// The voice, as data. A patch editor would edit this; a script could
    /// generate it; serializing it is what makes a preset a file.
    fn describe(patch: &Patch) -> (GraphSpec, Layout) {
        let mut spec = GraphSpec::new();
        let table = crate::table::TableId(0);
        let mut head = [0usize; 2];
        let mut panner = [0usize; 2];

        let amp = {
            // Declared before the heads so the loop can connect to it.
            let node = spec.add(NodeSpec::new(NodeKind::Gain).with(ParamId::Gain, 0.0));
            node.0 as usize
        };

        for (index, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let source = spec.add(NodeSpec::new(NodeKind::BufferSource {
                table,
                looping: true,
            }));
            // 1/√2 restores the table's baked RMS from two decorrelated heads.
            let trim = spec.add(
                NodeSpec::new(NodeKind::Gain).with(ParamId::Gain, std::f32::consts::FRAC_1_SQRT_2),
            );
            let pan = spec.add(
                NodeSpec::new(NodeKind::StereoPanner)
                    .with(ParamId::Pan, side * patch.width.clamp(0.0, 1.0)),
            );
            spec.connect(source, trim).connect(trim, pan);
            spec.connect(pan, crate::graph::NodeRef(amp as u16));
            head[index] = source.0 as usize;
            panner[index] = pan.0 as usize;
        }

        let filter = spec.add(
            NodeSpec::new(NodeKind::Biquad {
                mode: BiquadMode::Lowpass,
            })
            .with(ParamId::Q, patch.resonance)
            .with(ParamId::Frequency, patch.cutoff),
        );
        spec.connect(crate::graph::NodeRef(amp as u16), filter);
        spec.output(filter);

        (
            spec,
            Layout {
                head,
                panner,
                amp,
                filter: filter.0 as usize,
            },
        )
    }

    pub fn patch(&self) -> &Patch {
        &self.patch
    }

    pub fn pool(&self) -> &VoicePool {
        &self.pool
    }

    pub fn table(&self) -> &TableSet {
        &self.table
    }

    fn frames(&self, seconds: f32) -> u64 {
        (seconds.max(0.0) * self.sample_rate as f32) as u64
    }

    /// Start a note. `dur` and `offset` are in frames; `offset` is where in the
    /// coming quantum it begins, which is what makes onsets sample-accurate.
    pub fn note_on(&mut self, hz: f32, level: f32, dur: u64, offset: usize, seed: u64) -> bool {
        // Choose the table baked nearest this pitch. With one table this is
        // `TableId(0)` and nothing changes; with a set baked every half octave
        // it is what keeps the read speed inside the window the bake's band
        // limit assumes (dsp-02 §4.4).
        let table = self.table.nearest(hz);
        if !self.pool.start(hz, level, dur, offset, seed, table) {
            return false;
        }
        let slot = self.pool.newest();
        self.schedule(slot, hz, dur);
        true
    }

    /// Schedule one voice's envelopes. The math is the inventory's §4, with the
    /// one JS-ism retired: that source never knew when a note would end, so it
    /// clamped the attack against the duration; we are told the duration up
    /// front (R-402a) and do not have to guess.
    fn schedule(&mut self, slot: usize, hz: f32, dur: u64) {
        let patch = self.patch;
        let rate = self.sample_rate as f32;
        let attack = self.frames(patch.attack);
        let decay_tau = (patch.decay * rate).max(1.0);
        let release_tau = (patch.release * rate).max(1.0);

        // The filter's base cutoff follows pitch by `key_track`, measured from
        // the reference. Clamped well inside Nyquist: a cutoff above it is not a
        // filter, it is an assertion failure waiting for a high note.
        let base_cut = (patch.cutoff * (hz / patch.reference_hz).powf(patch.key_track))
            .clamp(60.0, rate * 0.45);
        let peak_cut = (base_cut * 2f32.powf(patch.filter_env)).clamp(60.0, rate * 0.45);
        let sustain_cut =
            (base_cut * 2f32.powf(patch.filter_env * patch.sustain)).clamp(60.0, rate * 0.45);

        let Some(voice) = self.pool.voice_mut(slot) else {
            return;
        };

        // --- Amplitude: a *linear* attack, deliberately. An exponential ramp
        // from near zero is inaudible and then snaps; linear is what the ear
        // reads as an attack.
        if let Some(param) = voice.param_mut(self.layout.amp, ParamId::Gain) {
            let schedule = param.schedule();
            schedule.reset(0.0);
            schedule.set_value_at_time(0.0, 0);
            schedule.linear_ramp_to_value_at_time(1.0, attack.max(1));
            schedule.set_target_at_time(patch.sustain, attack.max(1), decay_tau);
            // The release is scheduled now because the duration is known now.
            schedule.set_target_at_time(0.0, dur.max(attack), release_tau);
        }

        // --- Filter: the same shape in octave space, which is why it is an
        // *exponential* ramp — log-linear there is linear here.
        if let Some(param) = voice.param_mut(self.layout.filter, ParamId::Frequency) {
            let schedule = param.schedule();
            schedule.reset(base_cut);
            schedule.set_value_at_time(base_cut, 0);
            schedule.exponential_ramp_to_value_at_time(peak_cut, attack.max(1));
            schedule.set_target_at_time(sustain_cut, attack.max(1), decay_tau);
            schedule.set_target_at_time(base_cut, dur.max(attack), release_tau);
        }

        // --- Pitch attack: start off-pitch and slide home. Signed, so a positive
        // value approaches from above.
        if patch.pitch_attack != 0.0 {
            let tau = (patch.pitch_attack_time * rate / 4.0).max(1.0);
            for head in self.layout.head {
                if let Some(param) = voice.param_mut(head, ParamId::Detune) {
                    let schedule = param.schedule();
                    schedule.reset(patch.pitch_attack);
                    schedule.set_value_at_time(patch.pitch_attack, 0);
                    schedule.set_target_at_time(0.0, 0, tau);
                }
            }
        }
    }

    /// Release every sounding voice.
    pub fn all_notes_off(&mut self) {
        self.pool.release_all();
    }

    /// Render a quantum, or the part of one that fits in what is left of a
    /// device block. `phase` is how far into the quantum this call begins —
    /// zero means a new quantum, which is when per-quantum parameters update.
    pub fn render(&mut self, span: crate::voice::Span, out: &mut [f32]) {
        self.pool.render(&self.table, span, out);
    }
}

#[cfg(test)]
mod test;
