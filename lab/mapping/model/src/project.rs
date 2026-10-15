//! The project: the root module. Its `nodes` are what sits at the top of
//! the canvas — fixtures, outputs, modules holding more of them. It is the
//! one thing that is authored and kept; everything else (objects, the
//! layout, the patch, the edges) is derived from it.
//!
//! Ids are minted here, so a component id or a node id is unique across
//! the whole project — and only across the project: nothing assumes it is
//! the only one (D44).
//!
//! Positions this file hands out (`objects`, `object`) are in **world**
//! units: a fixture's own coordinates plus where the layout put it
//! (`layout.rs`). Edits go to components in the fixture's own coordinates;
//! a move is the same delta in both.

use crate::component::{Component, ComponentId, ComponentKind};
use crate::fixture::Fixture;
use crate::geom::{Rect, Vec2};
use crate::node::{self, Node, NodeId, NodeKind};
use crate::object::{Object, ObjectId, ObjectShape, objects_of};
use crate::output::{Output, PixelFormat};
use crate::patch::PatchRule;

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub name: String,
    /// The root module's children, in authoring order.
    pub nodes: Vec<Node>,
    /// Per kind, the next number to mint. Never decremented, so an id is
    /// never handed out twice.
    counters: Counters,
}

#[derive(Debug, Clone, PartialEq)]
struct Counters {
    path: u32,
    circle: u32,
    group: u32,
    module: u32,
    fixture: u32,
    output: u32,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: Vec::new(),
            counters: Counters {
                path: 1,
                circle: 1,
                group: 1,
                module: 1,
                fixture: 1,
                output: 1,
            },
        }
    }

    // ---- minting ---------------------------------------------------------

    /// Mint an id for a new component of this kind and wrap it.
    pub fn mint(&mut self, kind: ComponentKind) -> Component {
        let (word, counter) = match kind {
            ComponentKind::Path { .. } => ("path", &mut self.counters.path),
            ComponentKind::Circle { .. } => ("circle", &mut self.counters.circle),
            ComponentKind::Group { .. } => ("group", &mut self.counters.group),
        };
        let id = ComponentId::new(format!("{word}{counter}"));
        *counter += 1;
        Component { id, kind }
    }

    /// Mint a node id and put the node in `parent` (a module), or at the
    /// root when `parent` is `None`. Returns its id; `None` if `parent` is
    /// not a module.
    pub fn add_node(
        &mut self,
        parent: Option<&NodeId>,
        name: impl Into<String>,
        kind: NodeKind,
    ) -> Option<NodeId> {
        let (word, n) = match kind {
            NodeKind::Module { .. } => ("module", self.counters.module),
            NodeKind::Fixture(_) => ("fixture", self.counters.fixture),
            NodeKind::Output(_) => ("output", self.counters.output),
        };
        let id = NodeId::new(format!("{word}{n}"));
        let node = Node {
            id: id.clone(),
            name: name.into(),
            kind,
            pinned: None,
        };
        let list = match parent {
            None => &mut self.nodes,
            Some(p) => self.node_mut(p)?.children_mut()?,
        };
        list.push(node);
        match word {
            "module" => self.counters.module += 1,
            "fixture" => self.counters.fixture += 1,
            _ => self.counters.output += 1,
        }
        Some(id)
    }

    pub fn add_module(&mut self, parent: Option<&NodeId>, name: &str) -> Option<NodeId> {
        self.add_node(parent, name, NodeKind::Module { children: vec![] })
    }

    /// A new fixture is square ([`Fixture::DEFAULT_SIDE`] each way) until
    /// you say otherwise: drag its corner, or set its width and height.
    pub fn add_fixture(&mut self, parent: Option<&NodeId>, name: &str) -> Option<NodeId> {
        self.add_node(parent, name, NodeKind::Fixture(Fixture::square()))
    }

    /// A fixture of a given size, for when the shape is known up front.
    pub fn add_fixture_sized(
        &mut self,
        parent: Option<&NodeId>,
        name: &str,
        size: Vec2,
    ) -> Option<NodeId> {
        self.add_node(parent, name, NodeKind::Fixture(Fixture::new(size)))
    }

    pub fn add_output(
        &mut self,
        parent: Option<&NodeId>,
        name: &str,
        ports: &[(&str, u32, PixelFormat)],
    ) -> Option<NodeId> {
        self.add_node(parent, name, NodeKind::Output(Output::new(ports)))
    }

    /// Mint and append a top-level component to a fixture; returns its id.
    pub fn add(&mut self, fixture: &NodeId, kind: ComponentKind) -> Option<ComponentId> {
        let component = self.mint(kind);
        let id = component.id.clone();
        self.fixture_mut(fixture)?.components.push(component);
        Some(id)
    }

    /// Patch a fixture's lamps to an output with the auto rule.
    pub fn auto_patch(&mut self, fixture: &NodeId, output: &NodeId) {
        if let Some(f) = self.fixture_mut(fixture) {
            f.rules.push(PatchRule::Auto {
                output: output.clone(),
            });
        }
    }

    // ---- nodes -------------------------------------------------------------

    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        node::find(&self.nodes, id)
    }

    pub fn node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        node::find_mut(&mut self.nodes, id)
    }

    /// Every node, depth-first: a module before what's inside it.
    pub fn all_nodes(&self) -> Vec<&Node> {
        let mut out = Vec::new();
        node::walk(&self.nodes, &mut out);
        out
    }

    /// The module holding a node, or `None` at the root (or if absent).
    pub fn parent_module(&self, id: &NodeId) -> Option<&NodeId> {
        node::parent_of(&self.nodes, id, None).flatten()
    }

    /// The node's name, or its id if it's gone.
    pub fn node_name(&self, id: &NodeId) -> String {
        self.node(id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    pub fn remove_node(&mut self, id: &NodeId) -> Option<Node> {
        node::remove_from(&mut self.nodes, id)
    }

    pub fn fixture(&self, id: &NodeId) -> Option<&Fixture> {
        self.node(id)?.fixture()
    }

    pub fn fixture_mut(&mut self, id: &NodeId) -> Option<&mut Fixture> {
        self.node_mut(id)?.fixture_mut()
    }

    pub fn output(&self, id: &NodeId) -> Option<&Output> {
        self.node(id)?.output()
    }

    pub fn output_mut(&mut self, id: &NodeId) -> Option<&mut Output> {
        self.node_mut(id)?.output_mut()
    }

    /// Every fixture node, in tree order.
    pub fn fixtures(&self) -> Vec<&Node> {
        self.all_nodes()
            .into_iter()
            .filter(|n| n.fixture().is_some())
            .collect()
    }

    /// Every output node, in tree order.
    pub fn outputs(&self) -> Vec<&Node> {
        self.all_nodes()
            .into_iter()
            .filter(|n| n.output().is_some())
            .collect()
    }

    /// The first node of this name, in tree order.
    pub fn named(&self, name: &str) -> Option<&Node> {
        self.all_nodes().into_iter().find(|n| n.name == name)
    }

    // ---- components --------------------------------------------------------

    /// The fixture node a component lives in.
    pub fn fixture_of(&self, id: &ComponentId) -> Option<&Node> {
        self.fixtures()
            .into_iter()
            .find(|n| n.fixture().is_some_and(|f| f.component(id).is_some()))
    }

    pub fn component(&self, id: &ComponentId) -> Option<&Component> {
        self.fixtures()
            .into_iter()
            .find_map(|n| n.fixture()?.component(id))
    }

    pub fn component_mut(&mut self, id: &ComponentId) -> Option<&mut Component> {
        let fixture = self.fixture_of(id)?.id.clone();
        self.fixture_mut(&fixture)?.component_mut(id)
    }

    /// The group holding `id`, or `None` when `id` is top-level in its
    /// fixture (or absent).
    pub fn parent_group(&self, id: &ComponentId) -> Option<&ComponentId> {
        self.fixture_of(id)?.fixture()?.parent_group(id)
    }

    /// Remove a component (and everything under it) wherever it lives.
    pub fn remove(&mut self, id: &ComponentId) -> Option<Component> {
        let fixture = self.fixture_of(id)?.id.clone();
        self.fixture_mut(&fixture)?.remove(id)
    }

    // ---- derived: objects and lamps, in world units ----------------------

    /// Every object with the fixture it lives in, in tree then authoring
    /// order — the order the auto patch fills ports in.
    pub fn placed_objects(&self) -> Vec<(NodeId, Object)> {
        let mut out = Vec::new();
        for n in self.fixtures() {
            let Some(f) = n.fixture() else { continue };
            let origin = self.origin(&n.id);
            let mut objects = Vec::new();
            collect_objects(&f.components, &mut objects);
            out.extend(
                objects
                    .into_iter()
                    .map(|o| (n.id.clone(), placed(o, origin))),
            );
        }
        out
    }

    pub fn objects(&self) -> Vec<Object> {
        self.placed_objects().into_iter().map(|(_, o)| o).collect()
    }

    /// One fixture's objects, in authoring order, in world units.
    pub fn objects_in(&self, fixture: &NodeId) -> Vec<Object> {
        let Some(f) = self.fixture(fixture) else {
            return Vec::new();
        };
        let origin = self.origin(fixture);
        let mut objects = Vec::new();
        collect_objects(&f.components, &mut objects);
        objects.into_iter().map(|o| placed(o, origin)).collect()
    }

    pub fn object(&self, id: &ObjectId) -> Option<Object> {
        let n = self.fixture_of(&id.component)?;
        let origin = self.origin(&n.id);
        let component = n.fixture()?.component(&id.component)?;
        objects_of(component)
            .into_iter()
            .find(|o| &o.id == id)
            .map(|o| placed(o, origin))
    }

    /// Lamps outside their own fixture's box, as (object, lamp index).
    pub fn lamps_outside(&self) -> Vec<(ObjectId, u32)> {
        let mut out = Vec::new();
        for (f, object) in self.placed_objects() {
            let Some(bounds) = self.node_rect(&f) else {
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

    /// The fixture whose box holds a world point.
    pub fn fixture_at(&self, p: Vec2) -> Option<NodeId> {
        self.fixtures()
            .into_iter()
            .find(|n| self.node_rect(&n.id).is_some_and(|r| r.contains(p)))
            .map(|n| n.id.clone())
    }

    /// The lab's starting project: Yona's desk. A tail rig (the tail that
    /// runs out and back, and its own output), a two-ring circle display
    /// and a strip sharing the main output, and an empty fixture to draw a
    /// scarf into, already patched to its own output — so drawing in it
    /// lights pixels at once.
    ///
    /// Every fixture is square, as a new one is, sized to hold its drawing
    /// with room around it. The tail is two straight paths, out and back,
    /// the way the choker was built.
    pub fn seed() -> Self {
        let mut p = Project::new("desk");
        let grb = PixelFormat::Grb;
        let side = |s: f64| Vec2::new(s, s);

        let rig = p.add_module(None, "tail rig");
        let tail = p.add_fixture_sized(rig.as_ref(), "tail", side(82.0));
        let tail_out = p.add_output(rig.as_ref(), "tail out", &[("GPIO 10", 60, grb)]);
        let out = p.mint(ComponentKind::path(
            vec![Vec2::new(37.0, 70.0), Vec2::new(37.0, 12.0)],
            25,
        ));
        let back = p.mint(ComponentKind::path(
            vec![Vec2::new(45.0, 12.0), Vec2::new(45.0, 70.0)],
            25,
        ));

        let ring = p.add_fixture_sized(None, "ring display", side(70.0));
        let strip = p.add_fixture_sized(None, "strip", side(80.0));
        let main = p.add_output(None, "main", &[("D0", 64, grb), ("D1", 30, grb)]);
        let scarf = p.add_fixture(None, "scarf");
        let scarf_out = p.add_output(None, "scarf out", &[("D10", 60, grb)]);

        let (
            Some(tail),
            Some(tail_out),
            Some(ring),
            Some(strip),
            Some(main),
            Some(scarf),
            Some(scarf_out),
        ) = (tail, tail_out, ring, strip, main, scarf, scarf_out)
        else {
            unreachable!("the seed's parents are modules")
        };
        p.add(&tail, ComponentKind::group(vec![out, back]));
        p.add(
            &ring,
            ComponentKind::circle(Vec2::new(35.0, 35.0), &[(26.0, 40), (16.0, 24)]),
        );
        p.add(
            &strip,
            ComponentKind::path(vec![Vec2::new(6.0, 40.0), Vec2::new(74.0, 40.0)], 30),
        );
        p.auto_patch(&tail, &tail_out);
        p.auto_patch(&ring, &main);
        p.auto_patch(&strip, &main);
        p.auto_patch(&scarf, &scarf_out);
        p
    }
}

/// Every object under these components, in authoring order.
pub(crate) fn collect_objects(list: &[Component], out: &mut Vec<Object>) {
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
        ObjectShape::Polyline { points } => ObjectShape::Polyline {
            points: points.into_iter().map(|p| p + origin).collect(),
        },
        ObjectShape::Ring { center, radius } => ObjectShape::Ring {
            center: center + origin,
            radius,
        },
    };
    o
}

/// The rectangle around a set of rectangles.
pub(crate) fn union(rects: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    Rect::around(rects.into_iter().flat_map(|r| [r.min, r.max]))
}
