//! A **minimal mirror** of the wire vocabulary the device fold reads.
//!
//! Why a mirror instead of `lpc-wire`: this crate needs four facts from a
//! frame (is it a hello, what proto does it claim, what identity does it
//! carry, was it merely *some* frame from a live peer). `lpc-wire` would
//! bring `lpc-model` — the whole project/slot/tree model — along for those
//! four facts, and the milestone's rule is "do not drag heavy deps in for
//! one type". Mirroring also keeps the dependency inversion honest: the
//! model owns its vocabulary and `lpa-link` adapts to it.
//!
//! **M3 reconciliation:** `lpa-link` maps
//! `lpc_wire::WireServerMessage` → [`ServerFrame`] (hello →
//! [`HelloFacts`], `ServerMessage` id 0 heartbeat → [`ServerFrameBody::Heartbeat`],
//! everything else → [`ServerFrameBody::Other`]) and
//! [`ClientFrame`] → `lpc_wire::ClientRequest`. That adapter is the ONE
//! place the two vocabularies meet; it is also where
//! `WIRE_PROTO_VERSION` is read and handed to
//! [`RosterConfig::expected_proto`](crate::RosterConfig::expected_proto) —
//! this crate deliberately hardcodes no proto number.

use serde::{Deserialize, Serialize};

use crate::identity::PeerIdentity;

/// One decoded frame from the peer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ServerFrame {
    /// Correlation id; `0` is the firmware's unsolicited/heartbeat channel.
    pub request_id: u32,
    pub body: ServerFrameBody,
}

impl ServerFrame {
    pub fn hello(request_id: u32, hello: HelloFacts) -> Self {
        Self {
            request_id,
            body: ServerFrameBody::Hello(hello),
        }
    }

    pub fn heartbeat(identity: Option<PeerIdentity>) -> Self {
        Self {
            request_id: 0,
            body: ServerFrameBody::Heartbeat { identity },
        }
    }

    pub fn other(request_id: u32, label: impl Into<String>) -> Self {
        Self {
            request_id,
            body: ServerFrameBody::Other {
                label: label.into(),
            },
        }
    }

    /// Identity the frame announced, if any.
    pub fn identity(&self) -> Option<&PeerIdentity> {
        match &self.body {
            ServerFrameBody::Hello(hello) => Some(&hello.identity),
            ServerFrameBody::Heartbeat { identity } => identity.as_ref(),
            ServerFrameBody::Other { .. } => None,
        }
    }
}

/// What kind of frame arrived.
///
/// The distinction that matters to the fold is hello / not-hello. A
/// not-hello frame is **live-peer evidence, never a verdict** — a running
/// server heartbeats every 5 s, so a mid-stream attach legitimately sees
/// frames before any hello answer
/// (`docs/defects/2026-08-21-hello-gate-assumes-fresh-boot.md`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ServerFrameBody {
    Hello(HelloFacts),
    Heartbeat { identity: Option<PeerIdentity> },
    Other { label: String },
}

/// The hello facts the model reads. A subset of `lpc_wire::ServerHello`:
/// proto for compatibility, identity for the chain, labels for display.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HelloFacts {
    pub proto: u32,
    pub identity: PeerIdentity,
    /// Firmware package/commit label for display ("fw-esp32c6 abc1234").
    pub firmware: Option<String>,
    /// Board identifier the firmware was built for, when it knows.
    pub board_id: Option<String>,
}

impl HelloFacts {
    /// Short display label: what the user should read on a ready card.
    pub fn label(&self) -> String {
        match (&self.board_id, &self.firmware) {
            (Some(board), Some(firmware)) => format!("{board} · {firmware}"),
            (Some(board), None) => board.clone(),
            (None, Some(firmware)) => firmware.clone(),
            (None, None) => format!("LightPlayer (proto {})", self.proto),
        }
    }
}

/// One frame the model wants sent to the peer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientFrame {
    pub request_id: u32,
    pub body: ClientFrameBody,
}

impl ClientFrame {
    pub fn hello(request_id: u32) -> Self {
        Self {
            request_id,
            body: ClientFrameBody::Hello,
        }
    }
}

/// The client requests the model itself issues. Deliberately tiny: the model
/// asks who a peer is and (round 2) asks it to reboot; every other request
/// belongs to the app above it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClientFrameBody {
    /// `ClientRequest::Hello` — the ONLY thing that can grant a verdict on a
    /// device that is already running (a connect cannot assume the power to
    /// cause a boot, so the unsolicited boot hello may never come).
    Hello,
    /// `ClientRequest::Reboot` (vision R4) — bridge-independent restart,
    /// used by activity recovery in round 2.
    Reboot,
    /// An opaque request forwarded on behalf of a coarse effect; the label
    /// exists so journals read honestly.
    Opaque { label: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{DeviceUid, MacAddress};

    #[test]
    fn a_heartbeat_can_carry_identity_passively() {
        let frame = ServerFrame::heartbeat(Some(PeerIdentity {
            uid: Some(DeviceUid("dev_abc".to_string())),
            mac: Some(MacAddress("aa:bb".to_string())),
            name: None,
        }));

        assert_eq!(frame.request_id, 0);
        assert_eq!(
            frame.identity().and_then(|identity| identity.uid.clone()),
            Some(DeviceUid("dev_abc".to_string()))
        );
    }

    #[test]
    fn an_unrelated_frame_announces_nothing() {
        assert!(ServerFrame::other(7, "UnloadProject").identity().is_none());
    }

    #[test]
    fn hello_labels_prefer_board_and_firmware() {
        let hello = HelloFacts {
            proto: 9,
            board_id: Some("dig-uno".to_string()),
            firmware: Some("fw-esp32c6 abc1234".to_string()),
            ..Default::default()
        };
        assert_eq!(hello.label(), "dig-uno · fw-esp32c6 abc1234");

        let bare = HelloFacts {
            proto: 9,
            ..Default::default()
        };
        assert_eq!(bare.label(), "LightPlayer (proto 9)");
    }
}
