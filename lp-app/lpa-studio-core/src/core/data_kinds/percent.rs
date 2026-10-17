//! A percentage: how far along, as a whole number with its sign — "42%".
//! Never past 100, never a decimal (a row's progress is a glance, not a
//! measurement), and never "42 %" or "42 percent".

/// `percent` as the card and the progress lines say it, clamped at 100.
pub fn percent(percent: u32) -> String {
    format!("{}%", percent.min(100))
}

/// `part` of `whole` as a percentage, rounded; `None` when there is no
/// whole to measure against.
pub fn percent_of(part: f64, whole: f64) -> Option<String> {
    (whole > 0.0).then(|| {
        let share = ((part / whole) * 100.0).clamp(0.0, 100.0).round();
        percent(share as u32)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percentage_is_a_whole_number_clamped_at_a_hundred() {
        assert_eq!(percent(0), "0%");
        assert_eq!(percent(42), "42%");
        assert_eq!(percent(100), "100%");
        assert_eq!(percent(250), "100%");
    }

    #[test]
    fn a_share_rounds_and_needs_a_whole() {
        assert_eq!(percent_of(1.0, 3.0).as_deref(), Some("33%"));
        assert_eq!(percent_of(2.0, 3.0).as_deref(), Some("67%"));
        assert_eq!(percent_of(5.0, 4.0).as_deref(), Some("100%"));
        assert_eq!(percent_of(-1.0, 4.0).as_deref(), Some("0%"));
        assert_eq!(percent_of(1.0, 0.0), None);
    }
}
