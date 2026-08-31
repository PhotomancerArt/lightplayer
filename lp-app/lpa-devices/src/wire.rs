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
            body: ServerFrameBody::Heartbeat {
                identity,
                loaded: None,
            },
        }
    }

    /// A heartbeat that also reported what the board is running — the fact
    /// the empty/running faces are made of.
    pub fn heartbeat_with_loaded(
        identity: Option<PeerIdentity>,
        loaded: Vec<LoadedProjectFacts>,
    ) -> Self {
        Self {
            request_id: 0,
            body: ServerFrameBody::Heartbeat {
                identity,
                loaded: Some(loaded),
            },
        }
    }

    /// An answer to "what have you got loaded?" — the same fact, asked for
    /// rather than volunteered.
    pub fn loaded_report(request_id: u32, loaded: Vec<LoadedProjectFacts>) -> Self {
        Self {
            request_id,
            body: ServerFrameBody::Loaded { loaded },
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
            ServerFrameBody::Heartbeat { identity, .. } => identity.as_ref(),
            ServerFrameBody::Loaded { .. } | ServerFrameBody::Other { .. } => None,
        }
    }

    /// What the frame said the board is running, when it said anything.
    pub fn loaded(&self) -> Option<&[LoadedProjectFacts]> {
        match &self.body {
            ServerFrameBody::Heartbeat { loaded, .. } => loaded.as_deref(),
            ServerFrameBody::Loaded { loaded } => Some(loaded),
            ServerFrameBody::Hello(_) | ServerFrameBody::Other { .. } => None,
        }
    }
}

/// One project the board reports having loaded.
///
/// Just the storage path, because that is all the wire's heartbeat carries.
/// The board names its own storage dir; the library's name for a project is
/// a JOIN the app makes, never something the device is asked to remember.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LoadedProjectFacts {
    /// Server filesystem path, as reported (`/projects/demo`).
    pub path: String,
}

impl LoadedProjectFacts {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// The storage dir's own name — the last path segment (`demo`). What a
    /// running card can honestly call the thing on the board.
    pub fn label(&self) -> &str {
        let trimmed = self.path.trim_end_matches('/');
        match trimmed.rsplit('/').next() {
            Some(tail) if !tail.is_empty() => tail,
            _ => self.path.as_str(),
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
    Heartbeat {
        identity: Option<PeerIdentity>,
        /// What the board reports running. `None` from firmware that does
        /// not report it; `Some(vec![])` is an explicit "nothing loaded" —
        /// the difference the empty face depends on, so it is never
        /// collapsed.
        #[serde(default)]
        loaded: Option<Vec<LoadedProjectFacts>>,
    },
    /// A `ListLoadedProjects` answer. The one non-hello response body the
    /// mirror decodes rather than labels, because the empty-vs-running face
    /// is made of it.
    Loaded {
        loaded: Vec<LoadedProjectFacts>,
    },
    Other {
        label: String,
    },
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

    pub fn list_loaded(request_id: u32) -> Self {
        Self {
            request_id,
            body: ClientFrameBody::ListLoadedProjects,
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
    /// `ClientRequest::ListLoadedProjects` — "what have you got on you?".
    ///
    /// The third and last thing the model asks a peer itself, and it earns
    /// its place the same way the hello does: the empty and running faces
    /// are made of the answer, and waiting for a heartbeat to volunteer it
    /// would leave a just-pushed card claiming nothing for a heartbeat
    /// period.
    ListLoadedProjects,
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

    /// Only frames that actually CARRY the report answer it. The difference
    /// between "said nothing" and "said nothing is loaded" is what the empty
    /// face rests on, so it is pinned here rather than left to a reader.
    #[test]
    fn only_a_frame_that_reported_says_what_is_loaded() {
        assert_eq!(ServerFrame::hello(1, HelloFacts::default()).loaded(), None);
        assert_eq!(ServerFrame::other(7, "UnloadProject").loaded(), None);
        assert_eq!(
            ServerFrame::heartbeat(None).loaded(),
            None,
            "pre-report firmware volunteers nothing, and that is not 'empty'"
        );
        assert_eq!(
            ServerFrame::heartbeat_with_loaded(None, Vec::new()).loaded(),
            Some(&[][..]),
            "an explicit empty list IS the empty answer"
        );
        assert_eq!(
            ServerFrame::loaded_report(3, vec![LoadedProjectFacts::new("/projects/demo")])
                .loaded()
                .map(<[LoadedProjectFacts]>::len),
            Some(1)
        );
    }

    #[test]
    fn a_loaded_report_labels_itself_by_its_storage_dir() {
        assert_eq!(LoadedProjectFacts::new("/projects/demo").label(), "demo");
        assert_eq!(LoadedProjectFacts::new("projects/demo/").label(), "demo");
        assert_eq!(LoadedProjectFacts::new("demo").label(), "demo");
        // Never empty: a card must render something for whatever arrived.
        assert_eq!(LoadedProjectFacts::new("/").label(), "/");
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
