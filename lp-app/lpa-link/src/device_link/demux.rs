//! A board's byte stream → [`LinkEvent`]s: the `M!` demux over the
//! frame-aware splitter every native reader shares
//! ([`lpc_wire::WireStream`]).
//!
//! On a serial wire, protocol messages and console output share one byte
//! stream: an `M!`-prefixed line IS a frame, a packed frame (`0x00 'P' COBS
//! 0x00`, on a link that opted in) is one too, and everything else is device
//! output. Both matter to the model — frames are peer evidence, lines are how
//! a blank chip or somebody else's firmware gets diagnosed — so neither is
//! dropped here. Deciding which is which is ALL this module does; no
//! classification lives on this side of the seam.
//!
//! A packed frame decodes to exactly the JSON its `M!` line would have
//! carried, and goes down the same path from there: the fold cannot tell the
//! two forms apart, and must not.

use std::collections::VecDeque;

use lpa_devices::link::{APP_CONVERSATION_ID_BASE, LinkEvent};
use lpc_wire::{WireChunk, WireServerMessage, WireStream};

use crate::device_link::wire::{decode_server_message, server_frame};
use crate::device_link::wire_reader::{ReadFrame, WireRead};

/// Demux one whole serial line into the event it is.
///
/// A malformed frame becomes [`LinkEvent::Error`] rather than being silently
/// discarded — the fold counts anomalies, and a wire that garbles every frame
/// must not read as a wire that is merely quiet.
///
/// Interleaved device output can corrupt a frame line by splicing into it
/// (logs and frames share the wire). When decoding fails and another `M!`
/// marker is embedded further along, decoding resyncs at it, mirroring the
/// shipped browser line wire's behavior.
///
/// A frame whose id is at or above
/// [`APP_CONVERSATION_ID_BASE`](lpa_devices::link::APP_CONVERSATION_ID_BASE)
/// is an app conversation's reply, not the model's: it is handed on as
/// [`LinkEvent::Passthrough`] with the ORIGINAL line (resynced, if it had
/// to be) so the conversation decodes it with the full wire vocabulary
/// the mirror deliberately drops. The classification happens here, once,
/// so the pump and the lens tap cannot disagree about which frames the
/// fold hears.
pub fn demux_line(line: &str) -> LinkEvent {
    match line.strip_prefix("M!") {
        Some(frame_json) => demux_frame_json(frame_json),
        None => LinkEvent::Line(line.to_string()),
    }
}

/// Demux one chunk of a [`WireStream`] into the event it is.
///
/// A packed frame's JSON takes the same path as an `M!` line's body — a
/// passthrough carries it re-formed as `M!{json}`, so an app conversation
/// reads the line it always read — and a frame that could not be delivered
/// is [`LinkEvent::Error`], never silence.
pub fn demux_chunk(chunk: WireChunk) -> LinkEvent {
    match chunk {
        WireChunk::Line(line) => LinkEvent::Line(line),
        WireChunk::Frame(frame) => demux_frame_json(&frame.json),
        WireChunk::Error(error) => LinkEvent::Error(error),
    }
}

/// One [`WireRead`] → the event it is, or `None` for a request the caller
/// must write ([`WireRead::Send`]) rather than hand to the model.
///
/// A frame the reader already decoded is not decoded again; one that did not
/// decode (console text spliced into a JSON line) takes [`demux_line`]'s
/// resync.
pub fn demux_read(read: WireRead) -> Option<LinkEvent> {
    Some(match read {
        WireRead::Line(line) => LinkEvent::Line(line),
        WireRead::Frame(ReadFrame {
            json,
            message: Ok(message),
            ..
        }) => classify(&json, &message),
        WireRead::Frame(ReadFrame { json, .. }) => demux_frame_json(&json),
        WireRead::Error(error) => LinkEvent::Error(error),
        WireRead::Note(note) => LinkEvent::WireNote(note),
        WireRead::Send(_) => return None,
    })
}

/// An `M!` body (or a decoded packed frame) → the event it is. See
/// [`demux_line`] for the resync and the app-range rule.
fn demux_frame_json(mut frame_json: &str) -> LinkEvent {
    loop {
        match decode_server_message(frame_json) {
            Ok(message) => return classify(frame_json, &message),
            Err(error) => match frame_json.find("M!").filter(|offset| *offset > 0) {
                Some(offset) => frame_json = &frame_json[offset + 2..],
                None => return LinkEvent::Error(error),
            },
        }
    }
}

/// A decoded message → the model's frame, or an app conversation's
/// passthrough by its id.
fn classify(frame_json: &str, message: &WireServerMessage) -> LinkEvent {
    if message.id >= u64::from(APP_CONVERSATION_ID_BASE) {
        return LinkEvent::Passthrough {
            // Wire ids are `u64`, the model's `u32`; saturating keeps a
            // pathological id from aliasing (the same rule `server_frame`
            // applies).
            request_id: u32::try_from(message.id).unwrap_or(u32::MAX),
            line: format!("M!{frame_json}"),
        };
    }
    LinkEvent::Frame(server_frame(message))
}

/// Demux a chunk of bytes straight onto an event queue.
pub fn push_bytes(stream: &mut WireStream, bytes: &[u8], events: &mut VecDeque<LinkEvent>) {
    stream.push(bytes, |chunk| events.push_back(demux_chunk(chunk)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_devices::wire::ServerFrameBody;

    #[test]
    fn console_output_and_protocol_frames_come_out_of_one_stream() {
        let mut stream = WireStream::new();
        let mut events = VecDeque::new();

        push_bytes(
            &mut stream,
            b"ESP-ROM:esp32c6-20220919\r\nM!{\"id\":0,\"msg\":\"unloadProject\"}\n",
            &mut events,
        );

        assert!(matches!(
            events.pop_front(),
            Some(LinkEvent::Line(line)) if line == "ESP-ROM:esp32c6-20220919"
        ));
        assert!(matches!(
            events.pop_front(),
            Some(LinkEvent::Frame(frame)) if matches!(frame.body, ServerFrameBody::Other { .. })
        ));
        assert!(events.is_empty());
    }

    #[test]
    fn a_line_split_across_reads_is_delivered_once_whole() {
        let mut stream = WireStream::new();
        let mut events = VecDeque::new();

        push_bytes(&mut stream, b"M!{\"id\":0,", &mut events);
        assert!(events.is_empty());
        assert!(stream.pending_bytes() > 0, "the tail is held, not dropped");
        push_bytes(
            &mut stream,
            b"\"msg\":\"unloadProject\"}\nnext\n",
            &mut events,
        );

        assert!(matches!(events.pop_front(), Some(LinkEvent::Frame(_))));
        assert_eq!(events.pop_front(), Some(LinkEvent::Line("next".into())));
        assert_eq!(stream.pending_bytes(), 0);
    }

    #[test]
    fn a_reopen_drops_the_previous_generations_partial_line() {
        let mut stream = WireStream::new();
        let mut events = VecDeque::new();
        push_bytes(&mut stream, b"half a li", &mut events);

        stream.clear();
        push_bytes(&mut stream, b"whole\n", &mut events);

        assert_eq!(events.pop_front(), Some(LinkEvent::Line("whole".into())));
        assert!(events.is_empty());
    }

    /// A packed frame is the same frame as its `M!` line: the same event,
    /// and in the app range the same passthrough line.
    #[test]
    fn a_packed_frame_demuxes_exactly_like_its_json_line() {
        for id in [7, u64::from(APP_CONVERSATION_ID_BASE) + 3] {
            let message =
                lpc_wire::WireServerMessage::new(id, lpc_wire::ServerMsgBody::UnloadProject);
            let json = lpc_wire::json::to_string(&message).unwrap();
            let mut framed = vec![0u8; 256];
            let n = lpc_wire::ser_packed_frame_to(&mut framed, &message).unwrap();

            let mut stream = WireStream::new();
            let mut events = VecDeque::new();
            push_bytes(&mut stream, &framed[..n], &mut events);

            assert_eq!(events.pop_front(), Some(LinkEvent::Line(String::new())));
            assert_eq!(events.pop_front(), Some(demux_line(&format!("M!{json}"))));
            assert!(events.is_empty());
        }
    }

    #[test]
    fn a_packed_frame_that_does_not_decode_is_an_anomaly() {
        let mut stream = WireStream::new();
        let mut events = VecDeque::new();
        push_bytes(&mut stream, b"\x00P\x02\xff\x00", &mut events);

        assert!(
            matches!(events.pop_front(), Some(LinkEvent::Error(_))),
            "{events:?}"
        );
    }

    #[test]
    fn a_garbled_frame_is_an_anomaly_not_silence() {
        let event = demux_line("M!{not json");

        assert!(
            matches!(&event, LinkEvent::Error(message) if message.contains("malformed M! frame")),
            "{event:?}"
        );
    }

    /// Interleaved device output can splice into a frame line; the frame
    /// after the splice is still a frame.
    #[test]
    fn decoding_resyncs_at_an_embedded_marker() {
        let event =
            demux_line("M!{\"id\":0,\"msM![INIT] logM!{\"id\":9,\"msg\":\"unloadProject\"}");

        let LinkEvent::Frame(frame) = event else {
            panic!("expected a resynced frame, got {event:?}");
        };
        assert_eq!(frame.request_id, 9);
    }

    /// An app conversation's reply is classified by its id and handed on
    /// whole — the conversation decodes it, the mirror never sees it.
    #[test]
    fn a_reply_in_the_app_range_passes_through_verbatim() {
        let line = "M!{\"id\":1073741825,\"msg\":\"unloadProject\"}";

        let event = demux_line(line);

        assert_eq!(
            event,
            LinkEvent::Passthrough {
                request_id: APP_CONVERSATION_ID_BASE + 1,
                line: line.to_string(),
            }
        );
    }

    /// The last id below the base is still the model's.
    #[test]
    fn a_reply_just_below_the_app_range_is_a_model_frame() {
        let event = demux_line("M!{\"id\":1073741823,\"msg\":\"unloadProject\"}");

        let LinkEvent::Frame(frame) = event else {
            panic!("expected a mirrored frame, got {event:?}");
        };
        assert_eq!(frame.request_id, APP_CONVERSATION_ID_BASE - 1);
    }

    /// A resync inside an app-range line hands on the resynced frame, not
    /// the spliced garbage before it.
    #[test]
    fn a_passthrough_resyncs_like_any_frame() {
        let event = demux_line(
            "M!{\"id\":0,\"msM![INIT] logM!{\"id\":1073741826,\"msg\":\"unloadProject\"}",
        );

        assert_eq!(
            event,
            LinkEvent::Passthrough {
                request_id: APP_CONVERSATION_ID_BASE + 2,
                line: "M!{\"id\":1073741826,\"msg\":\"unloadProject\"}".to_string(),
            }
        );
    }

    #[test]
    fn a_bare_marker_line_is_an_anomaly_rather_than_a_frame() {
        assert!(matches!(demux_line("M!"), LinkEvent::Error(_)));
    }
}
