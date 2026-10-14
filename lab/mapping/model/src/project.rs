//! The project: every fixture, laid out on one canvas in a grid. The one
//! thing that is authored and kept; everything else is derived from it.
//!
//! Each fixture keeps its own box and coordinates. The canvas places them in
//! a grid, in order, so a project reads like a board of nodes. Ids are
//! minted here, so a component id is unique across the whole project.
//!
//! Positions this file hands out (`objects`, `object`, `world_bounds`) are
//! in **world** units: the fixture's own coordinates plus where the grid
//! put it. Edits go to components in the fixture's own coordinates; a move
//! is the same delta in both.

use crate::component::{Component, ComponentId, ComponentKind};
use crate::fixture::{Fixture, FixtureId};
use crate::geom::{Rect, Vec2};
use crate::object::{Object, ObjectId, ObjectShape, objects_of};

/// Space between fixtures in the grid, in world units.
pub const GRID_GAP: f64 = 24.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub name: String,
    /// In grid order: left to right, then down.
    pub fixtures: Vec<Fixture>,
    /// Per kind, the next number to mint. Never decremented, so an id is
    /// never handed out twice.
    next_line: u32,
    next_circle: u32,
    next_group: u32,
    next_fixture: u32,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fixtures: Vec::new(),
            next_line: 1,
            next_circle: 1,
            next_group: 1,
            next_fixture: 1,
        }
    }

    // ---- minting ---------------------------------------------------------

    /// Mint an id for a new component of this kind and wrap it.
    pub fn mint(&mut self, kind: ComponentKind) -> Component {
        let (word, counter) = match kind {
            ComponentKind::Line { .. } => ("line", &mut self.next_line),
            ComponentKind::Circle { .. } => ("circle", &mut self.next_circle),
            ComponentKind::Group { .. } => ("group", &mut self.next_group),
        };
        let id = ComponentId::new(format!("{word}{counter}"));
        *counter += 1;
        Component { id, kind }
    }

    pub fn add_fixture(&mut self, name: impl Into<String>, size: Vec2) -> FixtureId {
        let id = FixtureId::new(format!("fixture{}", self.next_fixture));
        self.next_fixture += 1;
        self.fixtures.push(Fixture {
            id: id.clone(),
            name: name.into(),
            size,
            components: Vec::new(),
        });
        id
    }

    /// Mint and append a top-level component to a fixture; returns its id.
    pub fn add(&mut self, fixture: &FixtureId, kind: ComponentKind) -> Option<ComponentId> {
        let component = self.mint(kind);
        let id = component.id.clone();
        self.fixture_mut(fixture)?.components.push(component);
        Some(id)
    }

    // ---- lookups ---------------------------------------------------------

    pub fn fixture(&self, id: &FixtureId) -> Option<&Fixture> {
        self.fixtures.iter().find(|f| &f.id == id)
    }

    pub fn fixture_mut(&mut self, id: &FixtureId) -> Option<&mut Fixture> {
        self.fixtures.iter_mut().find(|f| &f.id == id)
    }

    /// The fixture a component lives in.
    pub fn fixture_of(&self, id: &ComponentId) -> Option<&Fixture> {
        self.fixtures.iter().find(|f| f.component(id).is_some())
    }

    pub fn component(&self, id: &ComponentId) -> Option<&Component> {
        self.fixtures.iter().find_map(|f| f.component(id))
    }

    pub fn component_mut(&mut self, id: &ComponentId) -> Option<&mut Component> {
        self.fixtures.iter_mut().find_map(|f| f.component_mut(id))
    }

    /// The group holding `id`, or `None` when `id` is top-level in its
    /// fixture (or absent).
    pub fn parent_group(&self, id: &ComponentId) -> Option<&ComponentId> {
        self.fixture_of(id)?.parent_group(id)
    }

    /// Remove a component (and everything under it) wherever it lives.
    pub fn remove(&mut self, id: &ComponentId) -> Option<Component> {
        self.fixtures.iter_mut().find_map(|f| f.remove(id))
    }

    pub fn remove_fixture(&mut self, id: &FixtureId) -> Option<Fixture> {
        let i = self.fixtures.iter().position(|f| &f.id == id)?;
        Some(self.fixtures.remove(i))
    }

    // ---- the grid --------------------------------------------------------

    /// Where each fixture's `(0, 0)` lands in world units. Columns are
    /// ⌈√n⌉ wide; each column as wide as its widest fixture, each row as
    /// tall as its tallest.
    pub fn origins(&self) -> Vec<Vec2> {
        let n = self.fixtures.len();
        if n == 0 {
            return Vec::new();
        }
        let cols = (n as f64).sqrt().ceil() as usize;
        let rows = n.div_ceil(cols);
        let mut col_w = vec![0.0f64; cols];
        let mut row_h = vec![0.0f64; rows];
        for (i, f) in self.fixtures.iter().enumerate() {
            col_w[i % cols] = col_w[i % cols].max(f.size.x);
            row_h[i / cols] = row_h[i / cols].max(f.size.y);
        }
        let col_x: Vec<f64> = col_w
            .iter()
            .scan(0.0, |x, w| {
                let at = *x;
                *x += w + GRID_GAP;
                Some(at)
            })
            .collect();
        let row_y: Vec<f64> = row_h
            .iter()
            .scan(0.0, |y, h| {
                let at = *y;
                *y += h + GRID_GAP;
                Some(at)
            })
            .collect();
        (0..n)
            .map(|i| Vec2::new(col_x[i % cols], row_y[i / cols]))
            .collect()
    }

    pub fn origin(&self, id: &FixtureId) -> Vec2 {
        let i = self.fixtures.iter().position(|f| &f.id == id);
        i.and_then(|i| self.origins().get(i).copied())
            .unwrap_or_default()
    }

    /// A fixture's box in world units.
    pub fn world_bounds(&self, id: &FixtureId) -> Option<Rect> {
        let f = self.fixture(id)?;
        let o = self.origin(id);
        Some(Rect::new(o, o + f.size))
    }

    /// Everything on the canvas, for "fit".
    pub fn extent(&self) -> Rect {
        let rects: Vec<Rect> = self
            .fixtures
            .iter()
            .filter_map(|f| self.world_bounds(&f.id))
            .collect();
        Rect::around(rects.iter().flat_map(|r| [r.min, r.max]))
            .unwrap_or(Rect::new(Vec2::default(), Vec2::new(100.0, 60.0)))
    }

    /// The fixture whose box holds a world point.
    pub fn fixture_at(&self, p: Vec2) -> Option<FixtureId> {
        self.fixtures
            .iter()
            .find(|f| self.world_bounds(&f.id).is_some_and(|r| r.contains(p)))
            .map(|f| f.id.clone())
    }

    // ---- derived: objects and lamps, in world units ----------------------

    /// Every object with the fixture it lives in, in grid then authoring
    /// order.
    pub fn placed_objects(&self) -> Vec<(FixtureId, Object)> {
        let origins = self.origins();
        let mut out = Vec::new();
        for (f, origin) in self.fixtures.iter().zip(origins) {
            let mut objects = Vec::new();
            collect_objects(&f.components, &mut objects);
            out.extend(
                objects
                    .into_iter()
                    .map(|o| (f.id.clone(), placed(o, origin))),
            );
        }
        out
    }

    pub fn objects(&self) -> Vec<Object> {
        self.placed_objects().into_iter().map(|(_, o)| o).collect()
    }

    pub fn object(&self, id: &ObjectId) -> Option<Object> {
        let f = self.fixture_of(&id.component)?;
        let origin = self.origin(&f.id);
        let component = f.component(&id.component)?;
        objects_of(component)
            .into_iter()
            .find(|o| &o.id == id)
            .map(|o| placed(o, origin))
    }

    /// Lamps outside their own fixture's box, as (object, lamp index).
    pub fn lamps_outside(&self) -> Vec<(ObjectId, u32)> {
        let mut out = Vec::new();
        for (f, object) in self.placed_objects() {
            let Some(bounds) = self.world_bounds(&f) else {
                continue;
            };
            for (i, lamp) in object.lamps.iter().enumerate() {
                if !bounds.contains(lamp.pos) {
                    out.push((object.id.clone(), i as u32));
                }
            }
        }
        out
    }

    pub fn lamp_count(&self) -> usize {
        self.objects().iter().map(|o| o.lamps.len()).sum()
    }

    /// The lab's starting project: Yona's desk. A tail that runs out and
    /// back, a two-ring circle display, a strip, and an empty fixture to
    /// draw a scarf into.
    pub fn seed() -> Self {
        let mut p = Project::new("desk");
        let tail = p.add_fixture("tail", Vec2::new(36.0, 82.0));
        let out = p.mint(ComponentKind::line(
            Vec2::new(14.0, 70.0),
            Vec2::new(14.0, 12.0),
            25,
        ));
        let back = p.mint(ComponentKind::line(
            Vec2::new(22.0, 12.0),
            Vec2::new(22.0, 70.0),
            25,
        ));
        p.add(&tail, ComponentKind::group(vec![out, back]));
        let ring = p.add_fixture("ring display", Vec2::new(70.0, 70.0));
        p.add(
            &ring,
            ComponentKind::circle(Vec2::new(35.0, 35.0), &[(26.0, 40), (16.0, 24)]),
        );
        let strip = p.add_fixture("strip", Vec2::new(80.0, 20.0));
        p.add(
            &strip,
            ComponentKind::line(Vec2::new(6.0, 10.0), Vec2::new(74.0, 10.0), 30),
        );
        p.add_fixture("scarf", Vec2::new(100.0, 30.0));
        p
    }
}

fn collect_objects(list: &[Component], out: &mut Vec<Object>) {
    for c in list {
        out.extend(objects_of(c));
        collect_objects(c.children(), out);
    }
}

/// An object moved from its fixture's coordinates to world units.
fn placed(mut o: Object, origin: Vec2) -> Object {
    for l in &mut o.lamps {
        l.pos = l.pos + origin;
    }
    o.shape = match o.shape {
        ObjectShape::Segment { from, to } => ObjectShape::Segment {
            from: from + origin,
            to: to + origin,
        },
        ObjectShape::Ring { center, radius } => ObjectShape::Ring {
            center: center + origin,
            radius,
        },
    };
    o
}
