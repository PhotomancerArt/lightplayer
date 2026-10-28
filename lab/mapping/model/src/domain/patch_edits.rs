//! Patching by hand: the edits a person makes to rules (`patch_rule.rs`)
//! and overrides (`object_override.rs`).
//!
//! Every edit sets or clears one thing on one fixture, component or object.
//! The effective patch is derived afterwards, as always (D7), by the walker.
//!
//! **An edit moves what follows it.** The walker's cursor carries on from
//! wherever an object starts (theory "Patching": the cursor continues, no
//! snapping), so putting ring 3 on D9 takes rings 4 and 5 along with it
//! (Yona, D1b). Nothing is pinned behind a person's back; what moved is
//! what the walk says follows.
//!
//! Turning is **direction-aware**, as lp2014's keys were: one step moves
//! where the data enters the object back one lamp, whichever way its run
//! lies, so the same key turns the same way on the piece after a reverse.

use crate::domain::cursor_op::{Amount, ChannelOp, OutputOp, PortOp};
use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;
use crate::domain::object_override::{Direction, ObjectOverride};
use crate::domain::patch_rule::{Children, Start};
use crate::domain::patch_target::PatchTarget;
use crate::domain::project::Project;

/// What an edit did besides what it was asked. Kept for the hint the keys
/// show; the walker never pins anything, so it is always zero now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PatchEdit {
    pub pinned: usize,
}

/// How an object is on the wire.
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

impl Project {
    /// How an object is on the wire right now.
    pub fn on_wire(&self, object: &ObjectId) -> OnWire {
        let patch = self.effective_patch();
        let own = self.override_of(object);
        if own.is_some_and(|o| o.moves_start() && !o.off)
            && let Some(w) = patch.walked(object)
            && let Some(at) = &w.landed
            && let (Some(output), Some(port)) = (&at.output, &at.port)
        {
            let per = self.channels_per_lamp_of(object);
            return OnWire::Placed {
                output: output.clone(),
                port: port.clone(),
                pixel: at.channel / per,
                reversed: w.direction.is_reversed(),
                rotation: w.offset,
            };
        }
        if patch.lands(object) {
            OnWire::Flowing
        } else {
            OnWire::Nowhere
        }
    }

    /// An object's override, if it has one.
    pub fn override_of(&self, object: &ObjectId) -> Option<&ObjectOverride> {
        let fixture = self.fixture_of_place(&PatchTarget::Object(object.clone()))?;
        self.fixture(&fixture)?.overrides.get(object)
    }

    /// How many channels a lamp of this object takes: its fixture's.
    pub fn channels_per_lamp_of(&self, object: &ObjectId) -> u32 {
        self.fixture_of_place(&PatchTarget::Object(object.clone()))
            .and_then(|f| self.fixture(&f))
            .map_or(3, |f| f.channels_per_lamp.max(1))
    }

    /// The fixture a patch target belongs to (`None` for the fixture
    /// itself, which names no fixture on its own).
    pub fn fixture_of_place(&self, target: &PatchTarget) -> Option<NodeId> {
        match target {
            PatchTarget::Fixture => None,
            PatchTarget::Component(c) => self.fixture_of(c).map(|n| n.id.clone()),
            PatchTarget::Object(o) => self.fixture_of(&o.component).map(|n| n.id.clone()),
        }
    }

    /// The leaves a patch target reaches.
    pub fn objects_of_place(&self, fixture: &NodeId, target: &PatchTarget) -> Vec<ObjectId> {
        match target {
            PatchTarget::Fixture => self
                .objects_in(fixture)
                .iter()
                .map(|o| o.id.clone())
                .collect(),
            PatchTarget::Component(c) => {
                self.objects_under(c).iter().map(|o| o.id.clone()).collect()
            }
            PatchTarget::Object(o) => match self.placed().node(o) {
                Some((_, n)) => n.leaves().iter().map(|l| l.id.clone()).collect(),
                None => vec![o.clone()],
            },
        }
    }

    // ---- rules -------------------------------------------------------------

    /// Set where a fixture or a component starts.
    pub fn set_start(&mut self, fixture: &NodeId, target: &PatchTarget, start: Start) -> bool {
        let Some(f) = self.fixture_mut(fixture) else {
            return false;
        };
        match target {
            PatchTarget::Fixture => f.start = start,
            PatchTarget::Component(c) => f.rules.entry(c.clone()).or_default().start = start,
            PatchTarget::Object(_) => return false,
        }
        f.tidy_patching();
        true
    }

    /// Set how a fixture's or a container's children follow each other.
    pub fn set_children(
        &mut self,
        fixture: &NodeId,
        target: &PatchTarget,
        children: Children,
    ) -> bool {
        let Some(f) = self.fixture_mut(fixture) else {
            return false;
        };
        match target {
            PatchTarget::Fixture => f.children = children,
            PatchTarget::Component(c) => f.rules.entry(c.clone()).or_default().children = children,
            PatchTarget::Object(_) => return false,
        }
        f.tidy_patching();
        true
    }

    // ---- overrides ---------------------------------------------------------

    /// Edit one object's override (made empty if it has none); one that
    /// ends up overriding nothing goes.
    pub fn edit_override(
        &mut self,
        object: &ObjectId,
        edit: impl FnOnce(&mut ObjectOverride),
    ) -> Option<PatchEdit> {
        let fixture = self.fixture_of_place(&PatchTarget::Object(object.clone()))?;
        let f = self.fixture_mut(&fixture)?;
        edit(f.overrides.entry(object.clone()).or_default());
        f.tidy_patching();
        Some(PatchEdit::default())
    }

    /// Start an object at `pixel` of `output`'s port `port`, on the wire
    /// again if it was off. It keeps its direction and offset; what follows
    /// it moves along.
    pub fn patch_object(
        &mut self,
        object: &ObjectId,
        output: &NodeId,
        port: &str,
        pixel: u32,
    ) -> Option<PatchEdit> {
        self.edit_override(object, |o| {
            o.output = Some(OutputOp::Set(output.clone()));
            o.port = Some(PortOp::set(port));
            o.channel = Some(ChannelOp::Set(Amount::px(pixel)));
            o.off = false;
        })
    }

    /// Take an object's own start away, so it follows its rule again.
    /// False when it had none.
    pub fn unpatch_object(&mut self, object: &ObjectId) -> bool {
        if !self.override_of(object).is_some_and(|o| o.moves_start()) {
            return false;
        }
        self.edit_override(object, |o| {
            o.output = None;
            o.port = None;
            o.channel = None;
        });
        true
    }

    /// Give a following object a start of its own where it lands now, so a
    /// wiring edit can move it. Nothing to do for one placed already;
    /// `None` for one that lands nowhere (there is no "where it is" to
    /// keep).
    pub fn pin_object(&mut self, object: &ObjectId) -> Option<PatchEdit> {
        match self.on_wire(object) {
            OnWire::Placed { .. } => Some(PatchEdit::default()),
            OnWire::Nowhere => None,
            OnWire::Flowing => {
                let at = self.effective_patch().walked(object)?.landed.clone()?;
                let (output, port) = (at.output?, at.port?);
                let pixel = at.channel / self.channels_per_lamp_of(object);
                self.patch_object(object, &output, &port, pixel)
            }
        }
    }

    /// Lay an object's run the other way inside its span. It stays where it
    /// is: a direction of its own.
    pub fn reverse_placement(&mut self, object: &ObjectId) -> Option<PatchEdit> {
        let own = self.override_of(object).and_then(|o| o.direction);
        self.edit_override(object, |o| {
            o.direction = match own {
                Some(Direction::Reversed) => None,
                _ => Some(Direction::Reversed),
            };
        })
    }

    /// Turn an object's run inside its own span by `lamps`: each step moves
    /// where the data enters it back one lamp, whichever way it runs.
    pub fn turn_placement(&mut self, object: &ObjectId, lamps: i64) -> Option<PatchEdit> {
        let n = self.object(object)?.lamps.len() as i64;
        if n == 0 {
            return Some(PatchEdit::default());
        }
        let (reversed, turn) = self
            .effective_patch()
            .walked(object)
            .map_or((false, 0), |w| (w.direction.is_reversed(), w.offset));
        let step = if reversed { -lamps } else { lamps };
        let to = (turn as i64 + step).rem_euclid(n) as u32;
        self.edit_override(object, |o| o.offset = (to != 0).then_some(to))
    }

    /// The big turn: a third of the object (a dome panel's side, lp2014's
    /// 0.3), at least one lamp.
    pub fn coarse_turn(&self, object: &ObjectId) -> u32 {
        let n = self.object(object).map_or(0, |o| o.lamps.len());
        ((n as f64 / 3.0).round() as u32).max(1)
    }

    /// Move an object's start `pixels` along its port. A following object
    /// gets a start of its own first. It can't leave the port: past either
    /// end is refused, and nothing changes.
    pub fn shift_placement(
        &mut self,
        object: &ObjectId,
        pixels: i64,
    ) -> Result<PatchEdit, PortEdge> {
        let before = self.clone();
        if self.pin_object(object).is_none() {
            return Err(PortEdge::Start);
        }
        let OnWire::Placed {
            output,
            port,
            pixel,
            ..
        } = self.on_wire(object)
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
            .patch_object(object, &output, &port, to as u32)
            .unwrap_or_default())
    }

    /// Two objects trade starts on the wire, each keeping its own direction
    /// and offset. When only `a` is on the wire, `b` takes its place and
    /// `a` comes off. `None` when neither is.
    pub fn swap_placements(&mut self, a: &ObjectId, b: &ObjectId) -> Option<PatchEdit> {
        let place = |p: &Project, o: &ObjectId| {
            let at = p.effective_patch().walked(o)?.landed.clone()?;
            p.effective_patch().lands(o).then_some(())?;
            Some((at.output?, at.port?, at.channel / p.channels_per_lamp_of(o)))
        };
        let (at_a, at_b) = (place(self, a), place(self, b));
        if at_a.is_none() && at_b.is_none() {
            return None;
        }
        for (object, to) in [(a, at_b), (b, at_a)] {
            match to {
                Some((output, port, pixel)) => {
                    self.patch_object(object, &output, &port, pixel);
                }
                None => {
                    self.edit_override(object, |o| o.off = true);
                }
            }
        }
        Some(PatchEdit::default())
    }

    /// Where a fixture starts, when it says.
    pub fn fixture_start(&self, fixture: &NodeId) -> Option<&Start> {
        self.fixture(fixture)
            .map(|f| &f.start)
            .filter(|s| !matches!(s, Start::Follows))
    }

    /// Take every object of a fixture off the wire, for patching it by hand
    /// from nothing: each leaf off, and every start of its own gone. Returns
    /// how many objects went dark.
    pub fn unpatch_all(&mut self, fixture: &NodeId) -> usize {
        let leaves: Vec<ObjectId> = self
            .objects_in(fixture)
            .iter()
            .map(|o| o.id.clone())
            .collect();
        let lit: Vec<bool> = leaves
            .iter()
            .map(|o| self.effective_patch().lands(o))
            .collect();
        let Some(f) = self.fixture_mut(fixture) else {
            return 0;
        };
        for o in &leaves {
            let own = f.overrides.entry(o.clone()).or_default();
            own.output = None;
            own.port = None;
            own.channel = None;
            own.off = true;
        }
        f.tidy_patching();
        lit.iter().filter(|l| **l).count()
    }
}
