//! The ProjectRead gate sized **per request**: a read needs its own
//! estimated working set plus the chip's margin free, and one block for its
//! own largest ask — instead of one floor for every read.
//!
//! Today's C6 gate asks every read for 40 KiB free — the worst measured
//! read's working set (the first sync's skeleton, 25 KB) plus room for the
//! link and radio tasks — so the device card's read, whose working set is
//! 4–9 KB at the choker's 73 lamps, is refused for the editor's sake
//! (`docs/defects/2026-10-09-a-phones-bluetooth-link-leaves-the-choker-under-the-read-gate.md`).
//! And the same floor is too SMALL at the design target: at 512 lamps the
//! card's read peaks at ~39 KB and the editor's control focus at ~50 KB, so
//! a 512-lamp board just over the floor passes the gate and resets.
//!
//! [`ReadCost::estimate`] reads the request's own shape and the project's
//! lamp count, and answers the two numbers the gate checks: the working set
//! the read will build, and its largest single ask. The coefficients are
//! the per-read allocation census of research experiment E7
//! (`lp2025/2026-10-09-1203-ram-research`, branch `research/ram-e07`) on
//! `lp-emu:esp32c6:t1` — the choker at 73 lamps, Logo Sign at 241, a
//! 512-lamp zook variant — rounded up; silicon (loose-c6 with a Bluetooth
//! central) came in under them. `tests` below holds every measured read
//! under its estimate.

use lpc_wire::{ProjectProbeRequest, ProjectReadQuery, ProjectReadRequest};

/// What one read will cost the heap, estimated from its shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadCost {
    /// Peak bytes the read holds at once (every task's allocations during
    /// it, as the census measured them).
    pub working_set: u32,
    /// The largest single allocation it makes.
    pub largest_ask: u32,
}

/// Any read: the envelope, the sink's batch, the runtime status.
const BASE: u32 = 4 * 1024;
/// A first sync's queries (`since: null`): the shape registry and every
/// node's entry, plus every slot's value as JSON when slots are asked for
/// (the skeleton 25,072 B, the slot page 21,032 B with the choker's 7.8 KB
/// mapping).
const SYNC_QUERIES: u32 = 22 * 1024;
/// An editor read's queries (`since` set): the deltas since the last read.
const DELTA_QUERIES: u32 = 4 * 1024;
/// The binding-graph probe (8,236 B with the base).
const BINDING_GRAPH: u32 = 5 * 1024;
/// The timebase probe.
const TIMEBASE: u32 = 1024;
/// The output-frame probe, per lamp: the merged display layout (20 B), its
/// clone, the mapping points (16 B) and the samples — 66 B a lamp across
/// 73, 241 and 512 lamps, rounded up.
const OUTPUT_FRAME_PER_LAMP: u32 = 72;
/// The control-product probe, per lamp (lens/control less lens/none:
/// 2.9 KB at 73 lamps, 10.8 KB at 512).
const CONTROL_PRODUCT_PER_LAMP: u32 = 24;
/// The display layout's element: one lamp's position and size.
const LAYOUT_BYTES_PER_LAMP: u32 = 20;
/// A slot's JSON on a first sync with slots: the catalog's largest value is
/// 8.9 KB (Logo Sign's mapping), sized exactly since the 2026-09-28 quick win.
const SLOT_VALUE_ASK: u32 = 9 * 1024;

impl ReadCost {
    /// The cost of `request` against a project whose published outputs carry
    /// `lamps` lamps.
    pub fn estimate(request: &ProjectReadRequest, lamps: u32) -> Self {
        let mut working_set = BASE;
        let mut largest_ask = 4 * 1024;
        if !request.queries.is_empty() {
            if request.since.is_none() {
                working_set += SYNC_QUERIES;
                let slots = request.queries.iter().any(|query| {
                    matches!(query, ProjectReadQuery::Nodes(nodes) if nodes.include_slots)
                });
                if slots {
                    largest_ask = largest_ask.max(SLOT_VALUE_ASK);
                }
            } else {
                working_set += DELTA_QUERIES;
            }
        }
        // The layout Vec is sized by doubling, so its block is the next power
        // of two of the lamp count (2,560 B at 73 lamps, 10,240 B at 512).
        let layout_ask = LAYOUT_BYTES_PER_LAMP.saturating_mul(lamps.max(1).next_power_of_two());
        // Lamp-sized probes add up (their layouts are alive together: the
        // control focus at 512 lamps is 10.8 KB over the output frame's own
        // read). The fixed-size ones run one after another and free their
        // scratch between, so the biggest counts, plus 1 KiB for each other
        // one's retained result (the lens reads at 73 lamps are 3.5 KB over
        // the output frame alone, not the 9 KB a sum would say).
        let mut fixed_max = 0u32;
        let mut fixed_count = 0u32;
        for probe in &request.probes {
            let fixed = match probe {
                ProjectProbeRequest::OutputFrame(_) => {
                    working_set += OUTPUT_FRAME_PER_LAMP.saturating_mul(lamps);
                    largest_ask = largest_ask.max(layout_ask);
                    continue;
                }
                ProjectProbeRequest::ControlProduct(_) => {
                    working_set += CONTROL_PRODUCT_PER_LAMP.saturating_mul(lamps);
                    largest_ask = largest_ask.max(layout_ask);
                    continue;
                }
                ProjectProbeRequest::RenderProduct(render) => {
                    // The texture (w × h × 8) and its read-back copy.
                    let texture = render.width.saturating_mul(render.height).saturating_mul(8);
                    largest_ask = largest_ask.max(texture);
                    texture.saturating_mul(2)
                }
                ProjectProbeRequest::BindingGraph(_) => BINDING_GRAPH,
                ProjectProbeRequest::Timebase(_) => TIMEBASE,
            };
            fixed_max = fixed_max.max(fixed);
            fixed_count += 1;
        }
        working_set += fixed_max + 1024 * fixed_count.saturating_sub(1);
        Self {
            working_set,
            largest_ask,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::format;

    fn request(json: &str) -> ProjectReadRequest {
        serde_json::from_str(json).expect("a read request")
    }

    const CARD: &str =
        r#"{"since":null,"probes":[{"output_frame":{"geometry":"always","samples":"srgb8"}}]}"#;

    #[test]
    fn the_cards_read_at_the_chokers_size_needs_under_ten_kilobytes() {
        let cost = ReadCost::estimate(&request(CARD), 73);
        // Measured: 8,656 B with geometry (emulated), 3,996 B held (LC6).
        assert!(cost.working_set >= 8_656, "{cost:?}");
        assert!(cost.working_set < 10 * 1024, "{cost:?}");
        assert_eq!(cost.largest_ask, 4 * 1024);
    }

    #[test]
    fn the_cards_read_at_512_lamps_covers_the_measured_peak() {
        let cost = ReadCost::estimate(&request(CARD), 512);
        // Measured: 38,716 B and three 10,240 B asks (lp-emu:esp32c6:t1).
        assert!(cost.working_set >= 38_716, "{cost:?}");
        assert_eq!(cost.largest_ask, 10_240);
    }

    #[test]
    fn every_measured_editor_read_is_covered() {
        // The lens reads of `lp-cli/tests/support/editor_reads.rs`, with the
        // E7 census's peaks (lp-emu:esp32c6:t1, alloc_watch_diag @ d108574fc).
        let lens = |probe: &str| {
            format!(
                r#"{{"since":5,"queries":[{{"shapes":{{"level":"detail"}}}},{{"nodes":{{"level":"detail","nodes":"all","include_slots":true}}}},{{"runtime":null}}],"probes":[{probe}{{"output_frame":{{"geometry":"always","samples":"srgb8"}}}},{{"binding_graph":{{"structure":"always","include_values":true}}}}]}}"#
            )
        };
        let render = r#"{"render_product":{"product":{"node":5,"output":0},"width":16,"height":16,"format":"srgb8"}},"#;
        let control = r#"{"control_product":{"product":{"node":2,"output":0,"preferred_extent":{"rows":1,"samples_per_row":219}},"sample_format":"srgb8","geometry":"always"}},"#;
        for (probe, lamps, measured) in [
            ("", 73, 12_140),
            ("", 241, 20_408),
            ("", 512, 39_404),
            (render, 73, 14_020),
            (render, 512, 40_196),
            (control, 73, 15_520),
            (control, 241, 25_900),
            (control, 512, 50_168),
        ] {
            let cost = ReadCost::estimate(&request(&lens(probe)), lamps);
            assert!(cost.working_set >= measured, "{lamps} lamps {probe}: {cost:?} < {measured}");
        }
    }

    #[test]
    fn a_first_sync_with_slots_asks_for_the_largest_slot_value() {
        let cost = ReadCost::estimate(
            &request(r#"{"since":null,"queries":[{"nodes":{"level":"detail","nodes":"all","include_slots":true}}]}"#),
            73,
        );
        assert!(cost.working_set >= 25_072, "{cost:?}");
        assert_eq!(cost.largest_ask, SLOT_VALUE_ASK);
    }
}
