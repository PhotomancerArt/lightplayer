//! From placements to runs on the wire: where each object ends up, and in
//! what order (`patch_place.rs` is the vocabulary, `patch.rs` the result).
//!
//! **Each setting comes from the nearest placement up the object's chain**
//! — the object, the component that makes it, the groups it sits in, the
//! fixture — that sets it (plan D8, D9). Then, per fixture:
//!
//! 1. **Placed at an address.** Objects whose wire comes from one placement
//!    (`Wire::At`) make one run there, end to end in tree order (plan D10).
//!    When that placement is on a component or the fixture and its
//!    direction is reversed, the objects go in the other order. Each object
//!    runs the way its own resolved direction says, and turns by its own
//!    turn. An object placed on its own leaves its parent's run, and the
//!    others close up behind it.
//! 2. **Auto.** Objects on auto flow onto that output end to end in tree
//!    order — after the fixture's own placed runs there when it has any,
//!    else after every run on the output, one fixture after another in
//!    attach order (the product's arithmetic, `patch.rs`).
//! 3. **Off, or nothing set anywhere**: on no pixel.
//!
//! A placement naming something gone (its target, output or port) is
//! reported and the part that can't work is left out, so its objects
//! follow their parent instead: never retargeted on its own.

use std::collections::HashMap;

use crate::domain::node::{Node, NodeId};
use crate::domain::object::ObjectId;
use crate::domain::patch::{FixtureLamps, Gone, PatchProblem, Run};
use crate::domain::patch_place::{Direction, PatchPlace, PatchTarget, Wire};
use crate::domain::project::Project;

/// What one object ends up with, and where each setting comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub fixture: NodeId,
    pub object: ObjectId,
    /// Where its lamps go; `None` when nothing up its chain says.
    pub wire: Option<Wire>,
    /// Which placement set the wire.
    pub wire_from: Option<PatchTarget>,
    pub direction: Direction,
    /// Which placement set the direction (`None`: as drawn, by default).
    pub direction_from: Option<PatchTarget>,
    pub turn: u32,
}

impl Project {
    /// The chain a placement on `target` inherits through: the target, the
    /// groups above it, the fixture.
    pub fn place_chain(&self, target: &PatchTarget) -> Vec<PatchTarget> {
        let mut out = vec![target.clone()];
        let mut component = match target {
            PatchTarget::Fixture => return out,
            PatchTarget::Component(c) => self.parent_group(c).cloned(),
            PatchTarget::Object(o) => Some(o.component.clone()),
        };
        while let Some(c) = component {
            component = self.parent_group(&c).cloned();
            out.push(PatchTarget::Component(c));
        }
        out.push(PatchTarget::Fixture);
        out
    }

    /// One fixture's placements, checked: each one whose target is gone
    /// dropped, and a wire naming a gone output or port left out. Problems
    /// are reported as they are found.
    fn checked_places(
        &self,
        n: &Node,
        lamps: &FixtureLamps,
        problems: &mut Vec<PatchProblem>,
    ) -> HashMap<PatchTarget, PatchPlace> {
        let mut out = HashMap::new();
        let Some(f) = n.fixture() else {
            return out;
        };
        for (i, place) in f.places.iter().enumerate() {
            let problem = |gone| PatchProblem {
                fixture: n.id.clone(),
                place: i,
                gone,
            };
            match &place.on {
                PatchTarget::Fixture => {}
                PatchTarget::Component(c) => {
                    if self.fixture_of(c).map(|x| &x.id) != Some(&n.id) {
                        problems.push(problem(Gone::Component(c.clone())));
                        continue;
                    }
                }
                PatchTarget::Object(o) => {
                    if lamps.object(o).is_none() {
                        problems.push(problem(Gone::Object(o.clone())));
                        continue;
                    }
                }
            }
            let mut place = place.clone();
            match &place.wire {
                Some(Wire::Auto { output }) if self.output(output).is_none() => {
                    problems.push(problem(Gone::Output(output.clone())));
                    place.wire = None;
                }
                Some(Wire::At { output, port, .. }) => match self.output(output) {
                    None => {
                        problems.push(problem(Gone::Output(output.clone())));
                        place.wire = None;
                    }
                    Some(o) if o.port(port).is_none() => {
                        problems.push(problem(Gone::Port {
                            output: output.clone(),
                            port: port.clone(),
                        }));
                        place.wire = None;
                    }
                    Some(_) => {}
                },
                _ => {}
            }
            // One placement per thing: the first wins.
            out.entry(place.on.clone()).or_insert(place);
        }
        out
    }

    /// Every object of a fixture with its settings, in tree order.
    pub(crate) fn resolve_fixture(
        &self,
        n: &Node,
        lamps: &FixtureLamps,
        problems: &mut Vec<PatchProblem>,
    ) -> Vec<Resolved> {
        let places = self.checked_places(n, lamps, problems);
        lamps
            .objects
            .iter()
            .map(|(id, _, _)| {
                let chain = self.place_chain(&PatchTarget::Object(id.clone()));
                let set = |pick: &dyn Fn(&PatchPlace) -> bool| {
                    chain
                        .iter()
                        .find(|t| places.get(t).is_some_and(pick))
                        .cloned()
                };
                let wire_from = set(&|p| p.wire.is_some());
                let direction_from = set(&|p| p.direction.is_some());
                Resolved {
                    fixture: n.id.clone(),
                    object: id.clone(),
                    wire: wire_from.as_ref().and_then(|t| places.get(t)?.wire.clone()),
                    direction: direction_from
                        .as_ref()
                        .and_then(|t| places.get(t)?.direction)
                        .unwrap_or_default(),
                    wire_from,
                    direction_from,
                    turn: places
                        .get(&PatchTarget::Object(id.clone()))
                        .and_then(|p| p.turn)
                        .unwrap_or(0),
                }
            })
            .collect()
    }

    /// The runs one fixture's objects make: its placed runs now, and its
    /// auto flows either after them (it has placed runs) or handed back to
    /// go after everything on their output (`Flow`).
    pub(crate) fn fixture_runs(
        &self,
        n: &Node,
        lamps: &FixtureLamps,
        resolved: &[Resolved],
    ) -> (Vec<Run>, Vec<Flow>) {
        let places: HashMap<PatchTarget, Direction> = n
            .fixture()
            .map(|f| {
                f.places
                    .iter()
                    .filter_map(|p| Some((p.on.clone(), p.direction?)))
                    .collect()
            })
            .unwrap_or_default();
        // The direction a placement on `t` resolves to, up its chain.
        let direction_at = |t: &PatchTarget| {
            self.place_chain(t)
                .iter()
                .find_map(|x| places.get(x).copied())
                .unwrap_or_default()
        };
        let run = |i: usize, output: &NodeId, wire: u32| {
            let (object, start, count) = &lamps.objects[i];
            let r = &resolved[i];
            Run {
                fixture: n.id.clone(),
                object: object.clone(),
                start: *start,
                count: *count,
                output: output.clone(),
                wire,
                reversed: r.direction.is_reversed(),
                rotation: r.turn % count.max(&1),
            }
        };

        // 1. Placed at an address: one run per placement, its objects end
        // to end.
        let mut groups: Vec<(PatchTarget, Vec<usize>)> = Vec::new();
        for (i, r) in resolved.iter().enumerate() {
            if let (Some(Wire::At { .. }), Some(from)) = (&r.wire, &r.wire_from) {
                match groups.iter_mut().find(|(t, _)| t == from) {
                    Some((_, members)) => members.push(i),
                    None => groups.push((from.clone(), vec![i])),
                }
            }
        }
        let mut own: Vec<Run> = Vec::new();
        for (from, mut members) in groups {
            let Some(Wire::At {
                output,
                port,
                pixel,
            }) = resolved[members[0]].wire.clone()
            else {
                continue;
            };
            let Some(start) = self.output(&output).and_then(|o| o.port_start(&port)) else {
                continue;
            };
            if !matches!(from, PatchTarget::Object(_)) && direction_at(&from).is_reversed() {
                members.reverse();
            }
            let mut cursor = start.saturating_add(pixel);
            for i in members {
                own.push(run(i, &output, cursor));
                cursor = cursor.saturating_add(lamps.objects[i].2);
            }
        }

        // 2. Auto: one flow per output, its objects end to end.
        let mut flows: Vec<(NodeId, Vec<usize>)> = Vec::new();
        for (i, r) in resolved.iter().enumerate() {
            if let Some(Wire::Auto { output }) = &r.wire {
                match flows.iter_mut().find(|(o, _)| o == output) {
                    Some((_, members)) => members.push(i),
                    None => flows.push((output.clone(), vec![i])),
                }
            }
        }
        if own.is_empty() {
            let flows = flows
                .into_iter()
                .map(|(output, members)| Flow {
                    output,
                    runs: members
                        .iter()
                        .map(|&i| run(i, &NodeId::new(""), 0))
                        .collect(),
                })
                .collect();
            return (own, flows);
        }
        let mut runs = Vec::new();
        for (output, members) in flows {
            let mut cursor = own
                .iter()
                .filter(|r| r.output == output)
                .map(Run::wire_end)
                .max()
                .unwrap_or(0);
            for i in members {
                runs.push(run(i, &output, cursor));
                cursor = cursor.saturating_add(lamps.objects[i].2);
            }
        }
        own.extend(runs);
        (own, Vec::new())
    }

    /// The components a fixture's placements may name, and whether each
    /// still exists there: for the inspector's words.
    pub fn place_target_exists(&self, fixture: &NodeId, t: &PatchTarget) -> bool {
        match t {
            PatchTarget::Fixture => self.fixture(fixture).is_some(),
            PatchTarget::Component(c) => self.fixture_of(c).map(|n| &n.id) == Some(fixture),
            PatchTarget::Object(o) => self.object(o).is_some(),
        }
    }
}

/// A fixture's objects on auto, waiting to go on after everything already
/// on their output: their runs, without a place on the wire yet.
pub(crate) struct Flow {
    pub output: NodeId,
    pub runs: Vec<Run>,
}

impl Flow {
    /// Lay the runs end to end from `cursor` on the flow's output; returns
    /// where they end.
    pub(crate) fn lay(self, mut cursor: u32, out: &mut Vec<Run>) -> u32 {
        for mut r in self.runs {
            r.output = self.output.clone();
            r.wire = cursor;
            cursor = cursor.saturating_add(r.count);
            out.push(r);
        }
        cursor
    }
}

/// Words for a placement's target: `the fixture`, `circle1`, `sector/2`.
pub(crate) fn target_words(p: &Project, fixture: &NodeId, t: &PatchTarget) -> String {
    match t {
        PatchTarget::Fixture => p.node_name(fixture),
        PatchTarget::Component(c) => c.to_string(),
        PatchTarget::Object(o) => p.object_words(o),
    }
}
