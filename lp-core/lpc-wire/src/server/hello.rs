//! Wire protocol hello: the self-describing bootstrap frame.
//!
//! Every lpa-server embedder sends an unsolicited [`ServerHello`] as the
//! first id-0 frame when its server loop starts serving, and answers
//! [`crate::ClientRequest::Hello`] with the same payload at any time.
//! Clients compare `proto` against [`WIRE_PROTO_VERSION`]; absence of a
//! hello from the peer IS the mismatch signal (pre-hello firmware never
//! sends one). See `docs/adr/2026-07-14-wire-hello-versioning.md`.

use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Wire protocol version spoken by this build of the workspace.
///
/// # Bump rule
///
/// Hand-bump this integer on EVERY breaking wire change from now on:
/// renamed/removed/retyped fields, changed enum variants, changed encoding,
/// changed semantics of an existing message — anything that would make an
/// old peer misread a new frame or vice versa. There is no negotiation and
/// no compatibility shim (see AGENTS.md wire-compat policy): differing
/// versions mean "assume nothing works; upgrade the firmware".
pub const WIRE_PROTO_VERSION: u32 = 1;

/// Unsolicited/boot-time server identity and version report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerHello {
    /// Wire protocol version — compare against [`WIRE_PROTO_VERSION`].
    pub proto: u32,
    /// Firmware/build provenance.
    pub fw: FwProvenance,
    /// Device identity uid (`dev_…`) if the device is stamped.
    ///
    /// Sourced from `/.lp/device.json` at the device's fs ROOT (the
    /// lpa-server base fs): embedders read it at boot for the unsolicited
    /// hello, and `ClientRequest::Hello` answers re-read it, so a
    /// post-stamp request reports the new uid. `None` means unstamped.
    pub device_uid: Option<String>,
}

/// Build provenance of the firmware/server binary answering the hello.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FwProvenance {
    /// Crate/package that embeds the server (`fw-esp32`, `fw-host`, …).
    pub package: String,
    /// Short git commit the binary was built from, or `"unknown"`.
    pub commit: String,
    /// Whether the working tree was dirty at build time.
    pub dirty: bool,
    /// Cargo profile the binary was built with (`release-esp32`, …).
    pub profile: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn server_hello_round_trips_the_wire() {
        let hello = ServerHello {
            proto: WIRE_PROTO_VERSION,
            fw: FwProvenance {
                package: "fw-esp32".to_string(),
                commit: "abc123456789".to_string(),
                dirty: true,
                profile: "release-esp32".to_string(),
            },
            device_uid: Some("dev_0000000000000001".to_string()),
        };
        let json = crate::json::to_string(&hello).unwrap();
        assert!(json.contains("\"proto\":1"));
        assert!(json.contains("\"deviceUid\""));
        let back: ServerHello = crate::json::from_str(&json).unwrap();
        assert_eq!(back, hello);
    }

    #[test]
    fn server_hello_body_round_trips_as_unsolicited_frame() {
        use crate::message::envelope::ServerMessage;
        use crate::server::ServerMsgBody;

        let hello = ServerHello {
            proto: WIRE_PROTO_VERSION,
            fw: FwProvenance {
                package: "fw-host".to_string(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: "debug".to_string(),
            },
            device_uid: None,
        };
        let frame = ServerMessage::new(0, ServerMsgBody::Hello(hello.clone()));
        let json = crate::json::to_string(&frame).unwrap();
        let back: ServerMessage = crate::json::from_str(&json).unwrap();
        assert_eq!(back.id, 0);
        match back.msg {
            ServerMsgBody::Hello(back_hello) => assert_eq!(back_hello, hello),
            other => panic!("expected hello body, got {other:?}"),
        }
    }

    #[test]
    fn hello_request_round_trips() {
        let request = crate::ClientRequest::Hello;
        let json = crate::json::to_string(&request).unwrap();
        assert_eq!(json, "\"hello\"");
        let back: crate::ClientRequest = crate::json::from_str(&json).unwrap();
        assert!(matches!(back, crate::ClientRequest::Hello));
    }
}
