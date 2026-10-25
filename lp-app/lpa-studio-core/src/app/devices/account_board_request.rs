//! What core asks lightplayer.app to change on the account's board list,
//! and how it answered. Studio core does not know the cloud: the web edge
//! takes each [`AccountBoardRequest`] (`StudioController::set_on_account_board_request`),
//! makes the call (`ReportBoard`, `ForgetBoard`) and hands the answer back
//! as [`crate::StudioCommand::AccountBoardAnswered`].

/// One change to the account's board list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountBoardRequest {
    /// A board this tab reaches holds the account's current key: put it on
    /// the list, or freshen its record (`ReportBoard`).
    Report(AccountBoardReport),
    /// Remove from your account was pressed: take the board off the list,
    /// on every computer (`ForgetBoard`). The board keeps its keys.
    Forget {
        /// The board's MAC, twelve hex digits.
        id: String,
    },
}

/// The board's own facts, as a Studio linked to it read them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountBoardReport {
    /// The board's MAC, twelve hex digits.
    pub id: String,
    /// The board's own name: the name it was provisioned with, else its
    /// `lp-xxxx` LAN name, as its relay hello says it. Never this browser's
    /// rename.
    pub label: String,
    /// Its catalog model (a board id), when known.
    pub model: Option<String>,
    /// The firmware version it runs, when known.
    pub firmware: Option<String>,
    /// The project it plays, when known.
    pub project: Option<String>,
    /// The link its list was read over.
    pub seen_on: AccountBoardLink,
}

/// A link a Studio finds the account's key over (the relay writes its own).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountBoardLink {
    /// A cable.
    Usb,
    /// Bluetooth.
    Ble,
    /// The board's Wi‑Fi link.
    Lan,
}

/// lightplayer.app's answer to an [`AccountBoardRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountBoardAnswer {
    /// A report landed: the board is on the list.
    Reported {
        /// The board's MAC.
        id: String,
    },
    /// A report was refused: the list holds `max` boards and this one is
    /// not among them.
    ListFull {
        /// The board's MAC.
        id: String,
        /// How many boards a list holds.
        max: u32,
    },
    /// A forget landed: the board is off the list.
    Forgotten {
        /// The board's MAC.
        id: String,
    },
    /// The call failed (lightplayer.app unreachable, or refused it for
    /// another reason), in words.
    Failed {
        /// The request that failed.
        request: AccountBoardRequest,
        /// Why.
        error: String,
    },
}

/// A board's own name when it was provisioned with none: its LAN name,
/// `lp-` and the last two bytes of its MAC, as the board says it in its
/// relay hello (`fw_esp32_common::net::mdns::mdns_label`).
pub fn board_lan_label(mac: [u8; 6]) -> String {
    format!("lp-{:02x}{:02x}", mac[4], mac[5])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unnamed_board_is_called_by_its_lan_name() {
        assert_eq!(
            board_lan_label([0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30]),
            "lp-8e30"
        );
    }
}
