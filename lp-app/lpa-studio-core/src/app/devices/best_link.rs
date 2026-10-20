//! The roads Connect takes to a board no link of this tab reaches, best
//! first, from facts alone (pure, no IO).
//!
//! | facts | roads |
//! |---|---|
//! | a USB, Bluetooth, LAN or relay link of this tab reaches the board | none: Connect opens its session on that link |
//! | a Wi‑Fi address, the page may try the LAN ([`LanReach`]), and the account's row does not say "online, on another network" | Wi‑Fi first |
//! | signed in, and the page reaches lightplayer.app | then lightplayer.app |
//! | the board's cable can be asked for again | then the cable (a click) |
//!
//! The Wi‑Fi address is the one lightplayer.app says the board gave while
//! its row says the board shares this computer's network
//! ([`LanHint::same_network`]), else the one this browser remembers. The
//! row's address is used in memory only: it is never written to the
//! remembered-address book.

use super::lan_addresses::normalize_lan_address;
use super::lan_reach::LanReach;
use crate::ConnectReach;

/// What the account's list says about a board's own network, while it lists
/// the board as online at lightplayer.app.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LanHint {
    /// Where the board says it answers on its own network
    /// (`192.168.4.20:80`), unchecked.
    pub lan: Option<String>,
    /// The browser and the board reach lightplayer.app from one public
    /// address: they are likely on one network.
    pub same_network: bool,
}

/// The facts the roads are chosen from.
#[derive(Clone, Copy, Debug, Default)]
pub struct BestLinkFacts<'a> {
    /// A USB, Bluetooth, LAN or relay link of this tab reaches the board.
    pub linked_here: bool,
    /// The address this browser remembers for the board on Wi‑Fi (its IP).
    pub remembered: Option<&'a str>,
    /// The account's row, while it lists the board as online.
    pub row: Option<&'a LanHint>,
    /// What this page's browser lets a socket to the LAN do.
    pub lan_reach: LanReach,
    /// This page reaches lightplayer.app and someone is signed in.
    pub relay: bool,
    /// The board's cable can be asked for again (a click).
    pub cable: bool,
}

/// One road to a board.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Road {
    /// Over Wi‑Fi, at `url` (`ws://<host>/link`).
    Wifi { url: String },
    /// Through lightplayer.app.
    Relay,
    /// Its cable (or Bluetooth) again, through the browser's chooser.
    Cable,
}

impl Road {
    /// The road as a Connect names it.
    pub fn reach(&self) -> ConnectReach {
        match self {
            Self::Wifi { .. } => ConnectReach::Wifi,
            Self::Relay => ConnectReach::Relay,
            Self::Cable => ConnectReach::Usb,
        }
    }
}

/// The roads to the board, best first; empty while a link of this tab
/// reaches it.
pub fn roads(facts: &BestLinkFacts<'_>) -> Vec<Road> {
    if facts.linked_here {
        return Vec::new();
    }
    let mut roads = Vec::new();
    let elsewhere = facts.row.is_some_and(|row| !row.same_network);
    if facts.lan_reach.tries_by_itself()
        && !elsewhere
        && let Some(url) = wifi_address(facts)
    {
        roads.push(Road::Wifi { url });
    }
    if facts.relay {
        roads.push(Road::Relay);
    }
    if facts.cable {
        roads.push(Road::Cable);
    }
    roads
}

/// Where the board is on Wi‑Fi, as the socket Studio dials: the row's
/// address while the row says the board shares this network, else the
/// remembered one. Not gated on [`LanReach`] or on the row saying another
/// network: "Connect over Wi‑Fi" pressed by hand dials it all the same.
pub fn wifi_address(facts: &BestLinkFacts<'_>) -> Option<String> {
    let from_row = facts
        .row
        .filter(|row| row.same_network)
        .and_then(|row| row.lan.as_deref())
        .and_then(|lan| normalize_lan_address(lan).ok());
    from_row.or_else(|| {
        facts
            .remembered
            .and_then(|ip| normalize_lan_address(ip).ok())
    })
}

/// Whether, on these facts, a Wi‑Fi try that fails goes on to
/// lightplayer.app.
pub fn relay_follows_wifi(facts: &BestLinkFacts<'_>) -> bool {
    let roads = roads(facts);
    matches!(roads.first(), Some(Road::Wifi { .. })) && roads.contains(&Road::Relay)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW_LAN: &str = "192.168.4.20:80";
    const ROW_URL: &str = "ws://192.168.4.20:80/link";
    const REMEMBERED: &str = "192.168.4.100";
    const REMEMBERED_URL: &str = "ws://192.168.4.100/link";

    fn row(lan: Option<&str>, same_network: bool) -> LanHint {
        LanHint {
            lan: lan.map(str::to_string),
            same_network,
        }
    }

    fn wifi(url: &str) -> Road {
        Road::Wifi {
            url: url.to_string(),
        }
    }

    #[test]
    fn the_whole_rule_table() {
        let same = row(Some(ROW_LAN), true);
        let other = row(Some(ROW_LAN), false);
        let same_no_lan = row(None, true);
        let cases: [(&str, BestLinkFacts<'_>, Vec<Road>); 11] = [
            (
                "linked here: no road at all",
                BestLinkFacts {
                    linked_here: true,
                    remembered: Some(REMEMBERED),
                    row: Some(&same),
                    relay: true,
                    cable: true,
                    ..Default::default()
                },
                vec![],
            ),
            (
                "never met, the row shares this network: its address, then the relay",
                BestLinkFacts {
                    row: Some(&same),
                    relay: true,
                    ..Default::default()
                },
                vec![wifi(ROW_URL), Road::Relay],
            ),
            (
                "never met, the row says another network: the relay",
                BestLinkFacts {
                    row: Some(&other),
                    relay: true,
                    ..Default::default()
                },
                vec![Road::Relay],
            ),
            (
                "the page cannot reach the LAN: the relay",
                BestLinkFacts {
                    row: Some(&same),
                    lan_reach: LanReach::Blocked,
                    relay: true,
                    ..Default::default()
                },
                vec![Road::Relay],
            ),
            (
                "after the browser's prompt is still worth trying",
                BestLinkFacts {
                    row: Some(&same),
                    lan_reach: LanReach::AfterPrompt,
                    relay: true,
                    ..Default::default()
                },
                vec![wifi(ROW_URL), Road::Relay],
            ),
            (
                "remembered, no row (signed out): Wi‑Fi, then the cable",
                BestLinkFacts {
                    remembered: Some(REMEMBERED),
                    cable: true,
                    ..Default::default()
                },
                vec![wifi(REMEMBERED_URL), Road::Cable],
            ),
            (
                "remembered, the row says another network: the relay, then the cable",
                BestLinkFacts {
                    remembered: Some(REMEMBERED),
                    row: Some(&other),
                    relay: true,
                    cable: true,
                    ..Default::default()
                },
                vec![Road::Relay, Road::Cable],
            ),
            (
                "the row shares the network but gives no address: the remembered one",
                BestLinkFacts {
                    remembered: Some(REMEMBERED),
                    row: Some(&same_no_lan),
                    relay: true,
                    ..Default::default()
                },
                vec![wifi(REMEMBERED_URL), Road::Relay],
            ),
            (
                "both addresses: the row's wins while it shares the network",
                BestLinkFacts {
                    remembered: Some(REMEMBERED),
                    row: Some(&same),
                    relay: true,
                    ..Default::default()
                },
                vec![wifi(ROW_URL), Road::Relay],
            ),
            (
                "no address and no row: lightplayer.app",
                BestLinkFacts {
                    relay: true,
                    ..Default::default()
                },
                vec![Road::Relay],
            ),
            ("nothing reaches it", BestLinkFacts::default(), vec![]),
        ];
        for (what, facts, expected) in cases {
            assert_eq!(roads(&facts), expected, "{what}");
        }
    }

    #[test]
    fn the_address_by_hand_ignores_the_reach_and_another_network() {
        let other = row(Some(ROW_LAN), false);
        let facts = BestLinkFacts {
            remembered: Some(REMEMBERED),
            row: Some(&other),
            lan_reach: LanReach::Blocked,
            ..Default::default()
        };
        assert_eq!(wifi_address(&facts).as_deref(), Some(REMEMBERED_URL));
        assert!(roads(&facts).is_empty());
    }

    #[test]
    fn an_address_that_is_not_a_boards_is_no_road() {
        let bad = row(Some("http://nope"), true);
        let facts = BestLinkFacts {
            row: Some(&bad),
            relay: true,
            ..Default::default()
        };
        assert_eq!(roads(&facts), vec![Road::Relay]);
    }

    #[test]
    fn the_relay_follows_wifi_only_when_wifi_leads_and_the_relay_is_there() {
        let same = row(Some(ROW_LAN), true);
        let leads = BestLinkFacts {
            row: Some(&same),
            relay: true,
            ..Default::default()
        };
        assert!(relay_follows_wifi(&leads));
        let alone = BestLinkFacts {
            row: Some(&same),
            ..Default::default()
        };
        assert!(!relay_follows_wifi(&alone));
        let relay_only = BestLinkFacts {
            relay: true,
            ..Default::default()
        };
        assert!(!relay_follows_wifi(&relay_only));
    }

    #[test]
    fn each_road_names_its_reach() {
        assert_eq!(wifi(ROW_URL).reach(), ConnectReach::Wifi);
        assert_eq!(Road::Relay.reach(), ConnectReach::Relay);
        assert_eq!(Road::Cable.reach(), ConnectReach::Usb);
    }
}
