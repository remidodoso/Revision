//! A voice is a graph instance, and the pool is the point.
//!
//! **Everything is allocated when the instrument loads**: N voices, each with
//! its graph's nodes and buffers. Note-on takes a voice off a free list;
//! note-off moves it to releasing; **the voice returns itself when its release
//! finishes, not when its duration expires**.
//!
//! That last rule is the one that has to be right from the start. It is what
//! makes a release tail expressible at all — the DX7 harpsichord's key-up
//! pluck, the damper falling on a clavinet — and it is why R-402a matters: every
//! pooled voice eventually returns, without exception, so **a voice sounding
//! with no scheduled end is by construction a bug** and can be asserted on.

use crate::graph::{Graph, NodeKind, ParamId, QUANTUM};
use crate::table::TableSet;

/// What a render call needs to know about where it sits in time.
///
/// Grouped rather than passed as loose arguments because the three travel
/// together and mean nothing apart: a call renders `frames` frames beginning
/// `phase` frames into a quantum, writing planes `stride` apart.
#[derive(Debug, Clone, Copy)]
pub struct Span {
    /// How far into the current quantum this call begins. Zero means a new
    /// quantum, which is when per-quantum parameters update and when voice
    /// state may change.
    pub phase: usize,
    pub frames: usize,
    /// Distance between channel planes in the destination.
    pub stride: usize,
}

/// Where a voice is in its life.
///
/// `Held` and `Releasing` are distinguished because stealing needs to tell them
/// apart: a releasing voice is making a sound nobody is playing any more, and is
/// the right thing to take first. That distinction exists only because release
/// is a real state rather than a fade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    Free,
    Held,
    Releasing,
}

/// Per-node run-time state. Parallel to the graph's wiring.
#[derive(Debug, Clone)]
enum NodeState {
    Source { position: f64 },
    Biquad { z1: [f32; 2], z2: [f32; 2] },
    None,
}

/// One sounding voice.
///
/// **It does not own its graph.** The pool holds one, shared: sixteen voices of
/// one instrument have identical wiring, and a copy each would be waste. It is
/// also what lets the node loop hold `&Graph` and `&mut buffers` at once, which
/// is the difference between rendering in place and cloning a buffer per node.
pub struct Voice {
    state: VoiceState,
    node: Vec<NodeState>,
    /// **Per voice, not per graph.** Every sounding note has its own envelope,
    /// at its own point in its own life, so the schedule cannot live in the
    /// shared wiring. Allocated with the pool and reset at note-on.
    param: Vec<Vec<crate::param::Param>>,
    /// Frames since this note began. What the automation is evaluated against.
    elapsed: u64,
    /// `elapsed` as it stood at the last quantum boundary.
    ///
    /// **k-rate parameters are evaluated against this, not against `elapsed`.**
    /// A device block may end part-way through a quantum, so a render call is
    /// not a quantum — and a filter cutoff evaluated once per *call* would
    /// update at a rate that depended on the hardware, which is the exact defect
    /// the fixed quantum exists to prevent. Found by a test that rendered the
    /// same tune at two buffer sizes and compared the bits.
    k_elapsed: u64,
    /// When the release began, and how long it lasts. A release *tail* is why a
    /// voice cannot be reclaimed at note-off (R-402a); this is what makes the
    /// tail a length rather than a hope.
    released_at: u64,
    /// `buffer_count` buffers of `QUANTUM` frames × 2 channels, interleaved by
    /// plane: `buffer[b][c * QUANTUM + f]`.
    buffer: Vec<Vec<f32>>,
    /// Frequency this voice was started at, and its level.
    hz: f32,
    level: f32,
    /// Which table this note reads, overriding what the graph was described
    /// with.
    ///
    /// **Per note, because the right table depends on the pitch.** The
    /// description fixes a `TableId` when the graph is built, which was right
    /// when an instrument had one table; a set baked every half octave has to
    /// be chosen from at note-on, from the note's own frequency (dsp-02 §8).
    /// `None` keeps the described table, so a one-table instrument is unchanged.
    table: Option<crate::table::TableId>,
    /// Frames still to wait before the voice begins, within the current quantum.
    /// This is what makes onsets sample-accurate without splitting the block
    /// (eng-02 §8).
    start_offset: usize,
    /// Frames remaining of the note's written duration. Articulation *input*:
    /// reaching zero begins the release, it does not silence the voice.
    remaining: u64,
    /// Deterministic, and a pure function of the note and the instrument seed —
    /// never of the pool slot, which is an allocation detail. A slot-derived
    /// seed would make the same project render differently depending on what
    /// else was sounding, and R-1402 would fail intermittently.
    seed: u64,
    sample_rate: u32,
}

impl Voice {
    fn new(graph: &Graph, sample_rate: u32) -> Voice {
        let node = graph
            .wiring()
            .iter()
            .map(|w| match w.kind {
                NodeKind::BufferSource { .. } => NodeState::Source { position: 0.0 },
                NodeKind::Biquad { .. } => NodeState::Biquad {
                    z1: [0.0; 2],
                    z2: [0.0; 2],
                },
                _ => NodeState::None,
            })
            .collect();
        let buffer = (0..graph.buffer_count())
            .map(|_| vec![0.0f32; QUANTUM * 2])
            .collect();
        let param = graph.wiring().iter().map(|w| w.param.clone()).collect();
        Voice {
            state: VoiceState::Free,
            node,
            param,
            elapsed: 0,
            buffer,
            hz: 0.0,
            level: 0.0,
            table: None,
            start_offset: 0,
            k_elapsed: 0,
            released_at: 0,
            remaining: 0,
            seed: 0,
            sample_rate,
        }
    }

    pub fn state(&self) -> VoiceState {
        self.state
    }

    pub fn hz(&self) -> f32 {
        self.hz
    }

    /// Begin. `offset` is where in the coming quantum the note starts.
    #[allow(clippy::too_many_arguments)]
    fn start(
        &mut self,
        graph: &Graph,
        hz: f32,
        level: f32,
        dur: u64,
        offset: usize,
        seed: u64,
        table: Option<crate::table::TableId>,
    ) {
        // Every note starts from the patch's own values: an envelope left over
        // from the previous note in this slot would make a voice's sound depend
        // on what it happened to play before.
        for (slot, wiring) in self.param.iter_mut().zip(graph.wiring()) {
            slot.clone_from(&wiring.param);
        }
        self.elapsed = 0;
        self.k_elapsed = 0;
        self.released_at = 0;
        self.hz = hz;
        self.level = level;
        self.table = table;
        self.remaining = dur;
        self.start_offset = offset.min(QUANTUM);
        self.seed = seed;
        self.state = VoiceState::Held;

        // Read heads take decorrelated start offsets — the trick that gets a wide
        // stereo image out of one bake. Seeded, unlike the source this is ported
        // from, per R-706.
        let mut scramble = seed;
        for state in &mut self.node {
            if let NodeState::Source { position } = state {
                scramble = scramble
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                // **Bounded.** A read position is an `f64` that is incremented by
                // a fraction of a sample every frame, so it must stay small
                // enough that the increment survives: at 2^53 an `f64`'s ULP is
                // 2, and adding 3.3 to it quantizes to garbage — the read speed
                // becomes wrong, so the *pitch* becomes wrong, differently for
                // every seed. Heard as intermittently wrong notes before it was
                // measured. 2^20 is far larger than any table and leaves the
                // increment exact.
                *position = (scramble % (1 << 20)) as f64;
            }
            if let NodeState::Biquad { z1, z2 } = state {
                *z1 = [0.0; 2];
                *z2 = [0.0; 2];
            }
        }
    }

    /// Begin the release. The voice keeps sounding; it returns to the pool when
    /// the release finishes.
    pub fn release(&mut self) {
        if self.state == VoiceState::Held {
            self.state = VoiceState::Releasing;
            self.released_at = self.elapsed;
            self.remaining = 0;
        }
    }

    /// A parameter of one node, for an instrument scheduling its envelope.
    pub fn param_mut(&mut self, node: usize, id: ParamId) -> Option<&mut crate::param::Param> {
        self.param.get_mut(node)?.iter_mut().find(|p| p.id == id)
    }

    pub fn elapsed(&self) -> u64 {
        self.elapsed
    }

    /// Render one quantum, adding into `out` (planar, `frames` per channel).
    pub fn render(
        &mut self,
        graph: &Graph,
        table: &TableSet,
        release_frames: u64,
        span: Span,
        out: &mut [f32],
    ) {
        let Span {
            phase,
            frames,
            stride,
        } = span;
        if self.state == VoiceState::Free {
            return;
        }
        // A new quantum begins: this is where per-quantum parameters are allowed
        // to change, and the only place.
        if phase == 0 {
            self.k_elapsed = self.elapsed;
        }
        // Whether this call *finishes* a quantum. State transitions happen only
        // then — see the note on reclamation below.
        let ends_quantum = phase + frames >= QUANTUM;
        let begin = self.start_offset.min(frames);
        self.start_offset = self.start_offset.saturating_sub(frames);

        // Walk the wiring in order. No traversal, no sorting, no lookup: all of
        // that happened when the graph was built.
        for index in 0..graph.wiring().len() {
            self.run_node(graph, index, table, begin, frames);
        }

        let source = graph.output();
        let channel = graph.channel_out() as usize;
        for c in 0..channel.min(2) {
            let plane = &self.buffer[source][c * QUANTUM..][..frames];
            let target = &mut out[c * stride..][..frames];
            for (t, s) in target.iter_mut().zip(plane) {
                *t += s * self.level;
            }
        }

        // The written duration is articulation input, not a gate: reaching zero
        // starts the release rather than silencing anything (R-402a).
        let advanced = (frames - begin) as u64;
        self.elapsed += advanced;

        // Time always advances — a partial call is still elapsed time, and a
        // note that only aged on whole quanta would last longer at small buffer
        // sizes.
        if self.state == VoiceState::Held {
            self.remaining = self.remaining.saturating_sub(advanced);
        }

        // **State changes only at quantum boundaries**, though. A device block
        // may end part-way through a quantum, and if a voice could be reclaimed
        // on that partial call, the free list would come back in a different
        // order than at another buffer size. A later note would land in a
        // different slot, voices would be summed in a different order, and
        // floating-point addition is not associative — so the same project would
        // render differently on different hardware. Found by a test comparing
        // 64-frame and 1024-frame renders of the same tune, which diverged in
        // the last bits and grew from there through a resonant filter.
        if !ends_quantum {
            return;
        }
        if self.state == VoiceState::Held {
            if self.remaining == 0 {
                self.state = VoiceState::Releasing;
                self.released_at = self.elapsed;
            }
        } else if self.state == VoiceState::Releasing
            && self.elapsed.saturating_sub(self.released_at) >= release_frames
        {
            // The tail is over. Note *when* this happens: not at note-off, not
            // when the written duration expired, but when the release finished.
            self.state = VoiceState::Free;
        }
    }

    fn run_node(
        &mut self,
        graph: &Graph,
        index: usize,
        table: &TableSet,
        begin: usize,
        frames: usize,
    ) {
        let wiring = &graph.wiring()[index];
        let output = wiring.output;
        let channel = wiring.channel as usize;
        let channel_in = wiring.channel_in as usize;

        // **Clear before gathering.** Buffers are recycled *within* a pass — a
        // buffer freed when its last consumer ran is handed to a later node —
        // so it still holds that earlier node's output. Accumulating into it
        // without clearing adds a stale signal that bypasses everything in
        // between: found because a filter fed by a near-silent amplitude
        // envelope was still loud, having inherited a panner's output.
        self.buffer[output].fill(0.0);

        // Gather: sum every input into the output buffer, then operate on it in
        // place. Connecting two outputs to one input means summing them, and the
        // voice being ported relies on it — two panned read heads into one gain.
        // Buffer assignment guarantees the output is not among the inputs, so
        // the accumulation is safe.
        for &from in &wiring.input {
            let (src, dst) = two_mut(&mut self.buffer, from, output);
            for c in 0..channel_in {
                for frame in begin..frames {
                    dst[c * QUANTUM + frame] += src[c * QUANTUM + frame];
                }
            }
        }
        // Read parameters straight out of the wiring. Collecting them into a
        // `Vec` first would allocate — on the audio thread, once per node, per
        // quantum — which is precisely what the guard exists to forbid.
        // Parameters come from this voice's own copies, evaluated at this
        // voice's own age. Read straight out of the array — collecting them
        // first would allocate, on the audio thread, once per node per quantum.
        let elapsed = self.elapsed;
        let own = &self.param[index];
        // k-rate: evaluated at the quantum boundary, so a device block that ends
        // mid-quantum cannot change it.
        let k_elapsed = self.k_elapsed;
        let value = |id: ParamId| {
            own.iter()
                .find(|p| p.id == id)
                .map(|p| p.at(k_elapsed))
                .unwrap_or_else(|| id.default_value())
        };

        match &wiring.kind {
            NodeKind::BufferSource { table: id, looping } => {
                let Some(baked) = table.get(self.table.unwrap_or(*id)) else {
                    return;
                };
                let looping = *looping;
                let rate = f64::from(value(ParamId::PlaybackRate))
                    * 2f64.powf(f64::from(value(ParamId::Detune)) / 1200.0)
                    * f64::from(self.hz / baked.base_hz());
                let NodeState::Source { position } = &mut self.node[index] else {
                    return;
                };
                let len = baked.len() as f64;
                let plane = &mut self.buffer[output][begin..frames];
                for sample in plane.iter_mut() {
                    if !looping && *position >= len {
                        break;
                    }
                    *sample = baked.read(*position);
                    *position += rate;
                    // Wrap as we go rather than only in `read`, so the position
                    // stays small forever. A note held for ten minutes would
                    // otherwise drift into the range where the increment starts
                    // losing precision again.
                    if looping && *position >= len {
                        *position -= len;
                    }
                }
            }
            NodeKind::Gain => {
                let dst = &mut self.buffer[output];
                for c in 0..channel {
                    for frame in begin..frames {
                        // a-rate: the gain carries the amplitude envelope, so it
                        // is evaluated per sample rather than per quantum.
                        let gain = own
                            .iter()
                            .find(|p| p.id == ParamId::Gain)
                            .map(|p| p.at(elapsed + (frame - begin) as u64))
                            .unwrap_or(1.0);
                        dst[c * QUANTUM + frame] *= gain;
                    }
                }
            }
            NodeKind::StereoPanner => {
                // Equal power: a centred signal keeps its energy, and a sweep
                // does not dip in the middle the way a linear pan does.
                let pan = value(ParamId::Pan).clamp(-1.0, 1.0);
                let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
                let (left, right) = (angle.cos(), angle.sin());
                let dst = &mut self.buffer[output];
                for frame in begin..frames {
                    // Read before either write: channel 0 is both source and
                    // destination when a node works in place.
                    let mono = dst[frame];
                    dst[frame] = mono * left;
                    dst[QUANTUM + frame] = mono * right;
                }
            }
            NodeKind::Biquad { mode } => {
                let hz = value(ParamId::Frequency);
                let q = value(ParamId::Q).max(0.0001);
                let coefficient = Biquad::design(*mode, hz, q, self.sample_rate);
                let NodeState::Biquad { z1, z2 } = &mut self.node[index] else {
                    return;
                };
                let (z1, z2) = (*z1, *z2);
                let (mut s1, mut s2) = (z1, z2);
                let dst = &mut self.buffer[output];
                for c in 0..channel.min(2) {
                    let (mut a, mut b) = (s1[c], s2[c]);
                    for frame in begin..frames {
                        let x = dst[c * QUANTUM + frame];
                        let y = coefficient.b0 * x + a;
                        a = coefficient.b1 * x - coefficient.a1 * y + b;
                        b = coefficient.b2 * x - coefficient.a2 * y;
                        dst[c * QUANTUM + frame] = y;
                    }
                    s1[c] = a;
                    s2[c] = b;
                }
                if let NodeState::Biquad { z1, z2 } = &mut self.node[index] {
                    *z1 = s1;
                    *z2 = s2;
                }
            }
        }
    }
}

/// Two disjoint buffers by index, without copying either.
///
/// Buffer assignment guarantees a node never writes the buffer it reads, so the
/// two indices always differ; `split_at_mut` turns that guarantee into something
/// the borrow checker accepts. The obvious alternative — cloning the source —
/// would allocate half a kilobyte per node per quantum on the audio thread.
fn two_mut(buffer: &mut [Vec<f32>], from: usize, to: usize) -> (&[f32], &mut [f32]) {
    debug_assert_ne!(from, to, "a node never writes the buffer it reads");
    if from < to {
        let (head, tail) = buffer.split_at_mut(to);
        (&head[from], &mut tail[0])
    } else {
        let (head, tail) = buffer.split_at_mut(from);
        (&tail[0], &mut head[to])
    }
}

/// Direct-form-II transposed coefficients.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    /// The RBJ cookbook formulas, which are what Web Audio specifies.
    fn design(mode: crate::graph::BiquadMode, hz: f32, q: f32, sample_rate: u32) -> Biquad {
        use crate::graph::BiquadMode;
        let nyquist = sample_rate as f32 * 0.5;
        let hz = hz.clamp(10.0, nyquist * 0.95);
        let w = std::f32::consts::TAU * hz / sample_rate as f32;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;

        let (b0, b1, b2) = match mode {
            BiquadMode::Lowpass => {
                let b1 = 1.0 - cos;
                (b1 * 0.5, b1, b1 * 0.5)
            }
            BiquadMode::Highpass => {
                let b1 = -(1.0 + cos);
                ((1.0 + cos) * 0.5, b1, (1.0 + cos) * 0.5)
            }
            BiquadMode::Bandpass => (alpha, 0.0, -alpha),
        };
        Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha) / a0,
        }
    }
}

/// Every voice of one instrument, plus the free list.
pub struct VoicePool {
    /// One graph, shared by every voice.
    graph: Graph,
    /// How long a release lasts, in frames. An instrument property: the
    /// envelope's shape is scheduled per voice, and this is how long the pool
    /// waits before believing the sound is over. A `setTarget` release is
    /// asymptotic and never truly reaches zero, so somebody has to decide when
    /// it has become inaudible — about five time constants.
    release_frames: u64,
    voice: Vec<Voice>,
    /// Indices of free voices. A `Vec` used as a stack: pushing and popping are
    /// the only operations, and both are real-time safe because the capacity was
    /// fixed when the pool was built.
    free: Vec<usize>,
    /// How many notes had to take a voice from another. **Not** a starvation
    /// count: stealing always succeeds while the pool is non-empty, so
    /// starvation is unreachable by construction. What is worth counting is how
    /// often it happened, because that is the signal that the voice count is too
    /// low — a number nobody has otherwise.
    stolen: u64,
    /// Where the last note went.
    newest: usize,
}

impl VoicePool {
    /// Build every voice now, on the app thread. Nothing here happens again.
    pub fn new(graph: Graph, voices: usize, sample_rate: u32) -> VoicePool {
        let voices = voices.max(1);
        let voice: Vec<Voice> = (0..voices)
            .map(|_| Voice::new(&graph, sample_rate))
            .collect();
        VoicePool {
            graph,
            release_frames: 0,
            free: (0..voices).rev().collect(),
            voice,
            stolen: 0,
            newest: 0,
        }
    }

    /// Set the release tail length. Five time constants is the usual reading of
    /// "inaudible" for an exponential approach.
    pub fn set_release(&mut self, frames: u64) {
        self.release_frames = frames;
    }

    pub fn release_frames(&self) -> u64 {
        self.release_frames
    }

    /// A voice, for an instrument scheduling its envelope after note-on.
    pub fn voice_mut(&mut self, slot: usize) -> Option<&mut Voice> {
        self.voice.get_mut(slot)
    }

    /// The slot the last successful [`VoicePool::start`] used.
    ///
    /// An instrument schedules its envelope immediately after starting a note,
    /// and needs to know where the note went. Returning it from `start` would be
    /// tidier; this keeps `start` returning a plain success, which is what every
    /// other caller wants.
    pub fn newest(&self) -> usize {
        self.newest
    }

    pub fn len(&self) -> usize {
        self.voice.len()
    }

    pub fn is_empty(&self) -> bool {
        self.voice.is_empty()
    }

    pub fn free_count(&self) -> usize {
        self.free.len()
    }

    /// Notes that displaced another. Rising steadily means the instrument wants
    /// more voices.
    pub fn stolen(&self) -> u64 {
        self.stolen
    }

    pub fn sounding(&self) -> usize {
        self.voice
            .iter()
            .filter(|v| v.state != VoiceState::Free)
            .count()
    }

    /// Start a note. Steals if it must; never allocates, and never fails
    /// silently — a steal is recorded.
    ///
    /// Returns false only if the pool is empty, which [`VoicePool::new`] does not
    /// permit. The signature keeps the failure expressible rather than assuming
    /// the invariant holds forever.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        hz: f32,
        level: f32,
        dur: u64,
        offset: usize,
        seed: u64,
        table: Option<crate::table::TableId>,
    ) -> bool {
        let slot = match self.free.pop() {
            Some(slot) => slot,
            None => match self.steal() {
                Some(slot) => {
                    self.stolen += 1;
                    slot
                }
                None => return false,
            },
        };
        self.voice[slot].start(&self.graph, hz, level, dur, offset, seed, table);
        self.newest = slot;
        true
    }

    /// Take the voice that is losing least. **A releasing voice goes first** —
    /// it is making a sound nobody is playing any more.
    fn steal(&mut self) -> Option<usize> {
        self.voice
            .iter()
            .position(|v| v.state == VoiceState::Releasing)
            .or_else(|| {
                // Otherwise the oldest held voice. With no age recorded yet, the
                // lowest slot stands in — stated rather than dressed up, and
                // replaced when voices carry a start time.
                self.voice.iter().position(|v| v.state == VoiceState::Held)
            })
    }

    /// Release every held voice — the transport's `AllNotesOff`.
    pub fn release_all(&mut self) {
        for voice in &mut self.voice {
            voice.release();
        }
    }

    /// Render one quantum of every sounding voice, and reclaim the ones that
    /// finished. **Reclamation happens here and only here**, which is what makes
    /// the invariant checkable: a voice not free at the end of this call is
    /// still making a sound.
    pub fn render(&mut self, table: &TableSet, span: Span, out: &mut [f32]) {
        for (slot, voice) in self.voice.iter_mut().enumerate() {
            if voice.state == VoiceState::Free {
                continue;
            }
            voice.render(&self.graph, table, self.release_frames, span, out);
            if voice.state == VoiceState::Free {
                self.free.push(slot);
            }
        }
    }
}

#[cfg(test)]
mod test;
