//! What a board's radio hears: the answer to a network scan.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// The body of [`crate::server::ServerMsgBody::NetworkScan`], the answer to
/// [`crate::ClientRequest::NetworkScan`].
///
/// An image with no station answers [`Self::Unsupported`], as its status
/// says `station: unsupported` — never an empty list, which would claim the
/// radio listened and heard nothing. A station whose last scan is stale
/// answers [`Self::Scanning`] and starts a new one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkScan {
    /// This firmware cannot scan.
    Unsupported,
    /// The radio is listening now (a scan takes about two seconds): ask
    /// again shortly.
    Scanning,
    /// What the radio heard, strongest first: 2.4 GHz only, hidden networks
    /// omitted. An empty list means the radio listened and heard nothing.
    Heard(Vec<HeardNetwork>),
}

/// One network the radio heard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeardNetwork {
    /// The network name.
    pub ssid: String,
    /// Signal strength in dBm.
    pub rssi: i8,
    /// Whether it asks for a password (`false`: an open network).
    pub secure: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn both_spellings() {
        assert_eq!(
            crate::json::to_string(&NetworkScan::Unsupported).unwrap(),
            r#""unsupported""#
        );
        assert_eq!(
            crate::json::to_string(&NetworkScan::Scanning).unwrap(),
            r#""scanning""#
        );
        assert_eq!(
            crate::json::from_str::<NetworkScan>(r#""scanning""#).unwrap(),
            NetworkScan::Scanning
        );
        let heard = NetworkScan::Heard(vec![HeardNetwork {
            ssid: "lp-walk-net".to_string(),
            rssi: -48,
            secure: true,
        }]);
        let json = crate::json::to_string(&heard).unwrap();
        assert_eq!(
            json,
            r#"{"heard":[{"ssid":"lp-walk-net","rssi":-48,"secure":true}]}"#
        );
        assert_eq!(crate::json::from_str::<NetworkScan>(&json).unwrap(), heard);
    }
}
