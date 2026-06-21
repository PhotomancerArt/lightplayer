use lp_studio_core::{
    ActionId, BROWSER_SERIAL_ESP32_PROVIDER_ID, BROWSER_WORKER_PROVIDER_ID, ClientSession,
    ConnectedDeviceState, ConnectionSession, DeviceAccess, DeviceAccessStatus, DeviceCapability,
    DeviceFlowState, DeviceId, DeviceIssue, DeviceIssueKind, DeviceSession, ProgressState,
    ProjectChoice, ProjectSelectionReason, ProjectSession, ProviderAvailability,
    ProviderCapability, ProviderCardState, ProviderIntent, ProvisioningReason, RecoveryAction,
    RecoveryReason, STUDIO_DEMO_PROJECT_ID, StudioDiagnostic, StudioHeartbeat, StudioLogEntry,
    StudioLogLevel, StudioState,
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
    studio_state_provider_catalog()
}

pub fn studio_state_provider_catalog() -> StudioState {
    let mut state = StudioState::default();
    set_default_provider_cards(&mut state);
    state
}

pub fn studio_state_requesting_access() -> StudioState {
    let mut state = studio_state_provider_catalog();
    state
        .device_manager
        .providers
        .select_provider(BROWSER_SERIAL_ESP32_PROVIDER_ID);
    state.device_manager.active_flow = DeviceFlowState::RequestingAccess {
        provider_id: LinkProviderId::new(BROWSER_SERIAL_ESP32_PROVIDER_ID),
    };
    state.device_access = Some(DeviceAccess::new(
        BROWSER_SERIAL_ESP32_PROVIDER_ID,
        DeviceAccessStatus::PermissionRequired,
    ));
    state
}

pub fn studio_state_access_canceled() -> StudioState {
    let mut state = studio_state_provider_catalog();
    let provider_id = LinkProviderId::new(BROWSER_SERIAL_ESP32_PROVIDER_ID);
    let issue = DeviceIssue::error(
        "story-access-canceled",
        DeviceIssueKind::PermissionCanceled,
        "The browser device chooser was canceled.",
    )
    .with_provider(provider_id.clone())
    .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::ChooseSimulator]);
    state.device_access = Some(DeviceAccess::new(
        provider_id.clone(),
        DeviceAccessStatus::PermissionCanceled {
            reason: "The browser device chooser was canceled.".to_string(),
        },
    ));
    state
        .device_manager
        .providers
        .select_provider(provider_id.clone());
    state.device_manager.active_flow = DeviceFlowState::AccessFailed {
        provider_id,
        issue: issue.clone(),
    };
    state.device_manager.push_issue(issue);
    state
}

pub fn studio_state_connecting() -> StudioState {
    let mut state = StudioState::default();
    set_default_provider_cards(&mut state);
    set_provider_endpoints(
        &mut state,
        BROWSER_WORKER_PROVIDER_ID,
        vec![browser_endpoint().with_status(LinkEndpointStatus::Launching)],
    );
    state.device_manager.active_flow = DeviceFlowState::OpeningLink {
        endpoint_id: LinkEndpointId::new("browser-worker-worker-1"),
    };
    state
}

pub fn studio_state_connected() -> StudioState {
    let mut state = StudioState::default();
    set_default_provider_cards(&mut state);
    set_provider_endpoints(
        &mut state,
        BROWSER_WORKER_PROVIDER_ID,
        vec![browser_endpoint().with_status(LinkEndpointStatus::Connected)],
    );
    attach_device_session(&mut state);
    if let Some(session) = &state.device_session {
        state.device_manager.active_flow = DeviceFlowState::ServerReady {
            session_id: session.session_id.clone(),
        };
    }
    state.heartbeat = Some(StudioHeartbeat {
        fps_avg: 59.8,
        frame_count: 1_284,
        loaded_project_count: 0,
        uptime_ms: 42_000,
        free_memory_bytes: Some(154_112),
    });
    state
}

pub fn studio_state_probing_server() -> StudioState {
    let mut state = studio_state_connected();
    state.device_manager.active_flow = DeviceFlowState::ProbingTarget {
        endpoint_id: LinkEndpointId::new("browser-worker-worker-1"),
    };
    state
}

pub fn studio_state_blank_device_flash_offer() -> StudioState {
    let mut state = studio_state_hardware_granted();
    state.device_manager.active_flow = DeviceFlowState::ProvisioningRequired {
        endpoint_id: LinkEndpointId::new("browser-serial-esp32-port-1"),
        reason: ProvisioningReason::DeviceBlank,
    };
    state
}

pub fn studio_state_flash_confirm() -> StudioState {
    let mut state = studio_state_blank_device_flash_offer();
    state.device_manager.active_flow = DeviceFlowState::FlashConfirm {
        endpoint_id: LinkEndpointId::new("browser-serial-esp32-port-1"),
        firmware_id: Some("lightplayer-esp32c6-server".to_string()),
    };
    state
}

pub fn studio_state_flashing() -> StudioState {
    let mut state = studio_state_blank_device_flash_offer();
    state.device_manager.active_flow = DeviceFlowState::Flashing {
        endpoint_id: LinkEndpointId::new("browser-serial-esp32-port-1"),
        progress: Some(
            ProgressState::new("Writing LightPlayer firmware")
                .with_steps(1, 3)
                .with_percent(42),
        ),
    };
    state
}

pub fn studio_state_firmware_artifact_missing() -> StudioState {
    let mut state = studio_state_blank_device_flash_offer();
    let issue = DeviceIssue::error(
        "story-firmware-artifact-missing",
        DeviceIssueKind::FirmwareArtifactMissing,
        "The LightPlayer firmware artifact is missing from this Studio build.",
    )
    .with_endpoint("browser-serial-esp32-port-1")
    .with_recovery_actions(vec![
        RecoveryAction::Retry,
        RecoveryAction::OpenHelp {
            topic: "firmware packaging".to_string(),
        },
    ]);
    state.device_manager.active_flow = DeviceFlowState::Degraded {
        issue: issue.clone(),
    };
    state.device_manager.push_issue(issue);
    state
}

pub fn studio_state_flash_failed() -> StudioState {
    let mut state = studio_state_blank_device_flash_offer();
    let issue = DeviceIssue::error(
        "story-flash-failed",
        DeviceIssueKind::FlashFailed,
        "Firmware flashing failed before the device could be restarted.",
    )
    .with_endpoint("browser-serial-esp32-port-1")
    .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::ResetDevice]);
    state.device_manager.active_flow = DeviceFlowState::Degraded {
        issue: issue.clone(),
    };
    state.device_manager.push_issue(issue);
    state
}

pub fn studio_state_post_flash_reconnecting() -> StudioState {
    let mut state = studio_state_hardware_granted();
    state.device_manager.active_flow = DeviceFlowState::OpeningServer {
        endpoint_id: LinkEndpointId::new("browser-serial-esp32-port-1"),
    };
    state.logs.push(StudioLogEntry::new(
        StudioLogLevel::Info,
        "lp-studio-core",
        "firmware flash completed for browser-serial-esp32-port-1 using lightplayer-esp32c6-server",
    ));
    state
}

pub fn studio_state_post_flash_reconnect_failed() -> StudioState {
    let mut state = studio_state_post_flash_reconnecting();
    let issue = DeviceIssue::error(
        "story-post-flash-reconnect-failed",
        DeviceIssueKind::ConnectionLost,
        "Firmware was flashed, but Studio could not reconnect to the device.",
    )
    .with_endpoint("browser-serial-esp32-port-1")
    .with_recovery_actions(vec![RecoveryAction::Reconnect, RecoveryAction::ResetDevice]);
    state.device_manager.active_flow = DeviceFlowState::Degraded {
        issue: issue.clone(),
    };
    state.device_manager.push_issue(issue);
    state
}

pub fn studio_state_post_flash_ready() -> StudioState {
    let mut state = studio_state_ready();
    state.logs.push(StudioLogEntry::new(
        StudioLogLevel::Info,
        "lp-studio-core",
        "firmware flash completed for browser-serial-esp32-port-1 using lightplayer-esp32c6-server",
    ));
    state
}

pub fn studio_state_reading_project_state() -> StudioState {
    let mut state = studio_state_connected();
    state.device_manager.active_flow = DeviceFlowState::ReadingProjectState {
        session_id: LinkSessionId::new("browser-worker-worker-1:1"),
    };
    state
}

pub fn studio_state_project_selection_required() -> StudioState {
    let mut state = studio_state_connected();
    state.device_manager.active_flow = DeviceFlowState::ProjectSelectionRequired {
        session_id: LinkSessionId::new("browser-worker-worker-1:1"),
        reason: ProjectSelectionReason::NoLoadedProject,
        projects: Vec::new(),
    };
    state
}

pub fn studio_state_multiple_project_selection_required() -> StudioState {
    let mut state = studio_state_connected();
    state.device_manager.active_flow = DeviceFlowState::ProjectSelectionRequired {
        session_id: LinkSessionId::new("browser-worker-worker-1:1"),
        reason: ProjectSelectionReason::MultipleLoadedProjects,
        projects: vec![
            ProjectChoice::new(
                STUDIO_DEMO_PROJECT_ID,
                "/projects/studio-demo",
                WireProjectHandle::new(1),
            ),
            ProjectChoice::new("gallery", "/projects/gallery", WireProjectHandle::new(2)),
        ],
    };
    state
}

pub fn studio_state_recovery_required() -> StudioState {
    let mut state = studio_state_connected();
    state.device_manager.active_flow = DeviceFlowState::RecoveryRequired {
        session_id: LinkSessionId::new("browser-worker-worker-1:1"),
        reason: RecoveryReason::ProjectCrash {
            project_id: Some(STUDIO_DEMO_PROJECT_ID.to_string()),
            message: Some("The previous project crashed during startup.".to_string()),
        },
    };
    state
}

pub fn studio_state_deploying_project() -> StudioState {
    let mut state = studio_state_connected();
    state.device_manager.active_flow = DeviceFlowState::DeployingProject {
        project_id: STUDIO_DEMO_PROJECT_ID.to_string(),
        progress: Some(
            ProgressState::new("Writing starter project")
                .with_steps(2, 4)
                .with_percent(50),
        ),
    };
    state
}

pub fn studio_state_connection_lost() -> StudioState {
    let mut state = studio_state_connected();
    let issue = DeviceIssue::error(
        "story-connection-lost",
        DeviceIssueKind::ConnectionLost,
        "The device connection was lost.",
    )
    .with_recovery_actions(vec![
        RecoveryAction::Reconnect,
        RecoveryAction::ChooseSimulator,
    ]);
    state.device_manager.active_flow = DeviceFlowState::Degraded {
        issue: issue.clone(),
    };
    state.device_manager.push_issue(issue);
    state
}

pub fn studio_state_hardware_unsupported() -> StudioState {
    let mut state = StudioState::default();
    set_default_provider_cards(&mut state);
    state.device_manager.providers.set_provider_availability(
        LinkProviderId::new(BROWSER_SERIAL_ESP32_PROVIDER_ID),
        ProviderAvailability::unavailable(
            "Web Serial is not supported in this browser.",
            vec![
                RecoveryAction::UseCompatibleBrowser,
                RecoveryAction::ChooseSimulator,
            ],
        ),
    );
    state
        .device_manager
        .providers
        .select_provider(BROWSER_SERIAL_ESP32_PROVIDER_ID);
    state.device_access = Some(DeviceAccess::new(
        BROWSER_SERIAL_ESP32_PROVIDER_ID,
        DeviceAccessStatus::Unsupported {
            reason: "Web Serial is not supported in this browser.".to_string(),
        },
    ));
    state
}

pub fn studio_state_hardware_denied() -> StudioState {
    let mut state = StudioState::default();
    set_default_provider_cards(&mut state);
    state
        .device_manager
        .providers
        .select_provider(BROWSER_SERIAL_ESP32_PROVIDER_ID);
    state.device_access = Some(DeviceAccess::new(
        BROWSER_SERIAL_ESP32_PROVIDER_ID,
        DeviceAccessStatus::PermissionDenied {
            reason: "No port selected.".to_string(),
        },
    ));
    state
}

pub fn studio_state_hardware_granted() -> StudioState {
    let mut state = StudioState::default();
    set_default_provider_cards(&mut state);
    state
        .device_manager
        .providers
        .select_provider(BROWSER_SERIAL_ESP32_PROVIDER_ID);
    state.device_access = Some(DeviceAccess::new(
        BROWSER_SERIAL_ESP32_PROVIDER_ID,
        DeviceAccessStatus::Granted,
    ));
    set_provider_endpoints(
        &mut state,
        BROWSER_SERIAL_ESP32_PROVIDER_ID,
        vec![LinkEndpoint::new(
            "browser-serial-esp32-port-1",
            BROWSER_SERIAL_ESP32_PROVIDER_ID,
            "ESP32 Serial (303a:1001)",
        )],
    );
    state
}

pub fn studio_state_ready() -> StudioState {
    let mut state = studio_state_connected();
    state.project_session = Some(project_session(
        Some(demo_inventory()),
        Some("nodes[shader]"),
    ));
    state.device_manager.active_flow = DeviceFlowState::Ready {
        project_id: STUDIO_DEMO_PROJECT_ID.to_string(),
    };
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

pub fn studio_state_protocol_diagnostic() -> StudioState {
    let mut state = studio_state_connected();
    let message = concat!(
        "malformed M! frame while waiting for response id=1 kind=project.list_loaded: ",
        "missing field `min` at line 1 column 92; json={\\\"id\\\":0,\\\"msg\\\":{",
        "\\\"heartbeat\\\":{\\\"fps\\\":{\\\"avg\\\":29.06281,\\\"sdev\\\":0,",
        "\\\"miin\\\":29.102337,\\\"max\\\":29.102337},\\\"frame_count\\\":3065,",
        "\\\"loaded_projects\\\":[{\\\"handle\\\":1,\\\"path\\\":\\\"/projects/basic\\\"}],",
        "\\\"uptime_ms\\\":105318,\\\"memory\\\":{\\\"freeBytes\\\":154112,",
        "\\\"totalBytes\\\":299008}}}}"
    );
    state
        .diagnostics
        .push(StudioDiagnostic::error(Some(ActionId::new(43)), message));
    state.logs.push(StudioLogEntry::new(
        StudioLogLevel::Warn,
        "browser-serial",
        message,
    ));
    state
}

pub fn studio_state_long_content() -> StudioState {
    let mut state = studio_state_ready();
    let long_session_id =
        LinkSessionId::new("browser-worker-worker-1:session-with-a-very-long-debug-identifier");
    if let Some(device) = &mut state.device_session {
        device.session_id = long_session_id.clone();
    }
    if let Some(device) = &mut state.device_manager.current_device {
        device.session_id = long_session_id;
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
            DeviceCapability::WriteProjectFiles,
            DeviceCapability::ReadHeartbeat,
            DeviceCapability::LoadProject,
            DeviceCapability::ReadProjectInventory,
            DeviceCapability::ReadLogs,
        ],
    });
    state.connection_session = Some(ConnectionSession {
        endpoint_id,
        session_id: session_id.clone(),
        kind: LinkConnectionKind::BrowserWorker {
            protocol: "fw-browser-post-message-v1".to_string(),
        },
    });
    state.client_session = Some(ClientSession::connected("lp-server"));
    state.device_manager.current_device = Some(ConnectedDeviceState::connected(
        DeviceId::new("browser-worker:browser-worker-worker-1"),
        provider_id,
        LinkEndpointId::new("browser-worker-worker-1"),
        session_id,
        LinkConnectionKind::BrowserWorker {
            protocol: "fw-browser-post-message-v1".to_string(),
        },
        vec![
            DeviceCapability::Connect,
            DeviceCapability::UseBrowserWorker,
            DeviceCapability::WriteProjectFiles,
            DeviceCapability::ReadHeartbeat,
            DeviceCapability::LoadProject,
            DeviceCapability::ReadProjectInventory,
            DeviceCapability::ReadLogs,
        ],
    ));
}

fn browser_endpoint() -> LinkEndpoint {
    LinkEndpoint::new(
        "browser-worker-worker-1",
        BROWSER_WORKER_PROVIDER_ID,
        "Browser runtime worker",
    )
}

fn set_provider_endpoints(
    state: &mut StudioState,
    provider_id: impl Into<LinkProviderId>,
    endpoints: Vec<LinkEndpoint>,
) {
    let provider_id = provider_id.into();
    state
        .device_manager
        .providers
        .select_provider(provider_id.clone());
    state
        .device_manager
        .providers
        .set_provider_endpoints(provider_id, endpoints);
}

fn set_default_provider_cards(state: &mut StudioState) {
    state.device_manager.providers.set_providers(vec![
        ProviderCardState::new(
            BROWSER_WORKER_PROVIDER_ID,
            "Simulator",
            ProviderIntent::SimulateInBrowser,
        )
        .with_availability(ProviderAvailability::Available)
        .with_capabilities(vec![
            ProviderCapability::DiscoverEndpoints,
            ProviderCapability::Connect,
            ProviderCapability::Simulate,
            ProviderCapability::DeployProject,
            ProviderCapability::ReadProjectInventory,
        ])
        .with_endpoints(vec![browser_endpoint()]),
        ProviderCardState::new(
            BROWSER_SERIAL_ESP32_PROVIDER_ID,
            "USB ESP32",
            ProviderIntent::ConnectUsbEsp32,
        )
        .with_availability(ProviderAvailability::AvailableWithPermission)
        .with_capabilities(vec![
            ProviderCapability::RequestAccess,
            ProviderCapability::DiscoverEndpoints,
            ProviderCapability::Connect,
            ProviderCapability::ResetDevice,
            ProviderCapability::FlashFirmware,
            ProviderCapability::DeployProject,
            ProviderCapability::ReadProjectInventory,
        ]),
    ]);
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
