//! The device card's link section (plan D13): the board's own lp-link
//! counters, off its heartbeat, in words and units that read at a glance —
//! "3 frames", "2 times", "412 KB sent · 38 KB received".
//!
//! The words are the board's view, because the numbers are: a resend is a
//! frame the BOARD sent again, a damaged frame is one that reached the board
//! broken. On a clean cable every count but the traffic stays at 0, which is
//! why a count above 0 is marked [`UiLinkCounterRow::notable`].

use crate::DeviceLinkCounters;

/// One row of the section: a label and its value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiLinkCounterRow {
    /// "Resent", "Arrived damaged", …
    pub label: &'static str,
    /// "3 frames", "1 time", "412 KB sent · 38 KB received"
    pub value: String,
    /// Worth a second look: a recovery the link had to make.
    pub notable: bool,
}

/// The section's caption: whose numbers these are, and since when.
pub const LINK_COUNTERS_CAPTION: &str = "Counted by the board since it started.";

/// The section's rows, in reading order.
pub fn link_counter_rows(counters: &DeviceLinkCounters) -> Vec<UiLinkCounterRow> {
    vec![
        count_row("Resent", counters.resends, "frame", "frames"),
        count_row("Arrived damaged", counters.damaged, "frame", "frames"),
        count_row("Restarted", counters.resets, "time", "times"),
        count_row("Went quiet", counters.stalls, "time", "times"),
        UiLinkCounterRow {
            label: "Traffic",
            value: format!(
                "{} sent · {} received",
                byte_size(counters.bytes_sent),
                byte_size(counters.bytes_received)
            ),
            notable: false,
        },
    ]
}

fn count_row(label: &'static str, count: u32, one: &str, many: &str) -> UiLinkCounterRow {
    let unit = if count == 1 { one } else { many };
    UiLinkCounterRow {
        label,
        value: format!("{count} {unit}"),
        notable: count > 0,
    }
}

/// A byte count in the unit that keeps the number short: `980 B`,
/// `4.2 KB`, `38 KB`, `1.3 MB`. Binary steps, as the rest of Studio sizes
/// things; one decimal below ten, none above.
fn byte_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_link_reads_as_zeros_and_traffic() {
        let rows = link_counter_rows(&DeviceLinkCounters {
            bytes_sent: 421_888,
            bytes_received: 38_912,
            ..Default::default()
        });
        let text: Vec<String> = rows
            .iter()
            .map(|row| format!("{}: {}", row.label, row.value))
            .collect();
        assert_eq!(
            text,
            [
                "Resent: 0 frames",
                "Arrived damaged: 0 frames",
                "Restarted: 0 times",
                "Went quiet: 0 times",
                "Traffic: 412 KB sent · 38 KB received",
            ]
        );
        assert!(rows.iter().all(|row| !row.notable));
    }

    #[test]
    fn a_recovery_is_notable_and_singular_reads_singular() {
        let rows = link_counter_rows(&DeviceLinkCounters {
            resends: 1,
            resets: 2,
            ..Default::default()
        });
        assert_eq!(rows[0].value, "1 frame");
        assert!(rows[0].notable);
        assert_eq!(rows[2].value, "2 times");
        assert!(rows[2].notable);
        assert!(!rows[1].notable);
    }

    #[test]
    fn bytes_take_the_unit_that_keeps_the_number_short() {
        assert_eq!(byte_size(0), "0 B");
        assert_eq!(byte_size(980), "980 B");
        assert_eq!(byte_size(4_300), "4.2 KB");
        assert_eq!(byte_size(38_912), "38 KB");
        assert_eq!(byte_size(1_363_149), "1.3 MB");
        assert_eq!(byte_size(52_428_800), "50 MB");
    }
}
