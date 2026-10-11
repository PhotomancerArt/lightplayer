//! How long ago something happened, in units that read naturally (the
//! unit-awareness principle): seconds while seconds are meaningful, then
//! minutes, hours, days, and from two weeks on, weeks.
//!
//! One rule for every surface that names an age: the board card's status
//! corner ("5 h ago" for the picture), the connection bar of a board that is
//! away ("Offline · 2 weeks"), and the web's live-picture surfaces, which
//! read it from here. A stale picture counting "412 s" is arithmetic
//! homework; a remembered board's picture can be a month old.

/// Seconds below which an age is said in seconds.
const MINUTES_FROM: f64 = 90.0;
/// … in minutes.
const HOURS_FROM: f64 = 5_400.0;
/// … in hours.
const DAYS_FROM: f64 = 172_800.0;
/// … in days; from here on, in weeks.
const WEEKS_FROM: f64 = 14.0 * 86_400.0;

/// An age as the picture's reading says it: "12 s ago", "2 min ago",
/// "5 h ago", "7 d ago", "3 weeks ago". A negative age (a clock that
/// stepped back) reads "0 s ago".
pub fn age_words(age_secs: f64) -> String {
    format!("{} ago", duration_words(age_secs))
}

/// The same age as a duration, for a sentence that says "for how long":
/// "Offline · 2 weeks", "Offline · 5 h".
pub fn duration_words(age_secs: f64) -> String {
    let age = age_secs.max(0.0);
    if age < MINUTES_FROM {
        format!("{} s", age.round() as i64)
    } else if age < HOURS_FROM {
        format!("{} min", (age / 60.0).round() as i64)
    } else if age < DAYS_FROM {
        format!("{} h", (age / 3_600.0).round() as i64)
    } else if age < WEEKS_FROM {
        format!("{} d", (age / 86_400.0).round() as i64)
    } else {
        format!("{} weeks", (age / (7.0 * 86_400.0)).round() as i64)
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
}
