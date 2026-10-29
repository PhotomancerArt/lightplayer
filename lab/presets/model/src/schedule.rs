//! A schedule: when a preset turns on by itself, and for how long.
//!
//! A schedule is a trigger. Each firing makes one activation, whose end is
//! worked out at the firing and stored as a plain deadline. Editing a
//! schedule never touches an activation it already made.

use crate::preset::PresetId;
use crate::sun;
use crate::time::{Day, Minute, Weekday};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScheduleId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub id: ScheduleId,
    pub name: String,
    pub preset: PresetId,
    pub enabled: bool,
    pub days: Weekdays,
    /// Only between these dates, inclusive, every year. `None` is all year.
    pub dates: Option<DateRange>,
    pub start: TimeOfDay,
    pub end: ScheduleEnd,
    /// When this schedule took its current form (made, edited, enabled).
    /// Firings before it are not caught up: an edit changes what happens
    /// next, never what is showing now.
    pub since: Minute,
}

/// A time of day, fixed or from the sun.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeOfDay {
    Clock {
        hour: u8,
        minute: u8,
    },
    /// Minutes after sunset (negative is before).
    Sunset(i16),
    Sunrise(i16),
}

impl TimeOfDay {
    pub fn minute_of_day(self, day: Day) -> i64 {
        match self {
            TimeOfDay::Clock { hour, minute } => i64::from(hour) * 60 + i64::from(minute),
            TimeOfDay::Sunset(offset) => sun::sunset(day) + i64::from(offset),
            TimeOfDay::Sunrise(offset) => sun::sunrise(day) + i64::from(offset),
        }
    }

    pub fn on(self, day: Day) -> Minute {
        day.start().plus(self.minute_of_day(day))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleEnd {
    /// The next time this time of day comes round after the start (so
    /// "sunset to 01:00" ends after midnight).
    At(TimeOfDay),
    For {
        minutes: u32,
    },
    /// Holds until someone deletes the activation.
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weekdays(pub u8);

impl Weekdays {
    pub const EVERY_DAY: Weekdays = Weekdays(0b111_1111);

    pub fn only(days: &[Weekday]) -> Self {
        Weekdays(days.iter().fold(0, |mask, d| mask | 1 << (*d as u8)))
    }

    pub fn contains(self, day: Weekday) -> bool {
        self.0 & (1 << (day as u8)) != 0
    }
}

/// Month and day, inclusive at both ends. `from` after `to` wraps the new
/// year (Dec 1 – Jan 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateRange {
    pub from: (u8, u8),
    pub to: (u8, u8),
}

impl DateRange {
    pub fn contains(self, day: Day) -> bool {
        let (_, month, d) = day.civil();
        let at = (month, d);
        if self.from <= self.to {
            self.from <= at && at <= self.to
        } else {
            at >= self.from || at <= self.to
        }
    }
}

/// How far back a schedule looks for its latest firing. A year covers a
/// yearly date range; nothing in the toy needs more.
const LOOKBACK_DAYS: i64 = 370;

impl Schedule {
    /// The firing on `day`, if the schedule fires that day.
    pub fn firing_on(&self, day: Day) -> Option<Minute> {
        let fires = self.days.contains(day.weekday()) && self.dates.is_none_or(|r| r.contains(day));
        fires.then(|| self.start.on(day))
    }

    /// When the activation a firing at `fired` makes should end.
    pub fn end_of(&self, fired: Minute) -> Option<Minute> {
        match self.end {
            ScheduleEnd::For { minutes } => Some(fired.plus(i64::from(minutes))),
            ScheduleEnd::Never => None,
            ScheduleEnd::At(time) => {
                let mut day = fired.day();
                loop {
                    let candidate = time.on(day);
                    if candidate > fired {
                        return Some(candidate);
                    }
                    day = Day(day.0 + 1);
                }
            }
        }
    }

    /// The latest firing at or before `now` that this schedule, in its
    /// current form, made (or would have made had the board been on).
    pub fn latest_firing(&self, now: Minute) -> Option<Minute> {
        if !self.enabled {
            return None;
        }
        let today = now.day();
        (0..LOOKBACK_DAYS)
            .map(|back| Day(today.0 - back))
            .filter_map(|day| self.firing_on(day))
            .find(|fired| *fired <= now)
            .filter(|fired| *fired >= self.since)
    }

    /// Every firing that starts in `[from, to)`, with its end — what the
    /// calendar draws.
    pub fn firings_between(&self, from: Minute, to: Minute) -> Vec<(Minute, Option<Minute>)> {
        if !self.enabled {
            return Vec::new();
        }
        (from.day().0..=to.day().0)
            .filter_map(|d| self.firing_on(Day(d)))
            .filter(|fired| *fired >= from && *fired < to)
            .map(|fired| (fired, self.end_of(fired)))
            .collect()
    }

    /// The first moment after `now` that this schedule starts something.
    pub fn next_firing_after(&self, now: Minute) -> Option<Minute> {
        if !self.enabled {
            return None;
        }
        (0..LOOKBACK_DAYS)
            .map(|ahead| Day(now.day().0 + ahead))
            .filter_map(|day| self.firing_on(day))
            .find(|fired| *fired > now)
    }
}
