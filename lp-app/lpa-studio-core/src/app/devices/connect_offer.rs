//! `devices/<board>/connect` on a board Studio is talking to: its panel,
//! here, on its card.
//!
//! Connect has one meaning (the board card ADR, §4): this board's session,
//! held by the home page, on the board's card. On a ready board that is
//! [`RuntimeOp::ConnectDevice`]: the lens opens the way an address opens
//! it, and the card's bars become the board's panel. A board running
//! nothing has no panel to show, so its Connect is published disabled,
//! "Nothing on it yet", and the project bar's "Add a project" leads.
//!
//! On a board whose port is there but closed, `connect` is the card's own
//! verb ([`device_offers`](super::device_offers)), the same op.
//!
//! WHEN it is offered on a ready board is the studio controller's to
//! decide (it holds the pool, the access sessions and the registry), and it
//! hands the answers over as [`ConnectFacts`]; WHAT it is is decided here.

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{DeviceView, Escape, LoadedProject};

use crate::{OfferPath, RuntimeOp, UiAction, UiOffer};

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
}

/// `devices/<board>/connect` under `prefix` for a board Studio is talking
/// to, when it is Ready (or Degraded: a faulted show still has its panel),
/// linked, idle, granted, registered and not the board this tab's session
/// is on. Enabled while it runs a project; disabled, "Nothing on it yet",
/// while it runs nothing or has not said.
pub fn device_connect_offer(
    prefix: &OfferPath,
    view: &DeviceView,
    facts: &ConnectFacts,
) -> Option<UiOffer> {
    let ready = matches!(view.status, DeviceStatus::Ready | DeviceStatus::Degraded);
    let linked = view.escapes.contains(&Escape::Disconnect);
    let idle = view.activity.is_none();
    if !(ready && linked && idle && facts.granted && facts.registered) || facts.session_on_it {
        return None;
    }
    let action = connect_action(view);
    let action = match view.loaded_project {
        LoadedProject::Running { .. } => action,
        LoadedProject::Empty | LoadedProject::Unknown => action.disabled(NOTHING_ON_IT_YET),
    };
    Some(UiOffer::new(
        prefix.clone().child(CONNECT_VERB),
        facts.icon,
        action,
    ))
}

/// [`RuntimeOp::ConnectDevice`] for `view`'s board.
fn connect_action(view: &DeviceView) -> UiAction {
    UiAction::from_op(
        RuntimeOp::NODE_ID,
        RuntimeOp::ConnectDevice { device: view.id },
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
                device: DeviceId(7)
            })
        );
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
    fn connect_waits_for_a_ready_idle_granted_registered_board_not_already_open() {
        let cases: [(&str, fn(&mut DeviceView, &mut ConnectFacts)); 6] = [
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
            escapes: vec![Escape::Disconnect, Escape::Forget],
        }
    }
}
