//! A server message as one lp-link proto-channel payload (D4 of plan
//! `lp-link-usb-cutover`): the USB link's half of what [`super::server_msg`]
//! does for the `M!` transports.
//!
//! One proto-channel message is one whole wire message, and the link already
//! delimits and checks it, so there is no `\nM!` prefix, no trailing `\n`, and
//! no COBS:
//!
//! - **JSON**: the message's JSON text; its first byte is `{`.
//! - **Packed** (a host that opted in with `SetEncoding`): `b'L'`
//!   ([`lpc_wire::FRAME_KIND_LEARNED`]) followed by the learned frame's bytes
//!   — the 3-byte table header, then the packed value — exactly what
//!   [`lpc_wire::ser_learned_to`] writes, coded against the link's table.
//!
//! The bytes go into the same static frame buffer the `M!` path uses
//! ([`super::server_msg`]'s `FRAME_BUF`), serialized in thread context, and
//! [`super::server_msg::frame_bytes`] hands them to `Link::send`, which copies
//! them before the transport serializes again (see `FRAME_BUF` for the
//! exclusivity argument; the USB link transport never holds the buffer
//! across another serializer's turn).
//!
//! ⚠️ **Integration seam (P1).** Phase P1 adds the canonical codec for these
//! bytes to lpc-wire (`lpc_wire::link_payload::encode_server_payload`, and the
//! host's `decode_server_payload`). This function writes the same bytes by
//! the same contract; at integration it should call P1's encoder (keeping
//! this static buffer and the JSON fallback), so the two ends share one
//! definition of the tag byte.

use super::server_msg::{FrameBufWriter, SERVER_MSG_JSON_BUFFER_SIZE, server_message_detail};

/// The tag byte a packed proto payload starts with.
pub const PACKED_PAYLOAD_TAG: u8 = lpc_wire::FRAME_KIND_LEARNED;

/// Serialize `msg` into the static frame buffer as one proto-channel payload,
/// returning its length: packed against `table` when the link has one (a
/// [`super::packed_link::PackedLink::table_for`] answer), JSON otherwise.
///
/// As on the `M!` path, a packed write that does not fit or cannot be packed
/// goes out as JSON instead, never dropped, with the table left as it was;
/// the caller rolls a packed frame's learning back if the link then refuses
/// it. Without the `json-pack` feature every payload is JSON.
pub fn serialize_server_payload(
    msg: &lpc_wire::WireServerMessage,
    table: Option<&mut dyn lp_json_pack::LearnStore>,
) -> Result<usize, lpc_wire::TransportError> {
    #[cfg(feature = "json-pack")]
    if let Some(table) = table {
        match serialize_packed_payload(msg, table) {
            Ok(len) => return Ok(len),
            Err(error) => super::server_msg::note_packed_fallback(msg, error),
        }
    }
    #[cfg(not(feature = "json-pack"))]
    let _ = table;
    serialize_json_payload(msg)
}

/// `b'L'` + one learned frame (unframed) into the frame buffer.
#[cfg(feature = "json-pack")]
fn serialize_packed_payload(
    msg: &lpc_wire::WireServerMessage,
    table: &mut dyn lp_json_pack::LearnStore,
) -> Result<usize, lpc_wire::WireWriteError> {
    // SAFETY: single writer by protocol (see `server_msg::FRAME_BUF`): the
    // transport serializes only while no one else holds the buffer, and the
    // slice is dropped before this returns.
    let buf = unsafe { super::server_msg::frame_buf_mut() };
    let (tag, body) = buf
        .split_first_mut()
        .ok_or(lpc_wire::WireWriteError::Full)?;
    *tag = PACKED_PAYLOAD_TAG;
    let n = lpc_wire::ser_learned_to(body, table, msg)?;
    Ok(1 + n)
}

/// The message's JSON text into the frame buffer. The measure pass runs first
/// so an oversized message is refused with the budget numbers instead of a
/// mid-write failure.
fn serialize_json_payload(
    msg: &lpc_wire::WireServerMessage,
) -> Result<usize, lpc_wire::TransportError> {
    let json_len = lpc_wire::ser_write_json_len(msg);
    if json_len > SERVER_MSG_JSON_BUFFER_SIZE {
        let detail = server_message_detail(msg);
        log::warn!(
            "[usb_link] server message id={} {} exceeded frame budget: {} B > {} (frame_budget={})",
            msg.id,
            detail,
            json_len,
            SERVER_MSG_JSON_BUFFER_SIZE,
            lpc_wire::PROJECT_READ_FRAME_MAX_BYTES
        );
        return Err(lpc_wire::TransportError::Serialization(alloc::format!(
            "server message id={} {} exceeded frame budget ({json_len} B)",
            msg.id,
            detail
        )));
    }
    let mut writer = FrameBufWriter { len: 0 };
    if lpc_wire::ser_write_json_to(&mut writer, msg).is_err() {
        // Unreachable if the measure pass is honest; a real error rather than
        // a panic because the link must stay up.
        return Err(lpc_wire::TransportError::Serialization(alloc::format!(
            "server message id={} {} failed to serialize",
            msg.id,
            server_message_detail(msg)
        )));
    }
    debug_assert_eq!(writer.len, json_len, "measure and write passes disagree");
    Ok(writer.len)
}
