//! A board's last picture, as the relay keeps it: `BoardPictures` asks for
//! some, `BoardPictureList` answers.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::base64_bytes::Base64Bytes;

/// Ask for the cached pictures of up to
/// [`MAX_LISTED_BOARDS`](crate::MAX_LISTED_BOARDS) boards; the rest of a
/// longer list is ignored. Answered with [`BoardPictureList`]. Only the
/// accounts a board proved read its picture; a guest reads nothing, and an
/// anonymous caller is [`crate::error::CloudError::NotAuthenticated`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardPictures {
    /// The boards, each with the picture the caller already holds.
    pub boards: Vec<KnownPicture>,
    /// Keep these boards fast for a while (the caller is showing them): a
    /// short lease each read renews, so a board falls back to its idle
    /// pace by itself when nobody asks any more.
    pub watch: bool,
}

/// One board a [`BoardPictures`] asks about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownPicture {
    /// The board's relay id (`10bda3b08e30`).
    pub id: String,
    /// The [`BoardPicture::seq`] the caller holds, if any: an answer for a
    /// picture the caller already has leaves its colours out.
    pub seq: Option<u64>,
}

/// Answers [`BoardPictures`]: one entry per board the caller may read that
/// has a picture. A board the caller may not read, or with no picture yet,
/// is simply absent.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardPictureList {
    /// The pictures, in the order asked.
    pub pictures: Vec<BoardPicture>,
}

/// A board's last picture.
///
/// Sample `i` of `colors` is lamp `⌊i·T/count⌋` of the outputs concatenated
/// in order (`T` their lamps' sum, `count` the samples): three bytes each,
/// R, G, B, sRGB display codes, as the board's own picture frame defines
/// them (`lpc_relay::RelayPicture`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardPicture {
    /// The board's relay id.
    pub id: String,
    /// Whether the board is on the relay now. A picture outlives its board
    /// until the cloud next deploys.
    pub online: bool,
    /// Moves on every picture the relay accepts from the board: the
    /// caller's "do I have the latest?".
    pub seq: u64,
    /// When the picture arrived, f64 epoch seconds.
    pub at: f64,
    /// Lamps per output, in the project's tree order.
    pub outputs: Vec<u32>,
    /// R, G, B per sample (base64); absent when the caller's `seq` is this
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<Base64Bytes>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::CloudRequest;
    use crate::response::CloudResponse;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn pinned_json_literal_board_pictures() {
        let request = CloudRequest::BoardPictures(BoardPictures {
            boards: vec![
                KnownPicture {
                    id: "10bda3b08e30".to_string(),
                    seq: None,
                },
                KnownPicture {
                    id: "020000000001".to_string(),
                    seq: Some(7),
                },
            ],
            watch: true,
        });
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"boardPictures":{"boards":[{"id":"10bda3b08e30","seq":null},{"id":"020000000001","seq":7}],"watch":true}}"#
        );
        assert_eq!(
            serde_json::from_str::<CloudRequest>(&json).unwrap(),
            request
        );
    }

    #[test]
    fn pinned_json_literal_board_picture_list() {
        let response = CloudResponse::BoardPictureList(BoardPictureList {
            pictures: vec![
                BoardPicture {
                    id: "10bda3b08e30".to_string(),
                    online: true,
                    seq: 8,
                    at: 1.5,
                    outputs: vec![5, 3],
                    colors: Some(Base64Bytes(vec![0xff, 0, 0, 0, 0xff, 0])),
                },
                BoardPicture {
                    id: "020000000001".to_string(),
                    online: false,
                    seq: 7,
                    at: 0.25,
                    outputs: vec![3],
                    colors: None,
                },
            ],
        });
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            json,
            r#"{"boardPictureList":{"pictures":[{"id":"10bda3b08e30","online":true,"seq":8,"at":1.5,"outputs":[5,3],"colors":"/wAAAP8A"},{"id":"020000000001","online":false,"seq":7,"at":0.25,"outputs":[3]}]}}"#
        );
        assert_eq!(
            serde_json::from_str::<CloudResponse>(&json).unwrap(),
            response
        );
    }
}
