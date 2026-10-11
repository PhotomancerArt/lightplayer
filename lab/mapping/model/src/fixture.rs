//! The fixture: a bounding box and the component tree. The one thing that
//! is authored and kept; everything else is derived from it.

use crate::component::{Component, ComponentId, ComponentKind};
use crate::geom::{Rect, Vec2};
use crate::object::{Object, ObjectId, objects_of};

#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    pub name: String,
    /// The fixture's extent. Lamps outside it are never lit (D4).
    pub bounds: Rect,
    /// Top-level components, in authoring order.
    pub components: Vec<Component>,
    /// Per kind, the next number to mint. Never decremented, so an id is
    /// never handed out twice.
    next_line: u32,
    next_circle: u32,
    next_group: u32,
}

impl Fixture {
    pub fn new(name: impl Into<String>, bounds: Rect) -> Self {
        Self {
            name: name.into(),
            bounds,
            components: Vec::new(),
            next_line: 1,
            next_circle: 1,
            next_group: 1,
        }
    }

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

    /// Mint and append a top-level component; returns its id.
    pub fn add(&mut self, kind: ComponentKind) -> ComponentId {
        let component = self.mint(kind);
        let id = component.id.clone();
        self.components.push(component);
        id
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

    /// The list `id` lives in — the top level or a group's children.
    pub fn siblings_of(&self, id: &ComponentId) -> &[Component] {
        match self.parent_group(id).cloned() {
            Some(group) => self.component(&group).map(|g| g.children()).unwrap_or(&[]),
            None => &self.components,
        }
    }

    /// Remove a component (and everything under it) wherever it lives.
    pub fn remove(&mut self, id: &ComponentId) -> Option<Component> {
        remove_from(&mut self.components, id)
    }

    /// Every object, in authoring order (depth-first through groups).
    pub fn objects(&self) -> Vec<Object> {
        let mut out = Vec::new();
        collect_objects(&self.components, &mut out);
        out
    }

    pub fn object(&self, id: &ObjectId) -> Option<Object> {
        let component = self.component(&id.component)?;
        objects_of(component).into_iter().find(|o| &o.id == id)
    }

    /// Lamps that fall outside the bounding box, as (object, lamp index).
    pub fn lamps_outside(&self) -> Vec<(ObjectId, u32)> {
        let mut out = Vec::new();
        for object in self.objects() {
            for (i, lamp) in object.lamps.iter().enumerate() {
                if !self.bounds.contains(lamp.pos) {
                    out.push((object.id.clone(), i as u32));
                }
            }
        }
        out
    }

    pub fn lamp_count(&self) -> usize {
        self.objects().iter().map(|o| o.lamps.len()).sum()
    }

    /// The lab's starting fixture: something like Yona's desk. A tail that
    /// runs out and back, a two-ring circle, and a lone strip.
    pub fn seed() -> Self {
        let mut f = Fixture::new(
            "desk",
            Rect::new(Vec2::new(0.0, 0.0), Vec2::new(120.0, 80.0)),
        );
        let out = f.mint(ComponentKind::line(
            Vec2::new(14.0, 70.0),
            Vec2::new(14.0, 12.0),
            25,
        ));
        let back = f.mint(ComponentKind::line(
            Vec2::new(22.0, 12.0),
            Vec2::new(22.0, 70.0),
            25,
        ));
        let tail = ComponentKind::group(vec![out, back]);
        f.add(tail);
        f.add(ComponentKind::circle(
            Vec2::new(70.0, 40.0),
            &[(26.0, 40), (16.0, 24)],
        ));
        f.add(ComponentKind::line(
            Vec2::new(40.0, 74.0),
            Vec2::new(108.0, 74.0),
            30,
        ));
        f
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

fn collect_objects(list: &[Component], out: &mut Vec<Object>) {
    for c in list {
        out.extend(objects_of(c));
        collect_objects(c.children(), out);
    }
}
