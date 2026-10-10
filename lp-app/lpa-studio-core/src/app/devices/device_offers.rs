//! Every verb a device card offers, as offers under `devices/<board>/…`.
//!
//! The card's verbs were decided in core already — the escapes the model
//! projects ([`DeviceView::escapes`]), the firmware verb
//! ([`firmware_verb`]), the push and flash picks — but WHICH of them the
//! card drew, and when, was the renderer's reading of the view. Here that
//! reading moves into core, so the card and the app agent are offered the
//! same verbs at the same paths. The conditions are the card's own, verb
//! for verb:
//!
//! | path verb | offered when |
//! |---|---|
//! | `cancel`, `retry`, `reconnect`, `disconnect`, `forget` | the model projects that escape (`retry` and `forget` not on a board another tab holds: Forget hands the site's port grant back, which can pull the port from that tab, and deletes the registry row every tab shares) |
//! | `push` | a LightPlayer on an open port, idle, that has said what it runs ([`push_device_offer`]) and is not locked |
//! | `clear-faults` | the board reported it is degraded, linked, idle |
//! | `remove-project` | the board reported something running, port open, idle |
//! | `flash` | the needs-firmware faces ([`flash_device_offer`]); not on a board another tab holds over USB (this tab's port to it is that tab's) |
//! | `update-firmware` | over the air: an update available ([`update_offers`], Routine); else the USB flash on a running LightPlayer ([`update_firmware_offer`], Lasting; not on a board another tab holds over USB) |
//! | `reinstall-firmware` | the board's firmware keeps crashing ([`update_offers`]) |
//! | `install-firmware` | an idle board that can update over the air — up to date, update available, newer, rolled back, keeps crashing ("Other version…") or needs a version Studio can't get ("Install Y"): a `find` box over a `version` choice from the store's release index, or a look-up of a version the box names ([`update_offers`]) |
//! | `install-firmware-file` | wherever `install-firmware` opens its list: "From a file…", a custom build's update files picked in the web's file dialog (needs the user's click; the build joins the list, and its install arms) ([`update_offers`]) |
//! | `erase` | linked, idle, not a needs-firmware face (erasing a blank flash does nothing), and not where the update standing withdraws it ([`update_offers`]); not on a board another tab holds over USB |
//! | `identify` | linked and idle, where Retry (the same `Identify`) is not already offered; not on a board another tab holds |
//! | `connect` | the port is there but closed, and no other tab holds the board (then the controller offers `take-over` instead) |
//! | `reset-board` | linked: a USB link's reset lines, a network link's restart request ([`ResetReach`]); disabled over a network link without the author tier (the board would refuse) and while an activity runs (the model refuses a reset under one; Cancel is the escape); not on a board another tab holds over USB (the lines are that tab's port's) |
//! | `rename` | always: one Text param, `name` |
//! | `autoconnect` | a board at the end of a wire: one Toggle param, `enabled` |
//!
//! Levels are the ops' own (Forget, Factory reset and Remove project are
//! Lasting through their meta), plus the three that depend on the board:
//! Flash (Q3), Push (Q4) and the over-the-air install (an older version is
//! Lasting).
//!
//! `connect` reaches the board if it must, then opens its panel
//! ([`RuntimeOp::ConnectDevice`](crate::RuntimeOp::ConnectDevice)): on a
//! closed port (the row above) it opens the port, then the board's session
//! — and while an editor open is held for the board, it opens the port
//! only. On a ready board, and on an offline one Studio can reach, it is
//! the controller's, beside `edit` and `done`
//! ([`device_connect_offer`](super::connect_offer::device_connect_offer)).

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{DeviceView, Escape};
use lpa_devices::{Action, HoldVia};

use super::device_affordance::device_escape_action_for;
use super::device_flash::{FirmwareVerb, firmware_verb};
use super::device_reset_reach::{RESET_NEEDS_AUTHOR, ResetReach};

/// Why Reset is disabled while an activity runs: the device model refuses a
/// reset under one, and Cancel is the way out.
pub const RESET_WAITS_FOR_ACTIVITY: &str =
    "Reset waits until Studio finishes what it is doing; cancel it first";
use super::device_flash_offer::{flash_device_offer, update_firmware_offer};
use super::device_push_offer::push_device_offer;
use super::device_update_offers::{UpdateOfferFacts, update_offers};
use super::devices_op::{DeviceFace, DevicesOp};
use crate::app::home::{UiExampleCard, UiPackageCard};
use crate::{OfferArgs, OfferBinder, OfferParam, OfferPath, UiOffer};

/// The Rename offer's text parameter.
pub const RENAME_NAME_PARAM: &str = "name";
/// The Autoconnect offer's toggle parameter.
pub const AUTOCONNECT_ENABLED_PARAM: &str = "enabled";

/// What the device offers need beyond the card's own view: facts the
/// controller joins from elsewhere.
#[derive(Clone, Debug)]
pub struct DeviceOfferFacts<'a> {
    /// `devices/<board>`: the board's ref ([`crate::BoardRef`]).
    pub prefix: OfferPath,
    /// A sim is powered on and off; a board is connected and disconnected.
    pub face: DeviceFace,
    /// The device's remembered autoconnect choice.
    pub autoconnect: bool,
    /// A Bluetooth link nothing has unlocked: it answers only its hello and
    /// the unlock, so it is offered no push.
    pub locked: bool,
    /// How Reset restarts this board, and whether this link may ask.
    pub reset: ResetReach,
    /// Whether the library holds what the board runs (Q4).
    pub banked: bool,
    /// The library's projects, for the push.
    pub projects: &'a [UiPackageCard],
    /// The gallery's examples, for the push.
    pub examples: &'a [UiExampleCard],
    /// The board's update standing and route ([`update_offers`]); the
    /// default tells no story, and the card keeps today's firmware verbs.
    pub update: UpdateOfferFacts,
}

/// Every offer `view`'s card makes, in the order the card reads: the
/// running activity's Cancel, the project verbs, the firmware verbs, the
/// device verbs, the entry's own (rename, autoconnect), Forget last.
pub fn device_offers(view: &DeviceView, facts: &DeviceOfferFacts<'_>) -> Vec<UiOffer> {
    let device = view.id;
    let at = |verb: &str| facts.prefix.clone().child(verb);
    let idle = view.activity.is_none();
    let linked = view.escapes.contains(&Escape::Disconnect);
    let verb = firmware_verb(view);
    // A board another tab of this browser holds. Over USB, the OS opens a
    // port for one tab at a time, so this tab's port to the board (gated,
    // or let go on request) is that tab's: nothing here may touch its
    // firmware or its lines. Over the network the board's USB port, if this
    // tab has one, stays its own.
    let held_elsewhere = view.held_elsewhere.as_ref();
    let port_elsewhere = held_elsewhere.is_some_and(|held| held.via == HoldVia::Usb);
    let mut offers = Vec::new();
    let escape = |escape: Escape| {
        UiOffer::new(
            at(escape_verb(escape)),
            escape_icon(escape),
            device_escape_action_for(escape, device, facts.face),
        )
    };

    if view.escapes.contains(&Escape::Cancel) {
        offers.push(escape(Escape::Cancel));
    }

    // PROJECT
    if !facts.locked
        && let Some(push) = push_device_offer(
            view,
            facts.prefix.clone(),
            facts.projects,
            facts.examples,
            facts.banked,
        )
    {
        offers.push(push);
    }
    if view.status == DeviceStatus::Degraded && linked && idle {
        offers.push(UiOffer::new(
            at("clear-faults"),
            "revert",
            DevicesOp::action_for(Action::ClearFaults { device }),
        ));
    }
    if view.can_remove_project {
        offers.push(UiOffer::new(
            at("remove-project"),
            "remove",
            DevicesOp::action_for(Action::RemoveProject { device }),
        ));
    }

    // FIRMWARE
    let update = update_offers(view, &facts.update, &facts.prefix);
    if !port_elsewhere && let Some(flash) = flash_device_offer(view, facts.prefix.clone()) {
        offers.push(flash);
    }
    if update.keep_flash
        && !port_elsewhere
        && let Some(flash) = update_firmware_offer(view, facts.prefix.clone())
    {
        offers.push(flash);
    }
    offers.extend(update.offers);
    if update.keep_erase && idle && linked && !port_elsewhere && verb != Some(FirmwareVerb::Flash) {
        let erase = DevicesOp::action_for(Action::Erase { device });
        offers.push(UiOffer::new(
            at("erase"),
            "remove",
            match &view.firmware_blocked {
                Some(reason) => erase.disabled(reason),
                None => erase,
            },
        ));
    }

    // DEVICE
    // A board another tab holds is opened by its `take-over` (the
    // controller's): Retry, Identify and Connect would each open a port
    // that tab holds, fighting it for the board.
    if held_elsewhere.is_none() {
        if view.escapes.contains(&Escape::Retry) {
            offers.push(escape(Escape::Retry));
        } else if idle && linked {
            offers.push(UiOffer::new(
                at("identify"),
                "info",
                DevicesOp::action_for(Action::Identify { device }),
            ));
        }
    }
    if view.status == DeviceStatus::Attached && held_elsewhere.is_none() {
        offers.push(UiOffer::new(
            at("connect"),
            "connect",
            // Open the port, then the board's session: one press.
            super::connect_offer::connect_port_action(device),
        ));
    }
    // Reset is offered on any linked board, busy or not, so the way out of
    // a stuck board is always visible. Over a cable it pulses the reset
    // lines; over Bluetooth, Wi‑Fi or the relay it asks the board to restart
    // itself, which the board does only for the author tier — so a network
    // link without it is published DISABLED, saying so. The device model
    // also refuses a reset while an activity runs (`lpa-devices` device.rs:
    // a reset under a flash would wreck it), so while busy it is disabled
    // with that reason: an offer the user or the agent can press must do
    // something (director, M3 P3). The escape from a busy board is Cancel,
    // which leads the list; Reset enables once the activity ends.
    if linked && !port_elsewhere {
        let reset = DevicesOp::action_for(Action::ResetBoard { device });
        offers.push(UiOffer::new(
            at("reset-board"),
            "reset",
            if facts.reset == (ResetReach::Request { author: false }) {
                reset.disabled(RESET_NEEDS_AUTHOR)
            } else if view.activity.is_some() {
                reset.disabled(RESET_WAITS_FOR_ACTIVITY)
            } else {
                reset
            },
        ));
    }
    for wire in [Escape::Reconnect, Escape::Disconnect] {
        if view.escapes.contains(&wire) {
            offers.push(escape(wire));
        }
    }

    // THE ENTRY
    offers.push(rename_offer(view, at("rename")));
    if facts.face == DeviceFace::Wire {
        offers.push(autoconnect_offer(
            view,
            at("autoconnect"),
            facts.autoconnect,
        ));
    }
    // Not on a board another tab holds (or one this tab let go to it):
    // Forget hands the site's port grant back, which can pull the port from
    // the tab that holds it, and deletes the registry row every tab of this
    // browser shares. It is back once the hold ends.
    if view.escapes.contains(&Escape::Forget) && held_elsewhere.is_none() {
        offers.push(escape(Escape::Forget));
    }
    offers
}

/// The path verb an escape lives at.
pub fn escape_verb(escape: Escape) -> &'static str {
    match escape {
        Escape::Cancel => "cancel",
        Escape::Retry => "retry",
        Escape::Reconnect => "reconnect",
        Escape::Disconnect => "disconnect",
        Escape::Forget => "forget",
    }
}

/// `rename`: what Studio calls the device, as one required Text param
/// whose placeholder is what it is called now. The name is Studio's, never
/// written to the board.
fn rename_offer(view: &DeviceView, path: OfferPath) -> UiOffer {
    let device = view.id;
    let set_name = move |name: String| DevicesOp::action_for(Action::SetName { device, name });
    UiOffer::with_params(
        path,
        "edit",
        vec![OfferParam::text(
            RENAME_NAME_PARAM,
            "new name",
            view.title.clone(),
        )],
        OfferBinder::new(move |args: &OfferArgs| {
            Ok(set_name(
                args.text(RENAME_NAME_PARAM).unwrap_or_default().to_string(),
            ))
        }),
        set_name(String::new()),
    )
}

/// `autoconnect`: open this device's port whenever it appears, as one
/// Toggle param whose default is the remembered choice.
fn autoconnect_offer(view: &DeviceView, path: OfferPath, enabled: bool) -> UiOffer {
    let device = view.id;
    let set =
        move |enabled: bool| DevicesOp::action_for(Action::SetAutoconnect { device, enabled });
    UiOffer::with_params(
        path,
        "usb",
        vec![OfferParam::toggle(
            AUTOCONNECT_ENABLED_PARAM,
            "connect automatically",
            enabled,
        )],
        OfferBinder::new(move |args: &OfferArgs| {
            Ok(set(args
                .toggle(AUTOCONNECT_ENABLED_PARAM)
                .unwrap_or(enabled)))
        }),
        set(enabled),
    )
}

/// The icon token an escape is drawn with.
fn escape_icon(escape: Escape) -> &'static str {
    match escape {
        Escape::Cancel => "cancel",
        Escape::Retry => "retry",
        Escape::Reconnect => "connect",
        Escape::Disconnect => "disconnect",
        Escape::Forget => "remove",
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::DeviceId;
    use lpa_devices::view::{ActivityView, FirmwareFace, LoadedProject};

    use super::*;

    #[test]
    fn a_ready_board_offers_its_verbs_in_card_order() {
        let view = ready();
        assert_eq!(
            paths(&device_offers(&view, &facts(DeviceFace::Wire))),
            [
                "push",
                "update-firmware",
                "erase",
                "identify",
                "reset-board",
                "disconnect",
                "rename",
                "autoconnect",
                "forget"
            ]
        );
    }

    #[test]
    fn the_lasting_verbs_arm_and_the_rest_are_routine() {
        let mut view = ready();
        view.loaded_project = LoadedProject::Running {
            label: "porch".to_string(),
        };
        view.can_remove_project = true;
        let offers = device_offers(&view, &facts(DeviceFace::Wire));
        for offer in &offers {
            let verb = offer.path.last().unwrap();
            let lasting = matches!(
                verb,
                "forget" | "erase" | "remove-project" | "update-firmware" | "push"
            );
            assert_eq!(offer.consequence().arms(), lasting, "{verb}");
        }
    }

    /// Z4: a board another tab holds offers no verb that would open its
    /// port — not Connect, Retry or Identify; the controller offers
    /// `take-over` there instead. When the fact clears they are back.
    #[test]
    fn a_board_another_tab_holds_offers_no_verb_that_opens_its_port() {
        let mut view = ready();
        view.status = DeviceStatus::Attached;
        view.escapes = vec![Escape::Retry, Escape::Disconnect, Escape::Forget];
        view.held_elsewhere = Some(lpa_devices::HeldElsewhere {
            via: lpa_devices::HoldVia::Usb,
            level: lpa_devices::HoldLevel::Watching,
            taken_from_here: false,
        });

        let held = paths(&device_offers(&view, &facts(DeviceFace::Wire)));
        for verb in ["connect", "retry", "identify"] {
            assert!(!held.iter().any(|path| path == verb), "{verb}: {held:?}");
        }

        view.held_elsewhere = None;
        let free = paths(&device_offers(&view, &facts(DeviceFace::Wire)));
        for verb in ["connect", "retry"] {
            assert!(free.iter().any(|path| path == verb), "{verb}: {free:?}");
        }
    }

    /// DD35 fix 2: a board another tab holds offers no Forget (the grant and
    /// the registry row are shared with that tab), and one held over USB
    /// nothing that touches its port's firmware or lines. Held over the
    /// network, this tab's own USB port keeps its verbs.
    #[test]
    fn a_board_another_tab_holds_offers_no_forget_and_no_port_verb() {
        let held = |via| {
            Some(lpa_devices::HeldElsewhere {
                via,
                level: lpa_devices::HoldLevel::Watching,
                taken_from_here: false,
            })
        };
        let mut view = ready();
        let free = paths(&device_offers(&view, &facts(DeviceFace::Wire)));
        assert!(free.iter().any(|path| path == "forget"), "{free:?}");

        view.held_elsewhere = held(lpa_devices::HoldVia::Usb);
        let usb = paths(&device_offers(&view, &facts(DeviceFace::Wire)));
        for verb in ["forget", "reset-board", "erase", "update-firmware", "flash"] {
            assert!(!usb.iter().any(|path| path == verb), "{verb}: {usb:?}");
        }
        view.firmware_face = FirmwareFace::Blank;
        let blank = paths(&device_offers(&view, &facts(DeviceFace::Wire)));
        assert!(!blank.iter().any(|path| path == "flash"), "{blank:?}");

        let mut view = ready();
        view.held_elsewhere = held(lpa_devices::HoldVia::Network);
        let network = paths(&device_offers(&view, &facts(DeviceFace::Wire)));
        assert!(!network.iter().any(|path| path == "forget"), "{network:?}");
        for verb in ["reset-board", "erase", "update-firmware"] {
            assert!(
                network.iter().any(|path| path == verb),
                "{verb}: {network:?}"
            );
        }
    }

    #[test]
    fn escapes_appear_by_state() {
        // Busy: Cancel leads, and every verb that needs an idle board waits.
        let mut busy = ready();
        busy.activity = Some(ActivityView {
            kind: lpa_devices::ActivityKind::Identify,
            label: "Identifying…".to_string(),
            percent: None,
            cancellable: true,
            cancel_requested: false,
            layout: None,
            update: None,
        });
        busy.can_receive_project = false;
        busy.escapes = vec![Escape::Cancel, Escape::Disconnect, Escape::Forget];
        assert_eq!(
            paths(&device_offers(&busy, &facts(DeviceFace::Wire))),
            [
                "cancel",
                "reset-board",
                "disconnect",
                "rename",
                "autoconnect",
                "forget"
            ],
            "Reset stays visible on a busy board"
        );
        let offers = device_offers(&busy, &facts(DeviceFace::Wire));
        assert_eq!(
            find(&offers, "reset-board").action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: RESET_WAITS_FOR_ACTIVITY.to_string()
            },
            "but disabled until the activity ends: the model refuses a reset under one"
        );

        // Silent on an open link: Retry stands where Identify would.
        let mut silent = ready();
        silent.status = DeviceStatus::NotResponding;
        silent.firmware_face = FirmwareFace::Unknown;
        silent.can_receive_project = false;
        silent.escapes = vec![Escape::Retry, Escape::Disconnect, Escape::Forget];
        let offers = device_offers(&silent, &facts(DeviceFace::Wire));
        assert_eq!(
            paths(&offers),
            [
                "erase",
                "retry",
                "reset-board",
                "disconnect",
                "rename",
                "autoconnect",
                "forget"
            ]
        );
        let retry = offers
            .iter()
            .find(|o| o.path.last() == Some("retry"))
            .unwrap();
        assert_eq!(
            retry.action.op_as::<DevicesOp>().unwrap().action(),
            &Action::Identify {
                device: DeviceId(7)
            }
        );

        // Remembered, not here: Reconnect and Forget, and the entry's own.
        let mut gone = ready();
        gone.status = DeviceStatus::Offline;
        gone.can_receive_project = false;
        gone.escapes = vec![Escape::Reconnect, Escape::Forget];
        assert_eq!(
            paths(&device_offers(&gone, &facts(DeviceFace::Wire))),
            ["reconnect", "rename", "autoconnect", "forget"]
        );

        // A sim that is off: Reconnect is its Power on, and there is no
        // autoconnect (a sim has no port to appear).
        let offers = device_offers(&gone, &facts(DeviceFace::Sim));
        assert_eq!(paths(&offers), ["reconnect", "rename", "forget"]);
        assert_eq!(offers[0].label(), "Power on");
        assert_eq!(
            offers[0].action.op_as::<DevicesOp>().unwrap().action(),
            &Action::Connect {
                device: DeviceId(7)
            }
        );
    }

    #[test]
    fn a_port_that_is_there_but_closed_offers_connect() {
        let mut closed = ready();
        closed.status = DeviceStatus::Attached;
        closed.can_receive_project = false;
        let offers = device_offers(&closed, &facts(DeviceFace::Wire));
        let connect = offers
            .iter()
            .find(|offer| offer.path.last() == Some("connect"))
            .expect("Connect opens the closed port");
        assert_eq!(connect.label(), "Connect");
        assert!(connect.consequence().is_routine());
    }

    #[test]
    fn rename_binds_its_text_and_autoconnect_its_toggle() {
        let offers = device_offers(&ready(), &facts(DeviceFace::Wire));
        let rename = find(&offers, "rename");
        let crate::OfferParamKind::Text { placeholder, .. } = &rename.params()[0].kind else {
            panic!("{:?}", rename.params());
        };
        assert_eq!(
            placeholder, "Desk C6",
            "prefilled with what it is called now"
        );
        assert!(!rename.is_enabled(), "a rename needs a name");
        let bound = rename
            .press(&OfferArgs::new().with("name", "  Kitchen  "))
            .unwrap();
        assert_eq!(
            bound.op_as::<DevicesOp>().unwrap().action(),
            &Action::SetName {
                device: DeviceId(7),
                name: "Kitchen".to_string()
            }
        );
        assert!(rename.press(&OfferArgs::new().with("name", "  ")).is_err());

        let auto = find(&offers, "autoconnect");
        assert_eq!(
            auto.params()[0].kind,
            crate::OfferParamKind::Toggle { value: false }
        );
        let bound = auto
            .press(&OfferArgs::new().with("enabled", "true"))
            .unwrap();
        assert_eq!(
            bound.op_as::<DevicesOp>().unwrap().action(),
            &Action::SetAutoconnect {
                device: DeviceId(7),
                enabled: true
            }
        );
    }

    /// Over Bluetooth the firmware verbs need the cable (no ROM downloader
    /// behind a GATT service), but Reset does not: the board restarts itself
    /// when asked (`ClientRequest::Reboot`), so Reset is enabled for the
    /// author tier — the tier the board's access gate asks of the request —
    /// and drawn disabled, saying so, for anyone less.
    #[test]
    fn over_bluetooth_the_firmware_verbs_need_usb_but_reset_asks_the_board() {
        let mut view = ready();
        view.firmware_blocked = Some(lpa_devices::view::FIRMWARE_NEEDS_USB.to_string());
        let author = DeviceOfferFacts {
            reset: ResetReach::Request { author: true },
            ..facts(DeviceFace::Wire)
        };
        let offers = device_offers(&view, &author);
        for verb in ["update-firmware", "erase"] {
            assert!(!find(&offers, verb).is_enabled(), "{verb}");
        }
        assert!(
            find(&offers, "reset-board").is_enabled(),
            "the author tier may restart the board over the air"
        );

        let player = DeviceOfferFacts {
            reset: ResetReach::Request { author: false },
            ..facts(DeviceFace::Wire)
        };
        assert_eq!(
            find(&device_offers(&view, &player), "reset-board")
                .action
                .meta()
                .enablement,
            crate::ActionEnablement::Disabled {
                reason: RESET_NEEDS_AUTHOR.to_string()
            },
            "below the author tier the board would refuse, and the card says why"
        );

        let locked = DeviceOfferFacts {
            locked: true,
            ..facts(DeviceFace::Wire)
        };
        assert!(
            !paths(&device_offers(&view, &locked)).contains(&"push".to_string()),
            "a locked link is offered no push"
        );
    }

    /// The update's own reason wins on the USB Update verb when the link
    /// has one; erase keeps the USB reason either way; Reset is a request
    /// on a network link, enabled for the author tier.
    #[test]
    fn the_updates_own_reason_wins_and_erase_still_needs_usb() {
        let own = "the update's own reason";
        let mut view = ready();
        view.firmware_blocked = Some(lpa_devices::view::FIRMWARE_NEEDS_USB.to_string());
        view.update_blocked = Some(own.to_string());
        let offers = device_offers(&view, &facts(DeviceFace::Wire));
        let reason = |verb: &str| match find(&offers, verb).action.meta().enablement.clone() {
            crate::ActionEnablement::Disabled { reason } => reason,
            other => panic!("{verb}: {other:?}"),
        };
        assert_eq!(reason("update-firmware"), own);
        assert_eq!(reason("erase"), lpa_devices::view::FIRMWARE_NEEDS_USB);

        let author = DeviceOfferFacts {
            reset: ResetReach::Request { author: true },
            ..facts(DeviceFace::Wire)
        };
        assert!(find(&device_offers(&view, &author), "reset-board").is_enabled());
    }

    /// An over-the-air update replaces the USB flash at `update-firmware`,
    /// one click; a crashing board trades Factory reset for its repairs.
    #[test]
    fn an_over_the_air_update_takes_the_flashs_place_at_one_click() {
        use super::super::device_update_route::UpdateRoute;
        use super::super::device_update_standing::UpdateStanding;
        use super::super::device_update_version::UpdateVersion;

        let x = UpdateVersion::new("2026.10.03-1");
        let y = UpdateVersion::new("2026.10.05-2");
        let available = DeviceOfferFacts {
            update: UpdateOfferFacts {
                standing: UpdateStanding::Available {
                    board: x.clone(),
                    to: y.clone(),
                },
                route: UpdateRoute::OverTheAir,
                ..Default::default()
            },
            ..facts(DeviceFace::Wire)
        };
        let offers = device_offers(&ready(), &available);
        assert_eq!(
            paths(&offers),
            [
                "push",
                "update-firmware",
                "erase",
                "identify",
                "reset-board",
                "disconnect",
                "rename",
                "autoconnect",
                "forget"
            ]
        );
        let update = find(&offers, "update-firmware");
        assert_eq!(update.label(), "Update");
        assert!(update.consequence().is_routine());
        assert!(matches!(
            update.action.op_as::<DevicesOp>().unwrap().action(),
            Action::Update { .. }
        ));

        let crashing = DeviceOfferFacts {
            update: UpdateOfferFacts {
                standing: UpdateStanding::KeepsCrashing { board: y },
                route: UpdateRoute::OverTheAir,
                choices: vec![crate::InstallChoice {
                    version: UpdateVersion::new("2026.10.07-4"),
                    age: lpa_devices::FirmwareAge::Older,
                    on_board: false,
                    refused: false,
                    own: false,
                    from_file: false,
                    wire: None,
                    stranded_over: None,
                    published_at: None,
                    recent: true,
                }],
                listed: true,
                ..UpdateOfferFacts::default()
            },
            ..facts(DeviceFace::Wire)
        };
        let mut view = ready();
        view.firmware_face = FirmwareFace::CoreOnly {
            version: Some("2026.10.05-2".to_string()),
            state: lpa_devices::UpdateBoardState::EngineCrashing,
        };
        view.can_receive_project = false;
        assert_eq!(
            paths(&device_offers(&view, &crashing)),
            [
                "reinstall-firmware",
                "install-firmware",
                "install-firmware-file",
                "identify",
                "reset-board",
                "disconnect",
                "rename",
                "autoconnect",
                "forget"
            ],
            "no Factory reset: installing is the repair"
        );
    }

    fn find<'a>(offers: &'a [UiOffer], verb: &str) -> &'a UiOffer {
        offers
            .iter()
            .find(|offer| offer.path.last() == Some(verb))
            .unwrap_or_else(|| panic!("no {verb} in {:?}", paths(offers)))
    }

    fn paths(offers: &[UiOffer]) -> Vec<String> {
        offers
            .iter()
            .map(|offer| {
                let prefix = "devices/mac-a0f26287b48c/";
                let path = offer.path.to_string();
                assert!(path.starts_with(prefix), "{path}");
                path[prefix.len()..].to_string()
            })
            .collect()
    }

    fn facts(face: DeviceFace) -> DeviceOfferFacts<'static> {
        static EXAMPLES: std::sync::LazyLock<Vec<UiExampleCard>> = std::sync::LazyLock::new(|| {
            vec![UiExampleCard {
                id: "catalog/plasma".to_string(),
                name: "Plasma".to_string(),
                kind: lpc_model::ProjectKind::General,
                description: String::new(),
            }]
        });
        DeviceOfferFacts {
            prefix: OfferPath::board(&crate::BoardRef::Mac(
                lpa_devices::BoardKey::parse("a0:f2:62:87:b4:8c").unwrap(),
            )),
            face,
            autoconnect: false,
            locked: false,
            reset: ResetReach::Lines,
            banked: false,
            projects: &[],
            examples: &EXAMPLES,
            update: UpdateOfferFacts::default(),
        }
    }

    /// A Ready LightPlayer on an open USB port, idle, its board resolved,
    /// nothing loaded.
    fn ready() -> DeviceView {
        let board = crate::flash_offer(Some("esp32c6")).candidates[0]
            .board_id
            .clone();
        DeviceView {
            id: DeviceId(7),
            title: "Desk C6".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: Some("esp32c6".to_string()),
            board_id: Some(board),
            firmware_face: FirmwareFace::LightPlayer {
                firmware: None,
                wire: lpa_devices::WireVersion::Match,
                age: lpa_devices::FirmwareAge::Unknown,
            },
            remembered_firmware: None,
            degraded: None,
            loaded_project: LoadedProject::Empty,
            engine_fps: None,
            link_counters: None,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            held_elsewhere: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
            update_blocked: None,
            last_update_outcome: None,
        }
    }
}
