//! The Devices page (`/devices`, vision D9): the runtime roster.
//!
//! Two sections, from two different places:
//!
//! - **Runtimes** — the live simulator's card, while a sim session exists
//!   (D36). Unchanged by the device-model rebuild; the sim is not a device
//!   (D22) and keeps its own path through round 1.
//! - **Devices** — the `lpa-devices` roster's projection: one card per known
//!   device, one entry per link still being identified, and one button to ask
//!   the browser for another port.
//!
//! The device half renders `RosterView` DIRECTLY. There is no `Ui*` mirror of
//! it, on purpose: the projection is already a pure function of the fold, so
//! this page cannot show a state the model is not in — which is the whole
//! class of bug (two cards for one board, a stale verdict, a vanished danger
//! zone) the rebuild exists to end.
//!
//! Setup, flashing and pushing are round 2. Where they belong, the page says
//! so and disables the control rather than hiding it or, worse, offering a
//! button that does nothing.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceAction, DeviceRosterView, DevicesOp, UiAction, UiHomeView, split_roster,
};

use crate::app::home::device_roster_card::{DeviceRosterCard, PendingLinkCard};
use crate::app::home::sim_card::SimCard;
use crate::app::home::{device_grid_class, section_title_class};
use crate::core::ActionButton;

/// The runtime roster page (roadmap M4's gallery top, re-homed).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DevicesPage(home: UiHomeView, on_action: EventHandler<UiAction>) -> Element {
    let devices = home.devices.clone();
    // The names of boards the roster split as remembered (D7: an offline
    // device is a quiet line, not a card). P2 only builds the split; P7
    // wires its real tiles (Reconnect/Forget) into this page — this reads
    // just the titles so the page keeps compiling meanwhile.
    let remembered: Vec<String> = split_roster(&devices)
        .remembered
        .into_iter()
        .map(|entry| entry.title)
        .collect();
    // The registry's rows rehydrate into the roster, so the remembered list is
    // only worth showing when the roster has nothing — a store that has not
    // mounted yet, or rows the model could not key. Otherwise it would just
    // repeat the cards above it.
    let show_remembered = devices.roster.devices.is_empty() && !remembered.is_empty();

    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-7",
            if let Some(issue) = home.issue.clone() {
                div { class: "tw:flex tw:items-center tw:gap-3 tw:rounded-md tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-4 tw:py-2.5 tw:text-sm tw:text-status-error-foreground",
                    span { "{issue.message}" }
                }
            }

            // The live simulator, while a session is running (D36: its
            // card exists exactly as long as the session does).
            if let Some(card) = home.sim.clone() {
                section { class: "tw:grid tw:gap-3",
                    header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                        h2 { class: section_title_class(), "Runtimes" }
                    }
                    div { class: device_grid_class(),
                        SimCard { key: "{card.render_key()}", card, on_action }
                    }
                }
            }

            section { class: "tw:grid tw:gap-3",
                header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                    h2 { class: section_title_class(), "Devices" }
                }

                if !devices.transport_available {
                    UnavailableNote {}
                }

                if devices.transport_available {
                    div { class: device_grid_class(),
                        // Pending links come first: a board just plugged in is
                        // what the user is looking at.
                        for pending in devices.roster.pending.iter().cloned() {
                            PendingLinkCard {
                                key: "pending-{pending.link.0}",
                                pending,
                                on_action,
                            }
                        }
                        for card in devices.roster.devices.iter().cloned() {
                            DeviceRosterCard {
                                key: "device-{card.id.0}",
                                // The running face's Open needs the
                                // device's editor address (its registry
                                // uid); a board still identifying has none.
                                open_uid: devices.open_addresses.get(&card.id.0).cloned(),
                                card,
                                // The empty face's picker reads the SAME two
                                // lists the gallery does — there is no
                                // separate device-side project source.
                                projects: home.projects.clone(),
                                examples: home.examples.clone(),
                                on_action,
                            }
                        }
                        // Adding lives IN the roster, at the insertion point
                        // (the house rule: add buttons sit where the new
                        // entry will appear, never in headers).
                        AddDeviceCard { on_action }
                    }
                }

                if show_remembered {
                    RememberedList { names: remembered }
                }
            }
        }
    }
}

/// The roster's add slot: a card in the grid where the next device's card
/// will appear. It doubles as the empty state — same slot, same copy, same
/// layout whether it is the first board or the fifth (clear minimalism,
/// G1 ruling) — so there is no separate empty-state block to jump around.
///
/// The CTA wears the default Solid/Primary tier: the Outline override was
/// the round-1 dodge for the too-bold gradient fill, and the spike gate
/// (2026-08-31, "1F for the primary") made Primary the spectrum outline
/// the slot wanted all along.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AddDeviceCard(on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-40 tw:flex-col tw:items-center tw:justify-center tw:gap-3 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:bg-transparent tw:px-5 tw:py-6",
            // The invitation is transport-OPEN: connecting is the goal, and
            // the USB specifics live on the verb below ("It's plugged in"),
            // so a network path can join later as a sibling verb rather
            // than a rewrite.
            p { class: "tw:m-0 tw:max-w-56 tw:text-center tw:text-xs tw:leading-relaxed tw:text-muted-foreground",
                "Connect a LightPlayer board to control\u{a0}it."
            }
            ActionButton {
                action: DevicesOp::action_for(DeviceAction::AddFromUsb),
                running: false,
                on_action,
            }
        }
    }
}

/// No transport: this build (or this browser) cannot reach a USB port at all.
///
/// Said out loud, because an empty roster with no explanation reads as "you
/// have no devices" — a different and wrong claim.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UnavailableNote() -> Element {
    rsx! {
        div { class: note_class(),
            p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-strong-foreground",
                "This browser can't talk to USB devices"
            }
            p { class: "tw:m-0 tw:max-w-prose tw:text-xs tw:leading-relaxed tw:text-subtle-foreground",
                "Studio reaches boards over Web Serial, which Chrome, Edge and \
                 other Chromium browsers support. The simulator works everywhere."
            }
        }
    }
}

/// Registry rows the roster has not rehydrated (no store mounted yet).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RememberedList(names: Vec<String>) -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-1",
            p { class: "tw:m-0 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                "Remembered"
            }
            ul { class: "tw:m-0 tw:grid tw:list-none tw:gap-0.5 tw:p-0",
                for name in names.iter() {
                    li { key: "{name}",
                        class: "tw:m-0 tw:font-mono tw:text-xs tw:text-muted-foreground",
                        "{name}"
                    }
                }
            }
        }
    }
}

fn note_class() -> &'static str {
    "tw:grid tw:gap-2 tw:rounded-md tw:border tw:border-dashed tw:border-border tw:px-4 tw:py-5"
}

/// A page-shaped summary of what the roster is showing, for tests and
/// fallback renderers.
///
/// Keeping it here rather than in a test module means the page's own claims
/// ("identifying", "no devices yet") are asserted against the same values the
/// components read.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the view-test seam; the page renders the DTOs directly"
    )
)]
pub(crate) fn devices_page_lines(devices: &DeviceRosterView) -> Vec<String> {
    if !devices.transport_available {
        return vec!["This browser can't talk to USB devices".to_string()];
    }
    if devices.roster.devices.is_empty() && devices.roster.pending.is_empty() {
        return vec!["No devices yet".to_string()];
    }
    devices
        .roster
        .pending
        .iter()
        .map(|pending| format!("{} — {}", pending.title, pending.state_label))
        .chain(
            devices
                .roster
                .devices
                .iter()
                .map(|card| format!("{} — {}", card.title, card.state_label)),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{DeviceRosterConfig, DeviceView, RosterView};

    fn view(roster: RosterView, transport_available: bool) -> DeviceRosterView {
        DeviceRosterView {
            roster,
            transport_available,
            open_addresses: Default::default(),
        }
    }

    /// A host build (or a Firefox) has no transport, and the page says that
    /// rather than showing an empty roster that reads as "you have none".
    #[test]
    fn no_transport_says_so_instead_of_showing_an_empty_roster() {
        let lines = devices_page_lines(&view(
            RosterView {
                devices: Vec::new(),
                pending: Vec::new(),
            },
            false,
        ));

        assert_eq!(lines, vec!["This browser can't talk to USB devices"]);
    }

    #[test]
    fn a_working_transport_with_nothing_on_it_invites_a_port() {
        let lines = devices_page_lines(&view(
            RosterView {
                devices: Vec::new(),
                pending: Vec::new(),
            },
            true,
        ));

        assert_eq!(lines, vec!["No devices yet"]);
    }

    /// A fresh plug is an "identifying…" entry BEFORE it is a card — and it
    /// is listed first, because it is what the user is looking at.
    #[test]
    fn a_pending_link_reads_as_identifying_and_comes_first() {
        let mut roster = lpa_studio_core::DeviceRoster::new(DeviceRosterConfig::default());
        roster.load_records(&[lpa_studio_core::app::places::RegisteredDevice {
            uid: "dev0000000000000001".to_string(),
            name: "Porch sign".to_string(),
            ..Default::default()
        }]);
        roster.handle(
            lpa_studio_core::DeviceMillis(0),
            lpa_studio_core::DeviceInput::Event(lpa_devices_event_attach()),
        );

        let lines = devices_page_lines(&view(
            roster.view(lpa_studio_core::DeviceMillis(0)).roster,
            true,
        ));

        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("identifying"), "{lines:?}");
        assert!(lines[1].contains("Porch sign"), "{lines:?}");
    }

    /// Every card offers a way out, in every state — the renderer's half of
    /// invariant I3.
    #[test]
    fn every_rendered_card_has_at_least_one_escape() {
        let cards: Vec<DeviceView> = {
            let mut roster = lpa_studio_core::DeviceRoster::new(DeviceRosterConfig::default());
            roster.load_records(&[lpa_studio_core::app::places::RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                ..Default::default()
            }]);
            roster.view(lpa_studio_core::DeviceMillis(0)).roster.devices
        };

        for card in cards {
            assert!(!card.escapes.is_empty(), "{card:?}");
        }
    }

    fn lpa_devices_event_attach() -> lpa_studio_core::DeviceEvent {
        lpa_studio_core::DeviceEvent::LinkAttached {
            link: lpa_studio_core::DeviceLinkId(1),
            info: lpa_studio_core::DeviceLinkInfo {
                label: "usb-1".to_string(),
                endpoint: lpa_studio_core::DeviceEndpointKey("usb-1".to_string()),
                usb: None,
                serial_number: None,
            },
        }
    }
}
