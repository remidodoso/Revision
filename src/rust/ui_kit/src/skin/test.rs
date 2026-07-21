use super::*;

#[test]
fn hsl_matches_the_exhibits() {
    // Spot-checked against the CSS the skin was transcribed from: getting these
    // wrong would be silent, and the whole palette hangs off them.
    // --ro, the readout amber. Hand-computed: c = .76, x = .519, m = .24.
    assert_eq!(hsl(41.0, 1.00, 0.62), Color::rgb(255, 194, 61));
    assert_eq!(hsl(0.0, 0.0, 0.0), Color::rgb(0, 0, 0));
    assert_eq!(hsl(0.0, 0.0, 1.0), Color::rgb(255, 255, 255));
    assert_eq!(hsl(120.0, 1.0, 0.5), Color::rgb(0, 255, 0));
}

#[test]
fn the_panel_lightness_drives_everything() {
    // One scalar retunes the panel — the property that made the exhibits tunable
    // by a single slider, and the reason the skin is derived rather than tabulated.
    let dark = Skin::new(0.14);
    let light = Skin::new(0.34);
    assert!(light.panel.r > dark.panel.r);
    assert!(light.slot.r > dark.slot.r);
    assert!(light.frame.r > dark.frame.r);
    // Ink does not follow the panel: text contrast is fixed on purpose.
    assert_eq!(light.ink, dark.ink);
    assert_eq!(light.readout, dark.readout);
}

#[test]
fn derived_values_keep_their_order() {
    // The bevel reads as a bevel only while highlight > face > shadow > slot.
    let s = Skin::default();
    assert!(s.panel_hi.r > s.panel.r);
    assert!(s.panel.r > s.panel_lo.r);
    assert!(s.panel_lo.r > s.slot.r);
    assert!(s.tick_major.r > s.tick.r);
}

#[test]
fn every_role_has_its_own_band() {
    // A role keeps its colour on every panel; two roles sharing one would destroy
    // the read-at-a-glance property the whole scheme exists for.
    let s = Skin::default();
    let band: Vec<Color> = Role::ORDER.iter().map(|r| s.band(*r)).collect();
    for (n, a) in band.iter().enumerate() {
        for b in band.iter().skip(n + 1) {
            assert_ne!(a, b, "two roles share a band colour");
        }
    }
}

#[test]
fn canonical_order_is_fixed() {
    assert_eq!(
        Role::ORDER,
        [
            Role::Lfo,
            Role::Oscillator,
            Role::Filter,
            Role::Envelope,
            Role::Effect
        ]
    );
}

#[test]
fn inert_is_dimmed_but_not_erased() {
    // Present, visible, still announced — the exhibits' inert rule. A control that
    // vanished would be a different statement entirely.
    let s = Skin::default();
    let inert = s.state(State::Inert);
    let idle = s.state(State::Idle);
    assert_ne!(inert, idle, "inert is not distinguishable");
    assert_ne!(inert, s.panel, "inert dimmed all the way into the panel");
}

#[test]
fn lift_modulates_rather_than_replaces() {
    // The rule a bug taught us: interaction state must not overwrite intrinsic
    // state, or a latched control reads as unlatched while hovered.
    let s = Skin::default();
    for state in [State::Idle, State::Active, State::Recording] {
        let base = s.state(state);
        let hover = s.lift(base, 22);
        assert!(hover.r >= base.r && hover.g >= base.g && hover.b >= base.b);
        // Distinguishable from the *other* states' bases, not merged into them.
        for other in [State::Idle, State::Active, State::Recording] {
            if other != state {
                assert_ne!(hover, s.state(other), "hover collided with another state");
            }
        }
    }
}

#[test]
fn type_sizes_respect_the_legibility_floor() {
    // 14px is the floor for text meant to be read without effort on the target
    // display; nothing routinely-read may fall below it.
    let k = Skin::default().kind;
    for (name, size) in [
        ("label", k.label),
        ("readout", k.readout),
        ("body", k.body),
        ("title", k.title),
    ] {
        assert!(size >= 14.0, "{name} at {size}px is below the floor");
    }
    // Band labels are the sanctioned exception: uppercase, tracked, and scanned
    // rather than read.
    assert!(k.band < 14.0);
}
