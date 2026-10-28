//! The project: the root module. Its `nodes` are what sits at the top of
//! the canvas — fixtures, outputs, modules holding more of them. It is the
//! one thing that is authored and kept; everything else (segments, the
//! layout, the patch, the edges) is derived from it.
//!
//! Node ids are minted here, so a node id is unique across the whole
//! project — and only across the project: nothing assumes it is the only
//! one (D44). A part's id is its fixture's (`lpc_mapping::part::Mapping`
//! mints it), so a part is named with its fixture ([`PartRef`]).
//!
//! Positions this file hands out (`segments`, `segment`) are in **world**
//! units: a fixture's own coordinates plus where the view placed it
//! (`placement.rs`). Edits go to parts in the fixture's own coordinates;
//! a move is the same delta in both.
//!
//! The nodes are private to this file so that every edit comes in through
//! a door that forgets what was worked out about them (`derived.rs`): a
//! kept layout, segments or patch is never stale.

use lpc_mapping::part::{Circle, Part, PartKind, Path, Shape};

use crate::domain::channel::Channel;
use crate::domain::derived::Derived;
use crate::domain::drawn_segment::{DrawnSegment, f32s};
use crate::domain::fixture::Fixture;
use crate::domain::geom::{Rect, Vec2};
use crate::domain::node::{self, Node, NodeId, NodeKind};
use crate::domain::output::Output;
use crate::domain::part_ref::{PartRef, TreeRef};
use crate::domain::pattern::Pattern;
use crate::domain::placement::Carried;
use crate::domain::port_tables::CarriedPortTables;
use crate::domain::render_source::RenderSource;

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub name: String,
    /// The root module's children, in authoring order. Read with
    /// [`Project::nodes`]; change with [`Project::nodes_mut`].
    nodes: Vec<Node>,
    /// Per kind of node, the next number to mint. Never decremented, so an
    /// id is never handed out twice.
    counters: Counters,
    /// What has been worked out about the nodes, until they change.
    pub(crate) derived: Derived,
    /// Where the view placed the nodes (`placement.rs`).
    pub(crate) placement: Carried,
    /// The ports each fixture is patched against, when a host handed them
    /// over (`port_tables.rs`).
    pub(crate) port_tables: CarriedPortTables,
}

#[derive(Debug, Clone, PartialEq)]
struct Counters {
    module: u32,
    fixture: u32,
    output: u32,
    source: u32,
    node: u32,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: Vec::new(),
            counters: Counters {
                module: 1,
                fixture: 1,
                output: 1,
                source: 1,
                node: 1,
            },
            derived: Derived::default(),
            placement: Carried::default(),
            port_tables: CarriedPortTables::default(),
        }
    }

    // ---- minting ---------------------------------------------------------

    /// A new part of this kind for `fixture`, its id minted by the
    /// fixture's mapping (never one it has, never one it had); not yet
    /// in the fixture. `None` when `fixture` isn't one.
    pub fn new_part(&mut self, fixture: &NodeId, kind: PartKind) -> Option<Part> {
        Some(self.fixture_mut(fixture)?.mapping.new_part(kind))
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
        let id = loop {
            let (word, counter) = match kind {
                NodeKind::Module { .. } => ("module", &mut self.counters.module),
                NodeKind::Fixture(_) => ("fixture", &mut self.counters.fixture),
                NodeKind::Output(_) => ("output", &mut self.counters.output),
                NodeKind::Source(_) => ("source", &mut self.counters.source),
                NodeKind::Other(_) => ("node", &mut self.counters.node),
            };
            let id = NodeId::new(format!("{word}{counter}"));
            *counter += 1;
            if self.node(&id).is_none() {
                break id;
            }
        };
        let node = Node {
            id: id.clone(),
            name: name.into(),
            kind,
            pinned: None,
        };
        let list = match parent {
            None => self.nodes_mut(),
            Some(p) => self.node_mut(p)?.children_mut()?,
        };
        list.push(node);
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

    /// An output reads `control.out`, unless another output beside it
    /// already does: then a channel of its own, from its name
    /// (`control.scarf_out`) — the product's idiom. It is named as it is
    /// called, so a fixture's start can say it (ops name outputs by name).
    pub fn add_output(
        &mut self,
        parent: Option<&NodeId>,
        name: &str,
        ports: &[(&str, u32)],
    ) -> Option<NodeId> {
        let level = match parent {
            None => self.nodes(),
            Some(p) => self.node(p)?.children(),
        };
        let taken = level.iter().any(|n| {
            n.output()
                .is_some_and(|o| o.reads == Channel::control_out())
        });
        let mut output = Output::new(ports);
        output.label = Some(name.to_string());
        if taken {
            output.reads = Channel::control_for(name);
        }
        self.add_node(parent, name, NodeKind::Output(output))
    }

    /// A shader showing `pattern`, writing `visual.out` where it is.
    pub fn add_shader(
        &mut self,
        parent: Option<&NodeId>,
        name: &str,
        pattern: Pattern,
    ) -> Option<NodeId> {
        self.add_node(
            parent,
            name,
            NodeKind::Source(RenderSource::shader(pattern)),
        )
    }

    /// Mint and append a top-level part to a fixture (`lpc_mapping::edit`'s
    /// `insert_part`, last in wiring order); returns it.
    pub fn add(&mut self, fixture: &NodeId, kind: PartKind) -> Option<PartRef> {
        let part = self.new_part(fixture, kind)?;
        let id = PartRef::new(fixture, &part.id);
        let mapping = &mut self.fixture_mut(fixture)?.mapping;
        let at = mapping.parts.len();
        lpc_mapping::edit::insert_part(mapping, None, at, part).ok()?;
        Some(id)
    }

    /// Start a fixture's walk at `pixel` of `output`'s port `port`.
    pub fn start_at(&mut self, fixture: &NodeId, output: &NodeId, port: &str, pixel: u32) {
        if let Some(start) = self.start_ops_at(output, port, pixel)
            && let Some(f) = self.fixture_mut(fixture)
        {
            // Setting the fixture's start never refuses.
            let _ = lpc_mapping::edit::set_start(
                &mut f.mapping,
                lpc_mapping::patching::PatchTarget::Fixture,
                start,
            );
        }
    }

    // ---- nodes -------------------------------------------------------------

    /// The root module's children, in authoring order.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The root module's children, to change. Forgets what was worked out
    /// about them.
    pub fn nodes_mut(&mut self) -> &mut Vec<Node> {
        self.changed();
        &mut self.nodes
    }

    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        node::find(&self.nodes, id)
    }

    /// A node, to change. Forgets what was worked out about the project.
    pub fn node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        node::find_mut(self.nodes_mut(), id)
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

    /// Pin a node at `at`, in its container's coordinates (D48). Inside a
    /// module a pin stays inside its padding. Pins are session data: the
    /// layout honours them only while pinning is on (D64).
    pub fn pin(&mut self, id: &NodeId, at: Vec2) {
        let at = if self.parent_module(id).is_some() {
            Vec2::new(at.x.max(0.0), at.y.max(0.0))
        } else {
            at
        };
        if let Some(n) = self.node_mut(id) {
            n.pinned = Some(at);
        }
    }

    /// Give a node back to the automatic layout. Returns whether it was
    /// pinned.
    pub fn unpin(&mut self, id: &NodeId) -> bool {
        self.node_mut(id).is_some_and(|n| n.pinned.take().is_some())
    }

    /// The node's name, or its id if it's gone.
    pub fn node_name(&self, id: &NodeId) -> String {
        self.node(id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    pub fn remove_node(&mut self, id: &NodeId) -> Option<Node> {
        node::remove_from(self.nodes_mut(), id)
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

    // ---- parts ---------------------------------------------------------------

    pub fn part(&self, id: &PartRef) -> Option<&Part> {
        self.fixture(&id.fixture)?.part(&id.part)
    }

    pub fn part_mut(&mut self, id: &PartRef) -> Option<&mut Part> {
        self.fixture_mut(&id.fixture)?.part_mut(&id.part)
    }

    /// The part holding `id` (a group or a repeat), or `None` when `id` is
    /// top-level in its fixture (or absent).
    pub fn parent_part(&self, id: &PartRef) -> Option<PartRef> {
        let parent = self.fixture(&id.fixture)?.parent_part(&id.part)?;
        Some(PartRef::new(&id.fixture, parent))
    }

    /// Take a part (and everything under it) out of wherever it lives.
    pub fn remove_part(&mut self, id: &PartRef) -> Option<Part> {
        self.fixture_mut(&id.fixture)?.remove(&id.part)
    }

    // ---- derived: segments and lamps, in world units ---------------------

    /// Every segment with the fixture it lives in, fixtures in tree order,
    /// segments in each one's lamp order.
    pub fn placed_segments(&self) -> &[(NodeId, DrawnSegment)] {
        &self.placed().list
    }

    /// One fixture's segments, in its lamp order, in world units.
    pub fn segments_in(&self, fixture: &NodeId) -> Vec<&DrawnSegment> {
        self.placed_segments()
            .iter()
            .filter(|(f, _)| f == fixture)
            .map(|(_, o)| o)
            .collect()
    }

    pub fn segment(&self, id: &TreeRef) -> Option<&DrawnSegment> {
        self.placed().get(id).map(|(_, o)| o)
    }

    /// The segments a part renders, in world units and tree order: its
    /// own, its generated ones, or everything under it (a group's parts',
    /// every copy of a repeat's part's).
    pub fn segments_of(&self, id: &PartRef) -> Vec<&DrawnSegment> {
        self.placed()
            .of_part(id)
            .into_iter()
            .map(|(_, o)| o)
            .collect()
    }

    /// Lamps outside their own fixture's box, as (segment, lamp index).
    pub fn lamps_outside(&self) -> Vec<(TreeRef, u32)> {
        let mut out = Vec::new();
        for (f, segment) in self.placed_segments() {
            let Some(bounds) = self.node_rect(f) else {
                continue;
            };
            for (i, lamp) in segment.lamps.iter().enumerate() {
                if !bounds.contains(lamp.pos) {
                    out.push((segment.id.clone(), i as u32));
                }
            }
        }
        out
    }

    pub fn lamp_count(&self) -> usize {
        self.placed().lamp_count()
    }

    /// The lab's starting project: Yona's desk. A tail rig (the tail that
    /// runs out and back, its own shader and its own output), a two-ring
    /// circle display and a strip sharing the main output, and an empty
    /// fixture to draw a scarf into, already patched to its own output — so
    /// drawing in it lights pixels at once. A rainbow shader at the root
    /// feeds every fixture there; the tail reads its own module's shader,
    /// the nearer writer of `visual.out`.
    ///
    /// Every fixture is square, as a new one is, sized to hold its drawing
    /// with room around it. The tail is two straight paths, out and back,
    /// the way the choker was built.
    pub fn seed() -> Self {
        let mut p = Project::new("desk");
        let side = |s: f64| Vec2::new(s, s);
        let path = |points: &[Vec2], lamps: u32| {
            let points: Vec<[f32; 2]> = points.iter().map(|p| f32s(*p)).collect();
            PartKind::Shape(Shape::Path(Path::through(&points, lamps)))
        };

        p.add_shader(None, "rainbow", Pattern::Rainbow);
        let rig = p.add_module(None, "tail rig");
        p.add_shader(rig.as_ref(), "glow", Pattern::Gradient);
        let tail = p.add_fixture_sized(rig.as_ref(), "tail", side(82.0));
        let tail_out = p.add_output(rig.as_ref(), "tail out", &[("GPIO 10", 60)]);
        let ring = p.add_fixture_sized(None, "ring display", side(70.0));
        let strip = p.add_fixture_sized(None, "strip", side(80.0));
        let main = p.add_output(None, "main", &[("D0", 64), ("D1", 30)]);
        let scarf = p.add_fixture(None, "scarf");
        let scarf_out = p.add_output(None, "scarf out", &[("D10", 60)]);

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
        let out = p
            .new_part(
                &tail,
                path(&[Vec2::new(37.0, 70.0), Vec2::new(37.0, 12.0)], 25),
            )
            .expect("a fixture");
        let back = p
            .new_part(
                &tail,
                path(&[Vec2::new(45.0, 12.0), Vec2::new(45.0, 70.0)], 25),
            )
            .expect("a fixture");
        p.add(
            &tail,
            PartKind::Group {
                parts: vec![out, back],
            },
        );
        p.add(
            &ring,
            PartKind::Shape(Shape::Circle(Circle::new(
                [35.0, 35.0],
                &[(26.0, 40), (16.0, 24)],
            ))),
        );
        p.add(
            &strip,
            path(&[Vec2::new(6.0, 40.0), Vec2::new(74.0, 40.0)], 30),
        );
        // Each fixture starts somewhere of its own (nothing flows across
        // fixtures): the strip on D1, where the ring display's 64 lamps
        // end on D0's 64 pixels.
        p.start_at(&tail, &tail_out, "0", 0);
        p.start_at(&ring, &main, "0", 0);
        p.start_at(&strip, &main, "1", 0);
        p.start_at(&scarf, &scarf_out, "0", 0);
        p
    }
}

/// The rectangle around a set of rectangles.
pub(crate) fn union(rects: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    Rect::around(rects.into_iter().flat_map(|r| [r.min, r.max]))
}
