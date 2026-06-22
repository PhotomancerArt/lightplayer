use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::{LinkManagementProgress, LinkManagementResult};

/// Live event emitted while a link management operation is running.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkManagementEvent {
    Log { message: String },
    Progress(LinkManagementProgress),
}

impl LinkManagementEvent {
    pub fn log(message: impl Into<String>) -> Self {
        Self::Log {
            message: message.into(),
        }
    }

    pub fn progress(progress: LinkManagementProgress) -> Self {
        Self::Progress(progress)
    }
}

/// Cloneable in-process sink for live management events.
#[derive(Clone)]
pub struct LinkManagementEventSink {
    on_event: Rc<dyn Fn(LinkManagementEvent)>,
}

impl LinkManagementEventSink {
    pub fn new(on_event: impl Fn(LinkManagementEvent) + 'static) -> Self {
        Self {
            on_event: Rc::new(on_event),
        }
    }

    pub fn noop() -> Self {
        Self::new(|_| {})
    }

    pub fn emit(&self, event: LinkManagementEvent) {
        (self.on_event)(event);
    }
}

pub fn emit_management_result_events(
    result: &LinkManagementResult,
    sink: &LinkManagementEventSink,
) {
    for message in result.logs() {
        sink.emit(LinkManagementEvent::log(message.clone()));
    }
    for progress in result.progress() {
        sink.emit(LinkManagementEvent::progress(progress.clone()));
    }
}

impl LinkManagementResult {
    fn logs(&self) -> &[String] {
        match self {
            Self::ResetRuntime => &[],
            Self::FlashFirmware(result) => &result.logs,
            Self::EraseDeviceFlash(result) => &result.logs,
            Self::EraseRawFilesystem(result) => &result.logs,
        }
    }

    fn progress(&self) -> &[LinkManagementProgress] {
        match self {
            Self::ResetRuntime => &[],
            Self::FlashFirmware(result) => &result.progress,
            Self::EraseDeviceFlash(result) => &result.progress,
            Self::EraseRawFilesystem(result) => &result.progress,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::{LinkEraseDeviceResult, LinkManagementEvent, LinkManagementProgress};

    use super::*;

    #[test]
    fn result_events_replay_logs_then_progress() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = LinkManagementEventSink::new({
            let events = Rc::clone(&events);
            move |event| {
                events.borrow_mut().push(event);
            }
        });
        let result = LinkManagementResult::EraseDeviceFlash(LinkEraseDeviceResult {
            chip_name: Some("ESP32-C6".to_string()),
            logs: vec!["bootloader connected".to_string()],
            progress: vec![LinkManagementProgress::new("Erasing").with_percent(50)],
        });

        emit_management_result_events(&result, &sink);

        assert_eq!(
            *events.borrow(),
            vec![
                LinkManagementEvent::log("bootloader connected"),
                LinkManagementEvent::progress(
                    LinkManagementProgress::new("Erasing").with_percent(50)
                )
            ]
        );
    }
}
