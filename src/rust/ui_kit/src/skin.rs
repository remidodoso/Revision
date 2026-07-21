//! The skin: every colour, metric and radius, in one value.
//!
//! Transcribed from the Notorolla exhibits (`doc/revision_skin_inventory.md`), which
//! are the retained visual spec. Two properties are carried across deliberately:
//!
//! - **Derived, not tabulated.** The exhibits drive an entire panel from one scalar
//!   — `--pl`, the panel lightness — with everything else a `calc()` away. A flat
//!   table of literals would lose that: retuning would mean editing forty values and
//!   getting one wrong. [`Skin::new`] takes the lightness and derives the rest.
//! - **Two semantic colour systems on one set of primitives.** [`Role`] for
//!   parameters (an absent role leaves a deliberate gap in the spectrum) and
//!   [`State`] for transport. A transport has no LFO; inventing roles for it would
//!   be copying the mechanism instead of the idea.

use rev_ui_mech::{Color, TextStyle};

/// A parameter group's role. **Hue is the role, never the instrument**: a role keeps
/// its colour on every panel, and a panel missing one leaves a gap in the spectrum —
/// which reads, correctly, as "this instrument has no LFO".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Lfo,
    Oscillator,
    Filter,
    Envelope,
    Effect,
}

impl Role {
    /// Hue and saturation, locked by the exhibits. Lightness is fixed at 54% so
    /// black label text stays legible on every band.
    fn band(self) -> (f32, f32) {
        match self {
            Role::Lfo => (15.0, 0.45),
            Role::Oscillator => (30.0, 0.48),
            Role::Filter => (120.0, 0.30),
            Role::Envelope => (190.0, 0.40),
            Role::Effect => (215.0, 0.35),
        }
    }

    /// Canonical panel order, so instruments read against one another.
    pub const ORDER: [Role; 5] = [
        Role::Lfo,
        Role::Oscillator,
        Role::Filter,
        Role::Envelope,
        Role::Effect,
    ];
}

/// A transport control's condition. The Control Bar's semantics, distinct from
/// [`Role`] and sharing only the primitives beneath.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    /// Nothing special — ordinary chrome.
    Idle,
    /// Latched on: playing, looping, punched in.
    Active,
    /// Record-armed, awaiting the transport. Flashes; the skin supplies the colour,
    /// not the flashing.
    Armed,
    /// Recording now.
    Recording,
    /// Present, visible, and not operable at the current settings. The exhibits'
    /// inert rule: dimmed, still there, still announced.
    Inert,
}

/// Every value the widget kit is allowed to know about appearance.
#[derive(Debug, Clone)]
pub struct Skin {
    /// The panel face — everything below is derived from its lightness.
    pub panel: Color,
    /// Raised edge: the 1px top highlight that reads as a bevel. A hairline, not a
    /// blur — a good half of the exhibits' "inset shadows" are exactly this.
    pub panel_hi: Color,
    pub panel_lo: Color,
    /// Recessed groove behind a slider or a meter.
    pub slot: Color,
    pub tick: Color,
    pub tick_major: Color,
    /// Group frame line.
    pub frame: Color,
    pub ink: Color,
    pub ink_dim: Color,
    /// Readout amber — every value, everywhere.
    pub readout: Color,
    /// Accent red: lit lamps, the record light.
    pub accent: Color,
    /// Value-arc blue: active, armed, in progress.
    pub arc: Color,
    /// Slider-cap gradient, three stops top to bottom.
    pub cap: [Color; 3],
    pub metric: Metric,
    pub kind: Kind,
}

/// Sizes and spacings, in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Metric {
    pub panel_radius: f32,
    pub control_radius: f32,
    /// Gap between adjacent tick ladders.
    pub tick_gap: f32,
    pub slider_width: f32,
    pub slider_height: f32,
    pub slot_width: f32,
    /// Wide, near column width — the Jupiter-8 cap the exhibits locked.
    pub cap_width: f32,
    pub cap_height: f32,
    pub knob_diameter: f32,
    pub lamp_diameter: f32,
    /// Fixed readout window, in characters. Monospace makes this exact, which is
    /// why the numeric font role exists.
    pub readout_char: f32,
    /// Minimum hit size, independent of visual scale — the touch floor.
    pub touch_min: f32,
    /// How wide a scroll bar is, and the shortest its thumb may be.
    ///
    /// **Here rather than in `pane`** because it is a matter of feel, and this
    /// is where feel lives. Wider than current convention on purpose: bars
    /// shrank because phone conventions leaked to the desktop, and the target
    /// display is a 50-inch panel at desk distance — the same reasoning that
    /// made the type scale larger than control-skin convention.
    pub scroll_bar: f32,
    pub scroll_thumb_min: f32,
}

/// The type scale.
///
/// **Larger than control-skin convention, deliberately.** The exhibits are anchored
/// to a 14px browser panel; the target display here is a 50-inch 4K panel at desk
/// distance, where 14px is the *floor* for text meant to be read without effort and
/// 18px is comfortable. Sizes below the floor are for dense, deliberately-scanned
/// material only.
#[derive(Debug, Clone, Copy)]
pub struct Kind {
    /// Group band labels — small, uppercase, tracked.
    pub band: f32,
    /// Control names.
    pub label: f32,
    /// Values in readouts.
    pub readout: f32,
    /// Ordinary interface text.
    pub body: f32,
    /// Titles.
    pub title: f32,
}

impl Default for Skin {
    fn default() -> Skin {
        Skin::new(0.20)
    }
}

impl Skin {
    /// Derive a whole skin from the panel lightness, 0..1. The exhibits' default is
    /// 0.20; their meta control ranges 0.14 to 0.34, and so does this.
    pub fn new(lightness: f32) -> Skin {
        let l = lightness.clamp(0.14, 0.34);
        Skin {
            panel: hsl(220.0, 0.07, l),
            panel_hi: hsl(220.0, 0.07, l + 0.06),
            panel_lo: hsl(220.0, 0.08, l - 0.06),
            slot: hsl(222.0, 0.10, l - 0.10),
            tick: hsl(220.0, 0.08, l + 0.26),
            tick_major: hsl(220.0, 0.08, l + 0.38),
            frame: hsl(220.0, 0.08, l + 0.16),
            ink: hsl(220.0, 0.12, 0.88),
            ink_dim: hsl(220.0, 0.08, 0.62),
            readout: hsl(41.0, 1.00, 0.62),
            accent: hsl(358.0, 0.78, 0.56),
            arc: hsl(200.0, 0.70, 0.55),
            cap: [
                hsl(220.0, 0.06, 0.42),
                hsl(220.0, 0.07, 0.20),
                hsl(220.0, 0.08, 0.30),
            ],
            metric: Metric {
                panel_radius: 8.0,
                control_radius: 3.0,
                tick_gap: 2.0,
                slider_width: 26.0,
                slider_height: 104.0,
                slot_width: 5.0,
                cap_width: 24.0,
                cap_height: 14.0,
                knob_diameter: 44.0,
                lamp_diameter: 9.0,
                readout_char: 5.5,
                touch_min: 32.0,
                scroll_bar: crate::pane::BAR,
                scroll_thumb_min: crate::pane::MIN_THUMB,
            },
            kind: Kind {
                band: 12.0,
                label: 15.0,
                readout: 16.0,
                body: 16.0,
                title: 20.0,
            },
        }
    }

    /// A role's band colour. Black text sits on it, so lightness is fixed.
    pub fn band(&self, role: Role) -> Color {
        let (h, s) = role.band();
        hsl(h, s, 0.54)
    }

    /// Text colour for a band — black on every role, by design.
    pub fn band_ink(&self) -> Color {
        hsl(220.0, 0.30, 0.10)
    }

    /// A control's base colour for a transport state.
    ///
    /// **Base only.** Hover and press *modulate* this; they never replace it. A
    /// control painted with a flat "hover colour" reports the wrong state for
    /// exactly as long as the pointer rests on it — which is the moment it matters.
    pub fn state(&self, state: State) -> Color {
        match state {
            State::Idle => self.panel_hi,
            State::Active => self.arc,
            State::Armed => self.accent,
            State::Recording => self.accent,
            State::Inert => self.dim(self.panel_hi),
        }
    }

    /// Lift a colour for hover or press — the modulation the rule above requires.
    pub fn lift(&self, base: Color, amount: u8) -> Color {
        Color::rgba(
            base.r.saturating_add(amount),
            base.g.saturating_add(amount),
            base.b.saturating_add(amount),
            base.a,
        )
    }

    /// The exhibits' inert treatment: 35% toward the panel, still visible, still
    /// present to an assistive technology.
    pub fn dim(&self, base: Color) -> Color {
        mix(base, self.panel, 0.65)
    }

    pub fn label_style(&self) -> TextStyle {
        TextStyle::ui(self.kind.label)
    }

    pub fn readout_style(&self) -> TextStyle {
        TextStyle::numeric(self.kind.readout)
    }

    pub fn band_style(&self) -> TextStyle {
        TextStyle::ui(self.kind.band)
    }
}

/// HSL to RGB. The exhibits are written in HSL throughout, and transcribing them
/// into hex by hand would lose the derivation that makes one scalar retune a panel.
fn hsl(h: f32, s: f32, l: f32) -> Color {
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to8 = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::rgb(to8(r), to8(g), to8(b))
}

/// Blend `a` toward `b` by `t`.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color::rgba(f(a.r, b.r), f(a.g, b.g), f(a.b, b.b), a.a)
}

#[cfg(test)]
mod test;
