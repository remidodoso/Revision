use super::*;

use crate::table::TableId;

/// The Padlington voice's shape, from the inventory: two read heads → gain →
/// panner → gain → filter. Built here because it is the graph this API exists
/// to serve, and a test that builds it proves more than a synthetic one.
fn padlington() -> GraphSpec {
    let mut spec = GraphSpec::new();
    let table = TableId(0);
    let head_a = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table,
        looping: true,
    }));
    let head_b = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table,
        looping: true,
    }));
    // Head gain restores the baked RMS from two decorrelated heads: 1/√2.
    let mix = spec.add(NodeSpec::new(NodeKind::Gain).with(ParamId::Gain, 0.707_106_77));
    let pan = spec.add(NodeSpec::new(NodeKind::StereoPanner).with(ParamId::Pan, 0.7));
    let amp = spec.add(NodeSpec::new(NodeKind::Gain));
    let filter = spec.add(NodeSpec::new(NodeKind::Biquad {
        mode: BiquadMode::Lowpass,
    }));

    spec.connect(head_a, mix)
        .connect(head_b, mix)
        .connect(mix, pan)
        .connect(pan, amp)
        .connect(amp, filter)
        .output(filter);
    spec
}

#[test]
fn the_voice_we_are_porting_builds() {
    let graph = Graph::build(&padlington()).expect("build");
    assert_eq!(graph.wiring().len(), 6);
    assert_eq!(graph.channel_out(), 2, "stereo after the panner");
}

#[test]
fn channels_are_derived_rather_than_configured() {
    // Mono until the panner, stereo after — nothing in the description says so.
    let graph = Graph::build(&padlington()).expect("build");
    let width: Vec<u16> = graph.wiring().iter().map(|w| w.channel).collect();
    assert!(width.iter().filter(|&&c| c == 1).count() >= 3, "{width:?}");
    assert!(width.iter().filter(|&&c| c == 2).count() >= 3, "{width:?}");
}

#[test]
fn nodes_come_out_in_dependency_order() {
    // The callback walks this list and never traverses an edge, so a node must
    // never appear before something that feeds it.
    let spec = padlington();
    let graph = Graph::build(&spec).expect("build");

    let mut seen = Vec::new();
    for wiring in graph.wiring() {
        for &input in &wiring.input {
            assert!(
                seen.contains(&input),
                "{} reads buffer {input} before anything wrote it",
                wiring.kind.name()
            );
            assert_ne!(
                input, wiring.output,
                "a node must not write the buffer it reads: the sum is                  accumulated into the output"
            );
        }
        seen.push(wiring.output);
    }
}

#[test]
fn a_cycle_is_refused_rather_than_muted() {
    // The specification silences the nodes in a delay-free cycle. We refuse,
    // because a patch that silently plays nothing is worse than one that will
    // not build.
    let mut spec = GraphSpec::new();
    let a = spec.add(NodeSpec::new(NodeKind::Gain));
    let b = spec.add(NodeSpec::new(NodeKind::Gain));
    spec.connect(a, b).connect(b, a).output(b);

    match Graph::build(&spec) {
        Err(BuildError::Cycle(_)) => {}
        other => panic!("expected a cycle error, got {other:?}"),
    }
}

#[test]
fn a_channel_mismatch_is_refused_rather_than_mixed() {
    // Web Audio would up-mix silently. That is one of its genuinely confusing
    // corners, and a width change you cannot see is a bug you find by ear.
    let mut spec = GraphSpec::new();
    let mono = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    let wide = spec.add(NodeSpec::new(NodeKind::StereoPanner));
    let mix = spec.add(NodeSpec::new(NodeKind::Gain));
    spec.connect(mono, wide)
        .connect(wide, mix)
        .connect(mono, mix) // 2-channel and 1-channel into one gain
        .output(mix);

    match Graph::build(&spec) {
        Err(BuildError::ChannelMismatch { .. }) => {}
        other => panic!("expected a channel mismatch, got {other:?}"),
    }
}

#[test]
fn a_parameter_the_node_does_not_have_is_refused() {
    // Not dropped: a patch that sets a filter's Q on a gain is a mistake, and a
    // silent no-op would hide it.
    let mut spec = GraphSpec::new();
    let gain = spec.add(NodeSpec::new(NodeKind::Gain).with(ParamId::Q, 4.0));
    spec.output(gain);

    match Graph::build(&spec) {
        Err(BuildError::NoSuchParam(_, ParamId::Q)) => {}
        other => panic!("expected a parameter error, got {other:?}"),
    }
}

#[test]
fn a_source_cannot_be_fed() {
    let mut spec = GraphSpec::new();
    let gain = spec.add(NodeSpec::new(NodeKind::Gain));
    let source = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    spec.connect(gain, source).output(source);

    assert!(matches!(Graph::build(&spec), Err(BuildError::BadInput(_))));
}

#[test]
fn dangling_references_are_refused() {
    let mut spec = GraphSpec::new();
    let gain = spec.add(NodeSpec::new(NodeKind::Gain));
    spec.connect(gain, NodeRef(9)).output(gain);
    assert!(matches!(
        Graph::build(&spec),
        Err(BuildError::NoSuchNode(NodeRef(9)))
    ));

    assert!(matches!(
        Graph::build(&GraphSpec::new()),
        Err(BuildError::Empty)
    ));
}

#[test]
fn buffers_are_reused_so_memory_is_bounded() {
    // A chain of gains needs a handful of buffers, not one per node: once a
    // node's only consumer has run, its buffer is dead and can be taken.
    let mut spec = GraphSpec::new();
    let mut previous = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    for _ in 0..32 {
        let next = spec.add(NodeSpec::new(NodeKind::Gain));
        spec.connect(previous, next);
        previous = next;
    }
    spec.output(previous);

    let graph = Graph::build(&spec).expect("build");
    assert_eq!(graph.wiring().len(), 33);
    assert!(
        graph.buffer_count() <= 4,
        "a chain should not need one buffer per node: {}",
        graph.buffer_count()
    );
}

#[test]
fn parameters_take_their_defaults_unless_set() {
    let graph = Graph::build(&padlington()).expect("build");
    let mix = graph
        .wiring()
        .iter()
        .find(|w| matches!(w.kind, NodeKind::Gain) && (w.param[0].value - 0.707).abs() < 0.01)
        .expect("the head-mix gain");
    assert_eq!(mix.param.len(), 1);

    let filter = graph
        .wiring()
        .iter()
        .find(|w| matches!(w.kind, NodeKind::Biquad { .. }))
        .expect("the filter");
    assert_eq!(filter.param.len(), 2, "frequency and Q");
    assert_eq!(filter.param[1].value, ParamId::Q.default_value());
}

#[test]
fn the_rate_of_a_parameter_is_fixed_at_build() {
    // Gain and cutoff carry envelopes and are per-sample; Q and pan are not.
    assert!(ParamId::Gain.is_audio_rate());
    assert!(ParamId::Frequency.is_audio_rate());
    assert!(!ParamId::Q.is_audio_rate());
    assert!(!ParamId::Pan.is_audio_rate());
}

#[test]
fn the_quantum_is_a_power_of_two_and_not_the_block_size() {
    // Anchored to the session, not to the device: the whole reason it exists.
    assert_eq!(QUANTUM, 128);
    assert!(QUANTUM.is_power_of_two());
}

#[test]
fn several_inputs_into_one_node_are_summed() {
    // Connecting two outputs to one input means summing them, and the voice
    // being ported relies on it: two panned read heads into one gain.
    let mut spec = GraphSpec::new();
    let a = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    let b = spec.add(NodeSpec::new(NodeKind::BufferSource {
        table: TableId(0),
        looping: true,
    }));
    let mix = spec.add(NodeSpec::new(NodeKind::Gain));
    spec.connect(a, mix).connect(b, mix).output(mix);

    let graph = Graph::build(&spec).expect("build");
    let mixer = graph
        .wiring()
        .iter()
        .find(|w| matches!(w.kind, NodeKind::Gain))
        .expect("the mixer");
    assert_eq!(mixer.input.len(), 2, "both heads reach it");
}

#[test]
fn the_panner_widens_and_its_width_is_recorded_both_ways() {
    // `channel_in` and `channel` differ at exactly one node, which is what the
    // gather loop needs to know to sum the right number of planes.
    let graph = Graph::build(&padlington()).expect("build");
    let panner = graph
        .wiring()
        .iter()
        .find(|w| matches!(w.kind, NodeKind::StereoPanner))
        .expect("a panner");
    assert_eq!(panner.channel_in, 1);
    assert_eq!(panner.channel, 2);
}
