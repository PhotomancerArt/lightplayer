//! The show: everything a board knows about its presets, and every verb.
//!
//! Three kinds of state, by where a board would keep them:
//! - [`Show::project`]: what the author wrote (the project's files);
//! - [`Saved`]: what the user made to keep — panel defaults, presets,
//!   schedules (flash);
//! - [`Live`]: what is happening now — activations, overrides, deleted
//!   firings, the log (RTC RAM: it survives a soft reset or an update's
//!   reboot, and a power cut loses it).
//!
//! The page passes the time in through [`Show::tick`]; `None` means the
//! board does not know it yet. The model never reads a clock.

use std::collections::BTreeMap;

use crate::activation::{Activation, Source, SourceKey};
use crate::control::{ControlId, Value};
use crate::layer::Layer;
use crate::overrides::Overrides;
use crate::preset::{Preset, PresetId};
use crate::schedule::{Schedule, ScheduleId};
use crate::time::Minute;

#[derive(Clone, Debug)]
pub struct Show {
    /// The project's own values, one per control: the bottom layer.
    pub project: BTreeMap<ControlId, Value>,
    pub saved: Saved,
    pub live: Live,
    clock: Option<Minute>,
}

/// Kept in flash: the user's own making.
#[derive(Clone, Debug, Default)]
pub struct Saved {
    /// The baseline: a preset that never ends. A house sets power off here,
    /// and its schedules turn the lights on.
    pub panel_defaults: BTreeMap<ControlId, Value>,
    /// In priority order: later in the list is higher in the stack.
    pub presets: Vec<Preset>,
    pub schedules: Vec<Schedule>,
}

/// Kept in RTC RAM: survives a soft reset, not a power cut.
#[derive(Clone, Debug, Default)]
pub struct Live {
    pub activations: Vec<Activation>,
    pub overrides: Overrides,
    /// Schedule firings someone deleted, so catching up never brings them
    /// back.
    pub dismissed: Vec<(ScheduleId, Minute)>,
    pub log: Vec<LogLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub at: Option<Minute>,
    pub text: String,
}

/// One control's value and the layer it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub control: ControlId,
    pub value: Value,
    pub layer: Layer,
}

/// A preset on the "Active presets" row, with every activation holding it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivePreset {
    pub preset: PresetId,
    pub activations: Vec<Activation>,
}

impl Show {
    pub fn new(project: BTreeMap<ControlId, Value>, saved: Saved) -> Self {
        Show {
            project,
            saved,
            live: Live::default(),
            clock: None,
        }
    }

    pub fn clock(&self) -> Option<Minute> {
        self.clock
    }

    // ---- time -----------------------------------------------------------

    /// The board's time, every frame. Ends what is due, then lets every
    /// schedule fire — and the same code catches up a firing the board was
    /// off for, so there is no separate recovery path.
    pub fn tick(&mut self, clock: Option<Minute>) {
        self.clock = clock;
        let Some(now) = clock else {
            // Nothing starts or ends without the time. What was live before
            // a soft reset keeps showing until the time is back.
            return;
        };
        self.give_manual_activations_their_start(now);
        self.end_what_is_due(now);
        self.fire_schedules(now);
        if !self.live.overrides.is_empty() && self.live.overrides.ends.is_none() {
            self.live.overrides.ends = self.next_change_after(now);
        }
    }

    fn give_manual_activations_their_start(&mut self, now: Minute) {
        for a in &mut self.live.activations {
            if a.started.is_none() {
                a.started = Some(now);
            }
        }
    }

    fn end_what_is_due(&mut self, now: Minute) {
        let (ended, kept): (Vec<_>, Vec<_>) = self
            .live
            .activations
            .drain(..)
            .partition(|a| a.ends().is_some_and(|e| e <= now));
        self.live.activations = kept;
        for a in ended {
            let name = self.preset_name(a.preset);
            self.note(format!("{name} ended"));
        }
        if self.live.overrides.ends.is_some_and(|e| e <= now) {
            self.live.overrides = Overrides::default();
            self.note("Your changes ended".into());
        }
    }

    fn fire_schedules(&mut self, now: Minute) {
        let firings: Vec<_> = self
            .saved
            .schedules
            .iter()
            .filter_map(|s| {
                let fired = s.latest_firing(now)?;
                let ends = s.end_of(fired);
                let live = ends.is_none_or(|e| e > now);
                let deleted = self.live.dismissed.contains(&(s.id, fired));
                let made = self
                    .live
                    .activations
                    .iter()
                    .any(|a| a.source == Source::Schedule { id: s.id, fired });
                (live && !deleted && !made).then(|| (s.clone(), fired, ends))
            })
            .collect();
        for (schedule, fired, ends) in firings {
            let source = Source::Schedule {
                id: schedule.id,
                fired,
            };
            // One activation per (preset, source): a schedule's new firing
            // replaces its old one.
            self.live
                .activations
                .retain(|a| !(a.preset == schedule.preset && a.source.key() == source.key()));
            self.live.activations.push(Activation {
                preset: schedule.preset,
                source,
                started: Some(fired),
                lasts: ends.map(|e| (e.0 - fired.0) as u32),
            });
            let name = self.preset_name(schedule.preset);
            let until = ends.map_or("until turned off".into(), |e| {
                format!("until {}", e.label())
            });
            let late = if fired < now {
                format!(", caught up from {}", fired.label())
            } else {
                String::new()
            };
            self.note(format!("{name} on by \"{}\", {until}{late}", schedule.name));
        }
    }

    /// When the schedule next changes anything: a firing, or an activation
    /// ending. Overrides end here by default.
    pub fn next_change_after(&self, now: Minute) -> Option<Minute> {
        let firings = self
            .saved
            .schedules
            .iter()
            .filter_map(|s| s.next_firing_after(now));
        let endings = self
            .live
            .activations
            .iter()
            .filter_map(Activation::ends)
            .filter(|e| *e > now);
        firings.chain(endings).min()
    }

    // ---- the stack ------------------------------------------------------

    /// Every control's value and why, bottom layer to top, last wins.
    pub fn resolve(&self) -> Vec<Resolved> {
        let mut out: BTreeMap<ControlId, Resolved> = self
            .project
            .iter()
            .map(|(c, v)| {
                (
                    *c,
                    Resolved {
                        control: c,
                        value: v.clone(),
                        layer: Layer::Project,
                    },
                )
            })
            .collect();
        let mut lay = |pins: &BTreeMap<ControlId, Value>, layer: Layer| {
            for (control, value) in pins {
                if let Some(r) = out.get_mut(control) {
                    r.value = value.clone();
                    r.layer = layer;
                }
            }
        };
        lay(&self.saved.panel_defaults, Layer::PanelDefaults);
        for preset in &self.saved.presets {
            if self.live.activations.iter().any(|a| a.preset == preset.id) {
                lay(&preset.pins, Layer::Preset(preset.id));
            }
        }
        lay(&self.live.overrides.pins, Layer::Overrides);
        out.into_values().collect()
    }

    pub fn value(&self, control: ControlId) -> Resolved {
        self.resolve()
            .into_iter()
            .find(|r| r.control == control)
            .unwrap_or_else(|| panic!("no control {control}"))
    }

    /// The "Active presets" row, top of the stack first.
    pub fn active_presets(&self) -> Vec<ActivePreset> {
        self.saved
            .presets
            .iter()
            .rev()
            .filter_map(|p| {
                let activations: Vec<_> = self
                    .live
                    .activations
                    .iter()
                    .filter(|a| a.preset == p.id)
                    .cloned()
                    .collect();
                (!activations.is_empty()).then_some(ActivePreset {
                    preset: p.id,
                    activations,
                })
            })
            .collect()
    }

    // ---- verbs: presets ---------------------------------------------------

    /// Press a preset on the panel: on now, for `minutes` or until turned off.
    /// Pressing it again replaces the end.
    pub fn activate(&mut self, preset: PresetId, minutes: Option<u32>) {
        self.live
            .activations
            .retain(|a| !(a.preset == preset && a.source == Source::Manual));
        self.live.activations.push(Activation {
            preset,
            source: Source::Manual,
            started: self.clock,
            lasts: minutes,
        });
        let name = self.preset_name(preset);
        let until = match (minutes, self.clock) {
            (Some(m), Some(now)) => format!("until {}", now.plus(i64::from(m)).label()),
            (Some(m), None) => format!("for {m} min"),
            (None, _) => "until turned off".into(),
        };
        self.note(format!("You turned {name} on, {until}"));
    }

    /// The chip's ×: every activation holding the preset ends now.
    pub fn turn_off(&mut self, preset: PresetId) {
        let keys: Vec<_> = self
            .live
            .activations
            .iter()
            .filter(|a| a.preset == preset)
            .map(|a| a.source.key())
            .collect();
        for key in keys {
            self.end_one(preset, key);
        }
        let name = self.preset_name(preset);
        self.note(format!("You turned {name} off"));
    }

    /// End one source's activation and leave the others.
    pub fn end_one(&mut self, preset: PresetId, key: SourceKey) {
        let mut ended = Vec::new();
        self.live.activations.retain(|a| {
            let hit = a.preset == preset && a.source.key() == key;
            if hit {
                ended.push(a.source);
            }
            !hit
        });
        for source in ended {
            if let Source::Schedule { id, fired } = source {
                self.live.dismissed.push((id, fired));
            }
        }
    }

    /// Change what a preset pins (`None` unpins). An active preset shows the
    /// change at once: activations point at presets, they never copy them.
    pub fn pin(&mut self, preset: PresetId, control: ControlId, value: Option<Value>) {
        let p = self.preset_mut(preset);
        match value {
            Some(v) => {
                p.pins.insert(control, v);
            }
            None => {
                p.pins.remove(control);
            }
        }
    }

    /// Move a preset to `index` in the priority order (0 is the bottom).
    pub fn move_preset(&mut self, preset: PresetId, index: usize) {
        let at = self
            .saved
            .presets
            .iter()
            .position(|p| p.id == preset)
            .expect("preset");
        let p = self.saved.presets.remove(at);
        self.saved
            .presets
            .insert(index.min(self.saved.presets.len()), p);
    }

    // ---- verbs: controls --------------------------------------------------

    /// Turn a control: the change lands in the overrides layer, on top.
    pub fn turn(&mut self, control: ControlId, value: Value) {
        self.live.overrides.pins.insert(control, value);
        if self.live.overrides.ends.is_none() {
            self.live.overrides.ends = self.clock.and_then(|now| self.next_change_after(now));
        }
    }

    /// Drop every temporary change now.
    pub fn drop_overrides(&mut self) {
        self.live.overrides = Overrides::default();
    }

    /// Set the baseline (the panel defaults layer).
    pub fn set_panel_default(&mut self, control: ControlId, value: Value) {
        self.saved.panel_defaults.insert(control, value);
    }

    /// Save your changes as a new preset. It goes to the top of the order and
    /// stays on until your changes would have ended, so nothing you see
    /// changes.
    pub fn save_overrides_as_preset(&mut self, name: &str, colour: &'static str) -> PresetId {
        let id = PresetId(self.saved.presets.iter().map(|p| p.id.0).max().unwrap_or(0) + 1);
        let pins = core::mem::take(&mut self.live.overrides.pins);
        let ends = self.live.overrides.ends.take();
        self.saved.presets.push(Preset {
            id,
            name: name.into(),
            colour,
            pins,
        });
        let minutes = match (ends, self.clock) {
            (Some(e), Some(now)) => Some((e.0 - now.0).max(0) as u32),
            _ => None,
        };
        self.live.activations.push(Activation {
            preset: id,
            source: Source::Manual,
            started: self.clock,
            lasts: minutes,
        });
        self.note(format!("You saved your changes as {name}"));
        id
    }

    // ---- verbs: schedules -------------------------------------------------

    pub fn add_schedule(&mut self, mut schedule: Schedule) {
        schedule.since = self.clock.unwrap_or(schedule.since);
        self.saved.schedules.push(schedule);
    }

    /// Edit a schedule. What it already turned on stays as it is; the edit
    /// shapes its next firing.
    pub fn edit_schedule(&mut self, id: ScheduleId, edit: impl FnOnce(&mut Schedule)) {
        let clock = self.clock;
        let s = self
            .saved
            .schedules
            .iter_mut()
            .find(|s| s.id == id)
            .expect("schedule");
        edit(s);
        if let Some(now) = clock {
            s.since = now;
        }
    }

    /// What the lights would show at `time` from the saved show alone — the
    /// "pretend it's…" test and the calendar. It never moves the board's own
    /// clock: the live show only goes forward, and a clock moved back would
    /// keep activations from its "future".
    pub fn pretend(&self, time: Minute) -> Show {
        let mut copy = Show::new(self.project.clone(), self.saved.clone());
        copy.tick(Some(time));
        copy
    }

    // ---- the board --------------------------------------------------------

    /// A soft reset or an update's reboot: RTC RAM keeps everything live; the
    /// board has to learn the time again.
    pub fn soft_reset(&mut self) {
        self.clock = None;
    }

    /// A power cut: everything live is gone; what was saved is not.
    pub fn power_cut(&mut self) {
        self.live = Live::default();
        self.clock = None;
    }

    // ---- helpers ------------------------------------------------------------

    pub fn preset(&self, id: PresetId) -> &Preset {
        self.saved
            .presets
            .iter()
            .find(|p| p.id == id)
            .expect("preset")
    }

    pub fn preset_named(&self, name: &str) -> PresetId {
        self.saved
            .presets
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.id)
            .expect("preset")
    }

    fn preset_mut(&mut self, id: PresetId) -> &mut Preset {
        self.saved
            .presets
            .iter_mut()
            .find(|p| p.id == id)
            .expect("preset")
    }

    fn preset_name(&self, id: PresetId) -> String {
        self.saved
            .presets
            .iter()
            .find(|p| p.id == id)
            .map_or("A deleted preset".into(), |p| p.name.clone())
    }

    fn note(&mut self, text: String) {
        self.live.log.push(LogLine {
            at: self.clock,
            text,
        });
    }
}
