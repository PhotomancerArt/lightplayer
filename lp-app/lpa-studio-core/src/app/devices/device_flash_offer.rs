//! `devices/<board>/flash`: the needs-firmware face's Flash as an offer
//! that takes its board.
//!
//! The board pick has always been core's decision ([`flash_offer_for`]:
//! which boards a detected chip fits, which one to preselect, which served
//! build and which connect dance each resolves to); only the PICKED value
//! was the web's. Here the pick becomes the offer's `board` parameter, with
//! the same candidates as its options, and the binder resolves the
//! `build_id` and `park_first` the chosen [`FlashBoardChoice`] already
//! carries — nothing is decided twice.
//!
//! The optional `name` is the setup field's: blank leaves the derived
//! "<board> · <Mon D>", minted at flash time by the controller exactly as
//! for the picker.
//!
//! **The level depends on what is on the chip (Q3).** Flashing a blank
//! board loses nothing, so it is Routine: one click, and the agent may
//! press it. Over anything else — somebody's firmware, an older
//! LightPlayer, a chip that will not say — it is the Flash op's own
//! Lasting, with its copy.

use lpa_devices::Action;
use lpa_devices::view::{DeviceView, FirmwareFace};

use super::device_flash::{FirmwareVerb, FlashBoardChoice, firmware_verb, flash_offer_for};
use super::devices_op::DevicesOp;
use crate::{
    ActionConsequence, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath,
    UiAction, UiOffer,
};

/// The Flash offer's board parameter.
pub const FLASH_BOARD_PARAM: &str = "board";
/// The Flash offer's optional name parameter.
pub const FLASH_NAME_PARAM: &str = "name";

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
    let path = prefix.child("flash");
    let device = view.id;
    let consequence = flash_consequence(&view.firmware_face);
    let template = flash_action(device, None, None, Some(consequence.clone()));
    if let Some(reason) = &view.firmware_blocked {
        return Some(UiOffer::new(path, "flash", template.disabled(reason)));
    }
    let offer = flash_offer_for(view);
    if let Some(reason) = offer.unavailable {
        return Some(UiOffer::new(path, "flash", template.disabled(reason)));
    }
    let options = offer
        .candidates
        .iter()
        .map(|choice| OfferChoice::new(&choice.board_id, &choice.title).with_detail(&choice.blurb))
        .collect();
    let params = vec![
        OfferParam::choice(FLASH_BOARD_PARAM, "board", options, offer.preselect),
        OfferParam::text(FLASH_NAME_PARAM, "name", NAME_PLACEHOLDER).optional(),
    ];
    let candidates = offer.candidates;
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
        Ok(flash_action(
            device,
            Some(choice),
            args.text(FLASH_NAME_PARAM),
            Some(consequence.clone()),
        ))
    });
    Some(UiOffer::with_params(
        path, "flash", params, binder, template,
    ))
}

/// What flashing over `face` costs: nothing on a blank chip, the Flash op's
/// own Lasting everywhere else.
pub fn flash_consequence(face: &FirmwareFace) -> ActionConsequence {
    match face {
        FirmwareFace::Blank => ActionConsequence::Routine,
        _ => flash_action(lpa_devices::DeviceId(0), None, None, None)
            .meta()
            .consequence
            .clone(),
    }
}

/// The Flash op for `device`, with `choice`'s board and build (empty for the
/// unbound template), wearing `consequence` when one is given.
fn flash_action(
    device: lpa_devices::DeviceId,
    choice: Option<&FlashBoardChoice>,
    name: Option<&str>,
    consequence: Option<ActionConsequence>,
) -> UiAction {
    let action = DevicesOp::action_for(Action::Flash {
        device,
        board_id: choice.map(|c| c.board_id.clone()).unwrap_or_default(),
        build_id: choice.map(|c| c.build_id.clone()).unwrap_or_default(),
        park_first: choice.is_some_and(|c| c.park_first),
        name: name.map(str::to_string),
    });
    match consequence {
        Some(consequence) => action.with_consequence(consequence),
        None => action,
    }
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
        let [board, name] = offer.params() else {
            panic!("board + name: {:?}", offer.params());
        };
        assert_eq!(board.name, "board");
        let crate::OfferParamKind::Choice { options, preselect } = &board.kind else {
            panic!("board is a choice: {board:?}");
        };
        assert_eq!(
            options.iter().map(|o| o.value.clone()).collect::<Vec<_>>(),
            candidates
                .candidates
                .iter()
                .map(|c| c.board_id.clone())
                .collect::<Vec<_>>(),
            "the options are the core pick's candidates"
        );
        assert_eq!(preselect, &candidates.preselect);
        assert!(!name.is_required(), "the name is optional");

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
        }
    }
}
