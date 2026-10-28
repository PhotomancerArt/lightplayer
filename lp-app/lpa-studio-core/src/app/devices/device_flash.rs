//! The firmware verbs' decisions: what an Install or Update press writes,
//! which boards "Which board is this?" offers, and what a freshly-flashed
//! board gets called.
//!
//! In core rather than in the renderer for the usual reason — these are
//! decisions, and decisions get tests.
//!
//! The rules:
//!
//! - **Nobody picks a board first** (plan
//!   `lp2025/2026-10-28-0025-install-without-board-pick`, Yona and Sean
//!   2026-10-27). The press carries the chip table ([`FlashTarget`]); the
//!   bootloader session picks the build by the chip its SYNC names, reads
//!   what the board runs, and guesses the board from a fingerprint. The
//!   chip filter used to be the only narrowing, and it was missing exactly
//!   when it mattered: a mid-stream attach or a quiet firmware never names
//!   its chip passively.
//! - **No fallback build** (Yona 2026-08-03: "either it matches, or it's a
//!   fail case"): a chip the table lacks stops the install, nothing
//!   written; the flash preflight's chip guard (`assertChipMatchesManifest`,
//!   between SYNC and write) still checks the package against the silicon.
//! - **The board model is a refinement, asked after when it must be** (D1):
//!   the board this board already named, else the fingerprint's unique fit,
//!   else nothing stamped — and the card's "Which board is this?" offers
//!   [`flash_offer_for`]'s boards (the chip's catalog boards).
//! - **Skip ALL naming**: the derived name is the board's display name (or
//!   the chip's, before the board is known) with the ` 2`, ` 3` collision
//!   suffix, applied automatically at flash time; `SetName` handles renames
//!   later.

use lpa_devices::identity::DeviceId;
use lpa_devices::link::UsbIds;
use lpa_devices::view::{DeviceView, Escape, FirmwareFace, PendingLinkView};
use lpa_devices::{Action, ChipBuildRow, FlashBuildChoice};

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
/// `detected_chip` — the board list "Which board is this?" offers.
pub fn flash_offer_for(view: &DeviceView) -> FlashOffer {
    flash_offer(device_chip(view).as_deref())
}

/// What an Install or Update press writes, decided before the press
/// (plan `lp2025/2026-10-28-0025-install-without-board-pick`).
///
/// Nobody picks a board first. The build is the chip's when Studio has
/// heard the chip; otherwise it is the chip table
/// ([`lpa_boards::builds_by_chip`]), answered inside the bootloader session
/// by the chip its SYNC names — so a board plugged in mid-stream, or one
/// running a quiet firmware, installs on one press. The board model is the
/// one this board already named when it is a catalog board of its chip;
/// otherwise the session's fingerprint decides, and an ambiguous one stamps
/// nothing (the card asks "Which board is this?" afterwards).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlashTarget {
    pub build: FlashBuildChoice,
    pub board_id: Option<String>,
    /// Park the chip in its ROM downloader first (native USB).
    pub park_first: bool,
}

impl FlashTarget {
    /// The Flash gesture for `device`.
    pub fn action(&self, device: DeviceId, restore_backup: bool) -> Action {
        Action::Flash {
            device,
            board_id: self.board_id.clone(),
            build: self.build.clone(),
            park_first: self.park_first,
            name: None,
            restore_backup,
        }
    }
}

/// Espressif's USB vendor id: a chip on its own USB-Serial-JTAG.
const ESPRESSIF_USB_VENDOR: u16 = 0x303A;

/// The target for a board whose chip (joined, when known), named board and
/// USB bridge are these, or why nothing can be installed: this Studio
/// serves no firmware at all, or none for a chip it already knows.
pub fn flash_target(
    chip: Option<&str>,
    named_board: Option<&str>,
    usb: Option<UsbIds>,
) -> Result<FlashTarget, String> {
    let table = lpa_boards::builds_by_chip();
    if table.is_empty() {
        return Err(
            "This build of Studio ships no firmware at all — no board can be installed."
                .to_string(),
        );
    }
    let known_build = match chip {
        Some(chip) => match table.iter().find(|(served, _)| served == chip) {
            Some((_, build_id)) => Some(build_id.clone()),
            None => {
                return Err(format!(
                    "This build of Studio ships no firmware for a {chip} board — the chip was \
                     detected, but no served image runs on it."
                ));
            }
        },
        None => None,
    };
    let board_id = named_board
        .and_then(lpa_boards::board_by_id)
        .filter(|board| chip.is_none_or(|chip| board.family == chip))
        .filter(|board| {
            lpa_boards::provisioning_build_id(Some(board), Some(board.family.as_str())).is_some()
        })
        .map(|board| board.board_id.clone());
    let chip = chip.map(str::to_string).or_else(|| {
        board_id
            .as_deref()
            .and_then(lpa_boards::board_by_id)
            .map(|board| board.family.clone())
    });
    // A native-USB chip parks in its downloader first; a classic behind a
    // bridge does not. With no chip yet, the bridge says which it is.
    let park_first = match chip.as_deref() {
        Some(chip) => chip != "esp32",
        None => usb.is_some_and(|usb| usb.vendor == ESPRESSIF_USB_VENDOR),
    };
    // A chip already heard gets its build now (a sim or an emulated tab
    // board reads nothing in a bootloader); one nobody has heard gets the
    // table, answered in the session.
    let build = match known_build {
        Some(build_id) => FlashBuildChoice::Fixed(build_id),
        None => FlashBuildChoice::ByChip(
            table
                .into_iter()
                .map(|(chip, build_id)| ChipBuildRow { chip, build_id })
                .collect(),
        ),
    };
    Ok(FlashTarget {
        build,
        board_id,
        park_first,
    })
}

/// [`flash_target`] for a card: its joined chip, the board it named, its
/// link's USB bridge.
pub fn flash_target_for(view: &DeviceView) -> Result<FlashTarget, String> {
    flash_target(
        device_chip(view).as_deref(),
        view.board_id.as_deref(),
        view.usb,
    )
}

/// [`flash_target`] for a pending link: its boot banner's chip and its
/// USB bridge (a pending link names no board).
pub fn pending_flash_target(pending: &PendingLinkView) -> Result<FlashTarget, String> {
    flash_target(pending.detected_chip.as_deref(), None, pending.usb)
}

/// The chip's name as people say it ("ESP32-C6"), from the catalog.
pub fn chip_display_name(chip: &str) -> Option<String> {
    lpa_boards::all_boards()
        .iter()
        .find(|board| board.family == chip)
        .map(|board| board.soc.clone())
}

/// The FIRMWARE zone's verb, and which situation it is in. Two verbs for
/// two situations (ruled 2026-09-04):
///
/// - **Flash firmware** on the needs-firmware faces: one press. The board
///   is read in its bootloader before anything is written, and only
///   somebody else's firmware asks first (inside the activity).
/// - **Update firmware** on a running LightPlayer: one press too; the board
///   keeps its project and identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FirmwareVerb {
    Flash,
    Update,
}

impl FirmwareVerb {
    /// The verb's label, as the chip wears it.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Flash => "Flash firmware",
            Self::Update => "Update firmware",
        }
    }

    /// The verb's hover sentence: what happens, and what survives it.
    pub fn summary(&self) -> String {
        match self {
            Self::Flash => "Install LightPlayer on this board. Studio reads which chip it is and \
                            writes the firmware for it."
                .to_string(),
            Self::Update => "Write the firmware this Studio serves onto this board; the project \
                             and identity stay."
                .to_string(),
        }
    }

    /// This verb as a DISABLED action carrying `reason` — what the card
    /// draws when the link cannot carry firmware (a board reached over
    /// Bluetooth: [`DeviceView::firmware_blocked`]) or no firmware is
    /// served for the chip. Disabled rather than hidden, so "why can't I
    /// update this?" is answered where it is asked; the action inside is
    /// never dispatched.
    pub fn blocked_action(&self, device: DeviceId, reason: &str) -> UiAction {
        DevicesOp::action_for(Action::Flash {
            device,
            board_id: None,
            build: FlashBuildChoice::ByChip(Vec::new()),
            park_first: false,
            name: None,
            restore_backup: false,
        })
        .with_label(self.label())
        .with_summary(self.summary())
        .disabled(reason)
    }

    /// The press for `view`, wearing this verb's own label: dispatchable
    /// when [`flash_target_for`] resolves, else disabled with why.
    pub fn action_for(&self, view: &DeviceView) -> UiAction {
        match flash_target_for(view) {
            Ok(target) => DevicesOp::action_for(target.action(view.id, false))
                .with_label(self.label())
                .with_summary(self.summary()),
            Err(reason) => self.blocked_action(view.id, &reason),
        }
    }
}

/// Which firmware verb a card offers, or `None` when it offers none: while
/// an activity runs (the row holds Cancel or nothing), while nothing is
/// settled ([`FirmwareFace::Unknown`] — an Identify is running then), and
/// on a running board with no wire to flash over.
///
/// The Flash verb needs no wire: the needs-firmware faces come from the
/// link's evidence. Update rides the link (`Escape::Disconnect` is offered
/// exactly when the model has one).
///
/// An older LightPlayer (one whose wire this Studio cannot speak) is the
/// running-board case too — a LightPlayer, its project staying — so it
/// UPDATES (G1-F1: every fielded wire-32 C6, in a wire-33 Studio).
pub fn firmware_verb(view: &DeviceView) -> Option<FirmwareVerb> {
    if view.activity.is_some() {
        return None;
    }
    match &view.firmware_face {
        FirmwareFace::OlderLightPlayer { .. } => Some(FirmwareVerb::Update),
        FirmwareFace::LightPlayer { .. } => view
            .escapes
            .contains(&Escape::Disconnect)
            .then_some(FirmwareVerb::Update),
        FirmwareFace::Unknown => None,
        // A core-only board's verbs are its update standing's over-the-air
        // offers (`device_update_offers`): its firmware comes back over its
        // own link, never by a flash.
        FirmwareFace::CoreOnly { .. } => None,
        FirmwareFace::NoHello
        | FirmwareFace::Blank
        | FirmwareFace::Bootloader
        | FirmwareFace::Foreign { .. }
        | FirmwareFace::Silent => Some(FirmwareVerb::Flash),
    }
}

/// Factory reset as a DISABLED action carrying `reason`, for the same card
/// state as [`FirmwareVerb::blocked_action`]: erasing is a firmware verb too
/// (it needs the ROM downloader), so it is refused the same way.
pub fn blocked_erase_action(device: DeviceId, reason: &str) -> UiAction {
    DevicesOp::action_for(Action::Erase { device }).disabled(reason)
}

/// The auto-derived device name: the board's display name ("XIAO
/// ESP32-C6"), with the ` 2`, ` 3` collision suffix against names already
/// on the roster. No naming step; rename later via `SetName`. (Yona's O5,
/// 2026-10-16: no date — "· Oct 11" read as a time; two boards of one model
/// get a number.)
pub fn derive_flash_name(board_display: &str, taken: &[String]) -> String {
    let base = board_display.to_string();
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

/// The names a new derived name for `naming` must not collide with: every
/// title the roster currently shows on ANOTHER card.
///
/// The card being named is left out so that naming is a fixed point: a board
/// whose title already IS the derived name must derive that same name again,
/// never "… 2" because it collided with itself. Counting it is what let one
/// misrouted rename flip a title between "… · Oct 2" and "… · Oct 2 2" on
/// every settle (docs/defects/
/// 2026-10-02-saved-records-sharing-a-device-id-misroute-the-board.md).
pub fn taken_device_titles(devices: &[DeviceView], naming: DeviceId) -> Vec<String> {
    devices
        .iter()
        .filter(|device| device.id != naming)
        .map(|device| device.title.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use lpa_devices::device::DeviceStatus;

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

    /// O5: a new board is named after its model, with no date; a second of
    /// the same model gets a number.
    #[test]
    fn derived_names_are_the_model_and_dodge_collisions() {
        assert_eq!(
            derive_flash_name("Seeed XIAO ESP32-C6", &[]),
            "Seeed XIAO ESP32-C6"
        );
        let taken = vec![
            "Seeed XIAO ESP32-C6".to_string(),
            "Seeed XIAO ESP32-C6 2".to_string(),
        ];
        assert_eq!(
            derive_flash_name("Seeed XIAO ESP32-C6", &taken),
            "Seeed XIAO ESP32-C6 3"
        );
    }

    /// Deriving a name for a card that already wears it gives the same name
    /// back: the card's own title is not a collision.
    #[test]
    fn naming_a_card_again_is_a_fixed_point() {
        let base = "Seeed XIAO ESP32-C6".to_string();
        let mut named = running_view(None, None, FirmwareFace::Unknown);
        named.title = base.clone();
        let mut neighbour = running_view(None, None, FirmwareFace::Unknown);
        neighbour.id = DeviceId(8);
        neighbour.title = "Porch sign".to_string();
        let cards = [named.clone(), neighbour];

        let taken = taken_device_titles(&cards, named.id);
        assert_eq!(taken, vec!["Porch sign".to_string()]);
        let again = derive_flash_name("Seeed XIAO ESP32-C6", &taken);
        assert_eq!(again, base, "a card never collides with itself");
        assert_eq!(
            derive_flash_name(
                "Seeed XIAO ESP32-C6",
                &taken_device_titles(&cards, DeviceId(9))
            ),
            "Seeed XIAO ESP32-C6 2",
            "another card still dodges it"
        );
    }

    /// Nobody picks a board first: a chip already heard gets its build, and
    /// a board this board already named rides along when the catalog has it
    /// for the chip.
    #[test]
    fn the_target_is_the_chips_build_and_the_board_it_named() {
        let target = flash_target(Some("esp32c6"), Some("seeed/xiao-esp32-c6"), None).unwrap();
        assert_eq!(
            target.build,
            FlashBuildChoice::Fixed("esp32c6-4mb".to_string())
        );
        assert_eq!(target.board_id.as_deref(), Some("seeed/xiao-esp32-c6"));
        assert!(target.park_first, "a native-USB chip parks first");

        // A board of another chip is not this board's model.
        let mismatched = flash_target(Some("esp32c6"), Some("quinled/dig-uno"), None).unwrap();
        assert_eq!(mismatched.board_id, None);
        let classic = flash_target(Some("esp32"), Some("quinled/dig-uno"), None).unwrap();
        assert_eq!(classic.board_id.as_deref(), Some("quinled/dig-uno"));
        assert!(!classic.park_first, "a classic behind a bridge never parks");
    }

    /// The case that used to show every board in the catalog: no chip heard
    /// at all. It installs on one press; the bridge says whether to park.
    #[test]
    fn an_unknown_chip_still_installs_and_its_bridge_decides_the_park() {
        let native = UsbIds {
            vendor: 0x303A,
            product: 0x1001,
        };
        let ch340 = UsbIds {
            vendor: 0x1A86,
            product: 0x7523,
        };
        let target = flash_target(None, None, Some(native)).unwrap();
        let FlashBuildChoice::ByChip(rows) = &target.build else {
            panic!("the chip table: {target:?}");
        };
        assert!(
            rows.iter()
                .any(|row| row.chip == "esp32c6" && row.build_id == "esp32c6-4mb"),
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.chip == "esp32"),
            "every served chip"
        );
        assert_eq!(target.board_id, None);
        assert!(target.park_first);
        assert!(!flash_target(None, None, Some(ch340)).unwrap().park_first);
        assert!(!flash_target(None, None, None).unwrap().park_first);
        // A named board still names its chip.
        let named = flash_target(None, Some("seeed/xiao-esp32-c6"), None).unwrap();
        assert!(named.park_first);
        assert_eq!(named.board_id.as_deref(), Some("seeed/xiao-esp32-c6"));
    }

    #[test]
    fn a_known_chip_with_no_served_build_reads_honestly() {
        let refused = flash_target(Some("esp32p4"), None, None).unwrap_err();
        assert!(refused.contains("esp32p4"), "{refused}");
    }

    #[test]
    fn a_chip_reads_as_people_say_it() {
        assert_eq!(chip_display_name("esp32c6").as_deref(), Some("ESP32-C6"));
        assert_eq!(chip_display_name("esp8266"), None);
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
            heard_age_ms: None,
            identity_label: None,
            detected_chip: chip.map(str::to_string),
            board_id: board.map(str::to_string),
            firmware_face: face,
            remembered_firmware: None,
            degraded: None,
            engine_fps: None,
            link_counters: None,
            loaded_project: LoadedProject::Running {
                label: "studio".to_string(),
            },
            can_receive_project: true,
            can_remove_project: true,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            usb: None,
            held_elsewhere: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
            update_blocked: None,
            last_update_outcome: None,
        }
    }

    fn light_player(wire: lpa_devices::WireVersion) -> FirmwareFace {
        FirmwareFace::LightPlayer {
            firmware: Some("fw-esp32v3 7c80a27".to_string()),
            wire,
            age: lpa_devices::FirmwareAge::Unknown,
        }
    }

    /// The 2026-09-04 ruling: two verbs for two situations. A needs-firmware
    /// face FLASHES; a running LightPlayer UPDATES — both on one press,
    /// with no board pick (plan 2026-10-28-0025).
    #[test]
    fn a_needs_firmware_face_flashes_and_a_running_board_updates() {
        use lpa_devices::WireVersion;

        let blank = running_view(Some("esp32c6"), None, FirmwareFace::Blank);
        let verb = firmware_verb(&blank).expect("a blank chip has a verb");
        assert_eq!(verb, FirmwareVerb::Flash);
        assert_eq!(verb.label(), "Flash firmware");
        let action = verb.action_for(&blank);
        assert!(action.meta().enablement.is_enabled());
        let op = action.op_as::<DevicesOp>().expect("a devices op");
        assert!(matches!(
            op.action(),
            Action::Flash {
                board_id: None,
                build: FlashBuildChoice::Fixed(_),
                park_first: true,
                ..
            }
        ));

        // The case that used to open the pick: a classic chip several boards
        // fit, with no board named. One press now.
        let unknown = running_view(Some("esp32"), None, light_player(WireVersion::Match));
        assert!(
            flash_offer(Some("esp32")).candidates.len() > 1,
            "the case needs a chip several boards fit"
        );
        let verb = firmware_verb(&unknown).expect("an unnamed classic still has a verb");
        assert_eq!(verb, FirmwareVerb::Update);
        let action = verb.action_for(&unknown);
        assert_eq!(action.meta().label, "Update firmware");
        assert!(action.meta().enablement.is_enabled());

        // A named board rides along, to be stamped again.
        let named = running_view(
            Some("esp32"),
            Some("quinled/dig-uno"),
            light_player(WireVersion::Match),
        );
        let action = FirmwareVerb::Update.action_for(&named);
        let Action::Flash { board_id, .. } = action.op_as::<DevicesOp>().unwrap().action() else {
            panic!("a flash");
        };
        assert_eq!(board_id.as_deref(), Some("quinled/dig-uno"));
    }

    /// The line and the verb agree: an older board's line says "update
    /// recommended" and its verb says Update — never Flash beside it.
    /// G1-F1, end to end from the bytes: the hello a fielded XIAO C6 sends
    /// at wire 32 (captured off the spare board, 2026-10-02) — which a
    /// wire-33 Studio cannot decode — through the real demux and the real
    /// fold onto a card. It used to read "No LightPlayer hello — pre-hello
    /// firmware" with a Flash picker; it is older LightPlayer firmware,
    /// whose project stays, and with its board known it UPDATES in one
    /// click.
    #[test]
    fn a_wire_32_board_is_older_light_player_firmware_that_updates() {
        use lpa_devices::event::Input;
        use lpa_devices::link::LinkId;
        use lpa_devices::replay::{Replay, Step};
        use lpa_devices::time::Millis;

        let hello =
            include_str!("../../../../../lp-core/lpc-wire/testdata/hello-proto32-xiao-c6.json");
        let mut replay = Replay::new(lpa_link::device_link::wire::roster_config());
        replay.step(Millis(0), Step::attach(1, "usb-spare"));
        replay.step(Millis(20), Step::opened(1));
        replay.step(Millis(40), Step::line(1, "ESP-ROM:esp32c6-20220919"));
        // The board's heartbeats name it (its MAC), as a wire-32 board's do.
        let heartbeat = || Step::Heartbeat {
            link: 1,
            uid: None,
            mac: Some("10:bd:a3:b0:8e:30".to_string()),
            fps: None,
        };
        replay.step(Millis(60), heartbeat());
        let event = lpa_link::device_link::demux::demux_line(&format!("M!{}", hello.trim()));
        replay.feed(Millis(200), Input::link(LinkId(1), event));
        replay.step(Millis(1_200), heartbeat());

        let view = replay.view();
        let card = view.devices.first().expect("the board has a card");
        assert_eq!(
            card.firmware_face,
            FirmwareFace::OlderLightPlayer { proto: Some(32) },
            "{card:?}"
        );
        assert_eq!(
            super::super::device_firmware_face::device_firmware_line(&card.firmware_face, None),
            "Older LightPlayer firmware — flash to update; the project stays"
        );

        // The board's own record names it (Studio stamped it at its last
        // flash): one click, "Update firmware".
        let mut known = card.clone();
        known.board_id = Some("seeed/xiao-esp32-c6".to_string());
        let verb = firmware_verb(&known).expect("an older board has a verb");
        assert_eq!(verb, FirmwareVerb::Update);
        assert_eq!(verb.label(), "Update firmware");

        // Nothing names it: still one press — the session finds the chip.
        let mut unknown = card.clone();
        unknown.board_id = None;
        unknown.detected_chip = None;
        assert_eq!(firmware_verb(&unknown), Some(FirmwareVerb::Update));
        assert!(
            FirmwareVerb::Update
                .action_for(&unknown)
                .meta()
                .enablement
                .is_enabled()
        );
    }

    /// G1 walk (2026-10-03), Yona: "it really shouldn't say 8 boards fit,
    /// right? because we know it's a C6? ideally we'd know what board it
    /// is." A fresh browser (no record), attached mid-stream (no boot
    /// banner, so no chip off it): the wire-32 hello itself names the board
    /// Studio stamped, so the card knows the board — and through it the
    /// chip — and offers Update firmware for it in one click, with no pick.
    /// Its identity line no longer says "no firmware" beside "Older
    /// LightPlayer firmware".
    #[test]
    fn a_wire_32_board_names_its_board_and_updates_without_the_pick() {
        use super::super::device_identity::device_identity_line;
        use lpa_devices::event::Input;
        use lpa_devices::link::LinkId;
        use lpa_devices::replay::{Replay, Step};
        use lpa_devices::time::Millis;

        let hello =
            include_str!("../../../../../lp-core/lpc-wire/testdata/hello-proto32-xiao-c6.json");
        let mut replay = Replay::new(lpa_link::device_link::wire::roster_config());
        replay.step(Millis(0), Step::attach(1, "usb-spare"));
        replay.step(Millis(20), Step::opened(1));
        let heartbeat = || Step::Heartbeat {
            link: 1,
            uid: None,
            mac: Some("10:bd:a3:b0:8e:30".to_string()),
            fps: None,
        };
        replay.step(Millis(60), heartbeat());
        let event = lpa_link::device_link::demux::demux_line(&format!("M!{}", hello.trim()));
        replay.feed(Millis(200), Input::link(LinkId(1), event));
        replay.step(Millis(1_200), heartbeat());

        let view = replay.view();
        let card = view.devices.first().expect("the board has a card");
        assert_eq!(
            card.firmware_face,
            FirmwareFace::OlderLightPlayer { proto: Some(32) }
        );
        assert_eq!(card.detected_chip, None, "the premise: no banner was heard");
        assert_eq!(
            card.board_id.as_deref(),
            Some("seeed/xiao-esp32-c6"),
            "the hello named its board"
        );
        assert_eq!(firmware_verb(card), Some(FirmwareVerb::Update));
        let action = FirmwareVerb::Update.action_for(card);
        let Action::Flash { board_id, .. } = action.op_as::<DevicesOp>().unwrap().action() else {
            panic!("a flash");
        };
        assert_eq!(board_id.as_deref(), Some("seeed/xiao-esp32-c6"));
        let identity = device_identity_line(card).display();
        assert!(!identity.contains("no firmware"), "{identity}");
        assert!(identity.contains("XIAO ESP32-C6"), "{identity}");
    }

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
    /// or no wire.
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
                layout: None,
                replace: None,
                flash_step: None,
                update: None,
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

        // No chip, no board: the session finds the chip; the verb stands.
        let chipless = running_view(None, None, light_player(WireVersion::Match));
        assert_eq!(firmware_verb(&chipless), Some(FirmwareVerb::Update));

        // A blank chip needs no wire fact beyond its face.
        let blank_unlinked = DeviceView {
            escapes: vec![Escape::Forget],
            ..running_view(Some("esp32c6"), None, FirmwareFace::Blank)
        };
        assert_eq!(firmware_verb(&blank_unlinked), Some(FirmwareVerb::Flash));
    }
}
