//! `devices/<board>/edit`: show the board's project in the editor.
//!
//! Today's card drew this as an "Open in editor" link to `/device/<uid>`,
//! which was not an offer, so the app agent could not see it and the board
//! card's primary (one offer) could not be it. Here it is an offer, and
//! since "connected" (the board card ADR, §4) it means the editor on the
//! session the home page holds, with nothing reopening —
//! `RuntimeOp::EditDevice`:
//!
//! - on the connected board, while its session shows on its card: show the
//!   editor on it, with no reattach and no fresh read;
//! - on any other ready, running board: connect it first, then show the
//!   editor.
//!
//! The web's lens sync then writes the session's address (`/p/…`, or
//! `/device/<uid>` for a board whose project has none).
//!
//! Offered exactly when the old link was drawn: the board says it runs a
//! project, it is Ready (or Degraded — a faulted show is exactly what the
//! editor is for), its port is open and idle, and it has a registry row (a
//! board still identifying has no honest address). Never while the editor
//! already shows the board, where there is nowhere further to go: that is
//! the studio controller's to know (it holds the session and the place),
//! so it does not ask for this offer then (`connect_flow.rs`).

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{DeviceView, Escape, LoadedProject};

use crate::{OfferPath, RuntimeOp, UiAction, UiOffer};

/// The verb's path segment.
pub const EDIT_VERB: &str = "edit";

/// `devices/<board>/edit` for `view`, under `prefix`, when the board can be
/// shown in the editor: running a project, Ready or Degraded, linked, idle,
/// and registered as `uid`.
pub fn device_edit_offer(
    prefix: &OfferPath,
    view: &DeviceView,
    uid: Option<&str>,
) -> Option<UiOffer> {
    let running = matches!(view.loaded_project, LoadedProject::Running { .. });
    let ready = matches!(view.status, DeviceStatus::Ready | DeviceStatus::Degraded);
    let linked = view.escapes.contains(&Escape::Disconnect);
    let idle = view.activity.is_none();
    uid?;
    (running && ready && linked && idle).then(|| {
        UiOffer::new(
            prefix.clone().child(EDIT_VERB),
            "edit",
            UiAction::from_op(
                RuntimeOp::NODE_ID,
                RuntimeOp::EditDevice { device: view.id },
            )
            .with_label("Edit")
            .with_summary("Open this board's project in the editor.")
            .with_icon("edit"),
        )
    })
}

#[cfg(test)]
mod tests {
    use lpa_devices::DeviceId;
    use lpa_devices::view::FirmwareFace;

    use super::*;

    #[test]
    fn a_ready_running_registered_board_offers_edit() {
        let offer = device_edit_offer(&prefix(), &running(), Some("devabc")).expect("offered");
        assert_eq!(offer.path.to_string(), "devices/mac-a0f26287b48c/edit");
        assert_eq!(offer.label(), "Edit");
        assert_eq!(offer.icon, "edit");
        assert!(offer.consequence().is_routine());
        assert!(offer.is_enabled());
        assert_eq!(
            offer.action.op_as::<RuntimeOp>(),
            Some(&RuntimeOp::EditDevice {
                device: DeviceId(7)
            })
        );
    }

    #[test]
    fn a_degraded_board_is_still_opened_in_the_editor() {
        let mut view = running();
        view.status = DeviceStatus::Degraded;
        assert!(device_edit_offer(&prefix(), &view, Some("devabc")).is_some());
    }

    #[test]
    fn edit_waits_for_a_running_ready_idle_registered_board() {
        let cases: [(&str, fn(&mut DeviceView)); 4] = [
            ("nothing on it", |view| {
                view.loaded_project = LoadedProject::Empty
            }),
            ("not ready", |view| view.status = DeviceStatus::Attached),
            ("not linked", |view| view.escapes = vec![Escape::Forget]),
            ("busy", |view| {
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
        ];
        for (why, change) in cases {
            let mut view = running();
            change(&mut view);
            assert!(
                device_edit_offer(&prefix(), &view, Some("devabc")).is_none(),
                "{why}"
            );
        }
        assert!(
            device_edit_offer(&prefix(), &running(), None).is_none(),
            "no registry row, no address"
        );
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
