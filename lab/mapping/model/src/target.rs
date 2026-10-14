//! Targets: every selectable thing, and the one tree they form.
//!
//! The UI shows ONE tree (D18): fixtures hold components, groups hold
//! components, a circle holds its rings, a ring holds its lamps. A line is
//! its own object, so its lamps sit directly under it. Lamps are part of the
//! tree for navigation (Enter, arrows, drill-down) but the tree *pane* stops
//! above them.
//!
//! On the canvas the fixture level is see-through, like a top-level frame in
//! Figma: a click inside a fixture selects what is in it. A fixture itself
//! is picked by its title or border, from the tree, or with Esc
//! ([`Project::pick_level`]).
//!
//! A target is a path of stable ids, so a selection survives edits: change a
//! line's count and lamp 7 is still lamp 7.

use crate::component::ComponentId;
use crate::fixture::FixtureId;
use crate::geom::Vec2;
use crate::object::{ObjectId, objects_of};
use crate::project::Project;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    /// A fixture: one physical thing, its box and what's in it.
    Fixture(FixtureId),
    /// An authored component: a line, a circle, a group.
    Component(ComponentId),
    /// A derived object of a multi-object component (a ring).
    Object(ObjectId),
    /// One lamp, by its index in its object's data order.
    Lamp { object: ObjectId, index: u32 },
}

impl Target {
    /// The authored component this target belongs to. Moving or editing a
    /// target always lands here (D11). A fixture has none: the grid places
    /// it.
    pub fn owner(&self) -> Option<&ComponentId> {
        match self {
            Target::Fixture(_) => None,
            Target::Component(id) => Some(id),
            Target::Object(o) | Target::Lamp { object: o, .. } => Some(&o.component),
        }
    }

    /// Authored by the user (a fixture or a component), not derived.
    pub fn is_authored(&self) -> bool {
        matches!(self, Target::Component(_) | Target::Fixture(_))
    }
}

impl Project {
    /// Does this target still exist?
    pub fn exists(&self, t: &Target) -> bool {
        match t {
            Target::Fixture(id) => self.fixture(id).is_some(),
            Target::Component(id) => self.component(id).is_some(),
            Target::Object(o) => self.object(o).is_some(),
            Target::Lamp { object, index } => self
                .object(object)
                .is_some_and(|o| (*index as usize) < o.lamps.len()),
        }
    }

    /// The fixture a target lives in.
    pub fn fixture_of_target(&self, t: &Target) -> Option<FixtureId> {
        match t {
            Target::Fixture(id) => Some(id.clone()),
            other => self.fixture_of(other.owner()?).map(|f| f.id.clone()),
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

    /// One level up; `None` means the project itself.
    pub fn parent(&self, t: &Target) -> Option<Target> {
        match t {
            Target::Fixture(_) => None,
            Target::Component(id) => match self.parent_group(id) {
                Some(group) => Some(Target::Component(group.clone())),
                None => self.fixture_of(id).map(|f| Target::Fixture(f.id.clone())),
            },
            Target::Object(o) => Some(Target::Component(o.component.clone())),
            Target::Lamp { object, .. } => Some(self.object_target(object)),
        }
    }

    /// One level down. `None` asks for the project's top level: fixtures.
    pub fn children(&self, t: Option<&Target>) -> Vec<Target> {
        let Some(t) = t else {
            return self
                .fixtures
                .iter()
                .map(|f| Target::Fixture(f.id.clone()))
                .collect();
        };
        match t {
            Target::Fixture(id) => self
                .fixture(id)
                .map(|f| {
                    f.components
                        .iter()
                        .map(|c| Target::Component(c.id.clone()))
                        .collect()
                })
                .unwrap_or_default(),
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
                let objects = objects_of(c);
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

    /// What a click (or a box) considers at a level. The same as
    /// [`children`](Self::children), except at the top: there the fixtures
    /// are see-through, so it is every fixture's own contents, plus the
    /// fixtures themselves (which only their title and border hit).
    pub fn pick_level(&self, level: Option<&Target>) -> Vec<Target> {
        match level {
            Some(t) => self.children(Some(t)),
            None => {
                let mut out = Vec::new();
                for f in &self.fixtures {
                    out.push(Target::Fixture(f.id.clone()));
                    out.extend(f.components.iter().map(|c| Target::Component(c.id.clone())));
                }
                out
            }
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

    /// Every lamp position under a target, in world units.
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
                let mut out: Vec<Vec2> = objects_of(c)
                    .iter()
                    .filter_map(|o| self.object(&o.id))
                    .flat_map(|o| o.lamps.into_iter().map(|l| l.pos))
                    .collect();
                for child in c.children() {
                    out.extend(self.lamps_under(&Target::Component(child.id.clone())));
                }
                out
            }
            Target::Fixture(_) => self
                .children(Some(t))
                .iter()
                .flat_map(|c| self.lamps_under(c))
                .collect(),
        }
    }

    /// What the user calls this target, short: `tail`, `circle1`, `ring r2`,
    /// `lamp 14`.
    pub fn label(&self, t: &Target) -> String {
        match t {
            Target::Fixture(id) => self
                .fixture(id)
                .map(|f| f.name.clone())
                .unwrap_or_else(|| id.to_string()),
            Target::Component(id) => id.to_string(),
            Target::Object(o) => format!("ring {}", o.key),
            Target::Lamp { index, .. } => format!("lamp {index}"),
        }
    }

    /// The label with enough context to stand alone: `ring r2 of circle1`,
    /// `lamp 14 of line3`.
    pub fn long_label(&self, t: &Target) -> String {
        match t {
            Target::Fixture(_) | Target::Component(_) => self.label(t),
            Target::Object(o) => format!("ring {} of {}", o.key, o.component),
            Target::Lamp { object, .. } => {
                let parent = self.object_target(object);
                format!("{} of {}", self.label(t), self.long_label(&parent))
            }
        }
    }

    /// The word for a target's kind, plural-able: "fixture", "group",
    /// "circle", "line", "ring", "lamp".
    pub fn kind_word(&self, t: &Target) -> &'static str {
        match t {
            Target::Fixture(_) => "fixture",
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
