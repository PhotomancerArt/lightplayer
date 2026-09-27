//! What rides lp-link's proto channel: one wire message per link message.
//!
//! On a device link that runs lp-link (USB since `WIRE_PROTO_VERSION` 30;
//! plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D1/D4) the link
//! delimits, checksums and resends, so a wire message needs no framing of its
//! own: no `M!` prefix, no newline, no COBS. A proto-channel message is
//! exactly one of:
//!
//! - **JSON**: the message's JSON text, first byte `{`. Every client→server
//!   message, and every server→client message on a link that has not opted
//!   into packing (or one the packer could not take).
//! - **Packed**: [`PAYLOAD_TAG_PACKED`] (`'L'`, the byte the `M!`-era COBS
//!   frame kind used) then a learned JSON Pack frame
//!   ([`ser_learned_to`](crate::ser_learned_to): the 3-byte table header, then
//!   the value). Server→client only, on a link whose host opted in
//!   ([`ClientRequest::SetEncoding`](crate::ClientRequest::SetEncoding)).
//!
//! The learned table is per link and resets with it: both ends start a fresh
//! table on every lp-link `Up`/`Reset` (the board goes back to JSON until the
//! host opts in again). Over a reliable link the two tables cannot part, so a
//! packed payload that does not decode is a bug; the reader counts it and
//! restarts the link ([`WireLinkPort`](crate::WireLinkPort)).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use lp_json_pack::{DecodeError, HeaderMismatch, LearnStore, decode_learned};

use crate::WireServerMessage;
use crate::message::client::ClientMessage;
use crate::wire_encoding::{FRAME_KIND_LEARNED, WIRE_SEED};

/// The first byte of a packed server payload.
pub const PAYLOAD_TAG_PACKED: u8 = FRAME_KIND_LEARNED;

/// The first byte of a JSON payload (every wire message is a JSON object).
pub const PAYLOAD_TAG_JSON: u8 = b'{';

/// One server→client wire message off the proto channel, decoded.
///
/// Shaped like the reader's `ReadFrame` in `lpa-link` so hosts hand it on
/// unchanged: the JSON text (what an `M!` line carried, byte for byte), the
/// form it came in, and the typed message.
#[derive(Debug)]
pub struct ServerPayload {
    /// The message's JSON text. For a packed payload, the text it decodes to
    /// (byte-identical to the board's JSON form).
    pub json: String,
    /// Whether it came packed.
    pub packed: bool,
    /// The payload's size on the proto channel (before lp-link's framing).
    pub wire_len: usize,
    /// The typed message, or why the JSON did not parse into one (a board on
    /// another proto sends messages this build does not know; that is the
    /// caller's to report, not a link fault).
    pub message: Result<WireServerMessage, String>,
}

/// Why a proto-channel payload could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadError {
    /// A zero-length message.
    Empty,
    /// The first byte is neither `{` nor [`PAYLOAD_TAG_PACKED`].
    UnknownTag(u8),
    /// A packed payload's table header names another table state than the
    /// reader's (it started mid-session, or the two ends parted).
    OutOfStep(HeaderMismatch),
    /// A packed payload whose header matched and whose body did not decode.
    BadPacked(String),
    /// The text is not UTF-8.
    NotUtf8,
    /// A client payload whose JSON is not a `ClientMessage`.
    BadClientJson(String),
}

impl fmt::Display for PayloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("an empty proto message"),
            Self::UnknownTag(tag) => write!(f, "a proto message of unknown kind 0x{tag:02x}"),
            Self::OutOfStep(HeaderMismatch::Truncated) => {
                f.write_str("a packed message shorter than its table header")
            }
            Self::OutOfStep(HeaderMismatch::Epoch { frame, reader }) => write!(
                f,
                "a packed message in table epoch {frame}, this reader in {reader}"
            ),
            Self::OutOfStep(HeaderMismatch::State { frame, reader }) => write!(
                f,
                "a packed message against table state {frame:04x}, this reader at {reader:04x}"
            ),
            Self::BadPacked(why) => write!(f, "a packed message did not decode: {why}"),
            Self::NotUtf8 => f.write_str("a proto message that is not UTF-8"),
            Self::BadClientJson(why) => write!(f, "a request that did not parse: {why}"),
        }
    }
}

/// Read one server→client proto-channel message. `table` is the link's
/// learned table on this side: fresh at every link `Up`, learning as packed
/// payloads go by. On any error `table` is as it was.
pub fn decode_server_payload(
    bytes: &[u8],
    table: &mut dyn LearnStore,
) -> Result<ServerPayload, PayloadError> {
    let (&tag, body) = bytes.split_first().ok_or(PayloadError::Empty)?;
    let (json, packed) = match tag {
        PAYLOAD_TAG_JSON => (
            String::from_utf8(bytes.to_vec()).map_err(|_| PayloadError::NotUtf8)?,
            false,
        ),
        PAYLOAD_TAG_PACKED => {
            // Packed messages run 3-4x smaller than their JSON; start there.
            let mut out = Vec::with_capacity(body.len() * 4);
            match decode_learned(&WIRE_SEED, table, body, &mut out) {
                Ok(()) => {}
                Err(DecodeError::Learned(mismatch)) => {
                    return Err(PayloadError::OutOfStep(mismatch));
                }
                Err(error) => return Err(PayloadError::BadPacked(format!("{error:?}"))),
            }
            (
                String::from_utf8(out).map_err(|_| PayloadError::NotUtf8)?,
                true,
            )
        }
        other => return Err(PayloadError::UnknownTag(other)),
    };
    let message = crate::json::from_str::<WireServerMessage>(&json).map_err(|e| e.to_string());
    Ok(ServerPayload {
        json,
        packed,
        wire_len: bytes.len(),
        message,
    })
}

/// One client→server message as its proto-channel payload: its JSON (hosts
/// never pack requests).
pub fn encode_client_payload(message: &ClientMessage) -> Vec<u8> {
    crate::json::to_string(message)
        .expect("a ClientMessage always serializes")
        .into_bytes()
}

/// Read one client→server proto-channel message.
pub fn decode_client_payload(bytes: &[u8]) -> Result<ClientMessage, PayloadError> {
    match bytes.first() {
        None => Err(PayloadError::Empty),
        Some(&PAYLOAD_TAG_JSON) => crate::json::from_slice::<ClientMessage>(bytes)
            .map_err(|e| PayloadError::BadClientJson(e.to_string())),
        Some(&other) => Err(PayloadError::UnknownTag(other)),
    }
}

/// A server payload [`encode_server_payload_into`] wrote.
#[cfg(feature = "ser-write-json")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodedPayload {
    /// Bytes written at the start of the buffer.
    pub len: usize,
    /// Whether it went packed (else JSON).
    pub packed: bool,
}

/// Write one server→client message into `buf` as its proto-channel payload:
/// packed against `table` when one is given and the message packs, else
/// JSON. Allocation-free, for the board.
///
/// On success with `packed`, `table` holds the message's learning: roll it
/// back (to a [`LearnMark`](lp_json_pack::LearnMark) taken before the call)
/// if the payload is then not sent. A message that cannot be packed (it holds
/// text the packed form cannot reproduce, or it does not fit packed) goes as
/// JSON with `table` untouched; [`WireWriteError::Full`](crate::WireWriteError::Full)
/// only when the JSON does not fit either.
#[cfg(feature = "ser-write-json")]
pub fn encode_server_payload_into(
    buf: &mut [u8],
    message: &WireServerMessage,
    table: Option<&mut dyn LearnStore>,
) -> Result<EncodedPayload, crate::WireWriteError> {
    if let Some(table) = table
        && let Some((tag, rest)) = buf.split_first_mut()
    {
        if let Ok(n) = crate::ser_learned_to(rest, table, message) {
            *tag = PAYLOAD_TAG_PACKED;
            return Ok(EncodedPayload {
                len: 1 + n,
                packed: true,
            });
        }
    }
    let mut sink = ser_write_json::ser_write::SliceWriter::new(buf);
    crate::ser_write_json_to(&mut sink, message).map_err(|_| crate::WireWriteError::Full)?;
    Ok(EncodedPayload {
        len: sink.len(),
        packed: false,
    })
}

/// [`encode_server_payload_into`] onto a `Vec` (hosts: a board double, tests).
/// Returns whether it went packed.
#[cfg(feature = "ser-write-json")]
pub fn encode_server_payload(
    message: &WireServerMessage,
    table: Option<&mut dyn LearnStore>,
    out: &mut Vec<u8>,
) -> bool {
    // A packed message is never longer than its JSON (packed_frame's tests).
    let mut buf = alloc::vec![0u8; crate::ser_write_json_len(message) + 1];
    let encoded = encode_server_payload_into(&mut buf, message, table)
        .expect("a buffer sized to the JSON holds either form");
    out.extend_from_slice(&buf[..encoded.len]);
    encoded.packed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::ServerMsgBody;
    use lp_json_pack::LearnedTable;

    #[test]
    fn json_payloads_are_the_bare_json() {
        let msg = heartbeat_ish(7);
        let json = crate::json::to_string(&msg).unwrap();
        let payload = decode_server_payload(json.as_bytes(), &mut LearnedTable::default()).unwrap();
        assert!(!payload.packed);
        assert_eq!(payload.json, json);
        assert_eq!(payload.wire_len, json.len());
        assert_eq!(payload.message.unwrap().id, msg.id);
    }

    #[test]
    fn a_json_payload_the_types_do_not_know_is_handed_on_with_its_error() {
        let text = br#"{"id":3,"msg":{"fromTheFuture":{}}}"#;
        let payload = decode_server_payload(text, &mut LearnedTable::default()).unwrap();
        assert!(payload.message.is_err());
        assert_eq!(payload.json.as_bytes(), text);
    }

    #[test]
    fn bad_payloads_are_errors_not_panics() {
        let mut table = LearnedTable::NEW;
        assert!(matches!(
            decode_server_payload(&[], &mut table),
            Err(PayloadError::Empty)
        ));
        assert!(matches!(
            decode_server_payload(b"M!{}", &mut table),
            Err(PayloadError::UnknownTag(b'M'))
        ));
        assert!(matches!(
            decode_server_payload(b"L", &mut table),
            Err(PayloadError::OutOfStep(HeaderMismatch::Truncated))
        ));
        assert!(matches!(
            decode_client_payload(b""),
            Err(PayloadError::Empty)
        ));
        assert!(matches!(
            decode_client_payload(b"{\"id\":1}"),
            Err(PayloadError::BadClientJson(_))
        ));
    }

    #[test]
    fn client_payloads_round_trip_as_json() {
        let msg = ClientMessage {
            id: 42,
            msg: crate::ClientRequest::Hello,
        };
        let bytes = encode_client_payload(&msg);
        assert_eq!(bytes[0], PAYLOAD_TAG_JSON);
        let back = decode_client_payload(&bytes).unwrap();
        assert_eq!(back.id, 42);
        assert!(matches!(back.msg, crate::ClientRequest::Hello));
    }

    /// The recorded traffic, in order, through a board table and a host
    /// table: packed payloads decode to the JSON the board's JSON form would
    /// have been, byte for byte, and the tables stay in step throughout.
    #[cfg(feature = "ser-write-json")]
    #[test]
    fn recorded_traffic_round_trips_packed_with_the_tables_in_step() {
        use crate::test_traffic::{TrafficDirection, traffic_lines};
        let mut board = LearnedTable::NEW;
        let mut host = LearnedTable::NEW;
        let mut packed = 0;
        for line in traffic_lines() {
            if line.direction != TrafficDirection::BoardToHost {
                continue;
            }
            let msg: WireServerMessage = crate::json::from_str(&line.json).unwrap();
            let mut out = Vec::new();
            if encode_server_payload(&msg, Some(&mut board), &mut out) {
                assert_eq!(out[0], PAYLOAD_TAG_PACKED);
                packed += 1;
            }
            let payload = decode_server_payload(&out, &mut host)
                .unwrap_or_else(|e| panic!("line {}: {e}", line.index));
            assert_eq!(payload.json, line.json, "line {}", line.index);
            assert_eq!(payload.message.unwrap().id, msg.id);
            assert_eq!(board.mark(), host.mark(), "line {}", line.index);
        }
        assert!(packed > 100, "{packed}");
    }

    #[cfg(feature = "ser-write-json")]
    #[test]
    fn without_a_table_the_board_writes_json_and_a_short_buffer_is_full() {
        let msg = heartbeat_ish(1);
        let mut out = Vec::new();
        assert!(!encode_server_payload(&msg, None, &mut out));
        assert_eq!(out, crate::json::to_string(&msg).unwrap().into_bytes());
        let mut short = [0u8; 8];
        assert_eq!(
            encode_server_payload_into(&mut short, &msg, Some(&mut LearnedTable::default())),
            Err(crate::WireWriteError::Full)
        );
    }

    #[cfg(feature = "ser-write-json")]
    #[test]
    fn a_host_that_starts_mid_session_is_told_it_is_out_of_step() {
        let mut board = LearnedTable::NEW;
        let (mut first, mut second) = (Vec::new(), Vec::new());
        encode_server_payload(&heartbeat_ish(1), Some(&mut board), &mut first);
        encode_server_payload(&heartbeat_ish(2), Some(&mut board), &mut second);
        // A reader that missed the first message (and what it taught).
        let mut late = LearnedTable::NEW;
        assert!(matches!(
            decode_server_payload(&second, &mut late),
            Err(PayloadError::OutOfStep(_))
        ));
        assert_eq!(late.mark(), LearnedTable::NEW.mark(), "learned nothing");
    }

    fn heartbeat_ish(id: u64) -> WireServerMessage {
        WireServerMessage::new(
            id,
            ServerMsgBody::Error {
                error: alloc::format!("a repeated error text for learning, number {id}"),
            },
        )
    }
}
