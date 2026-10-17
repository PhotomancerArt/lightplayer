//! What the model derives from a project, kept until the project changes.
//!
//! Everything here is a function of the project and where the view placed
//! its nodes (`placement.rs`) — never of the camera, the selection or the
//! time: every object with its lamps in world units, the effective patch,
//! and which pixel each lamp lights. Asked for again with nothing changed, it is the same answer, so
//! it is worked out once and kept.
//!
//! **Why it can't go stale:** a project's nodes are private to
//! `project.rs`, and every way in that can change them — `nodes_mut`,
//! `node_mut`, `fixture_mut`, `component_mut`, adding, removing — throws
//! the kept answers away first ([`Project::changed`]). Nothing can read a
//! kept answer while something holds a `&mut` to the project, so the next
//! question after an edit always works it out afresh. A clone shares what
//! its original had worked out (it is the same project); undo snapshots
//! cost nothing extra.
//!
//! Why it matters: the radiance dome has 22,000 lamps. Before this, every
//! question re-resolved every lamp and re-laid-out every node, and a page
//! frame asked hundreds of them (`files/examples/scale_bench.rs`).

use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use crate::domain::component::{Component, ComponentId};
use crate::domain::geom::{Rect, Vec2};
use crate::domain::node::NodeId;
use crate::domain::object::{Object, ObjectId, ObjectShape};
use crate::domain::patch::EffectivePatch;
use crate::domain::placement::Frame;
use crate::domain::project::Project;

/// The kept answers. Each is worked out the first time it is asked for.
#[derive(Debug, Clone)]
pub(crate) struct Derived {
    /// Names what the project holds right now: see [`Project::revision`].
    revision: u64,
    placed: OnceLock<Arc<Placed>>,
    patch: OnceLock<Arc<EffectivePatch>>,
    wiring: OnceLock<Arc<Wiring>>,
    spans: OnceLock<Arc<SpanOutlines>>,
}

/// Each span's outline on its output, in world units: plain boxes, worked
/// out by the view's card geometry (`ux/area.rs`) and kept here with the
/// rest, since they too follow only the project and where its nodes sit.
#[derive(Debug, Default)]
pub struct SpanOutlines {
    /// Each span: its object, its output, its outline (a box per line of
    /// the port's pixels it covers).
    pub list: Vec<(ObjectId, NodeId, Vec<Rect>)>,
    pub by_object: HashMap<ObjectId, Vec<usize>>,
    pub by_output: HashMap<NodeId, Vec<usize>>,
}

impl Default for Derived {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Derived {
            revision: NEXT.fetch_add(1, Ordering::Relaxed),
            placed: OnceLock::new(),
            patch: OnceLock::new(),
            wiring: OnceLock::new(),
            spans: OnceLock::new(),
        }
    }
}

/// Two projects are equal by what they hold, never by what they have
/// worked out about it.
impl PartialEq for Derived {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// Every object, in world units, and where to find each.
#[derive(Debug, Default)]
pub struct Placed {
    /// Each object with the fixture it lives in: fixtures in tree order,
    /// objects in authoring order — the order the auto patch fills ports.
    pub list: Vec<(NodeId, Object)>,
    by_id: HashMap<ObjectId, usize>,
    /// A component's own objects sit together in `list`.
    by_component: HashMap<ComponentId, Range<usize>>,
    /// Every component (groups too) and the fixture it lives in.
    fixture_of: HashMap<ComponentId, NodeId>,
    /// Where each object's lamp 0 sits among all the lamps, `list` order.
    first_lamp: Vec<usize>,
    lamps: usize,
    /// Each fixture's lamps among all the lamps: its objects sit together
    /// in `list`, so they are one stretch. Every fixture has one, empty or
    /// not.
    fixture_lamps: HashMap<NodeId, Range<usize>>,
    /// Per lamp, among all the lamps: inside its fixture's box (D4).
    inside: Vec<bool>,
    /// The box round each object's lamps and drawn shape, `list` order…
    bounds: Vec<Rect>,
    /// …and round everything under each component (a group's children too).
    component_bounds: HashMap<ComponentId, Rect>,
}

impl Placed {
    pub fn get(&self, id: &ObjectId) -> Option<&(NodeId, Object)> {
        self.by_id.get(id).map(|&i| &self.list[i])
    }

    /// Its index in `list`.
    pub fn index_of(&self, id: &ObjectId) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    /// A component's own objects (a group has none).
    pub fn of_component(&self, id: &ComponentId) -> &[(NodeId, Object)] {
        match self.by_component.get(id) {
            Some(r) => &self.list[r.clone()],
            None => &[],
        }
    }

    pub fn fixture_of(&self, id: &ComponentId) -> Option<&NodeId> {
        self.fixture_of.get(id)
    }

    /// Where an object's lamp 0 sits among every lamp, counted in `list`
    /// order: the page keeps one flat array of lamps.
    pub fn first_lamp(&self, i: usize) -> usize {
        self.first_lamp[i]
    }

    pub fn lamp_count(&self) -> usize {
        self.lamps
    }

    /// A fixture's lamps among all the lamps (empty for a fixture with
    /// none, `None` for a node that isn't one).
    pub fn fixture_lamps(&self, fixture: &NodeId) -> Option<Range<usize>> {
        self.fixture_lamps.get(fixture).cloned()
    }

    /// Whether each lamp sits inside its fixture's box, among all the
    /// lamps.
    pub fn inside(&self) -> &[bool] {
        &self.inside
    }

    /// The box round an object's lamps and its drawn shape, world units.
    pub fn object_bounds(&self, id: &ObjectId) -> Option<Rect> {
        Some(self.bounds[*self.by_id.get(id)?])
    }

    /// The box round everything under a component, world units; `None`
    /// for one with nothing under it.
    pub fn component_bounds(&self, id: &ComponentId) -> Option<Rect> {
        self.component_bounds.get(id).copied()
    }
}

/// Which pixel each lamp lands on, worked out from the patch once: a
/// board frame then only colours them (`board.rs`).
#[derive(Debug, Default, PartialEq)]
pub struct Wiring {
    /// Every port in output then port order, and its pixel count: the
    /// board's ports, all black.
    pub ports: Vec<(NodeId, String, usize)>,
    /// Per span of the effective patch (same index): the port it lands on
    /// (an index into `ports`), and each of its lamps that lands on one of
    /// that port's pixels, as (lamp index, pixel).
    pub spans: Vec<Option<SpanWiring>>,
    /// Per lamp, in [`Placed`] order: the (port, pixel) it lands on — only
    /// lamps that are patched to a real pixel.
    pub lit: Vec<Option<(u32, u32)>>,
    /// Where each object's lamp 0 sits in `lit`.
    pub first_lamp: HashMap<ObjectId, usize>,
}

#[derive(Debug, PartialEq)]
pub struct SpanWiring {
    pub port: usize,
    /// Where the span's object's lamp 0 sits among all the lamps.
    pub first_lamp: usize,
    pub lamps: Vec<(u32, u32)>,
}

impl Wiring {
    /// The (port, pixel) a lamp lands on, if it is patched to one.
    pub fn lamp(&self, object: &ObjectId, index: u32) -> Option<(usize, usize)> {
        let first = *self.first_lamp.get(object)?;
        let (port, pixel) = (*self.lit.get(first + index as usize)?)?;
        Some((port as usize, pixel as usize))
    }
}

impl Project {
    /// Forget everything worked out about this project: it is about to
    /// change. Every `&mut` way into its nodes calls this first.
    pub(crate) fn changed(&mut self) {
        self.derived = Derived::default();
    }

    /// Forget what was worked out in world units: the view placed the
    /// nodes somewhere new. What the project holds is the same, so its
    /// revision stays.
    pub(crate) fn moved(&mut self) {
        let revision = self.derived.revision;
        self.derived = Derived {
            revision,
            ..Derived::default()
        };
    }

    /// A number naming what the project holds right now. Every edit gives
    /// it a new one, never used before by any project; a clone keeps its
    /// original's (it holds the same). So a page can keep anything it
    /// draws from the project — the tree's rows, say — until this moves.
    pub fn revision(&self) -> u64 {
        self.derived.revision
    }

    /// Every object in world units, kept.
    pub fn placed(&self) -> &Placed {
        self.derived
            .placed
            .get_or_init(|| Arc::new(self.compute_placed()))
    }

    pub(crate) fn kept_patch(&self) -> &EffectivePatch {
        self.derived
            .patch
            .get_or_init(|| Arc::new(self.compute_patch()))
    }

    /// Each span's outline, kept; `work_out` is asked only the first time.
    pub(crate) fn kept_span_outlines(
        &self,
        work_out: impl FnOnce() -> SpanOutlines,
    ) -> &SpanOutlines {
        self.derived.spans.get_or_init(|| Arc::new(work_out()))
    }

    /// Which pixel each lamp lands on, kept. Shared with every board frame
    /// made from this project.
    pub(crate) fn wiring(&self) -> Arc<Wiring> {
        self.derived
            .wiring
            .get_or_init(|| Arc::new(self.compute_wiring()))
            .clone()
    }

    fn compute_placed(&self) -> Placed {
        let mut p = Placed::default();
        let layout = self.placement();
        for n in self.fixtures() {
            let Some(f) = n.fixture() else { continue };
            index_components(&f.components, &n.id, self.frame(&n.id), &mut p);
        }
        let mut lamps = 0;
        for (i, (fixture, o)) in p.list.iter().enumerate() {
            p.by_id.insert(o.id.clone(), i);
            p.first_lamp.push(lamps);
            let stretch = p
                .fixture_lamps
                .entry(fixture.clone())
                .or_insert(lamps..lamps);
            stretch.end = lamps + o.lamps.len();
            let bounds = layout.rects.get(fixture);
            p.inside.extend(
                o.lamps
                    .iter()
                    .map(|l| bounds.is_some_and(|b| b.contains(l.pos))),
            );
            lamps += o.lamps.len();
            p.bounds.push(object_bounds(o));
        }
        p.lamps = lamps;
        for n in self.fixtures() {
            p.fixture_lamps.entry(n.id.clone()).or_insert(lamps..lamps);
        }
        for n in self.fixtures() {
            if let Some(f) = n.fixture() {
                bound_components(&f.components, &mut p);
            }
        }
        p
    }

    fn compute_wiring(&self) -> Wiring {
        let patch = self.kept_patch();
        let placed = self.placed();
        let mut w = Wiring::default();
        for n in self.outputs() {
            let Some(o) = n.output() else { continue };
            for p in &o.ports {
                let pixels = p.pixels.unwrap_or_else(|| patch.extent(&n.id, &p.key));
                w.ports.push((n.id.clone(), p.key.clone(), pixels as usize));
            }
        }
        w.lit = vec![None; placed.lamp_count()];
        for (i, (_, o)) in placed.list.iter().enumerate() {
            w.first_lamp.insert(o.id.clone(), placed.first_lamp(i));
        }
        for s in &patch.spans {
            let Some(at) = placed.index_of(&s.object) else {
                w.spans.push(None);
                continue;
            };
            let Some(port) = w
                .ports
                .iter()
                .position(|(o, k, _)| *o == s.output && *k == s.port)
            else {
                w.spans.push(None);
                continue;
            };
            let len = w.ports[port].2;
            let first = placed.first_lamp(at);
            let mut lamps = Vec::with_capacity(s.lamps.len());
            for lamp in s.lamps.clone() {
                let pixel = s.pixel(lamp);
                if pixel as usize >= len {
                    continue;
                }
                lamps.push((lamp, pixel));
                if let Some(slot) = w.lit.get_mut(first + lamp as usize) {
                    *slot = Some((port as u32, pixel));
                }
            }
            w.spans.push(Some(SpanWiring {
                port,
                first_lamp: first,
                lamps,
            }));
        }
        w
    }
}

/// The box round an object's lamps and its drawn shape (a ring's whole
/// circle).
fn object_bounds(o: &Object) -> Rect {
    let shape: Vec<Vec2> = match &o.shape {
        ObjectShape::Polyline { points } => points.clone(),
        ObjectShape::Ring { center, radius } => {
            let r = Vec2::new(*radius, *radius);
            vec![*center - r, *center + r]
        }
    };
    Rect::around(shape.into_iter().chain(o.lamps.iter().map(|l| l.pos)))
        .unwrap_or(Rect::new(Vec2::default(), Vec2::default()))
}

/// Each component's box: its own objects' and its children's. Returns the
/// box round the whole list.
fn bound_components(list: &[Component], p: &mut Placed) -> Option<Rect> {
    let mut all: Option<Rect> = None;
    for c in list {
        let own = p.by_component.get(&c.id).cloned().unwrap_or_default();
        let mut boxes: Vec<Rect> = p.bounds[own].to_vec();
        boxes.extend(bound_components(c.children(), p));
        if let Some(b) = Rect::around(boxes.iter().flat_map(|r| [r.min, r.max])) {
            p.component_bounds.insert(c.id.clone(), b);
            all = Rect::around(all.into_iter().chain([b]).flat_map(|r| [r.min, r.max]));
        }
    }
    all
}

/// Index a fixture's components: each one's fixture, and its objects placed
/// in world units, in authoring order (a component's own objects, then its
/// children's).
fn index_components(list: &[Component], fixture: &NodeId, frame: Frame, p: &mut Placed) {
    for c in list {
        p.fixture_of.insert(c.id.clone(), fixture.clone());
        let start = p.list.len();
        p.list.extend(
            crate::domain::object::objects_of(c)
                .into_iter()
                .map(|o| (fixture.clone(), crate::domain::project::placed(o, frame))),
        );
        p.by_component.insert(c.id.clone(), start..p.list.len());
        index_components(c.children(), fixture, frame, p);
    }
}
