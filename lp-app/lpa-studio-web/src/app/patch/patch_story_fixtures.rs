//! Shared patch-surface STORY FIXTURES: hand-built DTOs (small-dome,
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

/// A hand-built MINIATURE in the small-dome's shape (two named outputs
/// sharing repeated instances and doors) — deliberately kept at story
/// scale (five 30-lamp "sectors", three 9-lamp doors, five ports), NOT
/// the real example's 6,310 lamps: stories pin UI structure, not scale.
pub(crate) fn small_dome_surface(contested: bool) -> UiPatchSurface {
    let surface = build_small_dome_surface(contested);
    finish_surface(surface)
}

fn build_small_dome_surface(contested: bool) -> UiPatchSurface {
    let mut sector2 = cell("dome:0:60:0", "dome", 60, 30, 0);
    sector2.contested = contested;
    let mut door0 = cell("doors:0:0:30", "doors", 0, 9, 30);
    door0.contested = contested;
    UiPatchSurface {
        // The example's real tree shape: each fixture lives in its OWN
        // sub-module under the root show.
        modules: vec![
            module(1, "small_dome", 0),
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

/// The small-dome posed for the WALK-UP: both fixtures MANUAL, with the last
/// sector and the last door taken off the wire.
///
/// The auto-mapped pose ([`small_dome_surface`]) is the world where nothing is
/// ever waiting — objects flow onto the wire by themselves — so the panel's
/// invitation, armed and object-first states have nothing to be about there
/// (P5c left the story fixtures auto; P6 poses the other half). Manual flow
/// plus two unplaced objects gives the panel exactly what the walk-up loop
/// needs: free port space to select as a segment, unmapped objects to link it
/// to, and ports at three different occupancies for the pickers to explain.
///
/// The runs removed are the whole of `IO13` on box 1 (thirty free lamps, the
/// size of the sector waiting for them) and the door at the end of `IO2`
/// (nine free at its tail) — one empty port and one part-used one, because
/// those read differently in every picker and occupancy line.
pub(crate) fn small_dome_walkup_surface() -> UiPatchSurface {
    let mut surface = build_small_dome_surface(false);
    let unpatched = ["dome:0:120:39", "doors:0:18:99"];
    let keep = |cell: &UiPatchCell| !unpatched.contains(&cell.id.as_str());
    for output in &mut surface.outputs {
        for port in &mut output.bay.ports {
            port.cells.retain(keep);
        }
    }
    for fixture in &mut surface.fixtures {
        fixture.manual_flow = true;
        fixture.patch.cells.retain(keep);
    }
    // `finish_surface` re-derives `placed` from the runs that are left, the
    // way `build_patch_surface` derives it in production — so sector 4 and
    // door 2 come out of this unplaced without anyone saying so twice.
    finish_surface(surface)
}

/// The peach with both fixtures MANUAL: the range-grain shape (no object
/// table) is the scarf of Q8's exception — a fixture that IS its own object,
/// so the panel gives it the object treatment plus the flow row rather than a
/// fixture card it would have nothing to say on.
pub(crate) fn peach_manual_surface() -> UiPatchSurface {
    let mut surface = build_peach_surface();
    for fixture in &mut surface.fixtures {
        fixture.manual_flow = true;
    }
    finish_surface(surface)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The walk-up pose must actually POSE the walk-up: the panel's armed,
    /// invitation and object-first stories are only meaningful while these
    /// hold, and a fixture edit that quietly re-mapped everything would leave
    /// the captures showing paired states under armed names.
    #[test]
    fn walkup_pose_leaves_two_objects_waiting() {
        let surface = small_dome_walkup_surface();
        assert!(
            surface.fixtures.iter().all(|fixture| fixture.manual_flow),
            "the walk-up pose is the MANUAL world"
        );
        let unplaced: Vec<&str> = surface
            .fixtures
            .iter()
            .flat_map(|fixture| &fixture.instances)
            .filter(|instance| !instance.placed)
            .map(|instance| instance.path.as_str())
            .collect();
        assert_eq!(unplaced, vec!["/sector/4", "/door/2"]);
    }

    /// The free space the segment stories select, and the occupancies the
    /// pickers explain: one empty port, one part-used, the rest full.
    #[test]
    fn walkup_pose_frees_one_whole_port_and_one_tail() {
        let surface = small_dome_walkup_surface();
        let free: Vec<(String, u32)> = surface
            .outputs
            .iter()
            .flat_map(|output| output.bay.ports.iter())
            .map(|port| {
                let used: u32 = port.cells.iter().map(|cell| cell.lamps).sum();
                (port.pin_label.clone(), port.lamps.saturating_sub(used))
            })
            .filter(|(_, free)| *free > 0)
            .collect();
        assert_eq!(
            free,
            vec![("IO13".to_string(), 30), ("IO2".to_string(), 9)],
            "IO13 is empty (the segment stories' 30 lamps) and IO2 keeps a 9-lamp tail"
        );
    }

    /// The peach keeps its range grain when posed manual — the scarf story
    /// depends on there being no object table to make a fixture card out of.
    #[test]
    fn peach_manual_pose_stays_range_grain() {
        let surface = peach_manual_surface();
        assert!(
            surface
                .fixtures
                .iter()
                .all(|fixture| fixture.manual_flow && fixture.instances.is_empty())
        );
    }
}
