//! Studio story fixtures.
//!
//! This module is compiled only for storybook builds. It keeps broad
//! shell/device/project fixture builders in one place while story entrypoints
//! live next to their component families.

use crate::app::{PaneFrame, StudioShell};
use crate::base::{FieldRow, TabItem, Tabs};
use crate::core::MetricGrid;
use dioxus::prelude::*;
use lpa_studio_core::core::view::activity_view::{UiActivityStep, UiActivityStepState};
use lpa_studio_core::core::view::steps_view::{UiStepState, UiStepView};
use lpa_studio_core::{
    ControllerId, DeviceController, DeviceOp, LinkEndpointId, LinkProviderKind, ProjectController,
    ProjectEditorOp, ProjectEditorView, ProjectInventorySummary, ProjectNodeStatusTone,
    ProjectNodeStatusView, ProjectNodeTreeItem, ProjectNodeTreeView, ProjectOp,
    ProjectRuntimeSummary, ProjectState, ProjectSyncPhase, ProjectSyncSummary, UiAction,
    UiActivityView, UiAssetEditorKind, UiBindingEndpoint, UiConfigSlot, UiConsoleView, UiIssue,
    UiLogEntry, UiLogLevel, UiLogOrigin, UiLogSource, UiMetric, UiNodeChild, UiNodeHeader,
    UiNodeSection, UiNodeTab, UiNodeView, UiPaneView, UiProducedProduct, UiProducedValue,
    UiProgress, UiSlotAsset, UiSlotSourceState, UiSlotValue, UiStatus, UiStepsView, UiStudioView,
    UiTerminalLine, UiViewContent,
};

/// Timestamp shared by every story log fixture, so stories stay
/// deterministic. P2 renders the timestamp column; until then it is unused by
/// the row rendering.
pub(crate) const STORY_LOG_TIMESTAMP: f64 = 1_720_000_000.0;

/// A studio view whose console shows exactly `logs`. Fixtures assign the
/// entries directly (bypassing the display filter) so story rendering matches
/// the retired `logs` field byte-for-byte, debug entries included.
fn story_view(panes: Vec<UiPaneView>, logs: Vec<UiLogEntry>) -> UiStudioView {
    let mut console = UiConsoleView::empty();
    console.entries = logs;
    UiStudioView::new(panes, console)
}

/// A console view with explicit toolbar state for RuntimeLog stories:
/// `entries` are shown as-is; `hidden_count`, `min_level`, and the disabled
/// origins drive the toolbar affordances.
pub(crate) fn story_console(
    entries: Vec<UiLogEntry>,
    hidden_count: usize,
    min_level: UiLogLevel,
    disabled_origins: &[UiLogOrigin],
) -> UiConsoleView {
    let mut console = UiConsoleView::empty();
    console.entries = entries;
    console.hidden_count = hidden_count;
    console.min_level = min_level;
    console.origins = UiLogOrigin::ALL
        .into_iter()
        .map(|origin| (origin, !disabled_origins.contains(&origin)))
        .collect();
    console
}

pub(crate) fn shell_story(
    mut view: UiStudioView,
    running: bool,
    story_logs: Vec<UiLogEntry>,
) -> Element {
    view.console.entries.extend(story_logs);
    rsx! {
        StudioShell {
            view,
            running,
            on_action: move |_| {},
            on_console: move |_| {},
        }
    }
}

pub(crate) fn editor_primitives_story() -> Element {
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

pub(crate) fn editor_shell_story() -> Element {
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
pub(crate) fn NodeTreePane() -> Element {
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
pub(crate) fn NodeWorkspacePane() -> Element {
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
pub(crate) fn DeviceSidePane() -> Element {
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
pub(crate) fn ConsoleSidePane() -> Element {
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
pub(crate) fn SecondaryTabsPane() -> Element {
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
pub(crate) fn MobileEditorTabsPane() -> Element {
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

pub(crate) fn studio_log(level: UiLogLevel, message: impl Into<String>) -> UiLogEntry {
    UiLogEntry::new(STORY_LOG_TIMESTAMP, level, UiLogOrigin::Studio, message)
}

pub(crate) fn idle_view() -> UiStudioView {
    story_view(vec![idle_device_view()], Vec::new())
}

pub(crate) fn browser_serial_canceled_view() -> UiStudioView {
    story_view(
        vec![idle_device_view()],
        vec![studio_log(UiLogLevel::Info, "Port selection canceled")],
    )
}

pub(crate) fn browser_serial_open_failed_view() -> UiStudioView {
    picker_issue_view(
        "Failed to open serial port.",
        "Failed to execute 'open' on 'SerialPort': Failed to open serial port.",
    )
}

pub(crate) fn endpoint_view() -> UiStudioView {
    story_view(vec![endpoint_device_view()], Vec::new())
}

pub(crate) fn starting_view() -> UiStudioView {
    story_view(
        vec![starting_device_view()],
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Info,
            UiLogOrigin::Link,
            "browser worker session created",
        )],
    )
}

pub(crate) fn simulator_ready_view() -> UiStudioView {
    story_view(
        vec![project_synced_pane_view(), simulator_ready_device_view()],
        vec![
            UiLogEntry::new(
                STORY_LOG_TIMESTAMP,
                UiLogLevel::Info,
                UiLogSource::with_detail(UiLogOrigin::Device, "fw-browser"),
                "ready",
            ),
            UiLogEntry::new(
                STORY_LOG_TIMESTAMP,
                UiLogLevel::Info,
                UiLogOrigin::Link,
                "browser worker session owns Worker lifecycle in lpa-link",
            ),
            UiLogEntry::new(
                STORY_LOG_TIMESTAMP,
                UiLogLevel::Info,
                UiLogSource::with_detail(UiLogOrigin::Device, "fw-browser"),
                "project loaded",
            ),
        ],
    )
}

pub(crate) fn project_ready_view() -> UiStudioView {
    story_view(
        vec![project_synced_pane_view(), simulator_ready_device_view()],
        vec![
            UiLogEntry::new(
                STORY_LOG_TIMESTAMP,
                UiLogLevel::Info,
                UiLogSource::with_detail(UiLogOrigin::Device, "fw-browser"),
                "project loaded",
            ),
            UiLogEntry::new(
                STORY_LOG_TIMESTAMP,
                UiLogLevel::Debug,
                UiLogOrigin::Server,
                "heartbeat frame=42 uptime_ms=700",
            ),
        ],
    )
}

pub(crate) fn project_syncing_view() -> UiStudioView {
    story_view(
        vec![project_syncing_pane_view(), simulator_ready_device_view()],
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Info,
            UiLogOrigin::Studio,
            "syncing project",
        )],
    )
}

pub(crate) fn project_sync_failed_view() -> UiStudioView {
    story_view(
        vec![
            project_sync_failed_pane_view(),
            simulator_ready_device_view(),
        ],
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Error,
            UiLogOrigin::Studio,
            "project sync failed: protocol timeout",
        )],
    )
}

pub(crate) fn lightplayer_disconnected_view() -> UiStudioView {
    story_view(
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
                    UiViewContent::text("Attach Studio to LightPlayer on the connected simulator."),
                    vec![connect_lightplayer_action()],
                ),
            ],
            vec!["[lpa-studio-core] LightPlayer protocol detached; device session remains open"],
        )],
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Info,
            UiLogOrigin::Studio,
            "LightPlayer protocol detached; device session remains open",
        )],
    )
}

pub(crate) fn open_for_flashing_view() -> UiStudioView {
    story_view(
        vec![device_view(
            UiStatus::good("ESP32 over USB"),
            vec![
                select_connection_complete("ESP32 over USB"),
                connect_device_complete_with_actions(esp32_metrics(), device_management_actions()),
                stack_section(
                    "connect-lightplayer",
                    "Connect LightPlayer",
                    UiStepState::Active,
                    UiViewContent::text(
                        "Device is open for recovery. Flash firmware or connect LightPlayer.",
                    ),
                    vec![connect_lightplayer_action()],
                ),
            ],
            vec!["[lpa-link] ESP32 opened for flashing"],
        )],
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Info,
            UiLogOrigin::Studio,
            "Device opened for flashing",
        )],
    )
}

pub(crate) fn provision_ready_view() -> UiStudioView {
    story_view(
        vec![blank_device_view(
            UiStatus::warning("Ready to flash"),
            UiViewContent::text("No LightPlayer firmware is running on this ESP32."),
            false,
        )],
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Warn,
            UiLogOrigin::Studio,
            "server protocol is unavailable; firmware flashing is available",
        )],
    )
}

pub(crate) fn browser_serial_blank_firmware_view() -> UiStudioView {
    story_view(
        vec![blank_device_view(
            UiStatus::warning("Ready to flash"),
            UiViewContent::Activity(blank_firmware_activity()),
            false,
        )],
        vec![
            UiLogEntry::new(
                STORY_LOG_TIMESTAMP,
                UiLogLevel::Info,
                UiLogSource::with_detail(UiLogOrigin::Device, "fw-esp32"),
                "ESP-ROM:esp32c6-20220919",
            ),
            UiLogEntry::new(
                STORY_LOG_TIMESTAMP,
                UiLogLevel::Info,
                UiLogSource::with_detail(UiLogOrigin::Device, "fw-esp32"),
                "invalid header: 0xffffffff",
            ),
            UiLogEntry::new(
                STORY_LOG_TIMESTAMP,
                UiLogLevel::Warn,
                UiLogOrigin::Studio,
                "no LightPlayer firmware detected; firmware flashing is available",
            ),
        ],
    )
}

pub(crate) fn provisioning_view() -> UiStudioView {
    story_view(
        vec![device_view(
            UiStatus::working("Flashing"),
            vec![
                select_connection_complete("ESP32 over USB"),
                connect_device_complete(esp32_metrics()),
                stack_section(
                    "connect-lightplayer",
                    "Flashing firmware",
                    UiStepState::Active,
                    UiViewContent::Activity(provisioning_activity()),
                    Vec::new(),
                ),
            ],
            vec![
                "[lpa-link] Connected to ESP32 bootloader",
                "[lpa-link] Writing app image at 0x10000",
                "[lpa-link] Progress 42%",
            ],
        )],
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Info,
            UiLogOrigin::Link,
            "Connected to ESP32 bootloader",
        )],
    )
}

pub(crate) fn provision_failed_view() -> UiStudioView {
    story_view(
        vec![device_view(
            UiStatus::error("Needs attention"),
            vec![
                select_connection_complete("ESP32 over USB"),
                connect_device_complete_with_actions(esp32_metrics(), device_management_actions()),
                stack_section(
                    "connect-lightplayer",
                    "Flashing firmware",
                    UiStepState::NeedsAttention,
                    UiViewContent::Issue(
                        UiIssue::new("firmware flashing failed").with_detail(
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
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Error,
            UiLogOrigin::Link,
            "failed to write firmware image",
        )],
    )
}

pub(crate) fn resetting_to_blank_view() -> UiStudioView {
    story_view(
        vec![device_view(
            UiStatus::working("Resetting"),
            vec![
                select_connection_complete("ESP32 over USB"),
                connect_device_complete(esp32_metrics()),
                stack_section(
                    "connect-lightplayer",
                    "Wiping device",
                    UiStepState::Active,
                    UiViewContent::Activity(reset_activity()),
                    Vec::new(),
                ),
            ],
            vec![
                "[lpa-link] Connected to ESP32 bootloader",
                "[lpa-link] Erasing device flash",
            ],
        )],
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Info,
            UiLogOrigin::Link,
            "Erasing device flash",
        )],
    )
}

pub(crate) fn reset_complete_view() -> UiStudioView {
    story_view(
        vec![blank_device_view(
            UiStatus::warning("Blank ESP32"),
            UiViewContent::text("The device has been erased and can be flashed again."),
            true,
        )],
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Info,
            UiLogOrigin::Link,
            "Chip erase completed successfully",
        )],
    )
}

pub(crate) fn error_view() -> UiStudioView {
    picker_issue_view(
        "browser worker boot timed out",
        "browser worker boot timed out",
    )
}

pub(crate) fn picker_issue_view(message: &'static str, log_message: &'static str) -> UiStudioView {
    story_view(
        vec![device_view(
            UiStatus::error("Needs attention"),
            vec![stack_section(
                "select-connection",
                "Select connection",
                UiStepState::NeedsAttention,
                UiViewContent::Issue(UiIssue::new(message)),
                start_actions(),
            )],
            Vec::new(),
        )],
        vec![studio_log(UiLogLevel::Error, log_message)],
    )
}

pub(crate) fn idle_device_view() -> UiPaneView {
    device_view(
        UiStatus::neutral("Choose connection"),
        vec![stack_section(
            "select-connection",
            "Select connection",
            UiStepState::Active,
            UiViewContent::text("Choose how Studio should connect."),
            start_actions(),
        )],
        Vec::new(),
    )
}

pub(crate) fn endpoint_device_view() -> UiPaneView {
    device_view(
        UiStatus::working("Connecting"),
        vec![
            select_connection_complete("Simulator"),
            stack_section(
                "connect-device",
                "Connect device",
                UiStepState::Active,
                UiViewContent::text("Choose the device endpoint to open."),
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

pub(crate) fn starting_device_view() -> UiPaneView {
    device_view(
        UiStatus::working("Connecting"),
        vec![
            select_connection_complete("Simulator"),
            connect_device_complete(browser_worker_metrics()),
            stack_section(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Active,
                UiViewContent::Progress(UiProgress::indeterminate("Opening server protocol")),
                Vec::new(),
            ),
        ],
        vec![
            "[lpa-link] browser worker session created",
            "[fw-browser] booting firmware runtime",
        ],
    )
}

pub(crate) fn simulator_ready_device_view() -> UiPaneView {
    device_view(
        UiStatus::good("LightPlayer ready"),
        vec![
            select_connection_complete("Simulator"),
            connect_device_complete(browser_worker_metrics()),
            stack_section(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Complete,
                UiViewContent::Metrics(vec![UiMetric::new(
                    "Protocol",
                    "fw-browser-post-message-v1",
                )]),
                vec![disconnect_device_action(), disconnect_lightplayer_action()],
            ),
            stack_section(
                "open-project",
                "Open project",
                UiStepState::Complete,
                UiViewContent::text("Project controls are available in the Project pane."),
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

pub(crate) fn device_project_empty_view() -> UiPaneView {
    device_view(
        UiStatus::good("LightPlayer ready"),
        vec![
            select_connection_complete("ESP32 over USB"),
            connect_device_complete(esp32_metrics()),
            stack_section(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Complete,
                UiViewContent::Metrics(vec![UiMetric::new("Protocol", "lp-serial-json-lines-v1")]),
                connected_esp32_recovery_actions(),
            ),
            stack_section(
                "open-project",
                "Open project",
                UiStepState::Active,
                UiViewContent::text("Connect to a running project or load the demo project."),
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

pub(crate) fn device_project_selection_view() -> UiPaneView {
    device_view(
        UiStatus::good("LightPlayer ready"),
        vec![
            select_connection_complete("ESP32 over USB"),
            connect_device_complete(esp32_metrics()),
            stack_section(
                "connect-lightplayer",
                "Connect LightPlayer",
                UiStepState::Complete,
                UiViewContent::Metrics(vec![UiMetric::new("Protocol", "lp-serial-json-lines-v1")]),
                connected_esp32_recovery_actions(),
            ),
            stack_section(
                "open-project",
                "Open project",
                UiStepState::Active,
                UiViewContent::text("2 projects are running. Choose one to open."),
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

pub(crate) fn blank_device_view(
    status: UiStatus,
    body: UiViewContent,
    after_reset: bool,
) -> UiPaneView {
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

pub(crate) fn blank_firmware_activity() -> UiActivityView {
    UiActivityView::new("Connecting ESP32 server")
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

pub(crate) fn provisioning_activity() -> UiActivityView {
    UiActivityView::new("Flashing firmware")
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

pub(crate) fn reset_activity() -> UiActivityView {
    UiActivityView::new("Wiping device")
        .with_detail("Erasing ESP32 flash through the bootloader.")
        .with_progress(UiProgress::determinate("Erasing flash", 58))
        .with_steps(vec![
            UiActivityStep::new("bootloader", "Bootloader")
                .with_state(UiActivityStepState::Complete),
            UiActivityStep::new("erase", "Erase flash").with_state(UiActivityStepState::Active),
            UiActivityStep::new("blank", "Blank device").with_state(UiActivityStepState::Pending),
        ])
}

pub(crate) fn device_view(
    status: UiStatus,
    sections: Vec<UiStepView>,
    terminal: Vec<&'static str>,
) -> UiPaneView {
    UiPaneView::new(
        DeviceController::NODE_ID,
        "Device",
        status,
        UiViewContent::Stack(Box::new(
            UiStepsView::new(sections).with_terminal(
                terminal
                    .into_iter()
                    .map(UiTerminalLine::new)
                    .collect::<Vec<_>>(),
            ),
        )),
        Vec::new(),
    )
}

pub(crate) fn stack_section(
    id: &'static str,
    title: &'static str,
    state: UiStepState,
    body: UiViewContent,
    actions: Vec<UiAction>,
) -> UiStepView {
    UiStepView::new(id, title, state)
        .with_body(body)
        .with_actions(actions)
}

pub(crate) fn select_connection_complete(label: &'static str) -> UiStepView {
    stack_section(
        "select-connection",
        "Select connection",
        UiStepState::Complete,
        UiViewContent::text(label),
        Vec::new(),
    )
}

pub(crate) fn connect_device_complete(metrics: Vec<UiMetric>) -> UiStepView {
    connect_device_complete_with_actions(metrics, Vec::new())
}

pub(crate) fn connect_device_complete_with_actions(
    metrics: Vec<UiMetric>,
    actions: Vec<UiAction>,
) -> UiStepView {
    stack_section(
        "connect-device",
        "Connect device",
        UiStepState::Complete,
        UiViewContent::Metrics(metrics),
        actions,
    )
}

pub(crate) fn browser_worker_metrics() -> Vec<UiMetric> {
    vec![
        UiMetric::new("Provider", "Browser worker"),
        UiMetric::new("Endpoint", "browser-worker-worker-1"),
        UiMetric::new("Session", "browser-worker-worker-1:1"),
    ]
}

pub(crate) fn esp32_metrics() -> Vec<UiMetric> {
    vec![
        UiMetric::new("Provider", "Browser serial ESP32"),
        UiMetric::new("Endpoint", "browser-serial-esp32-port-1"),
        UiMetric::new("Session", "browser-serial-esp32-port-1:1"),
    ]
}

pub(crate) fn project_synced_pane_view() -> UiPaneView {
    UiPaneView::new(
        ProjectController::NODE_ID,
        "Project",
        UiStatus::good("Ready"),
        UiViewContent::ProjectEditor(Box::new(project_editor_fixture(ProjectSyncPhase::Ready))),
        // P6 sidebar tidy: a ready project produces no pane-level actions.
        Vec::new(),
    )
}

pub(crate) fn project_syncing_pane_view() -> UiPaneView {
    UiPaneView::new(
        ProjectController::NODE_ID,
        "Project",
        UiStatus::working("Syncing"),
        UiViewContent::ProjectEditor(Box::new(project_editor_empty_fixture(
            ProjectSyncPhase::SyncingProject,
        ))),
        Vec::new(),
    )
}

pub(crate) fn project_sync_failed_pane_view() -> UiPaneView {
    UiPaneView::new(
        ProjectController::NODE_ID,
        "Project",
        UiStatus::error("Sync issue"),
        UiViewContent::ProjectEditor(Box::new(project_editor_empty_fixture(
            ProjectSyncPhase::Failed,
        ))),
        // P6 sidebar tidy: a ready project produces no pane-level actions.
        Vec::new(),
    )
}

pub(crate) fn project_editor_fixture(phase: ProjectSyncPhase) -> ProjectEditorView {
    let running = story_node_status("Running", ProjectNodeStatusTone::Good);
    let warning = ProjectNodeStatusView::new(
        "Warning",
        Some("using fallback palette".to_string()),
        ProjectNodeStatusTone::Warning,
    );
    let project = tree_item(
        1,
        "/demo.project",
        "Demo",
        "Project",
        running.clone(),
        false,
        vec![
            tree_item(
                2,
                "/demo.project/clock.clock",
                "Clock",
                "Clock",
                running.clone(),
                false,
                Vec::new(),
            ),
            tree_item(
                3,
                "/demo.project/orbit.shader",
                "Orbit shader",
                "Shader",
                running.clone(),
                true,
                Vec::new(),
            ),
            tree_item(
                4,
                "/demo.project/palette.visual",
                "Sunrise palette",
                "Visual",
                warning.clone(),
                false,
                Vec::new(),
            ),
            tree_item(
                5,
                "/demo.project/output.output",
                "Output",
                "Output",
                running.clone(),
                false,
                Vec::new(),
            ),
        ],
    );
    let summary = project_editor_summary(phase);
    ProjectEditorView::new(
        "studio-demo",
        1,
        summary,
        project_synced_metrics(),
        ProjectNodeTreeView::new(vec![project], 5),
        project_workspace_nodes(),
    )
    .with_project_name("Demo")
    .with_root_slots(project_root_slots())
}

pub(crate) fn project_editor_empty_fixture(phase: ProjectSyncPhase) -> ProjectEditorView {
    ProjectEditorView::new(
        "studio-demo",
        1,
        project_editor_summary(phase),
        vec![
            UiMetric::new("Project", "studio-demo"),
            UiMetric::new("Handle", 1),
            UiMetric::new("Revision", 0),
            UiMetric::new("Sync", sync_story_label(phase)),
        ],
        ProjectNodeTreeView::new(Vec::new(), 0),
        Vec::new(),
    )
}

pub(crate) fn project_editor_summary(phase: ProjectSyncPhase) -> ProjectSyncSummary {
    ProjectSyncSummary {
        phase,
        revision: 42,
        overlay_revision: 7,
        node_count: 5,
        root_node_count: 1,
        slot_root_count: 10,
        resource_count: 2,
        shape_count: 18,
        shapes_complete: true,
        runtime: Some(ProjectRuntimeSummary {
            frame_num: 512,
            frame_delta_ms: 16,
            runtime_buffer_count: 2,
            free_bytes: Some(232 * 1024),
            used_bytes: Some(60 * 1024),
            total_bytes: Some(292 * 1024),
        }),
        issue: (phase == ProjectSyncPhase::Failed).then(|| UiIssue::new("protocol timeout")),
    }
}

pub(crate) fn tree_item(
    runtime_id: u32,
    path: &str,
    label: &str,
    kind: &str,
    status: ProjectNodeStatusView,
    focused: bool,
    children: Vec<ProjectNodeTreeItem>,
) -> ProjectNodeTreeItem {
    ProjectNodeTreeItem::new(
        path,
        label,
        kind,
        status,
        focused,
        project_focus_action(runtime_id, path, label),
        children,
    )
}

pub(crate) fn project_focus_action(runtime_id: u32, path: &str, label: &str) -> UiAction {
    UiAction::from_op(
        ControllerId::new(format!("studio|project|node|nid|{runtime_id}|path|{path}")),
        ProjectEditorOp::Focus,
    )
    .with_label(format!("Focus {label}"))
}

/// Flat-root workspace nodes (P6): the project root renders no card — its
/// child panes are the top-level entries; the root's own slots live in
/// [`project_root_slots`].
pub(crate) fn project_workspace_nodes() -> Vec<UiNodeView> {
    vec![
        workspace_node(clock_node_child()),
        workspace_node(orbit_shader_child()),
        workspace_node(palette_node_child()),
        workspace_node(output_node_child()),
    ]
}

/// The project root's own config rows for the project popup's settings
/// section: `name` editable, `format`/`nodes` read-only (Q3 policy).
pub(crate) fn project_root_slots() -> Vec<UiConfigSlot> {
    use lpa_studio_core::UiSlotFieldState;
    vec![
        UiConfigSlot::value("name", "Name", UiSlotValue::string("Demo")),
        UiConfigSlot::value("format", "Format", UiSlotValue::u32(1))
            .with_state(UiSlotFieldState::readonly()),
        UiConfigSlot::record(
            "nodes",
            "Nodes",
            vec![
                UiConfigSlot::value("clock", "clock", UiSlotValue::string("./clock.json"))
                    .with_state(UiSlotFieldState::readonly()),
                UiConfigSlot::value("orbit", "orbit", UiSlotValue::string("./orbit.json"))
                    .with_state(UiSlotFieldState::readonly()),
            ],
        )
        .with_detail("2 nodes")
        .with_state(UiSlotFieldState::readonly()),
    ]
}

/// One top-level workspace pane built from a child fixture (the same
/// projection `NodeChildren` applies when a child renders as a nested pane).
fn workspace_node(child: UiNodeChild) -> UiNodeView {
    let mut header = UiNodeHeader::new(
        child.label.clone(),
        child.kind.clone(),
        child.detail.clone(),
    )
    .with_status(child.status.clone())
    .with_dirty(child.dirty);
    if let Some(summary) = child.summary {
        header = header.with_summary(summary);
    }
    let mut view = UiNodeView::new(header, vec![UiNodeTab::main(child.sections)])
        .with_node_id(child.detail.clone())
        .with_children(child.children);
    view.focused = child.focused || child.active;
    view.action = child.action;
    view
}

fn clock_node_child() -> UiNodeChild {
    node_child(
        "Clock",
        "Clock",
        "/demo.project/clock.clock",
        UiStatus::good("Running"),
    )
    .with_sections(vec![
        UiNodeSection::ProducedProducts(vec![
            UiProducedProduct::control("time").with_detail("1 channel"),
        ]),
        UiNodeSection::ProducedValues(vec![
            UiProducedValue::new("Frame", "512").with_detail("rev 42"),
            UiProducedValue::new("Time", "3.333").with_detail("s"),
        ]),
        UiNodeSection::ConfigSlots(vec![UiConfigSlot::value(
            "tempo",
            "Tempo",
            UiSlotValue::f32(120.0),
        )]),
    ])
}

fn orbit_shader_child() -> UiNodeChild {
    node_child(
        "Orbit shader",
        "Shader",
        "/demo.project/orbit.shader",
        UiStatus::good("Running"),
    )
    .active("focused")
    .with_sections(vec![
        UiNodeSection::ProducedProducts(vec![
            UiProducedProduct::visual("output").with_detail("32 x 32"),
        ]),
        UiNodeSection::AssetSlots(vec![
            UiConfigSlot::asset(
                "shader_source",
                "Shader source",
                UiSlotAsset::new("assets/shaders/orbit.glsl", UiAssetEditorKind::Glsl)
                    .with_content(
                        "void mainImage(out vec4 color, in vec2 uv) {\n    color = vec4(uv, 0.4 + 0.4 * sin(iTime), 1.0);\n}",
                    ),
            )
            .with_detail("glsl, rev 42"),
        ]),
        UiNodeSection::ConfigSlots(vec![
            UiConfigSlot::value("time", "Time", UiSlotValue::f32(3.333).with_detail("s"))
                .with_source(UiSlotSourceState::Bound(UiBindingEndpoint::new(
                    "clock#time.seconds",
                ))),
            UiConfigSlot::record(
                "parameters",
                "Parameters",
                vec![
                    UiConfigSlot::value("brightness", "Brightness", UiSlotValue::f32(0.72)),
                    UiConfigSlot::value("speed", "Speed", UiSlotValue::f32(1.5)),
                    UiConfigSlot::value("center", "Center", UiSlotValue::vec2([0.5, 0.5])),
                ],
            )
            .with_detail("3 fields"),
        ]),
    ])
}

fn palette_node_child() -> UiNodeChild {
    node_child(
        "Sunrise palette",
        "Visual",
        "/demo.project/palette.visual",
        UiStatus::warning("Warning"),
    )
    .with_sections(vec![
        UiNodeSection::ProducedProducts(vec![
            UiProducedProduct::visual("output").with_detail("32 x 32"),
        ]),
        UiNodeSection::ConfigSlots(vec![
            UiConfigSlot::record(
                "colors",
                "Colors",
                vec![
                    UiConfigSlot::value("primary", "Primary", UiSlotValue::vec3([1.0, 0.45, 0.18])),
                    UiConfigSlot::value(
                        "secondary",
                        "Secondary",
                        UiSlotValue::vec3([0.08, 0.18, 0.42]),
                    ),
                    UiConfigSlot::value("accent", "Accent", UiSlotValue::vec3([0.95, 0.86, 0.34])),
                ],
            )
            .with_detail("fallback palette"),
        ]),
    ])
}

fn output_node_child() -> UiNodeChild {
    node_child(
        "Output",
        "Output",
        "/demo.project/output.output",
        UiStatus::good("Running"),
    )
    .with_sections(vec![UiNodeSection::ConfigSlots(vec![
        UiConfigSlot::value("input", "Input", UiSlotValue::string("orbit#output")).with_source(
            UiSlotSourceState::Bound(UiBindingEndpoint::new("orbit#visual.output")),
        ),
        UiConfigSlot::value(
            "endpoint",
            "Endpoint",
            UiSlotValue::string("ws281x:rmt:D10"),
        ),
        UiConfigSlot::value("samples", "Samples", UiSlotValue::u32(241)),
    ])])
}

fn node_child(label: &str, kind: &str, detail: &str, status: UiStatus) -> UiNodeChild {
    let mut child = UiNodeChild::new(label, kind, detail);
    child.status = status;
    child
}

pub(crate) fn story_node_status(label: &str, tone: ProjectNodeStatusTone) -> ProjectNodeStatusView {
    ProjectNodeStatusView::new(label, None, tone)
}

pub(crate) fn sync_story_label(phase: ProjectSyncPhase) -> &'static str {
    match phase {
        ProjectSyncPhase::Empty => "Not synced",
        ProjectSyncPhase::SyncingProject => "Syncing",
        ProjectSyncPhase::Ready => "Synced",
        ProjectSyncPhase::Failed => "Needs attention",
    }
}

pub(crate) fn project_synced_metrics() -> Vec<UiMetric> {
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

pub(crate) fn project_view(state: ProjectState, server_connected: bool) -> UiPaneView {
    let mut project = ProjectController::new();
    let no_running_project = matches!(state, ProjectState::NotLoaded) && server_connected;
    project.set_state(state);
    if no_running_project {
        project.mark_no_running_project();
    }
    project.view(server_connected)
}

pub(crate) fn project_ready_state() -> ProjectState {
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

pub(crate) fn start_actions() -> Vec<UiAction> {
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
        device_action(DeviceOp::OpenProviderForRecovery {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
        })
        .with_label("Open for flashing")
        .with_summary("Open the ESP32 connection without attaching LightPlayer.")
        .with_short_label("Flash")
        .with_icon("usb"),
    ]
}

pub(crate) fn disconnect_device_action() -> UiAction {
    device_action(DeviceOp::DisconnectDevice)
}

pub(crate) fn disconnect_lightplayer_action() -> UiAction {
    device_action(DeviceOp::DisconnectLightPlayer)
}

pub(crate) fn connect_lightplayer_action() -> UiAction {
    device_action(DeviceOp::ConnectLightPlayer)
}

pub(crate) fn device_management_actions() -> Vec<UiAction> {
    vec![
        device_action(DeviceOp::ProvisionFirmware),
        device_action(DeviceOp::ResetToBlank),
        disconnect_device_action(),
    ]
}

pub(crate) fn connected_esp32_recovery_actions() -> Vec<UiAction> {
    vec![
        device_action(DeviceOp::ProvisionFirmware),
        device_action(DeviceOp::ResetDevice),
        device_action(DeviceOp::ResetToBlank),
        disconnect_device_action(),
        disconnect_lightplayer_action().with_label("Disconnect LightPlayer"),
    ]
}

pub(crate) fn device_action(op: DeviceOp) -> UiAction {
    UiAction::from_op(ControllerId::new(DeviceController::NODE_ID), op)
}

pub(crate) fn project_action(op: ProjectOp) -> UiAction {
    UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op)
}
