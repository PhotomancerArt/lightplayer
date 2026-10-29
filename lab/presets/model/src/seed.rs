//! A house's eave lights: the show the statements and the page start from.
//!
//! Panel defaults keep the lights off; the schedules turn them on.
//!
//! Presets, bottom of the order first:
//! - **Evening** — power on, cycle Embers / Aurora / Twinkle;
//! - **Late dim** — brightness 30 % (and nothing else);
//! - **Christmas** — power on, cycle Twinkle / Candy cane, palette Red & green;
//! - **Halloween** — power on, cycle Spooky / Pumpkin, palette Orange;
//! - **Party** — power on, brightness 100 %, speed 80 %.
//!
//! Schedules:
//! - **Evenings**: every day, sunset − 15 min until 00:30 → Evening;
//! - **Late nights**: every day, 23:00 for 90 min → Late dim;
//! - **Halloween nights**: Oct 1–31, 18:30 until 23:00 → Halloween;
//! - **Christmas season**: Dec 1 – Jan 6, sunset until 00:00 → Christmas.

use std::collections::BTreeMap;

use crate::control::{ControlId, Value};
use crate::preset::{Preset, PresetId};
use crate::schedule::{DateRange, Schedule, ScheduleEnd, ScheduleId, TimeOfDay, Weekdays};
use crate::show::{Saved, Show};
use crate::time::Minute;

pub const PATTERNS: [&str; 6] = [
    "Embers",
    "Aurora",
    "Twinkle",
    "Spooky",
    "Pumpkin",
    "Candy cane",
];

impl Show {
    pub fn eaves() -> Show {
        let project: BTreeMap<ControlId, Value> = [
            ("power", Value::On(true)),
            ("brightness", Value::Level(100)),
            ("pattern", Value::Cycle(PATTERNS.to_vec())),
            ("palette", Value::Choice("Warm")),
            ("speed", Value::Level(50)),
        ]
        .into();
        let panel_defaults: BTreeMap<ControlId, Value> = [
            ("power", Value::On(false)),
            ("brightness", Value::Level(80)),
        ]
        .into();
        let preset = |id, name: &str, colour, pins: &[(ControlId, Value)]| Preset {
            id: PresetId(id),
            name: name.into(),
            colour,
            pins: pins.iter().cloned().collect(),
        };
        let presets = vec![
            preset(
                1,
                "Evening",
                "#53c6f2",
                &[
                    ("power", Value::On(true)),
                    ("pattern", Value::Cycle(vec!["Embers", "Aurora", "Twinkle"])),
                ],
            ),
            preset(
                2,
                "Late dim",
                "#a58af7",
                &[("brightness", Value::Level(30))],
            ),
            preset(
                3,
                "Christmas",
                "#5fe08b",
                &[
                    ("power", Value::On(true)),
                    ("pattern", Value::Cycle(vec!["Twinkle", "Candy cane"])),
                    ("palette", Value::Choice("Red & green")),
                ],
            ),
            preset(
                4,
                "Halloween",
                "#ffab4a",
                &[
                    ("power", Value::On(true)),
                    ("pattern", Value::Cycle(vec!["Spooky", "Pumpkin"])),
                    ("palette", Value::Choice("Orange")),
                ],
            ),
            preset(
                5,
                "Party",
                "#ff5d5d",
                &[
                    ("power", Value::On(true)),
                    ("brightness", Value::Level(100)),
                    ("speed", Value::Level(80)),
                ],
            ),
        ];
        let always = Minute::at(2026, 1, 1, 0, 0);
        let schedule = |id, name: &str, preset, dates, start, end| Schedule {
            id: ScheduleId(id),
            name: name.into(),
            preset: PresetId(preset),
            enabled: true,
            days: Weekdays::EVERY_DAY,
            dates,
            start,
            end,
            since: always,
        };
        let clock = |hour, minute| TimeOfDay::Clock { hour, minute };
        let schedules = vec![
            schedule(
                1,
                "Evenings",
                1,
                None,
                TimeOfDay::Sunset(-15),
                ScheduleEnd::At(clock(0, 30)),
            ),
            schedule(
                2,
                "Late nights",
                2,
                None,
                clock(23, 0),
                ScheduleEnd::For { minutes: 90 },
            ),
            schedule(
                3,
                "Halloween nights",
                4,
                Some(DateRange {
                    from: (10, 1),
                    to: (10, 31),
                }),
                clock(18, 30),
                ScheduleEnd::At(clock(23, 0)),
            ),
            schedule(
                4,
                "Christmas season",
                3,
                Some(DateRange {
                    from: (12, 1),
                    to: (1, 6),
                }),
                TimeOfDay::Sunset(0),
                ScheduleEnd::At(clock(0, 0)),
            ),
        ];
        Show::new(
            project,
            Saved {
                panel_defaults,
                presets,
                schedules,
            },
        )
    }
}
