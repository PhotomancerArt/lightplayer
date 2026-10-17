//! A calendar date, in UTC: "Oct 7" — the month's short name and the day.
//! Where the year is the fact (when an account was made), the same date
//! carries it: "Oct 7, 2026".
//!
//! The one month table and the one civil-date conversion (P17): a board's
//! release, a key's "added Sep 24", an account's "since Aug 6, 2026", a
//! file's date. A tiny conversion beats a date crate for one label; it is
//! the standard days-from-epoch one (Howard Hinnant's `civil_from_days`).

/// The months' short names, January first.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A day of the proleptic Gregorian calendar.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CalendarDate {
    pub year: i64,
    /// 1–12.
    pub month: u32,
    /// 1–31.
    pub day: u32,
}

impl CalendarDate {
    /// The UTC day `epoch_secs` falls on.
    pub fn from_epoch_secs(epoch_secs: f64) -> Self {
        let days = (epoch_secs.floor() as i64).div_euclid(86_400);
        Self::from_days(days)
    }

    /// The day `days` after 1970-01-01.
    pub fn from_days(days: i64) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        Self { year, month, day }
    }

    /// `2026-10-07` (or `2026.10.07`); `None` for anything else.
    pub fn parse_iso(date: &str) -> Option<Self> {
        let mut parts = date.split(['-', '.']);
        let year = parts.next()?.parse().ok()?;
        let month = parts
            .next()?
            .parse()
            .ok()
            .filter(|m| (1..=12).contains(m))?;
        let day = parts
            .next()?
            .parse()
            .ok()
            .filter(|d| (1..=31).contains(d))?;
        parts.next().is_none().then_some(Self { year, month, day })
    }

    /// The month's short name: "Oct".
    pub fn month_name(&self) -> &'static str {
        MONTHS[(self.month.clamp(1, 12) - 1) as usize]
    }

    /// "Oct 7".
    pub fn words(&self) -> String {
        format!("{} {}", self.month_name(), self.day)
    }

    /// "Oct 7, 2026": the same date, where the year is the fact.
    pub fn words_with_year(&self) -> String {
        format!("{}, {}", self.words(), self.year)
    }
}

/// The UTC day `epoch_secs` falls on, as "Oct 7".
pub fn date_words(epoch_secs: f64) -> String {
    CalendarDate::from_epoch_secs(epoch_secs).words()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_reads_as_its_month_and_day() {
        assert_eq!(date_words(0.0), "Jan 1");
        assert_eq!(date_words(1_788_091_200.0), "Aug 30");
        assert_eq!(date_words(1_758_672_000.0), "Sep 24");
        assert_eq!(
            CalendarDate::from_epoch_secs(1_785_974_400.0).words_with_year(),
            "Aug 6, 2026"
        );
    }

    #[test]
    fn the_civil_date_conversion_is_correct_at_the_edges() {
        let date = |days| {
            let date = CalendarDate::from_days(days);
            (date.year, date.month, date.day)
        };
        assert_eq!(date(0), (1970, 1, 1));
        // 2000-02-29 (leap): 11 016 days after the epoch.
        assert_eq!(date(11_016), (2000, 2, 29));
        assert_eq!(date(20_818), (2026, 12, 31));
        // Before the epoch.
        assert_eq!(date(-1), (1969, 12, 31));
        assert_eq!(
            CalendarDate::from_epoch_secs(1_709_164_800.0).words_with_year(),
            "Feb 29, 2024"
        );
    }

    #[test]
    fn an_iso_date_parses_and_nothing_else_does() {
        assert_eq!(
            CalendarDate::parse_iso("2026-10-07").map(|date| date.words()),
            Some("Oct 7".to_string())
        );
        assert_eq!(
            CalendarDate::parse_iso("2026.10.16").map(|date| date.words()),
            Some("Oct 16".to_string())
        );
        for odd in ["", "2026-13-01", "2026-10", "2026-10-07-1", "Oct 7"] {
            assert_eq!(CalendarDate::parse_iso(odd), None, "{odd}");
        }
    }
}
