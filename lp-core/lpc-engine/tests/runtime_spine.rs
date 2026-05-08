//! Integration-style tests for the demand-driven runtime spine.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;

use lpc_engine::node::NodeError;
use lpc_engine::{
    ArtifactLocation, ArtifactManager, ArtifactState, BindingDraft, BindingPriority,
    BindingRegistry, BindingSource, BindingTarget, Node, Production, QueryKey, ResolveHost,
    ResolveLogLevel, ResolveSession, ResolveTrace, Resolver, RuntimeProduct, SessionHostResolver,
    SessionResolveError, TickContext, TickResolver,
};
use lpc_model::{FrameId, Kind, LpValue, NodeId, SlotPath, bus::ChannelName};
use lpc_source::ArtifactLocator;
use lpc_model::node::node_invocation::NodeInvocation;
use lps_shared::LpsValueF32;

// --- Tests (concise scenarios; helpers below) ---

#[test]
fn runtime_spine_artifact_acquire_load_release_idle_content_frame_and_refcount() {
    let mut mgr: ArtifactManager<String> = ArtifactManager::new();
    let location = ArtifactLocation::file("dummy/test.lp");
    let r = mgr.acquire_location(location, FrameId::new(1));

    assert_eq!(mgr.refcount(&r), Some(1));
    assert_eq!(mgr.content_frame(&r), Some(FrameId::new(1)));

    mgr.load_with(&r, FrameId::new(20), |location| {
        let ArtifactLocation::File(path) = location;
        Ok(format!("loaded:{}", path.as_str()))
    })
    .unwrap();

    assert_eq!(mgr.content_frame(&r), Some(FrameId::new(20)));
    let ent = mgr.entry(&r).expect("entry");
    assert!(
        matches!(&ent.state, ArtifactState::Loaded(payload) if payload == "loaded:dummy/test.lp")
    );

    mgr.release(&r, FrameId::new(2)).unwrap();
    let ent = mgr.entry(&r).expect("idle entry kept");
    assert_eq!(ent.refcount, 0);
    assert!(matches!(&ent.state, ArtifactState::Idle(s) if s == "loaded:dummy/test.lp"));
}

#[test]
fn runtime_spine_tick_context_resolve_bus_query_and_artifact_frames() {
    let channel = ChannelName(String::from("live"));
    let mut registry = BindingRegistry::new();
    let frame = FrameId::new(99);
    registry
        .register(
            BindingDraft {
                source: BindingSource::Literal(LpValue::F32(2.0)),
                target: BindingTarget::BusChannel(channel.clone()),
                priority: BindingPriority::new(0),
                kind: Kind::Ratio,
                owner: NodeId::new(1),
            },
            frame,
        )
        .unwrap();

    let config = NodeInvocation::new(ArtifactLocator::path("e.lp"));

    let mut mgr: ArtifactManager<u8> = ArtifactManager::new();
    let ar = mgr.acquire_location(
        ArtifactLocation::try_from_src_spec(&config.artifact_locator().unwrap()).unwrap(),
        FrameId::new(0),
    );
    mgr.load_with(&ar, FrameId::new(40), |_location| Ok(7u8))
        .unwrap();
    let content_frame = mgr.content_frame(&ar).expect("content_frame");

    let mut resolver = Resolver::new();
    let mut session = ResolveSession::new(
        frame,
        &mut resolver,
        &registry,
        ResolveTrace::new(ResolveLogLevel::Off),
    );

    struct NoProduceHost;

    impl ResolveHost for NoProduceHost {
        fn produce(
            &mut self,
            _query: &QueryKey,
            _session: &mut ResolveSession<'_>,
        ) -> Result<Production, SessionResolveError> {
            Err(SessionResolveError::other("unexpected produce"))
        }
    }

    let mut host = NoProduceHost;
    let mut node = TickProbeNode {
        query: QueryKey::Bus(channel),
        last: None,
    };

    let mut bridge = SessionHostResolver {
        session: &mut session,
        host: &mut host,
    };
    let mut ctx = TickContext::new(
        NodeId::new(5),
        frame,
        ar,
        content_frame,
        &mut bridge as &mut dyn TickResolver,
    );

    node.tick(&mut ctx).unwrap();
    assert_eq!(node.last, Some(2.0));

    assert!(ctx.artifact_changed_since(FrameId::new(39)));
    assert!(!ctx.artifact_changed_since(FrameId::new(40)));
}

#[test]
fn runtime_spine_node_export_is_reachable() {
    fn assert_spine_ptr(_: Option<&dyn Node>) {}

    assert_spine_ptr(None);

    let _: Option<fn(&dyn lpc_engine::node::Node)> = None;
}

// --- Helpers ---

struct TickProbeNode {
    query: QueryKey,
    last: Option<f32>,
}

impl Node for TickProbeNode {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let pv = ctx
            .resolve(self.query.clone())
            .map_err(|e| NodeError::msg(format!("resolve: {}", e.message)))?;
        if let LpsValueF32::F32(v) = *pv.as_value().expect("value") {
            self.last = Some(v);
        }
        Ok(())
    }

    fn destroy(&mut self, _ctx: &mut lpc_engine::DestroyCtx<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: lpc_engine::PressureLevel,
        _ctx: &mut lpc_engine::MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        Ok(())
    }

    fn produced(&self) -> &dyn lpc_engine::ProducedSlotAccess {
        &EMPTY_PROPS
    }
}

struct EmptyProps;

impl lpc_engine::ProducedSlotAccess for EmptyProps {
    fn get(&self, _path: &SlotPath) -> Option<(RuntimeProduct, FrameId)> {
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        _since: FrameId,
    ) -> Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, FrameId)> + 'a> {
        Box::new(alloc::vec::Vec::new().into_iter())
    }

    fn snapshot<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, FrameId)> + 'a> {
        Box::new(alloc::vec::Vec::new().into_iter())
    }
}

static EMPTY_PROPS: EmptyProps = EmptyProps;
