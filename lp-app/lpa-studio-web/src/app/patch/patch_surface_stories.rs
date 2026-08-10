//! Patch surface stories (D36, slice 2): hand-built DTOs, like every face
//! story — derivation is covered by unit tests and e2e; these pin the
//! LOOK. Frames are deliberately absent (the canvas renders its honest
//! "no frame yet"): live pixels are the dev server's job, and a story that
//! faked them would drift from the renderer it claims to pin.

use dioxus::prelude::*;
use lpa_studio_core::{
    NodeId, UiFixturePatch, UiPatchBay, UiPatchCell, UiPatchInstance, UiPatchPort, UiPatchSurface,
    UiPatchSurfaceFixture, UiPatchSurfaceOutput, UiPatchTarget,
};
use lpa_studio_web_story_macros::story;

use super::PatchSurfacePage;

fn cell(id: &str, producer: &str, source_start: u32, lamps: u32, wire_start: u32) -> UiPatchCell {
    UiPatchCell {
        id: id.to_string(),
        producer: producer.to_string(),
        producer_node: None,
        source_start,
        lamps,
        wire_start,
        reversed: false,
        contested: false,
        port_key: Some(0),
        port_label: String::new(),
        output_label: String::new(),
    }
}

fn port(key: u32, pin: &str, start: u32, lamps: u32, cells: Vec<UiPatchCell>) -> UiPatchPort {
    UiPatchPort {
        key,
        pin_label: pin.to_string(),
        start,
        lamps,
        cells,
    }
}

fn instance(path: &str, label: &str, start: u32, lamps: u32, stride: u32) -> UiPatchInstance {
    UiPatchInstance {
        path: path.to_string(),
        label: label.to_string(),
        start,
        lamps,
        stride,
    }
}

/// The mini-dome's shape: two named outputs sharing sectors and doors.
fn mini_dome_surface(contested: bool) -> UiPatchSurface {
    let mut sector2 = cell("dome:0:60:0", "dome", 60, 30, 0);
    sector2.contested = contested;
    let mut door0 = cell("doors:0:0:30", "doors", 0, 9, 30);
    door0.contested = contested;
    UiPatchSurface {
        outputs: vec![
            UiPatchSurfaceOutput {
                node: NodeId::new(10),
                label: "out_a".to_string(),
                name: Some("1".to_string()),
                address: None,
                name_assign: None,
                bay: UiPatchBay {
                    ports: vec![
                        port(0, "IO18", 0, 39, vec![sector2, door0]),
                        port(
                            1,
                            "IO13",
                            39,
                            30,
                            vec![cell("dome:0:120:39", "dome", 120, 30, 39)],
                        ),
                        port(
                            2,
                            "IO2",
                            69,
                            39,
                            vec![
                                cell("dome:0:0:69", "dome", 0, 30, 69),
                                cell("doors:0:18:99", "doors", 18, 9, 99),
                            ],
                        ),
                    ],
                    frame: None,
                    contested_lamps: if contested { 9 } else { 0 },
                    gap_lamps: 0,
                },
            },
            UiPatchSurfaceOutput {
                node: NodeId::new(11),
                label: "out_b".to_string(),
                name: Some("Box 2".to_string()),
                address: None,
                name_assign: None,
                bay: UiPatchBay {
                    ports: vec![
                        port(
                            0,
                            "IO14",
                            0,
                            39,
                            vec![
                                cell("dome:0:30:0", "dome", 30, 30, 0),
                                cell("doors:0:9:30", "doors", 9, 9, 30),
                            ],
                        ),
                        port(
                            1,
                            "IO16",
                            39,
                            30,
                            vec![cell("dome:0:90:39", "dome", 90, 30, 39)],
                        ),
                    ],
                    frame: None,
                    contested_lamps: 0,
                    gap_lamps: 0,
                },
            },
        ],
        fixtures: vec![
            UiPatchSurfaceFixture {
                node: NodeId::new(2),
                label: "dome".to_string(),
                address: None,
                mapping_artifact: None,
                patch_artifact: None,
                mapping_loaded: true,
                patch_loaded: true,
                patch: UiFixturePatch {
                    lamps: 150,
                    cells: vec![
                        cell("dome:0:0:69", "dome", 0, 30, 69),
                        cell("dome:0:30:0", "dome", 30, 30, 0),
                        cell("dome:0:60:0", "dome", 60, 30, 0),
                        cell("dome:0:90:39", "dome", 90, 30, 39),
                        cell("dome:0:120:39", "dome", 120, 30, 39),
                    ],
                    frame: None,
                    single_output: false,
                },
                instances: (0..5)
                    .map(|k| {
                        instance(
                            &format!("/sector/{k}"),
                            &format!("sector {k}"),
                            k * 30,
                            30,
                            30,
                        )
                    })
                    .collect(),
            },
            UiPatchSurfaceFixture {
                node: NodeId::new(3),
                label: "doors".to_string(),
                address: None,
                mapping_artifact: None,
                patch_artifact: None,
                mapping_loaded: true,
                patch_loaded: true,
                patch: UiFixturePatch {
                    lamps: 27,
                    cells: vec![
                        cell("doors:0:0:30", "doors", 0, 9, 30),
                        cell("doors:0:9:30", "doors", 9, 9, 30),
                        cell("doors:0:18:99", "doors", 18, 9, 99),
                    ],
                    frame: None,
                    single_output: false,
                },
                instances: (0..3)
                    .map(|k| instance(&format!("/door/{k}"), &format!("door {k}"), k * 9, 9, 3))
                    .collect(),
            },
        ],
    }
}

/// The peach: one output, two fixtures, NO instance grain (format-1 range
/// entries over docs without ids) — the surface's second first-class shape.
fn peach_surface() -> UiPatchSurface {
    UiPatchSurface {
        outputs: vec![UiPatchSurfaceOutput {
            node: NodeId::new(10),
            label: "output".to_string(),
            name: None,
            address: None,
            name_assign: None,
            bay: UiPatchBay {
                ports: vec![port(
                    0,
                    "D10",
                    0,
                    56,
                    vec![
                        cell("body:0:0:0", "peach_body", 0, 22, 0),
                        cell("leaf:0:0:22", "peach_leaf", 0, 12, 22),
                        {
                            let mut second = cell("body:0:22:34", "peach_body", 22, 22, 34);
                            second.reversed = true;
                            second
                        },
                    ],
                )],
                frame: None,
                contested_lamps: 0,
                gap_lamps: 0,
            },
        }],
        fixtures: vec![
            UiPatchSurfaceFixture {
                node: NodeId::new(2),
                label: "peach_body".to_string(),
                address: None,
                mapping_artifact: None,
                patch_artifact: None,
                mapping_loaded: true,
                patch_loaded: true,
                patch: UiFixturePatch {
                    lamps: 44,
                    cells: vec![cell("body:0:0:0", "peach_body", 0, 22, 0), {
                        let mut second = cell("body:0:22:34", "peach_body", 22, 22, 34);
                        second.reversed = true;
                        second
                    }],
                    frame: None,
                    single_output: true,
                },
                instances: Vec::new(),
            },
            UiPatchSurfaceFixture {
                node: NodeId::new(3),
                label: "peach_leaf".to_string(),
                address: None,
                mapping_artifact: None,
                patch_artifact: None,
                mapping_loaded: true,
                patch_loaded: true,
                patch: UiFixturePatch {
                    lamps: 12,
                    cells: vec![cell("leaf:0:0:22", "peach_leaf", 0, 12, 22)],
                    frame: None,
                    single_output: true,
                },
                instances: Vec::new(),
            },
        ],
    }
}

fn surface_view(
    surface: UiPatchSurface,
    selection: Option<UiPatchTarget>,
) -> lpa_studio_core::ProjectEditorView {
    lpa_studio_core::ProjectEditorView::new(
        "story",
        0,
        lpa_studio_core::ProjectSyncSummary::default(),
        Vec::new(),
        lpa_studio_core::ProjectNodeTreeView::new(Vec::new(), 0),
        Vec::new(),
    )
    .with_patch_surface(Some(surface), selection)
}

#[story(
    description = "The patch surface on the mini-dome: output → port sidebar, two named outputs' port strips with sectors and doors sharing ports (many-to-many), and both fixtures' instance chips. Sector 2 is selected — the sidebar, its cells, and its chip light together. Frames are absent by design; live pixels are the dev server's demo."
)]
fn patch_surface_mini_dome_selected_sector() -> Element {
    rsx! {
        PatchSurfacePage {
            view: surface_view(
                mini_dome_surface(false),
                Some(UiPatchTarget::Instance {
                    node: NodeId::new(2),
                    path: "/sector/2".to_string(),
                }),
            ),
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "The overlap state: a sector and a door claiming the same stretch of port 0 wear the contested red on every strip they appear in, and the sidebar's output row counts the contested lamps. Degrade-and-report — nothing stops rendering."
)]
fn patch_surface_mini_dome_overlap() -> Element {
    rsx! {
        PatchSurfacePage {
            view: surface_view(mini_dome_surface(true), None),
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "The peach on the same surface: one unnamed output, format-1 range-grain fixtures (no instance chips — 'range grain' says so), the body split with its far half reversed. The surface is honest at both grains; instance verbs will simply be absent here."
)]
fn patch_surface_peach_range_grain() -> Element {
    rsx! {
        PatchSurfacePage {
            view: surface_view(peach_surface(), None),
            on_action: move |_| {},
        }
    }
}
