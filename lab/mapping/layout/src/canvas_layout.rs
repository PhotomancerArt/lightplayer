//! The canvas layout: automatic, by role (architecture §5.4).
//!
//! It works inside-out: each container (the root, then each module) lays
//! out its own children, then sits in its parent as one block. Inside any
//! container:
//!
//! 1. **Pinned nodes** sit where they were pinned, in the container's own
//!    coordinates. A pin inside a module never goes above or left of its
//!    inside. Everything else lays out around them (D64).
//! 2. **Controls** (a clock, a power button) sit in a row along the top.
//! 3. Below them, **columns that follow the flow**: sources, then
//!    fixtures, then outputs. Sources keep authoring order; fixtures go in
//!    the order of the first source feeding them, so lines cross as little
//!    as the columns allow.
//! 4. **Each output sits level with the fixture feeding it**, or with the
//!    middle of several; outputs push down to clear one another, so they
//!    keep the order of what feeds them.
//! 5. **Modules** stack below the pipeline, each a frame: its children's
//!    box plus padding, with room above for their title tabs.
//! 6. **Free nodes** (anything else) go in a column of their own at the
//!    right.
//!
//! Nothing at one level overlaps anything else at that level: whatever is
//! in a node's way moves it down until it's clear. The same input always
//! lays out the same way; nothing reads a clock or a hash map.

use std::collections::BTreeMap;

use crate::node_tree::NodeTree;
use crate::spacing::{COLUMN_GAP, EMPTY_MODULE, MODULE_PAD, MODULE_PAD_TOP, NODE_GAP};
use crate::{LayoutInput, LayoutRole, Placement, Rect, Vec2};

/// Lay `input` out on the canvas.
pub fn canvas_layout(input: &LayoutInput) -> Placement {
    let tree = NodeTree::new(input);
    let index: BTreeMap<&str, usize> = input
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let edges = input
        .edges
        .iter()
        .filter_map(|(a, b)| Some((*index.get(a.as_str())?, *index.get(b.as_str())?)))
        .collect();
    let mut pins = BTreeMap::new();
    for (id, at) in &input.pins {
        if let Some(&i) = index.get(id.as_str()) {
            pins.entry(i).or_insert(*at);
        }
    }
    let canvas = Canvas {
        input,
        tree,
        edges,
        pins,
    };
    let block = canvas.lay_out(None);
    let name = |i: usize| input.nodes[i].id.clone();
    Placement {
        rects: block.inner.into_iter().map(|(i, r)| (name(i), r)).collect(),
        insides: block.insides.into_iter().map(|(i, v)| (name(i), v)).collect(),
        extent: Rect::default(),
    }
    .with_extent()
}

struct Canvas<'a> {
    input: &'a LayoutInput,
    tree: NodeTree,
    /// Dataflow, by index into the input's nodes.
    edges: Vec<(usize, usize)>,
    pins: BTreeMap<usize, Vec2>,
}

/// A container's insides laid out, or one node before it is placed.
struct Block {
    size: Vec2,
    /// The box of every node inside it, relative to its top-left.
    inner: Vec<(usize, Rect)>,
    /// Where each module inside it starts its inside, relative to its
    /// top-left.
    insides: Vec<(usize, Vec2)>,
}

impl Block {
    /// A box inside this block once the block's top-left is at `at`.
    fn inner_rect(&self, node: usize, at: Vec2) -> Option<Rect> {
        self.inner
            .iter()
            .find(|(i, _)| *i == node)
            .map(|(_, r)| r.shifted(at))
    }
}

/// One container's children while they are being placed.
struct Level<'b> {
    kids: &'b [usize],
    blocks: Vec<Block>,
    placed: Vec<Option<Rect>>,
}

impl Level<'_> {
    /// Those of `ks` not placed yet, in order.
    fn unplaced(&self, ks: Vec<usize>) -> Vec<usize> {
        ks.into_iter().filter(|&k| self.placed[k].is_none()).collect()
    }

    fn taken(&self) -> Vec<Rect> {
        self.placed.iter().flatten().copied().collect()
    }

    fn place(&mut self, k: usize, want: Vec2) -> Rect {
        let at = clear_of(want, self.blocks[k].size, &self.taken());
        let r = Rect::at(at, self.blocks[k].size);
        self.placed[k] = Some(r);
        r
    }

    /// The right edge of the placed kids for which `pick` holds; `None`
    /// if there are none.
    fn right_of(&self, pick: impl Fn(usize) -> bool) -> Option<f64> {
        (0..self.kids.len())
            .filter(|&k| pick(k))
            .filter_map(|k| self.placed[k])
            .map(|r| r.max.x)
            .reduce(f64::max)
    }

    fn bottom(&self) -> Option<f64> {
        self.taken().iter().map(|r| r.max.y).reduce(f64::max)
    }
}

impl Canvas<'_> {
    fn role(&self, i: usize) -> LayoutRole {
        self.input.nodes[i].role
    }

    /// Lay out the children of `container` (the root when `None`), its
    /// inside's top-left at `(0, 0)`.
    fn lay_out(&self, container: Option<usize>) -> Block {
        let kids = self.tree.children_of(container);
        let mut level = Level {
            kids,
            blocks: kids.iter().map(|&i| self.block(i)).collect(),
            placed: vec![None; kids.len()],
        };
        let of_role = |role: LayoutRole| -> Vec<usize> {
            (0..kids.len())
                .filter(|&k| self.role(kids[k]) == role)
                .collect()
        };

        // 1. Pins, where they were pinned.
        for k in 0..kids.len() {
            if let Some(&pin) = self.pins.get(&kids[k]) {
                let pin = if container.is_some() {
                    Vec2::new(pin.x.max(0.0), pin.y.max(0.0))
                } else {
                    pin
                };
                level.place(k, pin);
            }
        }

        // 2. Controls, in a row along the top.
        let mut x = 0.0;
        let mut row_bottom = None::<f64>;
        for k in level.unplaced(of_role(LayoutRole::Control)) {
            let r = level.place(k, Vec2::new(x, 0.0));
            x = r.max.x + NODE_GAP;
            row_bottom = Some(row_bottom.map_or(r.max.y, |b| b.max(r.max.y)));
        }
        let top = row_bottom.map_or(0.0, |b| b + NODE_GAP);

        // 3. Sources, then fixtures, in columns.
        let mut y = top;
        for k in level.unplaced(of_role(LayoutRole::Source)) {
            y = level.place(k, Vec2::new(0.0, y)).max.y + NODE_GAP;
        }
        let sources = of_role(LayoutRole::Source);
        let fixture_x = level
            .right_of(|k| sources.contains(&k))
            .map_or(0.0, |r| r + COLUMN_GAP);
        let mut fixtures: Vec<usize> = level.unplaced(of_role(LayoutRole::Fixture));
        fixtures.sort_by_key(|&k| (self.first_source(kids, &sources, kids[k]), k));
        let mut y = top;
        for k in fixtures {
            y = level.place(k, Vec2::new(fixture_x, y)).max.y + NODE_GAP;
        }

        // 4. Outputs whose feeders are all in the columns, level with them.
        let pipeline = |k: usize| {
            matches!(
                self.role(kids[k]),
                LayoutRole::Source | LayoutRole::Fixture
            )
        };
        let output_x = level.right_of(pipeline).map_or(0.0, |r| r + COLUMN_GAP);
        let outputs: Vec<usize> = level.unplaced(of_role(LayoutRole::Output));
        let (now, later): (Vec<usize>, Vec<usize>) = outputs.into_iter().partition(|&k| {
            let feeders = self.feeders(&level, container, k);
            !feeders.is_empty() && feeders.iter().all(|&f| level.placed[f].is_some())
        });
        self.place_outputs(&mut level, container, now, top, output_x);

        // 5. Modules, stacked below the pipeline.
        let mut y = level.bottom().map_or(0.0, |b| b + NODE_GAP);
        for k in level.unplaced(of_role(LayoutRole::Module)) {
            y = level.place(k, Vec2::new(0.0, y)).max.y + NODE_GAP;
        }

        // 4, again: outputs fed from inside a module, then outputs nothing
        // feeds, in the output column.
        let (fed, unfed): (Vec<usize>, Vec<usize>) = later
            .into_iter()
            .partition(|&k| !self.feeders(&level, container, k).is_empty());
        self.place_outputs(&mut level, container, fed, top, output_x);
        for k in unfed {
            level.place(k, Vec2::new(output_x, top));
        }

        // 6. Free nodes, in a column at the right.
        let free_x = level.right_of(|_| true).map_or(0.0, |r| r + COLUMN_GAP);
        for k in (0..kids.len())
            .filter(|&k| level.placed[k].is_none())
            .collect::<Vec<_>>()
        {
            level.place(k, Vec2::new(free_x, top));
        }

        let mut out = Block {
            size: Vec2::ZERO,
            inner: Vec::new(),
            insides: Vec::new(),
        };
        let Level { blocks, placed, .. } = level;
        for (k, block) in blocks.into_iter().enumerate() {
            let Some(r) = placed[k] else { continue };
            out.size = Vec2::new(out.size.x.max(r.max.x), out.size.y.max(r.max.y));
            out.inner.push((kids[k], r));
            out.inner
                .extend(block.inner.into_iter().map(|(i, x)| (i, x.shifted(r.min))));
            out.insides
                .extend(block.insides.into_iter().map(|(i, v)| (i, v + r.min)));
        }
        out
    }

    /// One child as a block: a module laid out inside its frame, or a node
    /// at its own size.
    fn block(&self, i: usize) -> Block {
        if self.role(i) != LayoutRole::Module {
            return Block {
                size: self.input.nodes[i].size,
                inner: Vec::new(),
                insides: Vec::new(),
            };
        }
        let mut inside = self.lay_out(Some(i));
        if inside.inner.is_empty() {
            inside.size = EMPTY_MODULE;
        }
        let pad = Vec2::new(MODULE_PAD, MODULE_PAD_TOP);
        let mut insides = vec![(i, pad)];
        insides.extend(inside.insides.into_iter().map(|(n, v)| (n, v + pad)));
        Block {
            size: inside.size + Vec2::new(2.0 * MODULE_PAD, MODULE_PAD_TOP + MODULE_PAD),
            inner: inside
                .inner
                .into_iter()
                .map(|(n, r)| (n, r.shifted(pad)))
                .collect(),
            insides,
        }
    }

    /// The column position of the first source feeding fixture `node`, for
    /// ordering the fixtures; past the end when no source here feeds it.
    fn first_source(&self, kids: &[usize], sources: &[usize], node: usize) -> usize {
        self.edges
            .iter()
            .filter(|&&(_, to)| to == node)
            .filter_map(|&(from, _)| sources.iter().position(|&k| kids[k] == from))
            .min()
            .unwrap_or(usize::MAX)
    }

    /// The children of this container that hold what feeds output `k`.
    fn feeders(&self, level: &Level, container: Option<usize>, k: usize) -> Vec<usize> {
        let node = level.kids[k];
        let mut out: Vec<usize> = self
            .edges
            .iter()
            .filter(|&&(from, to)| to == node && from != node)
            .filter_map(|&(from, _)| self.tree.at_level(from, container))
            .filter_map(|n| level.kids.iter().position(|&x| x == n))
            .filter(|&f| f != k)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The boxes, in this container's coordinates, of what feeds output
    /// `k`: each feeding node's own box, even inside a module.
    fn feeder_rects(&self, level: &Level, container: Option<usize>, k: usize) -> Vec<Rect> {
        let node = level.kids[k];
        self.edges
            .iter()
            .filter(|&&(from, to)| to == node && from != node)
            .filter_map(|&(from, _)| {
                let at = self.tree.at_level(from, container)?;
                let f = level.kids.iter().position(|&x| x == at)?;
                let block_at = level.placed[f]?;
                if at == from {
                    Some(block_at)
                } else {
                    level.blocks[f].inner_rect(from, block_at.min)
                }
            })
            .collect()
    }

    /// Place outputs level with the middle of what feeds them, in that
    /// order, each clear of the ones before it.
    fn place_outputs(
        &self,
        level: &mut Level,
        container: Option<usize>,
        outputs: Vec<usize>,
        top: f64,
        column_x: f64,
    ) {
        let mut wants: Vec<(Vec2, usize)> = outputs
            .into_iter()
            .map(|k| {
                let rects = self.feeder_rects(level, container, k);
                let span_top = rects.iter().map(|r| r.min.y).fold(f64::INFINITY, f64::min);
                let span_bottom = rects.iter().map(|r| r.max.y).fold(f64::NEG_INFINITY, f64::max);
                let middle = (span_top + span_bottom) / 2.0;
                let y = (middle - level.blocks[k].size.y / 2.0).max(top);
                let beside = self
                    .feeders(level, container, k)
                    .iter()
                    .filter_map(|&f| level.placed[f])
                    .map(|r| r.max.x + COLUMN_GAP)
                    .fold(column_x, f64::max);
                (Vec2::new(beside, y), k)
            })
            .collect();
        wants.sort_by(|a, b| a.0.y.total_cmp(&b.0.y).then(a.1.cmp(&b.1)));
        for (want, k) in wants {
            level.place(k, want);
        }
    }
}

/// The first place at or below `want` where a box of `size` overlaps
/// nothing `taken`: it steps down past whatever is in the way, a
/// [`NODE_GAP`] below it.
fn clear_of(want: Vec2, size: Vec2, taken: &[Rect]) -> Vec2 {
    let mut at = want;
    loop {
        let r = Rect::at(at, size);
        let blocking = taken
            .iter()
            .filter(|t| r.overlaps(t))
            .map(|t| t.max.y)
            .reduce(f64::max);
        match blocking {
            Some(bottom) => at.y = bottom + NODE_GAP,
            None => return at,
        }
    }
}
