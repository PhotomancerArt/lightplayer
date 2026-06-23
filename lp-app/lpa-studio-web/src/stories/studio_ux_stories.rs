use dioxus::prelude::*;
use lpa_studio_ux::{
    DeviceOp, DeviceUx, LinkEndpointId, LinkProviderKind, ProgressState, ProjectInventorySummary,
    ProjectOp, ProjectState, ProjectUx, StudioView, UiAction, UiActivity, UiActivityStep,
    UiActivityStepState, UiBody, UiMetric, UiPaneView, UiProgress, UiStackSection, UiStackView,
    UiStatus, UiStepState, UiTerminalLine, UxIssue, UxLogEntry, UxLogLevel, UxNodeId,
};

use crate::components::{
    ActionStrip, FieldRow, MetricGrid, PaneFrame, StudioShell, TabItem, Tabs, UxPane,
};
use crate::stories::story::StoryDescriptor;

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "studio/actions/provider-actions",
        "Studio UX",
        "Connection actions",
        "Generic action strip for connection choices exposed by Device UX.",
    ),
    StoryDescriptor::new(
        "studio/primitives/editor-fields",
        "Studio UX",
        "Editor fields",
        "Field, metric, and tab primitives for the project editor foundation.",
    ),
    StoryDescriptor::new(
        "studio/editor-shell",
        "Studio UX",
        "Editor shell",
        "Responsive project editor shell with node tree, node workspace, and device rail.",
    ),
    StoryDescriptor::new(
        "studio/panes/device",
        "Studio UX",
        "Device pane",
        "Device pane rendered from a stack of connection, LightPlayer, and project steps.",
    ),
    StoryDescriptor::new(
        "studio/panes/project",
        "Studio UX",
        "Project pane",
        "Loaded project pane rendered directly from the Project UX view.",
    ),
    StoryDescriptor::new(
        "studio/device-project-empty",
        "Studio UX",
        "Project launcher",
        "Device pane offering running-project attach and demo loading.",
    ),
    StoryDescriptor::new(
        "studio/device-project-selection",
        "Studio UX",
        "Project selection",
        "Loaded project choices exposed in the Device open-project step.",
    ),
    StoryDescriptor::new(
        "studio/simulator-idle",
        "Studio UX",
        "Simulator idle",
        "Initial Studio UX state before launching the browser simulator.",
    ),
    StoryDescriptor::new(
        "studio/browser-serial-canceled",
        "Studio UX",
        "Serial chooser canceled",
        "Browser serial picker after the native dialog was canceled.",
    ),
    StoryDescriptor::new(
        "studio/browser-serial-open-failed",
        "Studio UX",
        "Serial open failed",
        "Recoverable serial-open failure with picker actions still available.",
    ),
    StoryDescriptor::new(
        "studio/simulator-endpoint",
        "Studio UX",
        "Simulator endpoint",
        "Endpoint choices returned by the selected lpa-link provider.",
    ),
    StoryDescriptor::new(
        "studio/simulator-starting",
        "Studio UX",
        "Simulator starting",
        "Progress state while the selected endpoint is opening.",
    ),
    StoryDescriptor::new(
        "studio/simulator-ready",
        "Studio UX",
        "Simulator ready",
        "Connected simulator after the UX layer auto-loads the demo project.",
    ),
    StoryDescriptor::new(
        "studio/server-disconnected-link-ready",
        "Studio UX",
        "Server disconnected",
        "Open device session with LightPlayer detached and reconnect action available.",
    ),
    StoryDescriptor::new(
        "studio/provision-ready",
        "Studio UX",
        "Flash ready",
        "Blank ESP32 device session offering firmware flashing.",
    ),
    StoryDescriptor::new(
        "studio/browser-serial-blank-firmware",
        "Studio UX",
        "Blank firmware readiness",
        "Browser serial readiness with boot logs and firmware flashing available.",
    ),
    StoryDescriptor::new(
        "studio/provisioning",
        "Studio UX",
        "Flashing",
        "Progress while Studio flashes packaged LightPlayer firmware.",
    ),
    StoryDescriptor::new(
        "studio/provision-failed",
        "Studio UX",
        "Flash failed",
        "Firmware flashing issue with retry and disconnect actions.",
    ),
    StoryDescriptor::new(
        "studio/resetting-to-blank",
        "Studio UX",
        "Wiping",
        "Progress while Studio erases an existing ESP32.",
    ),
    StoryDescriptor::new(
        "studio/reset-complete",
        "Studio UX",
        "Reset complete",
        "Blank ESP32 after erase with firmware flashing available again.",
    ),
    StoryDescriptor::new(
        "studio/project-ready",
        "Studio UX",
        "Project ready",
        "Demo project loaded and synced through the UX view.",
    ),
    StoryDescriptor::new(
        "studio/project-syncing",
        "Studio UX",
        "Project syncing",
        "Loaded project while Studio is reading the project mirror.",
    ),
    StoryDescriptor::new(
        "studio/project-sync-failed",
        "Studio UX",
        "Project sync failed",
        "Loaded project with recoverable sync failure and refresh available.",
    ),
    StoryDescriptor::new(
        "studio/error",
        "Studio UX",
        "Action error",
        "Failure state shown through the same shell and action surface.",
    ),
];

pub fn render_story(id: &str) -> Option<Element> {
    match id {
        "studio/actions/provider-actions" => {
            return Some(rsx! {
                PaneFrame {
                    title: "Actions",
                    primary: true,
                    status: None::<UiStatus>,
                    ActionStrip {
                        actions: start_actions(),
                        running: false,
                        on_action: move |_| {},
                    }
                }
            });
        }
        "studio/primitives/editor-fields" => return Some(editor_primitives_story()),
        "studio/editor-shell" => return Some(editor_shell_story()),
        "studio/panes/device" => {
            let view = idle_device_view();
            return Some(rsx! {
                UxPane {
                    view,
                    primary: true,
                    running: false,
                    on_action: move |_| {},
                }
            });
        }
        "studio/panes/project" => {
            let view = project_view(project_ready_state(), true);
            return Some(rsx! {
                UxPane {
                    view,
                    primary: false,
                    running: false,
                    on_action: move |_| {},
                }
            });
        }
        "studio/device-project-empty" => {
            let view = device_project_empty_view();
            return Some(rsx! {
                UxPane {
                    view,
                    primary: true,
                    running: false,
                    on_action: move |_| {},
                }
            });
        }
        "studio/device-project-selection" => {
            let view = device_project_selection_view();
            return Some(rsx! {
                UxPane {
                    view,
                    primary: true,
                    running: false,
                    on_action: move |_| {},
                }
            });
        }
        _ => {}
    }

    let (mut view, running, story_logs) = match id {
        "studio/simulator-idle" => (idle_view(), false, Vec::new()),
        "studio/browser-serial-canceled" => (browser_serial_canceled_view(), false, Vec::new()),
        "studio/browser-serial-open-failed" => {
            (browser_serial_open_failed_view(), false, Vec::new())
        }
        "studio/simulator-endpoint" => (endpoint_view(), false, Vec::new()),
        "studio/simulator-starting" => (starting_view(), true, Vec::new()),
        "studio/simulator-ready" => (
            simulator_ready_view(),
            false,
            vec![
                studio_log(UxLogLevel::Info, "Simulator is running"),
                studio_log(UxLogLevel::Info, "Demo project loaded"),
            ],
        ),
        "studio/server-disconnected-link-ready" => (
            lightplayer_disconnected_view(),
            false,
            vec![studio_log(UxLogLevel::Info, "LightPlayer disconnected")],
        ),
        "studio/provision-ready" => (provision_ready_view(), false, Vec::new()),
        "studio/browser-serial-blank-firmware" => {
            (browser_serial_blank_firmware_view(), false, Vec::new())
        }
        "studio/provisioning" => (provisioning_view(), true, Vec::new()),
        "studio/provision-failed" => (
            provision_failed_view(),
            false,
            vec![studio_log(
                UxLogLevel::Error,
                "browser serial firmware flashing failed",
            )],
        ),
        "studio/resetting-to-blank" => (resetting_to_blank_view(), true, Vec::new()),
        "studio/reset-complete" => (
            reset_complete_view(),
            false,
            vec![studio_log(UxLogLevel::Info, "ESP32-C6 wiped")],
        ),
        "studio/project-ready" => (
            project_ready_view(),
            false,
            vec![studio_log(UxLogLevel::Info, "Demo project loaded")],
        ),
        "studio/project-syncing" => (
            project_syncing_view(),
            true,
            vec![studio_log(UxLogLevel::Info, "Reading project shapes")],
        ),
        "studio/project-sync-failed" => (
            project_sync_failed_view(),
            false,
            vec![studio_log(
                UxLogLevel::Error,
                "project sync failed: protocol timeout",
            )],
        ),
        "studio/error" => (
            error_view(),
            false,
            vec![studio_log(
                UxLogLevel::Error,
                "browser worker boot timed out",
            )],
        ),
        _ => return None,
    };
    view.logs.extend(story_logs);

    Some(rsx! {
        StudioShell {
            view,
            running,
            on_action: move |_| {},
        }
    })
}

fn editor_primitives_story() -> Element {
    rsx! {
        PaneFrame {
            title: "Node inspector",
            primary: true,
            status: Some(UiStatus::good("Overlay active")),
            div { class: "ux-editor-inspector",
                FieldRow {
                    label: "Name",
                    value: "Orbit wash",
                    changed: false,
                    detail: None::<String>,
                }
                FieldRow {
                    label: "Brightness",
                    value: "0.72",
                    changed: true,
                    detail: Some("overlay value, not committed".to_string()),
                }
                FieldRow {
                    label: "Shader",
                    value: "assets/shaders/orbit.glsl",
                    changed: false,
                    detail: Some("resource reference".to_string()),
                }
                MetricGrid {
                    metrics: vec![
                        UiMetric::new("Inputs", 5),
                        UiMetric::new("Outputs", 2),
                        UiMetric::new("Bindings", 1),
                        UiMetric::new("Preview", "live"),
                    ],
                }
                Tabs {
                    tabs: vec![
                        TabItem::new("Values", "Slot values", "Direct values shown from the current overlay."),
                        TabItem::new("Changes", "Pending changes", "Brightness will be committed with the project overlay."),
                        TabItem::new("Assets", "Node assets", "Shader and SVG assets will open in editor-specific panes."),
                    ],
                    initial: 0,
                }
            }
        }
    }
}

fn editor_shell_story() -> Element {
    rsx! {
        div { class: "ux-editor-shell",
            div { class: "ux-editor-desktop-tree",
                NodeTreePane {}
            }
            div { class: "ux-editor-workspace",
                NodeWorkspacePane {}
            }
            div { class: "ux-editor-side",
                DeviceSidePane {}
                ConsoleSidePane {}
            }
            div { class: "ux-editor-compact-side",
                SecondaryTabsPane {}
            }
            div { class: "ux-editor-mobile",
                MobileEditorTabsPane {}
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeTreePane() -> Element {
    rsx! {
        PaneFrame {
            title: "Node tree",
            primary: false,
            status: Some(UiStatus::neutral("Project")),
            ol { class: "ux-node-tree",
                li { class: "ux-node-tree-item ux-node-tree-item-active", "Scene root" }
                li { class: "ux-node-tree-item ux-node-tree-depth-1", "Group: wash" }
                li { class: "ux-node-tree-item ux-node-tree-depth-2", "Shader: orbit" }
                li { class: "ux-node-tree-item ux-node-tree-depth-2", "Palette: sunrise" }
                li { class: "ux-node-tree-item ux-node-tree-depth-1", "Output: strip A" }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeWorkspacePane() -> Element {
    rsx! {
        PaneFrame {
            title: "Shader: orbit",
            primary: true,
            status: Some(UiStatus::warning("2 changes")),
            div { class: "ux-node-workspace",
                div { class: "ux-node-preview",
                    div { class: "ux-node-preview-bars",
                        span {}
                        span {}
                        span {}
                        span {}
                        span {}
                    }
                }
                div { class: "ux-node-fields",
                    FieldRow {
                        label: "Enabled",
                        value: "true",
                        changed: false,
                        detail: None::<String>,
                    }
                    FieldRow {
                        label: "Brightness",
                        value: "0.72",
                        changed: true,
                        detail: Some("overlay".to_string()),
                    }
                    FieldRow {
                        label: "Speed",
                        value: "bind /bus/audio/energy",
                        changed: true,
                        detail: Some("binding".to_string()),
                    }
                    FieldRow {
                        label: "Shader source",
                        value: "assets/shaders/orbit.glsl",
                        changed: false,
                        detail: Some("asset".to_string()),
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DeviceSidePane() -> Element {
    rsx! {
        PaneFrame {
            title: "Device",
            primary: false,
            status: Some(UiStatus::good("Connected")),
            MetricGrid {
                metrics: vec![
                    UiMetric::new("Runtime", "ESP32-C6"),
                    UiMetric::new("Project", "studio-demo"),
                    UiMetric::new("FPS", "936"),
                    UiMetric::new("Memory", "207k free"),
                ],
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ConsoleSidePane() -> Element {
    rsx! {
        PaneFrame {
            title: "Console",
            primary: false,
            status: None::<UiStatus>,
            ol { class: "ux-terminal ux-editor-terminal",
                li { "[lp-server] heartbeat frame=936" }
                li { "[studio] overlay has 2 pending changes" }
                li { "[fw-esp32] shader backend: native JIT" }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SecondaryTabsPane() -> Element {
    rsx! {
        PaneFrame {
            title: "Project side panel",
            primary: false,
            status: Some(UiStatus::good("Connected")),
            Tabs {
                tabs: vec![
                    TabItem::new("Tree", "Node tree", "Scene root / Group wash / Shader orbit / Output strip A"),
                    TabItem::new("Device", "Device", "ESP32-C6 connected, studio-demo loaded, 936 fps."),
                    TabItem::new("Bus", "Bus", "audio.energy, tempo.bpm, radio.peer_count"),
                    TabItem::new("Console", "Console", "[lp-server] heartbeat frame=936"),
                ],
                initial: 0,
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn MobileEditorTabsPane() -> Element {
    rsx! {
        PaneFrame {
            title: "Project",
            primary: true,
            status: Some(UiStatus::warning("2 changes")),
            Tabs {
                tabs: vec![
                    TabItem::new("Node", "Shader: orbit", "Brightness 0.72, speed bound to /bus/audio/energy."),
                    TabItem::new("Tree", "Node tree", "Scene root / Group wash / Shader orbit."),
                    TabItem::new("Device", "Device", "ESP32-C6 connected, studio-demo loaded."),
                    TabItem::new("Bus", "Bus", "audio.energy, tempo.bpm, radio.peer_count."),
                    TabItem::new("Console", "Console", "[fw-esp32] shader backend: native JIT."),
                ],
                initial: 0,
            }
        }
    }
}

fn studio_log(level: UxLogLevel, message: impl Into<String>) -> UxLogEntry {
    UxLogEntry::new(level, "studio", message)
}

fn idle_view() -> StudioView {
    StudioView::new(vec![idle_device_view()], Vec::new())
}

fn browser_serial_canceled_view() -> StudioView {
    StudioView::new(
        vec![idle_device_view()],
        vec![studio_log(UxLogLevel::Info, "Port selection canceled")],
    )
}

fn browser_serial_open_failed_view() -> StudioView {
    picker_issue_view(
        "Failed to open serial port.",
        "Failed to execute 'open' on 'SerialPort': Failed to open serial port.",
    )
}

fn endpoint_view() -> StudioView {
    StudioView::new(vec![endpoint_device_view()], Vec::new())
}

fn starting_view() -> StudioView {
    StudioView::new(
        vec![starting_device_view()],
        vec![UxLogEntry::new(
            UxLogLevel::Info,
            "lpa-link",
            "browser worker session created",
        )],
    )
}

fn simulator_ready_view() -> StudioView {
    StudioView::new(
        vec![project_synced_pane_view(), simulator_ready_device_view()],
        vec![
            UxLogEntry::new(UxLogLevel::Info, "fw-browser", "ready"),
            UxLogEntry::new(
                UxLogLevel::Info,
                "lpa-link",
                "browser worker session owns Worker lifecycle in lpa-link",
            ),
            UxLogEntry::new(UxLogLevel::Info, "fw-browser", "project loaded"),
        ],
    )
}

fn project_ready_view() -> StudioView {
    StudioView::new(
        vec![project_synced_pane_view(), simulator_ready_device_view()],
        vec![
            UxLogEntry::new(UxLogLevel::Info, "fw-browser", "project loaded"),
            UxLogEntry::new(
                UxLogLevel::Debug,
                "lp-server",
                "heartbeat frame=42 uptime_ms=700",
            ),
        ],
    )
}

fn project_syncing_view() -> StudioView {
    StudioView::new(
        vec![project_syncing_pane_view(), simulator_ready_device_view()],
        vec![UxLogEntry::new(
            UxLogLevel::Info,
            "lpa-studio-ux",
            "syncing project",
        )],
    )
}

fn project_sync_failed_view() -> StudioView {
    StudioView::new(
        vec![
            project_sync_failed_pane_view(),
            simulator_ready_device_view(),
        ],
        vec![UxLogEntry::new(
            UxLogLevel::Error,
            "lpa-studio-ux",
            "project sync failed: protocol timeout",
        )],
    )
}

fn lightplayer_disconnected_view() -> StudioView {
    StudioView::new(
        vec![device_view(
            UiStatus::good("Simulator connected"),
            vec![
                select_connection_complete("Simulator"),
                connect_device_complete_with_actions(
                    browser_worker_metrics(),
                    vec![disconnect_device_action()],
                ),
                stack_section(
                    "connect-lightplayer",
                    "Connect LightPlayer",
                    UiStepState::Active,
                    UiBody::text("Attach Studio to LightPlayer on the connected simulator."),
                    vec![connect_lightplayer_action()],
                ),
            ],
            vec!["[lpa-studio-ux] LightPlayer protocol detached; device session remains open"],
        )],
        vec![UxLogEntry::new(
            UxLogLevel::Info,
            "lpa-studio-ux",
            "LightPlayer protocol detached; device session remains open",
        )],
    )
}

fn provision_ready_view() -> StudioView {
    StudioView::new(
        vec![blank_device_view(
            UiStatus::warning("Ready to flash"),
            UiBody::text("No LightPlayer firmware is running on this ESP32."),
            false,
        )],
        vec![UxLogEntry::new(
            UxLogLevel::Warn,
            "lpa-studio-ux",
            "server protocol is unavailable; firmware flashing is available",
        )],
    )
}

fn browser_serial_blank_firmware_view() -> StudioView {
    StudioView::new(
        vec![blank_device_view(
            UiStatus::warning("Ready to flash"),
            UiBody::Activity(blank_firmware_activity()),
            false,
        )],
        vec![
            UxLogEntry::new(UxLogLevel::Info, "fw-esp32", "ESP-ROM:esp32c6-20220919"),
            UxLogEntry::new(UxLogLevel::Info, "fw-esp32", "invalid header: 0xffffffff"),
            UxLogEntry::new(
                UxLogLevel::Warn,
                "lpa-studio-ux",
                "no LightPlayer firmware detected; firmware flashing is available",
            ),
        ],
    )
}

fn provisioning_view() -> StudioView {
    StudioView::new(
        vec![device_view(
            UiStatus::working("Flashing"),
            vec![
                select_connection_complete("ESP32 over USB"),
                connect_device_complete(esp32_metrics()),
                stack_section(
                    "connect-lightplayer",
                    "Flashing firmware",
                    UiStepState::Active,
                    UiBody::Activity(provisioning_activity()),
                    Vec::new(),
                ),
            ],
            vec![
                "[lpa-link] Connected to ESP32 bootloader",
                "[lpa-link] Writing app image at 0x10000",
                "[lpa-link] Progress 42%",
            ],
        )],
        vec![UxLogEntry::new(
            UxLogLevel::Info,
            "lpa-link",
            "Connected to ESP32 bootloader",
        )],
    )
}

fn provision_failed_view() -> StudioView {
    StudioView::new(
        vec![device_view(
            UiStatus::error("Needs attention"),
            vec![
                select_connection_complete("ESP32 over USB"),
                connect_device_complete_with_actions(esp32_metrics(), device_management_actions()),
                stack_section(
                    "connect-lightplayer",
                    "Flashing firmware",
                    UiStepState::NeedsAttention,
                    UiBody::Issue(
                        UxIssue::new("firmware flashing failed").with_detail(
                            "Check the cable, boot mode, and browser serial permission.",
                        ),
                    ),
                    Vec::new(),
                ),
            ],
            vec![
                "[lpa-link] Connected to ESP32 bootloader",
                "[lpa-link] failed to write firmware image",
            ],
        )],
        vec![UxLogEntry::new(
            UxLogLevel::Error,
            "lpa-link",
            "failed to write firmware image",
        )],
    )
}

fn resetting_to_blank_view() -> StudioView {
    StudioView::new(
        vec![device_view(
            UiStatus::working("Resetting"),
            vec![
                select_connection_complete("ESP32 over USB"),
                connect_device_complete(esp32_metrics()),
                stack_section(
                    "connect-lightplayer",
                    "Wiping device",
                    UiStepState::Active,
                    UiBody::Activity(reset_activity()),
                    Vec::new(),
                ),
            ],
            vec![
                "[lpa-link] Connected to ESP32 bootloader",
                "[lpa-link] Erasing device flash",
            ],
        )],
        vec![UxLogEntry::new(
            UxLogLevel::Info,
            "lpa-link",
            "Erasing device flash",
        )],
    )
}

fn reset_complete_view() -> StudioView {
    StudioView::new(
        vec![blank_device_view(
            UiStatus::warning("Blank ESP32"),
            UiBody::text("The device has been erased and can be flashed again."),
            true,
        )],
        vec![UxLogEntry::new(
            UxLogLevel::Info,
            "lpa-link",
            "Chip erase completed successfully",
        )],
    )
}

fn error_view() -> StudioView {
    picker_issue_view(
        "browser worker boot timed out",
        "browser worker boot timed out",
    )
}

fn picker_issue_view(message: &'static str, log_message: &'static str) -> StudioView {
    StudioView::new(
        vec![device_view(
            UiStatus::error("Needs attention"),
            vec![stack_section(
                "select-connection",
                "Select connection",
                UiStepState::NeedsAttention,
                UiBody::Issue(UxIssue::new(message)),
                start_actions(),
            )],
            Vec::new(),
        )],
        vec![studio_log(UxLogLevel::Error, log_message)],
    )
}

fn idle_device_view() -> UiPaneView {
    device_view(
        UiStatus::neutral("Choose connection"),
        vec![stack_section(
            "select-connection",
            "Select connection",
            UiStepState::Active,
            UiBody::text("Choose how Studio should connect."),
            start_actions(),
        )],
        Vec::new(),
    )
}

fn endpoint_device_view() -> UiPaneView {
    device_view(
        UiStatus::working("Connecting"),
        vec![
            select_connection_complete("Simulator"),
            stack_section(
                "connect-device",
                "Connect device",
                UiStepState::Active,
                UiBody::text("Choose the device endpoint to open."),
                vec![
                    device_action(DeviceOp::ConnectEndpoint {
                        provider_id: LinkProviderKind::BrowserWorker,
                        endpoint_id: LinkEndpointId::new("browser-worker-worker-1"),
                    })
                    .with_label("Open browser simulator")
                    .with_summary("Open the browser-local firmware runtime."),
                ],
            ),
        ],
        vec!["[lpa-link] Browser worker provider selected"],
    )
}

fn starting_device_view() -> UiPaneView {
    device_view(
        UiStatus::working("Connecting"),
        vec![
            select_connection_complete("Simulator"),
            connect_device_complete(browser_worker_metrics()),
            stack_section(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Active,
                UiBody::Progress(ProgressState::new("Opening server protocol")),
                Vec::new(),
            ),
        ],
        vec![
            "[lpa-link] browser worker session created",
            "[fw-browser] booting firmware runtime",
        ],
    )
}

fn simulator_ready_device_view() -> UiPaneView {
    device_view(
        UiStatus::good("LightPlayer ready"),
        vec![
            select_connection_complete("Simulator"),
            connect_device_complete(browser_worker_metrics()),
            stack_section(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Complete,
                UiBody::Metrics(vec![UiMetric::new(
                    "Protocol",
                    "fw-browser-post-message-v1",
                )]),
                vec![disconnect_lightplayer_action()],
            ),
            stack_section(
                "open-project",
                "Open project",
                UiStepState::Complete,
                UiBody::text("Project controls are available in the Project pane."),
                Vec::new(),
            ),
        ],
        vec![
            "[fw-browser] ready",
            "[lp-server] loaded project studio-demo",
            "[fw-browser] heartbeat frame=42",
        ],
    )
}

fn device_project_empty_view() -> UiPaneView {
    device_view(
        UiStatus::good("LightPlayer ready"),
        vec![
            select_connection_complete("ESP32 over USB"),
            connect_device_complete(esp32_metrics()),
            stack_section(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Complete,
                UiBody::Metrics(vec![UiMetric::new("Protocol", "lp-serial-json-lines-v1")]),
                vec![disconnect_lightplayer_action()],
            ),
            stack_section(
                "open-project",
                "Open project",
                UiStepState::Active,
                UiBody::text("Connect to a running project or load the demo project."),
                vec![
                    project_action(ProjectOp::ConnectRunningProject),
                    project_action(ProjectOp::LoadDemoProject),
                ],
            ),
        ],
        vec![
            "[fw-esp32] LightPlayer protocol ready",
            "[lp-server] loaded projects: 0",
        ],
    )
}

fn device_project_selection_view() -> UiPaneView {
    device_view(
        UiStatus::good("LightPlayer ready"),
        vec![
            select_connection_complete("ESP32 over USB"),
            connect_device_complete(esp32_metrics()),
            stack_section(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Complete,
                UiBody::Metrics(vec![UiMetric::new("Protocol", "lp-serial-json-lines-v1")]),
                vec![disconnect_lightplayer_action()],
            ),
            stack_section(
                "open-project",
                "Open project",
                UiStepState::Active,
                UiBody::text("2 projects are running. Choose one to open."),
                vec![
                    project_action(ProjectOp::ConnectLoadedProject { handle_id: 1 })
                        .with_label("Connect /projects/ambient")
                        .with_summary("Attach to running project handle 1."),
                    project_action(ProjectOp::ConnectLoadedProject { handle_id: 2 })
                        .with_label("Connect /projects/palette-test")
                        .with_summary("Attach to running project handle 2."),
                ],
            ),
        ],
        vec![
            "[fw-esp32] LightPlayer protocol ready",
            "[lp-server] loaded projects: 2",
        ],
    )
}

fn blank_device_view(status: UiStatus, body: UiBody, after_reset: bool) -> UiPaneView {
    let detail = if after_reset {
        vec![
            "[lpa-link] Chip erase completed successfully",
            "[fw-esp32] invalid header: 0xffffffff",
        ]
    } else {
        vec![
            "[esp32-reset] Hard resetting via RTS pin...",
            "[fw-esp32] ESP-ROM:esp32c6-20220919",
            "[fw-esp32] invalid header: 0xffffffff",
        ]
    };
    device_view(
        status,
        vec![
            select_connection_complete("ESP32 over USB"),
            connect_device_complete_with_actions(esp32_metrics(), device_management_actions()),
            stack_section(
                "connect-lightplayer",
                "LightPlayer unavailable",
                UiStepState::Active,
                body,
                Vec::new(),
            ),
        ],
        detail,
    )
}

fn blank_firmware_activity() -> UiActivity {
    UiActivity::new("Connecting ESP32 server")
        .with_detail("ESP32 boot output looks like blank or erased flash.")
        .with_steps(vec![
            UiActivityStep::new("serial-access", "Serial access")
                .with_state(UiActivityStepState::Complete)
                .with_detail("Browser serial port is open."),
            UiActivityStep::new("reset-device", "Reset device")
                .with_state(UiActivityStepState::Complete)
                .with_detail("Device reset was requested before protocol attach."),
            UiActivityStep::new("boot-output", "Boot output")
                .with_state(UiActivityStepState::Complete),
            UiActivityStep::new("server-protocol", "LightPlayer protocol")
                .with_state(UiActivityStepState::Failed),
        ])
}

fn provisioning_activity() -> UiActivity {
    UiActivity::new("Flashing firmware")
        .with_detail("Writing packaged LightPlayer ESP32-C6 firmware.")
        .with_progress(UiProgress::determinate("Writing flash", 42))
        .with_steps(vec![
            UiActivityStep::new("bootloader", "Bootloader")
                .with_state(UiActivityStepState::Complete),
            UiActivityStep::new("erase", "Erase").with_state(UiActivityStepState::Complete),
            UiActivityStep::new("write", "Write firmware").with_state(UiActivityStepState::Active),
            UiActivityStep::new("reboot", "Reboot").with_state(UiActivityStepState::Pending),
        ])
}

fn reset_activity() -> UiActivity {
    UiActivity::new("Wiping device")
        .with_detail("Erasing ESP32 flash through the bootloader.")
        .with_progress(UiProgress::determinate("Erasing flash", 58))
        .with_steps(vec![
            UiActivityStep::new("bootloader", "Bootloader")
                .with_state(UiActivityStepState::Complete),
            UiActivityStep::new("erase", "Erase flash").with_state(UiActivityStepState::Active),
            UiActivityStep::new("blank", "Blank device").with_state(UiActivityStepState::Pending),
        ])
}

fn device_view(
    status: UiStatus,
    sections: Vec<UiStackSection>,
    terminal: Vec<&'static str>,
) -> UiPaneView {
    UiPaneView::new(
        DeviceUx::NODE_ID,
        "Device",
        status,
        UiBody::Stack(Box::new(
            UiStackView::new(sections).with_terminal(
                terminal
                    .into_iter()
                    .map(UiTerminalLine::new)
                    .collect::<Vec<_>>(),
            ),
        )),
        Vec::new(),
    )
}

fn stack_section(
    id: &'static str,
    title: &'static str,
    state: UiStepState,
    body: UiBody,
    actions: Vec<UiAction>,
) -> UiStackSection {
    UiStackSection::new(id, title, state)
        .with_body(body)
        .with_actions(actions)
}

fn select_connection_complete(label: &'static str) -> UiStackSection {
    stack_section(
        "select-connection",
        "Select connection",
        UiStepState::Complete,
        UiBody::text(label),
        Vec::new(),
    )
}

fn connect_device_complete(metrics: Vec<UiMetric>) -> UiStackSection {
    connect_device_complete_with_actions(metrics, Vec::new())
}

fn connect_device_complete_with_actions(
    metrics: Vec<UiMetric>,
    actions: Vec<UiAction>,
) -> UiStackSection {
    stack_section(
        "connect-device",
        "Connect device",
        UiStepState::Complete,
        UiBody::Metrics(metrics),
        actions,
    )
}

fn browser_worker_metrics() -> Vec<UiMetric> {
    vec![
        UiMetric::new("Provider", "Browser worker"),
        UiMetric::new("Endpoint", "browser-worker-worker-1"),
        UiMetric::new("Session", "browser-worker-worker-1:1"),
    ]
}

fn esp32_metrics() -> Vec<UiMetric> {
    vec![
        UiMetric::new("Provider", "Browser serial ESP32"),
        UiMetric::new("Endpoint", "browser-serial-esp32-port-1"),
        UiMetric::new("Session", "browser-serial-esp32-port-1:1"),
    ]
}

fn project_synced_pane_view() -> UiPaneView {
    UiPaneView::new(
        ProjectUx::NODE_ID,
        "Project",
        UiStatus::good("Ready"),
        UiBody::Metrics(project_synced_metrics()),
        project_ready_actions(),
    )
}

fn project_syncing_pane_view() -> UiPaneView {
    UiPaneView::new(
        ProjectUx::NODE_ID,
        "Project",
        UiStatus::working("Syncing"),
        UiBody::Metrics(vec![
            UiMetric::new("Project", "studio-demo"),
            UiMetric::new("Handle", 1),
            UiMetric::new("Sync", "Syncing"),
            UiMetric::new("Shapes", "reading page 2"),
        ]),
        Vec::new(),
    )
}

fn project_sync_failed_pane_view() -> UiPaneView {
    UiPaneView::new(
        ProjectUx::NODE_ID,
        "Project",
        UiStatus::error("Sync issue"),
        UiBody::Metrics(vec![
            UiMetric::new("Project", "studio-demo"),
            UiMetric::new("Handle", 1),
            UiMetric::new("Inventory nodes", 4),
            UiMetric::new("Definitions", 3),
            UiMetric::new("Assets", 1),
            UiMetric::new("Sync", "project sync failed: protocol timeout"),
            UiMetric::new("Revision", 0),
        ]),
        project_ready_actions(),
    )
}

fn project_synced_metrics() -> Vec<UiMetric> {
    vec![
        UiMetric::new("Project", "studio-demo"),
        UiMetric::new("Handle", 1),
        UiMetric::new("Inventory nodes", 4),
        UiMetric::new("Definitions", 3),
        UiMetric::new("Assets", 1),
        UiMetric::new("Sync", "Synced"),
        UiMetric::new("Revision", 42),
        UiMetric::new("Synced nodes", 7),
        UiMetric::new("Root nodes", 1),
        UiMetric::new("Slot roots", 12),
        UiMetric::new("Resources", 3),
        UiMetric::new("Shapes", 18),
        UiMetric::new("Frame", 512),
        UiMetric::new("Runtime buffers", 2),
        UiMetric::new("Memory free", "232 KB"),
    ]
}

fn project_ready_actions() -> Vec<UiAction> {
    vec![
        project_action(ProjectOp::RefreshProject),
        project_action(ProjectOp::DisconnectProject),
    ]
}

fn project_view(state: ProjectState, server_connected: bool) -> UiPaneView {
    let mut project = ProjectUx::new();
    let no_running_project = matches!(state, ProjectState::NotLoaded) && server_connected;
    project.set_state(state);
    if no_running_project {
        project.mark_no_running_project();
    }
    project.view(server_connected)
}

fn project_ready_state() -> ProjectState {
    ProjectState::Ready {
        project_id: "studio-demo".to_string(),
        handle_id: 1,
        inventory: ProjectInventorySummary {
            node_count: 4,
            definition_count: 3,
            asset_count: 1,
        },
    }
}

fn start_actions() -> Vec<UiAction> {
    vec![
        device_action(DeviceOp::OpenProvider {
            provider_id: LinkProviderKind::BrowserWorker,
        })
        .with_label("Start simulator")
        .with_summary("Run LightPlayer locally in a browser worker.")
        .with_short_label("Simulator")
        .with_icon("play"),
        device_action(DeviceOp::OpenProvider {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
        })
        .with_label("Connect ESP32")
        .with_summary("Connect to ESP32 hardware through browser Web Serial.")
        .with_short_label("ESP32")
        .with_icon("usb"),
    ]
}

fn disconnect_device_action() -> UiAction {
    device_action(DeviceOp::DisconnectDevice)
}

fn disconnect_lightplayer_action() -> UiAction {
    device_action(DeviceOp::DisconnectLightPlayer)
}

fn connect_lightplayer_action() -> UiAction {
    device_action(DeviceOp::ConnectLightPlayer)
}

fn device_management_actions() -> Vec<UiAction> {
    vec![
        device_action(DeviceOp::ProvisionFirmware),
        device_action(DeviceOp::ResetToBlank),
        disconnect_device_action(),
    ]
}

fn device_action(op: DeviceOp) -> UiAction {
    UiAction::from_op(UxNodeId::new(DeviceUx::NODE_ID), op)
}

fn project_action(op: ProjectOp) -> UiAction {
    UiAction::from_op(UxNodeId::new(ProjectUx::NODE_ID), op)
}
