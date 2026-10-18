//! Local wall time, as the toy sees it.
//!
//! A [`Minute`] is whole minutes since 2026-01-01 00:00 **local** time. The
//! board would get UTC from NTP and turn it into local time with the place's
//! timezone; the toy skips that step (no timezone, no daylight saving) because
//! none of the model's rules depend on it. The page passes the time in; the
//! model never reads a clock.

/// Minutes since 2026-01-01 00:00 local time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Minute(pub i64);

pub const MINUTES_PER_DAY: i64 = 1440;

impl Minute {
    /// `at(2026, 10, 31, 19, 0)` is 19:00 on Halloween 2026.
    pub fn at(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> Self {
        let days = days_from_civil(year, month, day) - days_from_civil(2026, 1, 1);
        Minute(days * MINUTES_PER_DAY + i64::from(hour) * 60 + i64::from(minute))
    }

    /// The day this minute falls on (day 0 is 2026-01-01).
    pub fn day(self) -> Day {
        Day(self.0.div_euclid(MINUTES_PER_DAY))
    }

    /// Minutes since this minute's local midnight.
    pub fn minute_of_day(self) -> i64 {
        self.0.rem_euclid(MINUTES_PER_DAY)
    }

    pub fn plus(self, minutes: i64) -> Self {
        Minute(self.0 + minutes)
    }

    /// "Sat Oct 31 19:00".
    pub fn label(self) -> String {
        let (_, month, day) = self.day().civil();
        let m = self.minute_of_day();
        format!(
            "{} {} {} {:02}:{:02}",
            self.day().weekday().short(),
            MONTHS[usize::from(month) - 1],
            day,
            m / 60,
            m % 60
        )
    }
}

/// A local calendar day (day 0 is Thursday 2026-01-01).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Day(pub i64);

impl Day {
    pub fn start(self) -> Minute {
        Minute(self.0 * MINUTES_PER_DAY)
    }

    pub fn weekday(self) -> Weekday {
        // 2026-01-01 was a Thursday; Monday is 0.
        Weekday::ALL[(self.0 + 3).rem_euclid(7) as usize]
    }

    pub fn civil(self) -> (i32, u8, u8) {
        civil_from_days(self.0 + days_from_civil(2026, 1, 1))
    }

    /// Day of the year, 1 for January 1st.
    pub fn day_of_year(self) -> i64 {
        let (year, _, _) = self.civil();
        self.0 + days_from_civil(2026, 1, 1) - days_from_civil(year, 1, 1) + 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    pub const ALL: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];

    pub fn short(self) -> &'static str {
        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][self as usize]
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Days since 1970-01-01 for a proleptic Gregorian date (the standard
/// era-based conversion, written from the calendar rules).
fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i32, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halloween_2026_is_a_saturday() {
        let m = Minute::at(2026, 10, 31, 19, 0);
        assert_eq!(m.day().weekday(), Weekday::Sat);
        assert_eq!(m.day().civil(), (2026, 10, 31));
        assert_eq!(m.label(), "Sat Oct 31 19:00");
    }

    #[test]
    fn day_of_year_resets_each_january() {
        assert_eq!(Minute::at(2026, 1, 1, 0, 0).day().day_of_year(), 1);
        assert_eq!(Minute::at(2027, 1, 2, 0, 0).day().day_of_year(), 2);
    }
}
