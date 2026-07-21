//! The node graph: described as data, compiled to something the callback walks.
//!
//! **A patch is data** (R-621, R-103), so a node is a value in a closed enum
//! rather than a boxed trait object. Adding a kind is then a compile error
//! everywhere it must be handled, a graph serializes without ceremony, and the
//! pool can pre-build every voice without allocating a vtable.
//!
//! Everything decidable before the deadline is decided before the deadline:
//! dependency order, channel counts, and buffer assignment are all resolved when
//! the graph is built, on the app thread. The real-time side walks a `Vec`.
//!
//! Approved at eng-02; see `doc/completed/revision_eng02_proposal.md`.

use crate::param::Param;
use crate::table::TableId;

/// The fixed render quantum, in frames.
///
/// **Not the device's block size.** The device hands us whatever it likes — 480
/// frames on one machine, something else on another — and a graph that processed
/// in device blocks would evaluate k-rate parameters at a rate that depended on
/// the hardware, so the same project would render differently on different
/// machines and R-1402 would break silently.
///
/// Quantum boundaries fall at multiples of this from the **session start**, not
/// from the block start, so they are identical however the device chops up time.
/// At 48 kHz this is a control rate of 375 Hz, which is what the voice being
/// ported was written against.
pub const QUANTUM: usize = 128;

/// Which node, within one graph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeRef(pub u16);

/// What a node *is*.
///
/// Closed on purpose: a third party cannot add a kind without editing this
/// crate, which is the right trade when plugins live out of process (R-1208).
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// A read head over a baked table. Two per Padlington voice, with
    /// decorrelated seeded start offsets (eng-02 §9).
    BufferSource { table: TableId, looping: bool },
    /// Multiplies its input. The amplitude envelope, and head mixing.
    Gain,
    /// The one place mono becomes stereo. Equal-power.
    StereoPanner,
    /// The filter.
    Biquad { mode: BiquadMode },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiquadMode {
    Lowpass,
    Highpass,
    Bandpass,
}

impl NodeKind {
    /// How many channels this node emits, given what reaches it.
    ///
    /// Derived rather than configured: Padlington's graph is mono until the
    /// panner and stereo after it, and that falls out of these three lines.
    fn channel_out(&self, channel_in: u16) -> u16 {
        match self {
            NodeKind::BufferSource { .. } => 1,
            NodeKind::StereoPanner => 2,
            NodeKind::Gain | NodeKind::Biquad { .. } => channel_in.max(1),
        }
    }

    /// Whether this node reads an input at all. A source does not.
    fn takes_input(&self) -> bool {
        !matches!(self, NodeKind::BufferSource { .. })
    }

    /// A delay of at least one quantum breaks a cycle (§4). None of the current
    /// kinds does, so every cycle is currently illegal — as a specialization of
    /// the general rule, not as a wall.
    fn breaks_cycle(&self) -> bool {
        false
    }

    pub fn name(&self) -> &'static str {
        match self {
            NodeKind::BufferSource { .. } => "buffer_source",
            NodeKind::Gain => "gain",
            NodeKind::StereoPanner => "stereo_panner",
            NodeKind::Biquad { .. } => "biquad",
        }
    }
}

/// One node as described: what it is, and where its parameters start.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeSpec {
    pub kind: NodeKind,
    /// Initial parameter values, by the kind's own parameter order. Missing
    /// entries take the parameter's default.
    pub param: Vec<(ParamId, f32)>,
}

impl NodeSpec {
    pub fn new(kind: NodeKind) -> NodeSpec {
        NodeSpec {
            kind,
            param: Vec::new(),
        }
    }

    pub fn with(mut self, id: ParamId, value: f32) -> NodeSpec {
        self.param.push((id, value));
        self
    }
}

/// Every parameter any node has. One namespace, so a patch that sets `Gain` on
/// something that has no gain is a build error rather than a silent no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamId {
    /// `Gain`: the multiplier. a-rate — it carries the amplitude envelope.
    Gain,
    /// `BufferSource`: read speed, 1.0 = the table's own pitch.
    PlaybackRate,
    /// `BufferSource`: cents away from `PlaybackRate`. The pitch attack.
    Detune,
    /// `StereoPanner`: −1 left, +1 right.
    Pan,
    /// `Biquad`: corner frequency in hertz. a-rate — it carries the filter
    /// envelope.
    Frequency,
    /// `Biquad`: resonance.
    Q,
}

impl ParamId {
    /// The parameters a kind actually has, in order.
    pub fn of(kind: &NodeKind) -> &'static [ParamId] {
        match kind {
            NodeKind::BufferSource { .. } => &[ParamId::PlaybackRate, ParamId::Detune],
            NodeKind::Gain => &[ParamId::Gain],
            NodeKind::StereoPanner => &[ParamId::Pan],
            NodeKind::Biquad { .. } => &[ParamId::Frequency, ParamId::Q],
        }
    }

    pub fn default_value(self) -> f32 {
        match self {
            ParamId::Gain => 1.0,
            ParamId::PlaybackRate => 1.0,
            ParamId::Detune => 0.0,
            ParamId::Pan => 0.0,
            ParamId::Frequency => 350.0,
            ParamId::Q => 1.0,
        }
    }

    /// Per-sample or per-quantum. Fixed at build, never dynamic: the cost
    /// difference is large and the choice never changes at run time.
    pub fn is_audio_rate(self) -> bool {
        matches!(self, ParamId::Gain | ParamId::Frequency)
    }
}

/// A graph as described — serializable, editable, and what a patch actually is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GraphSpec {
    pub node: Vec<NodeSpec>,
    /// `(from, to)`: an output feeding an input.
    pub edge: Vec<(NodeRef, NodeRef)>,
    /// The node whose output leaves the voice.
    pub out: NodeRef,
}

impl GraphSpec {
    pub fn new() -> GraphSpec {
        GraphSpec::default()
    }

    /// Add a node and return its reference.
    pub fn add(&mut self, spec: NodeSpec) -> NodeRef {
        self.node.push(spec);
        NodeRef((self.node.len() - 1) as u16)
    }

    pub fn connect(&mut self, from: NodeRef, to: NodeRef) -> &mut GraphSpec {
        self.edge.push((from, to));
        self
    }

    pub fn output(&mut self, node: NodeRef) -> &mut GraphSpec {
        self.out = node;
        self
    }
}

/// Why a graph could not be built.
///
/// Every one of these is a *build* failure, on the app thread, where reporting
/// is possible. Nothing here can happen at run time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    Empty,
    /// A reference to a node that does not exist.
    NoSuchNode(NodeRef),
    /// A cycle with no delay in it. The specification mutes these; we refuse
    /// them, because silence is the hardest failure to diagnose.
    Cycle(NodeRef),
    /// Connecting a 2-channel output to a 1-channel input. Web Audio would
    /// silently mix; we do not, because implicit mixing is one of its genuinely
    /// confusing corners.
    ChannelMismatch {
        from: NodeRef,
        to: NodeRef,
        out: u16,
        expected: u16,
    },
    /// A parameter the node does not have.
    NoSuchParam(NodeRef, ParamId),
    /// A source with an input, or a node fed twice from different widths.
    BadInput(NodeRef),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Empty => write!(f, "the graph has no nodes"),
            BuildError::NoSuchNode(n) => write!(f, "node {} does not exist", n.0),
            BuildError::Cycle(n) => write!(
                f,
                "node {} is in a cycle with no delay in it; a cycle needs a delay \
                 of at least one render quantum to be computable",
                n.0
            ),
            BuildError::ChannelMismatch {
                from,
                to,
                out,
                expected,
            } => write!(
                f,
                "node {} emits {out} channels into node {}, which reads {expected}",
                from.0, to.0
            ),
            BuildError::NoSuchParam(n, p) => {
                write!(f, "node {} has no parameter {p:?}", n.0)
            }
            BuildError::BadInput(n) => write!(f, "node {} cannot take that input", n.0),
        }
    }
}

impl std::error::Error for BuildError {}

/// One node, compiled: what it is, where it reads, where it writes.
#[derive(Debug, Clone)]
pub struct Wiring {
    pub kind: NodeKind,
    /// Buffers to read. **Several are summed** — that is what connecting two
    /// outputs to one input means, and the Padlington voice does exactly it: two
    /// panned read heads into one amplitude gain.
    ///
    /// Buffer assignment guarantees the output is never one of these, so the sum
    /// can be accumulated straight into the output.
    pub input: Vec<usize>,
    pub output: usize,
    /// The width this node emits.
    pub channel: u16,
    /// The width reaching it — different from `channel` only at the panner.
    pub channel_in: u16,
    pub param: Vec<Param>,
}

/// The compiled form. The callback walks `wiring` in order and does nothing else.
#[derive(Debug, Clone)]
pub struct Graph {
    wiring: Vec<Wiring>,
    /// Where each described node ended up in `wiring`, indexed by `NodeRef`.
    ///
    /// **The two orders are not the same** — the topological sort reorders — and
    /// anything holding on to a `NodeRef` from the description must come through
    /// here before touching a voice's parameters. Scheduling an envelope on a
    /// spec index would silently land it on whichever node happened to sort into
    /// that position.
    position: Vec<usize>,
    /// How many buffers a voice needs. Each is `QUANTUM` frames × 2 channels;
    /// stereo is allocated for every buffer because the alternative is
    /// reallocating when a panner appears, and the memory is trivial.
    buffer_count: usize,
    /// Index of the buffer the last node writes.
    output: usize,
    channel_out: u16,
}

impl Graph {
    pub fn wiring(&self) -> &[Wiring] {
        &self.wiring
    }

    /// Where a described node ended up. See [`Graph::position`].
    pub fn position_of(&self, node: NodeRef) -> Option<usize> {
        self.position.get(node.0 as usize).copied()
    }

    pub fn buffer_count(&self) -> usize {
        self.buffer_count
    }

    pub fn output(&self) -> usize {
        self.output
    }

    pub fn channel_out(&self) -> u16 {
        self.channel_out
    }

    /// Resolve a description into a runnable graph.
    ///
    /// Everything expensive happens here: dependency order, channel derivation,
    /// buffer assignment, and every check that could fail. Called on the app
    /// thread, once per instrument.
    pub fn build(spec: &GraphSpec) -> Result<Graph, BuildError> {
        if spec.node.is_empty() {
            return Err(BuildError::Empty);
        }
        let count = spec.node.len();
        let valid = |n: NodeRef| (n.0 as usize) < count;
        if !valid(spec.out) {
            return Err(BuildError::NoSuchNode(spec.out));
        }

        // Adjacency and in-degree, checking references as we go.
        let mut feeds: Vec<Vec<NodeRef>> = vec![Vec::new(); count];
        let mut fed_by: Vec<Vec<NodeRef>> = vec![Vec::new(); count];
        for &(from, to) in &spec.edge {
            if !valid(from) {
                return Err(BuildError::NoSuchNode(from));
            }
            if !valid(to) {
                return Err(BuildError::NoSuchNode(to));
            }
            if !spec.node[to.0 as usize].kind.takes_input() {
                return Err(BuildError::BadInput(to));
            }
            feeds[from.0 as usize].push(to);
            fed_by[to.0 as usize].push(from);
        }

        let order = topological(spec, &feeds, &fed_by)?;
        let channel = derive_channels(spec, &order, &fed_by)?;
        let (wiring, buffer_count) = assign_buffers(spec, &order, &feeds, &fed_by, &channel)?;

        let output = wiring
            .iter()
            .zip(&order)
            .find(|(_, n)| **n == spec.out)
            .map(|(w, _)| w.output)
            .ok_or(BuildError::NoSuchNode(spec.out))?;

        let mut position = vec![0usize; count];
        for (place, node) in order.iter().enumerate() {
            position[node.0 as usize] = place;
        }

        Ok(Graph {
            wiring,
            position,
            buffer_count,
            output,
            channel_out: channel[spec.out.0 as usize],
        })
    }
}

/// Kahn's algorithm. What is left over when it stops is a cycle.
fn topological(
    spec: &GraphSpec,
    feeds: &[Vec<NodeRef>],
    fed_by: &[Vec<NodeRef>],
) -> Result<Vec<NodeRef>, BuildError> {
    let count = spec.node.len();
    let mut remaining: Vec<usize> = fed_by.iter().map(|f| f.len()).collect();
    let mut ready: Vec<NodeRef> = (0..count)
        .filter(|&n| remaining[n] == 0)
        .map(|n| NodeRef(n as u16))
        .collect();
    let mut order = Vec::with_capacity(count);

    while let Some(node) = ready.pop() {
        order.push(node);
        for &next in &feeds[node.0 as usize] {
            remaining[next.0 as usize] -= 1;
            if remaining[next.0 as usize] == 0 {
                ready.push(next);
            }
        }
    }

    if order.len() != count {
        // Whatever is left is in a cycle. Name one of them: a delay in the cycle
        // would have made it legal, and none of the current kinds is one.
        let stuck = (0..count)
            .find(|&n| remaining[n] > 0 && !spec.node[n].kind.breaks_cycle())
            .unwrap_or(0);
        return Err(BuildError::Cycle(NodeRef(stuck as u16)));
    }
    Ok(order)
}

/// Channel counts, derived in dependency order.
fn derive_channels(
    spec: &GraphSpec,
    order: &[NodeRef],
    fed_by: &[Vec<NodeRef>],
) -> Result<Vec<u16>, BuildError> {
    let mut channel = vec![0u16; spec.node.len()];
    for &node in order {
        let index = node.0 as usize;
        let sources = &fed_by[index];
        let mut incoming = 0u16;
        for &from in sources {
            let width = channel[from.0 as usize];
            if incoming == 0 {
                incoming = width;
            } else if incoming != width {
                // Two inputs of different widths summed into one node. Web Audio
                // would up-mix; we refuse, because a silent width change is
                // exactly the bug you find by ear.
                return Err(BuildError::ChannelMismatch {
                    from,
                    to: node,
                    out: width,
                    expected: incoming,
                });
            }
        }
        channel[index] = spec.node[index].kind.channel_out(incoming);
    }
    Ok(channel)
}

/// Assign each node a buffer to write, reusing buffers whose contents are dead.
///
/// A small liveness analysis, done once, on the app thread. It is why a voice's
/// memory is bounded and known before a note is ever played.
fn assign_buffers(
    spec: &GraphSpec,
    order: &[NodeRef],
    feeds: &[Vec<NodeRef>],
    fed_by: &[Vec<NodeRef>],
    channel: &[u16],
) -> Result<(Vec<Wiring>, usize), BuildError> {
    let count = spec.node.len();
    let mut buffer_of = vec![usize::MAX; count];
    // How many consumers of each node's output are still to run.
    let mut pending: Vec<usize> = feeds.iter().map(|f| f.len()).collect();
    let mut free: Vec<usize> = Vec::new();
    let mut next = 0usize;
    let mut wiring = Vec::with_capacity(count);

    for &node in order {
        let index = node.0 as usize;
        let kind = spec.node[index].kind.clone();

        // Every source, not just the first: several inputs are summed.
        let input: Vec<usize> = fed_by[index]
            .iter()
            .map(|from| buffer_of[from.0 as usize])
            .collect();
        let channel_in = fed_by[index]
            .first()
            .map(|from| channel[from.0 as usize])
            .unwrap_or(0);

        let output = match free.pop() {
            Some(reused) => reused,
            None => {
                let fresh = next;
                next += 1;
                fresh
            }
        };
        buffer_of[index] = output;

        // Every source whose last consumer has now run gives its buffer back.
        for &from in &fed_by[index] {
            let source = from.0 as usize;
            pending[source] -= 1;
            if pending[source] == 0 && buffer_of[source] != output {
                free.push(buffer_of[source]);
            }
        }

        wiring.push(Wiring {
            param: build_params(spec, node)?,
            kind,
            input,
            output,
            channel: channel[index],
            channel_in,
        });
    }

    Ok((wiring, next.max(1)))
}

fn build_params(spec: &GraphSpec, node: NodeRef) -> Result<Vec<Param>, BuildError> {
    let index = node.0 as usize;
    let node_spec = &spec.node[index];
    let ids = ParamId::of(&node_spec.kind);

    // Reject values aimed at parameters this kind does not have, rather than
    // dropping them: a patch that sets a filter's Q on a gain is a mistake, and
    // a silent no-op would hide it.
    for &(id, _) in &node_spec.param {
        if !ids.contains(&id) {
            return Err(BuildError::NoSuchParam(node, id));
        }
    }

    Ok(ids
        .iter()
        .map(|&id| {
            let value = node_spec
                .param
                .iter()
                .find(|&&(set, _)| set == id)
                .map(|&(_, v)| v)
                .unwrap_or_else(|| id.default_value());
            Param::constant(id, value)
        })
        .collect())
}

#[cfg(test)]
mod test;
