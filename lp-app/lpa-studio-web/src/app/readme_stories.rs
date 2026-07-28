//! README hero stories — the front-page captures the repo README embeds.
//!
//! Every story here is SINGLE-STATE by design (one clean frame, no stacked
//! state sheets) and fully deterministic: fixed story clock, seeded thumb
//! gradients, and hand-computed `VisualSrgb8` preview bytes so tracked
//! visual products show real rendered output without a live engine. The
//! README references the `__lg` captures; keep these stories framed for a
//! roughly 1080-wide, 600–800-tall shot.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::{
    ProjectController, ProjectEditorView, ProjectNodeStatusTone, ProjectNodeTreeView,
    ProjectSyncPhase, RosterCardState, UiAgentStatus, UiDeviceCard, UiDeviceProjectChip,
    UiExampleCard, UiHomeView, UiMetric, UiNodeFace, UiNodeHeader, UiNodeTab, UiNodeView,
    UiPackageCard, UiPaneView, UiStatus, UiStepState, UiStudioView, UiViewContent,
};

use crate::app::home::HomeGallery;
use crate::app::node::NodePane;
use crate::app::node::face_story_fixtures::{
    fixture_node_view, playlist_node_face_view, shader_face, shader_sections,
};
use crate::app::story_fixtures::{
    browser_worker_metrics, connect_device_complete, device_view, disconnect_device_action,
    disconnect_lightplayer_action, project_editor_summary, project_synced_metrics,
    select_connection_complete, shell_story, stack_section, story_node_status, tree_item,
};

/// A fixed "now" so relative times in baselines never drift (matches the
/// home-gallery stories' clock).
const STORY_NOW: f64 = 1_800_000_000.0;

#[story(
    screenshot,
    description = "README front-page hero: the full Studio editing a loaded show — sidebar node tree, the focused Aurora shader card (TRACKED visual preview with rendered output, knob row, agent chat, code drawer), and the connected simulator pane. Single-state and deterministic on purpose; the repo README embeds the lg capture."
)]
fn studio_hero() -> Element {
    shell_story(
        UiStudioView::new(
            vec![readme_project_pane(), readme_device_pane()],
            lpa_studio_core::UiConsoleView::empty(),
        ),
        true,
        Vec::new(),
    )
}

#[story(
    screenshot,
    description = "README home shot: the gallery with the simulator running a project, a connected device, the project library, and examples. Single-state, fixed clock, seeded thumbs; the repo README embeds the lg capture."
)]
fn home_gallery() -> Element {
    rsx! {
        section { class: "tw:p-4",
            HomeGallery {
                home: readme_home_view(),
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(true),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    screenshot,
    description = "README node-card band: playlist strip, shader, and fixture cards side by side, every preview TRACKED and rendering deterministic output (aurora pixel fields, ring lamp layout). Single-state; the repo README embeds the lg capture."
)]
fn node_cards() -> Element {
    rsx! {
        div { class: "tw:flex tw:w-full tw:items-start tw:gap-3.5 tw:p-4",
            div { class: "tw:min-w-0 tw:flex-1",
                NodePane {
                    view: readme_playlist_node(),
                    on_action: |_| {},
                }
            }
            div { class: "tw:min-w-0 tw:flex-1",
                NodePane {
                    view: readme_shader_node(false),
                    on_action: |_| {},
                }
            }
            div { class: "tw:min-w-0 tw:flex-1",
                NodePane {
                    view: fixture_node_view(),
                    on_action: |_| {},
                }
            }
        }
    }
}

/// The hero's project pane: an "Evening glow" show with a playlist, the
/// focused Aurora shader, and a fixture — the workspace shows the focused
/// node's card.
fn readme_project_pane() -> UiPaneView {
    UiPaneView::new(
        ProjectController::NODE_ID,
        "Project",
        UiStatus::good("Ready"),
        UiViewContent::ProjectEditor(Box::new(readme_project_editor())),
        Vec::new(),
    )
}

fn readme_project_editor() -> ProjectEditorView {
    let running = story_node_status("Running", ProjectNodeStatusTone::Good);
    let show = tree_item(
        1,
        "/evening_glow.show",
        "Evening glow",
        "Project",
        running.clone(),
        false,
        vec![
            tree_item(
                2,
                "/evening_glow.show/evening.playlist",
                "Evening set",
                "Playlist",
                running.clone(),
                false,
                Vec::new(),
            ),
            tree_item(
                3,
                "/evening_glow.show/aurora.shader",
                "Aurora",
                "Shader",
                running.clone(),
                true,
                Vec::new(),
            ),
            tree_item(
                4,
                "/evening_glow.show/halo.fixture",
                "Halo ring",
                "Fixture",
                running,
                false,
                Vec::new(),
            ),
        ],
    );
    ProjectEditorView::new(
        "evening-glow",
        1,
        project_editor_summary(ProjectSyncPhase::Ready),
        project_synced_metrics(),
        ProjectNodeTreeView::new(vec![show], 4),
        vec![readme_shader_node(true)],
    )
    .with_project_name("Evening glow")
}

/// The Aurora shader card with its face installed: tracked hero preview
/// (deterministic aurora bytes), knob row with a bus-bound speed knob, and
/// — when `with_agent` — the condensed agent chat.
fn readme_shader_node(with_agent: bool) -> UiNodeView {
    let header = UiNodeHeader::new("Aurora", "Shader", "/evening_glow.show/aurora.shader")
        .with_source("aurora.glsl")
        .with_status(UiStatus::good("Running"))
        .with_summary("GPU");
    let mut view = UiNodeView::new(header, vec![UiNodeTab::main(shader_sections())])
        .with_node_id("readme-shader-aurora");
    let mut face = shader_face(true, UiAgentStatus::Idle);
    if !with_agent {
        face.agent = None;
        face.code_drawer = None;
    }
    view.face = Some(UiNodeFace::Shader(face));
    view
}

/// The playlist strip card without its active-child card below (the band
/// story shows one card per column).
fn readme_playlist_node() -> UiNodeView {
    let mut view = playlist_node_face_view();
    view.children.clear();
    view
}

/// A compact simulator device pane: connected and ready, without the
/// open-project step, so the hero column stays within README height.
fn readme_device_pane() -> UiPaneView {
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
        ],
        vec![
            "[fw-browser] ready",
            "[lp-server] loaded project evening-glow",
        ],
    )
}

/// Home gallery content for the README shot: simulator running a project,
/// a live device, a small library, and the example row.
fn readme_home_view() -> UiHomeView {
    let projects = vec![
        UiPackageCard {
            uid: "prj_3fKq8Zr21bTxYw0AhVmDpe".to_string(),
            kind: "Project".to_string(),
            slug: "2026-07-02-0930-porch-sign".to_string(),
            last_saved_at: Some(STORY_NOW - 2.0 * 3600.0),
            provenance: None,
            on_device: None,
            open_elsewhere: false,
            connected_device: Some(lpa_studio_core::UiCardConnection {
                device_name: "Workbench ESP32".to_string(),
                relation: lpa_studio_core::SyncRelation::AtHead,
            }),
            running_in_sim: false,
        },
        UiPackageCard {
            uid: "prj_9sLm2Xc44dQnUv7BgWkEyt".to_string(),
            kind: "Project".to_string(),
            slug: "2026-07-04-1102-evening-glow".to_string(),
            last_saved_at: Some(STORY_NOW - 5.0 * 86_400.0),
            provenance: Some("Remixed from Basic".to_string()),
            on_device: None,
            open_elsewhere: false,
            connected_device: None,
            running_in_sim: true,
        },
        UiPackageCard {
            uid: "prj_1aBc3De56fGhIj8KlMnOpq".to_string(),
            kind: "Project".to_string(),
            slug: "2026-05-28-1740-porch-sign".to_string(),
            last_saved_at: Some(STORY_NOW - 40.0 * 86_400.0),
            provenance: Some("Forked from 2026-07-02-0930-porch-sign".to_string()),
            on_device: None,
            open_elsewhere: false,
            connected_device: None,
            running_in_sim: false,
        },
    ];
    let devices = vec![
        UiDeviceCard {
            uid: None,
            name: "Simulator".to_string(),
            transport: String::new(),
            state: RosterCardState::RunningUpToDate,
            project: Some(UiDeviceProjectChip {
                uid: "prj_9sLm2Xc44dQnUv7BgWkEyt".to_string(),
                name: "2026-07-04-1102-evening-glow".to_string(),
            }),
            fw: None,
            sim: true,
            console_tail: Vec::new(),
            ui: Default::default(),
        },
        UiDeviceCard {
            uid: Some("dev_7pQr5St89uVwXy2CzDaFbg".to_string()),
            name: "Workbench ESP32".to_string(),
            transport: "USB".to_string(),
            state: RosterCardState::RunningUpToDate,
            project: Some(UiDeviceProjectChip {
                uid: "prj_3fKq8Zr21bTxYw0AhVmDpe".to_string(),
                name: "2026-07-02-0930-porch-sign".to_string(),
            }),
            fw: None,
            sim: false,
            console_tail: Vec::new(),
            ui: Default::default(),
        },
    ];
    UiHomeView {
        devices,
        projects,
        examples: vec![UiExampleCard {
            id: "examples/basic".to_string(),
            name: "Basic".to_string(),
            kind: "Project".to_string(),
        }],
        library_available: true,
        opening: None,
        issue: None,
    }
}
