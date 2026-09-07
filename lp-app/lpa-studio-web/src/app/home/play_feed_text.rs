//! Words the live-picture surfaces share: the sim card's ▶ tab and the
//! device card's preview slot say the same things about a frame's age, so
//! they say them from one place.

/// A frame age in units that read naturally (the unit-awareness principle):
/// seconds while seconds are meaningful, then minutes, then hours. A stale
/// pill counting "412 s" is arithmetic homework.
pub(crate) fn frame_age_label(age_secs: f64) -> String {
    let age = age_secs.max(0.0);
    if age < 90.0 {
        format!("{} s ago", age.round() as i64)
    } else if age < 5400.0 {
        format!("{} min ago", (age / 60.0).round() as i64)
    } else {
        format!("{} h ago", (age / 3600.0).round() as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_read_in_natural_units() {
        assert_eq!(frame_age_label(0.4), "0 s ago");
        assert_eq!(frame_age_label(12.0), "12 s ago");
        assert_eq!(frame_age_label(89.0), "89 s ago");
        assert_eq!(frame_age_label(90.0), "2 min ago");
        assert_eq!(frame_age_label(1_800.0), "30 min ago");
        assert_eq!(frame_age_label(5_400.0), "2 h ago");
        assert_eq!(frame_age_label(-3.0), "0 s ago");
    }
}
