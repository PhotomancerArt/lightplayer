//! [`UiDetailPanel`]: today's card surfaces that are more than facts and
//! verbs, named, each carrying core's data for it — never an action. The
//! web draws each one inside the details that list it.

use lpa_devices::LinkCounterFacts;
use lpa_devices::evidence::TerminalLine;

use super::ui_bluetooth_switch::UiBluetoothSwitch;
use crate::OfferPath;
use crate::app::access::UiAccessPanel;
use crate::app::devices::device_layout_view::UiLayoutPanel;
use crate::app::network::UiDeviceWifi;

/// One panel inside a bar's (or the status corner's) details.
#[derive(Clone, Debug, PartialEq)]
pub enum UiDetailPanel {
    /// The board's terminal: what it said, what the wire carried and what
    /// Studio did to it, oldest first, and how many lines fell off the
    /// front.
    Terminal {
        lines: Vec<TerminalLine>,
        dropped: u32,
    },
    /// Who has access: the play and edit passwords and the keys (the access
    /// panel), where this link may see them.
    Access(UiAccessPanel),
    /// The board's Bluetooth switch.
    Bluetooth(UiBluetoothSwitch),
    /// The board's Wi‑Fi: its networks, the form, the test, Cloud relay
    /// (the Wi‑Fi panel). Its verbs are offers at `devices/<board>/wifi/…`.
    Wifi(UiDeviceWifi),
    /// Rename: the `rename` offer's one text field, starting at the board's
    /// name now.
    Rename { offer: OfferPath, title: String },
    /// How the link is doing: the board's link counters off its heartbeat.
    LinkCounters(LinkCounterFacts),
    /// The layout question (or its refusal) before a board's files move:
    /// its words and the paths of its Download, Continue and Cancel.
    Layout(UiLayoutPanel),
    /// "Other version…": the install offer's version choice, inline, with
    /// "From a file…" inside it when a custom build can be picked.
    OtherVersion {
        install: OfferPath,
        from_file: Option<OfferPath>,
    },
    /// "Restore from a backup file…": the file picker, and the board's own
    /// base MAC so a mismatched backup can name both boards.
    RestoreFromFile {
        offer: OfferPath,
        current_base_mac: Option<String>,
    },
}

impl UiDetailPanel {
    /// The offers this panel's controls press.
    pub fn offer_paths(&self) -> Vec<&OfferPath> {
        match self {
            UiDetailPanel::Rename { offer, .. } | UiDetailPanel::RestoreFromFile { offer, .. } => {
                vec![offer]
            }
            UiDetailPanel::OtherVersion { install, from_file } => {
                std::iter::once(install).chain(from_file).collect()
            }
            UiDetailPanel::Layout(panel) => std::iter::once(&panel.download)
                .chain(&panel.continue_action)
                .chain(&panel.cancel)
                .collect(),
            UiDetailPanel::Terminal { .. }
            | UiDetailPanel::Access(_)
            | UiDetailPanel::Bluetooth(_)
            | UiDetailPanel::Wifi(_)
            | UiDetailPanel::LinkCounters(_) => Vec::new(),
        }
    }
}
