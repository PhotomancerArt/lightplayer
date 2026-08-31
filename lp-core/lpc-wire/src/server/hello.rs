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
/// - 19: `ClientRequest::Reboot` + its `ServerMsgBody::Reboot` ack — the
///   bridge-independent restart the round-2 recovery ladder needs (a CH340
///   board whose grant dies on replug cannot be reset by any signal dance
///   the browser can still run). New variants on BOTH request and response
///   enums: an old firmware cannot decode the request and an old client
///   cannot decode the ack, which is what earns the bump. The device
///   ANSWERS, then resets — the embedder's reset hook fires only after the
///   transport reports the ack written. Landing beside it, and NOT earning
///   a bump on its own: `ServerMsgBody::Heartbeat`'s optional `identity`
///   (uid + base MAC), an additive optional field per the rule below.
/// - 18: display layouts cross the wire PACKED — `ControlLayout2d`
///   serializes as packing spans (`[first_lamp, count, sample_start,
///   sample_stride, radius]` 5-tuples) plus base64 u16le lamp centers,
///   replacing the per-lamp `[i, s, x, y, r]` tuple list (~5.4 B/lamp vs
///   ~75). An old peer cannot decode the new shape and vice versa —
///   changed encoding of an existing message, which is what earns the
///   bump. This is what lets a dome-scale (≤2048-lamp) layout ride one
///   project-read frame instead of being refused as `Unsupported`.
/// - 17: the wire's CUT reaches the read — `OutputFrameEntry` (and its
///   chunk header) gain `placements: Vec<WireOutputPlacement>`, one
///   `(fixture span ↔ wire span)` run per producer, in lamps. Required
///   fields on existing structs: an old peer's entry cannot decode against
///   the new shape, which is what earns the bump. This is the only
///   description of which fixture owns which stretch of a strand — the
///   patch bay's data (D34a), and unavailable to a client that would
///   otherwise have to re-derive auto-flow ordering from the project.
/// - 16: output fragments — `SlotMerge` gains a `fragments` variant and
///   `OutputDef::input` declares it, so every project read now ships a slot
///   shape carrying `"merge": "fragments"`. `SlotSemantics.merge` rides the
///   shape frames (`ProjectReadEvent`'s `SlotShapeEntry`), and an old peer
///   deserializing that enum has no such variant — it cannot decode the
///   output node's shape at all, which is what earns the bump. Additive
///   though the model change reads.
/// - 15: the projection factorization (post-G2 ruling, format v9) —
///   `WireCellProjection` becomes the factored `{shape, mirror, flip}`
///   RECORD over a new `WireProjectionShape` (extrude_x/extrude_y/radial/
///   angular), replacing the per-shape direction payload enums, and
///   `WireProjectionOrigin` loses `consumer_default` (the producer always
///   declares; the fill rung is gone). Both are breaking shape changes on
///   the render-product probe's policy and result metadata.
/// - 14: radial/angular flips (post-G1b ruling) — `WireCellProjection`'s
///   `Radial`/`Angular` variants grow their own direction payloads
///   (`WireRadialDirection` outward/inward, `WireAngularDirection`
///   clockwise/counter_clockwise), changing their encoding from bare
///   strings to tagged objects — the same breaking shape change v13 made
///   to `Extrude`/`Mirror`, for the same reason.
/// - 13: directional projections (G1b ruling 4) — `WireCellProjection`'s
///   `Extrude`/`Mirror` variants grow a `WireProjectionDirection` payload
///   (right/left/down/up), changing their encoding from bare strings to
///   tagged objects on the render-product probe's policy and result
///   metadata. An old peer cannot decode the new cell encoding, which is
///   what earns the bump.
/// - 12: the published output-frame probe —
///   `ProjectProbeRequest::OutputFrame` / `ProjectProbeResult::OutputFrame`
///   (`OutputFrameProbeRequest`, `OutputFrameProbeResult`,
///   `OutputFrameEntry`) plus `ProjectProbeResultHeader::OutputFrame` for
///   its chunked bulk samples. The cheap pull-only read of the frame each
///   output has ALREADY published — no render in the request path, unlike
///   the control-product probe. New enum variants on the probe enums and on
///   the chunk header enum: an old peer cannot decode an `output_frame`
///   frame and an old server cannot answer one, which is what earns the
///   bump, additive though the surface reads.
/// - 11: per-reading shaping on the timebase probe — `WirePhasorRow` gains
///   `readings: Vec<WirePhasorReading>` (consumer node/slot + waveform +
///   phase_offset), the witness data behind the clock face's trace cards.
///   A required field on an existing struct: an old peer's row cannot
///   decode against the new shape, which is what earns the bump.
/// - 10: the timebase probe — `ProjectProbeRequest::Timebase` /
///   `ProjectProbeResult::Timebase` (`TimebaseProbeRequest`,
///   `TimebaseProbeResult`, `WirePhasorRow`, `WirePhasorOrigin`), the
///   read-only listing of the phasors riding a clock's time product. New
///   enum variants on both probe enums: an old peer cannot decode a
///   `timebase` probe frame, and an old server cannot answer one — which is
///   what earns the bump, additive though the surface reads.
/// - 9: panel-state auto-save reaches the wire —
///   `WireProjectCommand::PanelAutoSave` (the P11 toggle, beside
///   `PanelWrite`/`PanelClear`) and `ServerRuntimeStatus.panel_auto_save`,
///   which carries the CURRENT value back on every project read so the
///   module face can render the switch without a dedicated pull.
/// - 8: sink scopes list on the probe surface — `WireBusChannel` rows now
///   include playlist-entry sink scopes (same shape; new content
///   contract), which is what carries a bound panel control's live value
///   and engaged state for entry children. Presentation surfaces filter
///   `scope.is_sink()` rows; probe value resolution refuses values whose
///   winning provider is a sink-scope producer (R2 no-demand).
/// - 7: structured scope on the probe surface — `WireBusChannel` gains
///   `scope` (channels list per scope) and `WireBindingEndpoint::Bus`
///   carries the endpoint's scope; display strings become a client
///   concern. Same train: `WireProjectCommand::PanelWrite`/`PanelClear`
///   (the `(scope, channel)` panel command surface) and
///   `WireBindingOrigin::Panel`.
/// - 6: node kind `project` renamed to `module` — `NodeKind::Module`
///   serializes as `"Module"` in inventory frames, and `TreePath`
///   segments carry `.module` instead of `.project`.
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
///
/// Every entry above is a BREAKING change — that is what earns a bump,
/// because a differing version means "assume nothing works". Purely
/// ADDITIVE optional fields do not: `HardwareFacts`'s chip-identity
/// fields (2026-08-03, landed beside v9) carry `#[serde(default)]` and
/// nothing sets `deny_unknown_fields`, so an old firmware's hello reads
/// as `None` on new Studio and a new firmware's extra fields are ignored
/// by old Studio. Bumping for those would mark every board running
/// current firmware Incompatible in exchange for nothing.
pub const WIRE_PROTO_VERSION: u32 = 19;

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
    /// Device identity uid (`dev…`) if the device is stamped.
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
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareFacts {
    // `Default` is derived deliberately: every field's default is
    // "nothing known" (no service wired, no identity read), which is the
    // honest report for an embedder that cannot answer — and it keeps
    // adding a field from churning every test fixture that builds one.
    /// A radio service is wired on this unit.
    pub radio: bool,
    /// The board manifest's measured total-LED envelope, when it carries
    /// one — a SOFT limit (evidence of what has run clean, never a
    /// refusal; see `lpc-hardware`'s `HwSoftLimits`). `None` from boards
    /// without a record and embedders without a manifest.
    pub total_led_budget: Option<u32>,
    /// A button input service is wired on this unit.
    pub button: bool,
    /// The board id from the unit's loaded board manifest, when it has
    /// one.
    ///
    /// Always `None` today: nothing on the device writes a board identity
    /// yet. It becomes populatable once provisioning writes
    /// `/hardware.json` (board-selection roadmap M5); the field exists so
    /// that lands as a population change, not a wire change.
    pub board_id: Option<String>,
    /// The chip's factory MAC, lowercase colon hex (`aa:bb:cc:dd:ee:ff`).
    ///
    /// THE permanent identity of this unit: burned into efuse at
    /// manufacture, unique per chip, and — unlike the `dev…` uid Studio
    /// stamps into the filesystem — it survives an erase. `None` from
    /// embedders with no efuse to read (the host server, the browser
    /// worker, `lp-cli`).
    ///
    /// Deliberately the BASE address and not a list of per-interface
    /// ones: Wi-Fi STA *is* the base, and SoftAP/BLE are derived from it
    /// by a published rule (local-admin bit; BLE also bumps the last
    /// octet). Shipping the derivations would be redundant and would drift
    /// from what the radios actually use. An interface's own address is
    /// reported only if that interface is genuinely wired — same rule as
    /// [`Self::radio`] and [`Self::button`] — and by the DEVICE, which is
    /// the only honest source once `override_mac_address` exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_mac: Option<String>,
    /// The silicon revision as `major.minor` (e.g. `"0.2"`).
    ///
    /// Hardware fact the unit knows about itself and never used to say —
    /// esptool reports a revision while probing, but the running firmware
    /// stayed silent about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chip_revision: Option<String>,
    /// The IEEE 802.15.4 EUI-64 (Zigbee/Thread), lowercase colon hex.
    ///
    /// A DIFFERENT width from a MAC — 64 bits, not 48: the base MAC plus
    /// the chip's `MAC_EXT` efuse field. Present only on chips that have
    /// an 802.15.4 radio at all (the C6 does; the classic ESP32 has no
    /// `MAC_EXT`), which is why it is reported separately rather than
    /// folded into [`Self::base_mac`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eui64: Option<String>,
}

/// Chip-level identity the server CANNOT derive: it lives in efuse, and
/// only the embedder can read it.
///
/// The counterpart of [`HelloIdentity`] for hardware rather than build
/// provenance. Embedders without efuse (the host server, the browser
/// worker, `lp-cli`) simply never call the setter and report `None`,
/// exactly as they already do for build identity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HardwareIdentity {
    /// See [`HardwareFacts::base_mac`].
    pub base_mac: Option<String>,
    /// See [`HardwareFacts::chip_revision`].
    pub chip_revision: Option<String>,
    /// See [`HardwareFacts::eui64`].
    pub eui64: Option<String>,
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
                ..Default::default()
            },
            device_uid: Some("dev0000000000000001".to_string()),
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
                ..Default::default()
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
                ..Default::default()
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
            .with_device_uid(Some("dev1".to_string()));
        assert_eq!(scripted.proto, WIRE_PROTO_VERSION + 999);
        assert_eq!(scripted.device_uid.as_deref(), Some("dev1"));
        assert_eq!(identity.proto, WIRE_PROTO_VERSION);
    }
}
