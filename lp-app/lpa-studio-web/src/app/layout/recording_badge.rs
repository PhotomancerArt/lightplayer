//! The session recorder's badge: a small pill, on every route, while the
//! page carries `?record=` — "● Recording → 127.0.0.1:4321", with a count
//! of lines whose POST failed, or the refusal when the sink was not on
//! this machine or the local network (`record_sink.rs`).
//!
//! A recording is the whole session, unredacted, so it must never be
//! invisible. The pill sits at the bottom-left corner and lets pointer
//! events through, so on a phone-width page it can overlap a control
//! without ever taking its click.
//!
//! [`RecordingBadgeView`] is pure (stories render it); [`RecordingBadge`]
//! polls the recorder's status once a second — the status lives in the
//! recorder's thread-local, written from fetch callbacks that run outside
//! any component, so a poll is simpler and cheaper than a signal bridge.

use dioxus::prelude::*;

use crate::device_events_io::RecordingStatus;

/// The pill's text for a status (`None` when nothing is shown).
pub(crate) fn badge_text(status: &RecordingStatus) -> Option<String> {
    match status {
        RecordingStatus::Off => None,
        RecordingStatus::Recording {
            host,
            failed_lines: 0,
        } => Some(format!("● Recording → {host}")),
        RecordingStatus::Recording { host, failed_lines } => Some(format!(
            "● Recording → {host} · {failed_lines} line{} not sent",
            if *failed_lines == 1 { "" } else { "s" }
        )),
        RecordingStatus::Refused { host, .. } => Some(format!(
            "Recording refused: {host} is not on this machine or local network"
        )),
    }
}

/// The pill itself, for a status. Renders nothing when recording is off.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn RecordingBadgeView(status: RecordingStatus) -> Element {
    let Some(text) = badge_text(&status) else {
        return rsx! {};
    };
    let tone = match &status {
        RecordingStatus::Recording {
            failed_lines: 0, ..
        } => "tw:border-status-error-border tw:bg-status-error-bg tw:text-status-error-foreground",
        _ => {
            "tw:border-status-warning-border tw:bg-status-warning-bg tw:text-status-warning-foreground"
        }
    };
    let title = match &status {
        RecordingStatus::Refused { reason, .. } => format!("Session recorder: {reason}"),
        _ => "Session recorder (?record=): this page's session, device traffic included, is being sent to this address".to_string(),
    };
    rsx! {
        div {
            class: "tw:inline-flex tw:max-w-[calc(100vw-16px)] tw:items-center tw:truncate tw:rounded-full tw:border tw:px-2.5 tw:py-0.5 tw:text-[11px] tw:font-semibold tw:leading-5 tw:shadow-md {tone}",
            role: "status",
            title,
            "{text}"
        }
    }
}

/// The badge as the app mounts it: fixed at the bottom-left, click-through,
/// polling the recorder's status.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn RecordingBadge() -> Element {
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(unused_mut, reason = "polled on wasm only")
    )]
    let mut status = use_signal(crate::device_events_io::recording_status);
    #[cfg(target_arch = "wasm32")]
    use_future(move || async move {
        if matches!(*status.peek(), RecordingStatus::Off) {
            return;
        }
        loop {
            gloo_timers::future::TimeoutFuture::new(1000).await;
            let now = crate::device_events_io::recording_status();
            if *status.peek() != now {
                status.set(now);
            }
        }
    });
    let status = status();
    if matches!(status, RecordingStatus::Off) {
        return rsx! {};
    }
    rsx! {
        div { class: "tw:pointer-events-none tw:fixed tw:bottom-2 tw:left-2 tw:z-[90]",
            RecordingBadgeView { status }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_shows_nothing() {
        assert_eq!(badge_text(&RecordingStatus::Off), None);
    }

    #[test]
    fn recording_names_the_host_and_counts_failures() {
        let host = "127.0.0.1:4321".to_string();
        assert_eq!(
            badge_text(&RecordingStatus::Recording {
                host: host.clone(),
                failed_lines: 0
            })
            .as_deref(),
            Some("● Recording → 127.0.0.1:4321")
        );
        assert_eq!(
            badge_text(&RecordingStatus::Recording {
                host,
                failed_lines: 3
            })
            .as_deref(),
            Some("● Recording → 127.0.0.1:4321 · 3 lines not sent")
        );
    }

    #[test]
    fn a_refusal_says_why() {
        assert_eq!(
            badge_text(&RecordingStatus::Refused {
                host: "evil.example".to_string(),
                reason: "evil.example is not on this machine or local network".to_string(),
            })
            .as_deref(),
            Some("Recording refused: evil.example is not on this machine or local network")
        );
    }
}
