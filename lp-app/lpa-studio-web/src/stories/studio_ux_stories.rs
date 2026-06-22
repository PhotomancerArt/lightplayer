use dioxus::prelude::*;
use lpa_studio_ux::{
    DeviceOp, DeviceUx, LinkEndpointId, LinkProviderKind, ProgressState, ProjectInventorySummary,
    ProjectOp, ProjectState, ProjectUx, StudioView, UiAction, UiActivity, UiActivityStep,
    UiActivityStepState, UiBody, UiMetric, UiPaneView, UiProgress, UiStackSection, UiStackView,
    UiStatus, UiStepState, UiTerminalLine, UxIssue, UxLogEntry, UxLogLevel, UxNodeId,
};

use crate::components::{ActionStrip, StudioShell, UxPane};
use crate::stories::story::StoryDescriptor;

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "studio/actions/provider-actions",
        "Studio UX",
        "Connection actions",
        "Generic action strip for connection choices exposed by Device UX.",
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
        "Demo project loaded and summarized through the UX view.",
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
                section { class: "ux-panel ux-panel-primary",
                    div { class: "ux-panel-heading",
                        p { "Actions" }
                    }
                    ActionStrip {
                        actions: start_actions(),
                        running: false,
                        on_action: move |_| {},
                    }
                }
            });
        }
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
        vec![
            simulator_ready_device_view(),
            project_view(project_ready_state(), true),
        ],
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
        vec![
            simulator_ready_device_view(),
            project_view(project_ready_state(), true),
        ],
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

fn lightplayer_disconnected_view() -> StudioView {
    StudioView::new(
        vec![device_view(
            UiStatus::good("Simulator connected"),
            vec![
                select_connection_complete("Simulator"),
                connect_device_complete(browser_worker_metrics()),
                stack_section(
                    "connect-lightplayer",
                    "Connect LightPlayer",
                    UiStepState::Active,
                    UiBody::text("Attach Studio to LightPlayer on the connected simulator."),
                    vec![
                        device_action(DeviceOp::ConnectLightPlayer),
                        disconnect_device_action(),
                    ],
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
                connect_device_complete(esp32_metrics()),
                stack_section(
                    "connect-lightplayer",
                    "Flashing firmware",
                    UiStepState::NeedsAttention,
                    UiBody::Issue(
                        UxIssue::new("firmware flashing failed").with_detail(
                            "Check the cable, boot mode, and browser serial permission.",
                        ),
                    ),
                    vec![
                        device_action(DeviceOp::ProvisionFirmware),
                        disconnect_device_action(),
                    ],
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
                vec![disconnect_device_action()],
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
                vec![disconnect_device_action()],
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
                vec![disconnect_device_action()],
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
            connect_device_complete(esp32_metrics()),
            stack_section(
                "connect-lightplayer",
                "Flash firmware",
                UiStepState::Active,
                body,
                vec![
                    device_action(DeviceOp::ProvisionFirmware),
                    device_action(DeviceOp::ResetToBlank),
                    disconnect_device_action(),
                ],
            ),
        ],
        detail,
    )
}

fn blank_firmware_activity() -> UiActivity {
    let mut activity = UiActivity::new("Connecting ESP32 server")
        .with_detail("ESP32 boot output looks like blank or erased flash.")
        .with_progress(UiProgress::determinate(
            "LightPlayer protocol unavailable",
            100,
        ))
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
        ]);
    activity.push_terminal_line("ESP-ROM:esp32c6-20220919");
    activity.push_terminal_line("Build:Sep 19 2022");
    activity.push_terminal_line("rst:0x7 (TG0_WDT_HPSYS),boot:0x1e (SPI_FAST_FLASH_BOOT)");
    activity.push_terminal_line("invalid header: 0xffffffff");
    activity.push_terminal_line("invalid header: 0xffffffff");
    activity
}

fn provisioning_activity() -> UiActivity {
    let mut activity = UiActivity::new("Flashing firmware")
        .with_detail("Writing packaged LightPlayer ESP32-C6 firmware.")
        .with_progress(UiProgress::determinate("Writing flash", 42))
        .with_steps(vec![
            UiActivityStep::new("bootloader", "Bootloader")
                .with_state(UiActivityStepState::Complete),
            UiActivityStep::new("erase", "Erase").with_state(UiActivityStepState::Complete),
            UiActivityStep::new("write", "Write firmware").with_state(UiActivityStepState::Active),
            UiActivityStep::new("reboot", "Reboot").with_state(UiActivityStepState::Pending),
        ]);
    activity.push_terminal_line("Stub running...");
    activity.push_terminal_line("Changing baud rate to 921600");
    activity.push_terminal_line("Writing at 0x00010000... (42%)");
    activity
}

fn reset_activity() -> UiActivity {
    let mut activity = UiActivity::new("Wiping device")
        .with_detail("Erasing ESP32 flash through the bootloader.")
        .with_progress(UiProgress::determinate("Erasing flash", 58))
        .with_steps(vec![
            UiActivityStep::new("bootloader", "Bootloader")
                .with_state(UiActivityStepState::Complete),
            UiActivityStep::new("erase", "Erase flash").with_state(UiActivityStepState::Active),
            UiActivityStep::new("blank", "Blank device").with_state(UiActivityStepState::Pending),
        ]);
    activity.push_terminal_line("Stub running...");
    activity.push_terminal_line("Erasing flash (this may take a while)...");
    activity.push_terminal_line("Chip erase in progress");
    activity
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
    stack_section(
        "connect-device",
        "Connect device",
        UiStepState::Complete,
        UiBody::Metrics(metrics),
        Vec::new(),
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

fn device_action(op: DeviceOp) -> UiAction {
    UiAction::from_op(UxNodeId::new(DeviceUx::NODE_ID), op)
}

fn project_action(op: ProjectOp) -> UiAction {
    UiAction::from_op(UxNodeId::new(ProjectUx::NODE_ID), op)
}
