//! Wire protocol hello: the self-describing bootstrap frame.
//!
//! Every lpa-server embedder sends an unsolicited [`ServerHello`] as the
//! first id-0 frame when its server loop starts serving, and answers
//! [`crate::ClientRequest::Hello`] with the same payload at any time.
//! Clients compare `proto` against [`WIRE_PROTO_VERSION`]; absence of a
//! hello from the peer IS the mismatch signal (pre-hello firmware never
//! sends one). See `docs/adr/2026-07-14-wire-hello-versioning.md`.
//!
//! The payload separates two kinds of truth that used to be blurred:
//! [`BuildFacts`] (what this *binary* is and can do — the runtime
//! projection of the embedded firmware manifest core) and
//! [`HardwareFacts`] (what this *unit* actually has wired). Capability
//! reporting is orthogonal to protocol versioning: it describes runtime
//! abilities, never a wire dialect, and is never negotiated. See
//! `docs/adr/2026-08-01-capability-reporting-on-hello.md`.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::LpFeature;
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
///
/// # History
///
/// - 5: `ServerHello` reshaped into distinct build and hardware facts.
///   `fw: FwProvenance` is gone: its four provenance fields moved onto
///   `build: BuildFacts` alongside the build's `LpFeature` list, and
///   `hardware: HardwareFacts` reports the services this unit has wired
///   (`docs/adr/2026-08-01-capability-reporting-on-hello.md`).
/// - 4: `OutputDriverOptionsConfig.brightness` removed. Brightness is a
///   fixture-level control only; the output node no longer carries one.
/// - 3: merge of the two independent "2" bumps below — one wire surface
///   carrying BOTH the node authoring operations and the runtime node
///   command channel.
/// - 2 (node authoring, PR #154): `WireProjectCommand::CreateNode` /
///   `RemoveNode` plus inventory node entries/origins
///   (`docs/adr/2026-07-27-node-authoring-operations.md`).
/// - 2 (runtime commands, PR #158): `WireProjectCommand::NodeCommand`
///   runtime command channel (playlist activate-entry;
///   `docs/adr/2026-07-27-runtime-node-command-channel.md`).
/// - 1: hello handshake introduced.
pub const WIRE_PROTO_VERSION: u32 = 5;

/// Unsolicited/boot-time server identity, version, and capability report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerHello {
    /// Wire protocol version — compare against [`WIRE_PROTO_VERSION`].
    pub proto: u32,
    /// What this binary is and what it can do.
    pub build: BuildFacts,
    /// What this unit has wired.
    pub hardware: HardwareFacts,
    /// Device identity uid (`dev_…`) if the device is stamped.
    ///
    /// Sourced from `/.lp/device.json` at the device's fs ROOT (the
    /// lpa-server base fs): embedders read it at boot for the unsolicited
    /// hello, and `ClientRequest::Hello` answers re-read it, so a
    /// post-stamp request reports the new uid. `None` means unstamped.
    pub device_uid: Option<String>,
}

/// Build facts of the firmware/server binary answering the hello: its
/// provenance plus the feature set compiled into it.
///
/// This is the runtime projection of the same truth the embedded firmware
/// manifest core carries (`docs/adr/2026-08-01-firmware-manifest-\
/// architecture.md`) — the server derives `features` from
/// `lpc_engine::supported_features()` and its own injected services rather
/// than having them re-stated by each embedder.
///
/// `features` lists only what is PRESENT. A node kind whose feature is
/// absent has no runtime in this build; kinds that are never gated
/// (`Project`, `Output`) map to no feature at all, so a client derives
/// "supported node kinds" via [`LpFeature::for_node_kind`]: `None` means
/// always available, `Some(f)` means available iff `features` contains `f`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildFacts {
    /// Features compiled into this build, in [`LpFeature::ALL`] order.
    pub features: Vec<LpFeature>,
    /// Crate/package that embeds the server (`fw-esp32c6`, `fw-host`, …).
    pub package: String,
    /// Short git commit the binary was built from, or `"unknown"`.
    pub commit: String,
    /// Whether the working tree was dirty at build time.
    pub dirty: bool,
    /// Cargo profile the binary was built with (`release-esp32`, …).
    pub profile: String,
}

/// Hardware facts of the unit answering the hello — runtime truth about
/// what is actually wired, as distinct from what the build could drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareFacts {
    /// A radio service is wired on this unit.
    pub radio: bool,
    /// A button input service is wired on this unit.
    pub button: bool,
    /// The board id from the unit's loaded hardware manifest, when it has
    /// one.
    ///
    /// Always `None` today: nothing on the device writes a board identity
    /// yet. It becomes populatable once provisioning writes
    /// `/hardware.json` (board-selection roadmap M5); the field exists so
    /// that lands as a population change, not a wire change.
    pub board_id: Option<String>,
}

/// The embedder-supplied half of the hello: build identity the server has
/// no way to derive, plus the stamped device uid.
///
/// The other half — [`BuildFacts::features`] and [`HardwareFacts`] — is
/// computed inside the `LpServer` constructor from the engine's own `cfg!`
/// truth and the injected services, deliberately NOT injected here: two
/// embedders (`fw-emu`, `lp-cli`) never supply identity at all and must
/// still report their capabilities correctly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloIdentity {
    /// Wire protocol version to report. Normally [`WIRE_PROTO_VERSION`];
    /// scripted fakes override it to mimic mismatched firmware.
    pub proto: u32,
    /// Crate/package that embeds the server.
    pub package: String,
    /// Short git commit the binary was built from, or `"unknown"`.
    pub commit: String,
    /// Whether the working tree was dirty at build time.
    pub dirty: bool,
    /// Cargo profile the binary was built with.
    pub profile: String,
    /// Stamped device identity uid, when the device has one.
    pub device_uid: Option<String>,
}

impl HelloIdentity {
    /// Build-provenance identity at the compiled-in [`WIRE_PROTO_VERSION`],
    /// with no stamped uid.
    pub fn new(
        package: impl Into<String>,
        commit: impl Into<String>,
        dirty: bool,
        profile: impl Into<String>,
    ) -> Self {
        Self {
            proto: WIRE_PROTO_VERSION,
            package: package.into(),
            commit: commit.into(),
            dirty,
            profile: profile.into(),
            device_uid: None,
        }
    }

    /// Attach the boot-time read of the root-stamped device identity.
    #[must_use]
    pub fn with_device_uid(mut self, device_uid: Option<String>) -> Self {
        self.device_uid = device_uid;
        self
    }

    /// Report a different wire proto than this build speaks — scripted
    /// fakes only, to mimic firmware from an incompatible wire revision.
    #[must_use]
    pub fn with_proto(mut self, proto: u32) -> Self {
        self.proto = proto;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn server_hello_round_trips_the_wire() {
        let hello = ServerHello {
            proto: WIRE_PROTO_VERSION,
            build: BuildFacts {
                features: vec![LpFeature::NodeShader, LpFeature::GfxLpvm],
                package: "fw-esp32c6".to_string(),
                commit: "abc123456789".to_string(),
                dirty: true,
                profile: "release-esp32".to_string(),
            },
            hardware: HardwareFacts {
                radio: true,
                button: true,
                board_id: None,
            },
            device_uid: Some("dev_0000000000000001".to_string()),
        };
        let json = crate::json::to_string(&hello).unwrap();
        assert!(json.contains(&alloc::format!("\"proto\":{WIRE_PROTO_VERSION}")));
        assert!(json.contains("\"deviceUid\""));
        // Features ride the wire as their stable kebab identifiers.
        assert!(json.contains("\"node.shader\""), "{json}");
        assert!(json.contains("\"gfx.lpvm\""), "{json}");
        assert!(json.contains("\"boardId\""), "{json}");
        let back: ServerHello = crate::json::from_str(&json).unwrap();
        assert_eq!(back, hello);
    }

    /// An empty feature list and an absent board id survive the round trip
    /// as themselves — absence is a fact, never a default filled in on
    /// decode.
    #[test]
    fn server_hello_round_trips_empty_capabilities() {
        let hello = ServerHello {
            proto: WIRE_PROTO_VERSION,
            build: BuildFacts {
                features: vec![],
                package: "fw-emu".to_string(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: "release-emu".to_string(),
            },
            hardware: HardwareFacts {
                radio: false,
                button: false,
                board_id: None,
            },
            device_uid: None,
        };
        let json = crate::json::to_string(&hello).unwrap();
        let back: ServerHello = crate::json::from_str(&json).unwrap();
        assert_eq!(back, hello);
    }

    #[test]
    fn server_hello_body_round_trips_as_unsolicited_frame() {
        use crate::message::envelope::ServerMessage;
        use crate::server::ServerMsgBody;

        let hello = ServerHello {
            proto: WIRE_PROTO_VERSION,
            build: BuildFacts {
                features: vec![LpFeature::NodeClock],
                package: "fw-host".to_string(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: "debug".to_string(),
            },
            hardware: HardwareFacts {
                radio: false,
                button: true,
                board_id: None,
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

    /// The embedder half defaults to this build's proto and no uid, and the
    /// builders are the only way to move either.
    #[test]
    fn hello_identity_defaults_to_this_builds_proto() {
        let identity = HelloIdentity::new("fw-host", "unknown", false, "debug");
        assert_eq!(identity.proto, WIRE_PROTO_VERSION);
        assert_eq!(identity.device_uid, None);

        let scripted = identity
            .clone()
            .with_proto(WIRE_PROTO_VERSION + 999)
            .with_device_uid(Some("dev_1".to_string()));
        assert_eq!(scripted.proto, WIRE_PROTO_VERSION + 999);
        assert_eq!(scripted.device_uid.as_deref(), Some("dev_1"));
        assert_eq!(identity.proto, WIRE_PROTO_VERSION);
    }
}
