//! A fixture: one physical thing (a tail, a ring display, a dome), with its
//! own bounding box and component tree. Its coordinates are its own:
//! `(0, 0)` is the top-left of its box, wherever the layout puts it.
//!
//! A fixture is one kind of node (`node.rs`); its id and name live on the
//! node. What it carries of its own is its drawing, its patch rules and
//! the visual channel it reads. Each frame it samples that channel's
//! render product at its lamps and publishes their colours as its control
//! product (`board.rs`), as the product's fixture node does.
//!
//! The control channel it writes is its auto rule, read the product's way:
//! the fixture writes the channel its rule's output reads (fit-check §6,
//! `bindings.rs`). So a fixture names no channel of its own for that.

use crate::domain::channel::Channel;
use crate::domain::color_order::ColorOrder;
use crate::domain::component::{Component, ComponentId};
use crate::domain::geom::{Rect, Vec2};
use crate::domain::patch::PatchRule;

#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    /// The fixture's extent, from `(0, 0)`. Lamps outside it are never lit
    /// (D4).
    pub size: Vec2,
    /// Top-level components, in authoring order.
    pub components: Vec<Component>,
    /// Where its lamps go, in order. The patch they make is derived
    /// (`patch.rs`), never stored.
    pub rules: Vec<PatchRule>,
    /// Which byte of each of its pixels is red, green and blue.
    pub color_order: ColorOrder,
    /// The visual channel whose render product it samples (the product's
    /// `bindings.input.source`).
    pub reads: Channel,
    /// What in its drawing the lab can't hold yet (`grid`, `filled
    /// polygon`, `ring of several rings`), when it holds one. Such a
    /// fixture opens as its box alone: nothing is drawn in it, nothing can
    /// be drawn into it or resized, and its files are never written.
    pub unsupported: Option<String>,
}

impl Fixture {
    /// A new fixture's side: it starts square, this big each way, until
    /// you resize it.
    pub const DEFAULT_SIDE: f64 = 100.0;
    /// The smallest a fixture gets, either way.
    pub const MIN_SIDE: f64 = 4.0;

    /// A new, empty, square fixture.
    pub fn square() -> Self {
        Self::new(Vec2::new(Self::DEFAULT_SIDE, Self::DEFAULT_SIDE))
    }

    pub fn new(size: Vec2) -> Self {
        Self {
            size,
            components: Vec::new(),
            rules: Vec::new(),
            color_order: ColorOrder::default(),
            reads: Channel::visual_out(),
            unsupported: None,
        }
    }

    /// The box in the fixture's own coordinates.
    pub fn bounds(&self) -> Rect {
        Rect::new(Vec2::default(), self.size)
    }

    /// Where a point of the fixture (its own coordinates) falls in its box,
    /// as the unit square a render product is sampled in: `(0, 0)` the
    /// top-left, `(1, 1)` the bottom-right.
    pub fn uv(&self, at: Vec2) -> Vec2 {
        Vec2::new(at.x / self.size.x.max(1e-9), at.y / self.size.y.max(1e-9))
    }

    pub fn component(&self, id: &ComponentId) -> Option<&Component> {
        find(&self.components, id)
    }

    pub fn component_mut(&mut self, id: &ComponentId) -> Option<&mut Component> {
        find_mut(&mut self.components, id)
    }

    /// The group holding `id`, or `None` when `id` is top-level (or absent).
    pub fn parent_group(&self, id: &ComponentId) -> Option<&ComponentId> {
        parent_of(&self.components, id, None)
    }

    /// Remove a component (and everything under it) wherever it lives.
    pub fn remove(&mut self, id: &ComponentId) -> Option<Component> {
        remove_from(&mut self.components, id)
    }
}

fn find<'a>(list: &'a [Component], id: &ComponentId) -> Option<&'a Component> {
    for c in list {
        if &c.id == id {
            return Some(c);
        }
        if let Some(found) = find(c.children(), id) {
            return Some(found);
        }
    }
    None
}

fn find_mut<'a>(list: &'a mut [Component], id: &ComponentId) -> Option<&'a mut Component> {
    for c in list {
        if &c.id == id {
            return Some(c);
        }
        if let Some(children) = c.children_mut()
            && let Some(found) = find_mut(children, id)
        {
            return Some(found);
        }
    }
    None
}

fn parent_of<'a>(
    list: &'a [Component],
    id: &ComponentId,
    parent: Option<&'a ComponentId>,
) -> Option<&'a ComponentId> {
    for c in list {
        if &c.id == id {
            return parent;
        }
        if let Some(found) = parent_of(c.children(), id, Some(&c.id)) {
            return Some(found);
        }
    }
    None
}

fn remove_from(list: &mut Vec<Component>, id: &ComponentId) -> Option<Component> {
    if let Some(i) = list.iter().position(|c| &c.id == id) {
        return Some(list.remove(i));
    }
    for c in list.iter_mut() {
        if let Some(children) = c.children_mut()
            && let Some(found) = remove_from(children, id)
        {
            return Some(found);
        }
    }
    None
}
