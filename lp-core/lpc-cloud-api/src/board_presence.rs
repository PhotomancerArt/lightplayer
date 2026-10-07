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
            }],
        };
        let json = serde_json::to_string(&list).unwrap();
        assert_eq!(
            json,
            r#"{"boards":[{"id":"10bda3b08e30","label":"Lamp","wireProto":39,"lan":"192.168.4.20:80","sameNetwork":true,"since":1.5}]}"#
        );
        assert_eq!(serde_json::from_str::<BoardList>(&json).unwrap(), list);
    }
}
