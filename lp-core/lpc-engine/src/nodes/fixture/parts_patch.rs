//! A format-6 fixture's patching, kept current: what it holds between
//! frames and when it re-patches (lean Q36, Q37, Q42, Q43).
//!
//! **Resident:** the mapping (parts and patching), the runs it publishes,
//! and its status. The tree the patcher walks is rendered for each patch
//! and dropped with it (M2 P6 step 2: resident, it cost a node and a
//! segment, each owning its id, per segment on the device), and so is the
//! [`Patching`], made without its walk ([`patch_mapping_placed`], step 1).
//! The lamps' positions live once, in the fixture's carrier.
//!
//! **When:** only when the key moves: the layout's version (a re-render),
//! the port tables' key (a name, a port, a count, or the outputs on the
//! channel), or the fixture's color order (an order refusal depends on it).
//! A steady frame compares three small values and asks nothing.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_mapping::part::{Mapping, render_layout};
use lpc_mapping::patching::{
    OutputName, OutputPorts, Patching, Port, PortKey, patch_mapping_placed,
};
use lpc_model::Revision;
use lpc_model::nodes::fixture::ColorOrder;

use crate::node::{PatchedRun, PortTablesKey, ReachableOutputs, TickContext};
use crate::nodes::fixture::parts_problem_text::{problem_text, refusal_text};
use crate::nodes::fixture::patching_runs::patching_runs;

/// What a patch was made under.
pub(crate) type PartsPatchKey = (Revision, PortTablesKey, ColorOrder);

/// A format-6 fixture's patching.
pub(crate) struct FixturePartsPatch {
    /// The mapping: its parts, its fit facts and its patching.
    pub mapping: Mapping,
    key: Option<PartsPatchKey>,
    /// The runs the fixture publishes, every one naming its output.
    pub runs: Vec<PatchedRun>,
    /// The first problem in words, with how many more; `None` when nothing
    /// is wrong.
    pub status: Option<String>,
}

impl FixturePartsPatch {
    pub fn new(mapping: Mapping) -> Self {
        Self {
            mapping,
            key: None,
            runs: Vec::new(),
            status: None,
        }
    }

    /// Re-patch when the key moved; returns whether the runs changed.
    pub fn ensure(
        &mut self,
        ctx: &TickContext<'_>,
        layout_version: Revision,
        color_order: ColorOrder,
    ) -> bool {
        let key = (layout_version, ctx.port_tables_key(), color_order);
        if self.key == Some(key) {
            return false;
        }
        self.repatch(key, ctx.control_patch_outputs().0, color_order)
    }

    /// Take a mapping that draws the same lamps
    /// ([`Mapping::same_layout`]) and patch it on the next
    /// [`Self::ensure`], whatever the key: a patching-only edit keeps the
    /// layout's version, so nothing else would move the key.
    pub fn repatch_with(&mut self, mapping: Mapping) {
        debug_assert!(self.mapping.same_layout(&mapping));
        self.mapping = mapping;
        self.key = None;
    }

    /// [`Self::ensure`] with the question passed in (a test double of P1's
    /// host question): `ask` runs only when `key` differs from the one the
    /// runs were made under.
    #[cfg(test)]
    pub fn ensure_with(
        &mut self,
        key: PartsPatchKey,
        ask: impl FnOnce() -> ReachableOutputs,
        color_order: ColorOrder,
    ) -> bool {
        if self.key == Some(key) {
            return false;
        }
        self.repatch(key, ask(), color_order)
    }

    /// Patch against `reachable` and convert; returns whether the runs
    /// changed.
    ///
    /// Cold and never inlined: the caller is the fixture's
    /// `read_frame_settings`, which every fixture runs every frame on the
    /// C6's main stack. Inlined there, the patcher's working set became part
    /// of that function's frame (6,736 B on the C6 against 3,296 B without
    /// it, measured in M2 P3), paid by every fixture on every frame. Out of line, it is paid only while re-patching.
    #[cold]
    #[inline(never)]
    fn repatch(
        &mut self,
        key: PartsPatchKey,
        reachable: ReachableOutputs,
        color_order: ColorOrder,
    ) -> bool {
        self.key = Some(key);
        // The same mapping rendered at load, so only memory can refuse it
        // now; then nothing lands, and the status says why. Laid out, not
        // placed: the patch walks the tree's structure and never reads a
        // position (a circle's `sinf`/`cosf` per lamp, again, for nothing).
        let mut tree = match render_layout(&self.mapping) {
            Ok(tree) => tree,
            Err(error) => {
                self.status = Some(alloc::format!("render fixture mapping: {error}"));
                let changed = !self.runs.is_empty();
                self.runs = Vec::new();
                return changed;
            }
        };
        drop(tree.take_positions());
        let ports = patcher_ports(&reachable);
        let patching = patch_mapping_placed(&self.mapping, &tree, &ports);
        drop(ports);
        let placed = patching_runs(&patching, &tree, &reachable.outputs, color_order);
        drop(tree);
        self.status = status(&patching, &placed.refusals, &reachable);
        let changed = self.runs != placed.runs;
        self.runs = placed.runs;
        changed
    }
}

/// The port tables as the patcher reads them. Capacity in channels is the
/// table's samples: a sample is one channel of an RGB lamp, and RGB is all
/// this build drives (`SAMPLES_PER_LAMP = 3`).
fn patcher_ports(reachable: &ReachableOutputs) -> Vec<OutputPorts> {
    reachable
        .outputs
        .iter()
        .map(|table| OutputPorts {
            output: OutputName::new(&table.name),
            ports: table
                .ports
                .iter()
                .map(|slice| Port {
                    key: PortKey(slice.key),
                    channels: slice.capacity_samples,
                })
                .collect(),
        })
        .collect()
}

/// The first problem or refusal in words, plus "(and N more)": the
/// patcher's problems in its order, then this build's refusals in tree
/// order. A stale override alone says nothing.
fn status(
    patching: &Patching,
    refusals: &[crate::nodes::fixture::patching_runs::Refusal],
    reachable: &ReachableOutputs,
) -> Option<String> {
    let mut first: Option<String> = None;
    let mut more = 0usize;
    let channel = reachable.channel.as_ref();
    let texts = patching
        .problems
        .iter()
        .filter_map(|problem| problem_text(problem, patching, channel))
        .chain(refusals.iter().map(refusal_text));
    for text in texts {
        if first.is_none() {
            first = Some(text);
        } else {
            more += 1;
        }
    }
    first.map(|text| {
        if more == 0 {
            text
        } else {
            alloc::format!("{text} (and {more} more)")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use core::cell::Cell;
    use lpc_mapping::part::{Part, PartId, PartKind, Path, Shape};
    use lpc_mapping::patching::{Amount, Start};
    use lpc_model::ChannelName;

    use crate::node::{OutputPortSlice, OutputPortTable};

    /// Nothing re-patches while the key stands.
    #[test]
    fn nothing_re_patches_while_the_key_stands() {
        let mut parts = parts_patch(Start::at("main", 0, Amount::lamps(0)));
        let asked = Cell::new(0);
        let ask = || {
            asked.set(asked.get() + 1);
            reachable(&[10, 10])
        };
        assert!(parts.ensure_with(key(1, 1), ask, ColorOrder::Rgb));
        for _ in 0..3 {
            assert!(!parts.ensure_with(key(1, 1), ask, ColorOrder::Rgb));
        }
        assert_eq!(asked.get(), 1, "asked once, for the first patch");
    }

    /// A port edit re-patches.
    #[test]
    fn a_port_edit_re_patches() {
        // The fixture starts on port 1, which begins where port 0 ends.
        let mut parts = parts_patch(Start::at("main", 1, Amount::lamps(0)));
        // path1 and path2 land end to end: one run of both.
        parts.ensure_with(key(1, 1), || reachable(&[3, 10]), ColorOrder::Rgb);
        assert_eq!(lamps_of(&parts.runs), vec![(0, 4, 3)]);
        // Port 0 grows to 5 lamps: the edit moved the tables' key (P1).
        assert!(parts.ensure_with(key(1, 2), || reachable(&[5, 10]), ColorOrder::Rgb));
        assert_eq!(
            lamps_of(&parts.runs),
            vec![(0, 4, 5)],
            "everything on port 1 moved with its start"
        );
        // Port 1 shrinks to 3 lamps: path2 no longer fits and wraps to port 2,
        // which isn't there: it is past the last port.
        assert!(parts.ensure_with(key(1, 3), || reachable(&[5, 3]), ColorOrder::Rgb));
        assert_eq!(lamps_of(&parts.runs), vec![(0, 3, 5)]);
        let status = parts.status.as_deref().unwrap_or_default();
        assert!(status.contains("path2 runs past the last port"), "{status}");
    }

    /// A fixture with no start places nothing and says so.
    #[test]
    fn a_fixture_with_no_start_places_nothing_and_says_so() {
        let mut parts = parts_patch(Start::KEEP);
        parts.ensure_with(key(1, 1), || reachable(&[10, 10]), ColorOrder::Rgb);
        assert!(parts.runs.is_empty());
        let status = parts.status.as_deref().unwrap_or_default();
        assert!(status.contains("no start"), "{status}");
    }

    /// A start naming an output nobody on the fixture's channel reads
    /// places nothing there, and the status names the output and the
    /// channel.
    #[test]
    fn a_start_naming_an_unreachable_output_says_which_and_where() {
        let mut parts = parts_patch(Start::at("lounge", 0, Amount::lamps(0)));
        parts.ensure_with(key(1, 1), || reachable(&[10, 10]), ColorOrder::Rgb);
        assert!(parts.runs.is_empty());
        assert_eq!(
            parts.status.as_deref(),
            Some(
                "the fixture's start names output \"lounge\", but no named output reads \
                 bus:control.out"
            )
        );
    }

    /// The status is clear when nothing is wrong.
    #[test]
    fn the_status_is_clear_when_nothing_is_wrong() {
        let mut parts = parts_patch(Start::at("main", 0, Amount::lamps(0)));
        parts.ensure_with(key(1, 1), || reachable(&[10, 10]), ColorOrder::Rgb);
        assert_eq!(parts.status, None);
        assert_eq!(lamps_of(&parts.runs), vec![(0, 4, 0)]);
    }

    /// A fixture whose segments land end to end publishes one run, and every
    /// lamp of it lands where `Patching::landing` says.
    #[test]
    fn a_fixture_whose_segments_are_contiguous_publishes_one_run() {
        use lpc_mapping::part::{Repeat, render};
        use lpc_mapping::patching::patch_mapping;

        // segments-64-6's shape: 64 copies of an 8-lamp spoke, from lamp 0
        // of port 0 of `main`.
        let spoke = Part::new(
            PartId::new("path1").unwrap(),
            PartKind::Shape(Shape::Path(Path::through(&[[50.0, 40.0], [50.0, 4.0]], 8))),
        );
        let mut mapping = Mapping::new();
        mapping.parts.push(Part::new(
            PartId::new("spoke").unwrap(),
            PartKind::Repeat(Repeat::new(spoke, [50.0, 50.0], 64)),
        ));
        mapping.start = Start::at("main", 0, Amount::lamps(0));
        let mut parts = FixturePartsPatch::new(mapping.clone());
        let lamps = reachable(&[512]);
        parts.ensure_with(key(1, 1), || lamps.clone(), ColorOrder::Rgb);
        assert_eq!(parts.status, None);
        assert_eq!(lamps_of(&parts.runs), vec![(0, 512, 0)], "one run, not 64");

        let tree = render(&mapping).unwrap();
        let patching = patch_mapping(&mapping, &tree, &patcher_ports(&lamps));
        let run = &parts.runs[0];
        for segment in tree.segments() {
            for j in 0..segment.lamps.len() as u32 {
                let landed = patching.landing(&segment.id, j).unwrap();
                assert_eq!(
                    run.wire_lamp(segment.lamps.start + j - run.start),
                    landed.channel / 3,
                    "{} lamp {j}",
                    segment.id
                );
            }
        }
    }

    // --- helpers ---

    /// Two paths, 3 and 1 lamps, the fixture starting where `start` says.
    fn parts_patch(start: Start) -> FixturePartsPatch {
        let mut mapping = Mapping::new();
        for (i, count) in [3u32, 1].iter().enumerate() {
            mapping.parts.push(Part::new(
                PartId::new(&alloc::format!("path{}", i + 1)).unwrap(),
                PartKind::Shape(Shape::Path(Path::through(
                    &[[0.0, i as f32], [10.0, i as f32]],
                    *count,
                ))),
            ));
        }
        mapping.start = start;
        FixturePartsPatch::new(mapping)
    }

    /// Output `main` with ports of these lamp counts, on `bus:control.out`.
    fn reachable(lamps: &[u32]) -> ReachableOutputs {
        let mut start = 0;
        let ports = lamps
            .iter()
            .enumerate()
            .map(|(key, lamps)| {
                let slice = OutputPortSlice {
                    key: key as u32,
                    start_samples: start,
                    capacity_samples: Some(lamps * 3),
                };
                start += lamps * 3;
                slice
            })
            .collect();
        ReachableOutputs {
            channel: Some(ChannelName(String::from("control.out"))),
            outputs: vec![OutputPortTable {
                name: String::from("main"),
                ports,
            }],
        }
    }

    fn key(layout: i64, tables: u64) -> PartsPatchKey {
        (
            Revision::new(layout),
            PortTablesKey {
                structure_epoch: tables,
                output_identities: Revision::new(0),
            },
            ColorOrder::Rgb,
        )
    }

    fn lamps_of(runs: &[PatchedRun]) -> Vec<(u32, u32, u32)> {
        runs.iter().map(|r| (r.start, r.count, r.lamp)).collect()
    }
}
