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

use lpa_link::{DeviceEvent, DeviceEventSink, DeviceLineOrigin};

use crate::app::server::device_log_line::parse_device_log_line;
use crate::{
    UiActivityView, UiLogDraft, UiLogLevel, UiLogOrigin, UiLogSource, UiProgress, UiStatus,
    UxActivityTarget, UxUpdate, UxUpdateSink,
};

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
/// controller to drain into its log ring.
pub(crate) fn console_event_sink(pending: Rc<RefCell<Vec<UiLogDraft>>>) -> DeviceEventSink {
    DeviceEventSink::new(move |event| {
        if let DeviceEvent::LogLine { line, origin } = event {
            pending.borrow_mut().push(log_line_draft(&line, origin));
        }
    })
}

/// The management-operation sink: capture + mirror log lines, drive the
/// live activity view from progress events — and feed the CARD-OWNED op
/// flow (state-flow model §2): progress ticks keep it Running with the
/// live label/percent; a mid-manage device state change (reboot,
/// re-enumeration — the op's EXPECTED gap) flips it to AwaitingDevice
/// wearing `awaiting_detail` (I2).
pub(crate) fn management_event_sink(
    updates: UxUpdateSink,
    target: UxActivityTarget,
    activity: Rc<RefCell<UiActivityView>>,
    captured_logs: Rc<RefCell<Vec<UiLogDraft>>>,
    card_op: Rc<RefCell<crate::CardOp>>,
    card_uid: Option<String>,
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
                uid: card_uid.clone(),
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
            {
                let mut activity = activity.borrow_mut();
                activity.progress = Some(match percent {
                    Some(percent) => UiProgress::determinate(&label, u32::from(percent)),
                    None => UiProgress::indeterminate(&label),
                });
            }
            publish_card_op(crate::CardOp::new(format!("{label}…"), percent));
            updates.emit(UxUpdate::Activity {
                target: target.clone(),
                status: UiStatus::working("Managing"),
                activity: activity.borrow().clone(),
            });
        }
        DeviceEvent::State { state } => {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_lines_parse_into_device_origin_drafts() {
        let pending = Rc::new(RefCell::new(Vec::new()));
        let sink = console_event_sink(Rc::clone(&pending));

        sink.emit(DeviceEvent::LogLine {
            line: "boot: chip revision v0.1".to_string(),
            origin: DeviceLineOrigin::Device,
        });
        sink.emit(DeviceEvent::State {
            state: lpa_link::DeviceState::Booting,
        });

        let drafts = pending.borrow();
        assert_eq!(drafts.len(), 1, "state events produce no drafts");
        assert_eq!(drafts[0].source.origin, UiLogOrigin::Device);
    }

    #[test]
    fn management_lines_are_captured_and_mirrored_as_log_updates() {
        let updates = Rc::new(RefCell::new(Vec::new()));
        let sink_updates = UxUpdateSink::new({
            let updates = Rc::clone(&updates);
            move |update| updates.borrow_mut().push(update)
        });
        let captured = Rc::new(RefCell::new(Vec::new()));
        let activity = Rc::new(RefCell::new(UiActivityView::new("Flashing firmware")));
        let card_op = Rc::new(RefCell::new(crate::CardOp::new("Flashing firmware…", None)));
        let sink = management_event_sink(
            sink_updates,
            UxActivityTarget::pane(crate::ControllerId::new("studio|device")),
            Rc::clone(&activity),
            Rc::clone(&captured),
            Rc::clone(&card_op),
            Some("dev_abc".to_string()),
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
        assert!(activity.borrow().progress.is_some());
        // progress feeds the card-owned op flow (model §2)
        assert_eq!(
            *card_op.borrow(),
            crate::CardOp::new("Writing…", Some(42)),
            "progress ticks keep the flow Running with the live label"
        );
        let updates = updates.borrow();
        assert!(matches!(updates[0], UxUpdate::Log(_)));
        // The slot alone never reaches the screen mid-op: the op holds
        // `&mut controller`, so the delta is what moves the card.
        assert!(
            matches!(
                &updates[1],
                UxUpdate::CardOp { uid, op }
                    if uid.as_deref() == Some("dev_abc")
                        && *op == crate::CardOp::new("Writing…", Some(42))
            ),
            "a progress tick publishes the card op, not just the slot: {:?}",
            updates[1]
        );
        assert!(matches!(updates[2], UxUpdate::Activity { .. }));
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
            UxActivityTarget::pane(crate::ControllerId::new("studio|device")),
            Rc::new(RefCell::new(UiActivityView::new("Wiping device"))),
            Rc::new(RefCell::new(Vec::new())),
            Rc::clone(&card_op),
            None,
            "Checking for LightPlayer firmware",
        );

        sink.emit(DeviceEvent::State {
            state: lpa_link::DeviceState::Booting,
        });

        assert_eq!(
            *card_op.borrow(),
            crate::CardOp::awaiting("Checking for LightPlayer firmware"),
            "the expected disconnect is the AwaitingDevice phase (I2)"
        );
        assert!(
            matches!(
                emitted.borrow().first(),
                Some(UxUpdate::CardOp { uid: None, op })
                    if *op == crate::CardOp::awaiting("Checking for LightPlayer firmware")
            ),
            "the phase change reaches the live card, not just the slot"
        );
    }
}
