//! The over-the-air update's verbs, as offers under `devices/<board>/…`,
//! one set per [`UpdateStanding`] row (the update-states spike's offers
//! table, as ruled: an over-the-air update to a newer version is Routine,
//! and nothing is offered after a rollback — the board refuses that build
//! for good).
//!
//! | Row | Path verb | Button | Level |
//! |---|---|---|---|
//! | Update available | `update-firmware` | `Update` (another build: `Install Y`) | Routine |
//! | Keeps crashing | `reinstall-firmware` | `Reinstall` | Routine |
//! | Keeps crashing | `install-firmware` | `Other version…` (one `version` choice) | Routine; Lasting when the choice is older than the board's |
//! | A version Studio can't get | `install-firmware` | `Install Y` (the same offer) | Routine; Lasting when the choice is older |
//! | Backing up | `cancel` | the activity's own Cancel ([`super::device_offers`]) | Routine |
//! | Needs USB once, over USB | `update-firmware` | today's USB flash | Lasting |
//! | This Studio's build can't go over the air, over USB (standing `Nothing`, route `Flash`) | `update-firmware` | today's USB flash | Lasting |
//! | This Studio's build can't go over the air, over Bluetooth (`NoWirelessBuild`) | — | nothing; the line says why | — |
//! | everything else | — | nothing: Studio does it, or there is nothing to do | — |
//!
//! Heal and finish are never offers (DS4): the controller starts them. A
//! play-only user is offered nothing here; the device zone's "Enter a
//! password" is the way. Factory reset stays on every board except two —
//! keeps crashing and a version Studio can't get, where installing is the
//! repair — and the rows where firmware is being written.
//!
//! Each verb binds `Action::Update { device, intent }`: `Update` and
//! `Install Y` install Y; `Other version…` installs the chosen version,
//! with `allow_downgrade` when it is older than the board's; `Reinstall`
//! writes the crashing board's own firmware again. An over-the-air verb is
//! drawn disabled with [`DeviceView::update_blocked`]'s reason when the
//! link cannot carry it; the flash and Factory reset keep
//! [`DeviceView::firmware_blocked`].

use lpa_devices::view::{DeviceView, Escape};
use lpa_devices::{Action, DeviceId, FirmwareAge, UpdateIntentFacts};

use super::device_update_route::{UpdateLink, UpdateRoute, update_route};
use super::device_update_standing::{UpdateStanding, UpdateStandingInputs, update_standing};
use super::device_update_version::UpdateVersion;
use super::devices_op::DevicesOp;
use crate::{OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath, UiAction, UiOffer};

/// The install verb's version parameter.
pub const INSTALL_VERSION_PARAM: &str = "version";

/// What a board's standing offers, and which of today's firmware verbs it
/// keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateOffers {
    /// The over-the-air verbs, in card order.
    pub offers: Vec<UiOffer>,
    /// Today's USB flash at `update-firmware` stays.
    pub keep_flash: bool,
    /// Factory reset stays.
    pub keep_erase: bool,
}

/// The facts the card's offers need about a board's update: its standing
/// and which way its update goes. The default tells no story (today's
/// card).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateOfferFacts {
    pub standing: UpdateStanding,
    pub route: UpdateRoute,
}

impl UpdateOfferFacts {
    /// A board's facts: its standing from `inputs`, and its route — over the
    /// air when its manifest says it can update over its link and that link
    /// `carries_update_channel`. The one reading the controller and the
    /// stories share.
    ///
    /// The build half of the route is this Studio's own build: it has
    /// facts only when it is update-capable (`device_update_route`'s module
    /// docs). A board that could update over Bluetooth while this Studio's
    /// build cannot reads [`UpdateStanding::NoWirelessBuild`].
    pub fn read(inputs: &UpdateStandingInputs<'_>, carries_update_channel: bool) -> Self {
        let can_update_over_link = inputs
            .facts
            .and_then(|facts| lpa_update::BoardView::from_json(facts.manifest_json.as_bytes()))
            .is_some_and(|board| board.can_update_over_link());
        let route = update_route(
            can_update_over_link,
            inputs.own.is_some(),
            inputs.link,
            carries_update_channel,
        );
        let mut standing = update_standing(inputs);
        if route == UpdateRoute::NoWirelessBuild
            && standing == UpdateStanding::Nothing
            && let Some(version) = inputs.facts.and_then(|facts| facts.version.clone())
        {
            standing = UpdateStanding::NoWirelessBuild {
                board: UpdateVersion {
                    version,
                    build_id: inputs.facts.and_then(|facts| facts.build_id.clone()),
                },
            };
        }
        Self { standing, route }
    }
}

/// The update offers `view`'s card makes for `facts`, under `prefix`. See
/// the module docs.
pub fn update_offers(
    view: &DeviceView,
    facts: &UpdateOfferFacts,
    prefix: &OfferPath,
) -> UpdateOffers {
    let standing = &facts.standing;
    let mut set = UpdateOffers {
        offers: Vec::new(),
        keep_flash: true,
        keep_erase: !withdraws_factory_reset(standing),
    };
    match standing {
        UpdateStanding::Nothing => return set,
        // Over USB, today's flash is the one update this board takes; over
        // the air there is nothing to press, and the line says what to do.
        UpdateStanding::NeedsUsbOnce { link, .. } => {
            set.keep_flash = *link == UpdateLink::Usb;
            return set;
        }
        // This Studio's build cannot go over the air, and over Bluetooth
        // there is no flash either: nothing to install, the line says why.
        UpdateStanding::NoWirelessBuild { .. } => {
            set.keep_flash = false;
            return set;
        }
        _ => {}
    }
    match facts.route {
        UpdateRoute::Flash => return set,
        UpdateRoute::NoWirelessBuild => {
            set.keep_flash = false;
            return set;
        }
        UpdateRoute::OverTheAir => {}
    }
    set.keep_flash = false;
    let linked = view.escapes.contains(&Escape::Disconnect);
    if !linked || view.activity.is_some() {
        return set;
    }
    let device = view.id;
    let blocked = view.update_blocked.as_deref();
    let at = |verb: &str| prefix.clone().child(verb);
    match standing {
        UpdateStanding::Available { board, to } => {
            let label = match board.age_against(to) {
                FirmwareAge::Older | FirmwareAge::Unknown => "Update".to_string(),
                _ => format!("Install {}", to.short()),
            };
            let action = install_action(device, &to.version, false).with_label(label);
            set.offers.push(UiOffer::new(
                at("update-firmware"),
                "download",
                gate(action, blocked),
            ));
        }
        UpdateStanding::KeepsCrashing { board, choices } => {
            let reinstall = DevicesOp::action_for(Action::Update {
                device,
                intent: UpdateIntentFacts::Reinstall,
            });
            set.offers.push(UiOffer::new(
                at("reinstall-firmware"),
                "retry",
                gate(reinstall, blocked),
            ));
            // "Other version…" offers the versions this Studio can get
            // other than the board's own: that one is Reinstall.
            let others: Vec<UpdateVersion> = choices
                .iter()
                .filter(|choice| choice.version != board.version)
                .cloned()
                .collect();
            if !others.is_empty() {
                set.offers.push(install_offer(
                    at("install-firmware"),
                    device,
                    board,
                    &others,
                    "Other version…",
                    blocked,
                ));
            }
        }
        UpdateStanding::CantGetVersion {
            board,
            own,
            choices,
        } => {
            set.offers.push(install_offer(
                at("install-firmware"),
                device,
                board,
                choices,
                &format!("Install {}", own.short()),
                blocked,
            ));
        }
        _ => {}
    }
    set
}

/// Whether the row takes Factory reset away: where installing is the
/// repair, and while firmware is being written.
fn withdraws_factory_reset(standing: &UpdateStanding) -> bool {
    standing.is_progress()
        || matches!(
            standing,
            UpdateStanding::KeepsCrashing { .. } | UpdateStanding::CantGetVersion { .. }
        )
}

/// `install-firmware`: one `version` choice — the versions this Studio can
/// get, its own preselected — bound to an install of the chosen one, which
/// is Lasting when it is older than the board's. With a single version
/// there is nothing to choose, and the offer is that install, one press.
fn install_offer(
    path: OfferPath,
    device: DeviceId,
    board: &UpdateVersion,
    choices: &[UpdateVersion],
    label: &str,
    blocked: Option<&str>,
) -> UiOffer {
    let preselect = choices.first().map(|choice| choice.version.clone());
    let older = {
        let board = board.clone();
        move |version: &str| board.age_against(&UpdateVersion::new(version)) == FirmwareAge::Newer
    };
    let bind = {
        let label = label.to_string();
        move |version: &str| {
            install_action(device, version, older(version)).with_label(label.clone())
        }
    };
    let template = bind(preselect.as_deref().unwrap_or_default());
    if let Some(reason) = blocked {
        return UiOffer::new(path, "download", template.disabled(reason));
    }
    // One version to get is no choice: the button installs it at one press.
    if choices.len() == 1 {
        return UiOffer::new(path, "download", template);
    }
    let options = choices
        .iter()
        .map(|choice| OfferChoice::new(&choice.version, choice.short()))
        .collect();
    UiOffer::with_params(
        path,
        "download",
        vec![OfferParam::choice(
            INSTALL_VERSION_PARAM,
            "version",
            options,
            preselect,
        )],
        OfferBinder::new(move |args: &OfferArgs| {
            Ok(bind(args.choice(INSTALL_VERSION_PARAM).unwrap_or_default()))
        }),
        template,
    )
}

/// An install of `version`; the op's own meta makes a downgrade Lasting.
fn install_action(device: DeviceId, version: &str, allow_downgrade: bool) -> UiAction {
    DevicesOp::action_for(Action::Update {
        device,
        intent: UpdateIntentFacts::Install {
            version: version.to_string(),
            allow_downgrade,
        },
    })
}

/// `action`, disabled with `blocked` when the link cannot carry an update.
fn gate(action: UiAction, blocked: Option<&str>) -> UiAction {
    match blocked {
        Some(reason) => action.disabled(reason),
        None => action,
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::UpdateStageFacts;
    use lpa_devices::view::FIRMWARE_NEEDS_USB;

    use super::super::device_update_standing::tests::{ready_view, updating_view};
    use super::*;
    use crate::{ActionConsequence, ActionEnablement};

    fn x() -> UpdateVersion {
        UpdateVersion::with_build_id("2026.10.03-1", "2026.10.03-1+a41c9e2d11f0")
    }
    fn y() -> UpdateVersion {
        UpdateVersion::with_build_id("2026.10.05-2", "2026.10.05-2+626a1b851aaa")
    }
    fn latest() -> UpdateVersion {
        UpdateVersion::with_build_id("2026.10.07-4", "2026.10.07-4+c08d1f3eeee0")
    }

    fn prefix() -> OfferPath {
        OfferPath::board(&crate::BoardRef::Mac(
            lpa_devices::BoardKey::parse("60:55:f9:0a:0b:0c").unwrap(),
        ))
    }

    fn over_the_air(standing: UpdateStanding) -> UpdateOfferFacts {
        UpdateOfferFacts {
            standing,
            route: UpdateRoute::OverTheAir,
        }
    }

    /// Each offer's path verb and whether it arms.
    fn set_of(view: &DeviceView, facts: &UpdateOfferFacts) -> (Vec<(String, bool)>, bool, bool) {
        let set = update_offers(view, facts, &prefix());
        let offers = set
            .offers
            .iter()
            .map(|offer| {
                let path = offer.path.to_string();
                assert!(path.starts_with("devices/mac-6055f90a0b0c/"), "{path}");
                (
                    offer.path.last().unwrap().to_string(),
                    offer.consequence().arms(),
                )
            })
            .collect();
        (offers, set.keep_flash, set.keep_erase)
    }

    fn bound_intent(offer: &UiOffer, args: &OfferArgs) -> UpdateIntentFacts {
        let action = offer.press(args).expect("press");
        match action.op_as::<DevicesOp>().unwrap().action() {
            Action::Update { intent, .. } => intent.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// The director's note on P8, read end to end: a board that can update
    /// over the air, and this Studio with or without an update-capable build
    /// of its own, over USB and over Bluetooth.
    #[test]
    fn the_route_reads_the_board_and_this_studios_build() {
        use super::super::device_update_standing::tests::{board_x, facts_of, inputs, studio_y};
        let view = ready_view();
        let facts = facts_of(&board_x());
        let y = studio_y();

        // Both can, over USB: Update, over the air.
        let read = UpdateOfferFacts::read(&inputs(&view, Some(&facts), Some(&y)), true);
        assert_eq!(read.route, UpdateRoute::OverTheAir);
        assert!(matches!(read.standing, UpdateStanding::Available { .. }));
        assert_eq!(
            set_of(&view, &read),
            (vec![("update-firmware".to_string(), false)], false, true)
        );

        // A single-image Studio over USB: today's card, today's flash.
        let read = UpdateOfferFacts::read(&inputs(&view, Some(&facts), None), true);
        assert_eq!(read.route, UpdateRoute::Flash);
        assert_eq!(read.standing, UpdateStanding::Nothing);
        assert_eq!(set_of(&view, &read), (vec![], true, true));

        // The same over Bluetooth: nothing to install, and the line says why.
        let mut ble = inputs(&view, Some(&facts), None);
        ble.link = UpdateLink::Bluetooth;
        let read = UpdateOfferFacts::read(&ble, true);
        assert_eq!(read.route, UpdateRoute::NoWirelessBuild);
        assert!(
            matches!(&read.standing, UpdateStanding::NoWirelessBuild { board } if board.version == "2026.10.03-1"),
            "{:?}",
            read.standing
        );
        assert_eq!(set_of(&view, &read), (vec![], false, true));
        let words = crate::update_words(&read.standing).expect("words");
        assert_eq!(words.line, "Can't update over Bluetooth from this Studio");

        // Both can over Bluetooth: over the air there too.
        let mut ble = inputs(&view, Some(&facts), Some(&y));
        ble.link = UpdateLink::Bluetooth;
        assert_eq!(
            UpdateOfferFacts::read(&ble, true).route,
            UpdateRoute::OverTheAir
        );
    }

    #[test]
    fn up_to_date_offers_no_update_and_keeps_factory_reset() {
        let facts = over_the_air(UpdateStanding::UpToDate { version: y() });
        assert_eq!(set_of(&ready_view(), &facts), (vec![], false, true));
    }

    #[test]
    fn update_available_is_one_routine_update() {
        let facts = over_the_air(UpdateStanding::Available {
            board: x(),
            to: y(),
        });
        assert_eq!(
            set_of(&ready_view(), &facts),
            (vec![("update-firmware".to_string(), false)], false, true)
        );
        let set = update_offers(&ready_view(), &facts, &prefix());
        let update = &set.offers[0];
        assert_eq!(update.label(), "Update");
        assert!(update.consequence().is_routine(), "the agent presses it");
        assert_eq!(
            bound_intent(update, &OfferArgs::new()),
            UpdateIntentFacts::Install {
                version: "2026.10.05-2".to_string(),
                allow_downgrade: false
            }
        );
    }

    #[test]
    fn a_dev_board_is_offered_install_y() {
        let facts = over_the_air(UpdateStanding::Available {
            board: UpdateVersion::new("5eb70a7c2"),
            to: y(),
        });
        let set = update_offers(&ready_view(), &facts, &prefix());
        assert_eq!(set.offers[0].label(), "Install 2026.10.05-2");
        assert!(set.offers[0].params().is_empty(), "Y alone: one press");
        let facts = over_the_air(UpdateStanding::Available {
            board: x(),
            to: UpdateVersion::new("626a1b851"),
        });
        let set = update_offers(&ready_view(), &facts, &prefix());
        assert_eq!(set.offers[0].label(), "Install dev 626a1b8");
    }

    #[test]
    fn the_update_waits_for_a_link_that_carries_it() {
        let mut view = ready_view();
        view.update_blocked = Some(FIRMWARE_NEEDS_USB.to_string());
        let facts = over_the_air(UpdateStanding::Available {
            board: x(),
            to: y(),
        });
        let set = update_offers(&view, &facts, &prefix());
        assert_eq!(
            set.offers[0].action.meta().enablement,
            ActionEnablement::Disabled {
                reason: FIRMWARE_NEEDS_USB.to_string()
            }
        );
    }

    #[test]
    fn backing_up_offers_nothing_here_the_activitys_cancel_is_the_way_out() {
        let view = updating_view(Some(UpdateStageFacts::BackingUp), Some(18));
        assert!(view.escapes.contains(&Escape::Cancel), "the card's cancel");
        let facts = over_the_air(UpdateStanding::BackingUp {
            board: x(),
            to: y(),
            percent: Some(18),
        });
        assert_eq!(set_of(&view, &facts), (vec![], false, false));
    }

    #[test]
    fn while_firmware_is_written_nothing_is_offered() {
        let view = updating_view(Some(UpdateStageFacts::Updating), Some(40));
        assert!(
            !view.escapes.contains(&Escape::Cancel),
            "no Cancel once writing"
        );
        for standing in [
            UpdateStanding::Updating {
                board: x(),
                to: y(),
                link: UpdateLink::Bluetooth,
                percent: Some(40),
            },
            UpdateStanding::Finishing {
                board: x(),
                to: y(),
                percent: Some(70),
                running: true,
                resumed: false,
            },
            UpdateStanding::Restoring {
                board: x(),
                percent: Some(35),
                running: true,
            },
        ] {
            assert_eq!(
                set_of(&view, &over_the_air(standing)),
                (vec![], false, false)
            );
        }
        // Heal and finish about to start themselves, and another device's
        // update: still nothing (DS4), and no Factory reset under them.
        for standing in [
            UpdateStanding::Restoring {
                board: x(),
                percent: None,
                running: false,
            },
            UpdateStanding::Finishing {
                board: x(),
                to: y(),
                percent: Some(70),
                running: false,
                resumed: true,
            },
            UpdateStanding::AnotherDevice {
                board: x(),
                to: Some(y()),
                percent: Some(40),
            },
        ] {
            assert_eq!(
                set_of(&ready_view(), &over_the_air(standing)),
                (vec![], false, false)
            );
        }
    }

    #[test]
    fn needs_usb_once_keeps_the_flash_over_usb_and_offers_nothing_over_bluetooth() {
        for route in [UpdateRoute::OverTheAir, UpdateRoute::Flash] {
            let usb = UpdateOfferFacts {
                standing: UpdateStanding::NeedsUsbOnce {
                    board: x(),
                    to: y(),
                    link: UpdateLink::Usb,
                },
                route,
            };
            assert_eq!(set_of(&ready_view(), &usb), (vec![], true, true));
            let ble = UpdateOfferFacts {
                standing: UpdateStanding::NeedsUsbOnce {
                    board: x(),
                    to: y(),
                    link: UpdateLink::Bluetooth,
                },
                route,
            };
            assert_eq!(set_of(&ready_view(), &ble), (vec![], false, true));
        }
    }

    #[test]
    fn keeps_crashing_offers_reinstall_and_other_version_and_withdraws_factory_reset() {
        let facts = over_the_air(UpdateStanding::KeepsCrashing {
            board: x(),
            choices: vec![y(), latest()],
        });
        assert_eq!(
            set_of(&ready_view(), &facts),
            (
                vec![
                    ("reinstall-firmware".to_string(), false),
                    ("install-firmware".to_string(), false)
                ],
                false,
                false
            )
        );
        let set = update_offers(&ready_view(), &facts, &prefix());
        assert_eq!(set.offers[0].label(), "Reinstall");
        assert_eq!(
            bound_intent(&set.offers[0], &OfferArgs::new()),
            UpdateIntentFacts::Reinstall
        );
        let other = &set.offers[1];
        assert_eq!(other.label(), "Other version…");
        let crate::OfferParamKind::Choice { options, preselect } = &other.params()[0].kind else {
            panic!("{:?}", other.params());
        };
        assert_eq!(preselect.as_deref(), Some("2026.10.05-2"), "Y, preselected");
        let values: Vec<_> = options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(values, ["2026.10.05-2", "2026.10.07-4"]);
    }

    /// "Other version…" never repeats Reinstall, and one version to get is
    /// one press, no pick.
    #[test]
    fn other_version_leaves_out_the_boards_own_and_a_single_version_is_one_press() {
        let only_own = over_the_air(UpdateStanding::KeepsCrashing {
            board: y(),
            choices: vec![y()],
        });
        assert_eq!(
            set_of(&ready_view(), &only_own),
            (
                vec![("reinstall-firmware".to_string(), false)],
                false,
                false
            ),
            "the board's own version is Reinstall's"
        );
        let one_other = over_the_air(UpdateStanding::KeepsCrashing {
            board: y(),
            choices: vec![y(), latest()],
        });
        let set = update_offers(&ready_view(), &one_other, &prefix());
        let other = &set.offers[1];
        assert_eq!(other.label(), "Other version…");
        assert!(other.params().is_empty(), "nothing to pick");
        assert_eq!(
            bound_intent(other, &OfferArgs::new()),
            UpdateIntentFacts::Install {
                version: "2026.10.07-4".to_string(),
                allow_downgrade: false
            }
        );
    }

    #[test]
    fn a_version_studio_cant_get_offers_install_y_and_withdraws_factory_reset() {
        let facts = over_the_air(UpdateStanding::CantGetVersion {
            board: UpdateVersion::new("2026.09.28-4"),
            own: y(),
            choices: vec![y()],
        });
        assert_eq!(
            set_of(&ready_view(), &facts),
            (vec![("install-firmware".to_string(), false)], false, false)
        );
        let set = update_offers(&ready_view(), &facts, &prefix());
        assert_eq!(set.offers[0].label(), "Install 2026.10.05-2");
        assert!(set.offers[0].params().is_empty(), "Y alone: one press");
        assert_eq!(
            bound_intent(&set.offers[0], &OfferArgs::new()),
            UpdateIntentFacts::Install {
                version: "2026.10.05-2".to_string(),
                allow_downgrade: false
            }
        );
    }

    /// The binder: a choice older than the board's arms (Lasting, with the
    /// words that say what is at stake); a newer one is one click.
    #[test]
    fn other_version_is_lasting_only_when_the_choice_is_older() {
        let facts = over_the_air(UpdateStanding::KeepsCrashing {
            board: latest(),
            choices: vec![y(), x(), latest()],
        });
        let set = update_offers(&ready_view(), &facts, &prefix());
        let other = &set.offers[1];
        // Y (preselected) is older than the board's 2026.10.07-4.
        assert!(other.consequence().arms(), "the bound default is older");
        let older = other
            .press(&OfferArgs::new().with(INSTALL_VERSION_PARAM, "2026.10.05-2"))
            .unwrap();
        let ActionConsequence::Lasting(copy) = &older.meta().consequence else {
            panic!("{:?}", older.meta().consequence);
        };
        assert!(copy.message.contains("2026.10.05-2"), "{}", copy.message);
        assert_eq!(older.meta().label, "Other version…");
        assert_eq!(
            bound_intent(
                other,
                &OfferArgs::new().with(INSTALL_VERSION_PARAM, "2026.10.05-2")
            ),
            UpdateIntentFacts::Install {
                version: "2026.10.05-2".to_string(),
                allow_downgrade: true
            }
        );

        let facts = over_the_air(UpdateStanding::KeepsCrashing {
            board: x(),
            choices: vec![y(), latest()],
        });
        let set = update_offers(&ready_view(), &facts, &prefix());
        let other = &set.offers[1];
        for newer in ["2026.10.05-2", "2026.10.07-4"] {
            let action = other
                .press(&OfferArgs::new().with(INSTALL_VERSION_PARAM, newer))
                .unwrap();
            assert!(action.meta().consequence.is_routine(), "{newer}");
        }
        assert!(
            other
                .press(&OfferArgs::new().with(INSTALL_VERSION_PARAM, "2026.01.01-1"))
                .is_err(),
            "only the versions this Studio can get"
        );
    }

    #[test]
    fn rolled_back_newer_and_play_only_offer_no_update() {
        for standing in [
            UpdateStanding::RolledBack {
                board: x(),
                refused: y(),
            },
            UpdateStanding::Newer {
                board: latest(),
                own: y(),
            },
            UpdateStanding::PlayOnly {
                board: x(),
                to: y(),
            },
        ] {
            assert_eq!(
                set_of(&ready_view(), &over_the_air(standing)),
                (vec![], false, true)
            );
        }
    }

    /// The flash route (a link without the update channel): today's verbs,
    /// words aside.
    #[test]
    fn on_the_flash_route_the_card_keeps_todays_verbs() {
        let facts = UpdateOfferFacts {
            standing: UpdateStanding::Available {
                board: x(),
                to: y(),
            },
            route: UpdateRoute::Flash,
        };
        assert_eq!(set_of(&ready_view(), &facts), (vec![], true, true));
        assert_eq!(
            set_of(&ready_view(), &UpdateOfferFacts::default()),
            (vec![], true, true)
        );
    }
}
