//! `devices/<board>/flash`, `devices/<board>/update-firmware` and
//! `devices/<board>/set-board`: the firmware verbs as offers.
//!
//! **One press, no board pick** (plan
//! `lp2025/2026-10-28-0025-install-without-board-pick`). Flash and Update
//! used to take a `board` parameter — the user had to say which board this
//! was before anything could be installed, from a list that was often not
//! even narrowed to the chip, because the chip had not been heard yet. Now
//! the press carries the chip table ([`FlashTarget`]); the bootloader
//! session picks the build by the chip its SYNC names, reads what the board
//! runs, and guesses the board from a fingerprint. Only two things are still
//! asked, and neither before the press:
//!
//! - **Replacing somebody else's firmware** — inside the activity, once the
//!   board has been read, with words that name what is there
//!   (`devices/<board>/confirm-replace`, `device_replace_view`). Lasting.
//! - **Which board is this?** — [`set_board_offer`], afterwards, when the
//!   fingerprint fit several catalog boards (or any time, to correct it).
//!   Routine: the board model is a file on the board, written again on the
//!   next answer.
//!
//! **The level.** Flash is Routine at the press: nothing is written before
//! the board has been read, and the one thing that loses something is
//! asked inside. An older LightPlayer's update loses nothing either (its
//! files and identity stay: `device_layout_step`), so it is Routine
//! (2026-10-17, the desk C6's dead end). A current LightPlayer's USB update
//! keeps the Flash op's Lasting — the price ADR
//! 2026-10-06-studio-updates-over-the-update-channel §4 set for it.

use std::rc::Rc;

use lpa_devices::identity::DeviceId;
use lpa_devices::view::{DeviceView, FirmwareFace, PendingLinkView};
use lpa_devices::{Action, ActivityKind};

use super::device_flash::{
    FirmwareVerb, FlashBoardChoice, FlashTarget, firmware_verb, flash_offer_for,
    pending_flash_target,
};
use super::devices_op::DevicesOp;
use crate::{
    ActionConsequence, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath,
    UiAction, UiOffer,
};

/// The Flash offer's optional name parameter.
pub const FLASH_NAME_PARAM: &str = "name";
/// What an empty name field means.
const NAME_PLACEHOLDER: &str = "Leave blank to name it after the board";

/// The set-board verb's board parameter.
pub const SET_BOARD_PARAM: &str = "board";
/// The set-board verb's "show all" escape from the fingerprint's fits.
pub const SET_BOARD_ALL_PARAM: &str = "all_boards";
/// `devices/<board>/set-board`.
pub const SET_BOARD: &str = "set-board";

/// `<prefix>/flash` for a device whose firmware verb is Flash, or `None`
/// when it offers no Flash (a running LightPlayer updates instead; a busy
/// or unsettled card offers no firmware verb at all). Disabled with the
/// reason when the link cannot carry firmware (Bluetooth) or this Studio
/// serves nothing for the chip.
pub fn flash_device_offer(view: &DeviceView, prefix: OfferPath) -> Option<UiOffer> {
    let verb = firmware_verb(view)?;
    if verb != FirmwareVerb::Flash {
        return None;
    }
    let action = match &view.firmware_blocked {
        Some(reason) => verb.blocked_action(view.id, reason),
        None => verb.action_for(view),
    };
    Some(install_offer(prefix.child("flash"), action))
}

/// `<prefix>/flash` for a pending link that settled on a needs-firmware
/// verdict — a fresh blank chip — or `None` while it has not (an unsettled
/// link projects [`FirmwareFace::Unknown`], so the verb never appears
/// mid-identification). The Flash targets the link's provisional device,
/// and pressing it adopts the link.
pub fn flash_pending_offer(pending: &PendingLinkView, prefix: OfferPath) -> Option<UiOffer> {
    if !pending.needs_firmware() {
        return None;
    }
    let verb = FirmwareVerb::Flash;
    let action = match (&pending.firmware_blocked, pending_flash_target(pending)) {
        (Some(reason), _) => verb.blocked_action(pending.device, reason),
        (None, Err(reason)) => verb.blocked_action(pending.device, &reason),
        (None, Ok(target)) => target_action(&verb, &target, pending.device),
    };
    Some(install_offer(prefix.child("flash"), action))
}

/// An install at `path`: one press (Routine — the board is read before
/// anything is written), with an optional name for the board. A disabled
/// action stays a plain, disabled offer: there is nothing to fill in.
fn install_offer(path: OfferPath, action: UiAction) -> UiOffer {
    let action = action.with_consequence(ActionConsequence::Routine);
    if !action.meta().enablement.is_enabled() {
        return UiOffer::new(path, "flash", action);
    }
    let template = action.clone();
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let mut bound = action.clone();
        if let Some(name) = args.text(FLASH_NAME_PARAM)
            && let Some(op) = bound.op_as::<DevicesOp>()
            && let Action::Flash {
                device,
                board_id,
                build,
                park_first,
                restore_backup,
                ..
            } = op.action().clone()
        {
            bound = DevicesOp::action_for(Action::Flash {
                device,
                board_id,
                build,
                park_first,
                name: Some(name.to_string()),
                restore_backup,
            })
            .with_label(action.meta().label.clone())
            .with_summary(action.meta().summary.clone())
            .with_consequence(ActionConsequence::Routine);
        }
        Ok(bound)
    });
    UiOffer::with_params(
        path,
        "flash",
        vec![OfferParam::text(FLASH_NAME_PARAM, "name", NAME_PLACEHOLDER).optional()],
        binder,
        template,
    )
}

/// `<prefix>/update-firmware` for a running LightPlayer, or `None` when the
/// card offers no Update: one press, and disabled with the reason over a
/// link that cannot carry firmware.
pub fn update_firmware_offer(view: &DeviceView, prefix: OfferPath) -> Option<UiOffer> {
    let verb = firmware_verb(view)?;
    if verb != FirmwareVerb::Update {
        return None;
    }
    let path = prefix.child("update-firmware");
    if let Some(blocked) = &view.firmware_blocked {
        // The update's own reason, when it has one
        // (`DeviceView::update_blocked`); otherwise the flash's.
        let reason = view.update_blocked.as_ref().unwrap_or(blocked);
        return Some(UiOffer::new(
            path,
            "download",
            verb.blocked_action(view.id, reason),
        ));
    }
    let action = verb.action_for(view);
    let action = match matches!(view.firmware_face, FirmwareFace::OlderLightPlayer { .. }) {
        true => action.with_consequence(ActionConsequence::Routine),
        false => action,
    };
    Some(UiOffer::new(path, "download", action))
}

/// `<prefix>/set-board` — "Which board is this?" — on a running LightPlayer
/// whose chip several catalog boards fit, or `None`. The fingerprint's
/// fits (`fits`, when an install just read them) come first and preselect
/// when there is one; every other board of the chip waits behind the
/// `all_boards` toggle. Pressing writes the board's `/hardware.json` and
/// restarts it ([`Action::SetBoard`]). Routine: the answer is a file on the
/// board, and the next answer replaces it.
pub fn set_board_offer(view: &DeviceView, prefix: OfferPath, fits: &[String]) -> Option<UiOffer> {
    let running = matches!(view.firmware_face, FirmwareFace::LightPlayer { .. })
        && view.can_receive_project
        && view.firmware_blocked.is_none();
    if !running {
        return None;
    }
    let boards = flash_offer_for(view).candidates;
    if boards.len() < 2 {
        return None;
    }
    let (narrowed, wider): (Vec<FlashBoardChoice>, Vec<FlashBoardChoice>) = boards
        .into_iter()
        .partition(|board| fits.is_empty() || fits.contains(&board.board_id));
    let mut options: Vec<OfferChoice> = narrowed.iter().map(board_option).collect();
    options.extend(
        wider
            .iter()
            .map(|board| board_option(board).only_with(SET_BOARD_ALL_PARAM)),
    );
    let preselect = match narrowed.as_slice() {
        [only] => Some(only.board_id.clone()),
        _ => view
            .board_id
            .clone()
            .filter(|board| narrowed.iter().any(|fit| &fit.board_id == board)),
    };
    let mut params = vec![OfferParam::choice(
        SET_BOARD_PARAM,
        "board",
        options,
        preselect,
    )];
    if !wider.is_empty() {
        params.push(OfferParam::toggle(
            SET_BOARD_ALL_PARAM,
            "every board of this chip",
            false,
        ));
    }
    let device = view.id;
    let template = set_board_action(device, String::new());
    let boards: Rc<Vec<String>> = Rc::new(
        narrowed
            .iter()
            .chain(&wider)
            .map(|board| board.board_id.clone())
            .collect(),
    );
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let board = args.choice(SET_BOARD_PARAM).unwrap_or_default();
        if !boards.iter().any(|id| id == board) {
            return Err(OfferArgError::NotAnOption {
                name: SET_BOARD_PARAM.to_string(),
                value: board.to_string(),
                options: boards.as_ref().clone(),
            });
        }
        Ok(set_board_action(device, board.to_string()))
    });
    Some(UiOffer::with_params(
        prefix.child(SET_BOARD),
        "board",
        params,
        binder,
        template,
    ))
}

/// Whether the card should ASK "Which board is this?" rather than only
/// offer it: an install just ended that could not tell the board (`fits`
/// holds its candidates) and nothing has answered since.
pub fn asks_which_board(view: &DeviceView, fits: &[String]) -> bool {
    fits.len() > 1
        && !view
            .activity
            .as_ref()
            .is_some_and(|activity| activity.kind == ActivityKind::SetBoard)
}

/// The press for `target` on `device`, wearing `verb`'s words.
fn target_action(verb: &FirmwareVerb, target: &FlashTarget, device: DeviceId) -> UiAction {
    DevicesOp::action_for(target.action(device, false))
        .with_label(verb.label())
        .with_summary(verb.summary())
}

fn set_board_action(device: DeviceId, board_id: String) -> UiAction {
    DevicesOp::action_for(Action::SetBoard { device, board_id })
}

/// One board as a choice: its id, its display name, and the blurb line.
fn board_option(choice: &FlashBoardChoice) -> OfferChoice {
    OfferChoice::new(&choice.board_id, &choice.title).with_detail(&choice.blurb)
}

#[cfg(test)]
mod tests {
    use lpa_devices::device::DeviceStatus;
    use lpa_devices::view::{Escape, LoadedProject};
    use lpa_devices::{DeviceId, FlashBuildChoice};

    use super::*;
    use crate::flash_offer;

    #[test]
    fn a_blank_board_installs_on_one_press_with_no_board_to_pick() {
        let view = needs_firmware(FirmwareFace::Blank, Some("esp32c6"));
        let offer = flash_device_offer(&view, prefix()).expect("a blank board flashes");

        assert_eq!(offer.path.to_string(), "devices/mac-a0f26287b48c/flash");
        let [name] = offer.params() else {
            panic!("only the optional name: {:?}", offer.params());
        };
        assert!(!name.is_required(), "nothing to choose");
        assert!(offer.is_enabled());
        assert!(offer.consequence().is_routine(), "nothing on it to lose");
        let bound = offer.press(&OfferArgs::new()).expect("one press");
        let Action::Flash {
            device,
            board_id,
            build,
            park_first,
            name,
            ..
        } = bound.op_as::<DevicesOp>().unwrap().action()
        else {
            panic!("a flash");
        };
        assert_eq!(*device, DeviceId(7));
        assert_eq!(board_id, &None, "the session guesses the board");
        assert_eq!(build, &FlashBuildChoice::Fixed("esp32c6-4mb".to_string()));
        assert!(park_first, "a C6 parks in its downloader first");
        assert_eq!(name, &None, "the controller derives the name");

        let named = offer
            .press(&OfferArgs::new().with("name", "Desk"))
            .expect("a typed name binds");
        let Action::Flash { name, .. } = named.op_as::<DevicesOp>().unwrap().action() else {
            panic!("a flash");
        };
        assert_eq!(name.as_deref(), Some("Desk"));
        assert!(named.meta().consequence.is_routine());
    }

    /// The case Sean hit: a board whose chip nobody has heard. It used to
    /// open every board in the catalog; it installs on one press now.
    #[test]
    fn a_board_whose_chip_was_never_heard_installs_on_one_press() {
        for face in [
            FirmwareFace::Silent,
            FirmwareFace::NoHello,
            FirmwareFace::Foreign { label: None },
        ] {
            let view = needs_firmware(face.clone(), None);
            let offer = flash_device_offer(&view, prefix()).expect("it flashes");
            assert!(offer.params().iter().all(|p| !p.is_required()), "{face:?}");
            let bound = offer.press(&OfferArgs::new()).expect("one press");
            assert!(
                matches!(
                    bound.op_as::<DevicesOp>().unwrap().action(),
                    Action::Flash {
                        build: FlashBuildChoice::ByChip(_),
                        ..
                    }
                ),
                "{face:?}: the session finds the chip"
            );
        }
    }

    /// Replacing somebody else's firmware is still the user's call — asked
    /// inside the activity once the board has been read, so the question
    /// can name what is there. The press itself writes nothing.
    #[test]
    fn the_press_is_routine_even_over_somebody_elses_firmware() {
        for face in [
            FirmwareFace::Foreign { label: None },
            FirmwareFace::Bootloader,
            FirmwareFace::Silent,
            FirmwareFace::NoHello,
        ] {
            let view = needs_firmware(face.clone(), Some("esp32c6"));
            let offer = flash_device_offer(&view, prefix()).expect("it flashes");
            assert!(offer.consequence().is_routine(), "{face:?}");
        }
    }

    #[test]
    fn an_older_lightplayers_update_is_routine_and_a_current_ones_is_lasting() {
        let older = FirmwareFace::OlderLightPlayer { proto: Some(33) };
        let mut view = needs_firmware(older, Some("esp32c6"));
        view.escapes = vec![Escape::Disconnect, Escape::Forget];
        let update = update_firmware_offer(&view, prefix()).expect("it updates");
        assert_eq!(
            update.path.to_string(),
            "devices/mac-a0f26287b48c/update-firmware"
        );
        assert!(
            update.consequence().is_routine(),
            "{:?}",
            update.consequence()
        );
        assert!(update.params().is_empty(), "one press");

        let current = update_firmware_offer(&running(Some("esp32c6")), prefix()).unwrap();
        assert!(current.consequence().arms(), "the Flash op's own Lasting");
        assert!(current.params().is_empty(), "one press, no pick");
    }

    #[test]
    fn a_link_that_cannot_carry_firmware_offers_a_disabled_flash() {
        let mut view = needs_firmware(FirmwareFace::Blank, Some("esp32c6"));
        view.firmware_blocked = Some("Firmware updates need USB.".to_string());
        let offer = flash_device_offer(&view, prefix()).unwrap();
        assert_eq!(
            offer.press(&OfferArgs::new()),
            Err(OfferArgError::Unavailable {
                reason: "Firmware updates need USB.".to_string()
            })
        );

        let mut running = running(Some("esp32c6"));
        running.firmware_blocked = Some("Firmware updates need USB.".to_string());
        let blocked = update_firmware_offer(&running, prefix()).unwrap();
        assert_eq!(
            blocked.press(&OfferArgs::new()),
            Err(OfferArgError::Unavailable {
                reason: "Firmware updates need USB.".to_string()
            })
        );
        assert_eq!(blocked.label(), "Update firmware");
    }

    #[test]
    fn a_chip_this_studio_serves_nothing_for_reads_honestly() {
        let view = needs_firmware(FirmwareFace::Blank, Some("esp32p4"));
        let offer = flash_device_offer(&view, prefix()).unwrap();
        let Err(OfferArgError::Unavailable { reason }) = offer.press(&OfferArgs::new()) else {
            panic!("a chip with no build cannot install");
        };
        assert!(reason.contains("esp32p4"), "{reason}");
    }

    #[test]
    fn a_running_lightplayer_offers_no_flash() {
        let view = running(Some("esp32c6"));
        assert_eq!(flash_device_offer(&view, prefix()), None);
        assert!(update_firmware_offer(&view, prefix()).is_some());
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
            usb: None,
            held_by_tab: false,
            escapes: vec![Escape::Forget],
        };
        let prefix = OfferPath::board(&crate::BoardRef::New(3));
        let offer = flash_pending_offer(&pending, prefix.clone()).expect("a blank chip flashes");
        assert_eq!(offer.path.to_string(), "devices/new-3/flash");
        assert!(offer.consequence().is_routine());
        let bound = offer.press(&OfferArgs::new()).unwrap();
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

    /// "Which board is this?", after an install that could not tell: the
    /// fingerprint's fits first, the rest of the chip's boards behind the
    /// toggle, and the press saves the model.
    #[test]
    fn which_board_offers_the_fits_first_and_saves_the_pick() {
        let view = running(Some("esp32c6"));
        let chip_boards = flash_offer(Some("esp32c6")).candidates;
        assert!(chip_boards.len() > 1, "the premise: two C6 boards");
        let fit = chip_boards[1].board_id.clone();
        let offer = set_board_offer(&view, prefix(), std::slice::from_ref(&fit)).unwrap();
        assert_eq!(offer.path.to_string(), "devices/mac-a0f26287b48c/set-board");
        assert!(offer.consequence().is_routine());
        let [board, all] = offer.params() else {
            panic!("board + all_boards: {:?}", offer.params());
        };
        let crate::OfferParamKind::Choice { options, preselect } = &board.kind else {
            panic!("a choice: {board:?}");
        };
        assert_eq!(options[0].value, fit);
        assert!(options[0].only_with.is_none());
        assert!(
            options[1..]
                .iter()
                .all(|o| o.only_with.as_deref() == Some(SET_BOARD_ALL_PARAM))
        );
        assert_eq!(preselect.as_ref(), Some(&fit), "one fit preselects");
        assert_eq!(all.kind, crate::OfferParamKind::Toggle { value: false });

        let bound = offer.press(&OfferArgs::new()).expect("the preselect binds");
        assert_eq!(
            bound.op_as::<DevicesOp>().unwrap().action(),
            &Action::SetBoard {
                device: DeviceId(7),
                board_id: fit,
            }
        );
        assert!(
            offer
                .press(&OfferArgs::new().with("board", "quinled/dig-uno"))
                .is_err(),
            "a board of another chip is not an option"
        );
    }

    #[test]
    fn which_board_with_no_fits_offers_every_board_of_the_chip_unpicked() {
        let view = running(Some("esp32c6"));
        let offer = set_board_offer(&view, prefix(), &[]).unwrap();
        let [board] = offer.params() else {
            panic!("no toggle when nothing narrowed: {:?}", offer.params());
        };
        let crate::OfferParamKind::Choice { preselect, .. } = &board.kind else {
            panic!("a choice");
        };
        assert_eq!(preselect, &None);
        assert!(!offer.is_enabled(), "nothing chosen yet");
    }

    #[test]
    fn which_board_is_only_asked_of_a_running_board_whose_chip_fits_several() {
        assert!(set_board_offer(&running(Some("esp32c6")), prefix(), &[]).is_some());
        let blank = needs_firmware(FirmwareFace::Blank, Some("esp32c6"));
        assert_eq!(set_board_offer(&blank, prefix(), &[]), None);
        let mut over_ble = running(Some("esp32c6"));
        over_ble.firmware_blocked = Some("Firmware updates need USB.".to_string());
        assert_eq!(set_board_offer(&over_ble, prefix(), &[]), None);
        assert!(asks_which_board(
            &running(Some("esp32c6")),
            &["a".into(), "b".into()]
        ));
        assert!(!asks_which_board(&running(Some("esp32c6")), &["a".into()]));
    }

    fn running(chip: Option<&str>) -> DeviceView {
        DeviceView {
            status: DeviceStatus::Ready,
            can_receive_project: true,
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
            heard_age_ms: None,
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
            usb: None,
            held_elsewhere: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
            update_blocked: None,
            last_update_outcome: None,
        }
    }
}
