//! The patch verbs' UI-side helpers — pure functions over `(surface,
//! selection)` shared by every surface that writes a patch (the workbench
//! Patching view; formerly the interim `/patch` page). A write is a PRESS
//! of one of core's patch offers (`project/<node>/patch/<verb>`, M6b) with
//! the picks as args — the same press the app agent makes by path; the
//! surface's selection only tells these helpers which offer and which
//! subject. Nothing here renders, and nothing here builds an op.

use dioxus::prelude::*;
use lpa_studio_core::{
    NodeId, OfferArgs, OfferPath, PATCH_ASSIGN_VERB, PATCH_DELTA_PARAM, PATCH_LAMP_PARAM,
    PATCH_LAMPS_PARAM, PATCH_OUTPUT_PARAM, PATCH_PORT_PARAM, PATCH_SHIFT_PORT_VERB,
    PATCH_START_PARAM, PATCH_SUBJECT_PARAM, PATCH_SWAP_PORTS_VERB, PATCH_WITH_PARAM, UiAction,
    UiOfferTree, UiPatchInstance, UiPatchPort, UiPatchSurface, UiPatchSurfaceOutput, UiPatchTarget,
    patch_history_path,
};

/// One press of a patch offer: where it is, and the picks it carries.
pub(crate) type PatchPress = (OfferPath, OfferArgs);

/// Press `press` on `offers` and dispatch what it binds to. `false` (and a
/// console line saying why) when the tree does not publish it — a story, or
/// a verb with nothing to act on, like Undo with no history — or refuses
/// the picks.
pub(crate) fn press_patch_offer(
    on_action: &EventHandler<UiAction>,
    offers: &UiOfferTree,
    press: Option<PatchPress>,
) -> bool {
    match press.and_then(|(path, args)| bind_patch_press(offers, &path, &args)) {
        Some(action) => {
            on_action.call(action);
            true
        }
        None => false,
    }
}

/// The action the offer at `path` binds `args` to, or why not, logged.
pub(crate) fn bind_patch_press(
    offers: &UiOfferTree,
    path: &OfferPath,
    args: &OfferArgs,
) -> Option<UiAction> {
    match offers.get(path).map(|offer| offer.press(args)) {
        Some(Ok(action)) => Some(action),
        Some(Err(error)) => {
            log::warn!("patch: `{path}` refused the press: {error}");
            None
        }
        None => {
            log::warn!("patch: `{path}` is not offered");
            None
        }
    }
}

/// The press of a fixture verb on the object `selection` names (`reverse`,
/// `rotate`, `clear`, `assign`): the selection's fixture's offer, with the
/// selection as its `subject` and `args` beside it. `None` when the
/// selection names no object (a port, a segment, nothing).
pub(crate) fn subject_verb_press(
    surface: &UiPatchSurface,
    selection: Option<&UiPatchTarget>,
    verb: &str,
    args: OfferArgs,
) -> Option<PatchPress> {
    let (node, subject) = surface.patch_subject(selection?)?;
    Some((
        surface.patch_verbs_of(node)?.child(verb),
        args.with(PATCH_SUBJECT_PARAM, subject),
    ))
}

/// Press a fixture verb on the object `selection` names
/// ([`subject_verb_press`]).
pub(crate) fn press_subject_verb(
    on_action: &EventHandler<UiAction>,
    offers: &UiOfferTree,
    surface: &UiPatchSurface,
    selection: &Option<UiPatchTarget>,
    verb: &str,
    args: OfferArgs,
) -> bool {
    press_patch_offer(
        on_action,
        offers,
        subject_verb_press(surface, selection.as_ref(), verb, args),
    )
}

/// Press a verb of the fixture or output `node` itself (`set-flow`,
/// `unmap-all`).
pub(crate) fn press_node_verb(
    on_action: &EventHandler<UiAction>,
    offers: &UiOfferTree,
    surface: &UiPatchSurface,
    node: NodeId,
    verb: &str,
    args: OfferArgs,
) -> bool {
    press_patch_offer(
        on_action,
        offers,
        surface
            .patch_verbs_of(node)
            .map(|at| (at.child(verb), args)),
    )
}

/// Press the patch history's `undo` or `redo`.
pub(crate) fn press_history(
    on_action: &EventHandler<UiAction>,
    offers: &UiOfferTree,
    verb: &str,
) -> bool {
    press_patch_offer(
        on_action,
        offers,
        Some((patch_history_path(verb), OfferArgs::new())),
    )
}

/// The press that completes an armed SWAP: the armed port (`armed`, an
/// output and its port key) swaps with the clicked `port` of `output` — the
/// armed output's `swap-ports`, with both picks.
pub(crate) fn swap_press(
    surface: &UiPatchSurface,
    armed: (NodeId, u32),
    output: &UiPatchSurfaceOutput,
    port: u32,
) -> Option<PatchPress> {
    Some((
        surface
            .patch_verbs_of(armed.0)?
            .child(PATCH_SWAP_PORTS_VERB),
        OfferArgs::new()
            .with(PATCH_PORT_PARAM, armed.1.to_string())
            .with(PATCH_WITH_PARAM, output.patch_port_value(port)?),
    ))
}

/// Complete an armed SWAP ([`swap_press`]).
pub(crate) fn press_swap(
    on_action: &EventHandler<UiAction>,
    offers: &UiOfferTree,
    surface: &UiPatchSurface,
    armed: (NodeId, u32),
    output: &UiPatchSurfaceOutput,
    port: u32,
) -> bool {
    press_patch_offer(on_action, offers, swap_press(surface, armed, output, port))
}

/// Move every entry in the wire window `(start, lamps)` of `output` by
/// `delta` lamps (`shift-port`).
pub(crate) fn press_shift(
    on_action: &EventHandler<UiAction>,
    offers: &UiOfferTree,
    surface: &UiPatchSurface,
    output: NodeId,
    window: (u32, u32),
    delta: i32,
) -> bool {
    press_node_verb(
        on_action,
        offers,
        surface,
        output,
        PATCH_SHIFT_PORT_VERB,
        OfferArgs::new()
            .with(PATCH_START_PARAM, window.0.to_string())
            .with(PATCH_LAMPS_PARAM, window.1.to_string())
            .with(PATCH_DELTA_PARAM, delta.to_string()),
    )
}

/// The next free wire lamp on a port (after its last occupied cell).
pub(crate) fn port_next_free(output: &UiPatchSurfaceOutput, port_key: u32) -> Option<u32> {
    let port = output.bay.ports.iter().find(|port| port.key == port_key)?;
    let last = port
        .cells
        .iter()
        .map(|cell| cell.wire_start + cell.lamps)
        .max()
        .unwrap_or(port.start);
    Some(last.max(port.start))
}

/// One instance row's selection target: the D46 path when the object has a
/// sticky id, else the honest RANGE the same lamps occupy (an id-less
/// strand displays and selects, it just cannot be addressed — the
/// grain-robustness ruling).
pub(crate) fn instance_target(node: NodeId, instance: &UiPatchInstance) -> UiPatchTarget {
    if instance.path.is_empty() {
        UiPatchTarget::Range {
            node,
            start: instance.start,
            count: Some(instance.lamps),
        }
    } else {
        UiPatchTarget::Instance {
            node,
            path: instance.path.clone(),
        }
    }
}

/// A port's FREE runs in wire numbering — the gaps between its cells,
/// merged (runs may overlap) and clipped to the port's own span. The
/// walk-up surface's raw material: every free segment is a window inside
/// exactly one of these.
pub(crate) fn free_runs(port: &UiPatchPort) -> Vec<(u32, u32)> {
    let port_end = port.start.saturating_add(port.lamps);
    let mut occupied: Vec<(u32, u32)> = port
        .cells
        .iter()
        .map(|cell| {
            let start = cell.wire_start.clamp(port.start, port_end);
            let end = cell
                .wire_start
                .saturating_add(cell.lamps)
                .clamp(start, port_end);
            (start, end)
        })
        .filter(|(start, end)| end > start)
        .collect();
    occupied.sort_unstable();
    let mut runs = Vec::new();
    let mut cursor = port.start;
    for (start, end) in occupied {
        if start > cursor {
            runs.push((cursor, start - cursor));
        }
        cursor = cursor.max(end);
    }
    if port_end > cursor {
        runs.push((cursor, port_end - cursor));
    }
    runs
}

/// How many lamps the next object waiting for a wire wants, in surface
/// order: the first instance no run places, or a range-grain fixture with
/// no runs at all. This is what sizes a free segment — the walk-up doc's
/// improvement on lp2014's user-set chunk number (P4's panel names the
/// object itself; sizing only needs its count).
///
/// MANUAL fixtures only (Q11). An auto-mapped fixture places its own unnamed
/// lamps, so none of its objects is waiting for anything — sizing a free
/// segment by one would draw a window for a link the surface never offers.
/// With no manual fixture on the surface, a free segment simply takes its
/// whole run.
pub(crate) fn next_unmapped_lamps(surface: &UiPatchSurface) -> Option<u32> {
    surface
        .fixtures
        .iter()
        .filter(|f| f.manual_flow)
        .find_map(|fixture| {
            if fixture.instances.is_empty() {
                (fixture.patch.cells.is_empty() && fixture.patch.lamps > 0)
                    .then_some(fixture.patch.lamps)
            } else {
                fixture
                    .instances
                    .iter()
                    .find(|instance| !instance.placed)
                    .map(|instance| instance.lamps)
            }
        })
}

/// Is this fixture-side target still unmapped? The assign arm's
/// precondition on both ends — and the guard behind "mapped things always
/// plain-reselect" (the ruling: a click on something already on a wire
/// never steals a pending link).
pub(crate) fn target_is_unmapped(surface: &UiPatchSurface, target: &UiPatchTarget) -> bool {
    let fixture = |node: &NodeId| surface.fixtures.iter().find(|f| f.node == *node);
    match target {
        UiPatchTarget::Fixture { node } => fixture(node).is_some_and(|fixture| {
            if fixture.instances.is_empty() {
                fixture.patch.cells.is_empty()
            } else {
                fixture.instances.iter().any(|instance| !instance.placed)
            }
        }),
        UiPatchTarget::Instance { node, path } => fixture(node)
            .and_then(|fixture| {
                fixture
                    .instances
                    .iter()
                    .find(|instance| instance.path == *path)
            })
            .is_some_and(|instance| !instance.placed),
        UiPatchTarget::Range { node, start, count } => fixture(node).is_some_and(|fixture| {
            if fixture.instances.is_empty() {
                return fixture.patch.cells.is_empty();
            }
            let end = count.map_or(u32::MAX, |count| start.saturating_add(count));
            fixture
                .instances
                .iter()
                .filter(|instance| {
                    instance.start < end && instance.start.saturating_add(instance.lamps) > *start
                })
                .any(|instance| !instance.placed)
        }),
        // Wire-side targets and context levels are not objects.
        _ => false,
    }
}

/// How many lamps a free segment takes inside `available`: the size the
/// user nudged (the override `m` keeps), else the next unmapped object's
/// count, else the whole run — never zero, never past the run.
pub(crate) fn segment_size(
    surface: &UiPatchSurface,
    size_override: Option<u32>,
    available: u32,
) -> u32 {
    let wanted = size_override
        .or_else(|| next_unmapped_lamps(surface))
        .unwrap_or(available);
    wanted.clamp(1, available.max(1))
}

/// The free segment a click on free port space selects: the run's own
/// start (lp2014 grounding — the chunk begins where the free space
/// begins), sized by [`segment_size`].
pub(crate) fn segment_at_free_run(
    surface: &UiPatchSurface,
    node: NodeId,
    port: u32,
    run: (u32, u32),
    size_override: Option<u32>,
) -> UiPatchTarget {
    UiPatchTarget::Segment {
        node,
        port,
        start: run.0,
        lamps: segment_size(surface, size_override, run.1),
    }
}

/// Where an `m` scan starts: the selected output's ports, from the current
/// position. A fixture-side (or absent) selection has no output of its own,
/// so the scan starts at the surface's first port — v1 never hops outputs
/// (D3), it only ever scans ONE.
fn scan_origin(
    surface: &UiPatchSurface,
    from: Option<&UiPatchTarget>,
) -> Option<(NodeId, u32, u32)> {
    match from {
        Some(UiPatchTarget::Segment {
            node,
            port,
            start,
            lamps,
        }) => Some((*node, *port, start.saturating_add(*lamps))),
        Some(UiPatchTarget::Port { node, port }) => Some((*node, *port, 0)),
        Some(UiPatchTarget::Output { node }) => {
            let output = surface.outputs.iter().find(|output| output.node == *node)?;
            Some((*node, output.bay.ports.first()?.key, 0))
        }
        _ => {
            let output = surface
                .outputs
                .iter()
                .find(|output| !output.bay.ports.is_empty())?;
            Some((output.node, output.bay.ports[0].key, 0))
        }
    }
}

/// `m` (D3): the next free segment on the SELECTED OUTPUT — its ports in
/// order from the current position, wrapping once back through the port it
/// started on, sized by the next unmapped object or the kept override. No
/// output hop in v1.
pub(crate) fn next_free_segment(
    surface: &UiPatchSurface,
    from: Option<&UiPatchTarget>,
    size_override: Option<u32>,
) -> Option<UiPatchTarget> {
    let (node, port_key, from_lamp) = scan_origin(surface, from)?;
    let output = surface.outputs.iter().find(|output| output.node == node)?;
    let ports = &output.bay.ports;
    if ports.is_empty() {
        return None;
    }
    let index = ports
        .iter()
        .position(|port| port.key == port_key)
        .unwrap_or(0);
    for offset in 0..=ports.len() {
        let port = &ports[(index + offset) % ports.len()];
        // The first pass skips what is behind the cursor; the extra
        // wrapping pass over the starting port picks those runs back up.
        let floor = if offset == 0 { from_lamp } else { port.start };
        for (start, lamps) in free_runs(port) {
            let end = start.saturating_add(lamps);
            let start = start.max(floor);
            if end <= start {
                continue;
            }
            return Some(UiPatchTarget::Segment {
                node,
                port: port.key,
                start,
                lamps: segment_size(surface, size_override, end - start),
            });
        }
    }
    None
}

/// The free run a segment sits in, when its port still exists.
fn segment_run(
    surface: &UiPatchSurface,
    node: NodeId,
    port_key: u32,
    start: u32,
) -> Option<(u32, u32)> {
    let output = surface.outputs.iter().find(|output| output.node == node)?;
    let port = output.bay.ports.iter().find(|port| port.key == port_key)?;
    free_runs(port)
        .into_iter()
        .find(|(run_start, run_lamps)| start >= *run_start && start < run_start + run_lamps)
}

/// `[` / `]`: walk a free segment one lamp along its OWN free run.
/// Selection only — a window nudge is not a doc write (plan scope).
pub(crate) fn shift_segment(
    surface: &UiPatchSurface,
    target: &UiPatchTarget,
    delta: i32,
) -> Option<UiPatchTarget> {
    let UiPatchTarget::Segment {
        node,
        port,
        start,
        lamps,
    } = target
    else {
        return None;
    };
    let (run_start, run_lamps) = segment_run(surface, *node, *port, *start)?;
    let last = run_start
        .saturating_add(run_lamps)
        .saturating_sub(*lamps)
        .max(run_start);
    let next = (i64::from(*start) + i64::from(delta)).clamp(i64::from(run_start), i64::from(last));
    Some(UiPatchTarget::Segment {
        node: *node,
        port: *port,
        start: next as u32,
        lamps: *lamps,
    })
}

/// `-` / `=`: narrow or widen a free segment (min one lamp, max its free
/// run) — the size override the ruling keeps across `m`.
pub(crate) fn resize_segment(
    surface: &UiPatchSurface,
    target: &UiPatchTarget,
    delta: i32,
) -> Option<UiPatchTarget> {
    let UiPatchTarget::Segment {
        node,
        port,
        start,
        lamps,
    } = target
    else {
        return None;
    };
    let (run_start, run_lamps) = segment_run(surface, *node, *port, *start)?;
    let room = run_start
        .saturating_add(run_lamps)
        .saturating_sub(*start)
        .max(1);
    let next = (i64::from(*lamps) + i64::from(delta)).clamp(1, i64::from(room));
    Some(UiPatchTarget::Segment {
        node: *node,
        port: *port,
        start: *start,
        lamps: next as u32,
    })
}

/// Complete an armed ASSIGN: put `object` on `output` at wire `lamp` by
/// pressing the object's fixture's `assign` offer with both picks — one
/// write and one undo step like every other verb. An unnamed output gets
/// its numeric default alongside the write (D39): core's offer carries
/// that, because a link the user explicitly armed still has to name the
/// wire it lands on. Returns false when nothing was pressed (the object
/// names no subject, or the offer refused).
pub(crate) fn press_assign(
    on_action: &EventHandler<UiAction>,
    offers: &UiOfferTree,
    surface: &UiPatchSurface,
    object: &UiPatchTarget,
    output: &UiPatchSurfaceOutput,
    lamp: u32,
) -> bool {
    press_patch_offer(
        on_action,
        offers,
        assign_press(surface, object, output, lamp),
    )
}

/// The press that completes an armed ASSIGN ([`press_assign`]): the
/// object's fixture's `assign`, with the object, the output and the lamp.
pub(crate) fn assign_press(
    surface: &UiPatchSurface,
    object: &UiPatchTarget,
    output: &UiPatchSurfaceOutput,
    lamp: u32,
) -> Option<PatchPress> {
    subject_verb_press(
        surface,
        Some(object),
        PATCH_ASSIGN_VERB,
        OfferArgs::new()
            .with(PATCH_OUTPUT_PARAM, output.patch_output_value()?)
            .with(PATCH_LAMP_PARAM, lamp.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{
        UiFixturePatch, UiPatchBay, UiPatchCell, UiPatchSurfaceFixture, UiPatchSurfaceOutput,
    };

    fn output_node() -> NodeId {
        NodeId::new(10)
    }

    fn dome_node() -> NodeId {
        NodeId::new(2)
    }

    fn cell(id: &str, source_start: u32, wire_start: u32, lamps: u32) -> UiPatchCell {
        UiPatchCell {
            id: id.to_string(),
            producer: "dome".to_string(),
            source_start,
            lamps,
            wire_start,
            ..Default::default()
        }
    }

    fn port(key: u32, start: u32, lamps: u32, cells: Vec<UiPatchCell>) -> UiPatchPort {
        UiPatchPort {
            key,
            pin_label: format!("IO{key}"),
            start,
            lamps,
            cells,
        }
    }

    fn instance(path: &str, start: u32, lamps: u32, placed: bool) -> UiPatchInstance {
        UiPatchInstance {
            path: path.to_string(),
            label: path.to_string(),
            start,
            lamps,
            stride: 1,
            placed,
        }
    }

    /// One 60-lamp port with the dome's first sector on its front half:
    /// sector 1 is placed, sector 2 is the next object waiting for a wire.
    fn half_patched_surface() -> UiPatchSurface {
        UiPatchSurface {
            fixtures: vec![UiPatchSurfaceFixture {
                node: dome_node(),
                label: "dome".to_string(),
                // MANUAL: the walk-up grammar's own mode (Q11). An
                // auto-mapped fixture places its own objects, so none of
                // them ever sizes a segment or fills a picker.
                manual_flow: true,
                patch: UiFixturePatch {
                    lamps: 60,
                    cells: vec![cell("2:0", 0, 0, 30)],
                    ..Default::default()
                },
                instances: vec![
                    instance("/sector/1", 0, 30, true),
                    instance("/sector/2", 30, 30, false),
                ],
                ..Default::default()
            }],
            outputs: vec![UiPatchSurfaceOutput {
                node: output_node(),
                label: "out_a".to_string(),
                bay: UiPatchBay {
                    ports: vec![port(0, 0, 60, vec![cell("2:0", 0, 0, 30)])],
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// Two ports, the first partly taken — the shape `m` walks.
    fn two_port_surface() -> UiPatchSurface {
        let mut surface = half_patched_surface();
        surface.outputs[0].bay.ports = vec![
            port(0, 0, 40, vec![cell("2:0", 0, 0, 10)]),
            port(1, 40, 20, Vec::new()),
        ];
        surface
    }

    /// The arm grammar's second click is a press of core's offer with BOTH
    /// picks: the armed object (or port) and the clicked counterpart, bound
    /// by the offer core published for that surface — the same press the
    /// app agent makes by path.
    #[test]
    fn the_arms_second_click_presses_the_offer_with_both_picks() {
        use lpa_studio_core::{
            ArtifactLocation, PATCH_REVERSE_VERB, PatchVerbKind, PatchVerbOp,
            publish_patch_verb_offers,
        };

        let mut surface = half_patched_surface();
        surface.fixtures[0].address = Some("/demo.module/dome.fixture".to_string());
        surface.fixtures[0].patch_artifact = Some(ArtifactLocation::file("/dome.patch.json"));
        surface.outputs[0].address = Some("/demo.module/out_a.output".to_string());
        surface.outputs[0].name = Some("1".to_string());
        surface.outputs[0]
            .bay
            .ports
            .push(port(1, 60, 20, Vec::new()));
        let mut offers = UiOfferTree::new();
        publish_patch_verb_offers(&mut offers, &surface, None, false, false);
        let op = |press: Option<PatchPress>| {
            let (path, args) = press.expect("a press");
            bind_patch_press(&offers, &path, &args)
                .expect("the offer binds both picks")
                .op_as::<PatchVerbOp>()
                .expect("a patch verb")
                .clone()
        };

        // Assign: the armed object, then the clicked wire at its lamp.
        let object = UiPatchTarget::Instance {
            node: dome_node(),
            path: "/sector/2".to_string(),
        };
        let (path, _) = assign_press(&surface, &object, &surface.outputs[0], 30).unwrap();
        assert_eq!(
            path.to_string(),
            "project/demo.module/dome.fixture/patch/assign"
        );
        let assign = op(assign_press(&surface, &object, &surface.outputs[0], 30));
        assert_eq!(assign.subject.path.as_deref(), Some("/sector/2"));
        assert_eq!(
            assign.verb,
            PatchVerbKind::Assign {
                output_name: Some("1".to_string()),
                lamp: 30,
            }
        );

        // Swap: the armed port, then the clicked one.
        let swap = op(swap_press(
            &surface,
            (output_node(), 0),
            &surface.outputs[0],
            1,
        ));
        let PatchVerbKind::SwapPorts { a, b } = swap.verb else {
            panic!("a swap");
        };
        assert_eq!((a.start, a.lamps, b.start, b.lamps), (0, 60, 60, 20));

        // A selection that names no object presses nothing.
        assert!(
            subject_verb_press(
                &surface,
                Some(&UiPatchTarget::Port {
                    node: output_node(),
                    port: 0,
                }),
                PATCH_REVERSE_VERB,
                OfferArgs::new(),
            )
            .is_none()
        );
    }

    /// The gaps between cells, in wire numbering — clipped to the port and
    /// merged, so overlapping runs never open a phantom hole between them.
    #[test]
    fn free_runs_are_the_gaps_between_cells() {
        assert_eq!(
            free_runs(&port(0, 0, 60, vec![cell("a", 0, 10, 10)])),
            [(0, 10), (20, 40)],
            "a cell in the middle leaves a run on each side"
        );
        assert_eq!(
            free_runs(&port(0, 0, 60, Vec::new())),
            [(0, 60)],
            "an empty port is one free run"
        );
        assert_eq!(
            free_runs(&port(
                0,
                0,
                30,
                vec![cell("a", 0, 0, 20), cell("b", 0, 10, 20)],
            )),
            [],
            "overlapping (contested) cells still cover the port"
        );
        assert_eq!(
            free_runs(&port(0, 40, 20, vec![cell("a", 0, 30, 20)])),
            [(50, 10)],
            "a cell reaching in from the previous port is clipped, not counted twice"
        );
    }

    /// The walk-up sizing rule: a free segment takes the next unmapped
    /// object's lamps, the user's override beats it, and neither ever runs
    /// past the free space it was drawn on.
    #[test]
    fn a_free_segment_is_sized_by_the_next_unmapped_object() {
        let surface = half_patched_surface();
        assert_eq!(next_unmapped_lamps(&surface), Some(30), "sector 2 is next");

        let run = free_runs(&surface.outputs[0].bay.ports[0])[0];
        assert_eq!(run, (30, 30));
        assert_eq!(
            segment_at_free_run(&surface, output_node(), 0, run, None),
            UiPatchTarget::Segment {
                node: output_node(),
                port: 0,
                start: 30,
                lamps: 30,
            },
            "the segment starts where the free run does and takes sector 2's count"
        );
        assert_eq!(
            segment_at_free_run(&surface, output_node(), 0, run, Some(8)),
            UiPatchTarget::Segment {
                node: output_node(),
                port: 0,
                start: 30,
                lamps: 8,
            },
            "the nudged override wins"
        );
        assert_eq!(
            segment_size(&surface, Some(500), 30),
            30,
            "no override reaches past the free run"
        );
        assert_eq!(segment_size(&surface, Some(0), 30), 1, "never zero lamps");

        // Everything mapped: nothing sizes the segment but the run itself.
        let mut all_placed = half_patched_surface();
        all_placed.fixtures[0].instances[1].placed = true;
        assert_eq!(next_unmapped_lamps(&all_placed), None);
        assert_eq!(segment_size(&all_placed, None, 30), 30);
    }

    /// The arm's precondition, at every grain the tree can name.
    #[test]
    fn only_unmapped_objects_are_assignable() {
        let surface = half_patched_surface();
        let unmapped = |target: UiPatchTarget| target_is_unmapped(&surface, &target);

        assert!(unmapped(UiPatchTarget::Instance {
            node: dome_node(),
            path: "/sector/2".to_string(),
        }));
        assert!(!unmapped(UiPatchTarget::Instance {
            node: dome_node(),
            path: "/sector/1".to_string(),
        }));
        assert!(
            unmapped(UiPatchTarget::Fixture { node: dome_node() }),
            "a fixture with any unplaced instance still wants a wire"
        );
        assert!(
            unmapped(UiPatchTarget::Range {
                node: dome_node(),
                start: 30,
                count: Some(30),
            }),
            "the range grain answers from the instances it covers"
        );
        assert!(!unmapped(UiPatchTarget::Range {
            node: dome_node(),
            start: 0,
            count: Some(30),
        }));
        assert!(
            !unmapped(UiPatchTarget::Port {
                node: output_node(),
                port: 0,
            }),
            "wire-side targets are not objects"
        );
        assert!(
            !unmapped(UiPatchTarget::Instance {
                node: dome_node(),
                path: "/gone".to_string(),
            }),
            "an unknown path assigns nothing"
        );

        // A range-grain fixture (the peach): no instances, so its runs
        // answer instead.
        let mut peach = half_patched_surface();
        peach.fixtures[0].instances.clear();
        assert!(!target_is_unmapped(
            &peach,
            &UiPatchTarget::Fixture { node: dome_node() }
        ));
        peach.fixtures[0].patch.cells.clear();
        assert!(target_is_unmapped(
            &peach,
            &UiPatchTarget::Fixture { node: dome_node() }
        ));
        assert_eq!(next_unmapped_lamps(&peach), Some(60), "the whole fixture");
    }

    /// `m` (D3): the selected output's ports in order from the cursor,
    /// wrapping once, and never hopping to another output.
    #[test]
    fn m_advances_through_the_outputs_ports_and_wraps() {
        let surface = two_port_surface();
        let segment = |port: u32, start: u32, lamps: u32| UiPatchTarget::Segment {
            node: output_node(),
            port,
            start,
            lamps,
        };

        // From the front of the first port's free run, sized by sector 2
        // (30 lamps — exactly the run).
        assert_eq!(
            next_free_segment(&surface, Some(&segment(0, 10, 5)), None),
            Some(segment(0, 15, 25)),
            "the cursor advances inside the run, clipped to what is left"
        );
        assert_eq!(
            next_free_segment(&surface, Some(&segment(0, 10, 30)), None),
            Some(segment(1, 40, 20)),
            "a full run advances across the port boundary"
        );
        assert_eq!(
            next_free_segment(&surface, Some(&segment(1, 40, 20)), None),
            Some(segment(0, 10, 30)),
            "and wraps back to the first port"
        );
        assert_eq!(
            next_free_segment(&surface, Some(&segment(0, 10, 30)), Some(4)),
            Some(segment(1, 40, 4)),
            "the size override rides along"
        );
        assert_eq!(
            next_free_segment(
                &surface,
                Some(&UiPatchTarget::Port {
                    node: output_node(),
                    port: 1,
                }),
                None,
            ),
            Some(segment(1, 40, 20)),
            "a selected port starts the scan at its own first free run"
        );
        assert_eq!(
            next_free_segment(
                &surface,
                Some(&UiPatchTarget::Fixture { node: dome_node() }),
                None,
            ),
            Some(segment(0, 10, 30)),
            "with no wire-side selection the scan starts at the first port"
        );

        // A full output has nothing to advance to.
        let mut full = two_port_surface();
        full.outputs[0].bay.ports = vec![port(0, 0, 10, vec![cell("2:0", 0, 0, 10)])];
        assert_eq!(
            next_free_segment(&full, Some(&segment(0, 0, 10)), None),
            None,
        );
    }

    /// The nudges move a WINDOW, never a patch: both stay inside the free
    /// run the segment was drawn on.
    #[test]
    fn segment_nudges_stay_inside_their_free_run() {
        let surface = two_port_surface();
        let segment = |start: u32, lamps: u32| UiPatchTarget::Segment {
            node: output_node(),
            port: 0,
            start,
            lamps,
        };
        // The port's free run is 10..40.
        assert_eq!(
            shift_segment(&surface, &segment(10, 10), 1),
            Some(segment(11, 10))
        );
        assert_eq!(
            shift_segment(&surface, &segment(10, 10), -1),
            Some(segment(10, 10)),
            "the run's start is the floor"
        );
        assert_eq!(
            shift_segment(&surface, &segment(30, 10), 1),
            Some(segment(30, 10)),
            "and its end is the ceiling"
        );
        assert_eq!(
            resize_segment(&surface, &segment(10, 10), 1),
            Some(segment(10, 11))
        );
        assert_eq!(
            resize_segment(&surface, &segment(10, 10), -20),
            Some(segment(10, 1)),
            "one lamp is the minimum"
        );
        assert_eq!(
            resize_segment(&surface, &segment(10, 30), 5),
            Some(segment(10, 30)),
            "and the free run is the maximum"
        );
        assert_eq!(
            shift_segment(&surface, &UiPatchTarget::Fixture { node: dome_node() }, 1),
            None,
            "nudges only apply to segments"
        );
        assert_eq!(
            resize_segment(
                &surface,
                &UiPatchTarget::Segment {
                    node: output_node(),
                    port: 9,
                    start: 0,
                    lamps: 4,
                },
                1,
            ),
            None,
            "a segment on a port the surface no longer has nudges nothing"
        );
    }

    /// An id-less strand selects at range grain — the same target the tree
    /// row builds, so the arm's two ends agree about what was clicked.
    #[test]
    fn instance_targets_follow_addressability() {
        assert_eq!(
            instance_target(dome_node(), &instance("/sector/2", 30, 30, false)),
            UiPatchTarget::Instance {
                node: dome_node(),
                path: "/sector/2".to_string(),
            }
        );
        assert_eq!(
            instance_target(dome_node(), &instance("", 30, 30, false)),
            UiPatchTarget::Range {
                node: dome_node(),
                start: 30,
                count: Some(30),
            }
        );
    }
}
