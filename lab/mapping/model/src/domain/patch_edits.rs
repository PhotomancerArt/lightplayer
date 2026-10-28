//! Patching by hand: the edits a person makes to a fixture's starts, flows
//! and overrides (`lpc_mapping::patching`).
//!
//! Every edit sets or clears one thing on one fixture, part or node. The
//! effective patch is derived afterwards, as always (D7), by the patcher.
//!
//! **An edit moves what follows it.** The patcher's cursor carries on from
//! wherever a segment starts (theory "Patching": the cursor continues, no
//! snapping), so putting ring 3 on D9 takes rings 4 and 5 along with it
//! (Yona, D1b). Nothing is pinned behind a person's back; what moved is
//! what the walk says follows.
//!
//! Turning is **direction-aware**, as lp2014's keys were: one step moves
//! where the data enters the segment back one lamp, whichever way its run
//! lies, so the same key turns the same way on the piece after a reverse.
//!
//! Ops name an output by its name, as the file does: an output with no
//! name can't be patched to here (naming it is a verb of its own).
//!
//! Every change goes through `lpc_mapping::edit` (`set_start`,
//! `set_override_field`, `clear_override_field`): nothing here pokes a
//! mapping's fields. The editor's patching verbs (`ux/graph_edit.rs`) are
//! made of these.

use lpc_mapping::edit::{self, OverrideField, OverrideFieldKind};
use lpc_mapping::part::TreeId;
use lpc_mapping::patching::{
    Amount, ChannelOp, Direction, Flow, OutputName, OutputOp, Override, PatchTarget, PortKey,
    PortOp, Start,
};

use crate::domain::node::NodeId;
use crate::domain::part_ref::TreeRef;
use crate::domain::project::Project;

/// What an edit did besides what it was asked. Kept for the hint the keys
/// show; the patcher never pins anything, so it is always zero now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PatchEdit {
    pub pinned: usize,
}

/// How a segment is on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnWire {
    /// Its own override says where it starts.
    Placed {
        output: NodeId,
        port: String,
        pixel: u32,
        reversed: bool,
        rotation: u32,
    },
    /// It lands, where the walk carried it.
    Flowing,
    /// No pixel shows it.
    Nowhere,
}

/// Why a shift can't go further.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortEdge {
    /// Before the port's first pixel.
    Start,
    /// Past the port's last pixel.
    End,
}

/// Does an override move its node's start (any of output, port, channel)?
pub fn moves_start(o: &Override) -> bool {
    o.output.is_some() || o.port.is_some() || o.channel.is_some()
}

impl Project {
    /// How a segment is on the wire right now.
    pub fn on_wire(&self, segment: &TreeRef) -> OnWire {
        let patch = self.effective_patch();
        let own = self.override_of(segment);
        if own.is_some_and(|o| moves_start(o) && !o.off)
            && let Some(w) = patch.walked(segment)
            && let Some(at) = &w.landed
            && let (Some(output), Some(port)) = (&at.output, &at.port)
        {
            let per = self.channels_per_lamp_of(segment);
            return OnWire::Placed {
                output: output.clone(),
                port: port.clone(),
                pixel: at.channel / per,
                reversed: w.direction.is_reverse(),
                rotation: w.offset,
            };
        }
        if patch.lands(segment) {
            OnWire::Flowing
        } else {
            OnWire::Nowhere
        }
    }

    /// A node's override, if it has one.
    pub fn override_of(&self, node: &TreeRef) -> Option<&Override> {
        self.fixture(&node.fixture)?.mapping.overrides.get(&node.id)
    }

    /// How many channels a lamp of this node takes: its fixture's.
    pub fn channels_per_lamp_of(&self, node: &TreeRef) -> u32 {
        self.fixture(&node.fixture)
            .map_or(3, |f| f.channels_per_lamp())
    }

    /// The segments a patch target reaches.
    pub fn segments_of_place(&self, fixture: &NodeId, target: &PatchTarget) -> Vec<TreeRef> {
        match target {
            PatchTarget::Fixture => self
                .segments_in(fixture)
                .iter()
                .map(|o| o.id.clone())
                .collect(),
            PatchTarget::Part(part) => self
                .placed()
                .of_part(&crate::domain::part_ref::PartRef::new(fixture, part))
                .iter()
                .map(|(_, o)| o.id.clone())
                .collect(),
            PatchTarget::Node(id) => {
                let node = TreeRef::new(fixture, id.clone());
                match self.placed().node(&node) {
                    Some((_, n)) => n.leaves().iter().map(|l| l.id.clone()).collect(),
                    None => vec![node],
                }
            }
        }
    }

    // ---- starts and flows ----------------------------------------------------

    /// Set where a fixture, a part (every copy of it) or one node starts.
    pub fn set_start(&mut self, fixture: &NodeId, target: &PatchTarget, start: Start) -> bool {
        let Some(f) = self.fixture_mut(fixture) else {
            return false;
        };
        let done = edit::set_start(&mut f.mapping, target.clone(), start).is_ok();
        f.tidy_patching();
        done
    }

    /// Set how a fixture's or a part's children follow each other.
    pub fn set_flow(&mut self, fixture: &NodeId, target: &PatchTarget, flow: Flow) -> bool {
        let Some(f) = self.fixture_mut(fixture) else {
            return false;
        };
        edit::set_flow(&mut f.mapping, target.clone(), flow).is_ok()
    }

    /// The ops that start something at `pixel` of `output`'s port `port`:
    /// `None` when the output has no name to say it by, or the port key
    /// isn't a number.
    pub fn start_ops_at(&self, output: &NodeId, port: &str, pixel: u32) -> Option<Start> {
        let name = self.output(output)?.label.clone()?;
        Some(Start {
            output: OutputOp::Set(OutputName::new(&name)),
            port: PortOp::set(port.parse().ok()?),
            channel: ChannelOp::Set(Amount::lamps(pixel)),
        })
    }

    // ---- overrides -------------------------------------------------------------

    /// Edit one node's override (made empty if it has none); one that ends
    /// up overriding nothing goes. The closure edits a copy; each field it
    /// changed is then set or cleared through `lpc_mapping::edit`.
    pub fn edit_override(
        &mut self,
        node: &TreeRef,
        change: impl FnOnce(&mut Override),
    ) -> Option<PatchEdit> {
        let before = self.override_of(node).cloned().unwrap_or_default();
        let mut after = before.clone();
        change(&mut after);
        let m = &mut self.fixture_mut(&node.fixture)?.mapping;
        let id = &node.id;
        let mut put = |field: Option<OverrideField>, kind: OverrideFieldKind| match field {
            Some(field) => edit::set_override_field(m, id, field).is_ok(),
            None => edit::clear_override_field(m, id, kind).is_ok(),
        };
        let mut ok = true;
        if after.output != before.output {
            ok &= put(
                after.output.map(OverrideField::Output),
                OverrideFieldKind::Output,
            );
        }
        if after.port != before.port {
            ok &= put(after.port.map(OverrideField::Port), OverrideFieldKind::Port);
        }
        if after.channel != before.channel {
            ok &= put(
                after.channel.map(OverrideField::Channel),
                OverrideFieldKind::Channel,
            );
        }
        if after.off != before.off {
            ok &= put(Some(OverrideField::Off(after.off)), OverrideFieldKind::Off);
        }
        if after.direction != before.direction {
            ok &= put(
                after.direction.map(OverrideField::Direction),
                OverrideFieldKind::Direction,
            );
        }
        if after.offset != before.offset {
            ok &= put(
                after.offset.map(OverrideField::Offset),
                OverrideFieldKind::Offset,
            );
        }
        ok.then_some(PatchEdit::default())
    }

    /// Set one field of a node's override.
    pub fn set_override(&mut self, node: &TreeRef, field: OverrideField) -> Option<PatchEdit> {
        let m = &mut self.fixture_mut(&node.fixture)?.mapping;
        edit::set_override_field(m, &node.id, field).ok()?;
        Some(PatchEdit::default())
    }

    /// Clear one field of a node's override: it takes what its flow gives.
    pub fn clear_override(&mut self, node: &TreeRef, kind: OverrideFieldKind) {
        if let Some(f) = self.fixture_mut(&node.fixture) {
            // Clearing never refuses.
            let _ = edit::clear_override_field(&mut f.mapping, &node.id, kind);
        }
    }

    /// Set one node's start (its override's output, port and lamp; Keep
    /// clears that field).
    pub fn set_node_start(&mut self, node: &TreeRef, start: Start) -> Option<PatchEdit> {
        let m = &mut self.fixture_mut(&node.fixture)?.mapping;
        edit::set_start(m, PatchTarget::Node(node.id.clone()), start).ok()?;
        Some(PatchEdit::default())
    }

    /// Start a segment at `pixel` of `output`'s port `port`, on the wire
    /// again if it was off. It keeps its direction and offset; what follows
    /// it moves along. `None` when the output has no name.
    pub fn patch_segment(
        &mut self,
        segment: &TreeRef,
        output: &NodeId,
        port: &str,
        pixel: u32,
    ) -> Option<PatchEdit> {
        let start = self.start_ops_at(output, port, pixel)?;
        self.set_node_start(segment, start)?;
        self.set_override(segment, OverrideField::Off(false))
    }

    /// Take a segment's own start away, so it follows its rule again.
    /// False when it had none.
    pub fn unpatch_segment(&mut self, segment: &TreeRef) -> bool {
        if !self.override_of(segment).is_some_and(moves_start) {
            return false;
        }
        self.set_node_start(segment, Start::KEEP);
        true
    }

    /// Give a following segment a start of its own where it lands now, so
    /// a wiring edit can move it. Nothing to do for one placed already;
    /// `None` for one that lands nowhere (there is no "where it is" to
    /// keep).
    pub fn pin_segment(&mut self, segment: &TreeRef) -> Option<PatchEdit> {
        match self.on_wire(segment) {
            OnWire::Placed { .. } => Some(PatchEdit::default()),
            OnWire::Nowhere => None,
            OnWire::Flowing => {
                let at = self.effective_patch().walked(segment)?.landed.clone()?;
                let (output, port) = (at.output?, at.port?);
                let pixel = at.channel / self.channels_per_lamp_of(segment);
                self.patch_segment(segment, &output, &port, pixel)
            }
        }
    }

    /// Lay a segment's run the other way inside its span. It stays where
    /// it is: a direction of its own.
    pub fn reverse_placement(&mut self, segment: &TreeRef) -> Option<PatchEdit> {
        match self.override_of(segment).and_then(|o| o.direction) {
            Some(Direction::Reverse) => {
                self.clear_override(segment, OverrideFieldKind::Direction);
                Some(PatchEdit::default())
            }
            _ => self.set_override(segment, OverrideField::Direction(Direction::Reverse)),
        }
    }

    /// Turn a segment's run inside its own span by `lamps`: each step
    /// moves where the data enters it back one lamp, whichever way it runs.
    pub fn turn_placement(&mut self, segment: &TreeRef, lamps: i64) -> Option<PatchEdit> {
        match self.turned_offset(segment, lamps)? {
            0 => {
                self.clear_override(segment, OverrideFieldKind::Offset);
                Some(PatchEdit::default())
            }
            to => self.set_override(segment, OverrideField::Offset(to)),
        }
    }

    /// The offset a segment has after turning it by `lamps`: each step
    /// moves where the data enters it back one lamp, whichever way it runs.
    /// `None` for a segment that isn't there.
    pub fn turned_offset(&self, segment: &TreeRef, lamps: i64) -> Option<u32> {
        let n = self.segment(segment)?.lamps.len() as i64;
        if n == 0 {
            return Some(0);
        }
        let (reversed, turn) = self
            .effective_patch()
            .walked(segment)
            .map_or((false, 0), |w| (w.direction.is_reverse(), w.offset));
        let step = if reversed { -lamps } else { lamps };
        Some((turn as i64 + step).rem_euclid(n) as u32)
    }

    /// The big turn: a third of the segment (a dome panel's side, lp2014's
    /// 0.3), at least one lamp.
    pub fn coarse_turn(&self, segment: &TreeRef) -> u32 {
        let n = self.segment(segment).map_or(0, |o| o.lamps.len());
        ((n as f64 / 3.0).round() as u32).max(1)
    }

    /// Move a segment's start `pixels` along its port. A following segment
    /// gets a start of its own first. It can't leave the port: past either
    /// end is refused, and nothing changes.
    pub fn shift_placement(
        &mut self,
        segment: &TreeRef,
        pixels: i64,
    ) -> Result<PatchEdit, PortEdge> {
        let before = self.clone();
        if self.pin_segment(segment).is_none() {
            return Err(PortEdge::Start);
        }
        let OnWire::Placed {
            output,
            port,
            pixel,
            ..
        } = self.on_wire(segment)
        else {
            *self = before;
            return Err(PortEdge::Start);
        };
        let to = pixel as i64 + pixels;
        let counted = self
            .output(&output)
            .and_then(|o| o.port(&port))
            .and_then(|p| p.pixels);
        let edge = if to < 0 {
            Some(PortEdge::Start)
        } else if counted.is_some_and(|n| to >= n as i64) {
            Some(PortEdge::End)
        } else {
            None
        };
        if let Some(edge) = edge {
            *self = before;
            return Err(edge);
        }
        Ok(self
            .patch_segment(segment, &output, &port, to as u32)
            .unwrap_or_default())
    }

    /// Two segments trade starts on the wire, each keeping its own
    /// direction and offset. When only `a` is on the wire, `b` takes its
    /// place and `a` comes off. `None` when neither is.
    pub fn swap_placements(&mut self, a: &TreeRef, b: &TreeRef) -> Option<PatchEdit> {
        let place = |p: &Project, o: &TreeRef| {
            let at = p.effective_patch().walked(o)?.landed.clone()?;
            p.effective_patch().lands(o).then_some(())?;
            Some((at.output?, at.port?, at.channel / p.channels_per_lamp_of(o)))
        };
        let (at_a, at_b) = (place(self, a), place(self, b));
        if at_a.is_none() && at_b.is_none() {
            return None;
        }
        for (segment, to) in [(a, at_b), (b, at_a)] {
            match to {
                Some((output, port, pixel)) => {
                    self.patch_segment(segment, &output, &port, pixel);
                }
                None => {
                    self.set_override(segment, OverrideField::Off(true));
                }
            }
        }
        Some(PatchEdit::default())
    }

    /// Where a fixture starts, when it says.
    pub fn fixture_start(&self, fixture: &NodeId) -> Option<&Start> {
        self.fixture(fixture)
            .map(|f| &f.mapping.start)
            .filter(|s| !s.is_keep())
    }

    /// Take every segment of a fixture off the wire, for patching it by
    /// hand from nothing: each segment off, and every start of its own
    /// gone. Returns how many segments went dark.
    pub fn unpatch_all(&mut self, fixture: &NodeId) -> usize {
        let leaves: Vec<TreeId> = self
            .segments_in(fixture)
            .iter()
            .map(|o| o.id.id.clone())
            .collect();
        let lit: Vec<bool> = leaves
            .iter()
            .map(|o| {
                self.effective_patch()
                    .lands(&TreeRef::new(fixture, o.clone()))
            })
            .collect();
        for o in &leaves {
            let node = TreeRef::new(fixture, o.clone());
            self.set_node_start(&node, Start::KEEP);
            self.set_override(&node, OverrideField::Off(true));
        }
        lit.iter().filter(|l| **l).count()
    }
}

/// A port key as the patcher holds it, when the text is one.
pub fn port_key(key: &str) -> Option<PortKey> {
    key.parse().ok().map(PortKey)
}
