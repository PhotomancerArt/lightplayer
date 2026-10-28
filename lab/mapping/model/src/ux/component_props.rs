//! What each kind of thing says about its properties. This is the ONLY
//! place that knows a path has a lamp count, a ring has a radius and a
//! port has pixels; the inspector reads the descriptors (`props.rs`).
//!
//! Note the segment-level controls (D11): a ring's lamps and radius are
//! edited with the ring selected, but they are stored on the circle. Which
//! way round the rings run is the circle's (every ring's); one ring runs
//! the other way on the wire by its own override (`r`).

use lpc_mapping::part::{Circle, PartKind, Shape};
use lpc_mapping::{GridRouting as Routing, RingDir, RingOrder};

use crate::domain::color_order::ColorOrder;
use crate::domain::fixture::Fixture;
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
/// A circle wires its inner ring first ("drawn from" the inside).
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
            Target::Part(id) => match self.part(id).map(|c| &c.kind) {
                Some(PartKind::Shape(Shape::Path(path))) => {
                    vec![(LAMPS, PropRaw::Count(path.count as i64))]
                }
                Some(PartKind::Shape(Shape::Polygon(polygon))) => {
                    vec![(LAMPS, PropRaw::Count(polygon.count as i64))]
                }
                Some(PartKind::Repeat(repeat)) => {
                    let mut props = shape_lamps(&repeat.part.kind)
                        .map(|n| vec![(LAMPS, PropRaw::Count(n as i64))])
                        .unwrap_or_default();
                    props.push((COPIES, PropRaw::Count(repeat.count as i64)));
                    props
                }
                Some(PartKind::Shape(Shape::Circle(circle))) => vec![
                    (RINGS, PropRaw::Count(circle.rings.len() as i64)),
                    (COUNTER_CLOCKWISE, PropRaw::Flag(circle.dir == RingDir::Ccw)),
                    (
                        INNER_FIRST,
                        PropRaw::Flag(circle.drawn_from == RingOrder::InnerFirst),
                    ),
                ],
                Some(PartKind::Shape(Shape::Grid(g))) => vec![
                    (COLUMNS, PropRaw::Count(g.cols as i64)),
                    (ROWS, PropRaw::Count(g.rows as i64)),
                    (PITCH, PropRaw::Number(f64::from(g.pitch))),
                    (SNAKE, PropRaw::Flag(g.routing == Routing::Snake)),
                ],
                // Its lamp count is however many cells the outline holds.
                Some(PartKind::Shape(Shape::FilledPolygon(f))) => vec![
                    (PITCH, PropRaw::Number(f64::from(f.pitch))),
                    (ANGLE, PropRaw::Number(f64::from(f.angle_deg))),
                    (SNAKE, PropRaw::Flag(f.routing == Routing::Snake)),
                ],
                _ => Vec::new(),
            },
            Target::Generated(o) => match self.ring_of(o) {
                Some(r) => vec![
                    (LAMPS, PropRaw::Count(r.count as i64)),
                    (RADIUS, PropRaw::Number(f64::from(r.radius))),
                ],
                // A copy's properties are its part's (D11): an edit to one
                // copy is an edit to all of them.
                None if o.id.key.is_none() => self.props_of(&Target::Part(o.part())),
                None => Vec::new(),
            },
            Target::Lamp { .. } | Target::Span { .. } | Target::Pixel { .. } => Vec::new(),
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
            Target::Part(id) => {
                if prop_id == "copies"
                    && let PropRaw::Count(n) = new
                    && matches!(self.part(id).map(|p| &p.kind), Some(PartKind::Repeat(_)))
                {
                    let Some(f) = self.fixture_mut(&id.fixture) else {
                        return false;
                    };
                    return lpc_mapping::edit::set_repeat_count(&mut f.mapping, &id.part, n as u32)
                        .is_ok();
                }
                match (self.part_mut(id).map(|c| &mut c.kind), prop_id, new) {
                    (Some(PartKind::Shape(Shape::Path(path))), "lamps", PropRaw::Count(n)) => {
                        path.count = n as u32
                    }
                    (
                        Some(PartKind::Shape(Shape::Polygon(polygon))),
                        "lamps",
                        PropRaw::Count(n),
                    ) => polygon.count = n as u32,
                    (Some(PartKind::Repeat(repeat)), "lamps", PropRaw::Count(n)) => {
                        set_shape_lamps(&mut repeat.part.kind, n as u32)
                    }
                    (Some(PartKind::Shape(Shape::Circle(circle))), _, new) => {
                        match (prop_id, new) {
                            ("rings", PropRaw::Count(n)) => {
                                set_ring_count(circle, n as usize, spacing)
                            }
                            ("ccw", PropRaw::Flag(v)) => {
                                circle.dir = if v { RingDir::Ccw } else { RingDir::Cw }
                            }
                            ("inner-first", PropRaw::Flag(v)) => {
                                circle.drawn_from = if v {
                                    RingOrder::InnerFirst
                                } else {
                                    RingOrder::OuterFirst
                                }
                            }
                            _ => return false,
                        }
                    }
                    (Some(PartKind::Shape(Shape::Grid(g))), _, new) => match (prop_id, new) {
                        ("columns", PropRaw::Count(n)) => g.cols = n as u32,
                        ("rows", PropRaw::Count(n)) => g.rows = n as u32,
                        ("pitch", PropRaw::Number(v)) => g.pitch = v as f32,
                        ("snake", PropRaw::Flag(v)) => g.routing = routing(v),
                        _ => return false,
                    },
                    (Some(PartKind::Shape(Shape::FilledPolygon(f))), _, new) => {
                        match (prop_id, new) {
                            ("pitch", PropRaw::Number(v)) => f.pitch = v as f32,
                            ("angle", PropRaw::Number(v)) => f.angle_deg = v as f32,
                            ("snake", PropRaw::Flag(v)) => f.routing = routing(v),
                            _ => return false,
                        }
                    }
                    _ => return false,
                }
            }
            Target::Generated(o) => {
                if self.ring_of(o).is_none() {
                    // A copy's edits land on its part (D11).
                    return o.id.key.is_none()
                        && self.edit_prop(&Target::Part(o.part()), prop_id, edit, spacing);
                }
                let Some(key) = o.id.key.clone() else {
                    return false;
                };
                let Some(r) = self.ring_mut(&o.part(), key.as_str()) else {
                    return false;
                };
                match (prop_id, new) {
                    ("lamps", PropRaw::Count(n)) => r.count = n as u32,
                    ("radius", PropRaw::Number(v)) => r.radius = v as f32,
                    _ => return false,
                }
            }
            Target::Lamp { .. } | Target::Span { .. } | Target::Pixel { .. } => return false,
        }
        true
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
fn shape_lamps(kind: &PartKind) -> Option<u32> {
    match kind {
        PartKind::Shape(Shape::Path(path)) => Some(path.count),
        PartKind::Shape(Shape::Polygon(polygon)) => Some(polygon.count),
        PartKind::Repeat(repeat) => shape_lamps(&repeat.part.kind),
        _ => None,
    }
}

fn set_shape_lamps(kind: &mut PartKind, n: u32) {
    match kind {
        PartKind::Shape(Shape::Path(path)) => path.count = n,
        PartKind::Shape(Shape::Polygon(polygon)) => polygon.count = n,
        PartKind::Repeat(repeat) => set_shape_lamps(&mut repeat.part.kind, n),
        _ => {}
    }
}

/// Grow or shrink a circle's rings. Removing takes the innermost ring (the
/// last listed); adding puts a ring outside the largest one, first in the
/// list (rings are listed outside in), its key minted by the circle and
/// never one it had.
fn set_ring_count(circle: &mut Circle, want: usize, spacing: f64) {
    circle.rings.truncate(want);
    while circle.rings.len() < want {
        let radius = circle
            .rings
            .iter()
            .map(|r| f64::from(r.radius))
            .fold(0.0, f64::max)
            + 8.0;
        let lamps = ((std::f64::consts::TAU * radius / spacing).round() as u32).max(3);
        circle.add_ring(radius as f32, lamps);
        circle.rings.rotate_right(1);
    }
}
