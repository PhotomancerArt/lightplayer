//! A format-6 fixture's patching, kept current: what it holds between
//! frames and when it re-patches (lean Q36, Q37, Q42, Q43).
//!
//! **Resident:** the mapping (parts and patching), the rendered tree without
//! its positions (they live once, in the fixture's carrier), the runs it
//! publishes, and its status. The [`Patching`] itself, with what every node
//! did and why, lives only inside [`FixturePartsPatch::ensure_with`].
//!
//! **When:** only when the key moves: the layout's version (a re-render),
//! the port tables' key (a name, a port, a count, or the outputs on the
//! channel), or the fixture's color order (an order refusal depends on it).
//! A steady frame compares three small values and asks nothing.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_mapping::part::Tree;
use lpc_mapping::patching::{OutputName, OutputPorts, Patching, Port, PortKey, patch_mapping};
use lpc_model::Revision;
use lpc_model::nodes::fixture::ColorOrder;

use crate::node::{PatchedRun, PortTablesKey, ReachableOutputs, TickContext};
use crate::nodes::fixture::fixture_parts_source::FixturePartsSource;
use crate::nodes::fixture::parts_problem_text::{problem_text, refusal_text};
use crate::nodes::fixture::patching_runs::patching_runs;

/// What a patch was made under.
pub(crate) type PartsPatchKey = (Revision, PortTablesKey, ColorOrder);

/// A format-6 fixture's patching.
pub(crate) struct FixturePartsPatch {
    /// The mapping and its fit facts.
    pub source: FixturePartsSource,
    /// The rendered tree, positions taken out.
    pub tree: Tree,
    key: Option<PartsPatchKey>,
    /// The runs the fixture publishes, every one naming its output.
    pub runs: Vec<PatchedRun>,
    /// The first problem in words, with how many more; `None` when nothing
    /// is wrong.
    pub status: Option<String>,
}

impl FixturePartsPatch {
    pub fn new(source: FixturePartsSource, tree: Tree) -> Self {
        Self {
            source,
            tree,
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
    /// it, measured in M2 P3), paid by every fixture, format ≤ 5 included,
    /// on every frame. Out of line, it is paid only while re-patching.
    #[cold]
    #[inline(never)]
    fn repatch(
        &mut self,
        key: PartsPatchKey,
        reachable: ReachableOutputs,
        color_order: ColorOrder,
    ) -> bool {
        self.key = Some(key);
        let ports = patcher_ports(&reachable);
        let patching = patch_mapping(&self.source.mapping, &self.tree, &ports);
        drop(ports);
        let placed = patching_runs(&patching, &self.tree, &reachable.outputs, color_order);
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
    use lpc_mapping::part::{Mapping, Part, PartId, PartKind, Path, Shape, render};
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
        parts.ensure_with(key(1, 1), || reachable(&[3, 10]), ColorOrder::Rgb);
        assert_eq!(lamps_of(&parts.runs), vec![(0, 3, 3), (3, 1, 6)]);
        // Port 0 grows to 5 lamps: the edit moved the tables' key (P1).
        assert!(parts.ensure_with(key(1, 2), || reachable(&[5, 10]), ColorOrder::Rgb));
        assert_eq!(
            lamps_of(&parts.runs),
            vec![(0, 3, 5), (3, 1, 8)],
            "everything on port 1 moved with its start"
        );
        // Port 1 shrinks to 3 lamps: path2 no longer fits and wraps to port 2,
        // which isn't there: it is past the last port.
        assert!(parts.ensure_with(key(1, 3), || reachable(&[5, 3]), ColorOrder::Rgb));
        assert_eq!(lamps_of(&parts.runs), vec![(0, 3, 5)]);
        let status = parts.status.as_deref().unwrap_or_default();
        assert!(status.contains("path2 runs past the last port"), "{status}");
    }

    /// A format-6 fixture never asks to be auto-flowed: it is manual, and it
    /// answers its placement even before it has a run.
    #[test]
    fn a_format_6_fixture_never_asks_to_be_auto_flowed() {
        use crate::node::NodeRuntime;
        use lpc_model::nodes::fixture::{FixtureSamplingConfig, MappingConfig};

        let source = parts_patch(Start::at("main", 0, Amount::lamps(0))).source;
        let fixture = crate::nodes::FixtureNode::new(
            lpc_model::NodeId::new(1),
            MappingConfig::path_points_vec(Vec::new(), 1.0),
            FixtureSamplingConfig::default(),
            Revision::new(1),
        )
        .with_parts_mapping(source, 10, 2);
        assert!(fixture.control_patch_manual());
        assert_eq!(fixture.control_patch_placement(), Some(&[][..]));
        assert_eq!(fixture.runtime_status(), None);
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
        assert_eq!(parts.runs.len(), 2);
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
        let tree = render(&mapping).unwrap();
        FixturePartsPatch::new(
            FixturePartsSource {
                mapping,
                canvas: None,
                sample_diameter: 1.0,
            },
            tree,
        )
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
