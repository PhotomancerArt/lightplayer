//! A fixture: one physical thing (a tail, a ring display, a dome), with its
//! own bounding box and component tree. Its coordinates are its own:
//! `(0, 0)` is the top-left of its box, wherever the canvas lays it out.

use crate::component::{Component, ComponentId};
use crate::geom::{Rect, Vec2};

/// A fixture's id, minted by the project (`fixture1`, …) and never reused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FixtureId(pub String);

impl FixtureId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for FixtureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    pub id: FixtureId,
    pub name: String,
    /// The fixture's extent, from `(0, 0)`. Lamps outside it are never lit
    /// (D4).
    pub size: Vec2,
    /// Top-level components, in authoring order.
    pub components: Vec<Component>,
}

impl Fixture {
    /// The box in the fixture's own coordinates.
    pub fn bounds(&self) -> Rect {
        Rect::new(Vec2::default(), self.size)
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
