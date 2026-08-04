//! DeviceEventSink → studio adapters (the sink vocabulary lives in
//! `lpa-link`; the UX vocabulary lives here — lpa-link stays UxUpdate-free).
//!
//! Two adapters:
//!
//! - [`console_event_sink`] — the sink installed at connect: device serial
//!   lines become buffered console drafts (drained into the log ring by the
//!   controller), state/progress events are ignored (state is read by
//!   snapshot pull).
//! - [`management_event_sink`] — the per-operation sink for
//!   `DeviceSession::manage`: log lines are captured AND mirrored as
//!   progressive `UxUpdate::Log`s, progress events update the live activity
//!   view.

use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::{DeviceEvent, DeviceEventSink, DeviceLineOrigin, DeviceState};

use crate::app::server::device_log_line::parse_device_log_line;
use crate::core::log::{DeviceEventKind, DeviceEventRecorder};
use crate::{UiLogDraft, UiLogLevel, UiLogOrigin, UiLogSource, UxUpdate, UxUpdateSink};

/// Compact, stable label for a [`DeviceState`] in event-log records. Part
/// of the JSONL trace contract — extend, do not rename.
pub(crate) fn device_state_label(state: &DeviceState) -> String {
    match state {
        DeviceState::Bootloader => "bootloader".to_string(),
        DeviceState::BlankFlash => "blank-flash".to_string(),
        DeviceState::ForeignFirmware => "foreign-firmware".to_string(),
        DeviceState::Booting => "booting".to_string(),
        DeviceState::Ready { .. } => "ready".to_string(),
        DeviceState::Incompatible { reason } => {
            use lpa_link::IncompatibleReason;
            let detail = match reason {
                IncompatibleReason::FrameBeforeHello => "frame-before-hello",
                IncompatibleReason::NoHello => "no-hello",
                IncompatibleReason::ProtoMismatch { .. } => "proto-mismatch",
            };
            format!("incompatible({detail})")
        }
        DeviceState::Unresponsive { .. } => "unresponsive".to_string(),
        DeviceState::Gone => "gone".to_string(),
    }
}

/// Map one device event's log line into a console draft.
fn log_line_draft(line: &str, origin: DeviceLineOrigin) -> UiLogDraft {
    match origin {
        DeviceLineOrigin::Device => {
            let parsed = parse_device_log_line(line);
            UiLogDraft::new(
                parsed.level,
                match parsed.module {
                    Some(module) => UiLogSource::with_detail(UiLogOrigin::Device, module),
                    None => UiLogSource::new(UiLogOrigin::Device),
                },
                parsed.message,
            )
        }
        DeviceLineOrigin::Link => UiLogDraft::new(
            UiLogLevel::Info,
            UiLogSource::with_detail(UiLogOrigin::Link, "device-session"),
            line,
        ),
    }
}

/// The connect-time sink: buffer device console lines as drafts for the
/// controller to drain into its log ring, and feed the device event log —
/// state transitions (previously discarded here, which left the whole
/// device path with zero transition history), parse anomalies (always
/// counted), and raw RX/TX traffic (capture mode only).
pub(crate) fn console_event_sink(
    pending: Rc<RefCell<Vec<UiLogDraft>>>,
    events: DeviceEventRecorder,
    endpoint: Option<String>,
) -> DeviceEventSink {
    DeviceEventSink::new(move |event| {
        let endpoint = endpoint.as_deref();
        match event {
            DeviceEvent::LogLine { line, origin } => {
                if origin == DeviceLineOrigin::Device && events.capture() {
                    events.record(None, endpoint, DeviceEventKind::Rx { line: line.clone() });
                }
                pending.borrow_mut().push(log_line_draft(&line, origin));
            }
            DeviceEvent::State { from, to } => {
                events.record(
                    None,
                    endpoint,
                    DeviceEventKind::State {
                        from: from.as_ref().map(device_state_label),
                        to: device_state_label(&to),
                    },
                );
            }
            DeviceEvent::ParseAnomaly { detail } => {
                events.record(None, endpoint, DeviceEventKind::Anomaly { detail });
            }
            DeviceEvent::TxFrame { frame } => {
                if events.capture() {
                    events.record(None, endpoint, DeviceEventKind::Tx { frame });
                }
            }
            DeviceEvent::Progress { .. } => {}
        }
    })
}

/// The management-operation sink: capture + mirror log lines, and feed
/// the CARD-OWNED op flow (state-flow model §2): progress ticks keep it
/// Running with the live label/percent; a mid-manage device state change
/// (reboot, re-enumeration — the op's EXPECTED gap) flips it to
/// AwaitingDevice wearing `awaiting_detail` (I2).
///
/// The card flow is the WHOLE narration since the retired step-stack
/// device pane went away — this sink used to drive a parallel
/// `UiActivityView` into that pane's Device section as well.
pub(crate) fn management_event_sink(
    updates: UxUpdateSink,
    captured_logs: Rc<RefCell<Vec<UiLogDraft>>>,
    card_op: Rc<RefCell<crate::CardOp>>,
    session_key: String,
    awaiting_detail: &'static str,
) -> DeviceEventSink {
    // Publishing the op slot is two steps that must not come apart: the
    // controller's authoritative slot (what the next full view build
    // reads) and the delta that carries the same value to the live view
    // mid-flight. The op runs holding `&mut controller`, so the delta is
    // the ONLY way the card moves before the op settles.
    let publish_card_op = {
        let updates = updates.clone();
        let card_op = Rc::clone(&card_op);
        move |op: crate::CardOp| {
            *card_op.borrow_mut() = op.clone();
            updates.emit(UxUpdate::CardOp {
                session_key: session_key.clone(),
                op,
            });
        }
    };
    DeviceEventSink::new(move |event| match event {
        DeviceEvent::LogLine { line, origin } => {
            if line.trim().is_empty() {
                return;
            }
            let draft = log_line_draft(&line, origin);
            captured_logs.borrow_mut().push(draft.clone());
            updates.emit(UxUpdate::Log(draft));
        }
        DeviceEvent::Progress { label, percent } => {
            publish_card_op(crate::CardOp::new(format!("{label}…"), percent));
        }
        DeviceEvent::State { to: state, .. } => {
            // The device leaving Ready mid-manage is the expected gap;
            // reaching Ready again means the flow is finishing (the
            // settle half narrates from there).
            if !matches!(
                state,
                lpa_link::DeviceState::Ready { .. } | lpa_link::DeviceState::Booting
            ) {
                return;
            }
            if matches!(state, lpa_link::DeviceState::Booting) {
                publish_card_op(crate::CardOp::awaiting(awaiting_detail));
            }
        }
        // Emitted only on the session's own connect-time sink, never on the
        // per-operation manage sink; nothing to narrate here either way.
        DeviceEvent::ParseAnomaly { .. } | DeviceEvent::TxFrame { .. } => {}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_lines_parse_into_device_origin_drafts() {
        let pending = Rc::new(RefCell::new(Vec::new()));
        let sink = console_event_sink(Rc::clone(&pending), DeviceEventRecorder::noop(), None);

        sink.emit(DeviceEvent::LogLine {
            line: "boot: chip revision v0.1".to_string(),
            origin: DeviceLineOrigin::Device,
        });
        sink.emit(DeviceEvent::State {
            from: None,
            to: lpa_link::DeviceState::Booting,
        });

        let drafts = pending.borrow();
        assert_eq!(drafts.len(), 1, "state events produce no drafts");
        assert_eq!(drafts[0].source.origin, UiLogOrigin::Device);
    }

    #[test]
    fn the_console_sink_records_transitions_and_anomalies_into_the_event_log() {
        use crate::core::log::DeviceEventLog;

        let log = Rc::new(RefCell::new(DeviceEventLog::new()));
        let recorder = DeviceEventRecorder::new(Rc::clone(&log), Rc::new(|| 7.0));
        let sink = console_event_sink(
            Rc::new(RefCell::new(Vec::new())),
            recorder,
            Some("serial-1".to_string()),
        );

        sink.emit(DeviceEvent::State {
            from: None,
            to: lpa_link::DeviceState::Booting,
        });
        sink.emit(DeviceEvent::State {
            from: Some(lpa_link::DeviceState::Booting),
            to: lpa_link::DeviceState::Gone,
        });
        sink.emit(DeviceEvent::ParseAnomaly {
            detail: "malformed M! frame: eof".to_string(),
        });
        // raw traffic outside capture mode is not recorded
        sink.emit(DeviceEvent::LogLine {
            line: "boot: banner".to_string(),
            origin: DeviceLineOrigin::Device,
        });
        sink.emit(DeviceEvent::TxFrame {
            frame: "{}".to_string(),
        });

        let log = log.borrow();
        let kinds: Vec<_> = log.iter().map(|record| record.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                DeviceEventKind::State {
                    from: None,
                    to: "booting".to_string(),
                },
                DeviceEventKind::State {
                    from: Some("booting".to_string()),
                    to: "gone".to_string(),
                },
                DeviceEventKind::Anomaly {
                    detail: "malformed M! frame: eof".to_string(),
                },
            ]
        );
        assert_eq!(log.anomaly_count("serial-1"), 1);
        assert!(
            log.iter()
                .all(|record| record.endpoint.as_deref() == Some("serial-1")),
            "connect-time records attribute to the endpoint"
        );
    }

    #[test]
    fn the_console_sink_records_raw_traffic_in_capture_mode() {
        use crate::core::log::DeviceEventLog;

        let log = Rc::new(RefCell::new(DeviceEventLog::new()));
        log.borrow_mut().set_capture(true);
        let recorder = DeviceEventRecorder::new(Rc::clone(&log), Rc::new(|| 7.0));
        let sink = console_event_sink(Rc::new(RefCell::new(Vec::new())), recorder, None);

        sink.emit(DeviceEvent::LogLine {
            line: "boot: banner".to_string(),
            origin: DeviceLineOrigin::Device,
        });
        sink.emit(DeviceEvent::TxFrame {
            frame: "{\"t\":\"ping\"}".to_string(),
        });

        let log = log.borrow();
        let kinds: Vec<_> = log.iter().map(|record| record.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                DeviceEventKind::Rx {
                    line: "boot: banner".to_string(),
                },
                DeviceEventKind::Tx {
                    frame: "{\"t\":\"ping\"}".to_string(),
                },
            ]
        );
    }

    #[test]
    fn management_lines_are_captured_and_mirrored_as_log_updates() {
        let updates = Rc::new(RefCell::new(Vec::new()));
        let sink_updates = UxUpdateSink::new({
            let updates = Rc::clone(&updates);
            move |update| updates.borrow_mut().push(update)
        });
        let captured = Rc::new(RefCell::new(Vec::new()));
        let card_op = Rc::new(RefCell::new(crate::CardOp::new("Flashing firmware…", None)));
        let sink = management_event_sink(
            sink_updates,
            Rc::clone(&captured),
            Rc::clone(&card_op),
            "runtime-3".to_string(),
            "Waiting for firmware boot",
        );

        sink.emit(DeviceEvent::LogLine {
            line: "Writing at 0x10000...".to_string(),
            origin: DeviceLineOrigin::Link,
        });
        sink.emit(DeviceEvent::Progress {
            label: "Writing".to_string(),
            percent: Some(42),
        });

        assert_eq!(captured.borrow().len(), 1);
        assert_eq!(captured.borrow()[0].source.origin, UiLogOrigin::Link);
        // progress feeds the card-owned op flow (model §2) — the ONLY
        // narration now that the step-stack device pane is gone
        assert_eq!(
            *card_op.borrow(),
            crate::CardOp::new("Writing…", Some(42)),
            "progress ticks keep the flow Running with the live label"
        );
        let updates = updates.borrow();
        // The slot alone never reaches the screen mid-op: the op holds
        // `&mut controller`, so the delta is what moves the card. It is
        // now the ONLY delta — the parallel `Activity` update retired with
        // the step-stack device pane it patched.
        assert!(
            matches!(
                updates.as_slice(),
                [
                    UxUpdate::Log(_),
                    UxUpdate::CardOp { session_key, op },
                ] if session_key == "runtime-3"
                    && *op == crate::CardOp::new("Writing…", Some(42))
            ),
            "a progress tick mirrors the log and publishes the card op: {updates:?}"
        );
    }

    #[test]
    fn a_mid_manage_reboot_flips_the_op_flow_to_awaiting_device() {
        let emitted = Rc::new(RefCell::new(Vec::new()));
        let updates = UxUpdateSink::new({
            let emitted = Rc::clone(&emitted);
            move |update| emitted.borrow_mut().push(update)
        });
        let card_op = Rc::new(RefCell::new(crate::CardOp::new("Wiping device…", None)));
        let sink = management_event_sink(
            updates,
            Rc::new(RefCell::new(Vec::new())),
            Rc::clone(&card_op),
            "runtime-1".to_string(),
            "Checking for LightPlayer firmware",
        );

        sink.emit(DeviceEvent::State {
            from: Some(lpa_link::DeviceState::Gone),
            to: lpa_link::DeviceState::Booting,
        });

        assert_eq!(
            *card_op.borrow(),
            crate::CardOp::awaiting("Checking for LightPlayer firmware"),
            "the expected disconnect is the AwaitingDevice phase (I2)"
        );
        assert!(
            matches!(
                emitted.borrow().first(),
                Some(UxUpdate::CardOp { session_key, op })
                    if session_key == "runtime-1"
                        && *op == crate::CardOp::awaiting("Checking for LightPlayer firmware")
            ),
            "the phase change reaches the live card, not just the slot"
        );
    }
}
