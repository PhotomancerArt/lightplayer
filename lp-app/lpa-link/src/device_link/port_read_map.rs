//! What a [`WireLinkPort`](lpc_wire::WireLinkPort) read becomes above the
//! link: the model's [`LinkEvent`]s, or the [`WireRead`]s the older readers
//! were shaped around.
//!
//! Since `WIRE_PROTO_VERSION` 30 a board's USB link is an lp-link (plan
//! `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D1): the port hands up
//! decoded messages, console lines and the link's own `Up`/`Reset`, and the
//! layers above keep the vocabulary they had. This is the one mapping, so
//! every host reader (the byte-stream link, the Web Serial provider, the
//! emulator tab) says the same thing about the same read:
//!
//! | [`PortRead`] | [`LinkEvent`] | [`WireRead`] |
//! |---|---|---|
//! | `Message`, id below the app range | `Frame` | `Frame` |
//! | `Message`, app-range id | `Passthrough` (its `M!{json}` line) | `Frame` |
//! | `Message` that did not parse | `Error` | `Frame` (with the parse error) |
//! | `Log` | `Line` | `Line` |
//! | `Note` | `WireNote` | `Note` |
//! | `Reset` | `Error("link reset: …")` (D9) | `Error("link reset: …")` |
//! | `Up` | nothing | nothing |
//!
//! A `Reset` is an error-class item on purpose (D9): every request in flight
//! on the old session is lost, and a conversation waiting on one should fail
//! now rather than wait out its idle budget. [`is_link_reset_line`] tells it
//! apart from other errors.

use lpa_devices::link::{APP_CONVERSATION_ID_BASE, LinkEvent};
use lpc_wire::lp_link::ResetReason;
use lpc_wire::{PortRead, ServerPayload, WireServerMessage};

use crate::device_link::wire::server_frame;
use crate::device_link::wire_reader::{ReadFrame, WireRead};

/// The start of every link-reset error line (the same words `lpa-client`'s
/// transport uses).
pub const LINK_RESET_PREFIX: &str = lpa_client::LINK_RESET_PREFIX;

/// One port read as the model's event. `None` for `Up`, which the model has
/// no event for (the board's hello follows it, and that is the evidence).
pub fn port_read_link_event(read: PortRead) -> Option<LinkEvent> {
    Some(match read {
        PortRead::Message(payload) => payload_link_event(payload),
        PortRead::Log(line) => LinkEvent::Line(line),
        PortRead::Note(note) => LinkEvent::WireNote(note),
        PortRead::Reset { reason } => LinkEvent::Error(link_reset_line(reason)),
        PortRead::Up { .. } => return None,
    })
}

/// One port read as a [`WireRead`], for readers shaped around
/// `wire_reader`'s items. `None` for `Up`.
pub fn port_read_wire_read(read: PortRead) -> Option<WireRead> {
    Some(match read {
        PortRead::Message(ServerPayload {
            json,
            packed,
            message,
            ..
        }) => WireRead::Frame(ReadFrame {
            json,
            packed,
            message,
        }),
        PortRead::Log(line) => WireRead::Line(line),
        PortRead::Note(note) => WireRead::Note(note),
        PortRead::Reset { reason } => WireRead::Error(link_reset_line(reason)),
        PortRead::Up { .. } => return None,
    })
}

/// The line a link reset is reported with.
pub fn link_reset_line(reason: ResetReason) -> String {
    format!(
        "{LINK_RESET_PREFIX}: {}; requests in flight on the old session are lost",
        lpa_client::reset_reason_words(reason)
    )
}

/// Whether an error line is a link reset ([`link_reset_line`]).
pub fn is_link_reset_line(line: &str) -> bool {
    line.starts_with(LINK_RESET_PREFIX)
}

/// A decoded message → the model's frame, or an app conversation's
/// passthrough by its id (the rule `demux` applies to `M!` lines).
fn payload_link_event(payload: ServerPayload) -> LinkEvent {
    match &payload.message {
        Ok(message) => classify(&payload.json, message),
        Err(error) => LinkEvent::Error(format!("a message from the board did not parse: {error}")),
    }
}

fn classify(json: &str, message: &WireServerMessage) -> LinkEvent {
    if message.id >= u64::from(APP_CONVERSATION_ID_BASE) {
        return LinkEvent::Passthrough {
            // Wire ids are `u64`, the model's `u32`; saturating keeps a
            // pathological id from aliasing.
            request_id: u32::try_from(message.id).unwrap_or(u32::MAX),
            line: format!("M!{json}"),
        };
    }
    LinkEvent::Frame(server_frame(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::ServerMsgBody;

    #[test]
    fn a_model_reply_is_a_frame_and_an_app_reply_passes_through() {
        let model = payload(WireServerMessage::new(7, ServerMsgBody::UnloadProject));
        assert!(matches!(
            port_read_link_event(PortRead::Message(model)),
            Some(LinkEvent::Frame(frame)) if frame.request_id == 7
        ));

        let app_id = u64::from(APP_CONVERSATION_ID_BASE) + 3;
        let app = payload(WireServerMessage::new(app_id, ServerMsgBody::UnloadProject));
        let json = app.json.clone();
        assert_eq!(
            port_read_link_event(PortRead::Message(app)),
            Some(LinkEvent::Passthrough {
                request_id: app_id as u32,
                line: format!("M!{json}"),
            })
        );
    }

    #[test]
    fn a_reset_is_an_error_class_item_and_up_is_nothing() {
        let Some(LinkEvent::Error(line)) = port_read_link_event(PortRead::Reset {
            reason: ResetReason::PeerRestarted,
        }) else {
            panic!("a reset is an error");
        };
        assert!(is_link_reset_line(&line), "{line}");
        assert!(line.contains("the board restarted"), "{line}");
        assert!(matches!(
            port_read_wire_read(PortRead::Reset {
                reason: ResetReason::RetryLimit
            }),
            Some(WireRead::Error(line)) if is_link_reset_line(&line)
        ));
        assert_eq!(port_read_link_event(PortRead::Up { generation: 1 }), None);
        assert!(port_read_wire_read(PortRead::Up { generation: 1 }).is_none());
    }

    #[test]
    fn logs_and_notes_keep_their_kind() {
        assert_eq!(
            port_read_link_event(PortRead::Log("[INFO] boot".to_string())),
            Some(LinkEvent::Line("[INFO] boot".to_string()))
        );
        assert!(matches!(
            port_read_wire_read(PortRead::Note("wire: replies packed".to_string())),
            Some(WireRead::Note(note)) if note == "wire: replies packed"
        ));
    }

    fn payload(message: WireServerMessage) -> ServerPayload {
        let json = lpc_wire::json::to_string(&message).unwrap();
        ServerPayload {
            wire_len: json.len(),
            json,
            packed: false,
            message: Ok(message),
        }
    }
}
