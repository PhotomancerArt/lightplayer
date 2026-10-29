//! A fixture placed whole on a named output, for scenes wired by hand.
//!
//! A fixture's lamps land only where its patching puts them, and its
//! patching offers only outputs it reaches through a bus channel with an
//! authored def (their ports). A scene built node by node, with a fixture
//! bound straight to an output and no defs behind either, has none of
//! that. These scenes test the output's frame read and its flush, not the
//! patcher: [`PlacedFixture`] stands in for the patching with one run of
//! every lamp from the output's first lamp, and [`bind_output_name`] gives
//! the output the name that run names.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::{
    Kind, LpValue, NodeId, NodeRuntimeStatus, Revision, SlotAccess, SlotPath, SlotShapeRegistry,
    SlotShapeRegistryError,
};

use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::engine::Engine;
use crate::node::{
    AssetRefreshContext, AssetRefreshResult, ControlNode, DestroyCtx, MemPressureCtx, NodeError,
    NodeRuntime, PatchedRun, PressureLevel, ProduceResult, TickContext,
};
use crate::nodes::FixtureNode;

/// A [`FixtureNode`] whose `lamps` lamps land, in order, from the first
/// lamp of the output named `output`.
pub(crate) struct PlacedFixture {
    fixture: FixtureNode,
    runs: Vec<PatchedRun>,
}

impl PlacedFixture {
    pub(crate) fn new(fixture: FixtureNode, lamps: u32, output: &str) -> Self {
        Self {
            fixture,
            runs: vec![PatchedRun {
                start: 0,
                count: lamps,
                lamp: 0,
                reversed: false,
                offset: 0,
                output: String::from(output),
            }],
        }
    }
}

impl NodeRuntime for PlacedFixture {
    fn produce(
        &mut self,
        slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        self.fixture.produce(slot, ctx)
    }

    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        self.fixture.consume(ctx)
    }

    fn refresh_asset(
        &mut self,
        location: &lpc_model::AssetLocation,
        ctx: &mut AssetRefreshContext<'_>,
    ) -> Result<AssetRefreshResult, NodeError> {
        self.fixture.refresh_asset(location, ctx)
    }

    fn destroy(&mut self, ctx: &mut DestroyCtx) -> Result<(), NodeError> {
        self.fixture.destroy(ctx)
    }

    fn handle_memory_pressure(
        &mut self,
        level: PressureLevel,
        ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError> {
        self.fixture.handle_memory_pressure(level, ctx)
    }

    fn runtime_status(&self) -> Option<NodeRuntimeStatus> {
        self.fixture.runtime_status()
    }

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        self.fixture.runtime_state_slots()
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        self.fixture.register_runtime_state_shapes(registry)
    }

    fn control_patch_placement(&self) -> &[PatchedRun] {
        &self.runs
    }

    fn control_node(&mut self) -> Option<&mut dyn ControlNode> {
        self.fixture.control_node()
    }
}

/// Name the output `out_id` `name`, as its def's `name` would.
pub(crate) fn bind_output_name(engine: &mut Engine, out_id: NodeId, name: &str, frame: Revision) {
    engine
        .add_binding(
            BindingDraft {
                source: BindingSource::Literal(LpValue::String(String::from(name))),
                target: BindingTarget::ConsumedSlot {
                    node: out_id,
                    slot: SlotPath::parse("name.some").expect("the output's name slot"),
                },
                priority: BindingPriority::new(0),
                kind: Kind::Choice,
                owner: out_id,
            },
            frame,
        )
        .expect("bind the output's name");
}
