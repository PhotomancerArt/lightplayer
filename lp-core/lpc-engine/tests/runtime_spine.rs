//! Integration-style tests for the demand-driven runtime spine.

extern crate alloc;

use alloc::string::String;

use lpc_engine::node::NodeError;
use lpc_engine::{
    ArtifactLocation, ArtifactState, ArtifactStore, BindingDraft, BindingPriority, BindingRegistry,
    BindingSource, BindingTarget, NodeRuntime, Production, QueryKey, ResolveHost, ResolveLogLevel,
    ResolveSession, ResolveTrace, Resolver, SessionHostResolver, SessionResolveError, TickContext,
    TickResolver,
};
use lpc_model::node::node_invocation::NodeInvocation;
use lpc_model::{Kind, LpValue, NodeDef, NodeId, Revision, TextureDef, bus::ChannelName};
use lpc_source::ArtifactLocator;
use lps_shared::LpsValueF32;

// --- Tests (concise scenarios; helpers below) ---

#[test]
fn runtime_spine_artifact_acquire_load_release_idle_content_frame_and_refcount() {
    let mut mgr = ArtifactStore::new();
    let location = ArtifactLocation::file("dummy/test.lp");
    let r = mgr.acquire_location(location, Revision::new(1));

    assert_eq!(mgr.refcount(&r), Some(1));
    assert_eq!(mgr.content_frame(&r), Some(Revision::new(1)));

    mgr.load_with(&r, Revision::new(20), |location| {
        let ArtifactLocation::File(path) = location;
        assert_eq!(path.as_str(), "dummy/test.lp");
        Ok(texture_def(12, 8))
    })
    .unwrap();

    assert_eq!(mgr.content_frame(&r), Some(Revision::new(20)));
    let ent = mgr.entry(&r).expect("entry");
    assert!(
        matches!(&ent.state, ArtifactState::Loaded(NodeDef::Texture(payload)) if payload.width() == 12)
    );

    mgr.release(&r, Revision::new(2)).unwrap();
    let ent = mgr.entry(&r).expect("idle entry kept");
    assert_eq!(ent.refcount, 0);
    assert!(
        matches!(&ent.state, ArtifactState::Idle(NodeDef::Texture(payload)) if payload.height() == 8)
    );
}

#[test]
fn runtime_spine_tick_context_resolve_bus_query_and_artifact_frames() {
    let channel = ChannelName(String::from("live"));
    let mut registry = BindingRegistry::new();
    let frame = Revision::new(99);
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

    let mut mgr = ArtifactStore::new();
    let ar = mgr.acquire_location(
        ArtifactLocation::try_from_src_spec(&config.artifact_locator().unwrap()).unwrap(),
        Revision::new(0),
    );
    mgr.load_with(&ar, Revision::new(40), |_location| Ok(texture_def(7, 7)))
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
    let slot_shapes = lpc_model::SlotShapeRegistry::default();
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
        &slot_shapes,
    );

    node.tick(&mut ctx).unwrap();
    assert_eq!(node.last, Some(2.0));

    assert!(ctx.artifact_changed_since(Revision::new(39)));
    assert!(!ctx.artifact_changed_since(Revision::new(40)));
}

fn texture_def(width: u32, height: u32) -> NodeDef {
    NodeDef::Texture(TextureDef::new(width, height))
}

#[test]
fn runtime_spine_node_export_is_reachable() {
    fn assert_spine_ptr(_: Option<&dyn NodeRuntime>) {}

    assert_spine_ptr(None);

    let _: Option<fn(&dyn lpc_engine::node::NodeRuntime)> = None;
}

// --- Helpers ---

struct TickProbeNode {
    query: QueryKey,
    last: Option<f32>,
}

impl NodeRuntime for TickProbeNode {
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
}
