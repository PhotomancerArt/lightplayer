//! [`UiDetailSwitch`]: a row's own on/off switch, at the top of its details
//! beside the row's name. Wi‑Fi, Bluetooth and lightplayer.app are the three
//! a board can turn off, and all three draw it alike — as macOS draws the
//! switch at the top of its Wi‑Fi and Bluetooth menus
//! (`docs/ux/anatomy/board-card.md`, "Hushing a row").
//!
//! The switch is the board's own setting, so the choice holds across
//! browsers and is true. Its press is an offer the tree publishes
//! (`devices/<board>/wifi/enabled`, `…/wifi/cloud-relay`,
//! `…/bluetooth/enabled`), pressed to the other state; the web draws the
//! switch and never builds the op.

use super::UiCardAction;

/// One row's on/off switch. See the module doc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiDetailSwitch {
    /// What it switches, in the row's own name: "Wi‑Fi", "Bluetooth",
    /// "lightplayer.app".
    pub label: String,
    /// The row's icon token (`wifi`, `bluetooth`, `cloud`).
    pub icon: String,
    /// The board's switch, as it last said.
    pub on: bool,
    /// Flipping it: the board's switch offer pressed to `!on`, in its verb
    /// ("Disable Wi‑Fi"), refused with the offer's own reason where it is
    /// refused ("Bluetooth can only be turned off by USB…"). `None` where
    /// this link may not change it: the switch is drawn, dimmed.
    pub flip: Option<UiCardAction>,
    /// A change is on its way to the board (this switch's, or a neighbour's
    /// in the same settings): the flip is refused for a moment. The switch
    /// is dimmed and its hover says why; nothing is added under the line,
    /// so the details hold still (P3). A standing refusal, which is not
    /// waiting, is said under the line.
    pub waiting: bool,
}

impl UiDetailSwitch {
    /// The switch of the row named `label`, drawn with `icon`, standing at
    /// `on`, flipped by `flip`.
    pub fn new(
        label: impl Into<String>,
        icon: impl Into<String>,
        on: bool,
        flip: Option<UiCardAction>,
    ) -> Self {
        Self {
            label: label.into(),
            icon: icon.into(),
            on,
            flip,
            waiting: false,
        }
    }

    /// The same switch, `waiting` on a change ([`Self::waiting`]).
    pub fn waiting(self, waiting: bool) -> Self {
        Self { waiting, ..self }
    }
}
