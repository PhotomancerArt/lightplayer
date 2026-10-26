//! The walker: how a fixture's objects get onto the wire (theory
//! "Patching").
//!
//! It walks one fixture's object tree (`object_tree.rs`) depth first,
//! holding a cursor: an output, a port, a channel (`cursor_op.rs`).
//!
//! 1. **The fixture's own Start** puts the cursor down. It must say where
//!    (At): with no start the fixture patches nothing, and says so. Nothing
//!    flows in from another fixture.
//! 2. **Entering a node**, each cursor field takes one step: the node's
//!    override for that field if it has one, else its own Start if that
//!    says something, else what its parent's Children rule renders for it
//!    (child k), else Keep. One step per field, never two in a row: an
//!    override replaces its rule's step, it doesn't add to it.
//! 3. **A leaf** (an object that holds lamps) makes one span at the cursor
//!    and moves the cursor past its lamps (lamps × channels per lamp).
//!    - It doesn't fit what is left of its port: it starts on the next port
//!      at channel 0 (universe wrapping) — unless it is the first leaf laid
//!      since a port or channel was Set (its own step, its container's, or
//!      the fixture's Start), in which case it stays, is reported, and
//!      lands as far as the port reaches. A Set is a promise about where
//!      something starts; it isn't moved.
//!    - Past the last port is an error: it, and everything after it that
//!      only follows on, lands nowhere — until a step Sets a port again (a
//!      new start). Outputs have no order, so it never moves on to another
//!      output.
//!    - A start part-way into a pixel is modelled but lands nowhere (the
//!      board, and the product, speak in pixels), and is reported.
//!    - Off: it makes no span, and the cursor doesn't move.
//! 4. **Leaving a node never moves the cursor.** Only a step does.
//!
//! The result is the effective patch's runs (`patch.rs` lays them on the
//! pixels) and, for every node, what it did and why, for the inspector.

use crate::domain::component::ComponentId;
use crate::domain::cursor_op::{ChannelOp, Cursor, CursorOps, OutputOp, PortOp};
use crate::domain::node::{Node, NodeId};
use crate::domain::object::ObjectId;
use crate::domain::object_override::{Direction, ObjectOverride};
use crate::domain::object_tree::{ObjectNode, ObjectNodeKind};
use crate::domain::patch::{FixtureLamps, PatchProblem, ProblemKind, Run};
use crate::domain::patch_rule::ComponentRules;
use crate::domain::patch_target::PatchTarget;
use crate::domain::project::Project;

/// Where a step came from: what the inspector says beside a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepSource {
    /// Nothing set it: the walk carried on.
    Follows,
    /// The node's own override.
    Override,
    /// A Start rule: the fixture's, or the component's own.
    Start(PatchTarget),
    /// The Children rule of the container it sits in (the fixture's, or a
    /// component's).
    Children(PatchTarget),
}

/// What one node of the object tree did on the walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Walked {
    pub fixture: NodeId,
    pub object: ObjectId,
    /// The steps it took, after its override.
    pub ops: CursorOps,
    /// Where each step came from: output, port, channel.
    pub sources: [StepSource; 3],
    /// The cursor once its steps were taken: where its first lamp goes (a
    /// leaf that wrapped started on the next port; see `landed`).
    pub at: Cursor,
    /// For a leaf: where its span really started, after any wrap.
    pub landed: Option<Cursor>,
    /// For a leaf: it moved on to the next port because it didn't fit.
    pub wrapped: bool,
    pub off: bool,
    pub direction: Direction,
    pub offset: u32,
}

/// One fixture's walk.
#[derive(Debug, Default)]
pub(crate) struct Walk {
    pub runs: Vec<Run>,
    pub walked: Vec<Walked>,
    pub problems: Vec<PatchProblem>,
}

impl Project {
    /// Walk one fixture's object tree.
    pub(crate) fn walk_fixture(&self, n: &Node, lamps: &FixtureLamps) -> Walk {
        match n.fixture() {
            Some(f) => self.walk_fixture_with(n, lamps, &f.overrides),
            None => Walk::default(),
        }
    }

    /// Walk one fixture's object tree with these overrides in place of its
    /// own (to try some without editing the project).
    pub(crate) fn walk_fixture_with(
        &self,
        n: &Node,
        lamps: &FixtureLamps,
        overrides: &std::collections::BTreeMap<ObjectId, ObjectOverride>,
    ) -> Walk {
        let mut walk = Walk::default();
        let Some(f) = n.fixture() else {
            return walk;
        };
        let mut w = Walker {
            project: self,
            fixture: &n.id,
            lamps,
            rules: &f.rules,
            overrides,
            per_lamp: f.channels_per_lamp.max(1),
            cursor: Cursor::default(),
            stopped: false,
            past_end: false,
            fresh_set: false,
            output_from: PatchTarget::Fixture,
            port_from: PatchTarget::Fixture,
            walk: &mut walk,
        };
        w.check_rules(f);
        // The fixture is the root: its Start puts the cursor down, its
        // Children rule lays out its top-level components.
        let fixture_rules = ComponentRules {
            start: f.start.clone(),
            children: f.children.clone(),
        };
        match fixture_rules.start.render() {
            Some(ops) => w.step(&ops, &PatchTarget::Fixture, &PatchTarget::Fixture),
            None => w.walk.problems.push(PatchProblem {
                fixture: n.id.clone(),
                kind: ProblemKind::NoStart,
            }),
        }
        let roots = self.placed().tree(&n.id);
        for (k, root) in roots.iter().enumerate() {
            let from = fixture_rules.children.render(k);
            w.enter(root, &from, &PatchTarget::Fixture, false, Direction::AsDrawn);
        }
        walk
    }
}

struct Walker<'a> {
    project: &'a Project,
    fixture: &'a NodeId,
    lamps: &'a FixtureLamps,
    rules: &'a std::collections::BTreeMap<ComponentId, ComponentRules>,
    overrides: &'a std::collections::BTreeMap<ObjectId, ObjectOverride>,
    per_lamp: u32,
    cursor: Cursor,
    /// Past the last port: nothing after lands, until a port is Set.
    stopped: bool,
    /// A step put the cursor past its output's last port.
    past_end: bool,
    /// A port or channel was Set and no leaf has been laid since.
    fresh_set: bool,
    /// What last set the cursor's output, and its port: whom a gone one
    /// is reported against.
    output_from: PatchTarget,
    port_from: PatchTarget,
    walk: &'a mut Walk,
}

impl Walker<'_> {
    /// Rules naming a component that isn't in this fixture, and overrides
    /// naming an object that isn't, are reported (and do nothing): never
    /// pointed somewhere else on their own.
    fn check_rules(&mut self, f: &crate::domain::fixture::Fixture) {
        for c in self.rules.keys() {
            if f.component(c).is_none() {
                self.problem(ProblemKind::GoneComponent(c.clone()));
            }
        }
        for o in self.overrides.keys() {
            if self.project.placed().node(o).map(|(fx, _)| fx) != Some(self.fixture) {
                self.problem(ProblemKind::GoneObject(o.clone()));
            }
        }
    }

    /// Report a problem, once.
    fn problem(&mut self, kind: ProblemKind) {
        let p = PatchProblem {
            fixture: self.fixture.clone(),
            kind,
        };
        if !self.walk.problems.contains(&p) {
            self.walk.problems.push(p);
        }
    }

    /// Enter a node: take its steps, then walk its children (a container)
    /// or lay its span (a leaf).
    fn enter(
        &mut self,
        node: &ObjectNode,
        from_parent: &CursorOps,
        parent: &PatchTarget,
        off_above: bool,
        flip_above: Direction,
    ) {
        let own = self.overrides.get(&node.id);
        // A component's own node (in any copy) carries its component's
        // rules; a generated node (a ring) has none of its own.
        let rules = (node.kind == ObjectNodeKind::Component)
            .then(|| self.rules.get(&node.id.component))
            .flatten();
        let start = rules.and_then(|r| r.start.render());
        let me = PatchTarget::Component(node.id.component.clone());

        let (output, out_src) = choose(
            own.and_then(|o| o.output.clone()),
            start.as_ref().map(|s| s.output.clone()),
            &me,
            from_parent.output.clone(),
            parent,
            OutputOp::Keep,
        );
        let (port, port_src) = choose(
            own.and_then(|o| o.port.clone()),
            start.as_ref().map(|s| s.port.clone()),
            &me,
            from_parent.port.clone(),
            parent,
            PortOp::Keep,
        );
        let (channel, ch_src) = choose(
            own.and_then(|o| o.channel),
            start.as_ref().map(|s| s.channel),
            &me,
            from_parent.channel,
            parent,
            ChannelOp::Keep,
        );
        let ops = CursorOps {
            output,
            port,
            channel,
        };
        let sources = [out_src, port_src, ch_src];
        let off = off_above || own.is_some_and(|o| o.off);
        // A direction of its own is what it says; none takes its
        // container's.
        let direction = own.and_then(|o| o.direction).unwrap_or(flip_above);
        let offset = own.and_then(|o| o.offset).unwrap_or(0);
        if !off {
            let whom = |s: &StepSource| match s {
                StepSource::Override => PatchTarget::Object(node.id.clone()),
                StepSource::Start(t) | StepSource::Children(t) => t.clone(),
                StepSource::Follows => PatchTarget::Fixture,
            };
            self.step(&ops, &whom(&sources[0]), &whom(&sources[1]));
        }
        let at = self.cursor.clone();
        let index = self.walk.walked.len();
        self.walk.walked.push(Walked {
            fixture: self.fixture.clone(),
            object: node.id.clone(),
            ops: ops.clone(),
            sources: sources.clone(),
            at,
            landed: None,
            wrapped: false,
            off,
            direction,
            offset,
        });
        if node.is_leaf() {
            if !off {
                let explicit = self.fresh_set;
                self.lay(index, node, explicit, direction, offset);
                self.fresh_set = false;
            }
            return;
        }
        // Its children: a component's Children rule, or end to end.
        let children = rules.map(|r| r.children.clone()).unwrap_or_default();
        let container = match (node.kind, node.id.is_component_node()) {
            (ObjectNodeKind::Component, _) => me,
            _ => PatchTarget::Object(node.id.clone()),
        };
        for (k, child) in node.children.iter().enumerate() {
            let rendered = children.render(k);
            self.enter(child, &rendered, &container, off, direction);
        }
    }

    /// Take one step per field. `output_from` and `port_from` say whose
    /// steps they are.
    fn step(&mut self, ops: &CursorOps, output_from: &PatchTarget, port_from: &PatchTarget) {
        if let OutputOp::Set(o) = &ops.output {
            self.cursor.output = Some(o.clone());
            self.output_from = output_from.clone();
        }
        match &ops.port {
            PortOp::Keep => {}
            PortOp::Set { key, plus } => {
                self.move_port(key, *plus);
                self.port_from = port_from.clone();
                self.fresh_set = true;
                // A new start: the walk lands things again.
                self.stopped = false;
            }
            PortOp::Add(n) => {
                if let Some(at) = self.cursor.port.clone() {
                    self.move_port(&at, *n);
                }
                self.port_from = port_from.clone();
            }
        }
        match ops.channel {
            ChannelOp::Keep => {}
            ChannelOp::Set(a) => {
                self.cursor.channel = a.channels(self.per_lamp);
                self.fresh_set = true;
            }
            ChannelOp::Add(a) => {
                self.cursor.channel = self.cursor.channel.saturating_add(a.channels(self.per_lamp))
            }
        }
    }

    /// Move the cursor to the port `n` keys after `key`, in its output's
    /// order. Past the last one, the cursor is past the end (a leaf there
    /// is an error); a key the output doesn't have stays named, so the leaf
    /// can say which port is gone.
    fn move_port(&mut self, key: &str, n: u32) {
        self.past_end = false;
        let out = self.cursor.output.as_ref().and_then(|o| self.project.output(o));
        match out.and_then(|out| out.ports.iter().position(|p| p.key == key).map(|i| (out, i))) {
            Some((out, i)) => match out.ports.get(i + n as usize) {
                Some(p) => self.cursor.port = Some(p.key.clone()),
                None => self.past_end = true,
            },
            None => self.cursor.port = Some(key.to_string()),
        }
    }

    /// Lay one leaf's span at the cursor.
    fn lay(
        &mut self,
        index: usize,
        node: &ObjectNode,
        explicit: bool,
        direction: Direction,
        offset: u32,
    ) {
        let object = &node.id;
        let Some((start, count)) = self.lamps.object(object) else {
            return;
        };
        if self.stopped || count == 0 {
            return;
        }
        if self.past_end {
            self.problem(ProblemKind::PastLastPort {
                object: object.clone(),
            });
            self.stopped = true;
            return;
        }
        let (Some(output), Some(port)) = (self.cursor.output.clone(), self.cursor.port.clone())
        else {
            return; // The fixture's NoStart says why.
        };
        let project = self.project;
        let Some(out) = project.output(&output) else {
            self.problem(ProblemKind::GoneOutput {
                on: self.output_from.clone(),
                output,
            });
            return;
        };
        let Some(mut i) = out.ports.iter().position(|p| p.key == port) else {
            self.problem(ProblemKind::GonePort {
                on: self.port_from.clone(),
                output,
                port,
            });
            return;
        };
        let per_lamp = self.per_lamp;
        let need = count.saturating_mul(per_lamp);
        let last = out.ports.len() - 1;
        let room = |i: usize, channel: u32| -> u32 {
            match out.ports[i].pixels {
                Some(px) => px.saturating_mul(per_lamp).saturating_sub(channel),
                None if i == last => u32::MAX,
                None => 0,
            }
        };
        let mut channel = self.cursor.channel;
        let mut wrapped = false;
        let mut limit = None;
        if room(i, channel) < need {
            if explicit {
                self.problem(ProblemKind::DoesntFit {
                    object: object.clone(),
                    port: out.ports[i].key.clone(),
                });
                limit = Some(room(i, channel) / self.per_lamp);
            } else {
                // Universe wrapping: the next port, from its start.
                loop {
                    if i == last {
                        self.problem(ProblemKind::PastLastPort {
                            object: object.clone(),
                        });
                        self.stopped = true;
                        return;
                    }
                    i += 1;
                    channel = 0;
                    wrapped = true;
                    if room(i, 0) >= need {
                        break;
                    }
                }
            }
        }
        let port_key = out.ports[i].key.clone();
        self.cursor.port = Some(port_key.clone());
        let landed = Cursor {
            output: Some(output.clone()),
            port: Some(port_key.clone()),
            channel,
        };
        self.cursor.channel = channel.saturating_add(need);
        let w = &mut self.walk.walked[index];
        w.landed = Some(landed);
        w.wrapped = wrapped;
        if channel % self.per_lamp != 0 {
            self.problem(ProblemKind::MidPixel {
                object: object.clone(),
            });
            return;
        }
        let port_start = out.port_start(&port_key).unwrap_or(0);
        self.walk.runs.push(Run {
            fixture: self.fixture.clone(),
            object: object.clone(),
            start,
            count,
            output,
            wire: port_start.saturating_add(channel / self.per_lamp),
            reversed: direction.is_reversed(),
            rotation: offset % count.max(1),
            limit,
        });
    }
}

/// The step for one field: the override's, else the node's own Start's,
/// else what its parent renders for it, else `keep`; and where it came
/// from.
fn choose<T: PartialEq>(
    own: Option<T>,
    start: Option<T>,
    me: &PatchTarget,
    from_parent: T,
    parent: &PatchTarget,
    keep: T,
) -> (T, StepSource) {
    if let Some(v) = own {
        return (v, StepSource::Override);
    }
    // A Start that keeps this field says nothing about it.
    if let Some(v) = start.filter(|v| *v != keep) {
        return (v, StepSource::Start(me.clone()));
    }
    if from_parent != keep {
        return (from_parent, StepSource::Children(parent.clone()));
    }
    (keep, StepSource::Follows)
}

