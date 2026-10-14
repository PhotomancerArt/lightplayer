//! The patch: which lamp lands on which pixel of which port.
//!
//! What is authored is a fixture's **rules**, in order. What they make —
//! the effective patch — is derived every time and never stored (D7), so
//! an edit anywhere (a ring's count, a port's size, a deleted output) moves
//! the patch with it and nothing goes stale.
//!
//! There is one rule so far, **auto**: this fixture's lamps, in authoring
//! order (objects in tree order, lamps in data order), fill that output's
//! ports in order, after any fixture earlier in the project that also
//! auto-patches to it. An object can split across two ports.
//!
//! A rule whose output no longer exists places nothing and is reported as
//! a problem; it is never retargeted on its own. Its lamps fall to the
//! next rule, or are left unpatched.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::node::NodeId;
use crate::notice::Notice;
use crate::object::ObjectId;
use crate::project::Project;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchRule {
    /// Fill `output`'s ports, in order, after earlier fixtures.
    Auto { output: NodeId },
}

impl PatchRule {
    pub fn output(&self) -> &NodeId {
        match self {
            PatchRule::Auto { output } => output,
        }
    }
}

/// Some lamps of one object, landing on consecutive pixels of one port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub fixture: NodeId,
    pub object: ObjectId,
    /// Lamp indices, in data order.
    pub lamps: Range<u32>,
    pub output: NodeId,
    pub port: String,
    /// The port pixel lamp `lamps.start` lands on.
    pub start_pixel: u32,
}

/// Lamps that light nothing: past the end of an output's ports, or in a
/// fixture with no rule that works.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unpatched {
    pub fixture: NodeId,
    pub object: ObjectId,
    pub lamps: Range<u32>,
}

/// A rule that can't do what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProblem {
    pub fixture: NodeId,
    /// Its index in the fixture's rules.
    pub rule: usize,
    /// The output it names, which no longer exists.
    pub missing: NodeId,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectivePatch {
    pub spans: Vec<Span>,
    pub unpatched: Vec<Unpatched>,
    pub problems: Vec<PatchProblem>,
}

impl EffectivePatch {
    /// Pixels used on one port.
    pub fn used(&self, output: &NodeId, port: &str) -> u32 {
        self.spans
            .iter()
            .filter(|s| &s.output == output && s.port == port)
            .map(|s| s.lamps.len() as u32)
            .sum()
    }
}

impl Project {
    pub fn effective_patch(&self) -> EffectivePatch {
        let mut patch = EffectivePatch::default();
        // Per output: the port being filled, and how far into it.
        let mut cursors: BTreeMap<NodeId, (usize, u32)> = BTreeMap::new();
        for n in self.fixtures() {
            let Some(f) = n.fixture() else { continue };
            let objects = self.objects_in(&n.id);
            let mut placed = false;
            for (i, rule) in f.rules.iter().enumerate() {
                let PatchRule::Auto { output } = rule;
                let Some(out) = self.output(output) else {
                    patch.problems.push(PatchProblem {
                        fixture: n.id.clone(),
                        rule: i,
                        missing: output.clone(),
                    });
                    continue;
                };
                if placed {
                    continue;
                }
                placed = true;
                let (port_i, offset) = cursors.entry(output.clone()).or_insert((0, 0));
                for o in &objects {
                    let n_lamps = o.lamps.len() as u32;
                    let mut next = 0;
                    while next < n_lamps {
                        let Some(port) = out.ports.get(*port_i) else {
                            patch.unpatched.push(Unpatched {
                                fixture: n.id.clone(),
                                object: o.id.clone(),
                                lamps: next..n_lamps,
                            });
                            break;
                        };
                        let room = port.pixels.saturating_sub(*offset);
                        if room == 0 {
                            *port_i += 1;
                            *offset = 0;
                            continue;
                        }
                        let take = room.min(n_lamps - next);
                        patch.spans.push(Span {
                            fixture: n.id.clone(),
                            object: o.id.clone(),
                            lamps: next..next + take,
                            output: output.clone(),
                            port: port.key.clone(),
                            start_pixel: *offset,
                        });
                        next += take;
                        *offset += take;
                    }
                }
            }
            if !placed {
                for o in &objects {
                    patch.unpatched.push(Unpatched {
                        fixture: n.id.clone(),
                        object: o.id.clone(),
                        lamps: 0..o.lamps.len() as u32,
                    });
                }
            }
        }
        patch
    }

    /// How full a port is: (pixels used, pixels it has).
    pub fn port_fill(&self, output: &NodeId, port: &str) -> (u32, u32) {
        let pixels = self
            .output(output)
            .and_then(|o| o.port(port))
            .map_or(0, |p| p.pixels);
        (self.effective_patch().used(output, port), pixels)
    }

    /// A rule in words: `auto → main`.
    pub fn rule_label(&self, rule: &PatchRule) -> String {
        match rule {
            PatchRule::Auto { output } => match self.node(output) {
                Some(n) => format!("auto → {}", n.name),
                None => format!("auto → {output} (gone)"),
            },
        }
    }
}

impl PatchProblem {
    /// The problem in rustc style: what, why, what to do.
    pub fn notice(&self, p: &Project) -> Notice {
        let fixture = p.node_name(&self.fixture);
        Notice::refused(
            format!(
                "{fixture}'s rule {} patches to {}, which no longer exists",
                self.rule + 1,
                self.missing
            ),
            "its output was deleted, and a rule is never pointed somewhere else on its own",
            format!(
                "⌘Z brings {} back; until then {fixture}'s lamps light nothing",
                self.missing
            ),
        )
    }
}
