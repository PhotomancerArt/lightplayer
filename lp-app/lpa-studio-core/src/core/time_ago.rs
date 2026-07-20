//! Relative-time labels for card meta and status lines.

/// "just now" / "5m ago" / "3h ago" / "4d ago" / "6w ago". Coarse on
/// purpose: card meta is a recall clue, not a log timestamp.
pub fn time_ago(now_secs: f64, then_secs: f64) -> String {
    let elapsed = (now_secs - then_secs).max(0.0);
    if elapsed < 90.0 {
        return "just now".to_string();
    }
    let minutes = elapsed / 60.0;
    if minutes < 90.0 {
        return format!("{}m ago", minutes.round() as u64);
    }
    let hours = minutes / 60.0;
    if hours < 36.0 {
        return format!("{}h ago", hours.round() as u64);
    }
    let days = hours / 24.0;
    if days < 14.0 {
        return format!("{}d ago", days.round() as u64);
    }
    format!("{}w ago", (days / 7.0).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::time_ago;

    #[test]
    fn buckets_read_naturally() {
        let now = 1_000_000.0;
        assert_eq!(time_ago(now, now - 30.0), "just now");
        assert_eq!(time_ago(now, now - 300.0), "5m ago");
        assert_eq!(time_ago(now, now - 3.0 * 3600.0), "3h ago");
        assert_eq!(time_ago(now, now - 4.0 * 86_400.0), "4d ago");
        assert_eq!(time_ago(now, now - 42.0 * 86_400.0), "6w ago");
    }

    #[test]
    fn future_timestamps_clamp_to_just_now() {
        assert_eq!(time_ago(100.0, 200.0), "just now");
    }
}
