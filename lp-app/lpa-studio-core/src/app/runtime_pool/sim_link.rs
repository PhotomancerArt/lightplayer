//! Opening the SIMULATOR's link: the browser-worker attachment plus the
//! `lpa-link` → UX adapters the sim path needs.
//!
//! The sim used to reach its worker through `DeviceController::open_provider`
//! — one connect flow serving hardware AND the simulator, with the sim as
//! the one arm that had no states, no readiness and no card ceremony. M2 of
//! the device-model rebuild deleted that controller; this is the sim's own
//! half, lifted out verbatim: discover the browser-worker endpoint, connect
//! it, hand back a [`SimAttachment`] plus the provider's log/diagnostic
//! replay. The retired `link_ux` module's error and log folds live here too
//! — lpa-link stays UX-free, and the sim is what still needs the fold.

use std::rc::Rc;

use lpa_link::providers::{LinkEnv, LinkProviderRegistry};
use lpa_link::{
    DeviceTimers, LinkConnector, LinkDiagnosticSeverity, LinkEndpointId, LinkError, LinkLogLevel,
    LinkProvider, LinkProviderKind, LinkSessionId,
};

use crate::{UiError, UiLogDraft, UiLogLevel, UiLogOrigin, UiLogSource};

use super::runtime_session::SimAttachment;

/// How many rungs the simulator connect ladder walks before giving up.
const SIM_CONNECT_ATTEMPTS: u32 = 3;
/// The breath between simulator connect attempts: short, because the
/// worker boot the next rung starts carries the long wait itself.
const SIM_CONNECT_RETRY_BACKOFF: core::time::Duration = core::time::Duration::from_millis(400);

/// The studio's door to the simulator runtime: the provider registry (a
/// memoized connector per kind) plus the injected timer factory the retry
/// ladder sleeps on.
pub struct SimLink {
    registry: LinkProviderRegistry,
    timers: DeviceTimers,
}

impl SimLink {
    pub fn new() -> Self {
        Self {
            registry: LinkProviderRegistry::from_env(LinkEnv::default()),
            // Replaced by the shell's real timers (`set_timers`); the
            // default resolves instantly, which is what host tests want.
            timers: DeviceTimers::new(|_| Box::pin(std::future::ready(()))),
        }
    }

    /// Install the platform timer factory (the web shell's `gloo` sleep).
    pub fn set_timers(&mut self, timers: DeviceTimers) {
        self.timers = timers;
    }

    /// Start a simulator runtime: discover the browser-worker endpoint and
    /// connect it, walking the bounded retry ladder.
    pub async fn open(&mut self) -> Result<(SimAttachment, Vec<UiLogDraft>), UiError> {
        let connector = self
            .registry
            .create_connector(LinkProviderKind::BrowserWorker)
            .map_err(map_link_error)?;
        let endpoints = connector.discover().await.map_err(map_link_error)?;
        let endpoint = endpoints.first().ok_or_else(|| {
            UiError::Link(format!(
                "{} did not report any endpoints",
                LinkProviderKind::BrowserWorker.label()
            ))
        })?;
        let endpoint_id = endpoint.id.clone();
        open_sim_attachment_ladder(connector, &endpoint_id, &self.timers).await
    }
}

impl Default for SimLink {
    fn default() -> Self {
        Self::new()
    }
}

/// Open the simulator attachment, walking a bounded RETRY LADDER:
/// [`SIM_CONNECT_ATTEMPTS`] rungs with a [`SIM_CONNECT_RETRY_BACKOFF`]
/// breath between them.
///
/// Each rung connects through a FRESH browser worker — the previous one
/// terminates when its handle drops, so a boot that timed out is never
/// left fetching wasm against the retry. The final error names the
/// attempt count and the last cause, so an exhausted ladder never reads
/// like a single unlucky failure.
async fn open_sim_attachment_ladder(
    connector: Rc<LinkConnector>,
    endpoint_id: &LinkEndpointId,
    timers: &DeviceTimers,
) -> Result<(SimAttachment, Vec<UiLogDraft>), UiError> {
    let mut last_error: Option<UiError> = None;
    for attempt in 1..=SIM_CONNECT_ATTEMPTS {
        match open_sim_attachment(Rc::clone(&connector), endpoint_id).await {
            Ok(result) => return Ok(result),
            Err(error) => {
                log::warn!(
                    "simulator connect attempt {attempt}/{SIM_CONNECT_ATTEMPTS} failed: {}",
                    error.message()
                );
                last_error = Some(error);
            }
        }
        if attempt < SIM_CONNECT_ATTEMPTS {
            timers.sleep(SIM_CONNECT_RETRY_BACKOFF).await;
        }
    }
    let cause = last_error
        .as_ref()
        .map_or("no failure was recorded", UiError::message);
    Err(UiError::Link(format!(
        "simulator runtime did not start after {SIM_CONNECT_ATTEMPTS} attempts: {cause}"
    )))
}

/// Open the simulator attachment: connect + connection handoff (no
/// readiness — boot-ready IS the session, D22).
async fn open_sim_attachment(
    connector: Rc<LinkConnector>,
    endpoint_id: &LinkEndpointId,
) -> Result<(SimAttachment, Vec<UiLogDraft>), UiError> {
    let session = connector
        .connect(endpoint_id)
        .await
        .map_err(map_link_error)?;
    let connection = match connector.connection(session.id()).await {
        Ok(connection) => connection,
        Err(error) => {
            let _ = connector.close(session.id()).await;
            return Err(map_link_error(error));
        }
    };
    let logs = match link_session_logs(&connector, session.id()) {
        Ok(logs) => logs,
        Err(error) => {
            let _ = connector.close(session.id()).await;
            return Err(error);
        }
    };
    Ok((
        SimAttachment {
            connector,
            session,
            connection,
        },
        logs,
    ))
}

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
