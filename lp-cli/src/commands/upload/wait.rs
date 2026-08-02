//! Wait, after a deploy is acked, for evidence the *newly deployed* project
//! is actually running.
//!
//! `upload` resets the device on connect (`cli_connect`), so every session
//! begins with a boot that auto-loads and compiles whatever the *previous*
//! upload left on flash. The deploy's own reload then loads the *new*
//! project — but if the CLI disconnects the instant `LoadProject` is acked,
//! nothing has observed that reload actually settle. The compile line an
//! operator associates with an upload ends up describing the previous one.
//! See `docs/defects/2026-07-30-deploy-compiles-previous-upload.md`.
//!
//! The fix polls `project.read` on the *same* connection the deploy used
//! (the S3 serial port is exclusive — a second handle is not an option)
//! until the freshly loaded project (identified by the `WireProjectHandle`
//! the deploy's own `LoadProject` response returned) has rendered at least
//! one frame, or a node reports a definitive failure.

use std::time::Duration;

use anyhow::{Context, Result};
use lpa_client::{ClientIo, LpClient};
use lpc_wire::{
    NodeRuntimeStatus, ProjectReadEvent, ProjectReadNodeEvent, ProjectReadQueryEvent,
    ProjectReadRequest, WireProjectHandle, WireTreeDelta,
};

/// Polite dev-loop polling cadence.
pub const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Default `--wait-timeout` value, in seconds.
pub const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 30;

/// What one `project.read` poll found out about the freshly loaded project.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RunEvidence {
    /// No settled state yet: keep polling.
    Pending,
    /// The project rendered at least one frame with no node error observed
    /// in the same read.
    Running,
    /// A node (e.g. a shader) reported a definitive failure. Terminal: do
    /// not keep polling.
    Error(String),
}

/// Poll the same connection until there is evidence the project behind
/// `handle` is running, a node reports a definitive failure, or `timeout`
/// elapses.
///
/// Returns `Ok(())` once the project has rendered a frame with no node
/// error observed. Returns `Err` on a node failure (reported immediately,
/// without waiting out the timeout) or once `timeout` elapses with no
/// settled state either way.
pub async fn wait_for_project_running<Io: ClientIo>(
    client: &mut LpClient<Io>,
    handle: WireProjectHandle,
    timeout: Duration,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            anyhow::bail!(timeout_message(timeout));
        }

        let read = tokio::time::timeout_at(
            deadline,
            client.project_read(handle, ProjectReadRequest::default_debug(None)),
        )
        .await;

        let events = match read {
            Ok(Ok(outcome)) => outcome.into_value(),
            Ok(Err(error)) => {
                return Err(error)
                    .map_err(|error| anyhow::anyhow!("{error}"))
                    .context("failed to poll project status while waiting for it to run");
            }
            Err(_elapsed) => anyhow::bail!(timeout_message(timeout)),
        };

        match evaluate_run_evidence(&events) {
            RunEvidence::Running => return Ok(()),
            RunEvidence::Error(message) => {
                anyhow::bail!(
                    "deploy was acked, but the deployed project failed to run: {message}"
                );
            }
            RunEvidence::Pending => {}
        }

        let sleep_until = std::cmp::min(deadline, tokio::time::Instant::now() + POLL_INTERVAL);
        tokio::time::sleep_until(sleep_until).await;
    }
}

fn timeout_message(timeout: Duration) -> String {
    format!(
        "deploy was acked, but no evidence the project is running arrived within {:.1}s \
         (the project may still be booting, or the wait needs more time via --wait-timeout)",
        timeout.as_secs_f64()
    )
}

/// Reduce one `project.read` event stream to run/pending/error evidence.
///
/// The read is scoped to `handle` by the server (an invalid/stale handle
/// surfaces as a top-level protocol error, not inside this stream), so a
/// non-empty response here is necessarily about the project this connection
/// just loaded — no separate project-uid check is needed.
fn evaluate_run_evidence(events: &[ProjectReadEvent]) -> RunEvidence {
    let mut rendered = false;
    for event in events {
        match event {
            ProjectReadEvent::Error { message } => return RunEvidence::Error(message.clone()),
            ProjectReadEvent::Query {
                event: ProjectReadQueryEvent::Runtime(runtime),
                ..
            } => {
                if runtime.project.frame_num > 0 {
                    rendered = true;
                }
            }
            ProjectReadEvent::Query {
                event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::TreeDeltas { deltas }),
                ..
            } => {
                for delta in deltas {
                    if let Some(message) = node_failure_message(delta) {
                        return RunEvidence::Error(message);
                    }
                }
            }
            _ => {}
        }
    }
    if rendered {
        RunEvidence::Running
    } else {
        RunEvidence::Pending
    }
}

/// A definitive per-node failure (compile error, init error), if `delta`
/// carries one. `NodeRuntimeStatus::Created` (not yet attempted) and
/// `Warn` (running, degraded) are not terminal — only `Error`/`InitError`
/// end the wait early.
fn node_failure_message(delta: &WireTreeDelta) -> Option<String> {
    let status = match delta {
        WireTreeDelta::Created { status, .. } | WireTreeDelta::EntryChanged { status, .. } => {
            status
        }
        WireTreeDelta::ChildrenChanged { .. } => return None,
    };
    match status {
        NodeRuntimeStatus::Error(message) | NodeRuntimeStatus::InitError(message) => {
            Some(message.clone())
        }
        // Unsupported is not a failure of the upload: the project loaded,
        // this build just has no runtime for that kind.
        NodeRuntimeStatus::Created
        | NodeRuntimeStatus::Ok
        | NodeRuntimeStatus::Warn(_)
        | NodeRuntimeStatus::Unsupported(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::{ProjectRuntimeStatus, RuntimeReadResult};

    fn runtime_event(index: u32, frame_num: u64) -> ProjectReadEvent {
        ProjectReadEvent::Query {
            index,
            event: ProjectReadQueryEvent::Runtime(RuntimeReadResult {
                project: ProjectRuntimeStatus {
                    revision: lpc_model::Revision::new(1),
                    overlay_changed_at: lpc_model::Revision::new(0),
                    frame_num,
                    frame_delta_ms: 16,
                    frame_total_ms: 16,
                    demand_root_count: 1,
                    runtime_buffer_count: 0,
                },
                server: None,
            }),
        }
    }

    fn node_status_event(index: u32, status: NodeRuntimeStatus) -> ProjectReadEvent {
        ProjectReadEvent::Query {
            index,
            event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::TreeDeltas {
                deltas: vec![WireTreeDelta::EntryChanged {
                    id: lpc_model::NodeId::new(1),
                    status,
                    state: lpc_wire::WireEntryState::Alive,
                    change_frame: lpc_model::Revision::new(1),
                }],
            }),
        }
    }

    #[test]
    fn pending_before_any_frame_renders() {
        let events = vec![runtime_event(3, 0)];
        assert_eq!(evaluate_run_evidence(&events), RunEvidence::Pending);
    }

    #[test]
    fn running_once_a_frame_has_rendered() {
        let events = vec![runtime_event(3, 1)];
        assert_eq!(evaluate_run_evidence(&events), RunEvidence::Running);
    }

    #[test]
    fn shader_compile_error_is_terminal_even_before_a_frame_renders() {
        let events = vec![
            runtime_event(3, 0),
            node_status_event(
                1,
                NodeRuntimeStatus::Error("shader compile: bad glsl".into()),
            ),
        ];
        assert_eq!(
            evaluate_run_evidence(&events),
            RunEvidence::Error("shader compile: bad glsl".into())
        );
    }

    #[test]
    fn init_error_is_terminal() {
        let events = vec![node_status_event(
            1,
            NodeRuntimeStatus::InitError("bad config".into()),
        )];
        assert_eq!(
            evaluate_run_evidence(&events),
            RunEvidence::Error("bad config".into())
        );
    }

    #[test]
    fn created_and_warn_are_not_terminal() {
        let events = vec![
            node_status_event(1, NodeRuntimeStatus::Created),
            runtime_event(3, 0),
        ];
        assert_eq!(evaluate_run_evidence(&events), RunEvidence::Pending);

        let events = vec![
            node_status_event(1, NodeRuntimeStatus::Warn("slow frame".into())),
            runtime_event(3, 1),
        ];
        assert_eq!(evaluate_run_evidence(&events), RunEvidence::Running);
    }

    #[test]
    fn stream_level_error_is_terminal() {
        let events = vec![ProjectReadEvent::Error {
            message: "connection reset".into(),
        }];
        assert_eq!(
            evaluate_run_evidence(&events),
            RunEvidence::Error("connection reset".into())
        );
    }
}
