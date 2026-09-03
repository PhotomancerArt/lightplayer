//! Studio story fixtures.
//!
//! This module is compiled only for storybook builds. It keeps broad
//! shell/device/project fixture builders in one place while story entrypoints
//! live next to their component families.

use crate::app::{PaneFrame, StudioShell};
use crate::base::{FieldRow, TabItem, Tabs};
use crate::core::MetricGrid;
use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, ProjectController, ProjectEditorOp, ProjectEditorView, ProjectInventorySummary,
    ProjectNodeStatusTone, ProjectNodeStatusView, ProjectNodeTreeItem, ProjectNodeTreeView,
    ProjectRuntimeSummary, ProjectState, ProjectSyncPhase, ProjectSyncSummary, SimCardState,
    UiAction, UiAssetEditorKind, UiBindingEndpoint, UiConfigSlot, UiConsoleView, UiIssue,
    UiLensCard, UiLogEntry, UiLogLevel, UiLogOrigin, UiLogSource, UiMetric, UiNodeChild,
    UiNodeHeader, UiNodeSection, UiNodeTab, UiNodeView, UiPaneView, UiProducedProduct,
    UiProducedValue, UiSimCard, UiSimProjectChip, UiSlotAsset, UiSlotSourceState, UiSlotValue,
    UiStatus, UiStudioView, UiViewContent,
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
    UiStudioView::new(panes, console).with_lens_card(Some(UiLensCard::Sim(simulator_lens_card())))
}

/// The editor's runtime surface (D43): a running simulator as the LENS
/// card. Every pane-layout story carries one — the shell renders no other
/// runtime surface since the step-stack pane retired, and core pins
/// "panes non-empty ⇒ lens card".
pub(crate) fn simulator_lens_card() -> UiSimCard {
    UiSimCard {
        state: SimCardState::Running,
        project: Some(UiSimProjectChip {
            uid: "prj9sLm2Xc44dQnUv7BgWkEyt".to_string(),
            name: "demo-project".to_string(),
        }),
        board_id: None,
        console_tail: vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Info,
            UiLogSource::with_detail(UiLogOrigin::Device, "fw-browser"),
            "engine: project loaded",
        )],
        frame_preview: None,
        frame_age_secs: None,
        frame_fps: None,
        ui: Default::default(),
    }
}

pub(crate) fn shell_story(
    mut view: UiStudioView,
    running: bool,
    story_logs: Vec<UiLogEntry>,
) -> Element {
    // the global console UI retired (M7′ P2); the entries still ride the
    // view so fixtures stay honest about what the controller carries
    view.console.entries.extend(story_logs);
    rsx! {
        // Body only: the site chrome above it is `web_app`'s, and has its
        // own stories (`site_chrome_stories`).
        StudioShell {
            view,
            running,
            on_action: move |_| {},
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

pub(crate) fn studio_log(level: UiLogLevel, message: impl Into<String>) -> UiLogEntry {
    UiLogEntry::new(STORY_LOG_TIMESTAMP, level, UiLogOrigin::Studio, message)
}

pub(crate) fn project_ready_view() -> UiStudioView {
    story_view(
        vec![project_synced_pane_view()],
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
        vec![project_syncing_pane_view()],
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
        vec![project_sync_failed_pane_view()],
        vec![UiLogEntry::new(
            STORY_LOG_TIMESTAMP,
            UiLogLevel::Error,
            UiLogOrigin::Studio,
            "project sync failed: protocol timeout",
        )],
    )
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
        "/demo.module",
        "Demo",
        "Project",
        running.clone(),
        false,
        vec![
            tree_item(
                2,
                "/demo.module/clock.clock",
                "Clock",
                "Clock",
                running.clone(),
                false,
                Vec::new(),
            ),
            tree_item(
                3,
                "/demo.module/orbit.shader",
                "Orbit shader",
                "Shader",
                running.clone(),
                true,
                Vec::new(),
            ),
            tree_item(
                4,
                "/demo.module/palette.visual",
                "Sunrise palette",
                "Visual",
                warning.clone(),
                false,
                Vec::new(),
            ),
            tree_item(
                5,
                "/demo.module/output.output",
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
    .with_manifest(Some(project_manifest()))
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
/// section: `name` editable, `format`/`uid`/`nodes` read-only — matching
/// the `Fixed` role each carries on `lpc_model::ModuleDef`
/// (`uid` joined them 2026-07-28; it had been writable by default).
pub(crate) fn project_root_slots() -> Vec<UiConfigSlot> {
    use lpa_studio_core::UiSlotFieldState;
    vec![
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

/// Container-manifest identity for the settings-section stories.
pub(crate) fn project_manifest() -> lpa_studio_core::UiProjectManifest {
    lpa_studio_core::UiProjectManifest {
        format: Some(3),
        uid: Some("prj7k2mQx4vN8pL".to_string()),
        name: Some("Demo".to_string()),
        kind: "General".to_string(),
    }
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
        "/demo.module/clock.clock",
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
        "/demo.module/orbit.shader",
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
                    "bus:time",
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
        "/demo.module/palette.visual",
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
        "/demo.module/output.output",
        UiStatus::good("Running"),
    )
    .with_sections(vec![UiNodeSection::ConfigSlots(vec![
        UiConfigSlot::value("input", "Input", UiSlotValue::string("orbit#output")).with_source(
            UiSlotSourceState::Bound(UiBindingEndpoint::new("orbit#visual.output")),
        ),
        UiConfigSlot::value(
            "endpoint",
            "Endpoint",
            UiSlotValue::string("ws281x:local:D10"),
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
