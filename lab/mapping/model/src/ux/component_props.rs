//! What each kind of thing says about its properties. This is the ONLY
//! place that knows a path has a lamp count, a ring has a radius and a
//! port has pixels; the inspector reads the descriptors (`props.rs`).
//!
//! Note the object-level controls (D11): a ring's lamps, radius and
//! direction are edited with the ring selected, but they are stored on the
//! circle.

use crate::domain::color_order::ColorOrder;
use crate::domain::component::{ComponentKind, Ring};
use crate::domain::fixture::Fixture;
use crate::domain::grid::Routing;
use crate::domain::project::Project;
use crate::domain::target::Target;
use crate::ux::props::{PropDesc, PropEdit, PropKind, PropRaw, edited};

pub const LAMPS: PropDesc = PropDesc {
    id: "lamps",
    label: "Lamps",
    kind: PropKind::Count { min: 1 },
    nudge: true,
    reshapes: true,
};
pub const RINGS: PropDesc = PropDesc {
    id: "rings",
    label: "Rings",
    kind: PropKind::Count { min: 1 },
    nudge: true,
    reshapes: true,
};
/// A repeat's copies. Not on −/=, which change its lamps.
pub const COPIES: PropDesc = PropDesc {
    id: "copies",
    label: "Copies",
    kind: PropKind::Count { min: 1 },
    nudge: false,
    reshapes: true,
};
pub const RADIUS: PropDesc = PropDesc {
    id: "radius",
    label: "Radius",
    kind: PropKind::Number {
        min: 0.5,
        step: 0.5,
    },
    nudge: false,
    reshapes: true,
};
pub const WIDTH: PropDesc = PropDesc {
    id: "width",
    label: "Width",
    kind: PropKind::Number {
        min: Fixture::MIN_SIDE,
        step: 1.0,
    },
    nudge: false,
    reshapes: true,
};
pub const HEIGHT: PropDesc = PropDesc {
    id: "height",
    label: "Height",
    kind: PropKind::Number {
        min: Fixture::MIN_SIDE,
        step: 1.0,
    },
    nudge: false,
    reshapes: true,
};
pub const PIXELS: PropDesc = PropDesc {
    id: "pixels",
    label: "Pixels",
    kind: PropKind::Count { min: 1 },
    nudge: true,
    reshapes: false,
};
pub const COUNTER_CLOCKWISE: PropDesc = PropDesc {
    id: "ccw",
    label: "Counter-clockwise",
    kind: PropKind::Flag,
    nudge: false,
    reshapes: false,
};

/// Which byte of each pixel is red, green and blue: the product's six
/// orders, in `ColorOrder::ALL`'s order. Most WS2811 strings are RGB or
/// BRG; the way to find one's is to pick and look at the house.
pub const COLOUR_ORDER: PropDesc = PropDesc {
    id: "colour-order",
    label: "Colour order",
    kind: PropKind::Choice {
        options: &["RGB", "GRB", "RBG", "GBR", "BRG", "BGR"],
    },
    nudge: false,
    reshapes: false,
};
pub const COLUMNS: PropDesc = PropDesc {
    id: "columns",
    label: "Columns",
    kind: PropKind::Count { min: 1 },
    nudge: false,
    reshapes: true,
};
pub const ROWS: PropDesc = PropDesc {
    id: "rows",
    label: "Rows",
    kind: PropKind::Count { min: 1 },
    nudge: false,
    reshapes: true,
};
/// Lamp to lamp, in a grid or a filled polygon.
pub const PITCH: PropDesc = PropDesc {
    id: "pitch",
    label: "Spacing",
    kind: PropKind::Number {
        min: 0.1,
        step: 0.5,
    },
    nudge: false,
    reshapes: true,
};
/// A lattice's rows run back and forth (on) or all the same way (off).
pub const SNAKE: PropDesc = PropDesc {
    id: "snake",
    label: "Snake",
    kind: PropKind::Flag,
    nudge: false,
    reshapes: false,
};
/// A ring set wires its inner ring first.
pub const INNER_FIRST: PropDesc = PropDesc {
    id: "inner-first",
    label: "Inner ring first",
    kind: PropKind::Flag,
    nudge: false,
    reshapes: false,
};
/// A filled polygon's lattice, turned inside its outline.
pub const ANGLE: PropDesc = PropDesc {
    id: "angle",
    label: "Angle",
    kind: PropKind::Number {
        min: -360.0,
        step: 5.0,
    },
    nudge: false,
    reshapes: true,
};

impl Project {
    /// The properties of one target, with its values, in display order.
    pub fn props_of(&self, t: &Target) -> Vec<(PropDesc, PropRaw)> {
        match t {
            // A module and an output have none yet (a name, later).
            // A fixture the lab can't hold yet keeps its size.
            Target::Node(id) => match self.fixture(id) {
                Some(f) if f.unsupported.is_none() => vec![
                    (WIDTH, PropRaw::Number(f.size.x)),
                    (HEIGHT, PropRaw::Number(f.size.y)),
                    (
                        COLOUR_ORDER,
                        PropRaw::Choice(
                            ColorOrder::ALL
                                .iter()
                                .position(|o| *o == f.color_order)
                                .unwrap_or(0),
                        ),
                    ),
                ],
                _ => Vec::new(),
            },
            Target::Port { output, key } => match self.output(output).and_then(|o| o.port(key)) {
                Some(_) => vec![(PIXELS, PropRaw::Count(self.port_pixels(output, key) as i64))],
                None => Vec::new(),
            },
            Target::Component(id) => match self.component(id).map(|c| &c.kind) {
                Some(ComponentKind::Path { lamps, .. } | ComponentKind::Polygon { lamps, .. }) => {
                    vec![(LAMPS, PropRaw::Count(*lamps as i64))]
                }
                Some(ComponentKind::Repeat { shape, count, .. }) => {
                    let mut props = shape_lamps(shape)
                        .map(|n| vec![(LAMPS, PropRaw::Count(n as i64))])
                        .unwrap_or_default();
                    props.push((COPIES, PropRaw::Count(*count as i64)));
                    props
                }
                Some(ComponentKind::Circle { rings, .. }) => vec![
                    (RINGS, PropRaw::Count(rings.len() as i64)),
                    (
                        COUNTER_CLOCKWISE,
                        PropRaw::Flag(!rings.is_empty() && rings.iter().all(|r| r.reversed)),
                    ),
                ],
                Some(ComponentKind::Grid(g)) => vec![
                    (COLUMNS, PropRaw::Count(g.cols as i64)),
                    (ROWS, PropRaw::Count(g.rows as i64)),
                    (PITCH, PropRaw::Number(g.pitch)),
                    (SNAKE, PropRaw::Flag(g.routing == Routing::Snake)),
                ],
                // The outer ring's lamps (its own count when it has one);
                // an inner ring without its own follows by circumference,
                // as in the product.
                Some(ComponentKind::RingSet(r)) => vec![
                    (LAMPS, PropRaw::Count(r.outer_ring_lamps() as i64)),
                    (RINGS, PropRaw::Count(r.rings as i64)),
                    (RADIUS, PropRaw::Number(r.radius)),
                    (COUNTER_CLOCKWISE, PropRaw::Flag(r.reversed)),
                    (INNER_FIRST, PropRaw::Flag(r.inner_first)),
                ],
                // Its lamp count is however many cells the outline holds.
                Some(ComponentKind::Filled(f)) => vec![
                    (PITCH, PropRaw::Number(f.pitch)),
                    (ANGLE, PropRaw::Number(f.angle_deg)),
                    (SNAKE, PropRaw::Flag(f.routing == Routing::Snake)),
                ],
                _ => Vec::new(),
            },
            Target::Object(o) => match self.ring(&o.component, &o.key) {
                Some(r) => vec![
                    (LAMPS, PropRaw::Count(r.lamps as i64)),
                    (RADIUS, PropRaw::Number(r.radius)),
                    (COUNTER_CLOCKWISE, PropRaw::Flag(r.reversed)),
                ],
                // A copy's lamps are its repeat's shape's (D11).
                None => match self.component(&o.component).map(|c| &c.kind) {
                    Some(ComponentKind::Repeat { shape, .. }) => shape_lamps(shape)
                        .map(|n| vec![(LAMPS, PropRaw::Count(n as i64))])
                        .unwrap_or_default(),
                    _ => Vec::new(),
                },
            },
            Target::Lamp { .. } | Target::Pixel { .. } => Vec::new(),
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
            Target::Port { output, key } => {
                let Some(p) = self.output_mut(output).and_then(|o| o.port_mut(key)) else {
                    return false;
                };
                match (prop_id, new) {
                    ("pixels", PropRaw::Count(n)) => p.pixels = Some(n as u32),
                    _ => return false,
                }
            }
            Target::Node(id) => {
                let Some(f) = self.fixture_mut(id) else {
                    return false;
                };
                match (prop_id, new) {
                    ("width", PropRaw::Number(v)) => f.size.x = v,
                    ("height", PropRaw::Number(v)) => f.size.y = v,
                    ("colour-order", PropRaw::Choice(i)) => {
                        f.color_order = ColorOrder::ALL.get(i).copied().unwrap_or_default()
                    }
                    _ => return false,
                }
            }
            Target::Component(id) => {
                match (self.component_mut(id).map(|c| &mut c.kind), prop_id, new) {
                    (
                        Some(
                            ComponentKind::Path { lamps, .. }
                            | ComponentKind::Polygon { lamps, .. },
                        ),
                        "lamps",
                        PropRaw::Count(n),
                    ) => *lamps = n as u32,
                    (Some(ComponentKind::Repeat { shape, .. }), "lamps", PropRaw::Count(n)) => {
                        set_shape_lamps(shape, n as u32)
                    }
                    (Some(ComponentKind::Repeat { count, .. }), "copies", PropRaw::Count(n)) => {
                        *count = n as u32
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
                    (Some(ComponentKind::Grid(g)), _, new) => match (prop_id, new) {
                        ("columns", PropRaw::Count(n)) => g.cols = n as u32,
                        ("rows", PropRaw::Count(n)) => g.rows = n as u32,
                        ("pitch", PropRaw::Number(v)) => g.pitch = v,
                        ("snake", PropRaw::Flag(v)) => g.routing = routing(v),
                        _ => return false,
                    },
                    (Some(ComponentKind::RingSet(r)), _, new) => match (prop_id, new) {
                        ("lamps", PropRaw::Count(n)) => r.set_outer_ring_lamps(n as u32),
                        ("rings", PropRaw::Count(n)) => r.rings = n as u32,
                        ("radius", PropRaw::Number(v)) => r.radius = v,
                        ("ccw", PropRaw::Flag(v)) => r.reversed = v,
                        ("inner-first", PropRaw::Flag(v)) => r.inner_first = v,
                        _ => return false,
                    },
                    (Some(ComponentKind::Filled(f)), _, new) => match (prop_id, new) {
                        ("pitch", PropRaw::Number(v)) => f.pitch = v,
                        ("angle", PropRaw::Number(v)) => f.angle_deg = v,
                        ("snake", PropRaw::Flag(v)) => f.routing = routing(v),
                        _ => return false,
                    },
                    _ => return false,
                }
            }
            Target::Object(o) => {
                if self.ring(&o.component, &o.key).is_none() {
                    return match (
                        self.component_mut(&o.component).map(|c| &mut c.kind),
                        prop_id,
                        new,
                    ) {
                        (Some(ComponentKind::Repeat { shape, .. }), "lamps", PropRaw::Count(n)) => {
                            set_shape_lamps(shape, n as u32);
                            true
                        }
                        _ => false,
                    };
                }
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
            Target::Lamp { .. } | Target::Pixel { .. } => return false,
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

fn routing(snake: bool) -> Routing {
    if snake {
        Routing::Snake
    } else {
        Routing::Raster
    }
}

/// The lamps of a repeat's shape, when it has one count (a path, a
/// polygon, a repeat of one).
fn shape_lamps(kind: &ComponentKind) -> Option<u32> {
    match kind {
        ComponentKind::Path { lamps, .. } | ComponentKind::Polygon { lamps, .. } => Some(*lamps),
        ComponentKind::Repeat { shape, .. } => shape_lamps(shape),
        _ => None,
    }
}

fn set_shape_lamps(kind: &mut ComponentKind, n: u32) {
    match kind {
        ComponentKind::Path { lamps, .. } | ComponentKind::Polygon { lamps, .. } => *lamps = n,
        ComponentKind::Repeat { shape, .. } => set_shape_lamps(shape, n),
        _ => {}
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
