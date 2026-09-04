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
//!
//! [`flash_offer`] and [`reflash_choice`] take a chip string, not a
//! [`DeviceView`] — [`flash_offer_for`] is the honest call site: it reads
//! the JOINED chip ([`device_identity::device_chip`]), which resolves a
//! hello-only board's chip from its board id when the boot banner was never
//! seen (device-card-v2 plan, P2's amendment).

use lpa_devices::Action;
use lpa_devices::device::DeviceStatus;
use lpa_devices::identity::DeviceId;
use lpa_devices::view::{DeviceView, Escape, FirmwareFace};

use super::device_identity::device_chip;
use super::devices_op::DevicesOp;
use crate::UiAction;

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

/// [`flash_offer`], called with a view's JOINED chip rather than its raw
/// `detected_chip` — the honest call site for the needs-firmware face and
/// the pending-link face alike.
pub fn flash_offer_for(view: &DeviceView) -> FlashOffer {
    flash_offer(device_chip(view).as_deref())
}

/// The board a RUNNING card re-flashes as, or `None` when the pick would be
/// a guess: the registered board when the served catalog has it for the
/// joined chip, else the chip's single fit, else nothing (the
/// needs-firmware face owns the picker; this verb never shows one). A card
/// with no joined chip gets no verb — the chip guard has nothing to check
/// the pick against.
///
/// `joined_chip` is [`device_chip`]'s answer, not the raw banner — a
/// hello-only board (an already-running board the tab attached to, which
/// never showed a boot banner) still resolves its re-flash pick from its
/// board id this way.
pub fn reflash_choice(
    joined_chip: Option<&str>,
    registered_board: Option<&str>,
) -> Option<FlashBoardChoice> {
    let chip = joined_chip?;
    let offer = flash_offer(Some(chip));
    offer
        .candidates
        .iter()
        .find(|choice| Some(choice.board_id.as_str()) == registered_board)
        .or_else(
            || match (offer.preselect.as_deref(), offer.candidates.as_slice()) {
                (Some(_), [only]) => Some(only),
                _ => None,
            },
        )
        .cloned()
}

/// The FIRMWARE zone's verb, and which situation it is in. Two verbs for
/// two situations (ruled 2026-09-04) — the card used to carry one label
/// for both:
///
/// - **Flash firmware** on the needs-firmware faces, always with the board
///   pick, because nothing is known about what is on the chip.
/// - **Update firmware** on a running LightPlayer. The board is known →
///   one click, no pick ([`reflash_choice`] resolves). The board is NOT
///   known (the hello reported `?` because the board id comes from the
///   `/hardware.json` manifest Studio stamps at flash and this board was
///   flashed from the CLI; the registry row has no board either; and the
///   chip fits several catalog boards — the bench classic) → the SAME verb
///   opens the pick once, and the panel says why in one line. The firmware
///   line already reads "older than Studio, update recommended", so verb
///   and line now match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FirmwareVerb {
    /// A needs-firmware face: the board pick + Flash, on one row. The
    /// action is the pick's, so this variant carries no choice.
    Flash,
    /// A running LightPlayer whose board resolved: one click.
    Update(FlashBoardChoice),
    /// A running LightPlayer whose board did not resolve: the verb opens
    /// the pick, picking flashes, and the panel explains the detour.
    UpdatePick,
}

impl FirmwareVerb {
    /// Why the pick appears on a running board at all — the one line the
    /// popover shows under its filter line. Yona's words.
    pub const PICK_REASON: &'static str = "This board hasn't said which board it is. Pick \
        once; Studio stamps it at flash, and next time this is one click.";

    /// The verb's label, as the chip wears it.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Flash => "Flash firmware",
            Self::Update(_) | Self::UpdatePick => "Update firmware",
        }
    }

    /// The verb's hover sentence: what happens, and what survives it.
    pub fn summary(&self) -> String {
        match self {
            Self::Flash => "Write LightPlayer firmware for the picked board onto this chip.".to_string(),
            Self::Update(choice) => format!(
                "Write the firmware this Studio serves onto this {}; the project and identity stay.",
                choice.title
            ),
            Self::UpdatePick => "Write the firmware this Studio serves onto the board; the project \
                                 and identity stay. Several boards fit this chip, so say which one it is."
                .to_string(),
        }
    }

    /// The explanation the pick carries, when the verb opens one.
    pub fn pick_reason(&self) -> Option<&'static str> {
        match self {
            Self::UpdatePick => Some(Self::PICK_REASON),
            Self::Flash | Self::Update(_) => None,
        }
    }

    /// The one-click Update as a dispatchable action, wearing this verb's
    /// own label rather than the Flash op's default. `None` for the verbs
    /// whose action is the pick's.
    pub fn update_action(&self, device: DeviceId) -> Option<UiAction> {
        let Self::Update(choice) = self else {
            return None;
        };
        Some(
            DevicesOp::action_for(Action::Flash {
                device,
                board_id: choice.board_id.clone(),
                build_id: choice.build_id.clone(),
                park_first: choice.park_first,
            })
            .with_label(self.label())
            .with_summary(self.summary()),
        )
    }
}

/// Which firmware verb a card offers, or `None` when it offers none: while
/// an activity runs (the row holds Cancel or nothing), while nothing is
/// settled ([`FirmwareFace::Unknown`] — an Identify is running then), on a
/// running board with no wire to flash over, and on one whose chip is not
/// known (the chip guard has nothing to check a pick against).
///
/// The Flash verb needs no wire: the needs-firmware faces come from the
/// link's evidence. Update rides the link (`Escape::Disconnect` is offered
/// exactly when the model has one), and its pick is only offered on a board
/// that is actually up (Ready, or Degraded — a refinement of Ready).
pub fn firmware_verb(view: &DeviceView) -> Option<FirmwareVerb> {
    if view.activity.is_some() {
        return None;
    }
    match &view.firmware_face {
        face if face.wants_flash() => Some(FirmwareVerb::Flash),
        FirmwareFace::LightPlayer { .. } => {
            if !view.escapes.contains(&Escape::Disconnect) {
                return None;
            }
            let chip = device_chip(view)?;
            match reflash_choice(Some(&chip), view.board_id.as_deref()) {
                Some(choice) => Some(FirmwareVerb::Update(choice)),
                None if matches!(view.status, DeviceStatus::Ready | DeviceStatus::Degraded) => {
                    Some(FirmwareVerb::UpdatePick)
                }
                None => None,
            }
        }
        FirmwareFace::Unknown => None,
        // `wants_flash` covered every other face; the arm keeps the match
        // exhaustive so a new face is a compile error here.
        FirmwareFace::NoHello
        | FirmwareFace::Blank
        | FirmwareFace::Bootloader
        | FirmwareFace::Foreign { .. }
        | FirmwareFace::Silent => Some(FirmwareVerb::Flash),
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

    /// The re-flash verb never guesses: the registered board wins, a lone
    /// fit for the joined chip is the only other way in, and no chip means
    /// no verb (the chip guard would have nothing to check). The catalog
    /// carries TWO C6 boards, so a C6 with no registered board gets the
    /// picker, not a pick.
    ///
    /// Moved from `lpa-studio-web`'s `device_roster_card.rs` (#500) — same
    /// semantics, same test — per the device-card-v2 plan's P2 amendment.
    #[test]
    fn the_reflash_pick_is_the_registered_board_or_the_chips_only_fit() {
        let registered = reflash_choice(Some("esp32c6"), Some("seeed/xiao-esp32-c6"))
            .expect("the registered XIAO resolves against the served catalog");
        assert_eq!(registered.board_id, "seeed/xiao-esp32-c6");
        assert_eq!(registered.build_id, "esp32c6-4mb");
        assert!(
            registered.park_first,
            "a native-USB chip parks in its downloader first"
        );

        // Without a registered board, the answer is exactly the offer's
        // one-click preselect — never a pick from a list of several.
        let offer = flash_offer(Some("esp32c6"));
        let unregistered = reflash_choice(Some("esp32c6"), None);
        assert_eq!(
            unregistered.map(|choice| choice.board_id),
            offer.preselect,
            "no registered board: only a lone fit earns the pick",
        );

        // A registered board the chip cannot run falls back the same way,
        // never to the mismatched registration.
        let mismatched = reflash_choice(Some("esp32c6"), Some("dig-uno"));
        assert_ne!(
            mismatched.as_ref().map(|c| c.board_id.as_str()),
            Some("dig-uno")
        );

        assert!(reflash_choice(None, Some("seeed/xiao-esp32-c6")).is_none());
    }

    /// The amendment's new case: a board seen only through its hello (no
    /// boot banner, so `detected_chip` is `None`) still resolves its
    /// re-flash pick, because the CALLER passes the joined chip
    /// (`device_chip`, which reads the board id's catalog family) rather
    /// than the raw banner field.
    #[test]
    fn a_hello_only_boards_reflash_pick_comes_from_its_board_id() {
        use lpa_devices::device::DeviceStatus;
        use lpa_devices::identity::DeviceId;
        use lpa_devices::view::{DeviceView, Escape, LoadedProject};

        let view = DeviceView {
            id: DeviceId(1),
            title: "Bench board".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            // No boot banner: this board was already running when the tab
            // attached, so the only chip fact is its hello's board id.
            detected_chip: None,
            board_id: Some("seeed/xiao-esp32-c6".to_string()),
            firmware_face: lpa_devices::view::FirmwareFace::LightPlayer {
                firmware: Some("fw-esp32c6 abc1234".to_string()),
                wire: lpa_devices::WireVersion::Match,
            },
            degraded: None,
            loaded_project: LoadedProject::Empty,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            escapes: vec![Escape::Disconnect, Escape::Forget],
        };

        assert_eq!(
            reflash_choice(device_chip(&view).as_deref(), view.board_id.as_deref())
                .map(|choice| choice.board_id),
            Some("seeed/xiao-esp32-c6".to_string()),
            "a hello-only board resolves its own registered board, not just a lone fit"
        );

        // The same fixture, through the honest call site.
        let offer = flash_offer_for(&view);
        assert!(
            !offer.candidates.is_empty(),
            "the joined chip (esp32c6, from the board id) filters the offer: {offer:?}"
        );
        for candidate in &offer.candidates {
            let board = lpa_boards::board_by_id(&candidate.board_id).expect("a real board");
            assert_eq!(board.family, "esp32c6");
        }
    }

    /// A running LightPlayer for the verb tests: `chip` is the boot banner's
    /// word, `board` the registry's; the link is up (Disconnect offered).
    fn running_view(chip: Option<&str>, board: Option<&str>, face: FirmwareFace) -> DeviceView {
        use lpa_devices::view::LoadedProject;
        DeviceView {
            id: DeviceId(7),
            title: "Bench classic".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: chip.map(str::to_string),
            board_id: board.map(str::to_string),
            firmware_face: face,
            degraded: None,
            loaded_project: LoadedProject::Running {
                label: "studio".to_string(),
            },
            can_receive_project: true,
            can_remove_project: true,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            escapes: vec![Escape::Disconnect, Escape::Forget],
        }
    }

    fn light_player(wire: lpa_devices::WireVersion) -> FirmwareFace {
        FirmwareFace::LightPlayer {
            firmware: Some("fw-esp32v3 7c80a27".to_string()),
            wire,
        }
    }

    /// The 2026-09-04 ruling: two verbs for two situations. A needs-firmware
    /// face FLASHES (with the pick — nothing is known); a running
    /// LightPlayer UPDATES — one click when its board resolved, the same
    /// verb opening the pick once when it did not, with the reason.
    #[test]
    fn a_needs_firmware_face_flashes_and_a_running_board_updates() {
        use lpa_devices::WireVersion;

        let blank = running_view(Some("esp32c6"), None, FirmwareFace::Blank);
        let verb = firmware_verb(&blank).expect("a blank chip has a verb");
        assert_eq!(verb, FirmwareVerb::Flash);
        assert_eq!(verb.label(), "Flash firmware");
        assert_eq!(verb.pick_reason(), None, "the flash pick needs no excuse");
        assert!(
            verb.update_action(blank.id).is_none(),
            "the pick owns the action"
        );

        // Board known: one click, and the action wears the verb's label.
        let registered = running_view(
            Some("esp32"),
            Some("quinled/dig-uno"),
            light_player(WireVersion::Match),
        );
        let verb = firmware_verb(&registered).expect("a registered classic has a verb");
        let FirmwareVerb::Update(choice) = &verb else {
            panic!("a registered board updates in one click, got {verb:?}");
        };
        assert_eq!(choice.board_id, "quinled/dig-uno");
        assert_eq!(verb.label(), "Update firmware");
        assert_eq!(verb.pick_reason(), None);
        let action = verb.update_action(registered.id).expect("one click");
        assert_eq!(action.meta().label, "Update firmware");
        assert!(
            action.meta().summary.contains(&choice.title),
            "the hover names the board it re-flashes as: {}",
            action.meta().summary
        );

        // The bench case: the hello said `?` (CLI-flashed, no manifest), the
        // registry has no board, and a classic chip fits several boards —
        // the SAME verb, opening the pick once, and saying why.
        let unknown = running_view(Some("esp32"), None, light_player(WireVersion::Match));
        assert!(
            flash_offer(Some("esp32")).candidates.len() > 1,
            "the case needs a chip several boards fit"
        );
        let verb = firmware_verb(&unknown).expect("an unregistered classic still has a verb");
        assert_eq!(verb, FirmwareVerb::UpdatePick);
        assert_eq!(
            verb.label(),
            "Update firmware",
            "the verb does not change with the pick"
        );
        let reason = verb.pick_reason().expect("the pick explains itself");
        assert!(reason.contains("hasn't said which board"), "{reason}");
        assert!(reason.contains("next time this is one click"), "{reason}");
        assert!(
            verb.update_action(unknown.id).is_none(),
            "the pick owns the action"
        );
    }

    /// The line and the verb agree: an older board's line says "update
    /// recommended" and its verb says Update — never Flash beside it.
    #[test]
    fn the_update_verb_matches_the_older_firmware_line() {
        use super::super::device_firmware_face::device_firmware_line;
        use lpa_devices::WireVersion;

        let older = light_player(WireVersion::BoardOlder {
            board: 19,
            studio: 20,
        });
        let line = device_firmware_line(&older, Some("QuinLED-Dig-Uno"));
        assert!(line.contains("update recommended"), "{line}");
        for board in [Some("quinled/dig-uno"), None] {
            let view = running_view(Some("esp32"), board, older.clone());
            let verb = firmware_verb(&view).expect("an older board keeps its verb");
            assert_eq!(verb.label(), "Update firmware", "{board:?}");
            assert_ne!(verb, FirmwareVerb::Flash);
        }
    }

    /// When there is no verb at all: an activity running, nothing settled,
    /// no wire, no chip, or an unresolved board that is not up.
    #[test]
    fn the_verb_withdraws_when_it_has_nothing_honest_to_offer() {
        use lpa_devices::WireVersion;
        use lpa_devices::activity::ActivityKind;
        use lpa_devices::view::ActivityView;

        let base = running_view(
            Some("esp32"),
            Some("quinled/dig-uno"),
            light_player(WireVersion::Match),
        );
        assert!(firmware_verb(&base).is_some());

        let busy = DeviceView {
            activity: Some(ActivityView {
                kind: ActivityKind::Push,
                label: "Sending the project".to_string(),
                percent: Some(40),
                cancellable: true,
                cancel_requested: false,
            }),
            ..base.clone()
        };
        assert_eq!(
            firmware_verb(&busy),
            None,
            "the row holds Cancel or nothing"
        );

        let unsettled = DeviceView {
            firmware_face: FirmwareFace::Unknown,
            ..base.clone()
        };
        assert_eq!(firmware_verb(&unsettled), None, "nothing is known yet");

        let unlinked = DeviceView {
            escapes: vec![Escape::Forget],
            ..base.clone()
        };
        assert_eq!(firmware_verb(&unlinked), None, "no wire to update over");

        let chipless = running_view(None, None, light_player(WireVersion::Match));
        assert_eq!(
            firmware_verb(&chipless),
            None,
            "no chip for the guard to check"
        );

        // A hello-only board (no banner) still resolves through its board id.
        let hello_only = running_view(
            None,
            Some("quinled/dig-uno"),
            light_player(WireVersion::Match),
        );
        assert!(matches!(
            firmware_verb(&hello_only),
            Some(FirmwareVerb::Update(_))
        ));

        // The pick is only offered on a board that is up.
        let down = DeviceView {
            status: DeviceStatus::NotResponding,
            ..running_view(Some("esp32"), None, light_player(WireVersion::Match))
        };
        assert_eq!(firmware_verb(&down), None);
        let degraded = DeviceView {
            status: DeviceStatus::Degraded,
            ..running_view(Some("esp32"), None, light_player(WireVersion::Match))
        };
        assert_eq!(firmware_verb(&degraded), Some(FirmwareVerb::UpdatePick));

        // A blank chip needs no wire fact beyond its face.
        let blank_unlinked = DeviceView {
            escapes: vec![Escape::Forget],
            ..running_view(Some("esp32c6"), None, FirmwareFace::Blank)
        };
        assert_eq!(firmware_verb(&blank_unlinked), Some(FirmwareVerb::Flash));
    }
}
