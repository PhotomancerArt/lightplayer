//! The ONE place `lpc-wire` and `lpa-devices`' minimal mirror meet.
//!
//! `lpa-devices/src/wire.rs` pins the mapping this module implements:
//!
//! | `lpc_wire::WireServerMessage` | [`ServerFrame`] |
//! |---|---|
//! | `ServerMsgBody::Hello` | [`ServerFrameBody::Hello`](lpa_devices::wire::ServerFrameBody::Hello) with [`HelloFacts`] |
//! | `ServerMsgBody::Heartbeat` at id 0 | [`ServerFrameBody::Heartbeat`](lpa_devices::wire::ServerFrameBody::Heartbeat) |
//! | everything else | [`ServerFrameBody::Other`](lpa_devices::wire::ServerFrameBody::Other) with a stable label |
//!
//! and the other direction, [`ClientFrame`] → `lpc_wire::ClientMessage`.
//!
//! This is also where `WIRE_PROTO_VERSION` is read: `lpa-devices` hardcodes
//! no proto number, so [`roster_config`] is the only honest way for an app to
//! obtain a [`RosterConfig`] — a build that speaks proto N must not classify
//! a proto-N device as incompatible because someone re-typed the number.
//!
//! # Why frames are labelled rather than mirrored
//!
//! The model needs four facts from a frame (is it a hello, what proto does it
//! claim, what identity does it carry, was it *some* frame from a live peer).
//! [`ServerFrameBody::Other`](lpa_devices::wire::ServerFrameBody::Other)'s label exists so a journal reads honestly, not
//! so the model can act on it — anything that wants a real response body goes
//! through `lpa-client`, above this seam.

use lpa_devices::identity::{DeviceUid, EndpointKey, MacAddress, PeerIdentity};
use lpa_devices::link::{LinkInfo, UsbIds};
use lpa_devices::roster::RosterConfig;
use lpa_devices::wire::{ClientFrame, ClientFrameBody, HelloFacts, ServerFrame};
use lpc_wire::{
    ClientMessage, ClientRequest, ServerHello, ServerMsgBody, WIRE_PROTO_VERSION, WireServerMessage,
};

use crate::provider::base_mac::normalize_base_mac;
use crate::provider::endpoint::LinkEndpoint;

/// The model's knobs for THIS build: the proto number comes from `lpc-wire`,
/// everything else keeps the model's own product defaults.
///
/// Call this instead of `RosterConfig::default()` anywhere a real device is
/// involved. `RosterConfig::default()`'s `expected_proto` is a fixture value.
pub fn roster_config() -> RosterConfig {
    RosterConfig {
        expected_proto: WIRE_PROTO_VERSION,
        ..RosterConfig::default()
    }
}

/// Static facts about one granted endpoint, in the model's vocabulary.
///
/// The endpoint id is the fingerprint: it is minted per granted port and
/// stays stable for that port's identity, which is exactly what the model's
/// weakest identity rung wants. Web Serial exposes no serial number
/// (`getInfo()` is VID:PID only), so `serial_number` is always `None` here —
/// on native-USB Espressif boards that costs us the free MAC, which the
/// hello then supplies.
pub fn link_info(endpoint: &LinkEndpoint, usb_vid_pid: Option<(u16, u16)>) -> LinkInfo {
    LinkInfo {
        label: endpoint.label.clone(),
        endpoint: EndpointKey(endpoint.id.as_str().to_string()),
        usb: usb_vid_pid.map(|(vendor, product)| UsbIds { vendor, product }),
        serial_number: None,
    }
}

/// Decode one `M!` frame body into a [`ServerFrame`].
///
/// The error is a message, not a type: its only destination is
/// `LinkEvent::Error`, which the fold counts as an anomaly.
pub fn decode_server_frame(frame_json: &str) -> Result<ServerFrame, String> {
    let message: WireServerMessage = lpc_wire::json::from_str(frame_json)
        .map_err(|error| format!("malformed M! frame: {error}"))?;
    Ok(server_frame(&message))
}

/// Map one decoded wire message into the model's frame vocabulary.
pub fn server_frame(message: &WireServerMessage) -> ServerFrame {
    // Correlation ids are `u64` on the wire and `u32` in the model. A real id
    // never approaches the boundary (they are a per-session counter), and
    // saturating keeps a pathological id from ALIASING onto 0 — which is the
    // unsolicited channel and would turn a response into a heartbeat.
    let request_id = u32::try_from(message.id).unwrap_or(u32::MAX);
    match &message.msg {
        ServerMsgBody::Hello(hello) => ServerFrame::hello(request_id, hello_facts(hello)),
        // Identity in a heartbeat is vision R4: the firmware does not stamp
        // one yet, so a mid-stream attach still needs the hello ANSWER to
        // resolve identity.
        ServerMsgBody::Heartbeat { .. } if message.id == 0 => ServerFrame::heartbeat(None),
        other => ServerFrame::other(request_id, body_label(other)),
    }
}

/// The hello facts the fold reads, from the full wire hello.
///
/// `identity.name` stays `None` deliberately: the wire hello carries only
/// `device_uid`. The provisioned name lives in the device's own
/// `/.lp/device.json` and reaches the app through a filesystem read, not
/// through this frame.
pub fn hello_facts(hello: &ServerHello) -> HelloFacts {
    HelloFacts {
        proto: hello.proto,
        identity: PeerIdentity {
            uid: hello.device_uid.as_ref().map(|uid| DeviceUid(uid.clone())),
            // Normalized here rather than trusted: a reported MAC crosses an
            // untyped boundary, and an EUI-64 accepted as a MAC would mint a
            // second identity for the same board (see `normalize_base_mac`).
            mac: hello
                .hardware
                .base_mac
                .as_deref()
                .and_then(normalize_base_mac)
                .map(MacAddress),
            name: None,
        },
        firmware: Some(firmware_label(hello)),
        board_id: hello.hardware.board_id.clone(),
    }
}

/// Display label for the firmware behind a hello ("fw-esp32c6 abc1234").
fn firmware_label(hello: &ServerHello) -> String {
    let build = &hello.build;
    match build.dirty {
        true => format!("{} {} (dirty)", build.package, build.commit),
        false => format!("{} {}", build.package, build.commit),
    }
}

/// Stable, journal-readable name for a non-hello, non-heartbeat frame.
///
/// An exhaustive match on purpose: a new wire response must be named here
/// rather than silently becoming `"Other"`.
fn body_label(body: &ServerMsgBody) -> &'static str {
    match body {
        ServerMsgBody::Hello(_) => "Hello",
        ServerMsgBody::Filesystem(_) => "Filesystem",
        ServerMsgBody::LoadProject { .. } => "LoadProject",
        ServerMsgBody::UnloadProject => "UnloadProject",
        ServerMsgBody::ProjectRead { .. } => "ProjectRead",
        ServerMsgBody::ProjectCommand { .. } => "ProjectCommand",
        ServerMsgBody::ListAvailableProjects { .. } => "ListAvailableProjects",
        ServerMsgBody::ListLoadedProjects { .. } => "ListLoadedProjects",
        ServerMsgBody::StopAllProjects => "StopAllProjects",
        ServerMsgBody::SetLogLevel => "SetLogLevel",
        ServerMsgBody::Log { .. } => "Log",
        ServerMsgBody::Heartbeat { .. } => "Heartbeat",
        ServerMsgBody::Error { .. } => "Error",
    }
}

/// Map one model request into a wire client message.
///
/// `Reboot` has no wire request yet (vision R4 is an explicit non-goal of
/// this milestone) and `Opaque` is a placeholder for round-2 coarse effects,
/// so both are refused here rather than mistranslated. The refusal reaches
/// the model as `LinkEvent::Error`, which is the honest shape: the transport
/// could not do what it was asked.
pub fn client_message(frame: &ClientFrame) -> Result<ClientMessage, String> {
    let msg = match &frame.body {
        ClientFrameBody::Hello => ClientRequest::Hello,
        ClientFrameBody::Reboot => {
            return Err(
                "the wire has no Reboot request yet (vision R4); reset the link instead"
                    .to_string(),
            );
        }
        ClientFrameBody::Opaque { label } => {
            return Err(format!(
                "lpa-link cannot forward an opaque request ({label}): coarse effects go \
                 through lpa-client, not the device link"
            ));
        }
    };
    Ok(ClientMessage {
        id: u64::from(frame.request_id),
        msg,
    })
}

/// One model request as the line a serial wire carries (`M!{json}\n`).
pub fn encode_client_frame(frame: &ClientFrame) -> Result<String, String> {
    let message = client_message(frame)?;
    let json = lpc_wire::json::to_string(&message)
        .map_err(|error| format!("failed to encode {:?}: {error}", frame.body))?;
    Ok(format!("M!{json}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_devices::wire::ServerFrameBody;

    #[test]
    fn a_hello_carries_proto_identity_and_labels_into_the_fold() {
        let json = encoded_hello("dev_2f8a", Some("60:55:F9:0A:0B:0C"), Some("dig-uno"));

        let frame = decode_server_frame(&json).expect("decode");

        assert_eq!(frame.request_id, 7);
        let ServerFrameBody::Hello(hello) = &frame.body else {
            panic!("expected a hello, got {:?}", frame.body);
        };
        assert_eq!(hello.proto, WIRE_PROTO_VERSION);
        assert_eq!(hello.identity.uid, Some(DeviceUid("dev_2f8a".to_string())));
        // Normalized to the one spelling identity uses: the reporter's
        // uppercase must not mint a second binding for the same board.
        assert_eq!(
            hello.identity.mac,
            Some(MacAddress("60:55:f9:0a:0b:0c".to_string()))
        );
        assert_eq!(hello.board_id.as_deref(), Some("dig-uno"));
        assert!(hello.label().contains("dig-uno"), "{}", hello.label());
    }

    #[test]
    fn a_nonsense_mac_leaves_the_device_anonymous_rather_than_inventing_one() {
        let json = encoded_hello("dev_2f8a", Some("ff:ff:ff:ff:ff:ff"), None);

        let frame = decode_server_frame(&json).expect("decode");

        let ServerFrameBody::Hello(hello) = &frame.body else {
            panic!("expected a hello");
        };
        assert_eq!(hello.identity.mac, None, "a failed efuse read is not an id");
    }

    #[test]
    fn an_id_zero_heartbeat_is_the_unsolicited_channel() {
        let heartbeat = WireServerMessage::new(0, heartbeat_body());

        let frame = server_frame(&heartbeat);

        assert_eq!(frame.request_id, 0);
        assert!(matches!(frame.body, ServerFrameBody::Heartbeat { .. }));
        assert_eq!(frame.identity(), None, "identity in a heartbeat is R4");
    }

    #[test]
    fn every_other_response_is_live_peer_evidence_with_an_honest_label() {
        let frame = server_frame(&WireServerMessage::new(4, ServerMsgBody::StopAllProjects));

        assert_eq!(frame.request_id, 4);
        assert!(
            matches!(&frame.body, ServerFrameBody::Other { label } if label == "StopAllProjects"),
            "{:?}",
            frame.body
        );
    }

    /// A correlated response must never alias onto the unsolicited channel:
    /// that would turn an answer into a heartbeat and hide the answer.
    #[test]
    fn an_out_of_range_request_id_saturates_instead_of_wrapping_to_zero() {
        let frame = server_frame(&WireServerMessage::new(
            u64::from(u32::MAX) + 1,
            ServerMsgBody::UnloadProject,
        ));

        assert_eq!(frame.request_id, u32::MAX);
    }

    #[test]
    fn a_hello_request_round_trips_to_the_line_a_device_answers() {
        let line = encode_client_frame(&ClientFrame::hello(3)).expect("encode");

        assert!(line.starts_with("M!"), "{line}");
        assert!(line.ends_with('\n'), "{line:?}");
        let decoded: ClientMessage =
            lpc_wire::json::from_str(line.trim_start_matches("M!").trim_end()).expect("decode");
        assert_eq!(decoded.id, 3);
        assert!(matches!(decoded.msg, ClientRequest::Hello));
    }

    #[test]
    fn requests_the_wire_cannot_carry_are_refused_rather_than_mistranslated() {
        let reboot = ClientFrame {
            request_id: 1,
            body: ClientFrameBody::Reboot,
        };
        let opaque = ClientFrame {
            request_id: 2,
            body: ClientFrameBody::Opaque {
                label: "Flash".to_string(),
            },
        };

        assert!(encode_client_frame(&reboot).is_err());
        assert!(encode_client_frame(&opaque).is_err());
    }

    #[test]
    fn the_config_this_crate_hands_the_model_speaks_this_builds_proto() {
        assert_eq!(roster_config().expected_proto, WIRE_PROTO_VERSION);
    }

    fn encoded_hello(uid: &str, base_mac: Option<&str>, board_id: Option<&str>) -> String {
        let hello = ServerHello {
            proto: WIRE_PROTO_VERSION,
            build: lpc_wire::BuildFacts {
                features: Vec::new(),
                package: "fw-esp32c6".to_string(),
                commit: "abc1234".to_string(),
                dirty: false,
                profile: "release-esp32".to_string(),
            },
            hardware: lpc_wire::HardwareFacts {
                base_mac: base_mac.map(str::to_string),
                board_id: board_id.map(str::to_string),
                ..Default::default()
            },
            device_uid: Some(uid.to_string()),
        };
        lpc_wire::json::to_string(&WireServerMessage::new(7, ServerMsgBody::Hello(hello)))
            .expect("encode")
    }

    fn heartbeat_body() -> ServerMsgBody {
        ServerMsgBody::Heartbeat {
            fps: lpc_wire::server::SampleStats {
                avg: 0.0,
                sdev: 0.0,
                min: 0.0,
                max: 0.0,
            },
            frame_count: 0,
            loaded_projects: Vec::new(),
            uptime_ms: 0,
            memory: None,
            recovery: None,
            outputs: None,
        }
    }
}
