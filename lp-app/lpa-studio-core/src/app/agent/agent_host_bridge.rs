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

use std::cell::RefCell;
use std::rc::Rc;

use lpa_agent::{AgentHost, HostError, ShaderContext};
use lpc_model::ArtifactLocation;
use lps_probe::LedPoint;

use crate::app::studio::studio_view_channel::CommandSender;
use crate::{
    AssetEditOp, ControllerId, MAX_ASSET_BODY_BYTES, ProjectController, StudioCommand, UiAction,
};

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
}

/// The host handed to `lpa_agent::AgentSession` for one shader node.
pub struct AgentHostBridge {
    state: Rc<RefCell<AgentBridgeState>>,
    tx: CommandSender,
}

impl AgentHostBridge {
    pub fn new(state: Rc<RefCell<AgentBridgeState>>, tx: CommandSender) -> Self {
        Self { state, tx }
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

    fn stage_source(&mut self, source: &str) -> Result<(), HostError> {
        let artifact = self
            .state
            .borrow()
            .artifact
            .clone()
            .ok_or_else(|| HostError::new("no shader source artifact resolved"))?;
        // The tool layer pre-checks its own mirror of this cap; this guard
        // keeps the bridge honest if the two ever drift.
        if source.len() > MAX_ASSET_BODY_BYTES {
            return Err(HostError::new(format!(
                "source is {} bytes; the asset limit is {MAX_ASSET_BODY_BYTES}",
                source.len()
            )));
        }
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
    use super::*;
    use crate::app::studio::studio_view_channel::command_channel;

    fn bridge_with_artifact() -> (
        AgentHostBridge,
        crate::app::studio::studio_view_channel::CommandReceiver,
    ) {
        let (tx, rx) = command_channel();
        let state = Rc::new(RefCell::new(AgentBridgeState {
            artifact: Some(ArtifactLocation::file("/shader.glsl")),
            source: "old".to_string(),
            ..AgentBridgeState::default()
        }));
        (AgentHostBridge::new(state, tx), rx)
    }

    #[test]
    fn stage_source_enqueues_the_editor_apply_action_and_mirrors_locally() {
        let (mut bridge, rx) = bridge_with_artifact();
        bridge.stage_source("new source").expect("stage");

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
        let mut bridge = AgentHostBridge::new(state, tx);

        assert!(bridge.current_source().is_err());
        assert!(bridge.stage_source("x").is_err());
        assert!(rx.try_recv_all_for_test().is_empty());
    }

    #[test]
    fn oversized_source_is_refused_without_enqueueing() {
        let (mut bridge, rx) = bridge_with_artifact();
        let big = "x".repeat(MAX_ASSET_BODY_BYTES + 1);
        let error = bridge.stage_source(&big).expect_err("too big");
        assert!(error.message.contains("asset limit"));
        assert!(rx.try_recv_all_for_test().is_empty());
        assert_eq!(bridge.current_source().expect("source"), "old");
    }
}
