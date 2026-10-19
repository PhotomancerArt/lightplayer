//! [`BoardLinks`]: what the card's four link rows know beyond the link in
//! use. The card's input already carries the link this tab reaches the board
//! by (`BoardCardInput::link`), its Wi‑Fi facts while that link is open
//! (`wifi`), its LAN address (`lan`), a network connect (`wifi_connect`) and
//! its access (`access`). Each link row speaks on its own, so it also needs
//! the facts that outlive a link — what the board last said about its Wi‑Fi
//! and its Bluetooth, which an offline card keeps saying, dimmed (P13) — and
//! who is signed in, which lightplayer.app's row names.
//!
//! Joined by the controller (`DeviceRosterView`): the network controller
//! keeps each board's last status while it is on the roster, the access
//! controller its last access listing, and the access controller holds the
//! signed-in account's keys (Studio core does not know the cloud; the web
//! hands the keys in at sign-in, `AccessCommand::AccountKeys`).

use lpc_wire::server::NetworkStatus;

/// See the module doc.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoardLinks<'a> {
    /// The signed-in account's name ("Yona"), which lightplayer.app's row
    /// says as "Yona's account"; `None` while nobody is signed in.
    pub account: Option<&'a str>,
    /// The board's network status as it last answered, kept across a
    /// dropped link: the Wi‑Fi and lightplayer.app rows' last values.
    pub remembered_network: Option<&'a NetworkStatus>,
    /// Bluetooth on or off, as the board's access file last said.
    pub remembered_bluetooth: Option<bool>,
}

impl BoardLinks<'_> {
    /// The account as lightplayer.app's row names it: "Yona's account".
    pub fn account_words(&self) -> Option<String> {
        self.account.map(account_words)
    }
}

/// "Yona's account", the label the account's key already carries on every
/// board (`AccountKeys::held_keys`); "your account" when the account has no
/// name.
pub fn account_words(name: &str) -> String {
    match name.trim() {
        "" => "your account".to_string(),
        name => format!("{name}'s account"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_is_named_as_its_key_is() {
        let links = BoardLinks {
            account: Some("Yona"),
            ..BoardLinks::default()
        };
        assert_eq!(links.account_words().as_deref(), Some("Yona's account"));
        assert_eq!(account_words("  "), "your account");
        assert_eq!(BoardLinks::default().account_words(), None);
    }
}
