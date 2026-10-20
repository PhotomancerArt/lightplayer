//! An age: how long ago something happened, in units that read naturally
//! (the unit-awareness principle, P15): seconds while seconds are
//! meaningful, then minutes, hours, days, and from two weeks on, weeks.
//!
//! One rule for every surface that names an age (P17): a reading is
//! [`age_words`] ("12 s ago"); a details fact is a label and that value
//! (`Last heard` · `12 s ago`, `Last frame` · `12 s ago`); a "for how
//! long" is [`duration_words`] ("2 h"). An age said in a sentence, and
//! anywhere on a card beside one, is [`age_spoken`] ("It was last seen 2
//! hours ago.", "Last heard 2 hours ago"): the same units, spelled out, so
//! the board card's surface and its rows' details say one age one way. A stale
//! picture counting "412 s" is arithmetic homework; a remembered board's
//! picture can be a month old.

/// Seconds below which an age is said in seconds.
const MINUTES_FROM: f64 = 90.0;
/// … in minutes.
const HOURS_FROM: f64 = 5_400.0;
/// … in hours.
const DAYS_FROM: f64 = 172_800.0;
/// … in days; from here on, in weeks.
const WEEKS_FROM: f64 = 14.0 * 86_400.0;

/// An age as a reading says it: "12 s ago", "2 min ago", "5 h ago",
/// "7 d ago", "3 weeks ago". A negative age (a clock that stepped back)
/// reads "0 s ago".
pub fn age_words(age_secs: f64) -> String {
    format!("{} ago", duration_words(age_secs))
}

/// The same age as a duration, for "for how long": "2 weeks", "5 h".
pub fn duration_words(age_secs: f64) -> String {
    let (count, unit) = age_units(age_secs);
    match unit {
        AgeUnit::Seconds => format!("{count} s"),
        AgeUnit::Minutes => format!("{count} min"),
        AgeUnit::Hours => format!("{count} h"),
        AgeUnit::Days => format!("{count} d"),
        AgeUnit::Weeks => format!("{count} weeks"),
    }
}

/// An age as a sentence says it, its unit spelled out: "just now", "1
/// second ago", "2 minutes ago", "2 hours ago", "3 days ago", "2 weeks
/// ago". The board card's surface ("It was last seen 2 hours ago.") and
/// the row details beside it ("Last heard 2 hours ago") say an age this way.
pub fn age_spoken(age_secs: f64) -> String {
    let (count, unit) = age_units(age_secs);
    if count == 0 {
        return "just now".to_string();
    }
    let unit = match unit {
        AgeUnit::Seconds => "second",
        AgeUnit::Minutes => "minute",
        AgeUnit::Hours => "hour",
        AgeUnit::Days => "day",
        AgeUnit::Weeks => "week",
    };
    match count {
        1 => format!("1 {unit} ago"),
        count => format!("{count} {unit}s ago"),
    }
}

/// The unit an age reads in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgeUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Weeks,
}

/// `age_secs` in the unit it reads in (P15), and how many of it, rounded;
/// a negative age (a clock that stepped back) is no age at all.
fn age_units(age_secs: f64) -> (i64, AgeUnit) {
    let age = age_secs.max(0.0);
    if age < MINUTES_FROM {
        (age.round() as i64, AgeUnit::Seconds)
    } else if age < HOURS_FROM {
        ((age / 60.0).round() as i64, AgeUnit::Minutes)
    } else if age < DAYS_FROM {
        ((age / 3_600.0).round() as i64, AgeUnit::Hours)
    } else if age < WEEKS_FROM {
        ((age / 86_400.0).round() as i64, AgeUnit::Days)
    } else {
        ((age / (7.0 * 86_400.0)).round() as i64, AgeUnit::Weeks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The web's `frame_age_label` words, moved here unchanged.
    #[test]
    fn ages_read_in_natural_units() {
        assert_eq!(age_words(0.4), "0 s ago");
        assert_eq!(age_words(12.0), "12 s ago");
        assert_eq!(age_words(89.0), "89 s ago");
        assert_eq!(age_words(90.0), "2 min ago");
        assert_eq!(age_words(1_800.0), "30 min ago");
        assert_eq!(age_words(5_400.0), "2 h ago");
        assert_eq!(age_words(172_799.0), "48 h ago");
        assert_eq!(age_words(172_800.0), "2 d ago");
        assert_eq!(age_words(7.0 * 86_400.0), "7 d ago");
        assert_eq!(age_words(-3.0), "0 s ago");
    }

    /// From two weeks on an age is said in weeks: "13 d" is still days,
    /// "2 weeks" is not "14 d".
    #[test]
    fn two_weeks_and_more_read_in_weeks() {
        let day = 86_400.0;
        assert_eq!(age_words(13.0 * day), "13 d ago");
        assert_eq!(age_words(14.0 * day), "2 weeks ago");
        assert_eq!(age_words(20.0 * day), "3 weeks ago");
        assert_eq!(age_words(60.0 * day), "9 weeks ago");
        assert_eq!(duration_words(14.0 * day), "2 weeks");
        assert_eq!(duration_words(5.0 * 3_600.0), "5 h");
    }

    /// In a sentence the same age spells its unit out, singular at one;
    /// an age of nothing is "just now".
    #[test]
    fn a_spoken_age_spells_its_unit_out() {
        let day = 86_400.0;
        assert_eq!(age_spoken(0.2), "just now");
        assert_eq!(age_spoken(-3.0), "just now");
        assert_eq!(age_spoken(1.0), "1 second ago");
        assert_eq!(age_spoken(12.0), "12 seconds ago");
        assert_eq!(age_spoken(90.0), "2 minutes ago");
        assert_eq!(age_spoken(7_200.0), "2 hours ago");
        assert_eq!(age_spoken(5.0 * 3_600.0), "5 hours ago");
        assert_eq!(age_spoken(3.0 * day), "3 days ago");
        assert_eq!(age_spoken(14.0 * day), "2 weeks ago");
    }
}
