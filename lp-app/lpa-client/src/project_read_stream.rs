//! Shared project-read collect-until-`fin` state machine.
//!
//! Both the runtime-neutral [`LpClient`](crate::client::LpClient) and the Tokio
//! wrapper ([`TokioLpClient`](crate::tokio_client::TokioLpClient)) used to carry
//! byte-for-byte identical `project_read` receive loops. This module holds the
//! single, transport-agnostic state machine both now drive: each client only
//! owns its own "send one request / receive the next message" mechanics and
//! feeds every received message here.
//!
//! # The generic streaming rule (M6 envelope, E2)
//!
//! Streaming is an envelope capability. For the matched request id we:
//!
//! - require the envelope `seq` to be **contiguous from 0** (a gap is a protocol
//!   error, the same strictness the old per-frame sequence check enforced);
//! - accumulate the body's events while the envelope `fin == false`;
//! - complete the request when a message with `fin == true` arrives.
//!
//! # Event surface (M6/P4)
//!
//! This routine accumulates the ordered [`ProjectReadEvent`] values across
//! frames and returns them directly; `lpc-view`'s
//! [`ProjectReadApplier`](lpc_view::ProjectReadApplier) applies them
//! progressively at each consumer. The event grammar's own invariants
//! (`Begin`-once, family pairing, chunk reassembly, …) are validated by the
//! applier; this layer only owns envelope framing (E2) plus the E3 finality
//! agreement between the envelope `fin` and the terminal `End`/`Error` event.
//!
//! The aggregate [`ProjectReadCollector`] shim from M6/P1 is no longer on this
//! live path (it is left compiling for M6/P5 to delete).

use lpc_wire::{ProjectReadEvent, WireServerMessage, WireServerMsgBody};

use crate::client_event::ClientEvent;
use crate::protocol_session::{ProtocolSession, ResponseDisposition};

/// Failure while collecting a project-read stream.
///
/// Mirrors the two failure kinds callers must distinguish: a remote/server error
/// vs. a client-side protocol violation. Transport failures are handled by each
/// caller's own receive path, so they never reach here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectReadStreamError {
    /// The server reported an error (a top-level `Error` body or an `Error`
    /// event carried by the stream).
    Server(String),
    /// The stream violated the expected client protocol (seq gap, malformed
    /// event order, unexpected same-id body, or fin/End disagreement).
    Protocol(String),
    /// A valid but wrong-shaped same-id message arrived for the operation.
    Unexpected(String),
}

/// What the caller should do after feeding one received message.
#[derive(Debug)]
pub enum ProjectReadStreamStep {
    /// Nothing to record; keep receiving.
    Continue,
    /// Record this side-channel event, then keep receiving.
    Event(ClientEvent),
    /// The stream completed; stop receiving and return the ordered events.
    Complete(Vec<ProjectReadEvent>),
}

/// Correlation + collect state for one in-flight project read.
///
/// Created after the `ProjectRead` request is sent with its `request_id`. The
/// caller loops: receive a message, call [`accept`](Self::accept), and act on
/// the returned step.
pub struct ProjectReadStream {
    request_id: u64,
    /// Next envelope `seq` we require from the matched id (contiguous from 0).
    next_seq: u32,
    /// Ordered events accumulated across frames.
    events: Vec<ProjectReadEvent>,
}

impl ProjectReadStream {
    #[must_use]
    pub fn new(request_id: u64) -> Self {
        Self {
            request_id,
            next_seq: 0,
            events: Vec::new(),
        }
    }

    /// Feed one received message through the correlation + collect rule.
    pub fn accept(
        &mut self,
        protocol: &ProtocolSession,
        message: WireServerMessage,
    ) -> Result<ProjectReadStreamStep, ProjectReadStreamError> {
        match protocol.response_disposition(&message, self.request_id) {
            ResponseDisposition::Matched => self.accept_matched(message),
            ResponseDisposition::Unsolicited => Ok(ClientEvent::from_unsolicited_message(message)
                .map_or(
                    ProjectReadStreamStep::Continue,
                    ProjectReadStreamStep::Event,
                )),
            ResponseDisposition::Uncorrelated {
                response_id,
                expected_id,
            } => Ok(ProjectReadStreamStep::Event(
                ClientEvent::UncorrelatedResponse {
                    response_id,
                    expected_id,
                },
            )),
        }
    }

    fn accept_matched(
        &mut self,
        message: WireServerMessage,
    ) -> Result<ProjectReadStreamStep, ProjectReadStreamError> {
        // Envelope-level sequencing is enforced here for every matched frame,
        // including the terminal `Error` body.
        if message.seq != self.next_seq {
            return Err(ProjectReadStreamError::Protocol(format!(
                "expected project read frame seq {}, got {}",
                self.next_seq, message.seq
            )));
        }
        self.next_seq = self.next_seq.saturating_add(1);
        let fin = message.fin;

        match message.msg {
            WireServerMsgBody::Error { error } => Err(ProjectReadStreamError::Server(error)),
            WireServerMsgBody::ProjectRead { events } => {
                // E3: the envelope `fin` owns finality; the terminal `End`/`Error`
                // event must land in exactly the final frame. The full event
                // grammar is validated downstream by the applier — here we only
                // check the fin/terminal agreement and accumulate.
                let saw_terminal = events.iter().any(is_terminal_event);
                self.events.extend(events);
                if fin {
                    if !saw_terminal {
                        // `fin` arrived but the event grammar did not end the
                        // stream (no `End`/`Error` event).
                        return Err(ProjectReadStreamError::Protocol(
                            "project read stream ended (fin) before an End event".into(),
                        ));
                    }
                    Ok(ProjectReadStreamStep::Complete(core::mem::take(
                        &mut self.events,
                    )))
                } else {
                    if saw_terminal {
                        // A terminal event arrived on a non-final frame.
                        return Err(ProjectReadStreamError::Protocol(
                            "project read completed on a non-final frame (End without fin)".into(),
                        ));
                    }
                    Ok(ProjectReadStreamStep::Continue)
                }
            }
            other => Err(ProjectReadStreamError::Unexpected(format!("{other:?}"))),
        }
    }
}

fn is_terminal_event(event: &ProjectReadEvent) -> bool {
    matches!(
        event,
        ProjectReadEvent::End { .. } | ProjectReadEvent::Error { .. }
    )
}

#[cfg(test)]
mod tests {
    use lpc_model::Revision;
    use lpc_wire::{ProjectReadEvent, WireServerMessage, WireServerMsgBody};

    use super::*;

    fn frame(id: u64, seq: u32, fin: bool, events: Vec<ProjectReadEvent>) -> WireServerMessage {
        WireServerMessage::stream_frame(id, seq, fin, WireServerMsgBody::ProjectRead { events })
    }

    fn drive(
        messages: Vec<WireServerMessage>,
    ) -> Result<Vec<ProjectReadEvent>, ProjectReadStreamError> {
        let protocol = ProtocolSession::new();
        let mut stream = ProjectReadStream::new(1);
        for message in messages {
            match stream.accept(&protocol, message)? {
                ProjectReadStreamStep::Continue | ProjectReadStreamStep::Event(_) => {}
                ProjectReadStreamStep::Complete(events) => return Ok(events),
            }
        }
        Err(ProjectReadStreamError::Protocol(
            "stream did not complete".into(),
        ))
    }

    fn end_revision(events: &[ProjectReadEvent]) -> Revision {
        match events.last() {
            Some(ProjectReadEvent::End { revision }) => *revision,
            other => panic!("expected terminal End event, got {other:?}"),
        }
    }

    #[test]
    fn multi_frame_stream_completes_on_fin() {
        let events = drive(vec![
            frame(
                1,
                0,
                false,
                vec![ProjectReadEvent::Begin {
                    revision: Revision::new(7),
                }],
            ),
            frame(
                1,
                1,
                true,
                vec![ProjectReadEvent::End {
                    revision: Revision::new(7),
                }],
            ),
        ])
        .expect("stream completes");
        // Ordered events collected across both frames.
        assert_eq!(events.len(), 2);
        assert_eq!(end_revision(&events), Revision::new(7));
    }

    #[test]
    fn single_final_frame_completes() {
        let events = drive(vec![frame(
            1,
            0,
            true,
            vec![
                ProjectReadEvent::Begin {
                    revision: Revision::new(3),
                },
                ProjectReadEvent::End {
                    revision: Revision::new(3),
                },
            ],
        )])
        .expect("stream completes");
        assert_eq!(end_revision(&events), Revision::new(3));
    }

    #[test]
    fn seq_gap_is_protocol_error() {
        let error = drive(vec![
            frame(
                1,
                0,
                false,
                vec![ProjectReadEvent::Begin {
                    revision: Revision::new(7),
                }],
            ),
            // seq jumps 0 -> 2.
            frame(
                1,
                2,
                true,
                vec![ProjectReadEvent::End {
                    revision: Revision::new(7),
                }],
            ),
        ])
        .unwrap_err();
        assert!(matches!(error, ProjectReadStreamError::Protocol(m) if m.contains("seq")));
    }

    #[test]
    fn fin_without_end_event_is_protocol_error() {
        // A final frame that carries no End event: the grammar never completed.
        let error = drive(vec![frame(
            1,
            0,
            true,
            vec![ProjectReadEvent::Begin {
                revision: Revision::new(7),
            }],
        )])
        .unwrap_err();
        assert!(
            matches!(error, ProjectReadStreamError::Protocol(m) if m.contains("before an End"))
        );
    }

    #[test]
    fn end_event_on_non_final_frame_is_protocol_error() {
        // The End event arrives but the envelope says more frames follow.
        let error = drive(vec![frame(
            1,
            0,
            false,
            vec![
                ProjectReadEvent::Begin {
                    revision: Revision::new(7),
                },
                ProjectReadEvent::End {
                    revision: Revision::new(7),
                },
            ],
        )])
        .unwrap_err();
        assert!(
            matches!(error, ProjectReadStreamError::Protocol(m) if m.contains("End without fin"))
        );
    }

    #[test]
    fn top_level_error_body_is_server_error() {
        let error = drive(vec![WireServerMessage::new(
            1,
            WireServerMsgBody::Error {
                error: "bad read".into(),
            },
        )])
        .unwrap_err();
        assert_eq!(error, ProjectReadStreamError::Server("bad read".into()));
    }

    #[test]
    fn unexpected_same_id_body_is_unexpected() {
        let error = drive(vec![WireServerMessage::new(
            1,
            WireServerMsgBody::StopAllProjects,
        )])
        .unwrap_err();
        assert!(matches!(error, ProjectReadStreamError::Unexpected(_)));
    }

    #[test]
    fn unsolicited_messages_are_buffered_as_events() {
        let protocol = ProtocolSession::new();
        let mut stream = ProjectReadStream::new(1);

        // An unsolicited log (id 0) between frames is surfaced as an event and
        // does not disturb the required seq contiguity.
        let step = stream
            .accept(
                &protocol,
                frame(
                    1,
                    0,
                    false,
                    vec![ProjectReadEvent::Begin {
                        revision: Revision::new(7),
                    }],
                ),
            )
            .unwrap();
        assert!(matches!(step, ProjectReadStreamStep::Continue));

        let log = WireServerMessage::new(
            0,
            WireServerMsgBody::Log {
                level: lpc_wire::server::api::LogLevel::Info,
                message: "hi".into(),
            },
        );
        let step = stream.accept(&protocol, log).unwrap();
        assert!(matches!(
            step,
            ProjectReadStreamStep::Event(ClientEvent::Log { .. })
        ));

        let step = stream
            .accept(
                &protocol,
                frame(
                    1,
                    1,
                    true,
                    vec![ProjectReadEvent::End {
                        revision: Revision::new(7),
                    }],
                ),
            )
            .unwrap();
        assert!(matches!(step, ProjectReadStreamStep::Complete(_)));
    }
}
