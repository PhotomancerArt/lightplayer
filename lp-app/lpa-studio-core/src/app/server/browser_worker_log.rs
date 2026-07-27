//! Mapping of `fw-browser` worker output into UI log drafts.
//!
//! The worker transport itself is wasm-only (`browser_worker_client_io`), but
//! this mapping is pure logging policy, so it lives in an ungated module and
//! is unit-tested on the host.

use crate::{UiLogDraft, UiLogLevel, UiLogOrigin, UiLogSource};

/// Source detail for worker lifecycle/status lines, which carry no log target
/// of their own.
pub const WORKER_STATUS_DETAIL: &str = "fw-browser";

/// Map a worker `Log` envelope to a draft: origin `Device`, the worker's log
/// `target` (a module path) as display-only detail, and the level string
/// parsed with `trace` preserved as [`UiLogLevel::Trace`].
pub fn worker_log_draft(level: &str, target: String, message: String) -> UiLogDraft {
    UiLogDraft::new(
        parse_worker_log_level(level),
        UiLogSource::with_detail(UiLogOrigin::Device, target),
        message,
    )
}

/// Map a worker `Status` envelope to an Info draft labeled
/// [`WORKER_STATUS_DETAIL`]. The optional human message wins over the raw
/// status token.
///
/// The `"fatal"` status — the worker's sticky poisoned-instance report,
/// whose message carries the primary panic — maps to an Error draft: it is
/// the one console line that explains a sim crash, so it must not drown at
/// the Info floor.
pub fn worker_status_draft(status: String, message: Option<String>) -> UiLogDraft {
    let level = if status == "fatal" {
        UiLogLevel::Error
    } else {
        UiLogLevel::Info
    };
    UiLogDraft::new(
        level,
        UiLogSource::with_detail(UiLogOrigin::Device, WORKER_STATUS_DETAIL),
        message.unwrap_or(status),
    )
}

/// Parse the worker's lowercase level strings. Unknown strings read as
/// `Info` so unexpected worker output stays visible by default.
pub fn parse_worker_log_level(level: &str) -> UiLogLevel {
    match level {
        "trace" => UiLogLevel::Trace,
        "debug" => UiLogLevel::Debug,
        "warn" => UiLogLevel::Warn,
        "error" => UiLogLevel::Error,
        _ => UiLogLevel::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_trace_level_is_preserved_as_trace() {
        assert_eq!(parse_worker_log_level("trace"), UiLogLevel::Trace);
        assert_eq!(parse_worker_log_level("debug"), UiLogLevel::Debug);
        assert_eq!(parse_worker_log_level("info"), UiLogLevel::Info);
        assert_eq!(parse_worker_log_level("warn"), UiLogLevel::Warn);
        assert_eq!(parse_worker_log_level("error"), UiLogLevel::Error);
    }

    #[test]
    fn unknown_worker_level_reads_as_info() {
        assert_eq!(parse_worker_log_level("verbose"), UiLogLevel::Info);
        assert_eq!(parse_worker_log_level(""), UiLogLevel::Info);
    }

    #[test]
    fn fatal_status_maps_to_an_error_draft_with_the_primary_panic() {
        let draft = worker_status_draft(
            "fatal".to_string(),
            Some("browser worker instance fatal: panicked at 'boom'".to_string()),
        );
        assert_eq!(draft.level, UiLogLevel::Error);
        assert!(draft.message.contains("panicked at 'boom'"));
    }

    #[test]
    fn ordinary_status_stays_an_info_draft() {
        let draft = worker_status_draft("ready".to_string(), None);
        assert_eq!(draft.level, UiLogLevel::Info);
        assert_eq!(draft.message, "ready");
    }

    #[test]
    fn log_envelope_target_becomes_device_detail() {
        let draft = worker_log_draft(
            "trace",
            "lp_engine::frame".to_string(),
            "rendered frame".to_string(),
        );

        assert_eq!(draft.level, UiLogLevel::Trace);
        assert_eq!(
            draft.source,
            UiLogSource::with_detail(UiLogOrigin::Device, "lp_engine::frame")
        );
        assert_eq!(draft.message, "rendered frame");
    }

    #[test]
    fn status_envelope_is_info_with_fw_browser_detail() {
        let draft = worker_status_draft("ready".to_string(), Some("server booted".to_string()));

        assert_eq!(draft.level, UiLogLevel::Info);
        assert_eq!(
            draft.source,
            UiLogSource::with_detail(UiLogOrigin::Device, "fw-browser")
        );
        assert_eq!(draft.message, "server booted");

        let bare = worker_status_draft("ready".to_string(), None);
        assert_eq!(bare.message, "ready");
    }
}
