//! Shared patch-surface STORY FIXTURES: hand-built DTOs (mini-dome,
//! peach) the workbench and editor-shell stories pin their looks with —
//! derivation is covered by unit tests and e2e. Frames are deliberately
//! absent (cells draw their honest "no frame yet"): live pixels are the
//! dev server's job, and a story that faked them would drift from the
//! renderer it claims to pin. (The interim `/patch` page and its stories
//! are gone — R5 re-housed patching as a workbench view.)

use lpa_studio_core::{
    NodeId, UiFixturePatch, UiPatchBay, UiPatchCell, UiPatchInstance, UiPatchPort, UiPatchSurface,
    UiPatchSurfaceFixture, UiPatchSurfaceModule, UiPatchSurfaceOutput,
};

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

/// A surface module row — production always has at least the root (the
/// tree root wears the module kind), and the panels render the chain as
/// the levels above the fixtures/outputs.
fn module(node: u32, label: &str, depth: usize) -> UiPatchSurfaceModule {
    UiPatchSurfaceModule {
        node: NodeId::new(node),
        label: label.to_string(),
        address: None,
        depth,
    }
}

fn instance(path: &str, label: &str, start: u32, lamps: u32, stride: u32) -> UiPatchInstance {
    UiPatchInstance {
        path: path.to_string(),
        label: label.to_string(),
        start,
        lamps,
        stride,
        // Stamped by `finish_surface`, the way `build_patch_surface`
        // derives it in production.
        placed: false,
    }
}

/// What `build_patch_surface` stamps in production: per-instance placed
/// state from the fixture's runs, and a settled editor.json (stories show
/// the loaded state; the arrange facts default to unarranged).
fn finish_surface(mut surface: UiPatchSurface) -> UiPatchSurface {
    for fixture in &mut surface.fixtures {
        let cells = fixture.patch.cells.clone();
        for instance in &mut fixture.instances {
            instance.placed = cells.iter().any(|cell| {
                cell.source_start < instance.start + instance.lamps
                    && cell.source_start + cell.lamps > instance.start
            });
        }
        fixture.arrange = Some(lpa_studio_core::UiArrangeMeta::default());
        if fixture.address.is_none() {
            fixture.address = Some(format!("/{}", fixture.label));
        }
    }
    surface.editor_meta_loaded = true;
    surface.editor_meta_artifact = Some(lpa_studio_core::editor_meta_artifact());
    surface
}

/// The mini-dome's shape: two named outputs sharing sectors and doors.
pub(crate) fn mini_dome_surface(contested: bool) -> UiPatchSurface {
    let surface = build_mini_dome_surface(contested);
    finish_surface(surface)
}

fn build_mini_dome_surface(contested: bool) -> UiPatchSurface {
    let mut sector2 = cell("dome:0:60:0", "dome", 60, 30, 0);
    sector2.contested = contested;
    let mut door0 = cell("doors:0:0:30", "doors", 0, 9, 30);
    door0.contested = contested;
    UiPatchSurface {
        // The example's real tree shape: each fixture lives in its OWN
        // sub-module under the root show.
        modules: vec![
            module(1, "mini_dome", 0),
            module(20, "Dome", 1),
            module(21, "Doors", 1),
        ],
        outputs: vec![
            UiPatchSurfaceOutput {
                node: NodeId::new(10),
                label: "out_a".to_string(),
                name: Some("1".to_string()),
                address: None,
                name_assign: None,
                module: Some(NodeId::new(1)),
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
                module: Some(NodeId::new(1)),
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
                // Stories pose the AUTO-mapped world (P5b's default).
                manual_flow: false,
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
                module: Some(NodeId::new(20)),
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
                arrange: None,
            },
            UiPatchSurfaceFixture {
                node: NodeId::new(3),
                label: "doors".to_string(),
                address: None,
                mapping_artifact: None,
                patch_artifact: None,
                mapping_loaded: true,
                patch_loaded: true,
                // Stories pose the AUTO-mapped world (P5b's default).
                manual_flow: false,
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
                module: Some(NodeId::new(21)),
                instances: (0..3)
                    .map(|k| instance(&format!("/door/{k}"), &format!("door {k}"), k * 9, 9, 3))
                    .collect(),
                arrange: None,
            },
        ],
        ..Default::default()
    }
}

/// The peach: one output, two fixtures, NO instance grain (format-1 range
/// entries over docs without ids) — the surface's second first-class shape.
pub(crate) fn peach_surface() -> UiPatchSurface {
    finish_surface(build_peach_surface())
}

fn build_peach_surface() -> UiPatchSurface {
    UiPatchSurface {
        modules: vec![module(1, "peach", 0)],
        outputs: vec![UiPatchSurfaceOutput {
            node: NodeId::new(10),
            label: "output".to_string(),
            name: None,
            address: None,
            name_assign: None,
            module: Some(NodeId::new(1)),
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
                // Stories pose the AUTO-mapped world (P5b's default).
                manual_flow: false,
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
                module: Some(NodeId::new(1)),
                instances: Vec::new(),
                arrange: None,
            },
            UiPatchSurfaceFixture {
                node: NodeId::new(3),
                label: "peach_leaf".to_string(),
                address: None,
                mapping_artifact: None,
                patch_artifact: None,
                mapping_loaded: true,
                patch_loaded: true,
                // Stories pose the AUTO-mapped world (P5b's default).
                manual_flow: false,
                patch: UiFixturePatch {
                    lamps: 12,
                    cells: vec![cell("leaf:0:0:22", "peach_leaf", 0, 12, 22)],
                    frame: None,
                    single_output: true,
                },
                module: Some(NodeId::new(1)),
                instances: Vec::new(),
                arrange: None,
            },
        ],
        ..Default::default()
    }
}
