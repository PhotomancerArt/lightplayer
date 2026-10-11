//! What each component kind says about the properties of the things it
//! makes. This is the ONLY place that knows a line has a lamp count and a
//! ring has a radius; the inspector reads the descriptors (`props.rs`).
//!
//! Note the object-level controls (D11): a ring's lamps, radius and
//! direction are edited with the ring selected, but they are stored on the
//! circle.

use crate::component::{ComponentKind, Ring};
use crate::fixture::Fixture;
use crate::props::{PropDesc, PropEdit, PropKind, PropRaw, edited};
use crate::target::Target;

pub const LAMPS: PropDesc = PropDesc {
    id: "lamps",
    label: "Lamps",
    kind: PropKind::Count { min: 1 },
    nudge: true,
};
pub const RINGS: PropDesc = PropDesc {
    id: "rings",
    label: "Rings",
    kind: PropKind::Count { min: 1 },
    nudge: true,
};
pub const RADIUS: PropDesc = PropDesc {
    id: "radius",
    label: "Radius",
    kind: PropKind::Number {
        min: 0.5,
        step: 0.5,
    },
    nudge: false,
};
pub const COUNTER_CLOCKWISE: PropDesc = PropDesc {
    id: "ccw",
    label: "Counter-clockwise",
    kind: PropKind::Flag,
    nudge: false,
};

impl Fixture {
    /// The properties of one target, with its values, in display order.
    pub fn props_of(&self, t: &Target) -> Vec<(PropDesc, PropRaw)> {
        match t {
            Target::Component(id) => match self.component(id).map(|c| &c.kind) {
                Some(ComponentKind::Line { lamps, .. }) => {
                    vec![(LAMPS, PropRaw::Count(*lamps as i64))]
                }
                Some(ComponentKind::Circle { rings, .. }) => vec![
                    (RINGS, PropRaw::Count(rings.len() as i64)),
                    (
                        COUNTER_CLOCKWISE,
                        PropRaw::Flag(!rings.is_empty() && rings.iter().all(|r| r.reversed)),
                    ),
                ],
                _ => Vec::new(),
            },
            Target::Object(o) => match self.ring(&o.component, &o.key) {
                Some(r) => vec![
                    (LAMPS, PropRaw::Count(r.lamps as i64)),
                    (RADIUS, PropRaw::Number(r.radius)),
                    (COUNTER_CLOCKWISE, PropRaw::Flag(r.reversed)),
                ],
                None => Vec::new(),
            },
            Target::Lamp { .. } => Vec::new(),
        }
    }

    /// Apply an edit to one target's property. Returns whether it had it.
    /// `spacing` is the lamp spacing new rings are drawn with.
    pub fn edit_prop(&mut self, t: &Target, prop_id: &str, edit: PropEdit, spacing: f64) -> bool {
        let Some((desc, current)) = self.props_of(t).into_iter().find(|(d, _)| d.id == prop_id)
        else {
            return false;
        };
        let new = edited(&desc, current, edit);
        match t {
            Target::Component(id) => {
                match (self.component_mut(id).map(|c| &mut c.kind), prop_id, new) {
                    (Some(ComponentKind::Line { lamps, .. }), "lamps", PropRaw::Count(n)) => {
                        *lamps = n as u32
                    }
                    (
                        Some(ComponentKind::Circle {
                            rings, next_ring, ..
                        }),
                        "rings",
                        PropRaw::Count(n),
                    ) => set_ring_count(rings, next_ring, n as usize, spacing),
                    (Some(ComponentKind::Circle { rings, .. }), "ccw", PropRaw::Flag(v)) => {
                        rings.iter_mut().for_each(|r| r.reversed = v)
                    }
                    _ => return false,
                }
            }
            Target::Object(o) => {
                let Some(r) = self.ring_mut(&o.component, &o.key) else {
                    return false;
                };
                match (prop_id, new) {
                    ("lamps", PropRaw::Count(n)) => r.lamps = n as u32,
                    ("radius", PropRaw::Number(v)) => r.radius = v,
                    ("ccw", PropRaw::Flag(v)) => r.reversed = v,
                    _ => return false,
                }
            }
            Target::Lamp { .. } => return false,
        }
        true
    }

    pub fn ring(&self, circle: &crate::ComponentId, key: &str) -> Option<&Ring> {
        match &self.component(circle)?.kind {
            ComponentKind::Circle { rings, .. } => rings.iter().find(|r| r.key == key),
            _ => None,
        }
    }

    pub fn ring_mut(&mut self, circle: &crate::ComponentId, key: &str) -> Option<&mut Ring> {
        match &mut self.component_mut(circle)?.kind {
            ComponentKind::Circle { rings, .. } => rings.iter_mut().find(|r| r.key == key),
            _ => None,
        }
    }
}

/// Grow or shrink a circle's rings. Removing takes the newest ring; adding
/// puts a ring outside the largest one, keys minted by the circle.
fn set_ring_count(rings: &mut Vec<Ring>, next_ring: &mut u32, want: usize, spacing: f64) {
    rings.truncate(want);
    while rings.len() < want {
        let radius = rings.iter().map(|r| r.radius).fold(0.0, f64::max) + 8.0;
        let lamps = ((std::f64::consts::TAU * radius / spacing).round() as u32).max(3);
        rings.push(Ring {
            key: format!("r{next_ring}"),
            radius,
            lamps,
            reversed: false,
        });
        *next_ring += 1;
    }
}
