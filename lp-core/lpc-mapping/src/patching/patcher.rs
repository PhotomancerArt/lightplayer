//! The patcher: how one fixture's segments get their addresses (names.html
//! "Patcher"; the spike's walker, D5).
//!
//! It walks the rendered tree ([`crate::part::Tree`]) depth first, holding
//! a **cursor**: an output, a port on it, a channel on that port.
//!
//! 1. **One fixture.** [`patch_mapping`] takes one mapping, its tree, and
//!    the ports of the outputs it may name. It never sees another fixture:
//!    nothing flows in from one.
//! 2. **The fixture's own start** is taken first, then its top-level parts
//!    are entered as the children of its flow.
//! 3. **Entering a node, each field takes exactly one step** (F8): the
//!    node's override for that field if it has one; else the node's own
//!    start, if that field isn't Keep (a part's start, for the part's node
//!    in any copy; generated segments have none); else what its parent's
//!    flow renders for child k; else Keep. One step per field, never two in
//!    a row: an override replaces, it doesn't add. Which of the four it was
//!    is recorded ([`StepSource`]).
//! 4. **Taking a step:** output Set moves to that output; port
//!    `Set { key, plus }` moves to the port `plus` keys after `key` in that
//!    output's key order; port `Add(n)` to the port n keys after the
//!    cursor's; channel Set / Add keep the unit they were written in, and
//!    lamps are counted in the channels per lamp of the segment the patcher
//!    reaches next. A port past the output's last key leaves the cursor
//!    **past the end**. A port key the output doesn't have stays named, so
//!    the segment can say which port is gone.
//! 5. **A segment** (a leaf) gets one span at the cursor and moves the
//!    cursor past its lamps (lamps × its channels per lamp).
//!    - **Doesn't fit** what is left of its port: it starts on the next port
//!      at channel 0 (universe wrapping), and says it wrapped; **unless it
//!      is the first segment laid since a port or channel was Set** (by any
//!      source), in which case it stays where it was set, is reported
//!      ([`Problem::DoesntFit`]), and lands as far as the port reaches
//!      (F9). A Set is a promise about where something starts.
//!    - **Past the last port**: the segment, and everything after it that
//!      only follows on, lands nowhere, reported once, until a step Sets a
//!      port again, which is a new start (F10). Outputs have no order: it
//!      never moves on to another output.
//!    - **Off** (an override, or under an off part): no span, and the cursor
//!      doesn't move (F11). What follows takes its place.
//!    - **A start part-way into a lamp**: the segment gets its address, and
//!      [`Problem::OffLampBoundary`] is reported; the engine refuses it.
//!    - **No output or port in the cursor**: it lands nowhere, and the
//!      fixture reports [`Problem::NoStart`] once.
//!    - Its direction: its own override's, else the nearest part above that
//!      has one, else Forward (F14). Its offset: its own, else 0.
//! 6. **Leaving a node never moves the cursor.** Only a step does.
//! 7. **Overrides are by id, and never retargeted** (D12): one whose node
//!    isn't in the tree is reported ([`Problem::GoneNode`]) and kept.
//! 8. **Problems** each name the node or port they are about, and each is
//!    reported once. Two segments sharing channels on a port are an
//!    [`Problem::Overlap`]; an order that doesn't fit its colors an
//!    [`Problem::OrderMismatch`].
//!
//! Each port of each output is then laid out as a row of spans: segments in
//! channel order, and the gaps before and between them, and after the last
//! one up to the port's capacity (a port that takes the rest has no
//! trailing gap).

use alloc::vec::Vec;

use crate::part::{Mapping, Part, PartId, Tree};
use crate::patching::address::Address;
use crate::patching::amount::{Amount, Unit};
use crate::patching::cursor::Cursor;
use crate::patching::direction::Direction;
use crate::patching::output_name::OutputName;
use crate::patching::output_ports::{OutputPorts, Port};
use crate::patching::patch_target::PatchTarget;
use crate::patching::patching::Patching;
use crate::patching::port_key::PortKey;
use crate::patching::problem::Problem;
use crate::patching::segment_patch::SegmentPatch;
use crate::patching::span::{PortSpans, Span};
use crate::patching::start::{ChannelOp, OutputOp, PortOp, Start};
use crate::patching::step_source::StepSource;
use crate::patching::walked::Walked;

/// Patch one fixture: give each segment of its tree an address on the
/// outputs' ports (rules in the module doc).
#[must_use]
pub fn patch_mapping(mapping: &Mapping, tree: &Tree, outputs: &[OutputPorts]) -> Patching {
    patch(mapping, tree, outputs, true)
}

/// [`patch_mapping`] without the record of the walk: the same segments,
/// spans and problems, and [`Patching::walked`] left empty.
///
/// For a reader that needs only where each segment landed and what is
/// wrong: the engine on a device. The walk holds one entry per node of the
/// tree (its id, its steps and where they came from, the cursor after
/// them), which at a few hundred bytes each was most of a patching's
/// memory (mapping rebuild M2 P6).
#[must_use]
pub fn patch_mapping_placed(mapping: &Mapping, tree: &Tree, outputs: &[OutputPorts]) -> Patching {
    patch(mapping, tree, outputs, false)
}

fn patch(mapping: &Mapping, tree: &Tree, outputs: &[OutputPorts], record_walk: bool) -> Patching {
    // By id, sorted for a binary search; a later part of one id wins, as
    // a map's insert would have it (a rendered tree has none: the render
    // refuses two parts of one id).
    let mut parts = mapping.all_parts();
    parts.reverse();
    parts.sort_by(|a, b| a.id.cmp(&b.id));
    parts.dedup_by(|later, kept| later.id == kept.id);
    let outputs: Vec<(&OutputName, Vec<Port>)> = outputs
        .iter()
        .map(|o| {
            let mut ports = o.ports.clone();
            ports.sort_by_key(|p| p.key);
            (&o.output, ports)
        })
        .collect();
    let mut patcher = Patcher {
        mapping,
        tree,
        parts,
        outputs,
        cursor: Cursor::default(),
        stopped: false,
        past_end: false,
        fresh_set: false,
        output_from: PatchTarget::Fixture,
        port_from: PatchTarget::Fixture,
        record_walk,
        out: Patching::default(),
    };
    // 7. Overrides whose node is gone, and 8. orders that don't fit.
    for id in mapping.overrides.keys() {
        if tree.find(id).is_none() {
            patcher.problem(Problem::GoneNode(id.clone()));
        }
    }
    for segment in tree.segments() {
        if segment.lamp.checked_order().is_err() {
            patcher.problem(Problem::OrderMismatch {
                segment: segment.id.clone(),
            });
        }
    }
    // 2. The fixture's start, then its parts as its flow's children.
    patcher.step(&mapping.start, &PatchTarget::Fixture, &PatchTarget::Fixture);
    let roots: Vec<usize> = tree.roots().collect();
    for (k, root) in roots.into_iter().enumerate() {
        let from = mapping.flow.render(k);
        patcher.enter(
            root,
            &from,
            &PatchTarget::Fixture,
            false,
            Direction::Forward,
        );
    }
    patcher.lay_out_ports();
    patcher.out
}

struct Patcher<'a> {
    mapping: &'a Mapping,
    tree: &'a Tree,
    /// Every part, sorted by id (one per id).
    parts: Vec<&'a Part>,
    /// Each output's ports, in key order.
    outputs: Vec<(&'a OutputName, Vec<Port>)>,
    cursor: Cursor,
    /// Past the last port: nothing lands until a port is Set.
    stopped: bool,
    /// A step put the cursor past its output's last port.
    past_end: bool,
    /// A port or channel was Set and no segment has been laid since.
    fresh_set: bool,
    /// Whose step last set the output, and the port: whom a gone one is
    /// reported against.
    output_from: PatchTarget,
    port_from: PatchTarget,
    /// Whether [`Patching::walked`] is kept.
    record_walk: bool,
    out: Patching,
}

impl Patcher<'_> {
    /// 3. Enter a node: take its steps, then walk its children (a part) or
    /// lay its span (a segment).
    fn enter(
        &mut self,
        index: usize,
        from_parent: &Start,
        parent: &PatchTarget,
        off_above: bool,
        direction_above: Direction,
    ) {
        let tree = self.tree;
        let node = tree.node(index);
        let own = self.mapping.overrides.get(&node.id);
        // A part's own node (in any copy) carries its part's start and
        // flow; a generated segment has none.
        let part = if node.id.is_part_node() {
            let id: &PartId = &node.id.part;
            self.parts
                .binary_search_by(|p| p.id.cmp(id))
                .ok()
                .map(|at| self.parts[at])
        } else {
            None
        };
        let me = part.map(|p| PatchTarget::Part(p.id.clone()));
        let start = part.map(|p| &p.start);

        let (output, output_source) = choose(
            own.and_then(|o| o.output.clone()),
            start.map(|s| s.output.clone()),
            me.as_ref(),
            from_parent.output.clone(),
            parent,
        );
        let (port, port_source) = choose(
            own.and_then(|o| o.port.clone()),
            start.map(|s| s.port.clone()),
            me.as_ref(),
            from_parent.port.clone(),
            parent,
        );
        let (channel, channel_source) = choose(
            own.and_then(|o| o.channel),
            start.map(|s| s.channel),
            me.as_ref(),
            from_parent.channel,
            parent,
        );
        let steps = Start {
            output,
            port,
            channel,
        };
        let off = off_above || own.is_some_and(|o| o.off);
        let direction = own.and_then(|o| o.direction).unwrap_or(direction_above);
        let offset = own.and_then(|o| o.offset).unwrap_or(0);
        if !off {
            let whom = |source: &StepSource| match source {
                StepSource::Override => PatchTarget::Node(node.id.clone()),
                StepSource::Start(t) | StepSource::Flow(t) => t.clone(),
                StepSource::Follows => PatchTarget::Fixture,
            };
            self.step(&steps, &whom(&output_source), &whom(&port_source));
        }
        if self.record_walk {
            self.out.walked.push(Walked {
                id: node.id.clone(),
                steps,
                sources: [output_source, port_source, channel_source],
                at: self.cursor.clone(),
                off,
                direction,
                offset,
            });
        }

        if let Some(segment) = node.segment {
            if !off {
                self.lay(segment, direction, offset);
                self.fresh_set = false;
            }
            return;
        }
        let flow = part.map(|p| p.flow.clone()).unwrap_or_default();
        let container = me.unwrap_or_else(|| PatchTarget::Node(node.id.clone()));
        let children: Vec<usize> = tree.children(index).collect();
        for (k, child) in children.into_iter().enumerate() {
            let rendered = flow.render(k);
            self.enter(child, &rendered, &container, off, direction);
        }
    }

    /// 4. Take one step per field. `output_from` and `port_from` say whose
    /// steps they are.
    fn step(&mut self, steps: &Start, output_from: &PatchTarget, port_from: &PatchTarget) {
        if let OutputOp::Set(output) = &steps.output {
            self.cursor.output = Some(output.clone());
            self.output_from = output_from.clone();
        }
        match &steps.port {
            PortOp::Keep => {}
            PortOp::Set { key, plus } => {
                self.move_port(*key, *plus);
                self.port_from = port_from.clone();
                self.fresh_set = true;
                // A new start: things land again.
                self.stopped = false;
            }
            PortOp::Add(n) => {
                if let Some(at) = self.cursor.port {
                    self.move_port(at, *n);
                }
                self.port_from = port_from.clone();
            }
        }
        match steps.channel {
            ChannelOp::Keep => {}
            ChannelOp::Set(amount) => {
                self.cursor.channel = 0;
                self.cursor.lamps = 0;
                self.add_channel(amount);
                self.fresh_set = true;
            }
            ChannelOp::Add(amount) => self.add_channel(amount),
        }
    }

    /// An amount onto the cursor's channel, in the unit it was written in.
    fn add_channel(&mut self, amount: Amount) {
        match amount.unit {
            Unit::Channels => self.cursor.channel = self.cursor.channel.saturating_add(amount.n),
            Unit::Lamps => self.cursor.lamps = self.cursor.lamps.saturating_add(amount.n),
        }
    }

    /// Move the cursor to the port `n` keys after `key`, in its output's
    /// key order. Past the last one, the cursor is past the end; a key the
    /// output doesn't have (or an output the patcher wasn't given) stays
    /// named, so the segment can say which is gone.
    fn move_port(&mut self, key: PortKey, n: u32) {
        self.past_end = false;
        let ports = self.ports_of_cursor_output();
        match ports.and_then(|ports| ports.iter().position(|p| p.key == key).map(|i| (ports, i))) {
            Some((ports, i)) => match ports.get(i + n as usize) {
                Some(port) => self.cursor.port = Some(port.key),
                None => self.past_end = true,
            },
            None => self.cursor.port = Some(key),
        }
    }

    fn ports_of_cursor_output(&self) -> Option<&[Port]> {
        let output = self.cursor.output.as_ref()?;
        self.outputs
            .iter()
            .find(|(name, _)| *name == output)
            .map(|(_, ports)| ports.as_slice())
    }

    /// 5. Lay one segment's span at the cursor.
    fn lay(&mut self, segment: usize, direction: Direction, offset: u32) {
        let segment = &self.tree.segments()[segment];
        let id = &segment.id;
        let count = segment.count();
        if self.stopped || count == 0 {
            return;
        }
        if self.past_end {
            self.problem(Problem::PastLastPort {
                segment: id.clone(),
            });
            self.stopped = true;
            return;
        }
        let (Some(output), Some(port)) = (self.cursor.output.clone(), self.cursor.port) else {
            self.problem(Problem::NoStart);
            return;
        };
        let Some(ports) = self.ports_of_cursor_output() else {
            let set_by = self.output_from.clone();
            self.problem(Problem::GoneOutput { output, set_by });
            return;
        };
        let Some(mut i) = ports.iter().position(|p| p.key == port) else {
            let set_by = self.port_from.clone();
            self.problem(Problem::GonePort {
                output,
                port,
                set_by,
            });
            return;
        };
        let ports = ports.to_vec();
        let per_lamp = segment.lamp.channels_per_lamp();
        let need = count.saturating_mul(per_lamp);
        let last = ports.len() - 1;
        let room = |i: usize, channel: u32| -> u32 {
            match ports[i].channels {
                Some(capacity) => capacity.saturating_sub(channel),
                None if i == last => u32::MAX,
                None => 0,
            }
        };
        let mut channel = self
            .cursor
            .channel
            .saturating_add(self.cursor.lamps.saturating_mul(per_lamp));
        let mut wrapped = false;
        let mut limit = None;
        if room(i, channel) < need {
            if self.fresh_set {
                self.problem(Problem::DoesntFit {
                    segment: id.clone(),
                    port: ports[i].key,
                });
                limit = Some(room(i, channel) / per_lamp);
            } else {
                // Universe wrapping: the next port, from its start.
                loop {
                    if i == last {
                        self.problem(Problem::PastLastPort {
                            segment: id.clone(),
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
        let port = ports[i].key;
        self.cursor.port = Some(port);
        self.cursor.channel = channel.saturating_add(need);
        self.cursor.lamps = 0;
        if channel % per_lamp != 0 {
            self.problem(Problem::OffLampBoundary {
                segment: id.clone(),
            });
        }
        self.out.segments.push(SegmentPatch {
            id: id.clone(),
            address: Address {
                output,
                port,
                channel,
            },
            lamps: count,
            channels_per_lamp: per_lamp,
            direction,
            offset: offset % count,
            limit,
            wrapped,
        });
    }

    /// Each port of each output as a row of spans, and the overlaps on it.
    fn lay_out_ports(&mut self) {
        let mut rows = Vec::new();
        let mut overlaps = Vec::new();
        for (output, ports) in &self.outputs {
            for (i, port) in ports.iter().enumerate() {
                let mut on: Vec<&SegmentPatch> = self
                    .out
                    .segments
                    .iter()
                    .filter(|s| s.address.output == **output && s.address.port == port.key)
                    .collect();
                on.sort_by_key(|s| s.address.channel);
                let mut spans = Vec::new();
                let mut next = 0u32;
                for (j, segment) in on.iter().enumerate() {
                    let at = segment.address.channel;
                    let end = at.saturating_add(segment.channels());
                    for later in &on[j + 1..] {
                        if later.address.channel < end {
                            overlaps.push(Problem::Overlap {
                                a: segment.id.clone(),
                                b: later.id.clone(),
                                output: (*output).clone(),
                                port: port.key,
                            });
                        }
                    }
                    if at > next {
                        spans.push(Span::Gap {
                            at: next,
                            channels: at - next,
                        });
                    }
                    spans.push(Span::Segment {
                        id: segment.id.clone(),
                        at,
                        channels: segment.channels(),
                    });
                    next = next.max(end);
                }
                let capacity = match port.channels {
                    Some(capacity) => Some(capacity),
                    None if i + 1 == ports.len() => None,
                    None => Some(0),
                };
                if let Some(capacity) = capacity.filter(|c| *c > next) {
                    spans.push(Span::Gap {
                        at: next,
                        channels: capacity - next,
                    });
                }
                rows.push(PortSpans {
                    output: (*output).clone(),
                    port: port.key,
                    spans,
                });
            }
        }
        self.out.ports = rows;
        for overlap in overlaps {
            self.problem(overlap);
        }
    }

    /// 8. Report a problem, once.
    fn problem(&mut self, problem: Problem) {
        if !self.out.problems.contains(&problem) {
            self.out.problems.push(problem);
        }
    }
}

/// 3. The step for one field: the override's, else the node's own start's
/// (when it isn't Keep), else what its parent's flow renders for it (when
/// that isn't Keep), else Keep; and where it came from.
fn choose<T: PartialEq + Default>(
    own: Option<T>,
    start: Option<T>,
    me: Option<&PatchTarget>,
    from_parent: T,
    parent: &PatchTarget,
) -> (T, StepSource) {
    if let Some(step) = own {
        return (step, StepSource::Override);
    }
    let keep = T::default();
    if let (Some(step), Some(me)) = (start, me)
        && step != keep
    {
        return (step, StepSource::Start(me.clone()));
    }
    if from_parent != keep {
        return (from_parent, StepSource::Flow(parent.clone()));
    }
    (keep, StepSource::Follows)
}
