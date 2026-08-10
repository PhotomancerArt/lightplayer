//! Deriving the two-sided patch bay from what an output already published.
//!
//! The engine plans a wire as a set of runs (`OutputFragment`s) and the
//! probe states them in lamps (`WireOutputPlacement`). Everything the bay
//! shows is a re-ordering of that one set:
//!
//! - the OUTPUT face clips the runs into the ports they land on and lays
//!   them along the wire;
//! - the FIXTURE face keeps each run whole and lays it along the producer's
//!   own channel space.
//!
//! Nothing here re-derives placement. Auto-flow order is the resolver's
//! (provider priority, then module node order), and a client that guessed
//! at it would be wrong the first time two producers tied. Contested lamps
//! ARE computed here, from the same runs — it is a property of the set, and
//! the engine's own status text is the prose version of the same fact.
//!
//! Read-only by construction: no cell carries a slot address (P6, D34a).

use lpc_model::NodeId;
use lpc_wire::WireOutputPlacement;

use crate::{
    UiControlProductPreview, UiFixturePatch, UiOutputChannelRow, UiPatchBay, UiPatchCell,
    UiPatchPort,
};

/// Samples per RGB lamp — the unit the frame's extent is stated in.
const SAMPLES_PER_LAMP: u32 = 3;

/// One output's published wire, as the derivation reads it.
pub(crate) struct OutputWire<'a> {
    /// The output node itself — how a card finds its own wire.
    pub node: NodeId,
    /// The output node's display label — what a fixture cell names when a
    /// project drives more than one.
    pub label: &'a str,
    /// The runs it planned, in its own planning order.
    pub placements: &'a [WireOutputPlacement],
    /// The frame it published, when one has arrived.
    pub frame: Option<&'a UiControlProductPreview>,
    /// Its authored ports, in slice order.
    pub channels: &'a [UiOutputChannelRow],
}

/// What the derivation knows about the node behind a run: its display
/// label, and its address path when the join found the card.
pub(crate) type ProducerLookup<'a> = dyn Fn(NodeId) -> Option<(String, String)> + 'a;

/// Lamps this wire spans: what the published frame says, or — before a
/// frame has arrived — as far as the runs reach.
fn wire_lamps(wire: &OutputWire<'_>) -> u32 {
    if let Some(frame) = wire.frame {
        let lamps = frame.extent.sample_count() / SAMPLES_PER_LAMP;
        if lamps > 0 {
            return lamps;
        }
    }
    wire.placements
        .iter()
        .map(|run| run.wire_lamp.saturating_add(run.lamps))
        .max()
        .unwrap_or(0)
}

/// How many runs claim each lamp of the wire.
fn coverage(placements: &[WireOutputPlacement], lamps: u32) -> Vec<u8> {
    let mut counts = vec![0_u8; lamps as usize];
    for run in placements {
        let start = run.wire_lamp.min(lamps) as usize;
        let end = run.wire_lamp.saturating_add(run.lamps).min(lamps) as usize;
        for count in &mut counts[start..end] {
            *count = count.saturating_add(1);
        }
    }
    counts
}

/// Every run on one wire as a cell, whole (unclipped), in planning order.
///
/// The cell's [`UiPatchCell::id`] is the RUN's identity, so a run later
/// clipped across two ports keeps one id and the two faces hover as twins.
fn wire_cells(
    wire: &OutputWire<'_>,
    coverage: &[u8],
    producer: &ProducerLookup<'_>,
) -> Vec<UiPatchCell> {
    wire.placements
        .iter()
        .map(|run| {
            let (label, path) = producer(run.node).map_or_else(
                || (format!("node {}", run.node.0), None),
                |(label, path)| (label, Some(path)),
            );
            let port = port_of(wire.channels, run.wire_lamp);
            UiPatchCell {
                id: cell_id(run),
                producer: label,
                producer_node: path,
                source_start: run.source_lamp,
                lamps: run.lamps,
                wire_start: run.wire_lamp,
                reversed: run.reversed,
                contested: contested(coverage, run.wire_lamp, run.lamps),
                port_key: port.map(|channel| channel.key),
                port_label: port
                    .map(|channel| channel.pin_label.clone())
                    .unwrap_or_default(),
                output_label: wire.label.to_string(),
            }
        })
        .collect()
}

/// Does any lamp of `[start, start + lamps)` carry more than one claim?
fn contested(coverage: &[u8], start: u32, lamps: u32) -> bool {
    let from = (start as usize).min(coverage.len());
    let to = (start.saturating_add(lamps) as usize).min(coverage.len());
    coverage[from..to].iter().any(|count| *count > 1)
}

/// Stable identity for a run — producer, output, and where it starts on
/// both sides. Two runs of one fixture never share all four.
fn cell_id(run: &WireOutputPlacement) -> String {
    format!(
        "{}:{}:{}:{}",
        run.node.0, run.output, run.source_lamp, run.wire_lamp
    )
}

/// The port a wire lamp falls in, when it falls in an authored one.
fn port_of(channels: &[UiOutputChannelRow], lamp: u32) -> Option<&UiOutputChannelRow> {
    channels.iter().find(|channel| {
        let Some(start) = channel.slice_start else {
            return false;
        };
        let Some(count) = channel.resolved_count.or(channel.count) else {
            // The remainder port, before the extent is known: everything
            // from its start onward is its.
            return lamp >= start;
        };
        lamp >= start && lamp < start.saturating_add(count)
    })
}

/// The output face's bay: the wire's runs, clipped into the ports that
/// drive them.
pub(crate) fn output_bay(wire: &OutputWire<'_>, producer: &ProducerLookup<'_>) -> UiPatchBay {
    let lamps = wire_lamps(wire);
    let coverage = coverage(wire.placements, lamps);
    let cells = wire_cells(wire, &coverage, producer);

    let mut ports = Vec::with_capacity(wire.channels.len());
    for channel in wire.channels {
        let start = channel.slice_start.unwrap_or(0);
        let count = channel
            .resolved_count
            .or(channel.count)
            .unwrap_or_else(|| lamps.saturating_sub(start));
        let end = start.saturating_add(count);
        // Wire order, not planning order: the row reads left to right, and
        // a reader following the strand should be able to follow the list.
        let mut clipped: Vec<UiPatchCell> = cells
            .iter()
            .filter_map(|cell| clip(cell, start, end))
            .collect();
        clipped.sort_by_key(|cell| cell.wire_start);
        ports.push(UiPatchPort {
            key: channel.key,
            pin_label: channel.pin_label.clone(),
            start,
            lamps: count,
            cells: clipped,
        });
    }

    UiPatchBay {
        ports,
        frame: wire.frame.cloned(),
        contested_lamps: coverage.iter().filter(|count| **count > 1).count() as u32,
        gap_lamps: coverage.iter().filter(|count| **count == 0).count() as u32,
    }
}

/// One cell narrowed to `[start, end)` of the wire, keeping its identity
/// and re-basing the producer-side span it still shows.
///
/// A REVERSED run is laid end-first, so the piece that survives a clip at
/// the wire's low end is the producer's HIGH end — that arithmetic is the
/// whole reason clipping is not a subtraction on both sides.
fn clip(cell: &UiPatchCell, start: u32, end: u32) -> Option<UiPatchCell> {
    let cell_end = cell.wire_start.saturating_add(cell.lamps);
    let lo = cell.wire_start.max(start);
    let hi = cell_end.min(end);
    if hi <= lo {
        return None;
    }
    let head = lo - cell.wire_start;
    let tail = cell_end - hi;
    let source_start = if cell.reversed {
        cell.source_start.saturating_add(tail)
    } else {
        cell.source_start.saturating_add(head)
    };
    Some(UiPatchCell {
        source_start,
        lamps: hi - lo,
        wire_start: lo,
        ..cell.clone()
    })
}

/// The fixture face's row: every run this node contributed, across every
/// output, in its OWN channel order.
///
/// `lamps` is the producer's whole product, which the runs themselves carry
/// (`source_lamps`) — a fixture whose runs cover only part of it still
/// renders a full-width row with the uncovered stretch dark, which is the
/// honest picture of a partial patch.
pub(crate) fn fixture_patch(
    node: NodeId,
    wires: &[OutputWire<'_>],
    producer: &ProducerLookup<'_>,
) -> Option<UiFixturePatch> {
    let mut cells: Vec<UiPatchCell> = Vec::new();
    let mut lamps = 0_u32;
    let mut frame: Option<UiControlProductPreview> = None;
    let mut wires_driven = 0_usize;
    for wire in wires {
        let mine: Vec<&WireOutputPlacement> = wire
            .placements
            .iter()
            .filter(|run| run.node == node)
            .collect();
        if mine.is_empty() {
            continue;
        }
        // Coverage is a property of the WHOLE wire, so it is computed over
        // every run on it — a fixture is contested by its neighbours, not
        // only by itself.
        let coverage = coverage(wire.placements, wire_lamps(wire));
        // `wire_cells` maps one cell per placement, in order, so the same
        // filter picks this node's cells out of it.
        let all = wire_cells(wire, &coverage, producer);
        for (cell, run) in all.into_iter().zip(wire.placements) {
            if run.node != node {
                continue;
            }
            lamps = lamps.max(run.source_lamps);
            cells.push(cell);
        }
        wires_driven += 1;
        if frame.is_none() {
            frame = wire.frame.cloned();
        }
    }
    if cells.is_empty() {
        return None;
    }
    cells.sort_by_key(|cell| cell.source_start);
    Some(UiFixturePatch {
        lamps,
        cells,
        frame,
        single_output: wires_driven <= 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The peach's own cut: body 0–21, leaf 22–33, body 22–43 laid back
    /// down the strand from 34.
    fn peach() -> Vec<WireOutputPlacement> {
        vec![
            run(2, 0, 44, 0, 22, false),
            run(3, 0, 12, 22, 12, false),
            run(2, 22, 44, 34, 22, true),
        ]
    }

    fn run(
        node: u32,
        source_lamp: u32,
        source_lamps: u32,
        wire_lamp: u32,
        lamps: u32,
        reversed: bool,
    ) -> WireOutputPlacement {
        WireOutputPlacement {
            node: NodeId::new(node),
            output: 0,
            source_lamp,
            source_lamps,
            wire_lamp,
            lamps,
            reversed,
        }
    }

    fn channel(key: u32, pin: &str, count: Option<u32>, start: Option<u32>) -> UiOutputChannelRow {
        UiOutputChannelRow {
            key,
            pin_label: pin.to_string(),
            count,
            resolved_count: count,
            slice_start: start,
            ..UiOutputChannelRow::default()
        }
    }

    fn lookup(node: NodeId) -> Option<(String, String)> {
        match node.0 {
            2 => Some(("peach_body".into(), "/peach.module/body.fixture".into())),
            3 => Some(("peach_leaf".into(), "/peach.module/leaf.fixture".into())),
            _ => None,
        }
    }

    fn wire<'a>(
        placements: &'a [WireOutputPlacement],
        channels: &'a [UiOutputChannelRow],
    ) -> OutputWire<'a> {
        OutputWire {
            node: NodeId::new(1),
            label: "Output",
            placements,
            frame: None,
            channels,
        }
    }

    #[test]
    fn one_port_carries_the_peach_s_three_runs_in_wire_order() {
        let placements = peach();
        let channels = [channel(0, "D10", Some(56), Some(0))];
        let bay = output_bay(&wire(&placements, &channels), &lookup);

        assert_eq!(bay.ports.len(), 1);
        let cells = &bay.ports[0].cells;
        assert_eq!(
            cells
                .iter()
                .map(|cell| (
                    cell.producer.as_str(),
                    cell.source_start,
                    cell.lamps,
                    cell.wire_start,
                    cell.reversed
                ))
                .collect::<Vec<_>>(),
            vec![
                ("peach_body", 0, 22, 0, false),
                ("peach_leaf", 0, 12, 22, false),
                ("peach_body", 22, 22, 34, true),
            ]
        );
        assert_eq!(bay.contested_lamps, 0);
        assert_eq!(bay.gap_lamps, 0, "the peach's patch covers its whole wire");
    }

    /// The same cells, the other ordering: the body's two stretches read as
    /// one unbroken run in its own numbering. That difference IS the patch.
    #[test]
    fn the_fixture_side_lays_the_same_runs_along_its_own_space() {
        let placements = peach();
        let channels = [channel(0, "D10", Some(56), Some(0))];
        let wires = [wire(&placements, &channels)];
        let body = fixture_patch(NodeId::new(2), &wires, &lookup).expect("the body is on the wire");

        assert_eq!(body.lamps, 44);
        assert_eq!(
            body.cells
                .iter()
                .map(|cell| (
                    cell.source_start,
                    cell.lamps,
                    cell.wire_start,
                    cell.reversed
                ))
                .collect::<Vec<_>>(),
            vec![(0, 22, 0, false), (22, 22, 34, true)],
            "0–21 forward at ch0, 22–43 backwards from ch34"
        );
        assert!(!body.is_single_run(), "the body arrives in two pieces");
        assert!(body.single_output);

        let leaf = fixture_patch(NodeId::new(3), &wires, &lookup).expect("the leaf is on the wire");
        assert!(
            leaf.is_single_run(),
            "the leaf is anchored, but its own space still arrives whole — \
             indistinguishable from auto-flow FROM THIS SIDE, and honestly so"
        );
        assert_eq!(leaf.cells[0].wire_start, 22, "the label is what says where");
    }

    /// The twin key is the RUN's, so both faces' cells match up — and a run
    /// clipped across two ports keeps it, so hovering lights both halves.
    #[test]
    fn both_faces_and_every_clipped_piece_share_one_cell_id() {
        let placements = peach();
        let one = [channel(0, "D10", Some(56), Some(0))];
        let split = [
            channel(0, "D10", Some(40), Some(0)),
            channel(1, "D11", None, Some(40)),
        ];
        let bay = output_bay(&wire(&placements, &split), &lookup);
        let wires = [wire(&placements, &one)];
        let body = fixture_patch(NodeId::new(2), &wires, &lookup).expect("body");

        let reversed_id = body.cells[1].id.clone();
        let halves: Vec<&UiPatchCell> = bay
            .ports
            .iter()
            .flat_map(|port| port.cells.iter())
            .filter(|cell| cell.id == reversed_id)
            .collect();
        assert_eq!(halves.len(), 2, "the reversed run straddles both ports");
        // Reversed: the piece on the LOW port is the producer's HIGH end.
        assert_eq!(
            halves
                .iter()
                .map(|cell| (cell.wire_start, cell.lamps, cell.source_start))
                .collect::<Vec<_>>(),
            vec![(34, 6, 38), (40, 16, 22)]
        );
    }

    /// A forward run clipped the same way re-bases from its own head — the
    /// asymmetry the reversed case exists to prove is real.
    #[test]
    fn a_forward_run_clips_from_its_head() {
        let placements = [run(2, 0, 44, 0, 44, false)];
        let split = [
            channel(0, "D10", Some(20), Some(0)),
            channel(1, "D11", Some(24), Some(20)),
        ];
        let bay = output_bay(&wire(&placements, &split), &lookup);

        assert_eq!(
            bay.ports
                .iter()
                .map(|port| (port.cells[0].wire_start, port.cells[0].source_start))
                .collect::<Vec<_>>(),
            vec![(0, 0), (20, 20)]
        );
    }

    #[test]
    fn overlap_reddens_both_runs_and_a_short_patch_leaves_a_gap() {
        // The leaf slid four lamps early: it now shares 30–33 with the
        // body's first stretch, and 54–55 reaches nothing.
        let placements = vec![
            run(2, 0, 44, 0, 34, false),
            run(3, 0, 12, 30, 12, false),
            run(2, 34, 44, 44, 10, true),
        ];
        let channels = [channel(0, "D10", Some(56), Some(0))];
        let bay = output_bay(&wire(&placements, &channels), &lookup);

        let cells = &bay.ports[0].cells;
        assert!(cells[0].contested, "the body's first stretch is contested");
        assert!(cells[1].contested, "and so is the leaf that overlapped it");
        assert!(!cells[2].contested, "the far stretch is untouched");
        assert_eq!(bay.contested_lamps, 4);
        assert_eq!(bay.gap_lamps, 2, "54–55 are driven by nothing");
    }

    /// The ordinary project: one fixture, no patch, one run over the whole
    /// wire.
    #[test]
    fn an_unpatched_fixture_reads_as_auto_flow() {
        let placements = [run(2, 0, 241, 0, 241, false)];
        let channels = [channel(0, "LED1", None, Some(0))];
        let wires = [wire(&placements, &channels)];

        let patch = fixture_patch(NodeId::new(2), &wires, &lookup).expect("on the wire");
        assert!(patch.is_single_run());
        assert_eq!(patch.cells.len(), 1);
        // The remainder port still resolves its span from the runs.
        let bay = output_bay(&wires[0], &lookup);
        assert_eq!(bay.ports[0].lamps, 241);
        assert_eq!(bay.gap_lamps, 0);
    }

    #[test]
    fn a_node_that_drives_nothing_has_no_row() {
        let placements = peach();
        let channels = [channel(0, "D10", Some(56), Some(0))];
        let wires = [wire(&placements, &channels)];

        assert!(fixture_patch(NodeId::new(9), &wires, &lookup).is_none());
    }

    /// A node the project walk could not name still gets a cell: the run is
    /// real whether or not the card was found.
    #[test]
    fn an_unjoined_producer_is_named_by_its_node_id() {
        let placements = [run(7, 0, 10, 0, 10, false)];
        let channels = [channel(0, "D10", Some(10), Some(0))];
        let bay = output_bay(&wire(&placements, &channels), &lookup);

        assert_eq!(bay.ports[0].cells[0].producer, "node 7");
        assert_eq!(bay.ports[0].cells[0].producer_node, None);
    }
}
