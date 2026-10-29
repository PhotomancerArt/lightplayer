//! What the model derives from a project, kept until the project changes.
//!
//! Everything here is a function of the project and where the view placed
//! its nodes (`placement.rs`) — never of the camera, the selection or the
//! time: each fixture's rendered tree (`lpc_mapping::part::render`), every
//! segment with its lamps in world units, the effective patch, and which
//! pixel each lamp lights. Asked for again with nothing changed, it is the
//! same answer, so it is worked out once and kept.
//!
//! **Why it can't go stale:** a project's nodes are private to
//! `project.rs`, and every way in that can change them — `nodes_mut`,
//! `node_mut`, `fixture_mut`, adding, removing — throws the kept answers
//! away first ([`Project::changed`]). Nothing can read a kept answer while
//! something holds a `&mut` to the project, so the next question after an
//! edit always works it out afresh. A clone shares what its original had
//! worked out (it is the same project); undo snapshots cost nothing extra.
//!
//! Why it matters: the radiance dome has 22,000 lamps. Before this, every
//! question re-resolved every lamp and re-laid-out every node, and a page
//! frame asked hundreds of them (`files/examples/scale_bench.rs`).

use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use lpc_mapping::part::{Tree, render};

use crate::domain::drawn_segment::{DrawnSegment, DrawnShape, drawn_segments, in_world};
use crate::domain::geom::{Rect, Vec2};
use crate::domain::node::NodeId;
use crate::domain::part_ref::{PartRef, TreeRef};
use crate::domain::patch::EffectivePatch;
use crate::domain::project::Project;

/// The kept answers. Each is worked out the first time it is asked for.
#[derive(Debug, Clone)]
pub(crate) struct Derived {
    /// Names what the project holds right now: see [`Project::revision`].
    revision: u64,
    placed: OnceLock<Arc<Placed>>,
    patch: OnceLock<Arc<EffectivePatch>>,
    wiring: OnceLock<Arc<Wiring>>,
}

impl Default for Derived {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Derived {
            revision: NEXT.fetch_add(1, Ordering::Relaxed),
            placed: OnceLock::new(),
            patch: OnceLock::new(),
            wiring: OnceLock::new(),
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

/// Every segment, in world units, and where to find each.
#[derive(Debug, Default)]
pub struct Placed {
    /// Each segment with the fixture it lives in: fixtures in tree order,
    /// segments in each fixture's tree order (its lamp order).
    pub list: Vec<(NodeId, DrawnSegment)>,
    by_id: HashMap<TreeRef, usize>,
    /// Each fixture's rendered tree, and where its segments start in
    /// `list`.
    trees: HashMap<NodeId, (Arc<Tree>, usize)>,
    /// Why a fixture's mapping didn't render, when it didn't: it draws
    /// nothing.
    refused: HashMap<NodeId, String>,
    /// The segments under each part's node, every copy included, as
    /// indices into `list`, in tree order.
    by_part: HashMap<PartRef, Vec<usize>>,
    /// Where each segment's lamp 0 sits among all the lamps, `list` order.
    first_lamp: Vec<usize>,
    lamps: usize,
    /// Each fixture's lamps among all the lamps: its segments sit together
    /// in `list`, so they are one stretch. Every fixture has one, empty or
    /// not.
    fixture_lamps: HashMap<NodeId, Range<usize>>,
    /// Per lamp, among all the lamps: inside its fixture's box (D4).
    inside: Vec<bool>,
    /// The box round each segment's lamps and drawn shape, `list` order…
    bounds: Vec<Rect>,
    /// …and round everything under each part (a group's parts too).
    part_bounds: HashMap<PartRef, Rect>,
}

/// One node of a fixture's rendered tree, as placed.
#[derive(Debug, Clone, Copy)]
pub struct PlacedNode<'a> {
    placed: &'a Placed,
    fixture: &'a NodeId,
    tree: &'a Tree,
    base: usize,
    index: usize,
}

impl<'a> PlacedNode<'a> {
    pub fn id(&self) -> TreeRef {
        TreeRef::new(self.fixture, self.tree.node(self.index).id.clone())
    }

    /// Does it hold lamps of its own (a segment)?
    pub fn is_leaf(&self) -> bool {
        self.tree.node(self.index).is_segment()
    }

    /// Is it a part's own node (in any copy), not a segment its part
    /// generated (a ring, a subpath)?
    pub fn is_part_node(&self) -> bool {
        self.tree.node(self.index).id.is_part_node()
    }

    /// Every segment under it (itself, when it is one), in tree order.
    pub fn leaves(&self) -> Vec<&'a DrawnSegment> {
        self.tree
            .node(self.index)
            .segments
            .clone()
            .map(|s| &self.placed.list[self.base + s].1)
            .collect()
    }

    /// Its children, in order.
    pub fn children(&self) -> Vec<TreeRef> {
        self.tree
            .children(self.index)
            .map(|c| TreeRef::new(self.fixture, self.tree.node(c).id.clone()))
            .collect()
    }
}

impl Placed {
    pub fn get(&self, id: &TreeRef) -> Option<&(NodeId, DrawnSegment)> {
        self.by_id.get(id).map(|&i| &self.list[i])
    }

    /// Its index in `list`.
    pub fn index_of(&self, id: &TreeRef) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    /// The segments a part renders, in tree order: its own (a path), its
    /// generated ones (a circle's rings), or everything under it (a group's
    /// parts', every copy of a repeat's part's).
    pub fn of_part(&self, id: &PartRef) -> Vec<&(NodeId, DrawnSegment)> {
        self.by_part
            .get(id)
            .map(|v| v.iter().map(|&i| &self.list[i]).collect())
            .unwrap_or_default()
    }

    /// A fixture's rendered tree, when its mapping rendered.
    pub fn tree(&self, fixture: &NodeId) -> Option<&Tree> {
        self.trees.get(fixture).map(|(t, _)| &**t)
    }

    /// Why a fixture's mapping didn't render, when it didn't.
    pub fn refusal(&self, fixture: &NodeId) -> Option<&str> {
        self.refused.get(fixture).map(String::as_str)
    }

    /// The node with this id, a segment or not, in its fixture.
    pub fn node(&self, id: &TreeRef) -> Option<(&NodeId, PlacedNode<'_>)> {
        let (fixture, (tree, base)) = self.trees.get_key_value(&id.fixture)?;
        let index = tree.find(&id.id)?;
        Some((
            fixture,
            PlacedNode {
                placed: self,
                fixture,
                tree,
                base: *base,
                index,
            },
        ))
    }

    /// A fixture's top-level nodes (its parts' own), in order.
    pub fn roots(&self, fixture: &NodeId) -> Vec<TreeRef> {
        let Some((tree, _)) = self.trees.get(fixture) else {
            return Vec::new();
        };
        tree.roots()
            .map(|r| TreeRef::new(fixture, tree.node(r).id.clone()))
            .collect()
    }

    /// The node above this one; `None` for a top-level part's node (the
    /// fixture is above it) or an id that isn't there.
    pub fn parent_of(&self, id: &TreeRef) -> Option<TreeRef> {
        let (tree, _) = self.trees.get(&id.fixture)?;
        let parent = tree.parent(tree.find(&id.id)?)?;
        Some(TreeRef::new(&id.fixture, tree.node(parent).id.clone()))
    }

    /// Where a segment's lamp 0 sits among every lamp, counted in `list`
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

    /// The box round a segment's lamps and its drawn shape, world units.
    pub fn segment_bounds(&self, id: &TreeRef) -> Option<Rect> {
        Some(self.bounds[*self.by_id.get(id)?])
    }

    /// The box round everything under a part, world units; `None` for one
    /// with nothing under it.
    pub fn part_bounds(&self, id: &PartRef) -> Option<Rect> {
        self.part_bounds.get(id).copied()
    }
}

/// Which pixel each lamp lands on, worked out from the patch once: a
/// board frame then only colours them (`board.rs`).
#[derive(Debug, Default, PartialEq)]
pub struct Wiring {
    /// Every port in output then port order, and its pixel count: the
    /// board's ports, all black.
    pub ports: Vec<(NodeId, String, usize)>,
    /// Per pixel run of the effective patch (same index): the port it lands
    /// on (an index into `ports`), and each of its lamps that lands on one
    /// of that port's pixels, as (lamp index, pixel).
    pub pixel_runs: Vec<Option<SpanWiring>>,
    /// Per lamp, in [`Placed`] order: the (port, pixel) it lands on — only
    /// lamps that are patched to a real pixel.
    pub lit: Vec<Option<(u32, u32)>>,
    /// Where each segment's lamp 0 sits in `lit`.
    pub first_lamp: HashMap<TreeRef, usize>,
}

#[derive(Debug, PartialEq)]
pub struct SpanWiring {
    pub port: usize,
    /// Where the run's segment's lamp 0 sits among all the lamps.
    pub first_lamp: usize,
    pub lamps: Vec<(u32, u32)>,
}

impl Wiring {
    /// The (port, pixel) a lamp lands on, if it is patched to one.
    pub fn lamp(&self, segment: &TreeRef, index: u32) -> Option<(usize, usize)> {
        let first = *self.first_lamp.get(segment)?;
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

    /// Every segment in world units, kept.
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
            let tree = match render(&f.mapping) {
                Ok(tree) => tree,
                Err(why) => {
                    p.refused.insert(n.id.clone(), why.to_string());
                    continue;
                }
            };
            let frame = self.frame(&n.id);
            let base = p.list.len();
            for segment in drawn_segments(&n.id, &f.mapping, &tree) {
                p.list.push((n.id.clone(), in_world(segment, frame)));
            }
            for node in tree.nodes() {
                if !node.id.is_part_node() {
                    continue;
                }
                let under = p
                    .by_part
                    .entry(PartRef::new(&n.id, &node.id.part))
                    .or_default();
                under.extend(node.segments.clone().map(|s| base + s));
            }
            p.trees.insert(n.id.clone(), (Arc::new(tree), base));
        }
        for under in p.by_part.values_mut() {
            under.sort_unstable();
            under.dedup();
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
            p.bounds.push(segment_bounds(o));
        }
        p.lamps = lamps;
        for n in self.fixtures() {
            p.fixture_lamps.entry(n.id.clone()).or_insert(lamps..lamps);
        }
        let boxes: Vec<(PartRef, Rect)> = p
            .by_part
            .iter()
            .filter_map(|(part, under)| {
                let b = Rect::around(
                    under
                        .iter()
                        .flat_map(|&i| [p.bounds[i].min, p.bounds[i].max]),
                )?;
                Some((part.clone(), b))
            })
            .collect();
        p.part_bounds = boxes.into_iter().collect();
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
        for s in &patch.pixel_runs {
            let Some(at) = placed.index_of(&s.segment) else {
                w.pixel_runs.push(None);
                continue;
            };
            let Some(port) = w
                .ports
                .iter()
                .position(|(o, k, _)| *o == s.output && *k == s.port)
            else {
                w.pixel_runs.push(None);
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
            w.pixel_runs.push(Some(SpanWiring {
                port,
                first_lamp: first,
                lamps,
            }));
        }
        w
    }
}

/// The box round a segment's lamps and its drawn shape (a ring's whole
/// circle).
fn segment_bounds(o: &DrawnSegment) -> Rect {
    let shape: Vec<Vec2> = match &o.shape {
        DrawnShape::Polyline { points } => points.clone(),
        DrawnShape::Ring { center, radius } => {
            let r = Vec2::new(*radius, *radius);
            vec![*center - r, *center + r]
        }
        DrawnShape::Rings { center, radii } => {
            let outer = radii.iter().copied().fold(0.0, f64::max);
            let r = Vec2::new(outer, outer);
            vec![*center - r, *center + r]
        }
        DrawnShape::Filled { outline } => outline.clone(),
    };
    Rect::around(shape.into_iter().chain(o.lamps.iter().map(|l| l.pos)))
        .unwrap_or(Rect::new(Vec2::default(), Vec2::default()))
}
