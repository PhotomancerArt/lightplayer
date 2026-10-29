//! The presets lab's model: presets, activations and schedules, as a toy.
//!
//! The stack, bottom to top, last wins:
//!
//! 1. the project's own values ([`Layer::Project`]);
//! 2. the panel defaults, a preset that never ends ([`Layer::PanelDefaults`]);
//! 3. the active presets, in the presets' priority order;
//! 4. the overrides: what turning a control changes, temporary by design.
//!
//! A [`Preset`] pins some controls. An [`Activation`] holds a preset on from
//! one [`Source`] for a duration. A [`Schedule`] is a trigger: each firing
//! makes an activation. See `tests/statements.rs` for the behaviour as
//! sentences, and `lab/presets/README.md`.

pub mod activation;
pub mod control;
pub mod layer;
pub mod overrides;
pub mod preset;
pub mod schedule;
pub mod seed;
pub mod show;
pub mod sun;
pub mod time;

pub use activation::{Activation, Source, SourceKey};
pub use control::{ControlId, Value};
pub use layer::Layer;
pub use overrides::Overrides;
pub use preset::{Preset, PresetId};
pub use schedule::{DateRange, Schedule, ScheduleEnd, ScheduleId, TimeOfDay, Weekdays};
pub use show::{ActivePreset, LogLine, Resolved, Saved, Show};
pub use time::{Day, Minute, Weekday};
