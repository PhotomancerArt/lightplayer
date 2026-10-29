//! An activation holds a preset on, from a source, for a while.
//!
//! The panel's "Active presets" row is the list of activations. There is at
//! most one activation per (preset, source): two schedules turning on the
//! same preset make two activations, and ending one leaves the other.

use crate::preset::PresetId;
use crate::schedule::ScheduleId;
use crate::time::Minute;

/// What made an activation. Two sources never share an activation, the way
/// two identity providers vouching for one email are still two records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// Someone pressed the preset on the panel.
    Manual,
    /// A schedule's firing. `fired` is the occurrence's start, which is what
    /// keeps a deleted firing from coming back.
    Schedule { id: ScheduleId, fired: Minute },
}

impl Source {
    /// The key an activation is unique by: a schedule is one source however
    /// many times it fires.
    pub fn key(self) -> SourceKey {
        match self {
            Source::Manual => SourceKey::Manual,
            Source::Schedule { id, .. } => SourceKey::Schedule(id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceKey {
    Manual,
    Schedule(ScheduleId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activation {
    pub preset: PresetId,
    pub source: Source,
    /// When it started. `None` for one pressed before the board knew the
    /// time; it starts counting when the time arrives.
    pub started: Option<Minute>,
    /// How long it lasts, in minutes. `None` holds until someone turns it
    /// off. An activation only ever carries a duration: the calendar logic
    /// that chose it stays in the schedule.
    pub lasts: Option<u32>,
}

impl Activation {
    /// When it ends by itself, if it does and has started.
    pub fn ends(&self) -> Option<Minute> {
        Some(self.started?.plus(i64::from(self.lasts?)))
    }
}
