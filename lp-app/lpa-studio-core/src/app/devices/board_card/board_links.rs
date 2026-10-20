//! [`BoardLinks`]: what the card's four link rows know beyond the link in
//! use. The card's input already carries the link this tab reaches the board
//! by (`BoardCardInput::link`), its Wi‑Fi facts while that link is open
//! (`wifi`), its LAN address (`lan`), a network connect (`wifi_connect`) and
//! its access (`access`). Each link row speaks on its own, so it also needs
//! the facts that outlive a link — what the board last said about its Wi‑Fi
//! and its Bluetooth, which an offline card keeps saying, dimmed (P13) — and
//! who is signed in, which lightplayer.app's row names, and whether the
//! account's list holds the board, online at lightplayer.app or not.
//!
//! Joined by the controller (`DeviceRosterView`): the network controller
//! keeps each board's last status while it is on the roster, the access
//! controller its last access listing, and the access controller holds the
//! signed-in account's keys (Studio core does not know the cloud; the web
//! hands the keys in at sign-in, `AccessCommand::AccountKeys`).

use lpc_wire::server::NetworkStatus;

use crate::app::devices::account_boards::AccountBoardNote;
use crate::app::devices::ui_cloud_board::UiCloudBoard;

/// See the module doc.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoardLinks<'a> {
    /// The signed-in account as lightplayer.app's row names it, its email
    /// ([`account_words`]); `None` while nobody is signed in.
    pub account: Option<&'a str>,
    /// The board's network status as it last answered, kept across a
    /// dropped link: the Wi‑Fi and lightplayer.app rows' last values.
    pub remembered_network: Option<&'a NetworkStatus>,
    /// Bluetooth on or off, as the board's access file last said.
    pub remembered_bluetooth: Option<bool>,
    /// The account's list holds the board (`ListAccountBoards`), online at
    /// lightplayer.app or not, with its last picture there. `None` while it
    /// is not listed, or nobody is signed in.
    pub cloud: Option<&'a UiCloudBoard>,
    /// What lightplayer.app's last answer about the account's list left to
    /// say on this card (a full list, a Remove from your account that did
    /// not go through).
    pub account_note: Option<&'a AccountBoardNote>,
}

impl BoardLinks<'_> {
    /// The account as lightplayer.app's row names it: "hypher@gmail.com".
    pub fn account_words(&self) -> Option<String> {
        self.account.map(str::to_string)
    }
}

/// How lightplayer.app's row names the signed-in account: its email, the
/// name a person signed in with (Yona's phone, 10-19: "Yona's account" read
/// as a promise). A cache from before the email was kept falls back to the
/// label the account's key carries on every board ("Yona's account"), and
/// to "your account" when the account has no name either.
pub fn account_words(name: &str, email: &str) -> String {
    match (email.trim(), name.trim()) {
        ("", "") => "your account".to_string(),
        ("", name) => format!("{name}'s account"),
        (email, _) => email.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_is_named_by_its_email() {
        assert_eq!(
            account_words("Yona", "hypher@gmail.com"),
            "hypher@gmail.com"
        );
        assert_eq!(account_words("Yona", ""), "Yona's account");
        assert_eq!(account_words("  ", ""), "your account");
        assert_eq!(BoardLinks::default().account_words(), None);
    }
}
