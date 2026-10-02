//! The device card's link section (plan D13): the board's own lp-link
//! counters, off its heartbeat, in words and units that read at a glance —
//! "3 frames", "2 times", "412 KB".
//!
//! The words are the board's view, because the numbers are: a resend is a
//! frame the BOARD sent again, a damaged frame is one that reached the board
//! broken. On a clean cable every count but the traffic stays at 0.
//!
//! DD2 (2026-09-28, the lp-link comms-layer director): amber used to mean
//! "count above 0", and Yona found it too eager — "no one will care if a
//! few packets got lost". A restart or a stall stays amber the moment it
//! happens (there is no good reason for either), but a resend or a damaged
//! frame is only [`UiLinkCounterRow::notable`] once it clears a share of
//! that direction's traffic ([`is_notable_rate`]) — a few lost packets on a
//! link that has carried thousands of frames reads as the noise it is.

use crate::DeviceLinkCounters;

/// Below this many frames in a direction, a resend/damage ratio is too
/// noisy to judge (DD2's "min 20 frames") — 1 resend out of 2 frames sent
/// is 50 %, and says nothing about the link.
const NOTABLE_RATE_MIN_FRAMES: u32 = 20;

/// The share of a direction's frames a resend/damage count must clear
/// before it is worth the warning tone (DD2: "5 % of that direction's
/// frames") — a clean link runs near 0 %, so a few points above stands out.
const NOTABLE_RATE_PERCENT: u32 = 5;

/// One row of the section: a label and its value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiLinkCounterRow {
    /// "Resent", "Arrived damaged", …
    pub label: &'static str,
    /// "3 frames", "1 time", "412 KB"
    pub value: String,
    /// Worth a second look: a recovery the link had to make.
    pub notable: bool,
}

/// The section's caption: whose numbers these are, and since when.
pub const LINK_COUNTERS_CAPTION: &str = "Counted by the board since it started.";

/// The section's rows, in reading order.
pub fn link_counter_rows(counters: &DeviceLinkCounters) -> Vec<UiLinkCounterRow> {
    vec![
        rate_row(
            "Resent",
            counters.resends,
            counters.frames_sent,
            "frame",
            "frames",
        ),
        rate_row(
            "Arrived damaged",
            counters.damaged,
            counters.frames_received,
            "frame",
            "frames",
        ),
        count_row("Restarted", counters.resets, "time", "times"),
        count_row("Went quiet", counters.stalls, "time", "times"),
        UiLinkCounterRow {
            label: "Sent",
            value: byte_size(counters.bytes_sent),
            notable: false,
        },
        UiLinkCounterRow {
            label: "Received",
            value: byte_size(counters.bytes_received),
            notable: false,
        },
    ]
}

/// A restart or a stall: notable the moment it happens (DD2 leaves these
/// two alone — there is no good reason for either).
fn count_row(label: &'static str, count: u32, one: &str, many: &str) -> UiLinkCounterRow {
    let unit = if count == 1 { one } else { many };
    UiLinkCounterRow {
        label,
        value: format!("{count} {unit}"),
        notable: count > 0,
    }
}

/// A resend or a damaged-frame count: notable only once it clears
/// [`is_notable_rate`]'s floor (DD2) — otherwise plain, however small a
/// share of `total_frames` it already is.
fn rate_row(
    label: &'static str,
    count: u32,
    total_frames: u32,
    one: &str,
    many: &str,
) -> UiLinkCounterRow {
    let unit = if count == 1 { one } else { many };
    UiLinkCounterRow {
        label,
        value: format!("{count} {unit}"),
        notable: is_notable_rate(count, total_frames),
    }
}

/// Whether `count` (a resend or damaged-frame tally) is worth the warning
/// tone against `total_frames` (that direction's whole traffic) — DD2:
/// plain below [`NOTABLE_RATE_MIN_FRAMES`] frames of traffic (too little to
/// judge a ratio from), and plain at or below [`NOTABLE_RATE_PERCENT`] of
/// it once there is enough traffic to judge.
fn is_notable_rate(count: u32, total_frames: u32) -> bool {
    total_frames >= NOTABLE_RATE_MIN_FRAMES
        && u64::from(count) * 100 > u64::from(total_frames) * u64::from(NOTABLE_RATE_PERCENT)
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
    fn a_clean_link_reads_as_zeros_and_its_traffic() {
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
                "Sent: 412 KB",
                "Received: 38 KB",
            ]
        );
        assert!(rows.iter().all(|row| !row.notable));
    }

    #[test]
    fn a_restart_or_a_stall_is_notable_at_any_count_and_singular_reads_singular() {
        let rows = link_counter_rows(&DeviceLinkCounters {
            resets: 2,
            stalls: 1,
            ..Default::default()
        });
        assert_eq!(rows[2].value, "2 times");
        assert!(rows[2].notable);
        assert_eq!(rows[3].value, "1 time");
        assert!(rows[3].notable);
    }

    /// DD2: a resend or a damaged frame reads as noise — plain — until the
    /// direction has carried enough traffic to judge a ratio from, and even
    /// then only past the 5 % floor. "1 resend out of 2 frames sent" is a
    /// worse ratio than any of these and still reads as plain, because 2
    /// frames is not enough traffic to say anything about the link.
    #[test]
    fn a_resend_or_damaged_frame_stays_plain_below_the_rate_floor() {
        let rows = link_counter_rows(&DeviceLinkCounters {
            resends: 1,
            frames_sent: 2,
            damaged: 5,
            frames_received: 19,
            ..Default::default()
        });
        assert_eq!(rows[0].value, "1 frame");
        assert!(
            !rows[0].notable,
            "2 frames of traffic is not enough to judge"
        );
        assert_eq!(rows[1].value, "5 frames");
        assert!(!rows[1].notable, "19 is one short of the 20-frame floor");
    }

    /// The exact boundary DD2 names: "above 5 %", not "at or above" — 1 in
    /// 20 is exactly 5 % and stays plain, 2 in 20 (10 %) is notable.
    #[test]
    fn the_rate_floor_boundary_is_above_five_percent_not_at_it() {
        let at_five_percent = link_counter_rows(&DeviceLinkCounters {
            resends: 1,
            frames_sent: 20,
            ..Default::default()
        });
        assert!(
            !at_five_percent[0].notable,
            "exactly 5 % is not \"above\" it"
        );

        let above_five_percent = link_counter_rows(&DeviceLinkCounters {
            resends: 2,
            frames_sent: 20,
            ..Default::default()
        });
        assert!(above_five_percent[0].notable, "10 % clears the floor");
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
