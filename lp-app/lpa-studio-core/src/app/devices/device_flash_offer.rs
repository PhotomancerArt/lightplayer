//! `devices/<board>/flash` and `devices/<board>/update-firmware`: the
//! firmware verbs as offers that take their board.
//!
//! The board pick has always been core's decision ([`flash_offer`]: which
//! boards a detected chip fits, which one to preselect, which served build
//! and which connect dance each resolves to); only the PICKED value was the
//! web's. Here the pick becomes the offer's `board` parameter, with the same
//! candidates as its options, and the binder resolves the `build_id` and
//! `park_first` the chosen [`FlashBoardChoice`] already carries — nothing is
//! decided twice.
//!
//! Three verbs share the pick:
//!
//! - **Flash** on a card's needs-firmware face ([`flash_device_offer`]) and
//!   on a pending link that settled on one — a fresh blank chip
//!   ([`flash_pending_offer`], `devices/new-<n>/flash`). The optional
//!   `name` is the setup field's: blank leaves the derived
//!   "<board> · <Mon D>", minted at flash time by the controller exactly as
//!   for the picker.
//! - **Update firmware** on a running LightPlayer ([`update_firmware_offer`]):
//!   one click when its board resolved, the same pick (no name — the board
//!   has one) when it did not.
//!
//! **The chip filter and its escape.** A detected chip narrows the board
//! list; the picker's "show all" escape is the `all_boards` toggle, off by
//! default. The boards outside the chip are listed after the fits, each
//! [`OfferChoice::only_with`] the toggle, so a renderer draws them only
//! while it is on and a press that picks one with it off is refused. The
//! filter is convenience; the flash preflight's chip guard (it checks the
//! pick against the silicon before anything is written) is the safety, and
//! it runs whatever is picked. With no chip known the list was never
//! narrowed, so there is no toggle.
//!
//! **The level depends on what is on the chip (Q3).** Flashing a blank
//! board loses nothing, so it is Routine: one click, and the agent may
//! press it. Over anything else — somebody's firmware, an older
//! LightPlayer, a chip that will not say — it is the Flash op's own
//! Lasting, with its copy. Update keeps the Flash op's Lasting (M1).

use std::rc::Rc;

use lpa_devices::Action;
use lpa_devices::identity::DeviceId;
use lpa_devices::view::{DeviceView, FirmwareFace, PendingLinkView};

use super::device_flash::{FirmwareVerb, FlashBoardChoice, firmware_verb, flash_offer};
use super::device_identity::device_chip;
use super::devices_op::DevicesOp;
use crate::{
    ActionConsequence, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath,
    UiAction, UiOffer,
};

/// The firmware verbs' board parameter.
pub const FLASH_BOARD_PARAM: &str = "board";
/// The Flash offer's optional name parameter.
pub const FLASH_NAME_PARAM: &str = "name";
/// The firmware verbs' "show all" escape from the chip filter.
pub const FLASH_ALL_BOARDS_PARAM: &str = "all_boards";

/// What an empty name field means.
const NAME_PLACEHOLDER: &str = "Leave blank to name it after the board and today's date";

/// `<prefix>/flash` for a device whose firmware verb is Flash, or `None`
/// when it offers no Flash (a running LightPlayer updates instead; a busy
/// or unsettled card offers no firmware verb at all).
///
/// When the verb cannot be pressed whatever is picked — the link cannot
/// carry firmware (Bluetooth), or this Studio serves no build for the chip
/// — the offer is published disabled with that reason and no parameters:
/// there is nothing to choose.
pub fn flash_device_offer(view: &DeviceView, prefix: OfferPath) -> Option<UiOffer> {
    if firmware_verb(view)? != FirmwareVerb::Flash {
        return None;
    }
    Some(flash_at(
        prefix.child("flash"),
        view.id,
        &view.firmware_face,
        view.firmware_blocked.as_deref(),
        device_chip(view),
    ))
}

/// `<prefix>/flash` for a pending link that settled on a needs-firmware
/// verdict — a fresh blank chip — or `None` while it has not (an unsettled
/// link projects [`FirmwareFace::Unknown`], so the verb never appears
/// mid-identification). The Flash targets the link's provisional device,
/// and pressing it adopts the link. Its only chip fact is the ROM boot
/// banner.
pub fn flash_pending_offer(pending: &PendingLinkView, prefix: OfferPath) -> Option<UiOffer> {
    if !pending.needs_firmware() {
        return None;
    }
    Some(flash_at(
        prefix.child("flash"),
        pending.device,
        &pending.firmware_face,
        pending.firmware_blocked.as_deref(),
        pending.detected_chip.clone(),
    ))
}

/// `<prefix>/update-firmware` for a running LightPlayer, or `None` when the
/// card offers no Update: one click when its board resolved
/// ([`FirmwareVerb::Update`]), the board pick when it did not
/// ([`FirmwareVerb::UpdatePick`]), and disabled with the reason over a link
/// that cannot carry firmware.
pub fn update_firmware_offer(view: &DeviceView, prefix: OfferPath) -> Option<UiOffer> {
    let verb = firmware_verb(view)?;
    if verb == FirmwareVerb::Flash {
        return None;
    }
    let path = prefix.child("update-firmware");
    if let Some(reason) = &view.firmware_blocked {
        return Some(UiOffer::new(
            path,
            "download",
            verb.blocked_action(view.id, reason),
        ));
    }
    if let Some(action) = verb.update_action(view.id) {
        return Some(UiOffer::new(path, "download", action));
    }
    let dress = Dress {
        label: Some(verb.label().to_string()),
        summary: Some(verb.summary()),
        consequence: None,
    };
    Some(board_pick_offer(
        path,
        "download",
        view.id,
        device_chip(view),
        false,
        dress,
    ))
}

/// What flashing over `face` costs: nothing on a blank chip, the Flash op's
/// own Lasting everywhere else.
pub fn flash_consequence(face: &FirmwareFace) -> ActionConsequence {
    match face {
        FirmwareFace::Blank => ActionConsequence::Routine,
        _ => flash_action(DeviceId(0), None, None, &Dress::default())
            .meta()
            .consequence
            .clone(),
    }
}

/// The Flash verb at `path` for `device`: disabled with `blocked` when the
/// link cannot carry firmware, else the board pick (with the name field).
fn flash_at(
    path: OfferPath,
    device: DeviceId,
    face: &FirmwareFace,
    blocked: Option<&str>,
    chip: Option<String>,
) -> UiOffer {
    let dress = Dress {
        consequence: Some(flash_consequence(face)),
        ..Dress::default()
    };
    if let Some(reason) = blocked {
        let template = flash_action(device, None, None, &dress);
        return UiOffer::new(path, "flash", template.disabled(reason));
    }
    board_pick_offer(path, "flash", device, chip, true, dress)
}

/// The board pick both firmware verbs take: the chip's fits (preselected
/// when there is exactly one), then — when a chip narrowed the list — every
/// other served board behind the `all_boards` toggle, and the optional name
/// when `with_name`. Disabled with core's reason when this Studio serves
/// nothing for the chip.
fn board_pick_offer(
    path: OfferPath,
    icon: &str,
    device: DeviceId,
    chip: Option<String>,
    with_name: bool,
    dress: Dress,
) -> UiOffer {
    let template = flash_action(device, None, None, &dress);
    let narrowed = flash_offer(chip.as_deref());
    if let Some(reason) = narrowed.unavailable {
        return UiOffer::new(path, icon, template.disabled(reason));
    }
    let mut candidates = narrowed.candidates;
    let mut options: Vec<OfferChoice> = candidates.iter().map(board_option).collect();
    if chip.is_some() {
        for wider in flash_offer(None).candidates {
            if candidates.iter().any(|fit| fit.board_id == wider.board_id) {
                continue;
            }
            options.push(board_option(&wider).only_with(FLASH_ALL_BOARDS_PARAM));
            candidates.push(wider);
        }
    }
    let mut params = vec![OfferParam::choice(
        FLASH_BOARD_PARAM,
        "board",
        options,
        narrowed.preselect,
    )];
    if with_name {
        params.push(OfferParam::text(FLASH_NAME_PARAM, "name", NAME_PLACEHOLDER).optional());
    }
    if chip.is_some() {
        params.push(OfferParam::toggle(
            FLASH_ALL_BOARDS_PARAM,
            "every served board",
            false,
        ));
    }
    let candidates = Rc::new(candidates);
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let board = args.choice(FLASH_BOARD_PARAM).unwrap_or_default();
        let choice = candidates
            .iter()
            .find(|choice| choice.board_id == board)
            .ok_or_else(|| OfferArgError::NotAnOption {
                name: FLASH_BOARD_PARAM.to_string(),
                value: board.to_string(),
                options: candidates
                    .iter()
                    .map(|choice| choice.board_id.clone())
                    .collect(),
            })?;
        let name = if with_name {
            args.text(FLASH_NAME_PARAM)
        } else {
            None
        };
        Ok(flash_action(device, Some(choice), name, &dress))
    });
    UiOffer::with_params(path, icon, params, binder, template)
}

/// One board as a choice: its id, its display name, and the blurb line.
fn board_option(choice: &FlashBoardChoice) -> OfferChoice {
    OfferChoice::new(&choice.board_id, &choice.title).with_detail(&choice.blurb)
}

/// How a firmware verb wears the Flash op: the Update verbs' own label and
/// summary, and the level a Flash takes from what is on the chip.
#[derive(Clone, Debug, Default)]
struct Dress {
    label: Option<String>,
    summary: Option<String>,
    consequence: Option<ActionConsequence>,
}

/// The Flash op for `device`, with `choice`'s board and build (empty for the
/// unbound template), wearing `dress`.
fn flash_action(
    device: DeviceId,
    choice: Option<&FlashBoardChoice>,
    name: Option<&str>,
    dress: &Dress,
) -> UiAction {
    let mut action = DevicesOp::action_for(Action::Flash {
        device,
        board_id: choice.map(|c| c.board_id.clone()).unwrap_or_default(),
        build_id: choice.map(|c| c.build_id.clone()).unwrap_or_default(),
        park_first: choice.is_some_and(|c| c.park_first),
        name: name.map(str::to_string),
        restore_backup: false,
    });
    if let Some(label) = &dress.label {
        action = action.with_label(label.clone());
    }
    if let Some(summary) = &dress.summary {
        action = action.with_summary(summary.clone());
    }
    if let Some(consequence) = &dress.consequence {
        action = action.with_consequence(consequence.clone());
    }
    action
}

#[cfg(test)]
mod tests {
    use lpa_devices::DeviceId;
    use lpa_devices::device::DeviceStatus;
    use lpa_devices::view::{Escape, LoadedProject};

    use super::*;
    use crate::flash_offer;

    #[test]
    fn a_blank_boards_flash_lists_its_boards_preselects_and_binds() {
        let view = needs_firmware(FirmwareFace::Blank, Some("esp32c6"));
        let offer = flash_device_offer(&view, prefix()).expect("a blank board flashes");
        let candidates = flash_offer(Some("esp32c6"));

        assert_eq!(offer.path.to_string(), "devices/mac-a0f26287b48c/flash");
        assert!(offer.consequence().is_routine(), "nothing on it to lose");
        let [board, name, all_boards] = offer.params() else {
            panic!("board + name + all_boards: {:?}", offer.params());
        };
        assert_eq!(board.name, "board");
        let crate::OfferParamKind::Choice { options, preselect } = &board.kind else {
            panic!("board is a choice: {board:?}");
        };
        assert_eq!(
            options
                .iter()
                .filter(|o| o.only_with.is_none())
                .map(|o| o.value.clone())
                .collect::<Vec<_>>(),
            candidates
                .candidates
                .iter()
                .map(|c| c.board_id.clone())
                .collect::<Vec<_>>(),
            "the narrowed options are the core pick's candidates, first"
        );
        assert_eq!(
            options.len(),
            flash_offer(None).candidates.len(),
            "and every other served board waits behind the toggle"
        );
        assert!(
            options
                .iter()
                .skip(candidates.candidates.len())
                .all(|o| o.only_with.as_deref() == Some("all_boards")),
            "{options:?}"
        );
        assert_eq!(preselect, &candidates.preselect);
        assert!(!name.is_required(), "the name is optional");
        assert_eq!(
            all_boards.kind,
            crate::OfferParamKind::Toggle { value: false },
            "show all is off by default"
        );

        let first = &candidates.candidates[0];
        let bound = offer
            .press(
                &OfferArgs::new()
                    .with("board", &first.board_id)
                    .with("name", "Desk"),
            )
            .expect("a candidate binds");
        let op = bound.op_as::<DevicesOp>().expect("a devices op");
        assert_eq!(
            op.action(),
            &Action::Flash {
                device: DeviceId(7),
                board_id: first.board_id.clone(),
                build_id: first.build_id.clone(),
                park_first: first.park_first,
                name: Some("Desk".to_string()),
                restore_backup: false,
            },
            "build and connect dance come from the core choice"
        );
        assert!(bound.meta().consequence.is_routine());

        match &candidates.preselect {
            Some(preselect) => {
                assert!(offer.is_enabled(), "a preselect binds the default press");
                let default = offer.press(&OfferArgs::new()).expect("the preselect binds");
                let Action::Flash { board_id, name, .. } =
                    default.op_as::<DevicesOp>().unwrap().action()
                else {
                    panic!("a flash");
                };
                assert_eq!(board_id, preselect);
                assert_eq!(
                    name, &None,
                    "blank leaves the derived name to the controller"
                );
            }
            None => assert!(!offer.is_enabled(), "nothing chosen yet"),
        }
    }

    #[test]
    fn an_unknown_board_is_refused() {
        let view = needs_firmware(FirmwareFace::Blank, Some("esp32c6"));
        let offer = flash_device_offer(&view, prefix()).unwrap();
        let refused = offer
            .press(&OfferArgs::new().with("board", "esp8266-nodemcu"))
            .unwrap_err();
        assert!(
            matches!(&refused, OfferArgError::NotAnOption { name, .. } if name == "board"),
            "{refused:?}"
        );
        assert!(
            refused
                .to_string()
                .contains("`esp8266-nodemcu` is not one of them")
        );
    }

    #[test]
    fn flashing_over_firmware_is_lasting() {
        for face in [
            FirmwareFace::Foreign { label: None },
            FirmwareFace::OlderLightPlayer { proto: Some(3) },
            FirmwareFace::Bootloader,
            FirmwareFace::Silent,
            FirmwareFace::NoHello,
        ] {
            let view = needs_firmware(face.clone(), Some("esp32c6"));
            let offer = flash_device_offer(&view, prefix()).expect("it flashes");
            let copy = offer
                .consequence()
                .copy()
                .unwrap_or_else(|| panic!("{face:?} is Lasting"));
            assert_eq!(
                copy.title, "Replace what this board runs?",
                "the op's own copy"
            );
            let bound = offer
                .press(&OfferArgs::new().with("board", offer_first_board(&offer)))
                .unwrap();
            assert!(
                bound.meta().consequence.arms(),
                "{face:?}: bound Lasting too"
            );
        }
    }

    #[test]
    fn with_no_chip_nothing_is_preselected_and_the_verb_asks_for_a_board() {
        let view = needs_firmware(FirmwareFace::Blank, None);
        let offer = flash_device_offer(&view, prefix()).unwrap();
        assert!(!offer.is_enabled());
        assert_eq!(
            offer.action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: "choose a board".to_string()
            }
        );
        assert_eq!(offer.label(), "Flash firmware");
        assert!(
            offer.consequence().is_routine(),
            "the level reads before a choice"
        );
    }

    #[test]
    fn a_link_that_cannot_carry_firmware_offers_a_disabled_flash_with_no_choice() {
        let mut view = needs_firmware(FirmwareFace::Blank, Some("esp32c6"));
        view.firmware_blocked = Some("Firmware updates need USB.".to_string());
        let offer = flash_device_offer(&view, prefix()).unwrap();
        assert!(offer.params().is_empty());
        assert_eq!(
            offer.press(&OfferArgs::new()),
            Err(OfferArgError::Unavailable {
                reason: "Firmware updates need USB.".to_string()
            })
        );
    }

    #[test]
    fn a_running_lightplayer_offers_no_flash() {
        let mut view = needs_firmware(
            FirmwareFace::LightPlayer {
                firmware: None,
                wire: lpa_devices::WireVersion::Match,
                age: lpa_devices::FirmwareAge::Unknown,
            },
            Some("esp32c6"),
        );
        view.board_id = Some(flash_offer(Some("esp32c6")).candidates[0].board_id.clone());
        assert_eq!(
            flash_device_offer(&view, prefix()),
            None,
            "it updates instead"
        );
    }

    #[test]
    fn show_all_widens_the_pick_past_the_chip_and_the_preflight_still_guards() {
        let view = needs_firmware(FirmwareFace::Blank, Some("esp32c6"));
        let offer = flash_device_offer(&view, prefix()).unwrap();
        let fits = flash_offer(Some("esp32c6")).candidates;
        let outside = flash_offer(None)
            .candidates
            .into_iter()
            .find(|choice| !fits.iter().any(|fit| fit.board_id == choice.board_id))
            .expect("this build serves a board for another chip");

        let refused = offer
            .press(&OfferArgs::new().with("board", &outside.board_id))
            .unwrap_err();
        assert!(
            refused.to_string().contains("only with `all_boards` on"),
            "{refused}"
        );
        let bound = offer
            .press(
                &OfferArgs::new()
                    .with("board", &outside.board_id)
                    .with("all_boards", "true"),
            )
            .expect("show all widens the choice");
        let Action::Flash {
            board_id, build_id, ..
        } = bound.op_as::<DevicesOp>().unwrap().action()
        else {
            panic!("a flash");
        };
        assert_eq!(board_id, &outside.board_id);
        assert_eq!(build_id, &outside.build_id, "the wider pick's own build");
    }

    #[test]
    fn with_no_chip_the_list_was_never_narrowed_so_there_is_no_toggle() {
        let view = needs_firmware(FirmwareFace::Blank, None);
        let offer = flash_device_offer(&view, prefix()).unwrap();
        assert_eq!(
            offer
                .params()
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>(),
            ["board", "name"]
        );
    }

    #[test]
    fn a_blank_pending_chip_flashes_at_its_provisional_ref() {
        let pending = PendingLinkView {
            link: lpa_devices::LinkId(4),
            device: DeviceId(3),
            title: "New device".to_string(),
            state_label: "New device found — blank flash".to_string(),
            detail: None,
            can_adopt: true,
            firmware_face: FirmwareFace::Blank,
            detected_chip: Some("esp32c6".to_string()),
            mac: None,
            firmware_blocked: None,
            escapes: vec![Escape::Forget],
        };
        let prefix = OfferPath::board(&crate::BoardRef::New(3));
        let offer = flash_pending_offer(&pending, prefix.clone()).expect("a blank chip flashes");
        assert_eq!(offer.path.to_string(), "devices/new-3/flash");
        assert!(
            offer.consequence().is_routine(),
            "a blank chip loses nothing"
        );
        let bound = offer
            .press(&OfferArgs::new().with("board", offer_first_board(&offer)))
            .unwrap();
        assert!(matches!(
            bound.op_as::<DevicesOp>().unwrap().action(),
            Action::Flash {
                device: DeviceId(3),
                ..
            }
        ));

        let identifying = PendingLinkView {
            firmware_face: FirmwareFace::Unknown,
            ..pending
        };
        assert_eq!(
            flash_pending_offer(&identifying, prefix),
            None,
            "no verb mid-identification"
        );
    }

    #[test]
    fn a_running_board_updates_in_one_click_when_its_board_resolved() {
        let mut view = running(Some("esp32c6"));
        let choice = flash_offer(Some("esp32c6")).candidates[0].clone();
        view.board_id = Some(choice.board_id.clone());
        let offer = update_firmware_offer(&view, prefix()).expect("it updates");

        assert_eq!(
            offer.path.to_string(),
            "devices/mac-a0f26287b48c/update-firmware"
        );
        assert!(offer.params().is_empty(), "one click");
        assert_eq!(offer.label(), "Update firmware");
        assert!(offer.consequence().arms(), "the Flash op's own Lasting");
        let Action::Flash { board_id, name, .. } =
            offer.action.op_as::<DevicesOp>().unwrap().action()
        else {
            panic!("a flash");
        };
        assert_eq!(board_id, &choice.board_id);
        assert_eq!(name, &None, "an update never renames");
        assert_eq!(flash_device_offer(&view, prefix()), None);

        view.firmware_blocked = Some("Firmware updates need USB.".to_string());
        let blocked = update_firmware_offer(&view, prefix()).unwrap();
        assert_eq!(
            blocked.press(&OfferArgs::new()),
            Err(OfferArgError::Unavailable {
                reason: "Firmware updates need USB.".to_string()
            })
        );
        assert_eq!(blocked.label(), "Update firmware");
    }

    #[test]
    fn an_update_whose_board_did_not_resolve_takes_the_pick_without_a_name() {
        // A chip several served boards fit, and no board on record.
        let chip = ["esp32", "esp32c6", "esp32s3"]
            .into_iter()
            .find(|chip| flash_offer(Some(chip)).candidates.len() > 1);
        let Some(chip) = chip else {
            // Every served chip has one board in this build: the pick never
            // appears on a running board, which the verb itself says.
            assert!(matches!(
                firmware_verb(&running(Some("esp32c6"))),
                Some(FirmwareVerb::Update(_))
            ));
            return;
        };
        let view = running(Some(chip));
        assert_eq!(firmware_verb(&view), Some(FirmwareVerb::UpdatePick));
        let offer = update_firmware_offer(&view, prefix()).unwrap();
        assert_eq!(
            offer
                .params()
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>(),
            ["board", "all_boards"],
            "an update names nothing"
        );
        assert_eq!(offer.label(), "Update firmware");
        assert!(!offer.is_enabled(), "several fit: choose a board");
        let bound = offer
            .press(&OfferArgs::new().with("board", offer_first_board(&offer)))
            .unwrap();
        assert_eq!(bound.meta().label, "Update firmware");
        assert!(bound.meta().consequence.arms());
    }

    fn running(chip: Option<&str>) -> DeviceView {
        DeviceView {
            status: DeviceStatus::Ready,
            ..needs_firmware(
                FirmwareFace::LightPlayer {
                    firmware: None,
                    wire: lpa_devices::WireVersion::Match,
                    age: lpa_devices::FirmwareAge::Unknown,
                },
                chip,
            )
        }
    }

    fn offer_first_board(offer: &UiOffer) -> String {
        match &offer.params()[0].kind {
            crate::OfferParamKind::Choice { options, .. } => options[0].value.clone(),
            other => panic!("{other:?}"),
        }
    }

    fn prefix() -> OfferPath {
        OfferPath::board(&crate::BoardRef::Mac(
            lpa_devices::BoardKey::parse("a0:f2:62:87:b4:8c").unwrap(),
        ))
    }

    /// A linked card on a needs-firmware face (or any face, for the
    /// negative cases): the link is up, nothing is running on it.
    fn needs_firmware(face: FirmwareFace, chip: Option<&str>) -> DeviceView {
        DeviceView {
            id: DeviceId(7),
            title: "Desk C6".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Blank flash — needs firmware".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: chip.map(str::to_string),
            board_id: None,
            firmware_face: face,
            remembered_firmware: None,
            degraded: None,
            engine_fps: None,
            link_counters: None,
            loaded_project: LoadedProject::Unknown,
            can_receive_project: false,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
            update_blocked: None,
        }
    }
}
