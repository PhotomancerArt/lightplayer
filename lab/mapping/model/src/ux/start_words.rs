//! Where a node's start came from, in words (lean Q81; names.html "Flow",
//! "Start", "Override").
//!
//! The patcher records where each of a node's three steps came from
//! (`Walked.sources`: output, port, channel):
//!
//! | source | words | the words select |
//! |---|---|---|
//! | Follows | "follows ring r2" | the segment before it |
//! | the fixture's start (the first thing laid) | "from the fixture's start" | the fixture |
//! | `Flow(Part p)` | "from circle1's flow: one per port" | the part |
//! | `Start(Part p)` | "from panel's start", "…, every copy" in a repeat | the part |
//! | `Start(Fixture)` | "from the fixture's start" | the fixture |
//! | `Override` | "set here", with Back to the flow | — |
//!
//! Direction, offset and off have no recorded source (C35): it is "set
//! here" when the node's own override has it, else "from <part>" for the
//! nearest node above whose override has it (F14), else nothing (the
//! default). The fixture's own start is applied before the walk; its rows
//! read `Mapping.start` itself.
//!
//! Presentation B (`presentation_b.rs`) reads the same facts as one line:
//! the start the flow gives a node with its own override taken away.

use lpc_mapping::patching::{
    Amount, ChannelOp, Flow, OutputOp, Override, PatchTarget, PortOp, StepSource, Unit,
    patch_mapping,
};

use crate::domain::node::NodeId;
use crate::domain::part_ref::{PartRef, TreeRef};
use crate::domain::project::Project;
use crate::domain::target::{Target, output_name_words};
use crate::domain::walked::Walked;
use crate::ux::ui::UiSource;

impl Project {
    /// Where one step of a walked node came from (0 output, 1 port, 2
    /// channel).
    pub fn step_source(&self, w: &Walked, field: usize) -> Option<UiSource> {
        let from = |words: String, target: Target| {
            Some(UiSource::From {
                words,
                target: Some(target),
            })
        };
        match &w.sources[field] {
            StepSource::Override => Some(UiSource::SetHere),
            StepSource::Follows => match self.previous_segment(&w.node) {
                Some(prev) => from(
                    format!("follows {}", self.segment_words(&prev)),
                    self.tree_target(&prev),
                ),
                // The first thing laid: where the fixture starts.
                None => self.fixture_start(&w.fixture).and_then(|_| {
                    from(
                        "from the fixture's start".into(),
                        Target::Node(w.fixture.clone()),
                    )
                }),
            },
            StepSource::Start(t) => {
                let every_copy = match t {
                    PatchTarget::Part(part) => {
                        if self.in_repeat(&PartRef::new(&w.fixture, part)) {
                            ", every copy"
                        } else {
                            ""
                        }
                    }
                    _ => "",
                };
                from(
                    format!("from {}{every_copy}", self.start_owner(&w.fixture, t)),
                    self.rule_owner_target(&w.fixture, t),
                )
            }
            StepSource::Flow(t) => from(
                format!(
                    "from {}: {}",
                    self.flow_owner(&w.fixture, t),
                    flow_words(&self.flow_of(&w.fixture, t))
                ),
                self.rule_owner_target(&w.fixture, t),
            ),
        }
    }

    /// Where a direction, an offset or Off came from, which the patcher
    /// doesn't record (C35): the node's own override, else the nearest
    /// node above whose override has it (F14), else nothing.
    pub fn override_source(
        &self,
        node: &TreeRef,
        has: impl Fn(&Override) -> bool,
    ) -> Option<UiSource> {
        if self.override_of(node).is_some_and(&has) {
            return Some(UiSource::SetHere);
        }
        let mut at = self.placed().parent_of(node);
        while let Some(up) = at {
            if self.override_of(&up).is_some_and(&has) {
                return Some(UiSource::From {
                    words: format!("from {}", self.segment_words(&up)),
                    target: Some(self.tree_target(&up)),
                });
            }
            at = self.placed().parent_of(&up);
        }
        None
    }

    /// What follows a node whose start is its own: "ring r4 and the centre
    /// follow it".
    pub fn follows_it_words(&self, node: &TreeRef) -> Option<String> {
        let names = self.followers(node)?;
        Some(match names.len() {
            0 => return None,
            1 => format!("{} follows it", names[0]),
            _ => format!("{} follow it", and_list(&names)),
        })
    }

    /// What takes an off node's place (F11): "ring r4 and the centre move
    /// back into its place".
    pub fn moves_back_words(&self, node: &TreeRef) -> Option<String> {
        let names = self.followers(node)?;
        Some(match names.len() {
            0 => return None,
            1 => format!("{} moves back into its place", names[0]),
            _ => format!("{} move back into its place", and_list(&names)),
        })
    }

    /// The segments after a node, in its fixture, that only carry on from
    /// it (every step Follows), up to the first with a start of its own.
    fn followers(&self, node: &TreeRef) -> Option<Vec<String>> {
        let patch = self.effective_patch();
        let leaves = self.fixture_segment_ids(&node.fixture);
        let last_mine = self.placed().node(node)?.1.leaves().last()?.id.clone();
        let i = leaves.iter().position(|o| *o == last_mine)?;
        let mut names = Vec::new();
        for o in &leaves[i + 1..] {
            let w = patch.walked(o)?;
            if w.off {
                continue;
            }
            if !w.sources.iter().all(|s| *s == StepSource::Follows) {
                break;
            }
            names.push(self.segment_words(o));
        }
        Some(names)
    }

    /// The segment the walk laid before a node, in its fixture.
    pub fn previous_segment(&self, node: &TreeRef) -> Option<TreeRef> {
        let leaves = self.fixture_segment_ids(&node.fixture);
        let first_mine = self.placed().node(node)?.1.leaves().first()?.id.clone();
        let i = leaves.iter().position(|o| *o == first_mine)?;
        i.checked_sub(1).map(|j| leaves[j].clone())
    }

    fn fixture_segment_ids(&self, fixture: &NodeId) -> Vec<TreeRef> {
        self.placed_segments()
            .iter()
            .filter(|(f, _)| f == fixture)
            .map(|(_, o)| o.id.clone())
            .collect()
    }

    /// Is a part copied: is there a repeat round it?
    pub fn in_repeat(&self, part: &PartRef) -> bool {
        let mut at = self.parent_part(part);
        while let Some(up) = at {
            if self
                .part(&up)
                .is_some_and(|p| matches!(p.kind, lpc_mapping::part::PartKind::Repeat(_)))
            {
                return true;
            }
            at = self.parent_part(&up);
        }
        false
    }

    /// Whose start, in words: "the fixture's start", "panel's start".
    fn start_owner(&self, fixture: &NodeId, t: &PatchTarget) -> String {
        match t {
            PatchTarget::Fixture => "the fixture's start".into(),
            PatchTarget::Part(part) => format!("{part}'s start"),
            PatchTarget::Node(id) => format!(
                "{}'s start",
                self.segment_words(&TreeRef::new(fixture, id.clone()))
            ),
        }
    }

    /// Whose flow, in words: "the fixture's flow", "circle1's flow".
    fn flow_owner(&self, fixture: &NodeId, t: &PatchTarget) -> String {
        match t {
            PatchTarget::Fixture => "the fixture's flow".into(),
            PatchTarget::Part(part) => format!("{part}'s flow"),
            PatchTarget::Node(id) => format!(
                "{}'s flow",
                self.segment_words(&TreeRef::new(fixture, id.clone()))
            ),
        }
    }

    fn flow_of(&self, fixture: &NodeId, t: &PatchTarget) -> Flow {
        let Some(f) = self.fixture(fixture) else {
            return Flow::EndToEnd;
        };
        match t {
            PatchTarget::Fixture => f.mapping.flow.clone(),
            PatchTarget::Part(part) => f.part(part).map_or(Flow::EndToEnd, |p| p.flow.clone()),
            PatchTarget::Node(id) => f.part(&id.part).map_or(Flow::EndToEnd, |p| p.flow.clone()),
        }
    }

    /// What a start's or flow's words select: the fixture, or the part.
    pub fn rule_owner_target(&self, fixture: &NodeId, t: &PatchTarget) -> Target {
        match t {
            PatchTarget::Fixture => Target::Node(fixture.clone()),
            PatchTarget::Part(part) => Target::Part(PartRef::new(fixture, part)),
            PatchTarget::Node(id) => self.tree_target(&TreeRef::new(fixture, id.clone())),
        }
    }

    /// Presentation B's line: the start the flow gives a node, with its own
    /// override taken away. "circle1's flow: port D2, from its first lamp".
    pub fn flow_start_line(&self, node: &TreeRef) -> Option<String> {
        let f = self.fixture(&node.fixture)?;
        let tree = self.placed().tree(&node.fixture)?;
        let mut mapping = f.mapping.clone();
        mapping.overrides.remove(&node.id);
        let patching = patch_mapping(&mapping, tree, &self.port_tables(&node.fixture));
        let w = patching.walked(&node.id)?;
        let whose = match &w.sources[1] {
            StepSource::Flow(t) => self.flow_owner(&node.fixture, t),
            StepSource::Start(t) => self.start_owner(&node.fixture, t),
            StepSource::Follows => match self.previous_segment(node) {
                Some(prev) => format!("after {}", self.segment_words(&prev)),
                None => "the fixture's start".into(),
            },
            // Another node's own (it can't be this one's: it was taken
            // away).
            StepSource::Override => "set above it".into(),
        };
        let (output, port, channel) = match patching.segment(&node.id) {
            Some(s) => (
                Some(s.address.output.clone()),
                Some(s.address.port),
                s.address.channel,
            ),
            None => (w.at.output.clone(), w.at.port, w.at.channel),
        };
        let per = self.channels_per_lamp_of(node).max(1);
        let place = match (output, port) {
            (Some(o), Some(k)) => {
                let pin = match self.output_named(o.as_str()) {
                    Some(out) => self.pin_words(&out, &k.0.to_string()),
                    None => format!("port {} of {}", k.0, output_name_words(o.as_str())),
                };
                let from = match channel {
                    0 => "from its first lamp".to_string(),
                    c if c % per == 0 => format!("from lamp {}", c / per),
                    c => format!("from channel {c}"),
                };
                format!("port {pin}, {from}")
            }
            _ => "nowhere yet".into(),
        };
        Some(format!("{whose}: {place}"))
    }

    /// Debug's cursor ops: the patcher's own step for each field, and where
    /// it came from. "Port · Add 1 · from circle1's flow: one per port".
    pub fn cursor_ops_lines(&self, w: &Walked) -> Vec<String> {
        let source = |i: usize| match self.step_source(w, i) {
            Some(UiSource::From { words, .. }) => words,
            Some(UiSource::SetHere) => "set here".into(),
            Some(UiSource::Mixed) => "mixed".into(),
            None => "nothing above it".into(),
        };
        let output = match &w.steps.output {
            OutputOp::Keep => "Keep".to_string(),
            OutputOp::Set(name) => format!("Set {}", name.as_str()),
        };
        let port = match &w.steps.port {
            PortOp::Keep => "Keep".to_string(),
            PortOp::Set { key, plus: 0 } => format!("Set {}", key.0),
            PortOp::Set { key, plus } => format!("Set {} + {plus}", key.0),
            PortOp::Add(n) => format!("Add {n}"),
        };
        let channel = match w.steps.channel {
            ChannelOp::Keep => "Keep".to_string(),
            ChannelOp::Set(a) => format!("Set {}", amount_words(a)),
            ChannelOp::Add(a) => format!("Add {}", amount_words(a)),
        };
        let mut lines = vec![
            format!("Output · {output} · {}", source(0)),
            format!("Port · {port} · {}", source(1)),
            format!("Channel · {channel} · {}", source(2)),
        ];
        if w.off {
            lines.push("Off: no steps taken".into());
        }
        lines
    }
}

/// A flow in words: "end to end", "one per port".
pub fn flow_words(flow: &Flow) -> &'static str {
    match flow {
        Flow::EndToEnd => "end to end",
        Flow::OnePerPort { .. } => "one per port",
    }
}

fn amount_words(a: Amount) -> String {
    match a.unit {
        Unit::Lamps if a.n == 1 => "1 lamp".into(),
        Unit::Lamps => format!("{} lamps", a.n),
        Unit::Channels if a.n == 1 => "1 channel".into(),
        Unit::Channels => format!("{} channels", a.n),
    }
}

/// "a", "a and b", "a, b and c", "a, b and 3 more".
fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        _ if names.len() <= 4 => {
            let (last, rest) = names.split_last().expect("two or more");
            format!("{} and {last}", rest.join(", "))
        }
        _ => format!("{}, {} and {} more", names[0], names[1], names.len() - 2),
    }
}
