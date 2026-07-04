//! Integration-style tests for the demand-driven runtime spine.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use lpc_engine::dataflow::binding::{
    BindingEntry, BindingPriority, BindingRef, BindingSource, BindingTarget,
};
use lpc_engine::dataflow::resolver::{
    Production, QueryKey, ResolveHost, ResolveLogLevel, ResolveSession, ResolveTrace, Resolver,
    SessionHostResolver, SessionResolveError, TickResolver,
};
use lpc_engine::node::{
    MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult, TickContext,
};
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{
    ArtifactLocation, AssetChange, AssetChangeKind, AssetLocation, Kind, LpValue, NodeDefChange,
    NodeDefChangeKind, NodeId, NodeUseLocation, Revision, SlotPath, TreePath, bus::ChannelName,
};
use lpc_registry::ParseCtx;
use lpfs::{FsEvent, FsEventKind, LpFsMemory, LpPath, LpPathBuf};
use lps_shared::LpsValueF32;

// --- Tests (concise scenarios; helpers below) ---

#[test]
fn runtime_spine_tick_context_resolve_bus_query() {
    let channel = ChannelName(String::from("live"));
    let frame = Revision::new(99);
    let binding = BindingEntry {
        source: BindingSource::Literal(LpValue::F32(2.0)),
        target: BindingTarget::BusChannel(channel.clone()),
        priority: BindingPriority::new(0),
        kind: Kind::Ratio,
        version: frame,
        owner: NodeId::new(1),
    };

    let mut resolver = Resolver::new();
    let mut session = ResolveSession::new(
        frame,
        &mut resolver,
        ResolveTrace::new(ResolveLogLevel::Off),
    );

    struct NoProduceHost {
        channel: ChannelName,
        binding: BindingEntry,
    }

    impl ResolveHost for NoProduceHost {
        fn produce(
            &mut self,
            _query: &QueryKey,
            _session: &mut ResolveSession<'_>,
        ) -> Result<Production, SessionResolveError> {
            Err(SessionResolveError::other("unexpected produce"))
        }

        fn providers_for_bus(&self, channel: &ChannelName) -> Vec<(BindingRef, BindingEntry)> {
            if channel == &self.channel {
                Vec::from([(BindingRef::new(self.binding.owner, 0), self.binding.clone())])
            } else {
                Vec::new()
            }
        }
    }

    let mut host = NoProduceHost {
        channel: channel.clone(),
        binding,
    };
    let slot_shapes = lpc_model::SlotShapeRegistry::default();
    let mut node = ProduceProbeNode {
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
        &mut bridge as &mut dyn TickResolver,
        &slot_shapes,
    );

    node.produce(&lpc_model::SlotPath::root(), &mut ctx)
        .unwrap();
    assert_eq!(node.last, Some(2.0));
}

#[test]
fn runtime_spine_node_export_is_reachable() {
    fn assert_spine_ptr(_: Option<&dyn NodeRuntime>) {}

    assert_spine_ptr(None);

    let _: Option<fn(&dyn lpc_engine::node::NodeRuntime)> = None;
}

#[test]
fn project_apply_body_change_does_not_recreate_runtime_node() {
    let mut fs = clock_project_fs();
    let services = EngineServices::new(TreePath::parse("/body_change.show").unwrap());
    let loaded = ProjectLoader::load_from_root(&fs, services).expect("load");
    let (mut engine, mut registry) = loaded.into_parts();
    let clock_use = NodeUseLocation::root().child(SlotPath::parse("nodes[clock]").unwrap());
    let before = engine
        .project_runtime_index()
        .node_id(&clock_use)
        .expect("clock runtime node");

    fs.write_file_mut(
        LpPath::new("/clock.json"),
        br#"
{
  "kind": "Clock",
  "controls": {
    "rate": 2.0
  }
}
"#,
    )
    .expect("write clock");
    let shapes = engine.slot_shapes().clone();
    let changes = registry.refresh_artifacts(
        &fs,
        &[FsEvent {
            path: LpPathBuf::from("/clock.json"),
            kind: FsEventKind::Modify,
        }],
        Revision::new(2),
        &ParseCtx { shapes: &shapes },
    );

    assert_eq!(
        changes.defs.changed,
        vec![NodeDefChange::new(
            lpc_model::NodeDefLocation::artifact_root(ArtifactLocation::file("/clock.json")),
            NodeDefChangeKind::Body,
        )]
    );
    assert!(changes.uses.is_empty());
    let apply = engine
        .apply_project_changes(&fs, &mut registry, &changes)
        .expect("apply changes");

    assert!(apply.is_empty());
    assert_eq!(
        engine.project_runtime_index().node_id(&clock_use),
        Some(before)
    );
}

#[test]
fn project_apply_added_node_use_preserves_existing_runtime_node() {
    let mut fs = clock_project_fs();
    let services = EngineServices::new(TreePath::parse("/add_use.show").unwrap());
    let loaded = ProjectLoader::load_from_root(&fs, services).expect("load");
    let (mut engine, mut registry) = loaded.into_parts();
    let clock_use = NodeUseLocation::root().child(SlotPath::parse("nodes[clock]").unwrap());
    let shader_use = NodeUseLocation::root().child(SlotPath::parse("nodes[shader]").unwrap());
    let clock_before = engine
        .project_runtime_index()
        .node_id(&clock_use)
        .expect("clock runtime node");

    fs.write_file_mut(
        LpPath::new("/project.json"),
        br#"
{
  "kind": "Project",
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    },
    "shader": {
      "ref": "./shader.json"
    }
  }
}
"#,
    )
    .expect("write project");
    fs.write_file_mut(
        LpPath::new("/shader.json"),
        br#"
{
  "kind": "Shader",
  "source": {
    "path": "shader.glsl"
  }
}
"#,
    )
    .expect("write shader def");
    fs.write_file_mut(LpPath::new("/shader.glsl"), b"void main() {}")
        .expect("write shader source");

    let shapes = engine.slot_shapes().clone();
    let changes = registry.refresh_artifacts(
        &fs,
        &[FsEvent {
            path: LpPathBuf::from("/project.json"),
            kind: FsEventKind::Modify,
        }],
        Revision::new(2),
        &ParseCtx { shapes: &shapes },
    );

    assert_eq!(changes.uses.added, vec![shader_use.clone()]);
    let apply = engine
        .apply_project_changes(&fs, &mut registry, &changes)
        .expect("apply changes");

    assert_eq!(apply.added_nodes, vec![shader_use.clone()]);
    assert_eq!(
        engine.project_runtime_index().node_id(&clock_use),
        Some(clock_before)
    );
    assert!(
        engine
            .project_runtime_index()
            .node_id(&shader_use)
            .is_some()
    );
}

#[test]
fn project_apply_asset_body_change_refreshes_existing_shader_node() {
    let mut fs = shader_project_fs();
    let services = EngineServices::new(TreePath::parse("/shader_asset_change.show").unwrap());
    let loaded = ProjectLoader::load_from_root(&fs, services).expect("load");
    let (mut engine, mut registry) = loaded.into_parts();
    let shader_use = NodeUseLocation::root().child(SlotPath::parse("nodes[shader]").unwrap());
    let shader_before = engine
        .project_runtime_index()
        .node_id(&shader_use)
        .expect("shader runtime node");
    let shader_asset = AssetLocation::artifact(ArtifactLocation::file("/shader.glsl"));

    fs.write_file_mut(
        LpPath::new("/shader.glsl"),
        b"vec4 render(vec2 pos) { return vec4(pos.x, 0.0, 0.0, 1.0); }",
    )
    .expect("write shader source");
    let shapes = engine.slot_shapes().clone();
    let changes = registry.refresh_artifacts(
        &fs,
        &[FsEvent {
            path: LpPathBuf::from("/shader.glsl"),
            kind: FsEventKind::Modify,
        }],
        Revision::new(2),
        &ParseCtx { shapes: &shapes },
    );

    assert_eq!(
        changes.assets.changed,
        vec![AssetChange::new(
            shader_asset.clone(),
            AssetChangeKind::Body
        )]
    );
    assert!(changes.defs.is_empty());
    assert!(changes.uses.is_empty());
    let apply = engine
        .apply_project_changes(&fs, &mut registry, &changes)
        .expect("apply changes");

    assert_eq!(apply.refreshed_assets, vec![shader_asset]);
    assert_eq!(apply.refreshed_nodes, vec![shader_use.clone()]);
    assert!(apply.added_nodes.is_empty());
    assert!(apply.removed_nodes.is_empty());
    assert!(apply.reattached_nodes.is_empty());
    assert_eq!(
        engine.project_runtime_index().node_id(&shader_use),
        Some(shader_before)
    );
}

// --- Helpers ---

fn clock_project_fs() -> LpFsMemory {
    let mut fs = LpFsMemory::new();
    fs.write_file_mut(
        LpPath::new("/project.json"),
        br#"
{
  "kind": "Project",
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    }
  }
}
"#,
    )
    .expect("write project");
    fs.write_file_mut(
        LpPath::new("/clock.json"),
        br#"
{
  "kind": "Clock",
  "controls": {
    "rate": 1.0
  }
}
"#,
    )
    .expect("write clock");
    fs
}

fn shader_project_fs() -> LpFsMemory {
    let mut fs = LpFsMemory::new();
    fs.write_file_mut(
        LpPath::new("/project.json"),
        br#"
{
  "kind": "Project",
  "nodes": {
    "shader": {
      "ref": "./shader.json"
    }
  }
}
"#,
    )
    .expect("write project");
    fs.write_file_mut(
        LpPath::new("/shader.json"),
        br#"
{
  "kind": "Shader",
  "source": "shader.glsl"
}
"#,
    )
    .expect("write shader def");
    fs.write_file_mut(
        LpPath::new("/shader.glsl"),
        b"vec4 render(vec2 pos) { return vec4(0.0, pos.y, 0.0, 1.0); }",
    )
    .expect("write shader source");
    fs
}

struct ProduceProbeNode {
    query: QueryKey,
    last: Option<f32>,
}

impl NodeRuntime for ProduceProbeNode {
    fn produce(
        &mut self,
        _slot: &lpc_model::SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        let pv = ctx
            .resolve(self.query.clone())
            .map_err(|e| NodeError::msg(format!("resolve: {}", e.message)))?;
        if let LpsValueF32::F32(v) = pv.as_value().expect("value") {
            self.last = Some(v);
        }
        Ok(ProduceResult::Produced)
    }

    fn destroy(&mut self, _ctx: &mut lpc_engine::node::DestroyCtx<'_>) -> Result<(), NodeError> {
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
