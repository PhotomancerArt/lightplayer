//! Targets: every selectable thing, and the one tree they form.
//!
//! The UI shows ONE tree (D18): groups hold components, a circle holds its
//! rings, a ring holds its lamps. A line is its own object, so its lamps sit
//! directly under it. Lamps are part of the tree for navigation (Enter,
//! arrows, drill-down) but the tree *pane* stops above them.
//!
//! A target is a path of stable ids, so a selection survives edits: change a
//! line's count and lamp 7 is still lamp 7.

use crate::component::ComponentId;
use crate::fixture::Fixture;
use crate::geom::Vec2;
use crate::object::ObjectId;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    /// An authored component: a line, a circle, a group.
    Component(ComponentId),
    /// A derived object of a multi-object component (a ring).
    Object(ObjectId),
    /// One lamp, by its index in its object's data order.
    Lamp { object: ObjectId, index: u32 },
}

impl Target {
    /// The authored component this target belongs to. Moving or editing a
    /// target always lands here (D11).
    pub fn owner(&self) -> &ComponentId {
        match self {
            Target::Component(id) => id,
            Target::Object(o) | Target::Lamp { object: o, .. } => &o.component,
        }
    }

    pub fn is_authored(&self) -> bool {
        matches!(self, Target::Component(_))
    }
}

impl Fixture {
    /// Does this target still exist?
    pub fn exists(&self, t: &Target) -> bool {
        match t {
            Target::Component(id) => self.component(id).is_some(),
            Target::Object(o) => self.object(o).is_some(),
            Target::Lamp { object, index } => self
                .object(object)
                .is_some_and(|o| (*index as usize) < o.lamps.len()),
        }
    }

    /// The target that stands for an object in the tree: the component for
    /// a line (1:1), the object itself for a ring.
    pub fn object_target(&self, o: &ObjectId) -> Target {
        match self.component(&o.component) {
            Some(c) if c.is_single_object() => Target::Component(o.component.clone()),
            _ => Target::Object(o.clone()),
        }
    }

    /// One level up; `None` means the fixture itself.
    pub fn parent(&self, t: &Target) -> Option<Target> {
        match t {
            Target::Component(id) => self.parent_group(id).cloned().map(Target::Component),
            Target::Object(o) => Some(Target::Component(o.component.clone())),
            Target::Lamp { object, .. } => Some(self.object_target(object)),
        }
    }

    /// One level down. `None` asks for the fixture's top level.
    pub fn children(&self, t: Option<&Target>) -> Vec<Target> {
        let Some(t) = t else {
            return self
                .components
                .iter()
                .map(|c| Target::Component(c.id.clone()))
                .collect();
        };
        match t {
            Target::Component(id) => {
                let Some(c) = self.component(id) else {
                    return Vec::new();
                };
                if matches!(c.kind, crate::ComponentKind::Group { .. }) {
                    return c
                        .children()
                        .iter()
                        .map(|c| Target::Component(c.id.clone()))
                        .collect();
                }
                let objects = crate::object::objects_of(c);
                if c.is_single_object() {
                    objects
                        .first()
                        .map(|o| lamp_targets(&o.id, o.lamps.len()))
                        .unwrap_or_default()
                } else {
                    objects.into_iter().map(|o| Target::Object(o.id)).collect()
                }
            }
            Target::Object(o) => self
                .object(o)
                .map(|obj| lamp_targets(o, obj.lamps.len()))
                .unwrap_or_default(),
            Target::Lamp { .. } => Vec::new(),
        }
    }

    /// The path from the top level down to `t`, `t` last.
    pub fn chain(&self, t: &Target) -> Vec<Target> {
        let mut chain = vec![t.clone()];
        let mut cur = t.clone();
        while let Some(p) = self.parent(&cur) {
            chain.push(p.clone());
            cur = p;
        }
        chain.reverse();
        chain
    }

    /// Is `a` the same as `b`, or above it?
    pub fn is_ancestor_or_self(&self, a: &Target, b: &Target) -> bool {
        self.chain(b).contains(a)
    }

    /// The siblings of `t`, in order, including `t`.
    pub fn siblings(&self, t: &Target) -> Vec<Target> {
        self.children(self.parent(t).as_ref())
    }

    /// Every lamp position under a target (a group's are all its children's).
    pub fn lamps_under(&self, t: &Target) -> Vec<Vec2> {
        match t {
            Target::Lamp { object, index } => self
                .object(object)
                .and_then(|o| o.lamps.get(*index as usize).map(|l| l.pos))
                .into_iter()
                .collect(),
            Target::Object(o) => self
                .object(o)
                .map(|o| o.lamps.iter().map(|l| l.pos).collect())
                .unwrap_or_default(),
            Target::Component(id) => {
                let Some(c) = self.component(id) else {
                    return Vec::new();
                };
                let mut out: Vec<Vec2> = crate::object::objects_of(c)
                    .iter()
                    .flat_map(|o| o.lamps.iter().map(|l| l.pos))
                    .collect();
                for child in c.children() {
                    out.extend(self.lamps_under(&Target::Component(child.id.clone())));
                }
                out
            }
        }
    }

    /// What the user calls this target, short: `circle1`, `ring r2`,
    /// `lamp 14`.
    pub fn label(&self, t: &Target) -> String {
        match t {
            Target::Component(id) => id.to_string(),
            Target::Object(o) => format!("ring {}", o.key),
            Target::Lamp { index, .. } => format!("lamp {index}"),
        }
    }

    /// The label with enough context to stand alone: `ring r2 of circle1`,
    /// `lamp 14 of line3`.
    pub fn long_label(&self, t: &Target) -> String {
        match t {
            Target::Component(_) => self.label(t),
            Target::Object(o) => format!("ring {} of {}", o.key, o.component),
            Target::Lamp { object, .. } => {
                let parent = self.object_target(object);
                format!("{} of {}", self.label(t), self.long_label(&parent))
            }
        }
    }

    /// The word for a target's kind, plural-able: "group", "circle", "line",
    /// "ring", "lamp".
    pub fn kind_word(&self, t: &Target) -> &'static str {
        match t {
            Target::Component(id) => self.component(id).map(|c| c.kind_word()).unwrap_or("thing"),
            Target::Object(_) => "ring",
            Target::Lamp { .. } => "lamp",
        }
    }
}

fn lamp_targets(object: &ObjectId, n: usize) -> Vec<Target> {
    (0..n as u32)
        .map(|index| Target::Lamp {
            object: object.clone(),
            index,
        })
        .collect()
}
