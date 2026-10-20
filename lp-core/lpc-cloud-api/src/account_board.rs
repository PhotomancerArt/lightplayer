//! The signed-in account's board list: `ListAccountBoards` reads it,
//! `ReportBoard` adds to it, `ForgetBoard` takes a board off it.
//!
//! The list holds one record per board that ever proved it holds the
//! account's key — at the relay, or to a Studio linked to it — so the board
//! shows on every computer the account signs in on, offline included. A
//! record grants nothing: presence, pictures and routes stay gated by the
//! accounts the board proved to the relay.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::board_presence::BoardPresence;

/// The most boards one account's list holds, the most one
/// `ListAccountBoards` answer carries, and the most boards one account may
/// have online at the relay at once. A board new to a full list does not
/// join it: the relay still lets it online, and a [`ReportBoard`] is
/// refused [`CloudError::BoardListFull`](crate::error::CloudError::BoardListFull).
pub const MAX_ACCOUNT_BOARDS: usize = 64;

/// Answers [`crate::request::ListAccountBoards`]: the account's boards,
/// online ones first (by when they came online), then the rest by when they
/// were last seen, newest first; at most [`MAX_ACCOUNT_BOARDS`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBoardList {
    /// The boards.
    pub boards: Vec<AccountBoard>,
}

/// One board on the signed-in account's list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBoard {
    /// The board's relay id: its MAC as twelve lowercase hex digits
    /// (`10bda3b08e30`).
    pub id: String,
    /// The board's name as the board last reported it.
    pub label: String,
    /// The hardware model, once a Studio reported one (the relay hears
    /// none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The firmware version last reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    /// The name of the project the board last reported playing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// When the board joined the list, f64 epoch seconds.
    pub joined_at: f64,
    /// When each kind of link last saw it.
    pub seen: BoardSeen,
    /// Present while the board is on the relay now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub online: Option<BoardPresence>,
}

/// When each kind of link last saw a board, f64 epoch seconds: `relay`
/// from the relay itself, the others from Studio's reports. Absent: never.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardSeen {
    /// Online at the relay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay: Option<f64>,
    /// A Studio holding the account's key, over USB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb: Option<f64>,
    /// A Studio holding the account's key, over Bluetooth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ble: Option<f64>,
    /// A Studio holding the account's key, over the board's Wi‑Fi link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lan: Option<f64>,
}

impl BoardSeen {
    /// The newest sighting of any kind.
    #[must_use]
    pub fn last_seen(&self) -> Option<f64> {
        [self.relay, self.usb, self.ble, self.lan]
            .into_iter()
            .flatten()
            .reduce(f64::max)
    }
}

/// Which link a Studio found the account's key on. The relay link never
/// reports: the relay writes its own sightings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SeenOn {
    /// A USB (or UART) cable.
    Usb,
    /// Bluetooth.
    Ble,
    /// The board's own Wi‑Fi link.
    Lan,
}

/// A Studio read a board's access list over `seen_on` and found the
/// signed-in account's **current** key there: put the board on the
/// account's list, or freshen its record. The facts are the board's own
/// (its reported name, never a browser's rename). Answered with
/// [`crate::ack::Ack`]; refused
/// [`CloudError::BoardListFull`](crate::error::CloudError::BoardListFull)
/// when the board is new and the list holds [`MAX_ACCOUNT_BOARDS`]. It
/// grants nothing: the report adds a row to the caller's own list only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportBoard {
    /// The board's MAC as twelve hex digits.
    pub id: String,
    /// The board's name as it reports it.
    pub label: String,
    /// The hardware model the board identified as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The firmware version it runs, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    /// The project it is playing; absent when none is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The link the key was found on.
    pub seen_on: SeenOn,
}

/// Take a board off the signed-in account's list, on every computer. The
/// board keeps its keys, and it joins again the next time it reaches the
/// relay with the account's key (or a Studio finds the key on it). Answered
/// with [`crate::ack::Ack`], also when the board was not on the list (a
/// second press after a reload is not an error).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgetBoard {
    /// The board's MAC as twelve hex digits.
    pub id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::CloudRequest;
    use crate::response::CloudResponse;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn pinned_json_literal_list_account_boards() {
        assert_eq!(
            serde_json::to_string(&CloudRequest::ListAccountBoards).unwrap(),
            "\"listAccountBoards\""
        );
    }

    #[test]
    fn pinned_json_literal_account_board_list() {
        let response = CloudResponse::AccountBoardList(AccountBoardList {
            boards: vec![
                AccountBoard {
                    id: "10bda3b08e30".to_string(),
                    label: "Lamp".to_string(),
                    model: None,
                    firmware: Some("2026.10.09-1".to_string()),
                    project: Some("Rocaille".to_string()),
                    joined_at: 1.5,
                    seen: BoardSeen {
                        relay: Some(9.5),
                        ..BoardSeen::default()
                    },
                    online: Some(BoardPresence {
                        id: "10bda3b08e30".to_string(),
                        label: "Lamp".to_string(),
                        wire_proto: 39,
                        lan: None,
                        same_network: true,
                        since: 9.5,
                        relay_proto: 2,
                        firmware: Some("2026.10.09-1".to_string()),
                        project: Some("Rocaille".to_string()),
                    }),
                },
                AccountBoard {
                    id: "020000000001".to_string(),
                    label: "Porch".to_string(),
                    model: Some("XIAO ESP32-C6".to_string()),
                    firmware: None,
                    project: None,
                    joined_at: 0.25,
                    seen: BoardSeen {
                        relay: None,
                        usb: Some(3.0),
                        ble: Some(4.0),
                        lan: None,
                    },
                    online: None,
                },
            ],
        });
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            json,
            r#"{"accountBoardList":{"boards":[{"id":"10bda3b08e30","label":"Lamp","firmware":"2026.10.09-1","project":"Rocaille","joinedAt":1.5,"seen":{"relay":9.5},"online":{"id":"10bda3b08e30","label":"Lamp","wireProto":39,"lan":null,"sameNetwork":true,"since":9.5,"relayProto":2,"firmware":"2026.10.09-1","project":"Rocaille"}},{"id":"020000000001","label":"Porch","model":"XIAO ESP32-C6","joinedAt":0.25,"seen":{"usb":3.0,"ble":4.0}}]}}"#
        );
        assert_eq!(
            serde_json::from_str::<CloudResponse>(&json).unwrap(),
            response
        );
    }

    #[test]
    fn pinned_json_literal_report_board() {
        let request = CloudRequest::ReportBoard(ReportBoard {
            id: "10bda3b08e30".to_string(),
            label: "Lamp".to_string(),
            model: Some("XIAO ESP32-C6".to_string()),
            firmware: Some("2026.10.20-1".to_string()),
            project: None,
            seen_on: SeenOn::Ble,
        });
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"reportBoard":{"id":"10bda3b08e30","label":"Lamp","model":"XIAO ESP32-C6","firmware":"2026.10.20-1","seenOn":"ble"}}"#
        );
        assert_eq!(
            serde_json::from_str::<CloudRequest>(&json).unwrap(),
            request
        );
        for (seen_on, word) in [
            (SeenOn::Usb, "\"usb\""),
            (SeenOn::Ble, "\"ble\""),
            (SeenOn::Lan, "\"lan\""),
        ] {
            assert_eq!(serde_json::to_string(&seen_on).unwrap(), word);
        }
    }

    #[test]
    fn pinned_json_literal_forget_board() {
        let request = CloudRequest::ForgetBoard(ForgetBoard {
            id: "10bda3b08e30".to_string(),
        });
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"forgetBoard":{"id":"10bda3b08e30"}}"#);
        assert_eq!(
            serde_json::from_str::<CloudRequest>(&json).unwrap(),
            request
        );
    }

    #[test]
    fn last_seen_is_the_newest_of_any_kind() {
        let seen = BoardSeen {
            relay: Some(1.0),
            usb: None,
            ble: Some(5.0),
            lan: Some(2.0),
        };
        assert_eq!(seen.last_seen(), Some(5.0));
        assert_eq!(BoardSeen::default().last_seen(), None);
    }
}
