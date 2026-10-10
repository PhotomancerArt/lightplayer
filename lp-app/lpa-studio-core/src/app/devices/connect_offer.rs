//! `devices/<board>/connect`: the board's panel, here, on its card.
//!
//! Connect has one meaning (the board card ADR, §4): this board's session,
//! held by the home page, on the board's card —
//! [`RuntimeOp::ConnectDevice`]. The lens opens the way an address opens
//! it, and the card's bars become the board's panel. Connect reaches the
//! board first when it must, then opens the panel, in one press:
//!
//! | Board | Connect |
//! |---|---|
//! | Ready (or Degraded), linked, idle, holds a tier, registered | opens the session; disabled, "Nothing on it yet", while it runs nothing |
//! | the port is there but closed | opens the port, then the session ([`device_offers`](super::device_offers) publishes it, [`connect_port_action`]) |
//! | offline, Studio can reach it | reaches it — Wi‑Fi first, then lightplayer.app, then its cable — then opens the session |
//!
//! Never on the board this tab's session is on (Done is its verb), while a
//! Connect already waits for the board (the card says "Connecting…"), nor
//! on an offline stand-in (a sim, an in-tab board), whose primary is Power
//! on.
//!
//! WHEN it is offered is the studio controller's to decide (it holds the
//! pool, the access sessions, the registry and the held intent), and it
//! hands the answers over as [`ConnectFacts`]; WHAT it is is decided here.

use lpa_devices::DeviceId;
use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{DeviceView, Escape, LoadedProject};

use crate::{ConnectReach, OfferPath, RuntimeOp, UiAction, UiOffer};

/// The verb's path segment.
pub const CONNECT_VERB: &str = "connect";

/// Why Connect waits on a ready board that runs nothing (or has not said):
/// there is no panel to show yet.
pub const NOTHING_ON_IT_YET: &str = "Nothing on it yet";

/// What the controller knows about connecting to one board.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnectFacts {
    /// The board has a registry row: an address a session opens it by. A
    /// board still identifying has none, and no Connect.
    pub registered: bool,
    /// This tab's session is on the board already (connected, or opened
    /// by its address): there is nothing to connect, and Done is the verb.
    pub session_on_it: bool,
    /// The board's link holds a tier, any tier: the play password is
    /// enough to connect. A link that holds nothing is the Unlock's.
    pub granted: bool,
    /// The icon the board's link is drawn with (`usb`, `bluetooth`,
    /// `wifi`, `cloud`), as the card's primary draws it.
    pub icon: &'static str,
    /// How an offline board is reached, when Studio can reach it: the first
    /// of its Wi‑Fi address, lightplayer.app and its cable whose own offer
    /// (`connect-wifi`, `connect-relay`, `reconnect`) is published.
    pub reach: Option<ConnectReach>,
    /// A Connect is already waiting for this board.
    pub waiting: bool,
}

/// `devices/<board>/connect` under `prefix`, for a board that is ready or
/// offline and reachable (see the module's table); `None` otherwise. A
/// closed port's Connect is [`device_offers`](super::device_offers)'.
pub fn device_connect_offer(
    prefix: &OfferPath,
    view: &DeviceView,
    facts: &ConnectFacts,
) -> Option<UiOffer> {
    if facts.session_on_it || facts.waiting || !facts.registered {
        return None;
    }
    let path = prefix.clone().child(CONNECT_VERB);
    if view.status == DeviceStatus::Offline {
        let reach = facts.reach?;
        let action = connect_action(view.id, Some(reach));
        return Some(match reach {
            ConnectReach::Wifi => UiOffer::new(path, "wifi", action),
            ConnectReach::Relay => UiOffer::new(path, "cloud", action),
            // The browser's chooser: a real click or nothing.
            ConnectReach::Usb => UiOffer::new(path, facts.icon, action.needs_user_activation()),
        });
    }
    let ready = matches!(view.status, DeviceStatus::Ready | DeviceStatus::Degraded);
    let linked = view.escapes.contains(&Escape::Disconnect);
    let idle = view.activity.is_none();
    if !(ready && linked && idle && facts.granted) {
        return None;
    }
    let action = connect_action(view.id, None);
    let action = match view.loaded_project {
        LoadedProject::Running { .. } => action,
        LoadedProject::Empty | LoadedProject::Unknown => action.disabled(NOTHING_ON_IT_YET),
    };
    Some(UiOffer::new(path, facts.icon, action))
}

/// What a closed port's Connect presses (the card's own `connect`): open
/// the port, then the board's session — or, while an editor open is held
/// for the board, the port only.
pub fn connect_port_action(device: DeviceId) -> UiAction {
    connect_action(device, None)
}

/// [`RuntimeOp::ConnectDevice`] for `device`.
fn connect_action(device: DeviceId, reach: Option<ConnectReach>) -> UiAction {
    UiAction::from_op(
        RuntimeOp::NODE_ID,
        RuntimeOp::ConnectDevice { device, reach },
    )
}

#[cfg(test)]
mod tests {
    use lpa_devices::DeviceId;
    use lpa_devices::view::FirmwareFace;

    use super::*;

    #[test]
    fn a_ready_running_board_connects_by_its_link() {
        let offer = device_connect_offer(&prefix(), &running(), &facts()).expect("offered");
        assert_eq!(offer.path.to_string(), "devices/mac-a0f26287b48c/connect");
        assert_eq!(offer.label(), "Connect");
        assert_eq!(offer.icon, "usb");
        assert!(offer.is_enabled());
        assert!(offer.consequence().is_routine());
        assert_eq!(
            offer.action.op_as::<RuntimeOp>(),
            Some(&RuntimeOp::ConnectDevice {
                device: DeviceId(7),
                reach: None
            })
        );
        assert!(!offer.action.meta().needs_user_activation);
        let mut degraded = running();
        degraded.status = DeviceStatus::Degraded;
        assert!(
            device_connect_offer(&prefix(), &degraded, &facts())
                .is_some_and(|offer| offer.is_enabled()),
            "a faulted show still has its panel"
        );
    }

    #[test]
    fn a_board_running_nothing_says_so_disabled() {
        for loaded in [LoadedProject::Empty, LoadedProject::Unknown] {
            let mut view = running();
            view.loaded_project = loaded.clone();
            let offer = device_connect_offer(&prefix(), &view, &facts()).expect("published");
            assert_eq!(
                offer.action.meta().enablement,
                crate::ActionEnablement::Disabled {
                    reason: NOTHING_ON_IT_YET.to_string()
                },
                "{loaded:?}"
            );
        }
    }

    #[test]
    fn an_offline_board_is_reached_by_the_road_the_controller_found() {
        let mut view = running();
        view.status = DeviceStatus::Offline;
        view.escapes = vec![Escape::Reconnect, Escape::Forget];
        let reached = |reach| {
            let facts = ConnectFacts { reach, ..facts() };
            device_connect_offer(&prefix(), &view, &facts)
        };
        assert!(reached(None).is_none(), "nothing reaches it");
        for (reach, icon, click) in [
            (ConnectReach::Wifi, "wifi", false),
            (ConnectReach::Relay, "cloud", false),
            (ConnectReach::Usb, "usb", true),
        ] {
            let offer = reached(Some(reach)).expect("reachable");
            assert_eq!(offer.icon, icon, "{reach:?}");
            assert!(offer.is_enabled(), "{reach:?}");
            assert_eq!(
                offer.action.meta().needs_user_activation,
                click,
                "{reach:?}: the chooser needs a real click"
            );
            assert_eq!(
                offer.action.op_as::<RuntimeOp>(),
                Some(&RuntimeOp::ConnectDevice {
                    device: DeviceId(7),
                    reach: Some(reach)
                })
            );
        }
        let waiting = ConnectFacts {
            reach: Some(ConnectReach::Wifi),
            waiting: true,
            ..facts()
        };
        assert!(
            device_connect_offer(&prefix(), &view, &waiting).is_none(),
            "a Connect already waits for it"
        );
    }

    #[test]
    fn a_closed_ports_connect_opens_the_port_then_the_session() {
        assert_eq!(
            connect_port_action(DeviceId(7)).op_as::<RuntimeOp>(),
            Some(&RuntimeOp::ConnectDevice {
                device: DeviceId(7),
                reach: None
            })
        );
        let mut closed = running();
        closed.status = DeviceStatus::Attached;
        assert!(
            device_connect_offer(&prefix(), &closed, &facts()).is_none(),
            "the card's own verb, not published twice"
        );
    }

    #[test]
    fn connect_waits_for_a_ready_idle_granted_registered_board_not_already_open() {
        let cases: [(&str, fn(&mut DeviceView, &mut ConnectFacts)); 7] = [
            ("not ready", |view, _| view.status = DeviceStatus::Attached),
            ("not linked", |view, _| view.escapes = vec![Escape::Forget]),
            ("busy", |view, _| {
                view.activity = Some(lpa_devices::view::ActivityView {
                    kind: lpa_devices::ActivityKind::Push,
                    label: "Sending the project".to_string(),
                    percent: None,
                    cancellable: true,
                    cancel_requested: false,
                    layout: None,
                    update: None,
                })
            }),
            ("locked", |_, facts| facts.granted = false),
            ("no registry row", |_, facts| facts.registered = false),
            ("the session is on it", |_, facts| {
                facts.session_on_it = true
            }),
            ("a Connect waits for it", |_, facts| facts.waiting = true),
        ];
        for (why, change) in cases {
            let mut view = running();
            let mut facts = facts();
            change(&mut view, &mut facts);
            assert!(
                device_connect_offer(&prefix(), &view, &facts).is_none(),
                "{why}"
            );
        }
    }

    fn facts() -> ConnectFacts {
        ConnectFacts {
            registered: true,
            session_on_it: false,
            granted: true,
            icon: "usb",
            reach: None,
            waiting: false,
        }
    }

    fn prefix() -> OfferPath {
        OfferPath::board(&crate::BoardRef::Mac(
            lpa_devices::BoardKey::parse("a0:f2:62:87:b4:8c").unwrap(),
        ))
    }

    fn running() -> DeviceView {
        DeviceView {
            id: DeviceId(7),
            title: "Porch".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: None,
            firmware_face: FirmwareFace::Unknown,
            remembered_firmware: None,
            degraded: None,
            loaded_project: LoadedProject::Running {
                label: "porch".to_string(),
            },
            engine_fps: None,
            link_counters: None,
            can_receive_project: false,
            can_remove_project: true,
            activity: None,
            last_outcome: None,
            last_update_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            update_blocked: None,
            held_elsewhere: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
        }
    }
}
