//! Does an output's wire already carry a control product's lamps?
//!
//! The lens's one-copy rule (lean-wire P5, Yona 2026-09-23: "studio should
//! only ask for one copy at a time unless you ask for more"): a device lens
//! streams every output's published frame, pixels included, because the
//! module hero, the output's patch bay and each fixture's patch row draw
//! it. A control product whose EVERY lamp is placed on one of those wires is
//! then a second copy of pixels already on the link, so it stops riding the
//! read unasked; its own surfaces go not-live with Show live, and selecting
//! its node asks for it again.
//!
//! "Every lamp" is literal: the union of the product's placed runs, in the
//! producer's own numbering, must cover `0..source_lamps`. A fixture that
//! is only partly patched still streams — the unplaced stretch is visible
//! nowhere else.

use lpc_wire::WireOutputPlacement;

/// Whether the runs placed for `(node, output)` across `wires` cover all of
/// that producer's lamps. `false` when no run names it, or it has no lamps.
#[must_use]
pub(crate) fn product_lamps_all_placed<'a>(
    node: u32,
    output: u32,
    wires: impl IntoIterator<Item = &'a [WireOutputPlacement]>,
) -> bool {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    let mut total = 0_u32;
    for placements in wires {
        for run in placements
            .iter()
            .filter(|run| run.node.as_u32() == node && run.output == output)
        {
            total = total.max(run.source_lamps);
            runs.push((run.source_lamp, run.source_lamp.saturating_add(run.lamps)));
        }
    }
    if total == 0 {
        return false;
    }
    runs.sort_unstable();
    let mut covered_to = 0_u32;
    for (start, end) in runs {
        if start > covered_to {
            return false;
        }
        covered_to = covered_to.max(end);
    }
    covered_to >= total
}

#[cfg(test)]
mod tests {
    use lpc_model::NodeId;

    use super::*;

    #[test]
    fn one_whole_run_covers_the_product() {
        let wire = [run(2, 0, 73, 0, 73)];
        assert!(product_lamps_all_placed(2, 0, [&wire[..]]));
    }

    /// The peach's shape: one fixture split into two runs around another
    /// producer's on the same wire — still every lamp; and one fixture split
    /// across two wires (the small dome's boxes).
    #[test]
    fn split_runs_on_one_or_several_wires_cover_the_product() {
        let one_wire = [
            run(2, 0, 44, 0, 22),
            run(3, 0, 1, 0, 1),
            run(2, 22, 44, 22, 22),
        ];
        assert!(product_lamps_all_placed(2, 0, [&one_wire[..]]));

        let box_1 = [run(2, 0, 10, 0, 6)];
        let box_2 = [run(2, 6, 10, 0, 4)];
        assert!(product_lamps_all_placed(2, 0, [&box_1[..], &box_2[..]]));
    }

    /// A gap in the producer's own numbering — lamps no wire carries — means
    /// the product is the only place those lamps are visible.
    #[test]
    fn a_partial_patch_does_not_cover_the_product() {
        let wire = [run(2, 0, 44, 0, 20), run(2, 22, 44, 20, 22)];
        assert!(!product_lamps_all_placed(2, 0, [&wire[..]]));
    }

    #[test]
    fn another_producer_or_output_or_no_wire_covers_nothing() {
        let wire = [run(2, 0, 73, 0, 73)];
        assert!(!product_lamps_all_placed(3, 0, [&wire[..]]));
        assert!(!product_lamps_all_placed(2, 1, [&wire[..]]));
        assert!(!product_lamps_all_placed(2, 0, core::iter::empty()));
    }

    fn run(
        node: u32,
        source_lamp: u32,
        source_lamps: u32,
        wire_lamp: u32,
        lamps: u32,
    ) -> WireOutputPlacement {
        WireOutputPlacement {
            node: NodeId::new(node),
            output: 0,
            source_lamp,
            source_lamps,
            wire_lamp,
            lamps,
            reversed: false,
        }
    }
}
