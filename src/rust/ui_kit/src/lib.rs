//! rev-ui-kit — the control-skin widget kit (R-712): the owned layer where
//! widget style and identity live (lights glow, text never; mono-color
//! readouts; vertical sliders; wheel-on-hover). Built against the mechanism
//! contract only. First customer: the Control Bar census — pop-ups, tri-state
//! button, toggles, multi-field numeric displays, shuttle, locator bank.

pub mod kit;
pub mod skin;

pub use kit::{Anchor, Field, Intent, Kind, Kit, RecordMode, Widget, WidgetId};
pub use skin::{Kind as TypeScale, Metric, Role, Skin, State};
