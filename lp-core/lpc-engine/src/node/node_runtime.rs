//! Engine spine [`NodeRuntime`] trait: produce, consume, destroy, memory pressure, and runtime state.

use crate::nodes::OutputFragment;
use crate::products::control::ControlLayout;
use crate::resource::RuntimeBufferId;
use lpc_model::{
    AssetLocation, NodeId, NodeRuntimeStatus, Revision, SlotAccess, SlotPath, SlotShapeRegistry,
    SlotShapeRegistryError,
};

use super::contexts::{
    AssetRefreshContext, DestroyCtx, MemPressureCtx, NodeResourceInitContext, TickContext,
};
use super::node_error::NodeError;
use super::residency_request::ResidencyRequest;
use super::{ControlNode, RenderNode};
use crate::engine::memory_pressure::PressureLevel;

/// Result of a produced-slot request against a runtime node.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ProduceResult {
    Produced,
    Unsupported,
}

/// Result of asking a runtime node to refresh an asset it may consume.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AssetRefreshResult {
    /// The node does not consume this asset.
    Unused,
    /// The node consumes the asset, but the effective asset body did not change.
    Unchanged,
    /// The node refreshed internal state from the new effective asset body.
    Refreshed,
}

/// One run of a producer's lamps, placed on an output's wire.
///
/// What a fixture's patching gave a run of its lamps (`patching_runs.rs`),
/// restated here, in LAMPS, because the output node is never gated out while
/// the mapping crate can be: placement is a producer-declared property, and
/// what a producer patched it FROM is that producer's business.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchedRun {
    /// First lamp of the run, in the producer's own order.
    pub start: u32,
    /// Lamps in the run.
    pub count: u32,
    /// First wire lamp of the run's window.
    pub lamp: u32,
    /// Lay the run down end-first (applied before rotation: source lamp `j`
    /// lands at window slot `(j' + offset) mod count`, `j'` being `j` laid
    /// end-first when reversed).
    pub reversed: bool,
    /// Rotation in lamps within the run's window (0 = none).
    pub offset: u32,
    /// The addressed output's NAME: every run names the output it lands on.
    pub output: alloc::string::String,
}

impl PatchedRun {
    /// One past the last wire lamp this run occupies. Rotation permutes
    /// within the window, so the window is the whole story.
    #[must_use]
    pub const fn lamp_end(&self) -> u32 {
        self.lamp.saturating_add(self.count)
    }

    /// The wire lamp the run's lamp `j` (counted from its `start`) lands on:
    /// laid end-first when reversed, then rotated by `offset` within the
    /// window. The tests' oracle for what the output's render does.
    #[cfg(all(test, feature = "node-fixture"))]
    pub(crate) fn wire_lamp(&self, j: u32) -> u32 {
        debug_assert!(j < self.count);
        let oriented = if self.reversed { self.count - 1 - j } else { j };
        self.lamp + ((oriented + self.offset) % self.count.max(1))
    }
}

/// Runtime node instance for the demand-driven engine spine.
pub trait NodeRuntime {
    /// Allocate [`RuntimeBufferId`] slots owned by this node before first use.
    ///
    /// Default: no-op. [`crate::engine::Engine::attach_runtime_node`] invokes this immediately
    /// before storing the alive node.
    fn init_resources(&mut self, _ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    /// Materialize a produced slot.
    ///
    /// Value-producing nodes should update the runtime state backing `slot`.
    /// Nodes with no produced values may keep the default unsupported result.
    fn produce(
        &mut self,
        _slot: &SlotPath,
        _ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        Ok(ProduceResult::Unsupported)
    }

    /// Consume graph inputs as an every-frame demand root.
    ///
    /// Output-like boundary nodes use this for side effects. Nodes that only
    /// produce values can keep the no-op default.
    fn consume(&mut self, _ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    /// The consumed slots whose value decides what this node loads before
    /// it ever ticks — a playlist's pick decides its resident entry. Before
    /// the first tick after a load (and after a structural reattach), the
    /// engine hands each one a panel writer that supplies it
    /// ([`Self::settle_consumed_panel_value`]). Default: none.
    fn load_deciding_slots(&self) -> &'static [&'static str] {
        &[]
    }

    /// A panel writer supplies `slot` (one of [`Self::load_deciding_slots`])
    /// before the first tick: adopt it now, so the first tick's residency
    /// step loads what the node will show instead of what it was authored
    /// with (playlists-on-the-bus Q3). `write` is the writer's
    /// [`crate::dataflow::panel_writers::PanelWriter::write`]. Default:
    /// nothing.
    fn settle_consumed_panel_value(&mut self, slot: &str, value: &lpc_model::LpValue, write: u64) {
        let _ = (slot, value, write);
    }

    /// Refresh a referenced asset after the project registry reports an effective asset change.
    ///
    /// Nodes that compile or cache asset bodies should compare the incoming asset's revision to
    /// the revision they last consumed and invalidate only their own cached runtime state.
    fn refresh_asset(
        &mut self,
        _location: &AssetLocation,
        _ctx: &mut AssetRefreshContext<'_>,
    ) -> Result<AssetRefreshResult, NodeError> {
        Ok(AssetRefreshResult::Unused)
    }

    fn destroy(&mut self, ctx: &mut DestroyCtx) -> Result<(), NodeError>;

    fn handle_memory_pressure(
        &mut self,
        level: PressureLevel,
        ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError>;

    /// Whether this node was denied a heavy allocation transient (a shader
    /// compile) and is waiting for a compile window.
    ///
    /// When any alive node reports true, the engine broadcasts
    /// [`Self::handle_memory_pressure`] to every node at the top of the next
    /// tick — a safe point where no render borrow is live — and then calls
    /// [`Self::open_compile_window`], so the deferred transient runs against
    /// a heap where rebuildable state has been dropped. See the
    /// memory-pressure contract in
    /// `docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md`.
    fn wants_compile_window(&self) -> bool {
        false
    }

    /// The engine broadcast memory pressure and is opening a compile window
    /// for the tick at `revision`. A node that deferred heavy work may run
    /// it during this frame's render; the window expires with the frame.
    fn open_compile_window(&mut self, _revision: Revision) {}

    /// Load/unload of this node's own children, applied by the engine at the
    /// pre-tick step ([`crate::Engine::apply_residency`]). Only the playlist
    /// implements it (multi-pattern vision D3).
    ///
    /// Polled once per tick, before the tick. Taking the request clears it:
    /// the engine answers every request it takes, through
    /// [`Self::entry_unloaded`] / [`Self::entry_loaded`] /
    /// [`Self::entry_load_failed`], or [`Self::residency_refused`] when it
    /// changed nothing.
    fn residency_request(&mut self) -> Option<ResidencyRequest> {
        None
    }

    /// The engine loaded entry `entry`: its child is `child`, alive and
    /// bound, and its visual output is `output_slot` on that child. Called at
    /// the pre-tick step, so the child can be demanded in the tick that
    /// follows.
    fn entry_loaded(&mut self, _entry: u32, _child: NodeId, _output_slot: &SlotPath) {}

    /// The engine unloaded entry `entry`: its child subtree is gone from the
    /// tree, and its ids are never reused.
    fn entry_unloaded(&mut self, _entry: u32) {}

    /// Loading entry `entry` failed (parse, derive or attach). Nothing of it
    /// is attached; `reason` says why. The tick goes on.
    fn entry_load_failed(&mut self, _entry: u32, _reason: &str) {}

    /// The engine refused the whole `request` and changed nothing: the
    /// entry to unload carries edits a commit would write, so unloading it
    /// would strand them (and loading the other without unloading would hold
    /// two entries at once). The owner keeps playing what it has.
    fn residency_refused(&mut self, _request: ResidencyRequest, _reason: &str) {}

    /// Current runtime health, when the node has a more specific status than "ok".
    ///
    /// Returning `None` lets the engine report [`NodeRuntimeStatus::Ok`] after a successful
    /// runtime operation. Nodes with cached/degraded internal state can return an error or
    /// warning while still rendering fallback output or otherwise keeping the runtime alive.
    fn runtime_status(&self) -> Option<NodeRuntimeStatus> {
        None
    }

    /// Re-arm whatever this node latched when the RUNTIME failed it, so the
    /// next tick attempts the work again.
    ///
    /// Called by [`crate::Engine::clear_faults`] on every alive node. A
    /// no-op for nodes that latch nothing: a node whose fault was a failed
    /// tick simply ticks again next frame, and the engine re-derives the
    /// truth either way. The hook exists for the latches that would
    /// otherwise never retry — a shader whose compile the recovery ledger
    /// denied cleared `needs_compile` and would sit dark forever.
    ///
    /// This clears the node's own latch only. Whatever DENIED the work
    /// (the recovery ledger) is cleared by the caller, or the retry is
    /// denied again and the node re-faults honestly.
    fn clear_fault(&mut self) {}

    /// Node-owned runtime state exposed as a slot root.
    ///
    /// Nodes without public runtime state return `None`; they do not publish a
    /// synthetic state root in project-read snapshots.
    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        None
    }

    /// Register any shape roots required by [`Self::runtime_state_slots`].
    fn register_runtime_state_shapes(
        &self,
        _registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        Ok(())
    }

    /// Sink buffer backing an [`crate::nodes::OutputNode`] after [`Self::init_resources`] runs.
    fn runtime_output_sink_buffer_id(&self) -> Option<RuntimeBufferId> {
        None
    }

    /// Sample layout of the frame last published into
    /// [`Self::runtime_output_sink_buffer_id`].
    ///
    /// LATCHED by the tick that rendered the frame, never recomputed on
    /// demand: the published-frame read
    /// ([`lpc_wire::OutputFrameProbeRequest`]) exists precisely so a client
    /// can see the bytes the device already pushed without making it render
    /// again, and the layout is the interpretation half of those bytes.
    /// `None` before the first successful render.
    fn runtime_output_sample_layout(&self) -> Option<&ControlLayout> {
        None
    }

    /// The placement set the last published frame was rendered from: which
    /// producers contributed, and where on the wire each of their runs landed.
    ///
    /// The output node consumes N upstream control products; each producer —
    /// not the output — is what knows its own display geometry, and only the
    /// fragment knows where that geometry ended up on the wire. A reader
    /// therefore asks each producer for its layout and rebases it through the
    /// fragment that placed it
    /// ([`crate::nodes::output::merge_fragment_display_layouts`]). Latched
    /// alongside [`Self::runtime_output_sample_layout`], so no graph re-resolve
    /// is needed. Empty before the first successful render.
    fn runtime_output_fragments(&self) -> &[OutputFragment] {
        &[]
    }

    /// Revision stamped the last time [`Self::runtime_output_fragments`]
    /// changed.
    ///
    /// A producer's display-layout revision tracks its mapping and render
    /// extent; it says nothing about where the output put the result. A client
    /// gating on `IfChanged` needs both, so the published-frame read folds this
    /// into the revision it reports for the merged layout.
    fn runtime_output_placement_revision(&self) -> Revision {
        Revision::default()
    }

    /// Revision stamped the last time
    /// [`Self::runtime_output_sample_layout`] changed.
    ///
    /// Folded into the published-frame read's geometry revision beside the
    /// placement revision: a lamp-count or color-order change regroups the
    /// samples without re-cutting the wire.
    fn runtime_output_sample_layout_revision(&self) -> Revision {
        Revision::default()
    }

    /// Where this node's control product lands on its outputs' wires: the
    /// runs its patching gave its lamps, each naming its output. A run may be
    /// non-contiguous with its neighbours, or reversed, or rotated.
    ///
    /// Empty for every node but a fixture with a mapping: lamps no run
    /// places are on no wire (nothing flows by default).
    ///
    /// Read by the output DURING its own consume, after the resolve that ran
    /// this node's `produce` — so it is this frame's answer, and an edited
    /// mapping reaches the wire on the next tick with no cache to
    /// invalidate.
    fn control_patch_placement(&self) -> &[PatchedRun] {
        &[]
    }

    /// Moves whenever [`Self::control_patch_placement`]'s answer does. An
    /// output's frame plan holds its producers' placements and keys on their
    /// generations: a producer can re-resolve its patch on a frame no
    /// structural key moved (the first frame it sees a sibling output's
    /// name). A producer whose placement never changes leaves it at 0.
    fn control_patch_generation(&self) -> u64 {
        0
    }

    /// Render capability for nodes whose produced slots can materialize visual products.
    fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
        None
    }

    /// Control capability for nodes whose produced slots can render device-control samples.
    fn control_node(&mut self) -> Option<&mut dyn ControlNode> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;

    use crate::dataflow::resolver::{
        ResolveHost, ResolveSession, ResolveTrace, Resolver, SessionHostResolver, TickResolver,
        resolve_trace::ResolveLogLevel,
    };
    use lpc_model::{AssetLocation, NodeId, Revision, SlotShapeRegistry};

    struct EmptyResolveHost;

    impl ResolveHost for EmptyResolveHost {
        fn produce(
            &mut self,
            _query: &crate::dataflow::resolver::QueryKey,
            _session: &mut ResolveSession<'_>,
        ) -> Result<
            crate::dataflow::resolver::Production,
            crate::dataflow::resolver::SessionResolveError,
        > {
            Err(crate::dataflow::resolver::SessionResolveError::other(
                "EmptyResolveHost: unexpected produce",
            ))
        }
    }

    struct DummyNode;

    impl DummyNode {
        fn new() -> Self {
            Self
        }
    }

    impl NodeRuntime for DummyNode {
        fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
            Ok(())
        }

        fn handle_memory_pressure(
            &mut self,
            _level: PressureLevel,
            _ctx: &mut MemPressureCtx,
        ) -> Result<(), NodeError> {
            Ok(())
        }
    }

    #[test]
    fn node_trait_is_object_safe() {
        let node: Box<dyn NodeRuntime> = Box::new(DummyNode::new());
        assert!(core::mem::size_of_val(&node) > 0);
    }

    #[test]
    fn default_runtime_state_is_absent() {
        let node = DummyNode::new();
        assert!(node.runtime_state_slots().is_none());

        let mut res = Resolver::new();
        let frame = Revision::new(0);
        let mut session =
            ResolveSession::new(frame, &mut res, ResolveTrace::new(ResolveLogLevel::Off));
        let mut host = EmptyResolveHost;
        let slot_shapes = SlotShapeRegistry::default();

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut tick = TickContext::new(
            NodeId::new(0),
            frame,
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );
        let mut dyn_node: Box<dyn NodeRuntime> = Box::new(DummyNode::new());
        assert_eq!(
            dyn_node
                .produce(&SlotPath::root(), &mut tick)
                .expect("produce"),
            ProduceResult::Unsupported
        );
    }

    #[test]
    fn default_asset_refresh_is_unused() {
        let mut node = DummyNode::new();
        let fs = lpfs::LpFsMemory::new();
        let mut registry = lpc_registry::ProjectRegistry::new();
        let slot_shapes = SlotShapeRegistry::default();
        let mut ctx = AssetRefreshContext::new(&fs, &mut registry, &slot_shapes, Revision::new(1));

        assert_eq!(
            node.refresh_asset(
                &AssetLocation::artifact(lpc_model::ArtifactLocation::file("/shader.glsl")),
                &mut ctx,
            )
            .expect("refresh"),
            AssetRefreshResult::Unused
        );
    }
}
