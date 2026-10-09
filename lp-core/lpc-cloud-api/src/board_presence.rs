//! A board online at the relay, as `ListBoards` reports it.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// One of the signed-in account's boards that is connected to the relay
/// right now. Presence lives in the relay's memory only: a board that is
/// offline is simply absent, and Studio shows it from its own list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardPresence {
    /// The board's relay id: its MAC as twelve lowercase hex digits
    /// (`10bda3b08e30`). What `/relay/board/<id>` takes.
    pub id: String,
    /// The board's name for people, as it reported it.
    pub label: String,
    /// The device wire version the board speaks.
    pub wire_proto: u32,
    /// Where the board answers on its own network (`192.168.4.20:80`),
    /// when it reported an address. The board's word, unchecked.
    pub lan: Option<String>,
    /// Whether the caller and the board reach the relay from the same
    /// public address — behind the same home router, most likely — so a
    /// session can try the LAN. In local development both are 127.0.0.1,
    /// so this is always true there.
    pub same_network: bool,
    /// When the board registered, f64 epoch seconds.
    pub since: f64,
    /// The relay protocol the board speaks (`1`: the first relay; `2`:
    /// pictures through the cloud). A protocol 1 board sends no pictures,
    /// no firmware and no project.
    pub relay_proto: u16,
    /// The firmware version the board said in its hello (protocol 2 and
    /// later; empty when it said "unknown"). The board's word, unchecked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    /// The name of the project the board plays, as it last reported it
    /// (protocol 2 and later). Absent before it reports one, or when no
    /// project is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// Answers [`crate::request::ListBoards`]: at most
/// [`MAX_LISTED_BOARDS`] boards, oldest registration first.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BoardList {
    /// The account's online boards.
    pub boards: Vec<BoardPresence>,
}

/// The most boards one `ListBoards` answer carries.
pub const MAX_LISTED_BOARDS: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn pinned_json_literal() {
        let list = BoardList {
            boards: vec![BoardPresence {
                id: "10bda3b08e30".to_string(),
                label: "Lamp".to_string(),
                wire_proto: 39,
                lan: Some("192.168.4.20:80".to_string()),
                same_network: true,
                since: 1.5,
                relay_proto: 1,
                firmware: None,
                project: None,
            }],
        };
        let json = serde_json::to_string(&list).unwrap();
        assert_eq!(
            json,
            r#"{"boards":[{"id":"10bda3b08e30","label":"Lamp","wireProto":39,"lan":"192.168.4.20:80","sameNetwork":true,"since":1.5,"relayProto":1}]}"#
        );
        assert_eq!(serde_json::from_str::<BoardList>(&json).unwrap(), list);
    }

    /// v6: a protocol 2 board's firmware and project name.
    #[test]
    fn pinned_json_literal_with_firmware_and_project() {
        let list = BoardList {
            boards: vec![BoardPresence {
                id: "10bda3b08e30".to_string(),
                label: "Lamp".to_string(),
                wire_proto: 39,
                lan: None,
                same_network: false,
                since: 1.5,
                relay_proto: 2,
                firmware: Some("2026.10.09-1".to_string()),
                project: Some("Rocaille".to_string()),
            }],
        };
        let json = serde_json::to_string(&list).unwrap();
        assert_eq!(
            json,
            r#"{"boards":[{"id":"10bda3b08e30","label":"Lamp","wireProto":39,"lan":null,"sameNetwork":false,"since":1.5,"relayProto":2,"firmware":"2026.10.09-1","project":"Rocaille"}]}"#
        );
        assert_eq!(serde_json::from_str::<BoardList>(&json).unwrap(), list);
    }
}
