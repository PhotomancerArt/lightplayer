//! Patching by hand: the edits a person makes to a fixture's rules, one
//! object at a time (the lab roadmap's stage 4).
//!
//! Every edit here writes place rules (`patch.rs`): put an object's run
//! on a port's pixel, take it off, reverse it, turn it, shift it, swap two.
//! The effective patch is derived afterwards, as always (D7).
//!
//! **A wiring edit moves only what it edits.** Auto flows a fixture's
//! other lamps on after its own runs, and other auto fixtures after every
//! run on the output (`patch.rs`, the product's arithmetic). So placing
//! one object by hand can move objects nobody touched. When it would,
//! the edit pins each of them where it was, as a place rule of its own,
//! and says how many it pinned. Nothing the person didn't touch moves.
//!
//! Turning is **direction-aware**, as lp2014's keys were: one step moves
//! where the data enters the object back one lamp, whichever way its run
//! lies, so the same key turns the same way on the piece after a reverse.

use std::collections::HashMap;

use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;
use crate::domain::patch::{PatchRule, Span};
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
    /// A place rule puts it there.
    Placed {
        output: NodeId,
        port: String,
        pixel: u32,
        reversed: bool,
        rotation: u32,
    },
    /// Its fixture's auto rule flows it on.
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
        if let Some(PatchRule::Place {
            output,
            port,
            pixel,
            reversed,
            rotation,
            ..
        }) = self.place_rule(object)
        {
            return OnWire::Placed {
                output: output.clone(),
                port: port.clone(),
                pixel: *pixel,
                reversed: *reversed,
                rotation: *rotation,
            };
        }
        if self.effective_patch().lands(object) {
            OnWire::Flowing
        } else {
            OnWire::Nowhere
        }
    }

    /// Put an object's run on `pixel` of `output`'s port `port`. A placed
    /// object moves there and keeps its direction and turn.
    pub fn patch_object(
        &mut self,
        object: &ObjectId,
        output: &NodeId,
        port: &str,
        pixel: u32,
    ) -> Option<PatchEdit> {
        self.fixture_of(&object.component)?;
        Some(self.hand_edit(&[object.clone()], |p| {
            let (output, port) = (output.clone(), port.to_string());
            match p.place_rule_mut(object) {
                Some(PatchRule::Place {
                    output: o,
                    port: k,
                    pixel: x,
                    ..
                }) => {
                    *o = output;
                    *k = port;
                    *x = pixel;
                }
                _ => p.push_place(object, output, port, pixel, false, 0),
            }
        }))
    }

    /// Take an object's place rules away. Under auto it flows again; in a
    /// manual fixture it goes dark. False when it had none.
    pub fn unpatch_object(&mut self, object: &ObjectId) -> bool {
        if self.place_rule(object).is_none() {
            return false;
        }
        self.hand_edit(&[object.clone()], |p| p.remove_place_rules(object));
        true
    }

    /// Write a place rule where a flowing object lands now, so a wiring
    /// edit can change it. Nothing to do for a placed one; `None` for one
    /// that lands nowhere (there is no "where it is" to keep).
    pub fn pin_object(&mut self, object: &ObjectId) -> Option<PatchEdit> {
        match self.on_wire(object) {
            OnWire::Placed { .. } => Some(PatchEdit::default()),
            OnWire::Nowhere => None,
            OnWire::Flowing => {
                let spans: Vec<Span> = self.effective_patch().spans_of(object).cloned().collect();
                Some(self.hand_edit(&[object.clone()], |p| p.pin_at(object, &spans)))
            }
        }
    }

    /// Lay an object's run the other way along its window: the first lamp
    /// lands where the last did. A flowing object is pinned first.
    pub fn reverse_placement(&mut self, object: &ObjectId) -> Option<PatchEdit> {
        let pinned = self.pin_object(object)?;
        let edit = self.hand_edit(&[object.clone()], |p| {
            if let Some(PatchRule::Place { reversed, .. }) = p.place_rule_mut(object) {
                *reversed = !*reversed;
            }
        });
        Some(PatchEdit {
            pinned: pinned.pinned + edit.pinned,
        })
    }

    /// Turn an object's run inside its own window by `lamps`: each step
    /// moves where the data enters it back one lamp, whichever way it runs.
    pub fn turn_placement(&mut self, object: &ObjectId, lamps: i64) -> Option<PatchEdit> {
        let n = self.object(object)?.lamps.len() as i64;
        let pinned = self.pin_object(object)?;
        if n == 0 {
            return Some(pinned);
        }
        let edit = self.hand_edit(&[object.clone()], |p| {
            if let Some(PatchRule::Place {
                reversed, rotation, ..
            }) = p.place_rule_mut(object)
            {
                let step = if *reversed { -lamps } else { lamps };
                *rotation = (*rotation as i64 + step).rem_euclid(n) as u32;
            }
        });
        Some(PatchEdit {
            pinned: pinned.pinned + edit.pinned,
        })
    }

    /// The big turn: a third of the object (a dome panel's side, lp2014's
    /// 0.3), at least one lamp.
    pub fn coarse_turn(&self, object: &ObjectId) -> u32 {
        let n = self.object(object).map_or(0, |o| o.lamps.len());
        ((n as f64 / 3.0).round() as u32).max(1)
    }

    /// Move an object's start `pixels` along its port. It can't leave the
    /// port: past either end is refused, and nothing changes.
    pub fn shift_placement(
        &mut self,
        object: &ObjectId,
        pixels: i64,
    ) -> Result<PatchEdit, PortEdge> {
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
        if to < 0 {
            return Err(PortEdge::Start);
        }
        let counted = self
            .output(&output)
            .and_then(|o| o.port(&port))
            .and_then(|p| p.pixels);
        if counted.is_some_and(|n| to >= n as i64) {
            return Err(PortEdge::End);
        }
        let edit = self.hand_edit(&[object.clone()], |p| {
            if let Some(PatchRule::Place { pixel, .. }) = p.place_rule_mut(object) {
                *pixel = to as u32;
            }
        });
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
            } => Some((output, port, pixel)),
            _ => None,
        };
        let (at_a, at_b) = (place(self, a), place(self, b));
        if at_a.is_none() && at_b.is_none() {
            return None;
        }
        let edit = self.hand_edit(&[a.clone(), b.clone()], |p| {
            for (object, to) in [(a, at_b.clone()), (b, at_a.clone())] {
                match to {
                    Some((output, port, pixel)) => match p.place_rule_mut(object) {
                        Some(PatchRule::Place {
                            output: o,
                            port: k,
                            pixel: x,
                            ..
                        }) => {
                            *o = output;
                            *k = port;
                            *x = pixel;
                        }
                        _ => p.push_place(object, output, port, pixel, false, 0),
                    },
                    None => p.remove_place_rules(object),
                }
            }
        });
        Some(PatchEdit {
            pinned: pinned + edit.pinned,
        })
    }

    /// Turn a fixture's auto rule on or off. On, it flows every lamp no
    /// place rule claims onto the output its rules already reach (or the
    /// first output); off, those lamps go dark. Returns the output auto
    /// flows onto, when it is on.
    pub fn set_auto(&mut self, fixture: &NodeId, on: bool) -> Option<NodeId> {
        let named = self.control_outputs(fixture).into_iter().next();
        let first = self.outputs().first().map(|n| n.id.clone());
        let f = self.fixture_mut(fixture)?;
        let current = f.rules.iter().find_map(|r| match r {
            PatchRule::Auto { output } => Some(output.clone()),
            _ => None,
        });
        if on {
            if current.is_some() {
                return current;
            }
            let output = named.or(first)?;
            f.rules.push(PatchRule::Auto {
                output: output.clone(),
            });
            Some(output)
        } else {
            f.rules.retain(|r| !matches!(r, PatchRule::Auto { .. }));
            None
        }
    }

    /// Whether a fixture flows its unplaced lamps on by itself.
    pub fn is_auto(&self, fixture: &NodeId) -> bool {
        self.fixture(fixture)
            .is_some_and(|f| f.rules.iter().any(|r| matches!(r, PatchRule::Auto { .. })))
    }

    /// Take every one of a fixture's objects off the wire by hand: every
    /// place rule goes. Under auto, they all flow again; manual, the
    /// fixture goes dark. Returns how many rules went.
    pub fn unpatch_all(&mut self, fixture: &NodeId) -> usize {
        let Some(f) = self.fixture_mut(fixture) else {
            return 0;
        };
        let before = f.rules.len();
        f.rules.retain(|r| !matches!(r, PatchRule::Place { .. }));
        before - f.rules.len()
    }

    // ---- helpers -----------------------------------------------------------

    /// The place rule that places an object: the first naming it.
    fn place_rule(&self, object: &ObjectId) -> Option<&PatchRule> {
        let f = self.fixture_of(&object.component)?.fixture()?;
        f.rules
            .iter()
            .find(|r| matches!(r, PatchRule::Place { object: o, .. } if o == object))
    }

    fn place_rule_mut(&mut self, object: &ObjectId) -> Option<&mut PatchRule> {
        let fixture = self.fixture_of(&object.component)?.id.clone();
        self.fixture_mut(&fixture)?
            .rules
            .iter_mut()
            .find(|r| matches!(r, PatchRule::Place { object: o, .. } if o == object))
    }

    fn push_place(
        &mut self,
        object: &ObjectId,
        output: NodeId,
        port: String,
        pixel: u32,
        reversed: bool,
        rotation: u32,
    ) {
        let Some(fixture) = self.fixture_of(&object.component).map(|n| n.id.clone()) else {
            return;
        };
        if let Some(f) = self.fixture_mut(&fixture) {
            f.rules.push(PatchRule::Place {
                object: object.clone(),
                output,
                port,
                pixel,
                reversed,
                rotation,
            });
        }
    }

    fn remove_place_rules(&mut self, object: &ObjectId) {
        let Some(fixture) = self.fixture_of(&object.component).map(|n| n.id.clone()) else {
            return;
        };
        if let Some(f) = self.fixture_mut(&fixture) {
            f.rules
                .retain(|r| !matches!(r, PatchRule::Place { object: o, .. } if o == object));
        }
    }

    /// A place rule that lays an object exactly where `spans` (its spans
    /// as auto flowed it: forward, unturned) put it.
    fn pin_at(&mut self, object: &ObjectId, spans: &[Span]) {
        let Some(first) = spans.first() else {
            return;
        };
        let Some(out) = self.output(&first.output) else {
            return;
        };
        let Some(port_start) = out.port_start(&first.port) else {
            return;
        };
        // Where lamp 0 sits on the output's flat wire.
        let wire = (port_start + first.start_pixel).saturating_sub(first.lamps.start);
        let (port, pixel) = match out.port_at(wire) {
            Some((i, pixel)) => (out.ports[i].key.clone(), pixel),
            None => (first.port.clone(), first.start_pixel),
        };
        let output = first.output.clone();
        self.push_place(object, output, port, pixel, false, 0);
    }

    /// Run an edit to the rules, then pin every object it moved that it
    /// wasn't asked to move, where it was, until nothing else has moved.
    fn hand_edit(&mut self, edited: &[ObjectId], edit: impl FnOnce(&mut Project)) -> PatchEdit {
        let before = spans_by_object(&self.effective_patch().spans);
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
                .filter(|(o, _)| !stuck.contains(o) && self.place_rule(o).is_none())
                .map(|(o, _)| o.clone())
                .collect();
            if moved.is_empty() {
                return PatchEdit { pinned };
            }
            for o in moved {
                self.pin_at(&o, &before[&o]);
                if self.place_rule(&o).is_some() {
                    pinned += 1;
                } else {
                    stuck.push(o);
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
