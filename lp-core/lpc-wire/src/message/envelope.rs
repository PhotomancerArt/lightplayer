//! Bidirectional message envelope.

use crate::message::client::ClientMessage;
use crate::server::ServerMsgBody as ServerMessagePayload;
use serde::{Deserialize, Serialize};

/// Top-level JSON envelope (`client` / `server`).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Message {
    Client(ClientMessage),
    Server(ServerMessage),
}

/// Server message correlated to a client request id.
///
/// # Streaming envelope
///
/// A single response and a multi-frame stream share one shape. Streaming is an
/// envelope capability, not a project-read one-off:
///
/// - `seq` is the monotonic frame number within one request stream, starting at
///   `0`. Single responses use `0`.
/// - `fin` marks the final message of a request stream. A single response is
///   `fin == true`; every non-final stream frame is `fin == false`.
///
/// Both fields carry serde defaults (`seq = 0`, `fin = true`) and are skipped
/// when default, so a single response encodes byte-identically to the
/// pre-streaming envelope (`{"id":..,"msg":..}`). Only non-final stream frames
/// pay the `"seq":N,"fin":false` bytes; the final stream frame pays `"seq":N`
/// only.
///
/// Use [`ServerMessage::new`] for single responses and unsolicited messages, and
/// [`ServerMessage::stream_frame`] for stream frames, so construction sites never
/// hand-write the defaults.
#[derive(Debug, Serialize, Deserialize)]
pub struct ServerMessage {
    pub id: u64,
    /// Monotonic frame number within one request stream (default `0`).
    #[serde(default, skip_serializing_if = "seq_is_default")]
    pub seq: u32,
    /// Whether this is the final message of the request stream (default `true`).
    #[serde(default = "fin_default", skip_serializing_if = "fin_is_default")]
    pub fin: bool,
    pub msg: ServerMessagePayload,
}

impl ServerMessage {
    /// A single, final response (or unsolicited message): `seq = 0`, `fin = true`.
    ///
    /// This is the degenerate case of the stream: the first matched message is
    /// also the last. Its encoding is byte-identical to the pre-streaming
    /// envelope because both defaulted fields are skipped.
    #[must_use]
    pub fn new(id: u64, msg: ServerMessagePayload) -> Self {
        Self {
            id,
            seq: 0,
            fin: true,
            msg,
        }
    }

    /// One frame of a request stream carrying explicit `seq`/`fin`.
    #[must_use]
    pub fn stream_frame(id: u64, seq: u32, fin: bool, msg: ServerMessagePayload) -> Self {
        Self { id, seq, fin, msg }
    }
}

fn fin_default() -> bool {
    true
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if predicate signature takes a reference"
)]
fn seq_is_default(seq: &u32) -> bool {
    *seq == 0
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if predicate signature takes a reference"
)]
fn fin_is_default(fin: &bool) -> bool {
    *fin
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectReadEvent;
    use crate::server::ServerMsgBody;
    use alloc::vec;

    /// The compatibility lock: a single (default) response must encode
    /// byte-identically to the pre-streaming envelope. `seq = 0` and `fin = true`
    /// are both skipped, so the JSON is exactly `{"id":..,"msg":..}` with no extra
    /// keys. This is the "zero single-response cost" guarantee (E1).
    #[test]
    fn single_response_encodes_byte_identically_to_pre_streaming_envelope() {
        let msg = ServerMessage::new(7, ServerMsgBody::UnloadProject);
        let json = crate::json::to_string(&msg).expect("serialize");
        assert_eq!(json, r#"{"id":7,"msg":"unloadProject"}"#);
    }

    /// A non-final stream frame pays for both `seq` and `fin:false`; the final
    /// stream frame (fin defaulted true) pays only `seq`.
    #[test]
    fn stream_frames_encode_seq_and_fin_only_when_non_default() {
        let non_final =
            ServerMessage::stream_frame(7, 0, false, ServerMsgBody::ProjectRead { events: vec![] });
        let json = crate::json::to_string(&non_final).expect("serialize");
        assert_eq!(
            json,
            r#"{"id":7,"fin":false,"msg":{"projectRead":{"events":[]}}}"#
        );

        let final_frame =
            ServerMessage::stream_frame(7, 2, true, ServerMsgBody::ProjectRead { events: vec![] });
        let json = crate::json::to_string(&final_frame).expect("serialize");
        // seq=2 is non-default so it is encoded; fin=true is default so it is skipped.
        assert_eq!(
            json,
            r#"{"id":7,"seq":2,"msg":{"projectRead":{"events":[]}}}"#
        );
    }

    /// Defaults must round-trip: a message with no `seq`/`fin` keys decodes to
    /// `seq = 0`, `fin = true`.
    #[test]
    fn missing_seq_fin_default_to_zero_and_true() {
        let decoded: ServerMessage =
            crate::json::from_str(r#"{"id":9,"msg":"unloadProject"}"#).expect("decode");
        assert_eq!(decoded.id, 9);
        assert_eq!(decoded.seq, 0);
        assert!(decoded.fin);
    }

    #[test]
    fn stream_frame_round_trips_through_json() {
        let msg = ServerMessage::stream_frame(
            4,
            3,
            false,
            ServerMsgBody::ProjectRead {
                events: vec![ProjectReadEvent::Error {
                    message: "boom".into(),
                }],
            },
        );
        let json = crate::json::to_string(&msg).expect("serialize");
        let decoded: ServerMessage = crate::json::from_str(&json).expect("decode");
        assert_eq!(decoded.id, 4);
        assert_eq!(decoded.seq, 3);
        assert!(!decoded.fin);
        let ServerMsgBody::ProjectRead { events } = decoded.msg else {
            panic!("expected ProjectRead body");
        };
        assert_eq!(events.len(), 1);
    }

    /// `ser-write-json` (the firmware serializer) must also honor skip-defaults so
    /// the on-wire single response matches serde_json's.
    #[cfg(feature = "ser-write-json")]
    #[test]
    fn ser_write_json_single_response_matches_serde_json() {
        let msg = ServerMessage::new(7, ServerMsgBody::UnloadProject);
        let serde_len = crate::json::to_string(&msg).expect("serde").len();
        assert_eq!(crate::ser_write::ser_write_json_len(&msg), serde_len);
    }
}
