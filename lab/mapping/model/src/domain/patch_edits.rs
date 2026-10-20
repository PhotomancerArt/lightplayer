//! Patching by hand: the edits a person makes to placements
//! (`patch_place.rs`), on a fixture, a component or an object.
//!
//! Every edit sets or clears one setting — wire, direction or turn — on one
//! thing; what isn't set follows its parent. The effective patch is derived
//! afterwards, as always (D7).
//!
//! **A wiring edit moves only what it edits.** Auto flows a fixture's
//! other objects on after its own runs, and other auto fixtures after every
//! run on the output (`patch.rs`, the product's arithmetic). So placing one
//! object by hand can move objects nobody touched. When it would, the edit
//! pins each of them where it was, with a wire of its own, and says how
//! many it pinned. A placed component's own objects are not held: when one
//! leaves its run, the others close up behind it (plan D10).
//!
//! Turning is **direction-aware**, as lp2014's keys were: one step moves
//! where the data enters the object back one lamp, whichever way its run
//! lies, so the same key turns the same way on the piece after a reverse.

use std::collections::HashMap;

use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;
use crate::domain::patch::{ObjectRun, Span};
use crate::domain::patch_place::{Direction, PatchTarget, Wire};
use crate::domain::project::Project;

/// What an edit did besides what it was asked: how many other objects it
/// pinned where they were, so they wouldn't move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PatchEdit {
    pub pinned: usize,
}

/// How an object is on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnWire {
    /// Its own placement puts it at an address.
    Placed {
        output: NodeId,
        port: String,
        pixel: u32,
        reversed: bool,
        rotation: u32,
    },
    /// It lands, by what it follows (auto, or its component's address).
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
        if let Some(r) = patch.resolved(object)
            && let (
                Some(Wire::At {
                    output,
                    port,
                    pixel,
                }),
                Some(PatchTarget::Object(_)),
            ) = (&r.wire, &r.wire_from)
        {
            return OnWire::Placed {
                output: output.clone(),
                port: port.clone(),
                pixel: *pixel,
                reversed: r.direction.is_reversed(),
                rotation: r.turn,
            };
        }
        if patch.lands(object) {
            OnWire::Flowing
        } else {
            OnWire::Nowhere
        }
    }

    /// The fixture a placement target belongs to (`None` for the fixture
    /// itself, which names no fixture on its own).
    pub fn fixture_of_place(&self, target: &PatchTarget) -> Option<NodeId> {
        match target {
            PatchTarget::Fixture => None,
            PatchTarget::Component(c) => self.fixture_of(c).map(|n| n.id.clone()),
            PatchTarget::Object(o) => self.fixture_of(&o.component).map(|n| n.id.clone()),
        }
    }

    /// The objects a placement on `target` reaches.
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
            PatchTarget::Object(o) => vec![o.clone()],
        }
    }

    /// Set (or, with `None`, clear) a thing's wire. Clearing it makes it
    /// follow its parent.
    pub fn set_wire(
        &mut self,
        fixture: &NodeId,
        target: &PatchTarget,
        wire: Option<Wire>,
    ) -> Option<PatchEdit> {
        self.fixture(fixture)?;
        let edited = self.objects_of_place(fixture, target);
        Some(self.hand_edit(&edited, |p| {
            if let Some(f) = p.fixture_mut(fixture) {
                f.place_mut(target).wire = wire;
                f.tidy_places();
            }
        }))
    }

    /// Set (or clear) a thing's direction. Moves nothing on the wire: an
    /// object keeps its window and runs the other way inside it; a placed
    /// component keeps its run and lays its objects in the other order.
    pub fn set_direction(
        &mut self,
        fixture: &NodeId,
        target: &PatchTarget,
        direction: Option<Direction>,
    ) -> Option<PatchEdit> {
        self.fixture(fixture)?;
        let edited = self.objects_of_place(fixture, target);
        Some(self.hand_edit(&edited, |p| {
            if let Some(f) = p.fixture_mut(fixture) {
                f.place_mut(target).direction = direction;
                f.tidy_places();
            }
        }))
    }

    /// Put an object's run on `pixel` of `output`'s port `port`: a wire of
    /// its own. It keeps its direction and turn.
    pub fn patch_object(
        &mut self,
        object: &ObjectId,
        output: &NodeId,
        port: &str,
        pixel: u32,
    ) -> Option<PatchEdit> {
        let fixture = self.fixture_of_place(&PatchTarget::Object(object.clone()))?;
        let wire = Wire::At {
            output: output.clone(),
            port: port.to_string(),
            pixel,
        };
        self.set_wire(&fixture, &PatchTarget::Object(object.clone()), Some(wire))
    }

    /// Take an object's own wire away, so it follows its parent again.
    /// False when it had none.
    pub fn unpatch_object(&mut self, object: &ObjectId) -> bool {
        let target = PatchTarget::Object(object.clone());
        let Some(fixture) = self.fixture_of_place(&target) else {
            return false;
        };
        let own = self
            .fixture(&fixture)
            .and_then(|f| f.place(&target))
            .is_some_and(|p| p.wire.is_some());
        if !own {
            return false;
        }
        self.set_wire(&fixture, &target, None);
        true
    }

    /// Give a following object a wire of its own where it lands now, so a
    /// wiring edit can move it. Nothing to do for one placed already;
    /// `None` for one that lands nowhere (there is no "where it is" to
    /// keep).
    pub fn pin_object(&mut self, object: &ObjectId) -> Option<PatchEdit> {
        match self.on_wire(object) {
            OnWire::Placed { .. } => Some(PatchEdit::default()),
            OnWire::Nowhere => None,
            OnWire::Flowing => {
                let run = self
                    .effective_patch()
                    .runs
                    .iter()
                    .find(|r| &r.object == object)?
                    .clone();
                Some(self.hand_edit(&[object.clone()], |p| p.pin_at(&run)))
            }
        }
    }

    /// Lay an object's run the other way inside its window. It stays where
    /// it is, following or not: a direction of its own.
    pub fn reverse_placement(&mut self, object: &ObjectId) -> Option<PatchEdit> {
        let target = PatchTarget::Object(object.clone());
        let fixture = self.fixture_of_place(&target)?;
        let now = self
            .effective_patch()
            .resolved(object)
            .map(|r| r.direction)
            .unwrap_or_default();
        self.set_direction(&fixture, &target, Some(now.flipped()))
    }

    /// Turn an object's run inside its own window by `lamps`: each step
    /// moves where the data enters it back one lamp, whichever way it runs.
    pub fn turn_placement(&mut self, object: &ObjectId, lamps: i64) -> Option<PatchEdit> {
        let n = self.object(object)?.lamps.len() as i64;
        let target = PatchTarget::Object(object.clone());
        let fixture = self.fixture_of_place(&target)?;
        if n == 0 {
            return Some(PatchEdit::default());
        }
        let (reversed, turn) = self
            .effective_patch()
            .resolved(object)
            .map_or((false, 0), |r| (r.direction.is_reversed(), r.turn));
        let step = if reversed { -lamps } else { lamps };
        let to = (turn as i64 + step).rem_euclid(n) as u32;
        let edited = vec![object.clone()];
        Some(self.hand_edit(&edited, |p| {
            if let Some(f) = p.fixture_mut(&fixture) {
                f.place_mut(&target).turn = (to != 0).then_some(to);
                f.tidy_places();
            }
        }))
    }

    /// The big turn: a third of the object (a dome panel's side, lp2014's
    /// 0.3), at least one lamp.
    pub fn coarse_turn(&self, object: &ObjectId) -> u32 {
        let n = self.object(object).map_or(0, |o| o.lamps.len());
        ((n as f64 / 3.0).round() as u32).max(1)
    }

    /// Move an object's start `pixels` along its port. A following object
    /// gets a wire of its own first. It can't leave the port: past either
    /// end is refused, and nothing changes.
    pub fn shift_placement(
        &mut self,
        object: &ObjectId,
        pixels: i64,
    ) -> Result<PatchEdit, PortEdge> {
        let before = self.clone();
        let Some(pinned) = self.pin_object(object) else {
            return Err(PortEdge::Start);
        };
        let OnWire::Placed {
            output,
            port,
            pixel,
            ..
        } = self.on_wire(object)
        else {
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
        let edit = self
            .patch_object(object, &output, &port, to as u32)
            .unwrap_or_default();
        Ok(PatchEdit {
            pinned: pinned.pinned + edit.pinned,
        })
    }

    /// Two objects trade places on the wire, each keeping its own direction
    /// and turn. When only `a` is on the wire, `b` takes its place and `a`
    /// comes off. `None` when neither is.
    pub fn swap_placements(&mut self, a: &ObjectId, b: &ObjectId) -> Option<PatchEdit> {
        let pa = self.pin_object(a);
        let pb = self.pin_object(b);
        let pinned = pa.map_or(0, |e| e.pinned) + pb.map_or(0, |e| e.pinned);
        let place = |p: &Project, o: &ObjectId| match p.on_wire(o) {
            OnWire::Placed {
                output,
                port,
                pixel,
                ..
            } => Some(Wire::At {
                output,
                port,
                pixel,
            }),
            _ => None,
        };
        let (at_a, at_b) = (place(self, a), place(self, b));
        if at_a.is_none() && at_b.is_none() {
            return None;
        }
        let fixture_a = self.fixture_of_place(&PatchTarget::Object(a.clone()))?;
        let fixture_b = self.fixture_of_place(&PatchTarget::Object(b.clone()))?;
        let edit = self.hand_edit(&[a.clone(), b.clone()], |p| {
            for (fixture, object, to) in [(&fixture_a, a, at_b), (&fixture_b, b, at_a)] {
                if let Some(f) = p.fixture_mut(fixture) {
                    // Off the wire when the other had no place to give.
                    f.place_mut(&PatchTarget::Object(object.clone())).wire =
                        Some(to.unwrap_or(Wire::Off));
                    f.tidy_places();
                }
            }
        });
        Some(PatchEdit {
            pinned: pinned + edit.pinned,
        })
    }

    /// Turn a fixture's auto on or off: its own wire, auto onto the output
    /// its placements already reach (or the first output), or unset. Off,
    /// what follows the fixture goes dark. Returns the output auto flows
    /// onto, when it is on.
    pub fn set_auto(&mut self, fixture: &NodeId, on: bool) -> Option<NodeId> {
        let named = self.control_outputs(fixture).into_iter().next();
        let first = self.outputs().first().map(|n| n.id.clone());
        let current = self.auto_output(fixture);
        let f = self.fixture_mut(fixture)?;
        if on {
            if current.is_some() {
                return current;
            }
            let output = named.or(first)?;
            f.place_mut(&PatchTarget::Fixture).wire = Some(Wire::Auto {
                output: output.clone(),
            });
            Some(output)
        } else {
            f.place_mut(&PatchTarget::Fixture).wire = None;
            f.tidy_places();
            None
        }
    }

    /// The output a fixture's own wire flows onto, when it is auto.
    pub fn auto_output(&self, fixture: &NodeId) -> Option<NodeId> {
        match self
            .fixture(fixture)?
            .place(&PatchTarget::Fixture)?
            .wire
            .as_ref()?
        {
            Wire::Auto { output } => Some(output.clone()),
            _ => None,
        }
    }

    /// Whether a fixture flows its following lamps on by itself.
    pub fn is_auto(&self, fixture: &NodeId) -> bool {
        self.auto_output(fixture).is_some()
    }

    /// Put every component and object of a fixture back to following the
    /// fixture: every wire set below it goes. Under auto they all flow
    /// again; with the fixture's wire unset, it goes dark. Returns how many
    /// wires went.
    pub fn unpatch_all(&mut self, fixture: &NodeId) -> usize {
        let Some(f) = self.fixture_mut(fixture) else {
            return 0;
        };
        let mut n = 0;
        for p in f.places.iter_mut() {
            if p.on != PatchTarget::Fixture && p.wire.take().is_some() {
                n += 1;
            }
        }
        f.tidy_places();
        n
    }

    // ---- helpers -----------------------------------------------------------

    /// A wire of its own for an object, at the window `run` gives it (what
    /// it follows put it there).
    fn pin_at(&mut self, run: &ObjectRun) {
        let Some(out) = self.output(&run.output) else {
            return;
        };
        let Some((i, pixel)) = out.port_at(run.wire) else {
            return;
        };
        let port = out.ports[i].key.clone();
        let target = PatchTarget::Object(run.object.clone());
        if let Some(f) = self.fixture_mut(&run.fixture) {
            f.place_mut(&target).wire = Some(Wire::At {
                output: run.output.clone(),
                port,
                pixel,
            });
        }
    }

    /// Run an edit to the placements, then pin every object it moved that
    /// it wasn't asked to move, where it was, until nothing else has moved.
    fn hand_edit(&mut self, edited: &[ObjectId], edit: impl FnOnce(&mut Project)) -> PatchEdit {
        let before = spans_by_object(&self.effective_patch().spans);
        // Only what auto flowed is held still: a placed component's
        // objects close up behind one that leaves it (plan D10).
        let flowing: Vec<ObjectId> = self
            .effective_patch()
            .resolved
            .iter()
            .filter(|r| matches!(r.wire, Some(Wire::Auto { .. })))
            .map(|r| r.object.clone())
            .collect();
        let runs_before: HashMap<ObjectId, ObjectRun> = self
            .effective_patch()
            .runs
            .iter()
            .map(|r| (r.object.clone(), r.clone()))
            .collect();
        edit(self);
        let mut pinned = 0;
        // Objects a pin couldn't hold (their output or port is gone).
        let mut stuck: Vec<ObjectId> = Vec::new();
        // Each round pins, or gives up on, at least one more object, so
        // this ends.
        loop {
            let after = spans_by_object(&self.effective_patch().spans);
            let moved: Vec<ObjectId> = before
                .iter()
                .filter(|(o, was)| !edited.contains(o) && after.get(*o) != Some(*was))
                .filter(|(o, _)| flowing.contains(o) && !stuck.contains(o))
                .filter(|(o, _)| !matches!(self.on_wire(o), OnWire::Placed { .. }))
                .map(|(o, _)| o.clone())
                .collect();
            if moved.is_empty() {
                return PatchEdit { pinned };
            }
            for o in moved {
                match runs_before.get(&o) {
                    Some(run) => {
                        self.pin_at(run);
                        if matches!(self.on_wire(&o), OnWire::Placed { .. }) {
                            pinned += 1;
                        } else {
                            stuck.push(o);
                        }
                    }
                    None => stuck.push(o),
                }
            }
        }
    }
}

fn spans_by_object(spans: &[Span]) -> HashMap<ObjectId, Vec<Span>> {
    let mut out: HashMap<ObjectId, Vec<Span>> = HashMap::new();
    for s in spans {
        out.entry(s.object.clone()).or_default().push(s.clone());
    }
    out
}
