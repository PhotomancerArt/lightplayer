//! What the model derives from a project, kept until the project changes.
//!
//! Everything here is a function of the project alone — never of the
//! camera, the selection or the time: the layout, every object with its
//! lamps in world units, the effective patch, and which pixel each lamp
//! lights. Asked for again with nothing changed, it is the same answer, so
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
use std::sync::{Arc, OnceLock};

use crate::component::{Component, ComponentId};
use crate::layout::Layout;
use crate::node::NodeId;
use crate::object::{Object, ObjectId};
use crate::patch::EffectivePatch;
use crate::project::Project;

/// The kept answers. Each is worked out the first time it is asked for.
#[derive(Debug, Clone, Default)]
pub(crate) struct Derived {
    layout: OnceLock<Arc<Layout>>,
    placed: OnceLock<Arc<Placed>>,
    patch: OnceLock<Arc<EffectivePatch>>,
    wiring: OnceLock<Arc<Wiring>>,
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
}

/// Which pixel each lamp lights, worked out from the patch once: a device
/// frame then only colours them.
#[derive(Debug, Default, PartialEq)]
pub struct Wiring {
    /// Every port in output then port order, and its pixel count: the
    /// board's ports, all black.
    pub ports: Vec<(NodeId, String, usize)>,
    /// Per span of the effective patch (same index): the port it lands on
    /// (an index into `ports`), and each of its lamps that lands on one of
    /// that port's pixels, as (lamp index, pixel, inside its box).
    pub spans: Vec<Option<SpanWiring>>,
    /// Per lamp, in [`Placed`] order: the (port, pixel) it lights. Only
    /// lamps that light — patched, on a real pixel, inside their box.
    pub lit: Vec<Option<(u32, u32)>>,
    /// Where each object's lamp 0 sits in `lit`.
    pub first_lamp: HashMap<ObjectId, usize>,
}

#[derive(Debug, PartialEq)]
pub struct SpanWiring {
    pub port: usize,
    /// How many lamps the span's object has.
    pub object_lamps: usize,
    pub lamps: Vec<(u32, u32, bool)>,
}

impl Wiring {
    /// The (port, pixel) a lamp lights, if it lights one.
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

    /// Every node's box, kept.
    pub(crate) fn kept_layout(&self) -> &Layout {
        self.derived
            .layout
            .get_or_init(|| Arc::new(self.compute_layout()))
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

    /// Which pixel each lamp lights, kept. Shared with every device frame
    /// made from this project.
    pub(crate) fn wiring(&self) -> Arc<Wiring> {
        self.derived
            .wiring
            .get_or_init(|| Arc::new(self.compute_wiring()))
            .clone()
    }

    fn compute_placed(&self) -> Placed {
        let mut p = Placed::default();
        let layout = self.kept_layout();
        for n in self.fixtures() {
            let Some(f) = n.fixture() else { continue };
            let origin = layout.rects.get(&n.id).map(|r| r.min).unwrap_or_default();
            index_components(&f.components, &n.id, origin, &mut p);
        }
        let mut lamps = 0;
        for (i, (_, o)) in p.list.iter().enumerate() {
            p.by_id.insert(o.id.clone(), i);
            p.first_lamp.push(lamps);
            lamps += o.lamps.len();
        }
        p.lamps = lamps;
        p
    }

    fn compute_wiring(&self) -> Wiring {
        let patch = self.kept_patch();
        let placed = self.placed();
        let layout = self.kept_layout();
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
            let (fixture, object) = &placed.list[at];
            let Some(port) = w
                .ports
                .iter()
                .position(|(o, k, _)| *o == s.output && *k == s.port)
            else {
                w.spans.push(None);
                continue;
            };
            let bounds = layout.rects.get(fixture).copied();
            let len = w.ports[port].2;
            let first = placed.first_lamp(at);
            let mut lamps = Vec::with_capacity(s.lamps.len());
            for lamp in s.lamps.clone() {
                let pixel = s.pixel(lamp);
                if pixel as usize >= len {
                    continue;
                }
                let inside = object
                    .lamps
                    .get(lamp as usize)
                    .is_some_and(|l| bounds.is_some_and(|b| b.contains(l.pos)));
                lamps.push((lamp, pixel, inside));
                if let Some(slot) = w.lit.get_mut(first + lamp as usize) {
                    *slot = inside.then_some((port as u32, pixel));
                }
            }
            w.spans.push(Some(SpanWiring {
                port,
                object_lamps: object.lamps.len(),
                lamps,
            }));
        }
        w
    }
}

/// Index a fixture's components: each one's fixture, and its objects placed
/// in world units, in authoring order (a component's own objects, then its
/// children's).
fn index_components(
    list: &[Component],
    fixture: &NodeId,
    origin: crate::geom::Vec2,
    p: &mut Placed,
) {
    for c in list {
        p.fixture_of.insert(c.id.clone(), fixture.clone());
        let start = p.list.len();
        p.list.extend(
            crate::object::objects_of(c)
                .into_iter()
                .map(|o| (fixture.clone(), crate::project::placed(o, origin))),
        );
        p.by_component.insert(c.id.clone(), start..p.list.len());
        index_components(c.children(), fixture, origin, p);
    }
}
