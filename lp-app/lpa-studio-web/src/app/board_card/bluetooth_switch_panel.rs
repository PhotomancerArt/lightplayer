//! The board's Bluetooth switch, a section of the connection bar's details
//! ([`UiDetailPanel::Bluetooth`], Q39): the row — the icon, "Bluetooth",
//! the switch — with core's sub-line under the name (why it is locked) and
//! core's restart note under the row (the restart that applies a change).
//! Every word is core's ([`UiBluetoothSwitch`]); the toggle still sends the
//! access command, as today's Connections group did (making it an offer is
//! the agentic UI roadmap's).
//!
//! [`UiDetailPanel::Bluetooth`]: lpa_studio_core::UiDetailPanel::Bluetooth

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, DeviceAccessChange, DeviceId, UiBluetoothSwitch};

use crate::app::home::access_fields::Switch;
use crate::base::{StudioIcon, StudioIconName};

/// See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BluetoothSwitchPanel(
    switch: UiBluetoothSwitch,
    device: DeviceId,
    on_access: EventHandler<AccessCommand>,
) -> Element {
    rsx! {
        section { class: SECTION_CLASS,
            div { class: ROW_CLASS,
                span { class: "tw:inline-flex tw:flex-none tw:text-status-live-foreground", aria_hidden: "true",
                    StudioIcon { name: StudioIconName::Bluetooth, size: 15 }
                }
                span { class: "tw:grid tw:min-w-0 tw:flex-1",
                    "Bluetooth"
                    if let Some(sub) = switch.sub.clone() {
                        span { class: "tw:text-[11px] tw:font-medium tw:text-dim-foreground", "{sub}" }
                    }
                }
                Switch {
                    on: switch.on,
                    label: "Bluetooth".to_string(),
                    locked: switch.locked,
                    on_toggle: move |on| on_access.call(AccessCommand::Change {
                        device,
                        change: DeviceAccessChange::SetBluetooth(on),
                    }),
                }
            }
            if let Some(note) = switch.restart_note.clone() {
                p { class: NOTE_CLASS, "{note}" }
            }
        }
    }
}

/// A section of the details card: its divider and padding, no frame.
const SECTION_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-1 tw:border-0 tw:border-t tw:border-solid tw:border-border-muted tw:px-3 tw:py-2 tw:first:border-t-0";

/// The switch's row.
const ROW_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2.5 tw:text-[13px] tw:font-semibold tw:text-strong-foreground";

/// The restart note, under the row, in the warning's ink.
const NOTE_CLASS: &str =
    "tw:m-0 tw:pl-[25px] tw:text-[11.5px] tw:leading-snug tw:text-status-warning-foreground";

#[cfg(test)]
mod tests {
    use super::*;

    /// No box in a box: the switch is a section of the details card.
    #[test]
    fn the_switch_is_a_section_with_no_frame() {
        assert!(SECTION_CLASS.contains("tw:border-t"));
        for class in [SECTION_CLASS, ROW_CLASS, NOTE_CLASS] {
            assert!(!class.contains("rounded"), "{class}");
            assert!(!class.contains("tw:bg-"), "{class}");
        }
    }
}
