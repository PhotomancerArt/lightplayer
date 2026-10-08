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
/// the board (the USB link, the classic's UART link, the radio links) decodes
/// through here.
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

/// The heap's free bytes and largest free block, as the chip measures them
/// (`set_request_headroom_probe`); `None` until a chip installs one.
pub type RequestHeadroomProbe = fn() -> Option<(usize, usize)>;

static REQUEST_HEADROOM: core::sync::atomic::AtomicPtr<()> =
    core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Install the chip's heap probe for [`request_refusal`]. Once at boot.
pub fn set_request_headroom_probe(probe: RequestHeadroomProbe) {
    REQUEST_HEADROOM.store(probe as *mut (), core::sync::atomic::Ordering::Release);
}

fn request_headroom() -> Option<(usize, usize)> {
    let ptr = REQUEST_HEADROOM.load(core::sync::atomic::Ordering::Acquire);
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the only non-null value ever stored is a `RequestHeadroomProbe`
    // cast to a pointer (`set_request_headroom_probe`).
    let probe: RequestHeadroomProbe = unsafe { core::mem::transmute(ptr) };
    probe()
}

/// A request shorter than this is decoded without asking the heap.
const REQUEST_CHECKED_FROM_BYTES: usize = 2 * 1024;

/// Free bytes a large request must leave for the work that follows its
/// decode (the server's handling, the reply).
const REQUEST_FREE_MARGIN_BYTES: usize = 16 * 1024;

/// A request the heap cannot decode, refused before decoding: the error
/// reply to send for it (a small JSON message, ready for the link's send
/// ring) and the reason for the log. Decoding a request allocates its
/// largest value in one block — a file write's blob (base64 is 3/4 of its
/// text), a shader edit's byte array (one byte an element, grown by
/// doubling) — and that, not the reassembly, is where a 10 KB write reset
/// the silicon C6 with three links open (PR B's desk walk: `alloc 10242
/// bytes failed`). The block is read off the request's shape
/// ([`super::request_decode_block`]): measured as 3/4 of the whole message,
/// Studio's shader edits were refused on a board that could decode them
/// (`docs/defects/2026-10-08-shader-edits-over-wi-fi-are-refused-board-memory-busy.md`).
/// The read gate's posture: refusal ("board memory busy"), never a reset.
/// `None`: decode it (short, or the heap has room, or nothing probes the
/// heap).
pub fn request_refusal(data: &[u8]) -> Option<(alloc::vec::Vec<u8>, alloc::string::String)> {
    if data.len() < REQUEST_CHECKED_FROM_BYTES {
        return None;
    }
    refusal_given(data, request_headroom()?)
}

/// [`request_refusal`] against measured `(free, largest)` figures.
fn refusal_given(
    data: &[u8],
    (free, largest): (usize, usize),
) -> Option<(alloc::vec::Vec<u8>, alloc::string::String)> {
    if data.len() < REQUEST_CHECKED_FROM_BYTES {
        return None;
    }
    let block = super::request_decode_block::request_decode_block(data) + 1024;
    let total = data.len() + REQUEST_FREE_MARGIN_BYTES;
    if largest >= block && free >= total {
        return None;
    }
    let id = request_id(data)?;
    let reason = alloc::format!(
        "request refused: board memory busy (free {free} B, largest block {largest} B; a {} B \
         request needs {total} B free and a {block} B block); retry shortly or send it in \
         smaller pieces",
        data.len()
    );
    let reply = alloc::format!(r#"{{"id":{id},"msg":{{"error":{{"error":"{reason}"}}}}}}"#);
    Some((reply.into_bytes(), reason))
}

/// The id of a JSON client message, read off its start
/// (`{"id":<n>,…`, the field order every host's serializer writes) without
/// decoding the rest.
fn request_id(data: &[u8]) -> Option<u64> {
    let text = core::str::from_utf8(&data[..data.len().min(64)]).ok()?;
    let rest = text.trim_start().strip_prefix('{')?.trim_start();
    let rest = rest
        .strip_prefix(r#""id""#)?
        .trim_start()
        .strip_prefix(':')?
        .trim_start();
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    rest[..digits].parse().ok()
}

#[cfg(test)]
mod request_gate_tests {
    use super::*;

    /// A large request the heap cannot decode is refused with its own id
    /// and the read gate's words; one the heap can is not, and a short one
    /// is never measured.
    #[test]
    fn a_request_the_heap_cannot_decode_is_refused_in_words() {
        let mut big =
            alloc::string::String::from(r#"{"id":4242,"msg":{"filesystem":{"write":{"data":""#);
        big.push_str(&"QUFB".repeat(3_000));
        big.push_str(r#""}}}}"#);
        let (reply, reason) = refusal_given(big.as_bytes(), (60_000, 9_000)).expect("refused");
        assert!(
            reason.starts_with("request refused: board memory busy"),
            "{reason}"
        );
        let reply: lpc_wire::WireServerMessage =
            lpc_wire::json::from_slice(&reply).expect("the reply is a server message");
        assert_eq!(reply.id, 4242);
        assert!(matches!(
            reply.msg,
            lpc_wire::server::ServerMsgBody::Error { .. }
        ));
        assert!(
            refusal_given(br#"{"id":1,"msg":"hello"}"#, (0, 0)).is_none(),
            "short: not measured"
        );
        assert!(refusal_given(big.as_bytes(), (90_000, 40_000)).is_none());
        assert!(
            request_refusal(big.as_bytes()).is_none(),
            "no probe: no gate"
        );
        assert_eq!(request_id(br#" { "id" : 7 , "msg":"#), Some(7));
    }

    /// Studio's shader edit of the 1,971 B choker shader, ~7.1 KB as a byte
    /// array, at the figures the board refused it with on 2026-10-08
    /// (largest block 5,216 B): it decodes into a 2,048 B block, so it is
    /// taken; with no block that size it is still refused.
    #[test]
    fn a_shader_edit_is_measured_by_its_bytes_not_its_text() {
        let body = alloc::vec![b'v'; 1_971];
        let numbers: alloc::vec::Vec<alloc::string::String> =
            body.iter().map(|b| alloc::format!("{b}")).collect();
        let edit = alloc::format!(
            r#"{{"id":31,"msg":{{"projectCommand":{{"handle":1,"command":{{"mutateOverlay":{{"request":{{"batch":{{"commands":[{{"id":7,"mutation":{{"set_artifact_body":{{"artifact":{{"path":"/shader.glsl"}},"edit":{{"replace_body":[{}]}}}}}}}}]}}}}}}}}}}}}}}"#,
            numbers.join(",")
        );
        assert!(edit.len() > 7_000, "{}", edit.len());
        assert!(
            refusal_given(edit.as_bytes(), (41_724, 5_216)).is_none(),
            "a 2 KB decode on a 5 KB block is taken"
        );
        let (_, reason) = refusal_given(edit.as_bytes(), (41_724, 2_500)).expect("refused");
        assert!(reason.contains("a 3072 B block"), "{reason}");
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
