//! A port's FREE runs — the gaps between its cells — and the segment a
//! click on one selects (the Outputs panel's port bars: free space is a
//! click target, not a hole).

use lpa_studio_core::{NodeId, UiPatchPort, UiPatchTarget};

/// A port's FREE runs in wire numbering — the gaps between its cells,
/// merged (runs may overlap) and clipped to the port's own span.
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

/// The free segment a click on free port space selects: the whole run
/// `(start, lamps)` it landed in, never zero lamps. (The Patch tab sized it
/// by the next object waiting for a wire; with patching in the mapping
/// document there is no such object, and the run is the honest window.)
pub(crate) fn segment_at_free_run(node: NodeId, port: u32, run: (u32, u32)) -> UiPatchTarget {
    UiPatchTarget::Segment {
        node,
        port,
        start: run.0,
        lamps: run.1.max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::UiPatchCell;

    /// The gaps between cells, in wire numbering — clipped to the port and
    /// merged, so overlapping runs never open a phantom hole between them.
    #[test]
    fn free_runs_are_the_gaps_between_cells() {
        assert_eq!(
            free_runs(&port(0, 60, vec![cell(10, 10)])),
            [(0, 10), (20, 40)],
            "a cell in the middle leaves a run on each side"
        );
        assert_eq!(
            free_runs(&port(0, 60, Vec::new())),
            [(0, 60)],
            "an empty port is one free run"
        );
        assert_eq!(
            free_runs(&port(0, 30, vec![cell(0, 20), cell(10, 20)])),
            [],
            "overlapping (contested) cells still cover the port"
        );
        assert_eq!(
            free_runs(&port(40, 20, vec![cell(30, 20)])),
            [(50, 10)],
            "a cell reaching in from the previous port is clipped, not counted twice"
        );
    }

    /// A click on free space selects the whole run it landed in.
    #[test]
    fn a_free_segment_is_its_whole_run() {
        let node = NodeId::new(10);
        let run = free_runs(&port(0, 60, vec![cell(0, 30)]))[0];
        assert_eq!(run, (30, 30));
        assert_eq!(
            segment_at_free_run(node, 0, run),
            UiPatchTarget::Segment {
                node,
                port: 0,
                start: 30,
                lamps: 30,
            }
        );
        assert_eq!(
            segment_at_free_run(node, 0, (5, 0)),
            UiPatchTarget::Segment {
                node,
                port: 0,
                start: 5,
                lamps: 1,
            },
            "never zero lamps"
        );
    }

    fn cell(wire_start: u32, lamps: u32) -> UiPatchCell {
        UiPatchCell {
            id: format!("2:{wire_start}"),
            producer: "dome".to_string(),
            lamps,
            wire_start,
            ..Default::default()
        }
    }

    fn port(start: u32, lamps: u32, cells: Vec<UiPatchCell>) -> UiPatchPort {
        UiPatchPort {
            key: 0,
            pin_label: "IO0".to_string(),
            start,
            lamps,
            cells,
        }
    }
}
