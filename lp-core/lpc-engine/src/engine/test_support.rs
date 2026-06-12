use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};

use lpc_model::{
    ChannelName, Kind, MapSlot, NodeId, NodeName, Revision, SlotAccess, SlotMapKey, SlotPath,
    SlotPathSegment, SlotShapeRegistry, SlotShapeRegistryError, Slotted, TreePath, ValueSlot,
};
use lpc_wire::{WireChildKind, WireSlotIndex};
use lps_shared::LpsValueF32;

use crate::dataflow::binding::{
    BindingDraft, BindingError, BindingPriority, BindingSource, BindingTarget,
};
use crate::dataflow::resolver::{
    Production, QueryKey, ResolveLogLevel, ResolveTrace, ResolveTraceEvent, SessionResolveError,
};
use crate::engine::Engine;
use crate::node::test_placeholder_spine;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RuntimeStateShape, TickContext,
};

use super::engine::default_demand_input_path;
use super::resolve_with_engine_host;

pub(crate) struct EngineTestBuilder {
    engine: Engine,
    labels: BTreeMap<String, NodeId>,
    shader_ticks: BTreeMap<String, Arc<AtomicU32>>,
    fixture_records: BTreeMap<String, RecordedValue>,
    output_records: BTreeMap<String, RecordedValue>,
}

pub(crate) struct EngineTestHarness {
    pub(crate) engine: Engine,
    labels: BTreeMap<String, NodeId>,
    shader_ticks: BTreeMap<String, Arc<AtomicU32>>,
    fixture_records: BTreeMap<String, RecordedValue>,
    output_records: BTreeMap<String, RecordedValue>,
}

pub(crate) struct OutputSpec {
    path: SlotPath,
    value: LpsValueF32,
}

pub(crate) enum TestBindingSource {
    Literal(LpsValueF32),
    ProducedSlot { label: String, slot: SlotPath },
    Bus(ChannelName),
}

#[derive(Clone)]
pub(crate) struct RecordedValue {
    count: Arc<AtomicU32>,
    bits: Arc<AtomicU32>,
}

impl EngineTestBuilder {
    pub(crate) fn new() -> Self {
        Self {
            engine: Engine::new(TreePath::parse("/show.test").expect("test root path")),
            labels: BTreeMap::new(),
            shader_ticks: BTreeMap::new(),
            fixture_records: BTreeMap::new(),
            output_records: BTreeMap::new(),
        }
    }

    pub(crate) fn shader(mut self, label: &str, slot: OutputSpec) -> Self {
        let ticks = Arc::new(AtomicU32::new(0));
        let node = DummyShaderNode::new(slot.path, slot.value, Arc::clone(&ticks));
        self.attach_node(label, "shader", Box::new(node));
        self.shader_ticks.insert(String::from(label), ticks);
        self
    }

    pub(crate) fn fixture(mut self, label: &str) -> Self {
        let record = RecordedValue::new();
        let node = DummyFixtureNode::new(default_demand_input_path(), record.clone());
        self.attach_node(label, "fixture", Box::new(node));
        self.fixture_records.insert(String::from(label), record);
        self
    }

    pub(crate) fn output_node(mut self, label: &str) -> Self {
        let record = RecordedValue::new();
        let node = DummyOutputNode::new(default_demand_input_path(), record.clone());
        self.attach_node(label, "output", Box::new(node));
        self.output_records.insert(String::from(label), record);
        self
    }

    pub(crate) fn bind_bus(self, channel: &str, source: TestBindingSource) -> Self {
        self.bind_bus_with_priority(channel, source, 0)
            .expect("bind bus")
    }

    pub(crate) fn bind_bus_with_priority(
        mut self,
        channel: &str,
        source: TestBindingSource,
        priority: i32,
    ) -> Result<Self, BindingError> {
        let owner = source.owner(&self.labels);
        let source = source.into_binding_source(&self.labels);
        self.register_binding(
            source,
            BindingTarget::BusChannel(channel_name(channel)),
            priority,
            owner,
        )?;
        Ok(self)
    }

    pub(crate) fn bind_input(self, label: &str, slot: &str, source: TestBindingSource) -> Self {
        self.bind_input_with_priority(label, slot, source, 0)
            .expect("bind input")
    }

    pub(crate) fn bind_input_with_priority(
        mut self,
        label: &str,
        slot: &str,
        source: TestBindingSource,
        priority: i32,
    ) -> Result<Self, BindingError> {
        let node = self.node_id(label);
        let source = source.into_binding_source(&self.labels);
        self.register_binding(
            source,
            BindingTarget::ConsumedSlot {
                node,
                slot: path(slot),
            },
            priority,
            node,
        )?;
        Ok(self)
    }

    pub(crate) fn bind_demand_input(self, label: &str, source: TestBindingSource) -> Self {
        self.bind_input(label, "in", source)
    }

    pub(crate) fn demand_root(mut self, label: &str) -> Self {
        let node = self.node_id(label);
        self.engine.add_demand_root(node);
        self
    }

    pub(crate) fn build(self) -> EngineTestHarness {
        EngineTestHarness {
            engine: self.engine,
            labels: self.labels,
            shader_ticks: self.shader_ticks,
            fixture_records: self.fixture_records,
            output_records: self.output_records,
        }
    }

    fn attach_node(&mut self, label: &str, ty: &str, node: Box<dyn NodeRuntime>) -> NodeId {
        let root = self.engine.tree().root();
        let cfg = test_placeholder_spine();
        let node_id = self
            .engine
            .tree_mut()
            .add_child(
                root,
                NodeName::parse(label).expect("node label"),
                NodeName::parse(ty).expect("node type"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg,
                Revision::new(1),
            )
            .expect("add test node");
        self.engine
            .attach_runtime_node(node_id, node, Revision::new(1))
            .expect("attach test node");
        self.labels.insert(String::from(label), node_id);
        node_id
    }

    fn register_binding(
        &mut self,
        source: BindingSource,
        target: BindingTarget,
        priority: i32,
        owner: NodeId,
    ) -> Result<(), BindingError> {
        self.engine.add_binding(
            BindingDraft {
                source,
                target,
                priority: BindingPriority::new(priority),
                kind: Kind::Color,
                owner,
            },
            Revision::new(1),
        )?;
        Ok(())
    }

    fn node_id(&self, label: &str) -> NodeId {
        *self.labels.get(label).expect("test node label")
    }
}

impl EngineTestHarness {
    pub(crate) fn node(&self, label: &str) -> NodeId {
        *self.labels.get(label).expect("test node label")
    }

    pub(crate) fn shader_ticks(&self, label: &str) -> u32 {
        self.shader_ticks
            .get(label)
            .expect("shader tick label")
            .load(Ordering::Relaxed)
    }

    pub(crate) fn reset_shader_ticks(&self, label: &str) {
        self.shader_ticks
            .get(label)
            .expect("shader tick label")
            .store(0, Ordering::Relaxed);
    }

    pub(crate) fn fixture_f32(&self, label: &str) -> Option<f32> {
        self.fixture_records
            .get(label)
            .expect("fixture label")
            .last_f32()
    }

    pub(crate) fn output_f32(&self, label: &str) -> Option<f32> {
        self.output_records
            .get(label)
            .expect("output label")
            .last_f32()
    }

    pub(crate) fn resolve_bus(&mut self, channel: &str) -> Result<Production, SessionResolveError> {
        self.resolve(QueryKey::Bus(channel_name(channel)))
    }

    pub(crate) fn resolve(&mut self, query: QueryKey) -> Result<Production, SessionResolveError> {
        resolve_with_engine_host(&mut self.engine, query, ResolveLogLevel::Off).map(|(pv, _)| pv)
    }

    pub(crate) fn resolve_with_trace(
        &mut self,
        query: QueryKey,
    ) -> Result<(Production, ResolveTrace), SessionResolveError> {
        resolve_with_engine_host(&mut self.engine, query, ResolveLogLevel::Basic)
    }
}

impl OutputSpec {
    fn new(path: &str, value: f32) -> Self {
        Self {
            path: self::path(path),
            value: LpsValueF32::F32(value),
        }
    }
}

impl TestBindingSource {
    fn into_binding_source(self, labels: &BTreeMap<String, NodeId>) -> BindingSource {
        match self {
            Self::Literal(value) => {
                BindingSource::Literal(lpc_model::LpValue::F32(f32_value(value)))
            }
            Self::ProducedSlot { label, slot } => BindingSource::ProducedSlot {
                node: *labels.get(&label).expect("produced slot label"),
                slot,
            },
            Self::Bus(channel) => BindingSource::BusChannel(channel),
        }
    }

    fn owner(&self, labels: &BTreeMap<String, NodeId>) -> NodeId {
        match self {
            Self::ProducedSlot { label, .. } => *labels.get(label).expect("produced slot label"),
            Self::Literal(_) | Self::Bus(_) => NodeId::new(0),
        }
    }
}

impl RecordedValue {
    fn new() -> Self {
        Self {
            count: Arc::new(AtomicU32::new(0)),
            bits: Arc::new(AtomicU32::new(0)),
        }
    }

    fn record(&self, value: &LpsValueF32) {
        if let LpsValueF32::F32(v) = value {
            self.bits.store(v.to_bits(), Ordering::Relaxed);
            self.count.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn last_f32(&self) -> Option<f32> {
        (self.count.load(Ordering::Relaxed) > 0)
            .then(|| f32::from_bits(self.bits.load(Ordering::Relaxed)))
    }
}

pub(crate) fn output(path: &str, value: f32) -> OutputSpec {
    OutputSpec::new(path, value)
}

pub(crate) fn literal(value: f32) -> TestBindingSource {
    TestBindingSource::Literal(LpsValueF32::F32(value))
}

pub(crate) fn produced_slot(label: &str, slot: &str) -> TestBindingSource {
    TestBindingSource::ProducedSlot {
        label: String::from(label),
        slot: path(slot),
    }
}

pub(crate) fn bus(channel: &str) -> TestBindingSource {
    TestBindingSource::Bus(channel_name(channel))
}

pub(crate) fn path(path: &str) -> SlotPath {
    SlotPath::parse(path).expect("test slot path")
}

pub(crate) fn trace_has_value_origin_path(
    trace: &ResolveTrace,
    bus_name: &str,
    shader: NodeId,
    output_path: &SlotPath,
) -> bool {
    let bus_query = QueryKey::Bus(channel_name(bus_name));
    let output_query = QueryKey::ProducedSlot {
        node: shader,
        slot: output_path.clone(),
    };
    trace.events().iter().any(|e| {
        matches!(
            e,
            ResolveTraceEvent::BeginQuery(q) if q == &bus_query
        )
    }) && trace.events().iter().any(|e| {
        matches!(
            e,
            ResolveTraceEvent::ProduceStart(q) if q == &output_query
        )
    })
}

pub(crate) struct DummyShaderNode {
    state: DummyShaderState,
    tick_count: Arc<AtomicU32>,
}

#[derive(Default, Slotted)]
pub(crate) struct DummyShaderState {
    pub outputs: MapSlot<u32, ValueSlot<f32>>,
}

impl DummyShaderNode {
    fn new(slot: SlotPath, value: LpsValueF32, tick_count: Arc<AtomicU32>) -> Self {
        let mut outputs = BTreeMap::new();
        outputs.insert(output_key(&slot), ValueSlot::new(f32_value(value)));
        Self {
            state: DummyShaderState {
                outputs: MapSlot::with_version(Revision::new(0), outputs),
            },
            tick_count,
        }
    }
}

impl NodeRuntime for DummyShaderNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        for output in self.state.outputs.entries.values_mut() {
            output.set_with_version(ctx.revision(), *output.value());
        }
        Ok(ProduceResult::Produced)
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        Ok(())
    }

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        DummyShaderState::register_runtime_state_shape(registry).map(|_| ())
    }
}

pub(crate) struct DummyFixtureNode {
    slot: SlotPath,
    record: RecordedValue,
}

impl DummyFixtureNode {
    fn new(slot: SlotPath, record: RecordedValue) -> Self {
        Self { slot, record }
    }
}

impl NodeRuntime for DummyFixtureNode {
    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let pv = ctx
            .resolve(QueryKey::ConsumedSlot {
                node: ctx.node_id(),
                slot: self.slot.clone(),
            })
            .map_err(|e| NodeError::msg(format!("fixture resolve failed: {}", e.message)))?;
        self.record.record(&pv.as_value().expect("value"));
        Ok(())
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        Ok(())
    }
}

pub(crate) struct DummyOutputNode {
    slot: SlotPath,
    record: RecordedValue,
}

impl DummyOutputNode {
    fn new(slot: SlotPath, record: RecordedValue) -> Self {
        Self { slot, record }
    }
}

impl NodeRuntime for DummyOutputNode {
    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let pv = ctx
            .resolve(QueryKey::ConsumedSlot {
                node: ctx.node_id(),
                slot: self.slot.clone(),
            })
            .map_err(|e| NodeError::msg(format!("output resolve failed: {}", e.message)))?;
        self.record.record(&pv.as_value().expect("value"));
        Ok(())
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        Ok(())
    }
}

fn channel_name(name: &str) -> ChannelName {
    ChannelName(String::from(name))
}

fn f32_value(value: LpsValueF32) -> f32 {
    match value {
        LpsValueF32::F32(v) => v,
        _ => panic!("test literal must be f32"),
    }
}

fn output_key(path: &SlotPath) -> u32 {
    let [
        SlotPathSegment::Field(field),
        SlotPathSegment::Key(SlotMapKey::U32(key)),
    ] = path.segments()
    else {
        panic!("test shader output path must be outputs[<u32>]");
    };
    assert_eq!(field.as_str(), "outputs");
    *key
}
