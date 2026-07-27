//! [`AgentHostBridge`]: Studio's `lpa_agent::AgentHost` implementation.
//!
//! The agent run executes as a spawned task while the studio actor owns the
//! controller, so the host cannot borrow controller state. Instead it holds
//! a shared snapshot ([`AgentBridgeState`], refreshed at every run start)
//! and stages edits by enqueuing the SAME `AssetEditOp::ApplyBody` action
//! the inline editor's Apply dispatches — one write path, so the save
//! panel's dirty state, acks, verdict chasing, and the live sim all follow
//! for free. `current_source` mirrors staged edits locally so the next turn
//! sees them before the async apply round-trips.
//!
//! The engine-verdict wait rides the same shared cell: the controller
//! writes the target node's latest `(Revision, verdict)` into
//! [`AgentBridgeState::engine`] at the end of every processed batch, the
//! bridge records the pre-stage Revision when it stages, and
//! [`AgentHostBridge::await_engine_verdict`] polls the cell on the injected
//! actor timer until the Revision advances past it (or the budget runs out
//! → `unknown`).

use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use lpa_agent::{AgentHost, EngineVerdict, HostError, HostFuture, ShaderContext};
use lpc_model::{ArtifactLocation, Revision};
use lps_probe::LedPoint;

use crate::app::agent::agent_controller::AgentTimerFactory;
use crate::app::project::agent_support::AgentEngineStatus;
use crate::app::studio::studio_view_channel::CommandSender;
use crate::{
    AssetEditOp, ControllerId, MAX_ASSET_BODY_BYTES, ProjectController, StudioCommand, UiAction,
};

/// One poll step of the engine-verdict wait (matches the verdict-chase
/// tightened pull interval, so each poll can observe one fresh pull).
const ENGINE_POLL_STEP_MS: u32 = 250;

/// The snapshot the bridge serves between run-start refreshes. Written by
/// the controller at every Send; `source` is additionally updated by
/// [`AgentHostBridge::stage_source`] so intra-run reads stay coherent.
#[derive(Clone, Debug, Default)]
pub struct AgentBridgeState {
    /// The shader source artifact edits target (`None` until the first run
    /// resolves it — staging fails cleanly in that window).
    pub artifact: Option<ArtifactLocation>,
    /// The overlay-aware shader source as of the last refresh or staged
    /// edit.
    pub source: String,
    /// Union of all fixtures' mapping points, labeled by fixture node name.
    pub led_points: Vec<LedPoint>,
    /// System-prompt context (bindings, fixture summary, names).
    pub context: ShaderContext,
    /// The target node's latest engine status, written by the controller
    /// after every processed batch (`None` until the node resolves).
    pub engine: Option<AgentEngineStatus>,
}

/// The host handed to `lpa_agent::AgentSession` for one shader node.
pub struct AgentHostBridge {
    state: Rc<RefCell<AgentBridgeState>>,
    tx: CommandSender,
    /// Actor-injected timer factory driving the verdict-wait polls (wasm:
    /// `setTimeout`; tests: instant or scripted futures).
    timer: AgentTimerFactory,
    /// The engine-status Revision observed when the last stage was
    /// enqueued; the verdict wait resolves once the cell advances past it.
    pre_stage_revision: Option<Revision>,
}

impl AgentHostBridge {
    pub fn new(
        state: Rc<RefCell<AgentBridgeState>>,
        tx: CommandSender,
        timer: AgentTimerFactory,
    ) -> Self {
        Self {
            state,
            tx,
            timer,
            pre_stage_revision: None,
        }
    }

    /// The last-known engine verdict, without waiting.
    fn last_known_verdict(&self) -> EngineVerdict {
        match &self.state.borrow().engine {
            Some(status) => status.verdict.clone(),
            None => EngineVerdict::unknown("no engine status observed yet"),
        }
    }

    /// The engine verdict once the status Revision has advanced past the
    /// pre-stage mark; `None` while the cell still shows pre-stage state.
    fn advanced_verdict(&self) -> Option<EngineVerdict> {
        let state = self.state.borrow();
        let status = state.engine.as_ref()?;
        match self.pre_stage_revision {
            // No pre-stage observation: any engine status is fresh info.
            None => Some(status.verdict.clone()),
            Some(pre) => (status.revision > pre).then(|| status.verdict.clone()),
        }
    }
}

impl AgentHost for AgentHostBridge {
    fn current_source(&self) -> Result<String, HostError> {
        let state = self.state.borrow();
        if state.artifact.is_none() {
            return Err(HostError::new("no shader source artifact resolved"));
        }
        Ok(state.source.clone())
    }

    fn stage_source<'a>(&'a mut self, source: &'a str) -> HostFuture<'a, Result<(), HostError>> {
        Box::pin(async move {
            let artifact = self
                .state
                .borrow()
                .artifact
                .clone()
                .ok_or_else(|| HostError::new("no shader source artifact resolved"))?;
            // The tool layer pre-checks its own mirror of this cap; this
            // guard keeps the bridge honest if the two ever drift.
            if source.len() > MAX_ASSET_BODY_BYTES {
                return Err(HostError::new(format!(
                    "source is {} bytes; the asset limit is {MAX_ASSET_BODY_BYTES}",
                    source.len()
                )));
            }
            // Mark the pre-stage engine Revision BEFORE enqueuing the
            // apply, so a status change caused by this very edit always
            // reads as "newer".
            self.pre_stage_revision = self
                .state
                .borrow()
                .engine
                .as_ref()
                .map(|status| status.revision);
            self.tx.send(StudioCommand::Action(
                UiAction::from_op(
                    ControllerId::new(ProjectController::NODE_ID),
                    AssetEditOp::ApplyBody {
                        artifact,
                        bytes: source.as_bytes().to_vec(),
                    },
                )
                .with_summary("Apply the agent's staged shader edit."),
            ));
            self.state.borrow_mut().source = source.to_string();
            Ok(())
        })
    }

    fn await_engine_verdict(&mut self, budget_ms: u32) -> HostFuture<'_, Option<EngineVerdict>> {
        Box::pin(async move {
            if budget_ms == 0 {
                return Some(self.last_known_verdict());
            }
            let mut waited_ms = 0u32;
            loop {
                if let Some(verdict) = self.advanced_verdict() {
                    return Some(verdict);
                }
                if waited_ms >= budget_ms {
                    return Some(EngineVerdict::unknown(format!(
                        "engine verdict not observed within {budget_ms} ms \
                         (the engine may still be compiling; a later call reports it)"
                    )));
                }
                let step = ENGINE_POLL_STEP_MS.min(budget_ms - waited_ms);
                (self.timer.borrow_mut())(Duration::from_millis(u64::from(step))).await;
                waited_ms += step;
            }
        })
    }

    fn led_points(&self) -> Vec<LedPoint> {
        self.state.borrow().led_points.clone()
    }

    fn shader_context(&self) -> ShaderContext {
        self.state.borrow().context.clone()
    }
}

#[cfg(test)]
mod tests {
    use lpa_agent::EngineStatusKind;

    use super::*;
    use crate::app::agent::agent_controller::instant_agent_timer;
    use crate::app::studio::studio_view_channel::command_channel;

    fn bridge_with_artifact() -> (
        AgentHostBridge,
        crate::app::studio::studio_view_channel::CommandReceiver,
        Rc<RefCell<AgentBridgeState>>,
    ) {
        let (tx, rx) = command_channel();
        let state = Rc::new(RefCell::new(AgentBridgeState {
            artifact: Some(ArtifactLocation::file("/shader.glsl")),
            source: "old".to_string(),
            ..AgentBridgeState::default()
        }));
        (
            AgentHostBridge::new(Rc::clone(&state), tx, instant_agent_timer()),
            rx,
            state,
        )
    }

    /// Drive a bridge future to completion with a self-waking waker (the
    /// futures here resolve within a bounded number of polls).
    fn drive<F: core::future::Future>(future: F) -> F::Output {
        use core::task::{Context, Poll};
        use std::sync::Arc;
        use std::task::{Wake, Waker};

        struct NoopWake;
        impl Wake for NoopWake {
            fn wake(self: Arc<Self>) {}
        }

        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = core::pin::pin!(future);
        for _ in 0..10_000 {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => {}
            }
        }
        panic!("bridge future did not complete");
    }

    fn stage(bridge: &mut AgentHostBridge, source: &str) -> Result<(), HostError> {
        drive(bridge.stage_source(source))
    }

    fn verdict(bridge: &mut AgentHostBridge, budget_ms: u32) -> Option<EngineVerdict> {
        drive(bridge.await_engine_verdict(budget_ms))
    }

    fn engine_status(revision: i64, status: EngineStatusKind) -> AgentEngineStatus {
        AgentEngineStatus {
            revision: Revision::new(revision),
            verdict: EngineVerdict {
                status,
                message: (status == EngineStatusKind::Error)
                    .then(|| "missing uniform field `speed`".to_string()),
                line_col: None,
            },
        }
    }

    #[test]
    fn stage_source_enqueues_the_editor_apply_action_and_mirrors_locally() {
        let (mut bridge, rx, _state) = bridge_with_artifact();
        stage(&mut bridge, "new source").expect("stage");

        assert_eq!(bridge.current_source().expect("source"), "new source");
        let batch = rx.try_recv_all_for_test();
        assert_eq!(batch.len(), 1);
        let StudioCommand::Action(action) = &batch[0] else {
            panic!("expected an action, got {:?}", batch[0]);
        };
        assert!(action.is_for_node(ProjectController::NODE_ID));
        assert_eq!(
            action.op_as::<AssetEditOp>(),
            Some(&AssetEditOp::ApplyBody {
                artifact: ArtifactLocation::file("/shader.glsl"),
                bytes: b"new source".to_vec(),
            })
        );
    }

    #[test]
    fn staging_without_an_artifact_is_a_host_error() {
        let (tx, rx) = command_channel();
        let state = Rc::new(RefCell::new(AgentBridgeState::default()));
        let mut bridge = AgentHostBridge::new(state, tx, instant_agent_timer());

        assert!(bridge.current_source().is_err());
        assert!(stage(&mut bridge, "x").is_err());
        assert!(rx.try_recv_all_for_test().is_empty());
    }

    #[test]
    fn oversized_source_is_refused_without_enqueueing() {
        let (mut bridge, rx, _state) = bridge_with_artifact();
        let big = "x".repeat(MAX_ASSET_BODY_BYTES + 1);
        let error = stage(&mut bridge, &big).expect_err("too big");
        assert!(error.message.contains("asset limit"));
        assert!(rx.try_recv_all_for_test().is_empty());
        assert_eq!(bridge.current_source().expect("source"), "old");
    }

    #[test]
    fn zero_budget_reports_the_last_known_status_without_waiting() {
        let (mut bridge, _rx, state) = bridge_with_artifact();
        assert_eq!(
            verdict(&mut bridge, 0).expect("verdict").status,
            EngineStatusKind::Unknown,
            "no status observed yet"
        );
        state.borrow_mut().engine = Some(engine_status(7, EngineStatusKind::Ok));
        assert_eq!(
            verdict(&mut bridge, 0).expect("verdict").status,
            EngineStatusKind::Ok
        );
    }

    #[test]
    fn advanced_revision_resolves_the_wait_with_the_new_verdict() {
        let (mut bridge, _rx, state) = bridge_with_artifact();
        state.borrow_mut().engine = Some(engine_status(7, EngineStatusKind::Ok));
        stage(&mut bridge, "new source").expect("stage");

        // Same revision: still the pre-stage status → keeps polling until
        // the budget runs out (instant timers make this immediate).
        let unknown = verdict(&mut bridge, 500).expect("verdict");
        assert_eq!(unknown.status, EngineStatusKind::Unknown);
        assert!(unknown.message.expect("note").contains("not observed"));

        // Advanced revision: the fresh (error) verdict resolves the wait.
        state.borrow_mut().engine = Some(engine_status(9, EngineStatusKind::Error));
        let fresh = verdict(&mut bridge, 1500).expect("verdict");
        assert_eq!(fresh.status, EngineStatusKind::Error);
        assert_eq!(
            fresh.message.as_deref(),
            Some("missing uniform field `speed`")
        );
    }

    #[test]
    fn wait_polls_the_timer_in_chase_interval_steps() {
        let (tx, _rx) = command_channel();
        let state = Rc::new(RefCell::new(AgentBridgeState {
            artifact: Some(ArtifactLocation::file("/shader.glsl")),
            ..AgentBridgeState::default()
        }));
        let delays: Rc<RefCell<Vec<Duration>>> = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&delays);
        let timer: AgentTimerFactory = Rc::new(RefCell::new(move |delay| {
            seen.borrow_mut().push(delay);
            Box::pin(core::future::ready(()))
                as core::pin::Pin<Box<dyn core::future::Future<Output = ()>>>
        }));
        let mut bridge = AgentHostBridge::new(state, tx, timer);
        stage(&mut bridge, "src").expect("stage");
        let unknown = verdict(&mut bridge, 600).expect("verdict");
        assert_eq!(unknown.status, EngineStatusKind::Unknown);
        // 600 ms budget in 250 ms steps: 250, 250, then the 100 remainder.
        assert_eq!(
            delays.borrow().as_slice(),
            [
                Duration::from_millis(250),
                Duration::from_millis(250),
                Duration::from_millis(100),
            ]
        );
    }
}
