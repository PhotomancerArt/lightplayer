use lp_studio_core::{
    ActionId, BROWSER_WORKER_PROVIDER_ID, ClientSession, ConnectionSession, DeviceCapability,
    DeviceId, DeviceSession, ProjectSession, STUDIO_DEMO_PROJECT_ID, StudioDiagnostic,
    StudioHeartbeat, StudioLogEntry, StudioLogLevel, StudioState,
};
use lpa_link::{
    LinkConnectionKind, LinkEndpoint, LinkEndpointId, LinkEndpointStatus, LinkProviderId,
    LinkSessionId,
};
use lpc_model::{
    ArtifactLocation, ArtifactSpec, AssetBodyOrigin, AssetContentType, AssetEntry, AssetLocation,
    AssetState, NodeDefEntry, NodeDefLocation, NodeDefState, NodeInvocation, NodeUseLocation,
    ProjectInventory, ProjectNode, ProjectNodePlacement, Revision, SlotPath,
};
use lpc_wire::{WireProjectHandle, WireProjectInventoryReadResponse};

pub fn studio_state_idle() -> StudioState {
    StudioState::default()
}

pub fn studio_state_connecting() -> StudioState {
    let mut state = StudioState::default();
    state.link_selection.endpoints =
        vec![browser_endpoint().with_status(LinkEndpointStatus::Launching)];
    state
}

pub fn studio_state_connected() -> StudioState {
    let mut state = StudioState::default();
    state.link_selection.endpoints =
        vec![browser_endpoint().with_status(LinkEndpointStatus::Connected)];
    attach_device_session(&mut state);
    state.heartbeat = Some(StudioHeartbeat {
        fps_avg: 59.8,
        frame_count: 1_284,
        loaded_project_count: 0,
        uptime_ms: 42_000,
        free_memory_bytes: Some(154_112),
    });
    state
}

pub fn studio_state_ready() -> StudioState {
    let mut state = studio_state_connected();
    state.project_session = Some(project_session(
        Some(demo_inventory()),
        Some("nodes[shader]"),
    ));
    state.logs = vec![
        StudioLogEntry::new(StudioLogLevel::Info, "fw-browser", "runtime ready"),
        StudioLogEntry::new(
            StudioLogLevel::Debug,
            "lp-studio-runtime",
            "loaded studio-demo and read inventory",
        ),
    ];
    state
}

pub fn studio_state_error() -> StudioState {
    let mut state = studio_state_connected();
    state.diagnostics.push(StudioDiagnostic::error(
        Some(ActionId::new(42)),
        "Browser worker did not respond before the startup timeout.",
    ));
    state
}

pub fn studio_state_long_content() -> StudioState {
    let mut state = studio_state_ready();
    if let Some(device) = &mut state.device_session {
        device.session_id =
            LinkSessionId::new("browser-worker-worker-1:session-with-a-very-long-debug-identifier");
    }
    if let Some(project) = &mut state.project_session {
        project.project_id =
            "studio-demo-with-a-long-human-readable-name-for-layout-testing".to_string();
        project.selected_node_id =
            Some("nodes[shader] / uniforms[palette] / nested[very.deep.path]".to_string());
    }
    state
}

pub fn studio_state_log_heavy() -> StudioState {
    let mut state = studio_state_ready();
    state.logs = (0..18)
        .map(|index| {
            let level = match index % 5 {
                0 => StudioLogLevel::Trace,
                1 => StudioLogLevel::Debug,
                2 => StudioLogLevel::Info,
                3 => StudioLogLevel::Warn,
                _ => StudioLogLevel::Error,
            };
            StudioLogEntry::new(
                level,
                "fw-browser",
                format!("tick {index}: queued protocol frame and drained output"),
            )
        })
        .collect();
    state.diagnostics.push(StudioDiagnostic::info(
        "Inventory refresh completed from story fixture data.",
    ));
    state
}

fn attach_device_session(state: &mut StudioState) {
    let provider_id = LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID);
    let endpoint_id = LinkEndpointId::new("browser-worker-worker-1");
    let session_id = LinkSessionId::new("browser-worker-worker-1:1");
    state.device_session = Some(DeviceSession {
        device_id: DeviceId::new("browser-worker:browser-worker-worker-1"),
        provider_id: provider_id.clone(),
        endpoint_id: endpoint_id.clone(),
        session_id: session_id.clone(),
        capabilities: vec![
            DeviceCapability::Connect,
            DeviceCapability::UseBrowserWorker,
            DeviceCapability::ReadHeartbeat,
            DeviceCapability::LoadProject,
            DeviceCapability::ReadProjectInventory,
            DeviceCapability::ReadLogs,
        ],
    });
    state.connection_session = Some(ConnectionSession {
        endpoint_id,
        session_id,
        kind: LinkConnectionKind::BrowserWorker {
            protocol: "fw-browser-post-message-v1".to_string(),
        },
    });
    state.client_session = Some(ClientSession::connected("lp-server"));
}

fn browser_endpoint() -> LinkEndpoint {
    LinkEndpoint::new(
        "browser-worker-worker-1",
        BROWSER_WORKER_PROVIDER_ID,
        "Browser runtime worker",
    )
}

fn project_session(
    inventory: Option<WireProjectInventoryReadResponse>,
    selected_node_id: Option<&str>,
) -> ProjectSession {
    let mut session = ProjectSession::new(STUDIO_DEMO_PROJECT_ID, WireProjectHandle::new(1));
    session.inventory = inventory;
    session.selected_node_id = selected_node_id.map(str::to_string);
    session
}

fn demo_inventory() -> WireProjectInventoryReadResponse {
    let mut inventory = ProjectInventory::new();
    let root = NodeUseLocation::root();
    let clock = child(&root, "nodes[clock]");
    let fixture = child(&root, "nodes[fixture]");
    let output = child(&root, "nodes[output]");
    let shader = child(&root, "nodes[shader]");

    for (path, revision) in [
        ("/projects/studio-demo/project.toml", 1),
        ("/projects/studio-demo/clock.toml", 2),
        ("/projects/studio-demo/fixture.toml", 3),
        ("/projects/studio-demo/output.toml", 4),
        ("/projects/studio-demo/shader.toml", 5),
    ] {
        let location = def_location(path);
        inventory.defs.insert(
            location.clone(),
            NodeDefEntry::new(location, NodeDefState::NotFound, Revision::new(revision)),
        );
    }

    let shader_asset = AssetLocation::artifact(ArtifactLocation::file(
        "/projects/studio-demo/shaders/rainbow.glsl",
    ));
    inventory.assets.insert(
        shader_asset.clone(),
        AssetEntry::new(
            shader_asset.clone(),
            AssetContentType::ShaderSource,
            AssetState::Available {
                origin: AssetBodyOrigin::Committed,
            },
            Revision::new(6),
        ),
    );

    inventory.tree.insert_node(ProjectNode::root(
        root.clone(),
        def_location("/projects/studio-demo/project.toml"),
    ));
    inventory
        .tree
        .insert_node(project_node(clock, root.clone(), "clock"));
    inventory
        .tree
        .insert_node(project_node(fixture, root.clone(), "fixture"));
    inventory
        .tree
        .insert_node(project_node(output, root.clone(), "output"));
    inventory
        .tree
        .insert_node(project_node(shader.clone(), root, "shader"));
    inventory.tree.add_asset_consumer(shader_asset, shader);

    WireProjectInventoryReadResponse::from_inventory(&inventory)
}

fn project_node(key: NodeUseLocation, parent: NodeUseLocation, name: &str) -> ProjectNode {
    let slot = SlotPath::parse(&format!("nodes[{name}]")).expect("valid story fixture slot path");
    ProjectNode::invocation(
        key,
        parent,
        def_location(&format!("/projects/studio-demo/{name}.toml")),
        slot,
        ProjectNodePlacement::ProjectChild {
            name: name.to_string(),
        },
        NodeInvocation::path(ArtifactSpec::path(format!("{name}.toml"))),
    )
}

fn child(parent: &NodeUseLocation, slot: &str) -> NodeUseLocation {
    parent.child(SlotPath::parse(slot).expect("valid story fixture slot path"))
}

fn def_location(path: &str) -> NodeDefLocation {
    NodeDefLocation::artifact_root(ArtifactLocation::file(path))
}
