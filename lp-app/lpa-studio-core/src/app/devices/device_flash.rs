//! The needs-firmware face's decisions: which boards to offer, which build
//! a pick resolves to, and what a freshly-flashed board gets called.
//!
//! In core rather than in the renderer for the usual reason — these are
//! decisions, and decisions get tests. The renderer lays out
//! [`FlashOffer`]'s cards and dispatches the action it hands back.
//!
//! The rules (plan.md "Design pins", ruled):
//!
//! - **Chip necessary, board refinement, no fallback build.** Candidates are
//!   the boards whose family matches the DETECTED chip and that resolve to a
//!   served build (`lpa_boards::provisioning_build_id`). "Either it
//!   matches, or it's a fail case."
//! - **An unknown chip widens the pick, it does not remove the verb** (G1
//!   finding 2026-08-30: a mid-stream attach sees no boot banner, so an
//!   incompatible-proto board had no path forward — and the replug that
//!   would name the chip is exactly what a C6 re-enum walk bans and what
//!   kills a CH340's grant). With no chip we offer every board that
//!   resolves to a served build, never preselect, and say plainly that the
//!   pick is checked against the silicon before anything is written — the
//!   flash preflight's chip guard (`assertChipMatchesManifest`, between
//!   SYNC and write) is and always was the safety; the chip filter is
//!   convenience.
//! - **Preselect when exactly one candidate** — the primary verb is then one
//!   click.
//! - **Skip ALL naming**: the derived name is `"<board display_name> ·
//!   <Mon D>"` with the ` 2`, ` 3` collision suffix, applied automatically
//!   at flash time; `SetName` handles renames later.

use lpa_devices::view::DeviceView;

/// One board the user can pick on the needs-firmware face.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlashBoardChoice {
    pub board_id: String,
    /// The board's display name ("Seeed XIAO ESP32-C6").
    pub title: String,
    /// One terse consequence line for the option card.
    pub blurb: String,
    /// The served build this pick resolves to — carried on the Flash action.
    pub build_id: String,
    /// Park the chip in its ROM downloader before flashing. Native-USB
    /// families only: a blank native-USB chip boot-loops and re-enumerates
    /// every few seconds, cutting esptool's connect mid-SYNC (bench, G1
    /// 2026-08-31 — two straight losses on the C6), while the downloader
    /// waits stably with USB up. Classic UART bridges keep USB alive
    /// through any chip state, and their dance already worked on silicon.
    pub park_first: bool,
}

/// Everything the needs-firmware face needs to offer a board pick.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlashOffer {
    pub candidates: Vec<FlashBoardChoice>,
    /// Picked for the user when there is exactly one candidate.
    pub preselect: Option<String>,
    /// Why there is nothing to offer, when there is not. Honest copy, not a
    /// hidden button.
    pub unavailable: Option<String>,
}

/// The board pick for a detected chip (normalized, e.g. "esp32c6"), or —
/// when no boot output ever named the chip — the full served catalog, with
/// the flash preflight's chip guard as the stated safety.
pub fn flash_offer(detected_chip: Option<&str>) -> FlashOffer {
    let candidates: Vec<FlashBoardChoice> = lpa_boards::all_boards()
        .iter()
        .filter(|board| match detected_chip {
            Some(chip) => board.family == chip,
            // Unknown chip: every board is a candidate; the preflight
            // verifies the pick against the silicon before writing.
            None => true,
        })
        .filter_map(|board| {
            // The join checks flash fit and the served list. No fallback:
            // a board that resolves no build is not offered.
            let build_id =
                lpa_boards::provisioning_build_id(Some(board), Some(board.family.as_str()))?;
            Some(FlashBoardChoice {
                board_id: board.board_id.clone(),
                title: board.display_name.clone(),
                blurb: format!("{} · {} flash", board.manufacturer, board.flash),
                build_id: build_id.to_string(),
                park_first: board.family != "esp32",
            })
        })
        .collect();
    // A wrong preselect is the one risk the widened pick adds, so only a
    // DETECTED chip with exactly one fit earns the one-click path.
    let preselect = match (detected_chip, candidates.as_slice()) {
        (Some(_), [only]) => Some(only.board_id.clone()),
        _ => None,
    };
    let unavailable = candidates.is_empty().then(|| match detected_chip {
        Some(chip) => format!(
            "This build of Studio ships no firmware for a {chip} board — \
             the chip was detected, but no served image runs on it."
        ),
        None => {
            "This build of Studio ships no firmware at all — no board can be offered.".to_string()
        }
    });
    FlashOffer {
        candidates,
        preselect,
        unavailable,
    }
}

/// The auto-derived device name: `"<board display_name> · <Mon D>"`, with
/// the ` 2`, ` 3` collision suffix against names already on the roster.
/// (Yona's ruling: date + type, no naming step; rename later via `SetName`.)
pub fn derive_flash_name(board_display: &str, epoch_secs: f64, taken: &[String]) -> String {
    let base = format!("{board_display} · {}", month_day_label(epoch_secs));
    if !taken.iter().any(|name| name == &base) {
        return base;
    }
    let mut counter = 2_u32;
    loop {
        let candidate = format!("{base} {counter}");
        if !taken.iter().any(|name| name == &candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// The names a new derived name must not collide with: every title the
/// roster currently shows.
pub fn taken_device_titles(devices: &[DeviceView]) -> Vec<String> {
    devices.iter().map(|device| device.title.clone()).collect()
}

/// "Aug 30" from epoch seconds (UTC). A tiny civil-date conversion beats a
/// chrono dependency for one label; the algorithm is the standard
/// days-from-epoch one (Howard Hinnant's `civil_from_days`).
fn month_day_label(epoch_secs: f64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (epoch_secs.max(0.0) as i64) / 86_400;
    let (_, month, day) = civil_from_days(days);
    format!("{} {day}", MONTHS[(month - 1) as usize])
}

/// (year, month 1-12, day 1-31) from days since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_chip_widens_the_pick_instead_of_removing_the_verb() {
        // G1 finding 2026-08-30: a mid-stream attach never sees the boot
        // banner, so an incompatible-proto board must still be flashable —
        // the preflight chip guard, not the filter, is the safety.
        let offer = flash_offer(None);

        assert!(
            !offer.candidates.is_empty(),
            "every served board is a candidate when no chip was detected: {offer:?}"
        );
        assert!(
            offer.preselect.is_none(),
            "an undetected chip never earns a one-click preselect"
        );
        assert!(offer.unavailable.is_none());
        let families: std::collections::BTreeSet<_> = offer
            .candidates
            .iter()
            .map(|candidate| {
                lpa_boards::board_by_id(&candidate.board_id)
                    .expect("a real board")
                    .family
                    .clone()
            })
            .collect();
        assert!(
            families.len() > 1,
            "the widened offer spans chip families: {families:?}"
        );
    }

    #[test]
    fn a_detected_chip_offers_only_boards_that_resolve_a_served_build() {
        let offer = flash_offer(Some("esp32c6"));

        assert!(
            !offer.candidates.is_empty(),
            "the checked-in catalog ships C6 boards: {offer:?}"
        );
        for candidate in &offer.candidates {
            assert!(!candidate.build_id.is_empty(), "{candidate:?}");
            let board = lpa_boards::board_by_id(&candidate.board_id).expect("a real board");
            assert_eq!(board.family, "esp32c6", "chip is the necessary condition");
        }
        if offer.candidates.len() == 1 {
            assert_eq!(
                offer.preselect.as_deref(),
                Some(offer.candidates[0].board_id.as_str())
            );
        } else {
            assert!(offer.preselect.is_none(), "several candidates: user picks");
        }
    }

    #[test]
    fn a_chip_with_no_served_build_reads_honestly() {
        let offer = flash_offer(Some("esp32p4"));

        assert!(offer.candidates.is_empty());
        assert!(
            offer
                .unavailable
                .as_deref()
                .is_some_and(|copy| copy.contains("esp32p4")),
            "{offer:?}"
        );
    }

    #[test]
    fn derived_names_stamp_the_date_and_dodge_collisions() {
        // 2026-08-30 12:00 UTC (20_695 days + noon).
        let at = 1_788_091_200.0;
        assert_eq!(
            derive_flash_name("Seeed XIAO ESP32-C6", at, &[]),
            "Seeed XIAO ESP32-C6 · Aug 30"
        );

        let taken = vec![
            "Seeed XIAO ESP32-C6 · Aug 30".to_string(),
            "Seeed XIAO ESP32-C6 · Aug 30 2".to_string(),
        ];
        assert_eq!(
            derive_flash_name("Seeed XIAO ESP32-C6", at, &taken),
            "Seeed XIAO ESP32-C6 · Aug 30 3"
        );
    }

    #[test]
    fn the_civil_date_conversion_is_correct_at_the_edges() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2000-02-29 (leap): 11_016 days after epoch.
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        // 2026-12-31.
        assert_eq!(civil_from_days(20_818), (2026, 12, 31));
    }
}
