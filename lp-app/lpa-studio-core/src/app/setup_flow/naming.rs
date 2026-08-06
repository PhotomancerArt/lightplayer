//! R4 — the name the provision step prefills.
//!
//! Design: `docs/design/device-setup-flow.md` §3. Names are derived here,
//! rarely typed: a remembered board keeps the name it already had, a new
//! one gets `<board label> · <Mon D>`, and a collision takes a ` 2` / ` 3`
//! suffix. Shared with the card's rename placeholder.
//!
//! Pure, and clockless by construction: the month/day comes from the
//! already-injected library stamp (`YYYY-MM-DD-HHMM`, see
//! `library::package_slug::dated_slug`), never from a clock read — core
//! reads neither time nor timezone.
//!
//! The generated **project's** name is NOT derived here. It stays the
//! library's convention: `generate_board_project` names the package after
//! the board and `LibraryStore::install_package` dates the slug through
//! `dated_slug`, giving `YYYY-MM-DD-HHMM-<board-slug>`. Two naming schemes
//! for one gesture is confusing enough without a second implementation of
//! either.

/// The default device name for the provision step.
///
/// - `remembered` is the registry row's name when the probed MAC matched one.
/// - `board_label` is the catalog `display_name` (design §7.6).
/// - `stamp` is the injected `YYYY-MM-DD-HHMM` library stamp.
/// - `taken` is every OTHER device's name. The caller excludes the row this
///   device already owns — a board must not be renamed "Porch sign 2" for
///   colliding with itself.
pub fn derive_device_name(
    remembered: Option<&str>,
    board_label: &str,
    stamp: &str,
    taken: &[String],
) -> String {
    let remembered = remembered.map(str::trim).filter(|name| !name.is_empty());
    let base = match remembered {
        Some(name) => name.to_string(),
        None => match month_day_label(stamp) {
            Some(date) => format!("{} · {date}", board_label.trim()),
            // A malformed stamp must not poison the name with a broken
            // date; the board label alone is honest.
            None => board_label.trim().to_string(),
        },
    };
    unique_device_name(&base, taken)
}

/// First of `base`, `base 2`, `base 3`, … not present in `taken`.
pub fn unique_device_name(base: &str, taken: &[String]) -> String {
    let base = if base.trim().is_empty() {
        "Device"
    } else {
        base.trim()
    };
    if !taken.iter().any(|name| name == base) {
        return base.to_string();
    }
    for suffix in 2.. {
        let candidate = format!("{base} {suffix}");
        if !taken.iter().any(|name| *name == candidate) {
            return candidate;
        }
    }
    unreachable!("unbounded suffix search")
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `"2026-08-04-1421"` → `"Aug 4"`. `None` for anything that is not a
/// well-formed `YYYY-MM-DD…` stamp, including an out-of-range month.
pub fn month_day_label(stamp: &str) -> Option<String> {
    let mut parts = stamp.split('-');
    let _year = parts.next().filter(|y| y.len() == 4 && all_digits(y))?;
    let month: usize = parts
        .next()
        .filter(|m| m.len() == 2 && all_digits(m))?
        .parse()
        .ok()?;
    let day: u32 = parts
        .next()
        .filter(|d| d.len() == 2 && all_digits(d))?
        .parse()
        .ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{} {day}", MONTHS[month - 1]))
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAMP: &str = "2026-08-04-1421";

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_remembered_board_keeps_its_name() {
        assert_eq!(
            derive_device_name(Some("Porch sign"), "XIAO ESP32-C6", STAMP, &[]),
            "Porch sign"
        );
    }

    #[test]
    fn an_empty_remembered_name_is_not_a_name() {
        // A MAC-identified board arrives with a uid and an empty name
        // (device identity design §3); that row must not win over the
        // derived default.
        assert_eq!(
            derive_device_name(Some("   "), "XIAO ESP32-C6", STAMP, &[]),
            "XIAO ESP32-C6 · Aug 4"
        );
    }

    #[test]
    fn a_new_board_gets_board_and_date() {
        assert_eq!(
            derive_device_name(None, "XIAO ESP32-C6", STAMP, &[]),
            "XIAO ESP32-C6 · Aug 4"
        );
    }

    #[test]
    fn collisions_take_a_numeric_suffix() {
        let taken = names(&["XIAO ESP32-C6 · Aug 4"]);
        assert_eq!(
            derive_device_name(None, "XIAO ESP32-C6", STAMP, &taken),
            "XIAO ESP32-C6 · Aug 4 2"
        );

        let taken = names(&["XIAO ESP32-C6 · Aug 4", "XIAO ESP32-C6 · Aug 4 2"]);
        assert_eq!(
            derive_device_name(None, "XIAO ESP32-C6", STAMP, &taken),
            "XIAO ESP32-C6 · Aug 4 3"
        );
    }

    #[test]
    fn a_remembered_name_collides_too() {
        // Two boards remembered under the same name (a rename typo, an
        // imported registry) still have to end up distinguishable.
        let taken = names(&["Porch sign"]);
        assert_eq!(
            derive_device_name(Some("Porch sign"), "XIAO ESP32-C6", STAMP, &taken),
            "Porch sign 2"
        );
    }

    #[test]
    fn the_suffix_search_skips_gaps_rather_than_reusing_a_taken_name() {
        let taken = names(&["Rig", "Rig 3"]);
        assert_eq!(unique_device_name("Rig", &taken), "Rig 2");
        let taken = names(&["Rig", "Rig 2", "Rig 4"]);
        assert_eq!(unique_device_name("Rig", &taken), "Rig 3");
    }

    #[test]
    fn an_empty_base_still_produces_a_name() {
        assert_eq!(unique_device_name("   ", &[]), "Device");
    }

    #[test]
    fn month_day_reads_the_injected_library_stamp() {
        assert_eq!(month_day_label("2026-08-04-1421").as_deref(), Some("Aug 4"));
        assert_eq!(
            month_day_label("2026-01-31-0000").as_deref(),
            Some("Jan 31")
        );
        assert_eq!(month_day_label("2026-12-09").as_deref(), Some("Dec 9"));
    }

    #[test]
    fn a_malformed_stamp_yields_no_date_and_no_broken_name() {
        for stamp in ["", "today", "26-8-4", "2026-13-01-0000", "2026-08-32-0000"] {
            assert_eq!(month_day_label(stamp), None, "{stamp:?}");
        }
        assert_eq!(
            derive_device_name(None, "XIAO ESP32-C6", "not-a-stamp", &[]),
            "XIAO ESP32-C6"
        );
    }

    #[test]
    fn the_project_name_stays_the_library_convention() {
        // Pins design §3's second half: the PROJECT is named by the
        // library's dated slug, not by this module.
        let slug =
            crate::app::library::package_slug::dated_slug("2026-08-04-1421", "XIAO ESP32-C6", &[]);
        assert_eq!(slug, "2026-08-04-1421-xiao-esp32-c6");
    }
}
