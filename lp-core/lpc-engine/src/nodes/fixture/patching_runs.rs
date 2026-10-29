//! A fixture's [`Patching`] as the runs its outputs already place (lean
//! Q36, Q39): each laid segment becomes [`PatchedRun`]s naming its output,
//! and the segments this build can't drive are refused, by name.
//!
//! **Address → wire lamp.** An output's buffer is one flat run of samples
//! and its ports are cuts of it ([`crate::node::output_port_slices`]), so a
//! segment at `(output, port, channel)` starts at sample
//! `start_samples(port) + channel`, which is wire lamp `/ 3`. A segment the
//! patcher placed whole is one run, its direction and offset the run's own
//! (the output applies them by the arithmetic [`Patching::landing`] states:
//! orientation first, then rotation). A segment set where it doesn't fit
//! (F9, `limit`) lands only the lamps whose place is inside the port; those
//! are one or two runs with no rotation.
//!
//! **Refused, per segment** (lean Q41, D2): a start part-way into a lamp, a
//! lamp other than RGB, an order other than the fixture's. A refused segment
//! makes no run, but the patcher already laid it, so it keeps its room on the
//! port and what follows it still lands where the patcher said.

use alloc::vec::Vec;

use lpc_mapping::part::{Colors, Order, Tree, TreeId};
use lpc_mapping::patching::{Patching, SegmentPatch};
use lpc_model::nodes::fixture::ColorOrder;

use crate::node::{OutputPortTable, PORT_SAMPLES_PER_LAMP, PatchedRun};

/// The runs a fixture publishes, and the segments it refused.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlacedRuns {
    pub runs: Vec<PatchedRun>,
    pub refusals: Vec<Refusal>,
}

/// A segment this build won't place, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// It starts part-way into a lamp, at this channel of its port.
    OffLampBoundary { segment: TreeId, channel: u32 },
    /// Its lamps aren't RGB.
    Colors { segment: TreeId, colors: Colors },
    /// Its order isn't the fixture's.
    Order {
        segment: TreeId,
        order: Order,
        fixture: ColorOrder,
    },
}

/// Turn `patching` (of `tree`, against `outputs`) into runs for a fixture
/// whose lamps are in `color_order`.
pub(crate) fn patching_runs(
    patching: &Patching,
    tree: &Tree,
    outputs: &[OutputPortTable],
    color_order: ColorOrder,
) -> PlacedRuns {
    let fixture_order = Order::parse(color_order.as_str()).ok();
    let mut placed = PlacedRuns::default();
    // Both lists are in tree order, and every laid segment is in the tree:
    // one pass, no lookup by id.
    let mut segments = tree.segments().iter();
    for laid in &patching.segments {
        let Some(segment) = segments.find(|segment| segment.id == laid.id) else {
            debug_assert!(false, "a laid segment the tree doesn't have: {}", laid.id);
            continue;
        };
        if let Some(refusal) = refusal(laid, &segment.lamp, fixture_order, color_order) {
            placed.refusals.push(refusal);
            continue;
        }
        let Some(port_start) = port_start(outputs, laid) else {
            debug_assert!(
                false,
                "the patcher laid {} on a port it wasn't given",
                laid.id
            );
            continue;
        };
        let lamp = (port_start + laid.address.channel) / PORT_SAMPLES_PER_LAMP;
        push_runs(&mut placed.runs, laid, segment.lamps.start, lamp);
    }
    placed
}

/// Why `laid` can't be placed by this build, if it can't.
fn refusal(
    laid: &SegmentPatch,
    lamp: &lpc_mapping::part::LampRef,
    fixture_order: Option<Order>,
    color_order: ColorOrder,
) -> Option<Refusal> {
    let colors = lamp.colors_or_rgb();
    if colors != Colors::Rgb {
        return Some(Refusal::Colors {
            segment: laid.id.clone(),
            colors,
        });
    }
    if laid.address.channel % laid.channels_per_lamp.max(1) != 0 {
        return Some(Refusal::OffLampBoundary {
            segment: laid.id.clone(),
            channel: laid.address.channel,
        });
    }
    match lamp.order {
        Some(order) if Some(order) != fixture_order => Some(Refusal::Order {
            segment: laid.id.clone(),
            order,
            fixture: color_order,
        }),
        _ => None,
    }
}

/// The first sample of the port `laid` starts on.
fn port_start(outputs: &[OutputPortTable], laid: &SegmentPatch) -> Option<u32> {
    outputs
        .iter()
        .find(|table| table.name == laid.address.output.as_str())?
        .ports
        .iter()
        .find(|port| port.key == laid.address.port.0)
        .map(|port| port.start_samples)
}

/// The runs that place `laid`'s lamps (the fixture's `first..first + n`)
/// from wire lamp `lamp` on.
fn push_runs(runs: &mut Vec<PatchedRun>, laid: &SegmentPatch, first: u32, lamp: u32) {
    let n = laid.lamps;
    if n == 0 {
        return;
    }
    let reversed = laid.direction.is_reverse();
    let output = Some(alloc::string::String::from(laid.address.output.as_str()));
    let reach = laid.limit.map_or(n, |limit| limit.min(n));
    if reach == n {
        runs.push(PatchedRun {
            start: first,
            count: n,
            lamp,
            reversed,
            offset: laid.offset % n,
            output,
        });
        return;
    }
    // Clipped (F9): wire places `0..reach` only. Wire place `w` holds the
    // oriented lamp `(w − offset) mod n`, so the places split where that
    // wraps: `0..min(offset, reach)` hold oriented lamps from `n − offset`,
    // and `offset..reach` (when the offset is short of the reach) from 0.
    let offset = laid.offset % n;
    let wrapped = offset.min(reach);
    let blocks = [
        (0, n - offset, wrapped),
        (offset, 0, reach.saturating_sub(offset)),
    ];
    for (place, oriented, count) in blocks {
        if count == 0 {
            continue;
        }
        // Forward, oriented lamp IS the segment's lamp; reversed, oriented
        // `o..o + count` are the segment's `n − o − count..n − o`, laid
        // end-first.
        let start = if reversed {
            n - oriented - count
        } else {
            oriented
        };
        runs.push(PatchedRun {
            start: first + start,
            count,
            lamp: lamp + place,
            reversed,
            offset: 0,
            output: output.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use lpc_mapping::part::{Mapping, Part, PartId, PartKind, Path, Shape, render};
    use lpc_mapping::patching::{Address, Direction, OutputName, PortKey};
    use lpc_mapping::{PatchedRange, patched_wire_lamp};

    use crate::node::OutputPortSlice;

    /// A placed segment is one run, at the port's start plus its channel.
    #[test]
    fn a_placed_segment_is_one_run_at_its_ports_start_plus_its_channel() {
        let tree = tree_of(&[4, 5]);
        let patching = laid(vec![
            seg("path1", "main", 1, 6, 4),
            seg("path2", "main", 0, 3, 5),
        ]);
        let placed = patching_runs(&patching, &tree, &[main_table()], ColorOrder::Rgb);
        assert_eq!(
            placed.runs,
            vec![
                run(0, 4, 10 + 2, false, 0, "main"),
                run(4, 5, 1, false, 0, "main"),
            ],
            "port 1 starts at sample 30 (lamp 10); channel 6 is lamp 2"
        );
        assert!(placed.refusals.is_empty());
    }

    /// A reversed, offset segment lands each lamp where `Patching::landing`
    /// says (every count to 12, every offset, every limit, both directions).
    #[test]
    fn a_reversed_offset_segment_lands_each_lamp_where_landing_says() {
        for n in 1..=12u32 {
            let tree = tree_of(&[n]);
            for offset in 0..n {
                for limit in (0..=n).map(Some).chain([None]) {
                    for direction in [Direction::Forward, Direction::Reverse] {
                        let mut s = seg("path1", "main", 1, 9, n);
                        s.offset = offset;
                        s.limit = limit;
                        s.direction = direction;
                        let patching = laid(vec![s]);
                        let placed =
                            patching_runs(&patching, &tree, &[main_table()], ColorOrder::Rgb);
                        let id = TreeId::of(&pid("path1"));
                        let mut wire: Vec<Option<u32>> = vec![None; n as usize];
                        for run in &placed.runs {
                            let range = PatchedRange {
                                start: run.start,
                                count: run.count,
                                lamp: run.lamp,
                                reversed: run.reversed,
                                offset: run.offset,
                                output: run.output.clone(),
                            };
                            for j in 0..run.count {
                                let source = (run.start + j) as usize;
                                assert!(
                                    wire[source].is_none(),
                                    "lamp {source} placed twice (n {n} offset {offset} limit {limit:?} {direction:?})"
                                );
                                wire[source] = Some(patched_wire_lamp(&range, j));
                            }
                        }
                        for (j, placed) in wire.iter().enumerate() {
                            let expected = patching
                                .landing(&id, j as u32)
                                .map(|address| (30 + address.channel) / 3);
                            assert_eq!(
                                *placed, expected,
                                "lamp {j} (n {n} offset {offset} limit {limit:?} {direction:?})"
                            );
                        }
                    }
                }
            }
        }
    }

    /// A segment held where it doesn't fit places only the lamps its port
    /// reaches.
    #[test]
    fn a_segment_held_where_it_doesnt_fit_places_only_what_its_port_reaches() {
        let tree = tree_of(&[8]);
        let mut s = seg("path1", "main", 0, 24, 8);
        s.limit = Some(2);
        let placed = patching_runs(&laid(vec![s]), &tree, &[main_table()], ColorOrder::Rgb);
        assert_eq!(placed.runs, vec![run(0, 2, 8, false, 0, "main")]);
    }

    /// A segment the patcher didn't lay makes no run.
    #[test]
    fn a_segment_the_patcher_didnt_lay_makes_no_run() {
        let tree = tree_of(&[4, 5, 6]);
        let patching = laid(vec![seg("path2", "main", 0, 0, 5)]);
        let placed = patching_runs(&patching, &tree, &[main_table()], ColorOrder::Rgb);
        assert_eq!(placed.runs, vec![run(4, 5, 0, false, 0, "main")]);
    }

    /// Every run names its output.
    #[test]
    fn every_run_names_its_output() {
        let tree = tree_of(&[4, 5]);
        let other = OutputPortTable {
            name: String::from("other"),
            ports: vec![port(0, 0, None)],
        };
        let patching = laid(vec![
            seg("path1", "main", 0, 0, 4),
            seg("path2", "other", 0, 0, 5),
        ]);
        let placed = patching_runs(&patching, &tree, &[main_table(), other], ColorOrder::Rgb);
        let outputs: Vec<Option<&str>> = placed.runs.iter().map(|r| r.output.as_deref()).collect();
        assert_eq!(outputs, [Some("main"), Some("other")]);
    }

    /// A segment part-way into a lamp is not placed, and the fixture says
    /// which.
    #[test]
    fn a_segment_part_way_into_a_lamp_is_not_placed() {
        let tree = tree_of(&[4, 5]);
        let patching = laid(vec![
            seg("path1", "main", 0, 4, 4),
            seg("path2", "main", 0, 15, 5),
        ]);
        let placed = patching_runs(&patching, &tree, &[main_table()], ColorOrder::Rgb);
        assert_eq!(placed.runs.len(), 1, "path2 still lights");
        assert_eq!(
            placed.refusals,
            vec![Refusal::OffLampBoundary {
                segment: TreeId::of(&pid("path1")),
                channel: 4
            }]
        );
    }

    /// An RGBW segment is not placed, and the others still light at their
    /// addresses.
    #[test]
    fn an_rgbw_segment_is_not_placed_and_the_others_still_light() {
        // Three RGBW lamps take 12 channels, so path2 starts on a whole RGB
        // lamp (four would leave it part-way into one, and refused too).
        let mut mapping = mapping_of(&[3, 5]);
        mapping.parts[0].lamp.colors = Some(Colors::Rgbw);
        let tree = render(&mapping).unwrap();
        let mut rgbw = seg("path1", "main", 0, 0, 3);
        rgbw.channels_per_lamp = 4;
        let patching = laid(vec![rgbw, seg("path2", "main", 0, 12, 5)]);
        let placed = patching_runs(&patching, &tree, &[main_table()], ColorOrder::Rgb);
        assert_eq!(
            placed.refusals,
            vec![Refusal::Colors {
                segment: TreeId::of(&pid("path1")),
                colors: Colors::Rgbw
            }]
        );
        assert_eq!(placed.runs, vec![run(3, 5, 4, false, 0, "main")]);
    }

    /// A segment whose order isn't the fixture's is not placed.
    #[test]
    fn a_segment_whose_order_isnt_the_fixtures_is_not_placed() {
        let mut mapping = mapping_of(&[4, 5]);
        mapping.parts[1].lamp.order = Some(Order::parse("BGR").unwrap());
        mapping.parts[0].lamp.order = Some(Order::parse("GRB").unwrap());
        let tree = render(&mapping).unwrap();
        let patching = laid(vec![
            seg("path1", "main", 0, 0, 4),
            seg("path2", "main", 0, 12, 5),
        ]);
        let placed = patching_runs(&patching, &tree, &[main_table()], ColorOrder::Grb);
        assert_eq!(
            placed.refusals,
            vec![Refusal::Order {
                segment: TreeId::of(&pid("path2")),
                order: Order::parse("BGR").unwrap(),
                fixture: ColorOrder::Grb,
            }]
        );
        assert_eq!(placed.runs.len(), 1, "path1's GRB is the fixture's own");
    }

    // --- helpers ---

    fn pid(text: &str) -> PartId {
        PartId::new(text).unwrap()
    }

    fn mapping_of(counts: &[u32]) -> Mapping {
        let mut mapping = Mapping::new();
        for (i, count) in counts.iter().enumerate() {
            let y = i as f32;
            mapping.parts.push(Part::new(
                pid(&alloc::format!("path{}", i + 1)),
                PartKind::Shape(Shape::Path(Path::through(&[[0.0, y], [10.0, y]], *count))),
            ));
        }
        mapping
    }

    fn tree_of(counts: &[u32]) -> Tree {
        render(&mapping_of(counts)).unwrap()
    }

    /// Output `main`: port 0 of 10 lamps, port 1 the rest.
    fn main_table() -> OutputPortTable {
        OutputPortTable {
            name: String::from("main"),
            ports: vec![port(0, 0, Some(30)), port(1, 30, None)],
        }
    }

    fn port(key: u32, start_samples: u32, capacity_samples: Option<u32>) -> OutputPortSlice {
        OutputPortSlice {
            key,
            start_samples,
            capacity_samples,
        }
    }

    fn seg(id: &str, output: &str, port: u32, channel: u32, lamps: u32) -> SegmentPatch {
        SegmentPatch {
            id: TreeId::of(&pid(id)),
            address: Address {
                output: OutputName::new(output),
                port: PortKey(port),
                channel,
            },
            lamps,
            channels_per_lamp: 3,
            direction: Direction::Forward,
            offset: 0,
            limit: None,
            wrapped: false,
        }
    }

    fn laid(segments: Vec<SegmentPatch>) -> Patching {
        Patching {
            segments,
            ..Patching::default()
        }
    }

    fn run(
        start: u32,
        count: u32,
        lamp: u32,
        reversed: bool,
        offset: u32,
        output: &str,
    ) -> PatchedRun {
        PatchedRun {
            start,
            count,
            lamp,
            reversed,
            offset,
            output: Some(String::from(output)),
        }
    }
}
