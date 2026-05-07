//! Integration-style tests for the demand-driven runtime spine.

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_engine::node::NodeError;
use lpc_engine::resolver::{ResolverContext, resolve_slot};
use lpc_engine::{
    ArtifactLocation, ArtifactManager, ArtifactState, BindingDraft, BindingKind, BindingPriority,
    BindingRegistry, BindingSource, BindingTarget, Bus, Node, Production, QueryKey, ResolveHost,
    ResolveLogLevel, ResolveSession, ResolveSource, ResolveTrace, Resolver, RuntimeProduct,
    SessionHostResolver, SessionResolveError, SlotResolverCache, TickContext, TickResolver,
};
use lpc_model::{
    FrameId, Kind, ModelValue, NodeId, NodePropSpec, SlotPath, ValuePath, bus::ChannelName,
    prop::value_path::parse_path, tree::tree_path::TreePath,
};
use lpc_source::node::node_invocation::NodeInvocation;
use lpc_source::{
    ArtifactLocator, prop::src_binding::SrcBinding, prop::src_value_spec::SrcValueSpec,
};
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
fn runtime_spine_literal_override_and_artifact_default_resolution() {
    let mut cache = SlotResolverCache::new();
    let mut config = NodeInvocation::new(ArtifactLocator::path("a.lp"));
    let prop_lit = parse_path("params.gain").unwrap();
    config.overrides.push((
        prop_lit.clone(),
        SrcBinding::Literal(SrcValueSpec::Literal(ModelValue::F32(6.25))),
    ));

    let ctx = SyntheticResolverContext::new(FrameId::new(7))
        .with_default("params.bias", LpsValueF32::F32(1.5));

    let slot_lit = resolve_slot(&mut cache, &config, &prop_lit, &ctx).unwrap();
    assert!(matches!(slot_lit.value, LpsValueF32::F32(6.25)));
    assert!(matches!(
        slot_lit.source,
        ResolveSource::Override(BindingKind::Literal)
    ));

    let prop_def = parse_path("params.bias").unwrap();
    let slot_def = resolve_slot(&mut cache, &config, &prop_def, &ctx).unwrap();
    assert!(matches!(slot_def.value, LpsValueF32::F32(1.5)));
    assert!(matches!(slot_def.source, ResolveSource::Default));
}

#[test]
fn runtime_spine_bus_claim_publish_resolver_sees_value_in_resolved_slot() {
    let mut bus = Bus::new();
    let channel = ChannelName(String::from("ctrl/in/0"));
    let out_path = slot_path("output");
    bus.claim_writer(&channel, NodeId::new(42), out_path, Kind::Amplitude)
        .unwrap();
    bus.publish(&channel, LpsValueF32::F32(9.0), FrameId::new(11));

    let mut cache = SlotResolverCache::new();
    let config = NodeInvocation::new(ArtifactLocator::path("b.lp"));

    let ctx = SyntheticResolverContext::new(FrameId::new(100))
        .with_bus(&bus)
        .with_binding("inputs.level", SrcBinding::Bus(channel.clone()));

    let prop = parse_path("inputs.level").unwrap();
    let slot = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

    assert!(matches!(slot.value, LpsValueF32::F32(9.0)));
    assert_eq!(slot.changed_frame.as_i64(), 11);
    assert!(matches!(
        slot.source,
        ResolveSource::ArtifactBind(BindingKind::Bus)
    ));
}

#[test]
fn runtime_spine_node_prop_reads_legacy_value_path_target() {
    let target_path = TreePath::parse("/show.demo/node_a.demo").unwrap();
    let output = parse_path("output").unwrap();

    let mut target_props = MapRuntimeProps::default();
    target_props.insert(output.clone(), LpsValueF32::F32(3.3), FrameId::new(4));

    let mut targets: BTreeMap<TreePath, MapRuntimeProps> = BTreeMap::new();
    targets.insert(target_path, target_props);

    let mut cache = SlotResolverCache::new();
    let config = NodeInvocation::new(ArtifactLocator::path("c.lp"));

    let spec = NodePropSpec::parse("/show.demo/node_a.demo#output").expect("output NodePropSpec");
    let ctx = SyntheticResolverContext::new(FrameId::new(8))
        .with_targets_map(targets)
        .with_binding("params.drive", SrcBinding::NodeProp(spec));

    let prop = parse_path("params.drive").unwrap();
    let slot = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();
    assert!(matches!(slot.value, LpsValueF32::F32(3.3)));
    assert_eq!(slot.changed_frame.as_i64(), 4);
    assert!(matches!(
        slot.source,
        ResolveSource::ArtifactBind(BindingKind::NodeProp)
    ));
}

#[test]
fn runtime_spine_node_prop_without_target_falls_back_to_default() {
    let mut cache = SlotResolverCache::new();
    let config = NodeInvocation::new(ArtifactLocator::path("d.lp"));

    let spec = NodePropSpec::parse("/show.demo/node_a.demo#params.k").expect("params spec");
    let ctx = SyntheticResolverContext::new(FrameId::new(1))
        .with_binding("params.x", SrcBinding::NodeProp(spec));

    let prop = parse_path("params.x").unwrap();
    let slot = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();
    assert!(matches!(slot.value, LpsValueF32::F32(0.0)));
    assert_eq!(slot.source, ResolveSource::Failed);
}

#[test]
fn runtime_spine_tick_context_resolve_bus_query_and_artifact_frames() {
    let channel = ChannelName(String::from("live"));
    let mut registry = BindingRegistry::new();
    let frame = FrameId::new(99);
    registry
        .register(
            BindingDraft {
                source: BindingSource::Literal(SrcValueSpec::Literal(ModelValue::F32(2.0))),
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

fn slot_path(path: &str) -> SlotPath {
    SlotPath::parse(path).expect("slot path")
}

/// Maps node path → prop values; [`ResolverContext::target_prop`] reads like engine-side dereference.
#[derive(Default, Clone)]
struct MapRuntimeProps {
    values: Vec<(ValuePath, LpsValueF32, FrameId)>,
}

impl MapRuntimeProps {
    fn insert(&mut self, path: ValuePath, value: LpsValueF32, frame: FrameId) {
        self.values.push((path, value, frame));
    }
}

struct SyntheticResolverContext<'a> {
    frame: FrameId,
    bus: Option<&'a Bus>,
    bindings: BTreeMap<ValuePath, SrcBinding>,
    defaults: BTreeMap<ValuePath, LpsValueF32>,
    targets: BTreeMap<TreePath, MapRuntimeProps>,
}

impl<'a> SyntheticResolverContext<'a> {
    fn new(frame: FrameId) -> Self {
        Self {
            frame,
            bus: None,
            bindings: BTreeMap::new(),
            defaults: BTreeMap::new(),
            targets: BTreeMap::new(),
        }
    }

    fn with_bus(mut self, bus: &'a Bus) -> Self {
        self.bus = Some(bus);
        self
    }

    fn with_binding(mut self, prop: &str, binding: SrcBinding) -> Self {
        self.bindings.insert(parse_path(prop).unwrap(), binding);
        self
    }

    fn with_default(mut self, prop: &str, value: LpsValueF32) -> Self {
        self.defaults.insert(parse_path(prop).unwrap(), value);
        self
    }

    fn with_targets_map(mut self, map: BTreeMap<TreePath, MapRuntimeProps>) -> Self {
        self.targets = map;
        self
    }
}

impl ResolverContext for SyntheticResolverContext<'_> {
    fn frame_id(&self) -> FrameId {
        self.frame
    }

    fn bus_value(&self, channel: &ChannelName) -> Option<(&LpsValueF32, FrameId)> {
        self.bus
            .and_then(|b| b.read(channel).map(|v| (v, b.last_writer_frame(channel))))
    }

    fn target_prop(&self, node: &TreePath, prop: &ValuePath) -> Option<(LpsValueF32, FrameId)> {
        self.targets.get(node).and_then(|target| {
            target
                .values
                .iter()
                .find(|(path, _, _)| path == prop)
                .map(|(_, value, frame)| (value.clone(), *frame))
        })
    }

    fn artifact_binding(&self, prop: &ValuePath) -> Option<SrcBinding> {
        self.bindings.get(prop).cloned()
    }

    fn artifact_default(&self, prop: &ValuePath) -> Option<LpsValueF32> {
        self.defaults.get(prop).cloned()
    }
}

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
