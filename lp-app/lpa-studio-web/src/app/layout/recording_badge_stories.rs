//! Stories for the session recorder's badge (`?record=`).

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::layout::recording_badge::RecordingBadgeView;
use crate::device_events_io::RecordingStatus;

#[story(
    description = "The recording pill: streaming, streaming with lines that failed to send, and a refused sink."
)]
pub(crate) fn states() -> Element {
    rsx! {
        div { class: "tw:flex tw:flex-col tw:items-start tw:gap-3 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-4",
            RecordingBadgeView {
                status: RecordingStatus::Recording {
                    host: "127.0.0.1:52811".to_string(),
                    failed_lines: 0,
                },
            }
            RecordingBadgeView {
                status: RecordingStatus::Recording {
                    host: "127.0.0.1:52811".to_string(),
                    failed_lines: 12,
                },
            }
            RecordingBadgeView {
                status: RecordingStatus::Refused {
                    host: "collector.example.com".to_string(),
                    reason: "collector.example.com is not on this machine or local network"
                        .to_string(),
                },
            }
        }
    }
}
