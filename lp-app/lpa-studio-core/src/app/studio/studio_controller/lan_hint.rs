//! The one reader of what the account's list says about a board's own
//! network ([`crate::LanHint`]): its address there, and whether this
//! computer shares that network. Connect's road (`connect_flow.rs`) and the
//! move onto Wi‑Fi (`lan_move_flow.rs`) read it here and nowhere else, so a
//! change to the list's rows changes only this file.
//!
//! The address is held in memory only, for the board it names, while the
//! list names the board as online: it is never written to the
//! remembered-address book.

use lpa_devices::BoardKey;

use super::StudioController;

impl StudioController {
    /// What the account's list says about `key`'s network, while it lists
    /// the board as online at lightplayer.app (`ListAccountBoards`' presence);
    /// `None` otherwise, an offline row included.
    pub(super) fn account_lan_hint(&self, key: BoardKey) -> Option<crate::LanHint> {
        let online = self.account_boards.row(key)?.online.as_ref()?;
        Some(crate::LanHint {
            lan: online.lan.clone(),
            same_network: online.same_network,
        })
    }
}
