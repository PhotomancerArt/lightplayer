//! Where each board Studio has met is on Wi‑Fi: its address, as the board
//! last said it, kept in THIS browser (`lp.devices.wifi-addresses.v1`).
//!
//! A browser cannot browse the LAN for boards, so Studio remembers what a
//! board said about itself: whenever a board's Wi‑Fi status — read over any
//! link, USB, Bluetooth or Wi‑Fi — says `connected { ip, host }`, its
//! address is learned here, by the board's MAC. A board this browser knows
//! an address for, and holds no link to, is offered "Connect over Wi‑Fi"
//! on its card (`devices/<board>/connect-wifi`). The card's Forget forgets
//! the address with the board.
//!
//! A convenience, not saved data: it is NOT in `/registry.json` and never
//! reaches the account. Core holds the book and says when it changed; the
//! web edge reads it from `localStorage` at boot and writes it back
//! (`lpa-studio-web/src/wifi_addresses_io.rs`), every access in try/catch.
//! A page whose storage is blocked starts with an empty book and still works.
//!
//! The stored form, one object keyed by the board's MAC (12 lowercase hex):
//!
//! ```json
//! { "a0f26287b48c": { "ip": "192.168.1.40", "host": "lp-b48c.local", "seenAt": 1791234567.5 } }
//! ```
//!
//! An entry that does not read is skipped, never fatal: the book is a
//! guess at where a board is, and a bad guess costs one "Couldn't reach".

use std::collections::BTreeMap;

use lpa_devices::BoardKey;
use serde::{Deserialize, Serialize};

use super::lan_addresses::normalize_lan_address;

/// The `localStorage` key the web edge keeps the book under.
pub const WIFI_ADDRESSES_STORAGE_KEY: &str = "lp.devices.wifi-addresses.v1";

/// Where one board was on Wi‑Fi, as it last said.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiAddress {
    /// Dotted IPv4, as the board's status gave it.
    pub ip: String,
    /// The board's name on the LAN (`lp-b48c.local`).
    pub host: String,
    /// When Studio learned it, in epoch seconds (the caller's clock).
    pub seen_at: f64,
}

impl WifiAddress {
    /// The socket Studio dials for this board: `ws://<ip>/link`. The IP, not
    /// the `.local` name: an address the board reported needs no resolver.
    pub fn url(&self) -> Option<String> {
        normalize_lan_address(&self.ip).ok()
    }
}

/// Every board's address this browser knows, by MAC.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WifiAddressBook {
    entries: BTreeMap<BoardKey, WifiAddress>,
}

impl WifiAddressBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// The book as the web edge stored it. Anything that does not read — the
    /// whole document, or one entry — is left out.
    pub fn from_json(json: &str) -> Self {
        let Ok(raw) = serde_json::from_str::<BTreeMap<String, serde_json::Value>>(json) else {
            return Self::default();
        };
        let entries = raw
            .into_iter()
            .filter_map(|(key, value)| {
                let key = BoardKey::parse(&key).ok()?;
                let address = serde_json::from_value::<WifiAddress>(value).ok()?;
                address.url().is_some().then_some((key, address))
            })
            .collect();
        Self { entries }
    }

    /// The book as the web edge stores it.
    pub fn to_json(&self) -> String {
        let raw: BTreeMap<String, &WifiAddress> = self
            .entries
            .iter()
            .map(|(key, address)| (key.to_string(), address))
            .collect();
        serde_json::to_string(&raw).unwrap_or_else(|_| "{}".to_string())
    }

    /// Learn that the board `key` is at `ip` (`host` on the LAN), as of
    /// `seen_at`. Answers whether the book changed — an address the board
    /// keeps across a reboot moves only its time, which is still a change
    /// worth writing (the time says how fresh the guess is). An `ip` that is
    /// not an address is ignored.
    pub fn learn(&mut self, key: BoardKey, ip: &str, host: &str, seen_at: f64) -> bool {
        let address = WifiAddress {
            ip: ip.trim().to_string(),
            host: host.trim().to_string(),
            seen_at,
        };
        if address.ip.is_empty() || address.url().is_none() {
            return false;
        }
        if self.entries.get(&key) == Some(&address) {
            return false;
        }
        self.entries.insert(key, address);
        true
    }

    /// Forget the board `key`'s address (the card's Forget). Answers whether
    /// it had one.
    pub fn forget(&mut self, key: &BoardKey) -> bool {
        self.entries.remove(key).is_some()
    }

    /// The board `key`'s address, when this browser knows one.
    pub fn get(&self, key: &BoardKey) -> Option<&WifiAddress> {
        self.entries.get(key)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_learned_address_round_trips_through_its_stored_form() {
        let mut book = WifiAddressBook::new();
        assert!(book.learn(key("a0f26287b48c"), "192.168.1.40", "lp-b48c.local", 10.5));
        let json = book.to_json();
        assert_eq!(
            json,
            r#"{"a0f26287b48c":{"ip":"192.168.1.40","host":"lp-b48c.local","seenAt":10.5}}"#
        );
        let back = WifiAddressBook::from_json(&json);
        assert_eq!(back, book);
        assert_eq!(
            back.get(&key("a0:f2:62:87:b4:8c"))
                .and_then(WifiAddress::url),
            Some("ws://192.168.1.40/link".to_string())
        );
    }

    #[test]
    fn the_same_address_again_changes_nothing_and_a_new_one_replaces_it() {
        let mut book = WifiAddressBook::new();
        let board = key("a0f26287b48c");
        assert!(book.learn(board, "192.168.1.40", "lp-b48c.local", 1.0));
        assert!(!book.learn(board, "192.168.1.40", "lp-b48c.local", 1.0));
        assert!(book.learn(board, "192.168.1.41", "lp-b48c.local", 2.0));
        assert_eq!(book.get(&board).unwrap().ip, "192.168.1.41");
        assert_eq!(book.len(), 1);
    }

    #[test]
    fn forget_drops_the_board_and_only_that_board() {
        let mut book = WifiAddressBook::new();
        book.learn(key("a0f26287b48c"), "10.0.0.5", "lp-b48c.local", 1.0);
        book.learn(key("60550f0a0b0c"), "10.0.0.6", "lp-0b0c.local", 1.0);
        assert!(book.forget(&key("a0f26287b48c")));
        assert!(!book.forget(&key("a0f26287b48c")));
        assert!(book.get(&key("a0f26287b48c")).is_none());
        assert!(book.get(&key("60550f0a0b0c")).is_some());
    }

    #[test]
    fn what_does_not_read_is_left_out_never_fatal() {
        assert!(WifiAddressBook::from_json("not json").is_empty());
        assert!(WifiAddressBook::from_json("[]").is_empty());
        let book = WifiAddressBook::from_json(
            r#"{
                "a0f26287b48c": { "ip": "10.0.0.5", "host": "lp-b48c.local", "seenAt": 1 },
                "not-a-mac": { "ip": "10.0.0.6", "host": "x", "seenAt": 1 },
                "60550f0a0b0c": { "ip": 7 },
                "60550f0a0b0d": { "ip": "http://x/", "host": "x", "seenAt": 1 }
            }"#,
        );
        assert_eq!(book.len(), 1);
        assert!(book.get(&key("a0f26287b48c")).is_some());
    }

    #[test]
    fn an_empty_address_is_not_learned() {
        let mut book = WifiAddressBook::new();
        assert!(!book.learn(key("a0f26287b48c"), " ", "lp-b48c.local", 1.0));
        assert!(book.is_empty());
    }

    fn key(text: &str) -> BoardKey {
        BoardKey::parse(text).unwrap()
    }
}
