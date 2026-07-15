//! UX adapters for `lpa-link` vocabulary: errors, session logs and
//! diagnostics, and management results, folded into console drafts and
//! [`UiError`]s. lpa-link stays UX-free; the fold lives here.

use lpa_link::{
    LinkConnector, LinkDiagnosticSeverity, LinkError, LinkLogLevel, LinkManagementResult,
    LinkProvider, LinkSessionId,
};

use crate::{UiError, UiLogDraft, UiLogLevel, UiLogOrigin, UiLogSource};

pub(crate) fn map_link_error(error: LinkError) -> UiError {
    match error {
        LinkError::Cancelled { message } => UiError::Cancelled(message),
        _ => UiError::Link(error.to_string()),
    }
}

/// A session's provider logs + diagnostics as console drafts.
pub(crate) fn link_session_logs(
    connector: &LinkConnector,
    session_id: &LinkSessionId,
) -> Result<Vec<UiLogDraft>, UiError> {
    let mut logs = connector
        .logs(session_id)
        .map_err(map_link_error)?
        .into_iter()
        .map(link_log_draft)
        .collect::<Vec<_>>();
    logs.extend(
        connector
            .diagnostics(session_id)
            .map_err(map_link_error)?
            .into_iter()
            .map(|diagnostic| {
                UiLogDraft::new(
                    map_diagnostic_level(diagnostic.severity),
                    UiLogOrigin::Link,
                    diagnostic.message,
                )
            }),
    );
    Ok(logs)
}

/// A management result's log/progress replay as console drafts.
pub(crate) fn management_result_logs(result: &LinkManagementResult) -> Vec<UiLogDraft> {
    match result {
        LinkManagementResult::FlashFirmware(result) => {
            let mut logs = result
                .logs
                .iter()
                .map(|message| {
                    UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, message.clone())
                })
                .collect::<Vec<_>>();
            logs.extend(result.progress.iter().map(|progress| {
                UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, progress.label.clone())
            }));
            logs
        }
        LinkManagementResult::EraseDeviceFlash(result) => {
            let mut logs = result
                .logs
                .iter()
                .map(|message| {
                    UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, message.clone())
                })
                .collect::<Vec<_>>();
            logs.extend(result.progress.iter().map(|progress| {
                UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, progress.label.clone())
            }));
            logs
        }
        LinkManagementResult::EraseRawFilesystem(result) => {
            let mut logs = result
                .logs
                .iter()
                .map(|message| {
                    UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, message.clone())
                })
                .collect::<Vec<_>>();
            logs.extend(result.progress.iter().map(|progress| {
                UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Link, progress.label.clone())
            }));
            logs
        }
        LinkManagementResult::ResetRuntime => {
            vec![UiLogDraft::new(
                UiLogLevel::Info,
                UiLogOrigin::Link,
                "runtime reset completed",
            )]
        }
    }
}

/// Map a provider log entry to a console draft: origin `Link`, the endpoint
/// id as display-only detail.
///
/// The session id is deliberately omitted from the detail: providers derive
/// session ids from the endpoint id plus a counter (`{endpoint}:{n}`), and
/// the studio drives at most one session per endpoint, so an
/// `endpoint/session` detail would only repeat the endpoint stem and widen
/// the console's source column.
fn link_log_draft(entry: lpa_link::LinkLogEntry) -> UiLogDraft {
    UiLogDraft::new(
        map_link_log_level(entry.level),
        UiLogSource::with_detail(UiLogOrigin::Link, entry.endpoint_id.as_str()),
        entry.message,
    )
}

/// Link log levels map one-to-one; `Trace` is preserved (it collapsed to
/// `Debug` before the console gained a Trace level).
fn map_link_log_level(level: LinkLogLevel) -> UiLogLevel {
    match level {
        LinkLogLevel::Trace => UiLogLevel::Trace,
        LinkLogLevel::Debug => UiLogLevel::Debug,
        LinkLogLevel::Info => UiLogLevel::Info,
        LinkLogLevel::Warn => UiLogLevel::Warn,
        LinkLogLevel::Error => UiLogLevel::Error,
    }
}

fn map_diagnostic_level(level: LinkDiagnosticSeverity) -> UiLogLevel {
    match level {
        LinkDiagnosticSeverity::Info => UiLogLevel::Info,
        LinkDiagnosticSeverity::Warning => UiLogLevel::Warn,
        LinkDiagnosticSeverity::Error => UiLogLevel::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_log_drafts_preserve_trace_and_carry_endpoint_detail() {
        let entry = lpa_link::LinkLogEntry::new(
            "usb-serial-0",
            Some(LinkSessionId::new("usb-serial-0:1")),
            LinkLogLevel::Trace,
            "probe ok",
        );

        let draft = link_log_draft(entry);

        assert_eq!(draft.level, UiLogLevel::Trace);
        assert_eq!(
            draft.source,
            UiLogSource::with_detail(UiLogOrigin::Link, "usb-serial-0")
        );
        assert_eq!(draft.message, "probe ok");
    }

    #[test]
    fn link_log_levels_map_one_to_one() {
        assert_eq!(map_link_log_level(LinkLogLevel::Trace), UiLogLevel::Trace);
        assert_eq!(map_link_log_level(LinkLogLevel::Debug), UiLogLevel::Debug);
        assert_eq!(map_link_log_level(LinkLogLevel::Info), UiLogLevel::Info);
        assert_eq!(map_link_log_level(LinkLogLevel::Warn), UiLogLevel::Warn);
        assert_eq!(map_link_log_level(LinkLogLevel::Error), UiLogLevel::Error);
    }

    #[test]
    fn cancelled_link_error_maps_to_cancelled_ux_error() {
        let error = map_link_error(LinkError::cancelled("Port selection canceled"));

        assert_eq!(
            error,
            UiError::Cancelled("Port selection canceled".to_string())
        );
    }
}
