//! A server message as one lp-link proto-channel payload (D4 of plan
//! `lp-link-usb-cutover`): the USB link's half of what [`super::server_msg`]
//! does for the `M!` transports.
//!
//! One proto-channel message is one whole wire message, and the link already
//! delimits and checks it, so there is no `\nM!` prefix, no trailing `\n`, and
//! no COBS: the message's JSON text (first byte `{`), or on a link whose host
//! opted in `b'L'` followed by the learned frame. The encoding is lpc-wire's
//! own ([`lpc_wire::encode_server_payload_into`]), the one the host's
//! `decode_server_payload` reads.
//!
//! The bytes go into the same static frame buffer the `M!` path uses
//! ([`super::server_msg`]'s `FRAME_BUF`), serialized in thread context, and
//! [`super::server_msg::frame_bytes`] hands them to `Link::send`, which copies
//! them before the transport serializes again (see `FRAME_BUF` for the
//! exclusivity argument).

use super::server_msg::{SERVER_MSG_JSON_BUFFER_SIZE, server_message_detail};

/// Serialize `msg` into the static frame buffer as one proto-channel payload,
/// returning its length: packed against `table` when the link has one (a
/// [`super::packed_link::PackedLink::table_for`] answer) and the message
/// packs, JSON otherwise — never dropped for not packing, and the table is
/// left as it was then. The caller rolls a packed payload's learning back if
/// the link then refuses it. Without the `json-pack` feature every payload is
/// JSON.
pub fn serialize_server_payload(
    msg: &lpc_wire::WireServerMessage,
    table: Option<&mut dyn lp_json_pack::LearnStore>,
) -> Result<usize, lpc_wire::TransportError> {
    #[cfg(not(feature = "json-pack"))]
    let table = {
        let _ = table;
        None
    };
    let wanted_packed = table.is_some();
    // SAFETY: single writer by protocol (see `server_msg::FRAME_BUF`): the
    // transport serializes only while no one else holds the buffer, and the
    // slice is dropped before this returns.
    let buf = unsafe { super::server_msg::frame_buf_mut() };
    match lpc_wire::encode_server_payload_into(buf, msg, table) {
        Ok(encoded) => {
            if wanted_packed && !encoded.packed {
                note_sent_as_json(msg);
            }
            Ok(encoded.len)
        }
        Err(_) => {
            let detail = server_message_detail(msg);
            log::warn!(
                "[usb_link] server message id={} {} exceeded frame budget: {} B > {} (frame_budget={})",
                msg.id,
                detail,
                lpc_wire::ser_write_json_len(msg),
                SERVER_MSG_JSON_BUFFER_SIZE,
                lpc_wire::PROJECT_READ_FRAME_MAX_BYTES
            );
            Err(lpc_wire::TransportError::Serialization(alloc::format!(
                "server message id={} {} exceeded frame budget",
                msg.id,
                detail
            )))
        }
    }
}

/// One proto-channel payload from a host → the client message it carries:
/// always JSON (hosts never pack), first byte `{`. Every lp-link transport on
/// the board (the USB link, the radio links) decodes through here.
///
/// Out of line on purpose: the deserializer's frame stays its own, and never
/// joins the server loop future's.
///
/// `lpc_wire::decode_client_payload` reads the same bytes, but through
/// `json::from_slice`: a second instantiation of the whole `ClientMessage`
/// deserializer beside the `from_str` one the image already links, measured at
/// +102 KB on the C6 image. Same contract (bare JSON, first byte `{`), one
/// deserializer.
#[inline(never)]
pub fn decode_client_payload(
    data: &[u8],
) -> Result<lpc_wire::ClientMessage, lpc_wire::PayloadError> {
    match (data.first(), core::str::from_utf8(data)) {
        (Some(&lpc_wire::PAYLOAD_TAG_JSON), Ok(text)) => lpc_wire::json::from_str::<
            lpc_wire::ClientMessage,
        >(text)
        .map_err(|e| lpc_wire::PayloadError::BadClientJson(alloc::string::ToString::to_string(&e))),
        (Some(&lpc_wire::PAYLOAD_TAG_JSON), Err(_)) => Err(lpc_wire::PayloadError::NotUtf8),
        (Some(&tag), _) => Err(lpc_wire::PayloadError::UnknownTag(tag)),
        (None, _) => Err(lpc_wire::PayloadError::Empty),
    }
}

/// A packed link's message went as JSON: say so once per boot, then at
/// debug, so a message class that never packs cannot flood the log.
fn note_sent_as_json(msg: &lpc_wire::WireServerMessage) {
    use core::sync::atomic::{AtomicBool, Ordering::Relaxed};
    static WARNED: AtomicBool = AtomicBool::new(false);
    if WARNED.swap(true, Relaxed) {
        log::debug!(
            "[usb_link] server message id={} not packed; sent as JSON",
            msg.id
        );
    } else {
        log::warn!(
            "[usb_link] server message id={} {} not packed; sent as JSON (further ones log \
             at debug)",
            msg.id,
            server_message_detail(msg)
        );
    }
}
