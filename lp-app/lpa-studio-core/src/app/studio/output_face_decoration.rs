//! Studio-level decoration of output-node faces.
//!
//! The face builder derives from a node's own projected sections and never
//! reads controller state, but an output face needs two facts that are not
//! in those sections:
//!
//! 1. **Which board the running device is.** Board identity lives on the
//!    device registry (`RegisteredDevice.board_id`, stamped at provisioning);
//!    the wire's `HardwareFacts.board_id` is always `None` today. A device
//!    provisioned outside Studio has none at all, which is why "no board
//!    known" is a first-class face state and not an error.
//! 2. **How many lamps the node's control input carries.** The extent lives
//!    on the UPSTREAM node's produced control product, one card over — the
//!    output node itself publishes no produced values. Without it the
//!    remainder channel (the count-less one, which is EVERY single-channel
//!    output migrated from format 2) could not say how many lamps it drives.
//!
//! Both are filled here, in the same shape as
//! [`crate::AgentController::decorate_editor_view`]: the studio controller
//! walks the finished project editor DTO once and injects what the project
//! walk could not see. Endpoint↔pin translation belongs to this layer too —
//! the web renders the board diagram, it does not parse endpoint specs.

use lpa_boards::board_by_id;
use lpc_wire::server::OutputWireStatus;

use crate::{
    ProjectEditorView, UiLedBudget, UiNodeChild, UiNodeFace, UiNodeSection, UiNodeTabBody,
    UiOutputBoardFacts, UiOutputFace, UiOutputPin, UiProducedProduct, UiProductPreview,
    UiProductRef, UiWireStatus,
};

/// Samples per lamp on the control bus — the engine's own slice unit
/// (`lpc_engine`'s `planned_wires`: "a channel's `count` is in lamps and a
/// lamp is three samples").
const SAMPLES_PER_LAMP: u32 = 3;

/// Fill every [`UiNodeFace::Output`] in `editor` with the facts its builder
/// could not see: the board (when `board_id` names one Studio ships a
/// display manifest for) and the incoming lamp extent + span boundaries
/// (from whichever node in the same project publishes the control product
/// this output is bound to).
pub(crate) fn decorate_output_faces(
    editor: &mut ProjectEditorView,
    board_id: Option<&str>,
    wire_status: Option<&[OutputWireStatus]>,
    total_led_budget: Option<u32>,
) {
    let board = board_id.and_then(board_facts);
    let upstreams = control_sources(&editor.nodes);
    for node in &mut editor.nodes {
        decorate_node(node, board.as_ref(), &upstreams);
    }
    // Second pass, after counts are resolved: live wire status (joined by
    // GPIO — the one key both the heartbeat and the authored rows know) and
    // the board-wide LED budget bar (every face on the device shows the same
    // used total, so it is summed across all faces first).
    let used: u32 = fold_faces(&mut editor.nodes, 0, &mut |used, face| {
        used + face
            .channels
            .iter()
            .map(|row| row.resolved_count.or(row.count).unwrap_or(0))
            .sum::<u32>()
    });
    let budget = total_led_budget.map(|budget| UiLedBudget { used, budget });
    for node in &mut editor.nodes {
        apply_live_facts(node, wire_status, budget.as_ref());
    }
}

/// Fold over every output face in the tree (nodes and children alike).
fn fold_faces<T: Copy>(
    nodes: &mut [crate::UiNodeView],
    init: T,
    fold: &mut impl FnMut(T, &UiOutputFace) -> T,
) -> T {
    fn node_faces<T: Copy>(
        node: &mut crate::UiNodeView,
        acc: T,
        fold: &mut impl FnMut(T, &UiOutputFace) -> T,
    ) -> T {
        let mut acc = acc;
        if let Some(UiNodeFace::Output(face)) = node.face.as_ref() {
            acc = fold(acc, face);
        }
        for child in &mut node.children {
            acc = child_faces(child, acc, fold);
        }
        acc
    }
    fn child_faces<T: Copy>(
        child: &mut UiNodeChild,
        acc: T,
        fold: &mut impl FnMut(T, &UiOutputFace) -> T,
    ) -> T {
        let mut acc = acc;
        if let Some(UiNodeFace::Output(face)) = child.face.as_ref() {
            acc = fold(acc, face);
        }
        for child in &mut child.children {
            acc = child_faces(child, acc, fold);
        }
        acc
    }
    let mut acc = init;
    for node in nodes {
        acc = node_faces(node, acc, fold);
    }
    acc
}

/// Inject the live per-wire status (by GPIO) and the budget bar into every
/// output face under `node`.
fn apply_live_facts(
    node: &mut crate::UiNodeView,
    wire_status: Option<&[OutputWireStatus]>,
    budget: Option<&UiLedBudget>,
) {
    fn apply_face(
        face: &mut UiOutputFace,
        wire_status: Option<&[OutputWireStatus]>,
        budget: Option<&UiLedBudget>,
    ) {
        face.led_budget = budget.cloned();
        let Some(status) = wire_status else {
            return;
        };
        for row in &mut face.channels {
            let Some(gpio) = row.gpio else {
                continue;
            };
            row.wire_status = status
                .iter()
                .find(|wire| u32::from(wire.gpio) == gpio)
                .map(|wire| UiWireStatus {
                    sent: wire.sent,
                    torn: wire.torn,
                    waves: wire.waved > 0,
                    queue_wait_ms: wire.queue_wait_max_us.div_ceil(1_000),
                });
        }
    }
    fn apply_child(
        child: &mut UiNodeChild,
        wire_status: Option<&[OutputWireStatus]>,
        budget: Option<&UiLedBudget>,
    ) {
        if let Some(UiNodeFace::Output(face)) = child.face.as_mut() {
            apply_face(face, wire_status, budget);
        }
        for child in &mut child.children {
            apply_child(child, wire_status, budget);
        }
    }
    if let Some(UiNodeFace::Output(face)) = node.face.as_mut() {
        apply_face(face, wire_status, budget);
    }
    for child in &mut node.children {
        apply_child(child, wire_status, budget);
    }
}

/// A produced control product as an output's decoration needs it: the bus
/// label it publishes to, its lamp extent, and its per-path span starts.
#[derive(Clone, Debug, Default, PartialEq)]
struct ControlSource {
    bus_label: String,
    lamps: u32,
    span_boundaries: Vec<u32>,
}

fn decorate_node(
    node: &mut crate::UiNodeView,
    board: Option<&UiOutputBoardFacts>,
    upstreams: &[ControlSource],
) {
    if let Some(UiNodeFace::Output(face)) = node.face.as_mut() {
        decorate_face(face, board, upstreams);
    }
    for child in &mut node.children {
        decorate_child(child, board, upstreams);
    }
}

fn decorate_child(
    child: &mut UiNodeChild,
    board: Option<&UiOutputBoardFacts>,
    upstreams: &[ControlSource],
) {
    if let Some(UiNodeFace::Output(face)) = child.face.as_mut() {
        decorate_face(face, board, upstreams);
    }
    for child in &mut child.children {
        decorate_child(child, board, upstreams);
    }
}

/// One output face: board facts (with each wire's GPIO resolved and each
/// eligible pin's assignment marked) plus the upstream extent.
fn decorate_face(
    face: &mut UiOutputFace,
    board: Option<&UiOutputBoardFacts>,
    upstreams: &[ControlSource],
) {
    if let Some(board) = board {
        let mut board = board.clone();
        for channel in &mut face.channels {
            channel.gpio = board
                .pins
                .iter()
                .find(|pin| pin.label == channel.pin_label)
                .map(|pin| pin.gpio);
        }
        // Lowest channel key wins a contested pin: two wires on one pin is
        // an authoring mistake, and the assignment marking must be a
        // function of the map, not of iteration luck.
        for pin in &mut board.pins {
            pin.assigned_to = face
                .channels
                .iter()
                .find(|channel| channel.pin_label == pin.label)
                .map(|channel| channel.key);
        }
        face.board = Some(board);
    }

    let Some(binding) = face.input_binding.as_deref() else {
        return;
    };
    let Some(source) = upstreams.iter().find(|source| source.bus_label == binding) else {
        return;
    };
    face.span_boundaries = source.span_boundaries.clone();
    face.resolve_extent(source.lamps);
}

/// Every produced control product in the project, keyed by the bus label it
/// publishes to — the join an output's bound `input` row addresses.
fn control_sources(nodes: &[crate::UiNodeView]) -> Vec<ControlSource> {
    let mut sources = Vec::new();
    for node in nodes {
        for tab in &node.tabs {
            if let UiNodeTabBody::Sections(sections) = &tab.body {
                collect_sources(sections, &mut sources);
            }
        }
        collect_child_sources(&node.children, &mut sources);
    }
    sources
}

fn collect_child_sources(children: &[UiNodeChild], sources: &mut Vec<ControlSource>) {
    for child in children {
        collect_sources(&child.sections, sources);
        collect_child_sources(&child.children, sources);
    }
}

fn collect_sources(sections: &[UiNodeSection], sources: &mut Vec<ControlSource>) {
    for section in sections {
        let UiNodeSection::ProducedProducts(products) = section else {
            continue;
        };
        sources.extend(products.iter().filter_map(control_source));
    }
}

/// One produced product → its control source, when it is a control product
/// published to a bus.
fn control_source(product: &UiProducedProduct) -> Option<ControlSource> {
    let Some(UiProductRef::Control {
        rows,
        samples_per_row,
        ..
    }) = product.product
    else {
        return None;
    };
    let bus_label = product
        .binding
        .bindings
        .bus_target
        .as_ref()
        .map(|endpoint| endpoint.label.clone())?;
    Some(ControlSource {
        // Slices are flat (D1): the buffer stays one row, so the extent is
        // simply every sample it carries.
        lamps: rows.saturating_mul(samples_per_row) / SAMPLES_PER_LAMP,
        span_boundaries: span_boundaries(product),
        bus_label,
    })
}

/// The lamp indices the upstream's authored paths start at (P2's honest
/// spans: one `ControlSampleSpan` per authored fixture path), ascending and
/// deduplicated.
///
/// Empty when the product has no materialized preview yet — the sample
/// layout rides the probe result, so a card that has never previewed offers
/// no snapping grid. That is a missing convenience, not a broken face.
fn span_boundaries(product: &UiProducedProduct) -> Vec<u32> {
    let UiProductPreview::ControlNative(preview) = &product.preview else {
        return Vec::new();
    };
    let mut boundaries: Vec<u32> = preview
        .sample_layout
        .spans
        .iter()
        .map(|span| span.start / SAMPLES_PER_LAMP)
        .collect();
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

/// The board's display facts, when the id names a board Studio ships a
/// display manifest for. An unknown id degrades to "no board known" rather
/// than to a half-drawn diagram.
fn board_facts(board_id: &str) -> Option<UiOutputBoardFacts> {
    let board = board_by_id(board_id)?;
    // Rails AND screw terminals: the display manifest keeps terminals in
    // their own list (`hw.terminals`), so `pins()` alone would silently drop
    // every wire on a DIN-rail controller. The eligible set is the union —
    // the same rule the board-editor lint applies.
    let pins = board
        .pins()
        .map(|pin| (&pin.label, pin.role, pin.gpio))
        .chain(
            board
                .hw
                .terminals
                .iter()
                .map(|terminal| (&terminal.label, terminal.role, terminal.gpio)),
        )
        .filter(|(_, role, _)| role.output_eligible())
        .filter_map(|(label, _, gpio)| {
            Some(UiOutputPin {
                label: label.clone(),
                gpio: u32::from(gpio?),
                assigned_to: None,
            })
        })
        .collect();
    Some(UiOutputBoardFacts {
        board_id: board.board_id.clone(),
        display_name: board.display_name.clone(),
        pins,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        UiBindingEndpoint, UiNodeHeader, UiNodeTab, UiNodeView, UiOutputChannelRow,
        UiProducedBinding, UiProducedBindings, UiProductKind,
    };

    #[test]
    fn a_known_board_resolves_pins_and_marks_assignments() {
        let mut editor =
            editor_with_output(output_face(&[(0, "IO18", Some(100)), (1, "IO2", None)]));

        decorate_output_faces(&mut editor, Some("domraem/dom-z-102"), None, None);

        let face = output_face_of(&editor);
        let board = face.board.as_ref().expect("the desk board is known");
        assert_eq!(board.board_id, "domraem/dom-z-102");
        assert_eq!(
            face.channels[0].gpio,
            Some(18),
            "IO18 translates to gpio 18 through the display manifest"
        );
        assert_eq!(face.channels[1].gpio, Some(2));
        // The DOM-Z-102's LED wires are screw terminals on the rails; every
        // io-role one is offered, and only the two in use are assigned.
        let labels: Vec<&str> = board.pins.iter().map(|pin| pin.label.as_str()).collect();
        assert_eq!(labels, ["IO13", "IO18", "IO16", "IO14", "IO2"]);
        let assigned: Vec<(&str, Option<u32>)> = board
            .pins
            .iter()
            .map(|pin| (pin.label.as_str(), pin.assigned_to))
            .collect();
        assert_eq!(
            assigned,
            [
                ("IO13", None),
                ("IO18", Some(0)),
                ("IO16", None),
                ("IO14", None),
                ("IO2", Some(1)),
            ]
        );
    }

    #[test]
    fn screw_terminals_in_their_own_list_are_eligible_too() {
        // The dig-uno keeps its LED outputs in `hw.terminals`, which
        // `board.pins()` does not iterate.
        let mut editor = editor_with_output(output_face(&[(0, "LED2", None)]));

        decorate_output_faces(&mut editor, Some("quinled/dig-uno"), None, None);

        let face = output_face_of(&editor);
        let board = face.board.as_ref().expect("board known");
        assert!(
            board.pins.iter().any(|pin| pin.label == "LED1"),
            "terminal pins join the eligible set: {:?}",
            board.pins
        );
        assert_eq!(face.channels[0].gpio, Some(3), "LED2 is gpio 3");
    }

    #[test]
    fn no_board_known_leaves_the_face_editable_and_unresolved() {
        let mut editor = editor_with_output(output_face(&[(0, "IO18", None)]));

        decorate_output_faces(&mut editor, None, None, None);

        let face = output_face_of(&editor);
        assert_eq!(face.board, None, "no board known is a first-class state");
        assert_eq!(face.channels[0].gpio, None);
        assert_eq!(
            face.channels[0].endpoint_display, "ws281x:local:IO18",
            "the wire is still fully addressable without a board"
        );
    }

    #[test]
    fn an_unknown_board_id_degrades_to_no_board() {
        let mut editor = editor_with_output(output_face(&[(0, "IO18", None)]));

        decorate_output_faces(&mut editor, Some("acme/not-a-board"));

        assert_eq!(output_face_of(&editor).board, None);
    }

    #[test]
    fn a_label_the_board_lacks_stays_unresolved_but_shown() {
        let mut editor = editor_with_output(output_face(&[(0, "IO27", Some(50))]));

        decorate_output_faces(&mut editor, Some("domraem/dom-z-102"), None, None);

        let face = output_face_of(&editor);
        assert!(face.board.is_some(), "the board is still known");
        assert_eq!(face.channels[0].gpio, None, "IO27 is not on this board");
        assert_eq!(face.channels[0].pin_label, "IO27", "and it is still shown");
    }

    #[test]
    fn the_upstream_product_resolves_the_remainder_channel() {
        // 2 wires over a 16-lamp buffer: 6 authored, the rest to channel 1.
        let mut editor = editor_with_output(output_face(&[(0, "IO18", Some(6)), (1, "IO2", None)]));
        editor.nodes.push(fixture_node(16, &[0, 6]));

        decorate_output_faces(&mut editor, None, None, None);

        let face = output_face_of(&editor);
        assert_eq!(face.total_lamps, Some(16));
        assert_eq!(face.channels[0].resolved_count, Some(6));
        assert_eq!(
            face.channels[1].resolved_count,
            Some(10),
            "the count-less channel takes what is left"
        );
        assert_eq!(
            face.span_boundaries,
            vec![0, 6],
            "the upstream's authored paths become the snapping grid"
        );
    }

    #[test]
    fn an_overrun_remainder_resolves_to_zero_rather_than_underflowing() {
        let mut editor =
            editor_with_output(output_face(&[(0, "IO18", Some(40)), (1, "IO2", None)]));
        editor.nodes.push(fixture_node(16, &[]));

        decorate_output_faces(&mut editor, None, None, None);

        let face = output_face_of(&editor);
        assert_eq!(face.channels[1].resolved_count, Some(0));
    }

    #[test]
    fn an_unrelated_bus_leaves_the_extent_unknown() {
        let mut editor = editor_with_output(output_face(&[(0, "IO18", None)]));
        let mut fixture = fixture_node(16, &[]);
        set_bus_target(&mut fixture, "bus:other.out");
        editor.nodes.push(fixture);

        decorate_output_faces(&mut editor, None, None, None);

        let face = output_face_of(&editor);
        assert_eq!(face.total_lamps, None, "no upstream publishes this bus");
        assert_eq!(face.channels[0].resolved_count, None);
    }

    // -- fixtures ------------------------------------------------------------

    /// An output face as the builder leaves it: wires with their authored
    /// counts and slice starts, nothing hardware-side filled in.
    fn output_face(wires: &[(u32, &str, Option<u32>)]) -> UiOutputFace {
        let mut start = Some(0u32);
        let channels = wires
            .iter()
            .map(|(key, pin, count)| {
                let slice_start = start;
                match count {
                    Some(count) => start = start.map(|start| start + count),
                    None => start = None,
                }
                UiOutputChannelRow {
                    key: *key,
                    endpoint_display: format!("ws281x:local:{pin}"),
                    pin_label: (*pin).to_string(),
                    count: *count,
                    resolved_count: *count,
                    slice_start,
                    ..UiOutputChannelRow::default()
                }
            })
            .collect();
        UiOutputFace {
            channels,
            input_binding: Some("bus:control.out".to_string()),
            ..UiOutputFace::default()
        }
    }

    fn editor_with_output(face: UiOutputFace) -> ProjectEditorView {
        let mut node = UiNodeView::new(
            UiNodeHeader::new("output", "Output", "/demo.project/output.output"),
            Vec::new(),
        );
        node.face = Some(UiNodeFace::Output(face));
        ProjectEditorView::new(
            "demo",
            1,
            crate::ProjectSyncSummary::default(),
            Vec::new(),
            crate::ProjectNodeTreeView::new(Vec::new(), 0),
            vec![node],
        )
    }

    /// An upstream fixture publishing `lamps` lamps to `bus:control.out`,
    /// with one authored path per entry of `span_starts` (lamp indices).
    fn fixture_node(lamps: u32, span_starts: &[u32]) -> UiNodeView {
        let mut product = UiProducedProduct::control("Output");
        product.product = Some(UiProductRef::Control {
            node_id: 3,
            output: 0,
            rows: 1,
            samples_per_row: lamps * SAMPLES_PER_LAMP,
        });
        product.binding = UiProducedBinding {
            bindings: UiProducedBindings {
                bus_target: Some(UiBindingEndpoint::new("bus:control.out")),
                ..UiProducedBindings::default()
            },
            revision: None,
        };
        if !span_starts.is_empty() {
            product.preview = control_preview(lamps, span_starts);
        }
        assert_eq!(product.kind, UiProductKind::Control);
        let mut node = UiNodeView::new(
            UiNodeHeader::new("pixels", "Fixture", "/demo.project/pixels.fixture"),
            vec![UiNodeTab::main(vec![UiNodeSection::ProducedProducts(
                vec![product],
            )])],
        );
        node.face = None;
        node
    }

    fn control_preview(lamps: u32, span_starts: &[u32]) -> UiProductPreview {
        use lpc_model::{
            ColorOrder, ControlExtent, ControlSampleEncoding, ControlSampleLayout,
            ControlSampleSpan,
        };

        let mut spans = Vec::new();
        for (index, start) in span_starts.iter().enumerate() {
            let end = span_starts
                .get(index + 1)
                .copied()
                .unwrap_or(lamps.max(*start));
            spans.push(ControlSampleSpan {
                row: 0,
                start: start * SAMPLES_PER_LAMP,
                len: (end - start) * SAMPLES_PER_LAMP,
                encoding: ControlSampleEncoding::RgbPixels {
                    count: end - start,
                    color_order: ColorOrder::Rgb,
                },
            });
        }
        UiProductPreview::ControlNative(crate::UiControlProductPreview {
            revision: 1,
            extent: ControlExtent::new(1, lamps * SAMPLES_PER_LAMP),
            sample_format: crate::UiControlSampleFormat::U16,
            sample_layout: ControlSampleLayout { spans },
            display_layout: None,
            bytes: Vec::new().into(),
        })
    }

    fn set_bus_target(node: &mut UiNodeView, label: &str) {
        let UiNodeTabBody::Sections(sections) = &mut node.tabs[0].body else {
            panic!("sections tab");
        };
        let UiNodeSection::ProducedProducts(products) = &mut sections[0] else {
            panic!("produced products section");
        };
        products[0].binding.bindings.bus_target = Some(UiBindingEndpoint::new(label));
    }

    fn output_face_of(editor: &ProjectEditorView) -> &UiOutputFace {
        let Some(UiNodeFace::Output(face)) =
            editor.nodes.iter().find_map(|node| node.face.as_ref())
        else {
            panic!("the editor carries an output face");
        };
        face
    }
}
