//! The product's patch as it is today (patch format 3), and the step that
//! turns it into the walker's terms: the upgrade step in miniature.
//!
//! The product patches by a different arithmetic (`lpc-mapping`'s
//! `resolve_patch`, the engine's `plan_fragments`): an output's ports are
//! one flat wire; a row puts one object's lamps at a place on it, reversed
//! and turned; a fixture on auto flows its other objects on after its own
//! rows on that output, or — with no rows — after every run on that output,
//! one fixture after another in attach order. That is the one place lamps
//! flow from one fixture into the next, which the walker never does
//! (theory: "nothing flows across fixtures").
//!
//! The lab keeps that arithmetic for two things only:
//!
//! - **opening** a project: [`Project::freeze_product_patches`] gives each
//!   fixture a Start where its first lamp lands today, and an override on
//!   each object the walker would otherwise put somewhere else (as few as
//!   it can), so the walker lands every lamp where the product does;
//! - **saving** one (`lab-mapping-files`): what the product would make of a
//!   file is checked against the walker before the file is kept as it is.

use std::collections::BTreeMap;

use crate::domain::cursor_op::{Amount, ChannelOp, OutputOp, PortOp};
use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;
use crate::domain::object_override::{Direction, ObjectOverride};
use crate::domain::patch::{ObjectRun, Run, lamps_of};
use crate::domain::patch_rule::{Children, Start};
use crate::domain::project::Project;

/// One row of a product patch file: an object at a pixel of a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRow {
    pub object: ObjectId,
    pub output: NodeId,
    pub port: String,
    pub pixel: u32,
    pub reversed: bool,
    pub rotation: u32,
}

/// One fixture's product patch: its rows, and the output it flows the rest
/// onto, when it is on auto.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProductPatch {
    pub auto: Option<NodeId>,
    pub rows: Vec<ProductRow>,
}

impl Project {
    /// Every object's run as the product would lay these patches: one per
    /// fixture, by its node id; a fixture with none patches nothing.
    pub fn product_runs(&self, patches: &BTreeMap<NodeId, ProductPatch>) -> Vec<ObjectRun> {
        let mut runs: Vec<Run> = Vec::new();
        let mut flows: Vec<(NodeId, Vec<Run>)> = Vec::new();
        for n in self.attach_order() {
            let (Some(_), Some(patch)) = (n.fixture(), patches.get(&n.id)) else {
                continue;
            };
            let lamps = lamps_of(self, n);
            let run = |object: &ObjectId, output: &NodeId, wire: u32, rev: bool, rot: u32| {
                let (start, count) = lamps.object(object)?;
                Some(Run {
                    fixture: n.id.clone(),
                    object: object.clone(),
                    start,
                    count,
                    output: output.clone(),
                    wire,
                    reversed: rev,
                    rotation: rot % count.max(1),
                    limit: None,
                })
            };
            let mut own = Vec::new();
            for r in &patch.rows {
                let Some(start) = self.output(&r.output).and_then(|o| o.port_start(&r.port)) else {
                    continue;
                };
                if own.iter().any(|x: &Run| x.object == r.object) {
                    continue; // One row per object: the first wins.
                }
                own.extend(run(&r.object, &r.output, start + r.pixel, r.reversed, r.rotation));
            }
            let Some(output) = patch.auto.clone().filter(|o| self.output(o).is_some()) else {
                runs.extend(own);
                continue;
            };
            let flowing: Vec<Run> = lamps
                .objects
                .iter()
                .filter(|(o, _, _)| !own.iter().any(|r| &r.object == o))
                .filter_map(|(o, _, _)| run(o, &output, 0, false, 0))
                .collect();
            if own.is_empty() {
                flows.push((output, flowing));
                continue;
            }
            let mut cursor = own
                .iter()
                .filter(|r| r.output == output)
                .map(Run::wire_end)
                .max()
                .unwrap_or(0);
            runs.extend(own);
            for mut r in flowing {
                r.wire = cursor;
                cursor = r.wire_end();
                runs.push(r);
            }
        }
        // Fixtures that only flow go after every run on their output.
        let mut cursors: BTreeMap<NodeId, u32> = BTreeMap::new();
        for r in &runs {
            let c = cursors.entry(r.output.clone()).or_default();
            *c = (*c).max(r.wire_end());
        }
        for (output, flowing) in flows {
            let cursor = cursors.entry(output).or_default();
            for mut r in flowing {
                r.wire = *cursor;
                *cursor = r.wire_end();
                runs.push(r);
            }
        }
        runs.iter().map(Run::object_run).collect()
    }

    /// Put each fixture's product patch into the walker's terms, so the
    /// walker lands every lamp where the product does: a Start where its
    /// first lamp lands, and an override only where the walker, carrying
    /// on from there, would land an object somewhere else. Returns the
    /// objects it couldn't match (an object the product lays across two
    /// ports, which the walker never does).
    pub fn freeze_product_patches(
        &mut self,
        patches: &BTreeMap<NodeId, ProductPatch>,
    ) -> Vec<ObjectId> {
        let truth = self.product_runs(patches);
        let mut unmatched = Vec::new();
        let fixtures: Vec<NodeId> = self.fixtures().iter().map(|n| n.id.clone()).collect();
        for fixture in fixtures {
            unmatched.extend(self.freeze_fixture(&fixture, &truth));
        }
        unmatched
    }

    fn freeze_fixture(&mut self, fixture: &NodeId, truth: &[ObjectRun]) -> Vec<ObjectId> {
        let Some(n) = self.node(fixture) else {
            return Vec::new();
        };
        let lamps = lamps_of(self, n);
        let want: BTreeMap<&ObjectId, &ObjectRun> = truth
            .iter()
            .filter(|r| &r.fixture == fixture)
            .map(|r| (&r.object, r))
            .collect();
        let first = lamps.objects.iter().find_map(|(o, _, _)| want.get(o));
        let start = match first.and_then(|r| self.wire_place(r)) {
            Some((output, port, pixel)) => Start::At {
                output,
                port,
                channel: Amount::px(pixel),
            },
            None => Start::Follows,
        };
        {
            let Some(f) = self.fixture_mut(fixture) else {
                return Vec::new();
            };
            f.start = start;
            f.children = Children::EndToEnd;
            f.rules.clear();
            f.overrides.clear();
        }
        let mut skip: Vec<ObjectId> = Vec::new();
        let mut overrides: BTreeMap<ObjectId, ObjectOverride> = BTreeMap::new();
        loop {
            let n = self.node(fixture).expect("still there");
            let walk = self.walk_fixture_with(n, &lamps, &overrides);
            // A run the walker clipped at its port's end is never a match:
            // the product lays it on across the next port.
            let got: BTreeMap<&ObjectId, ObjectRun> = walk
                .runs
                .iter()
                .filter(|r| r.limit.is_none())
                .map(|r| (&r.object, r.object_run()))
                .collect();
            let wrong = lamps.objects.iter().map(|(o, _, _)| o).find(|o| {
                !skip.contains(o)
                    && match (want.get(o), got.get(o)) {
                        (None, None) => false,
                        (Some(a), Some(b)) => !same_place(a, b),
                        _ => true,
                    }
            });
            let Some(object) = wrong.cloned() else {
                if let Some(f) = self.fixture_mut(fixture) {
                    f.overrides = overrides;
                }
                return skip;
            };
            let fix = match want.get(&object) {
                None => ObjectOverride {
                    off: true,
                    ..ObjectOverride::default()
                },
                Some(r) => {
                    let mut o = ObjectOverride::default();
                    let lands_elsewhere = got
                        .get(&object)
                        .is_none_or(|b| b.output != r.output || b.wire != r.wire);
                    if lands_elsewhere && let Some((output, port, pixel)) = self.wire_place(r) {
                        o.output = Some(OutputOp::Set(output));
                        o.port = Some(PortOp::set(port));
                        o.channel = Some(ChannelOp::Set(Amount::px(pixel)));
                    }
                    if r.reversed {
                        o.direction = Some(Direction::Reversed);
                    }
                    if r.rotation != 0 {
                        o.offset = Some(r.rotation);
                    }
                    o
                }
            };
            if overrides.get(&object) == Some(&fix) {
                // It can't be said in the walker's terms: leave it, say so.
                skip.push(object);
                continue;
            }
            overrides.insert(object, fix);
        }
    }

    /// The (output, port, pixel) a run on the flat wire starts at.
    fn wire_place(&self, r: &ObjectRun) -> Option<(NodeId, String, u32)> {
        let out = self.output(&r.output)?;
        let (i, pixel) = out.port_at(r.wire)?;
        Some((r.output.clone(), out.ports[i].key.clone(), pixel))
    }
}

/// Two runs that put the same lamps in the same place.
fn same_place(a: &ObjectRun, b: &ObjectRun) -> bool {
    a.output == b.output
        && a.wire == b.wire
        && a.count == b.count
        && a.reversed == b.reversed
        && a.rotation == b.rotation
}
