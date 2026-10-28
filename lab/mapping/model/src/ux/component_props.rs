//! What each kind of thing says about its properties. This is the ONLY
//! place that knows a path has a lamp count, a ring has a radius and a
//! port has pixels; the inspector reads the descriptors (`props.rs`).
//!
//! Note the segment-level controls (D11): a ring's lamps and radius are
//! edited with the ring selected, but they are stored on the circle. Which
//! way round the rings run is the circle's (every ring's); one ring runs
//! the other way on the wire by its own override (`r`).
//!
//! **Rows that are verbs** (mapping rebuild M4 P07). Lamps, Copies, Rings
//! and Inner ring first press the layout verbs (`set-count`, `add-ring`,
//! `remove-ring`, `set-drawn-from`) through [`Project::prop_edits`] and
//! `Editor::edit`, the function their offers run; the other rows (a
//! radius, a grid's columns, a fixture's size…) still set their field.

use lpc_mapping::part::{Circle, PartKind, Shape};
use lpc_mapping::{GridRouting as Routing, RingDir, RingOrder};

use crate::domain::color_order::ColorOrder;
use crate::domain::fixture::Fixture;
use crate::domain::project::Project;
use crate::domain::target::Target;
use crate::ux::graph_edit::GraphEdit;
use crate::ux::graph_layout_edit::Units;
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

    /// The verbs a row that is one presses for `targets` (module docs):
    /// `None` for a row that sets its field. `spacing` is the lamp spacing
    /// new rings are drawn with.
    pub(crate) fn prop_edits(
        &self,
        targets: &[Target],
        prop_id: &str,
        edit: PropEdit,
        spacing: f64,
    ) -> Option<Vec<GraphEdit>> {
        if !matches!(prop_id, "lamps" | "copies" | "rings" | "inner-first") {
            return None;
        }
        let mut out = Vec::new();
        for t in targets {
            let Some((desc, current)) = self.props_of(t).into_iter().find(|(d, _)| d.id == prop_id)
            else {
                continue;
            };
            let new = edited(&desc, current, edit);
            let part = match t {
                Target::Part(id) => Some(id.clone()),
                Target::Generated(o) if o.id.key.is_none() => Some(o.part()),
                _ => None,
            };
            match (t, prop_id, new) {
                (Target::Generated(o), "lamps", PropRaw::Count(n)) if o.id.key.is_some() => out
                    .push(GraphEdit::SetCount {
                        subject: o.clone(),
                        count: n as u32,
                    }),
                (_, "lamps", PropRaw::Count(n)) => {
                    let part = part?;
                    // A repeat's lamps are the part it holds.
                    let subject = match self.part(&part).map(|p| &p.kind) {
                        Some(PartKind::Repeat(r)) => {
                            crate::PartRef::new(&part.fixture, &r.part.id).node()
                        }
                        _ => part.node(),
                    };
                    out.push(GraphEdit::SetCount {
                        subject,
                        count: n as u32,
                    });
                }
                (_, "copies", PropRaw::Count(n)) => out.push(GraphEdit::SetCount {
                    subject: part?.node(),
                    count: n as u32,
                }),
                (_, "inner-first", PropRaw::Flag(inside)) => out.push(GraphEdit::SetDrawnFrom {
                    part: part?,
                    from: if inside {
                        lpc_mapping::edit::DrawnFrom::Inside
                    } else {
                        lpc_mapping::edit::DrawnFrom::Outside
                    },
                }),
                (_, "rings", PropRaw::Count(n)) => {
                    let part = part?;
                    let circle = self.circle(&part)?;
                    out.extend(ring_count_edits(&part, circle, n.max(1) as usize, spacing));
                }
                _ => {}
            }
        }
        Some(out)
    }

    /// Apply an edit to one target's property, for a row that sets its
    /// field ([`Project::prop_edits`] says which rows are verbs). Returns
    /// whether it had it.
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
            Target::Part(id) => match (self.part_mut(id).map(|c| &mut c.kind), prop_id, new) {
                (Some(PartKind::Shape(Shape::Circle(circle))), _, new) => match (prop_id, new) {
                    ("ccw", PropRaw::Flag(v)) => {
                        circle.dir = if v { RingDir::Ccw } else { RingDir::Cw }
                    }
                    _ => return false,
                },
                (Some(PartKind::Shape(Shape::Grid(g))), _, new) => match (prop_id, new) {
                    ("columns", PropRaw::Count(n)) => g.cols = n as u32,
                    ("rows", PropRaw::Count(n)) => g.rows = n as u32,
                    ("pitch", PropRaw::Number(v)) => g.pitch = v as f32,
                    ("snake", PropRaw::Flag(v)) => g.routing = routing(v),
                    _ => return false,
                },
                (Some(PartKind::Shape(Shape::FilledPolygon(f))), _, new) => match (prop_id, new) {
                    ("pitch", PropRaw::Number(v)) => f.pitch = v as f32,
                    ("angle", PropRaw::Number(v)) => f.angle_deg = v as f32,
                    ("snake", PropRaw::Flag(v)) => f.routing = routing(v),
                    _ => return false,
                },
                _ => return false,
            },
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

/// The verbs that grow or shrink a circle's rings to `want`. Removing
/// takes the innermost ring (the last listed: `remove-ring`); adding puts a
/// ring outside the largest one (`add-ring`, which lists it first), its
/// key minted by the circle and never one it had.
fn ring_count_edits(
    part: &crate::PartRef,
    circle: &Circle,
    want: usize,
    spacing: f64,
) -> Vec<GraphEdit> {
    let have = circle.rings.len();
    if want < have {
        return circle.rings[want..]
            .iter()
            .rev()
            .map(|r| GraphEdit::RemoveRing {
                ring: crate::TreeRef::new(
                    &part.fixture,
                    lpc_mapping::part::TreeId::generated(&part.part, r.key.clone()),
                ),
            })
            .collect();
    }
    let mut largest = circle
        .rings
        .iter()
        .map(|r| f64::from(r.radius))
        .fold(0.0, f64::max);
    (have..want)
        .map(|_| {
            largest += 8.0;
            let lamps = ((std::f64::consts::TAU * largest / spacing).round() as u32).max(3);
            GraphEdit::AddRing {
                circle: part.clone(),
                count: lamps,
                radius: Units(largest as f32),
            }
        })
        .collect()
}
