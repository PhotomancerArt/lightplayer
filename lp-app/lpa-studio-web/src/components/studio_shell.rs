use dioxus::prelude::*;
use lpa_studio_ux::{DeviceUx, StudioView, UiAction, UiBody, UiPaneView, UxLogEntry, UxLogLevel};

use crate::components::{RuntimeLog, UxPane};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioShell(view: StudioView, running: bool, on_action: EventHandler<UiAction>) -> Element {
    let StudioView { panes, logs } = view;
    let PaneGroups { main, device } = group_panes(panes);
    let logs = logs_with_device_terminal(logs, device.as_ref());
    let layout_class = if main.is_empty() {
        "ux-layout ux-layout-device-only"
    } else {
        "ux-layout ux-layout-main-device"
    };
    let device_is_primary = main.is_empty();

    rsx! {
        main { class: "ux-shell",
            header { class: "ux-header",
                div {
                    p { class: "ux-eyebrow", "LightPlayer Studio" }
                }
            }

            section { class: "{layout_class}",
                if !main.is_empty() {
                    div { class: "ux-main-column",
                        for (index, pane) in main.into_iter().enumerate() {
                            UxPane {
                                key: "{pane.node_id}",
                                view: pane,
                                primary: index == 0,
                                running,
                                on_action,
                            }
                        }
                    }
                }

                div { class: "ux-device-column",
                    if let Some(device) = device {
                        UxPane {
                            key: "{device.node_id}",
                            view: device,
                            primary: device_is_primary,
                            running,
                            on_action,
                        }
                    }
                    RuntimeLog { logs }
                }
            }
        }
    }
}

struct PaneGroups {
    main: Vec<UiPaneView>,
    device: Option<UiPaneView>,
}

fn group_panes(panes: Vec<UiPaneView>) -> PaneGroups {
    let mut main = Vec::new();
    let mut device = None;
    for pane in panes {
        if pane.node_id.as_str() == DeviceUx::NODE_ID {
            device = Some(pane);
        } else {
            main.push(pane);
        }
    }
    PaneGroups { main, device }
}

fn logs_with_device_terminal(
    logs: Vec<UxLogEntry>,
    device: Option<&UiPaneView>,
) -> Vec<UxLogEntry> {
    let Some(device) = device else {
        return logs;
    };
    let terminal_logs = device_terminal_logs(device);
    if terminal_logs.is_empty() {
        return logs;
    }

    let mut merged = terminal_logs;
    for log in logs {
        if !log_exists(&merged, &log) {
            merged.push(log);
        }
    }
    merged
}

fn device_terminal_logs(device: &UiPaneView) -> Vec<UxLogEntry> {
    let UiBody::Stack(stack) = &device.body else {
        return Vec::new();
    };
    stack
        .terminal
        .iter()
        .map(|line| terminal_line_to_log(&line.text))
        .collect()
}

fn terminal_line_to_log(line: &str) -> UxLogEntry {
    if let Some((source, message)) = parse_bracketed_log_line(line) {
        UxLogEntry::new(UxLogLevel::Info, source, message)
    } else {
        UxLogEntry::new(UxLogLevel::Info, "device", line)
    }
}

fn parse_bracketed_log_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('[')?;
    let (source, message) = rest.split_once("] ")?;
    Some((source, message))
}

fn log_exists(logs: &[UxLogEntry], candidate: &UxLogEntry) -> bool {
    logs.iter()
        .any(|log| log.source == candidate.source && log.message == candidate.message)
}
