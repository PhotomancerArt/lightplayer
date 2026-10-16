//! Targets: every selectable thing, and the one tree they form.
//!
//! The UI shows ONE tree (D18): the project holds nodes, a module holds
//! nodes, a fixture holds components, groups hold components, a circle
//! holds its rings, a ring holds its lamps; an output holds its ports, and
//! a port its pixels. A path is its own object, so its lamps sit directly
//! under it. Lamps and pixels are part of the tree for navigation (Enter,
//! arrows, drill-down) but the tree *pane* stops above them.
//!
//! On the canvas a frame — a module, a fixture — is see-through, like a
//! frame in Figma: a click inside it selects what is in it. A frame itself
//! is picked by its title or border, from the tree, or with Esc
//! ([`Project::pick_level`]). An output card is not a frame: a click on it
//! selects the output, the next goes into its ports, and the next into the
//! port's pixels (D47) — the hardware, reachable like anything else.
//!
//! A target is a path of stable ids, so a selection survives edits: change a
//! path's count and lamp 7 is still lamp 7.

use crate::component::ComponentId;
use crate::geom::Vec2;
use crate::node::{NodeId, NodeKind};
use crate::object::ObjectId;
use crate::project::Project;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    /// A node: a module, a fixture, an output.
    Node(NodeId),
    /// An authored component: a path, a circle, a group.
    Component(ComponentId),
    /// A derived object of a multi-object component (a ring).
    Object(ObjectId),
    /// One lamp, by its index in its object's data order.
    Lamp { object: ObjectId, index: u32 },
    /// One port of an output, by the key its output minted.
    Port { output: NodeId, key: String },
    /// One pixel of a port, by its index on the strip (D47). Like a lamp,
    /// it comes from a count: the port's.
    Pixel {
        output: NodeId,
        key: String,
        index: u32,
    },
}

impl Target {
    /// The authored component this target belongs to. Moving or editing a
    /// target always lands here (D11). Nodes and ports have none: the
    /// layout places them.
    pub fn owner(&self) -> Option<&ComponentId> {
        match self {
            Target::Node(_) | Target::Port { .. } | Target::Pixel { .. } => None,
            Target::Component(id) => Some(id),
            Target::Object(o) | Target::Lamp { object: o, .. } => Some(&o.component),
        }
    }

    /// Authored by the user, not derived from something else.
    pub fn is_authored(&self) -> bool {
        matches!(
            self,
            Target::Component(_) | Target::Node(_) | Target::Port { .. }
        )
    }

    /// The port a pixel is on, as a target.
    pub fn port_of_pixel(&self) -> Option<Target> {
        match self {
            Target::Pixel { output, key, .. } => Some(Target::Port {
                output: output.clone(),
                key: key.clone(),
            }),
            _ => None,
        }
    }

    pub fn node(&self) -> Option<&NodeId> {
        match self {
            Target::Node(id) => Some(id),
            _ => None,
        }
    }
}

impl Project {
    /// Does this target still exist?
    pub fn exists(&self, t: &Target) -> bool {
        match t {
            Target::Node(id) => self.node(id).is_some(),
            Target::Component(id) => self.component(id).is_some(),
            Target::Object(o) => self.object(o).is_some(),
            Target::Lamp { object, index } => self
                .object(object)
                .is_some_and(|o| (*index as usize) < o.lamps.len()),
            Target::Port { output, key } => {
                self.output(output).is_some_and(|o| o.port(key).is_some())
            }
            Target::Pixel { output, key, index } => {
                self.output(output).and_then(|o| o.port(key)).is_some()
                    && *index < self.port_pixels(output, key)
            }
        }
    }

    /// A module or a fixture: see-through, picked by title or border.
    pub fn is_frame(&self, t: &Target) -> bool {
        match t {
            Target::Node(id) => self.node(id).is_some_and(|n| n.is_frame()),
            _ => false,
        }
    }

    pub fn is_fixture(&self, t: &Target) -> bool {
        matches!(t, Target::Node(id) if self.fixture(id).is_some())
    }

    pub fn is_output(&self, t: &Target) -> bool {
        matches!(t, Target::Node(id) if self.output(id).is_some())
    }

    /// The fixture a target lives in (or is).
    pub fn fixture_of_target(&self, t: &Target) -> Option<NodeId> {
        match t {
            Target::Node(id) => self.fixture(id).map(|_| id.clone()),
            Target::Port { .. } | Target::Pixel { .. } => None,
            other => self.fixture_of(other.owner()?).map(|n| n.id.clone()),
        }
    }

    /// The target that stands for an object in the tree: the component for
    /// a path (1:1), the object itself for a ring.
    pub fn object_target(&self, o: &ObjectId) -> Target {
        match self.component(&o.component) {
            Some(c) if c.is_single_object() => Target::Component(o.component.clone()),
            _ => Target::Object(o.clone()),
        }
    }

    /// One level up; `None` means the project itself.
    pub fn parent(&self, t: &Target) -> Option<Target> {
        match t {
            Target::Node(id) => self.parent_module(id).cloned().map(Target::Node),
            Target::Component(id) => match self.parent_group(id) {
                Some(group) => Some(Target::Component(group.clone())),
                None => self.fixture_of(id).map(|n| Target::Node(n.id.clone())),
            },
            Target::Object(o) => Some(Target::Component(o.component.clone())),
            Target::Lamp { object, .. } => Some(self.object_target(object)),
            Target::Port { output, .. } => Some(Target::Node(output.clone())),
            Target::Pixel { .. } => t.port_of_pixel(),
        }
    }

    /// One level down. `None` asks for the project's top level: its nodes.
    pub fn children(&self, t: Option<&Target>) -> Vec<Target> {
        let Some(t) = t else {
            return self
                .nodes()
                .iter()
                .map(|n| Target::Node(n.id.clone()))
                .collect();
        };
        match t {
            Target::Node(id) => match self.node(id).map(|n| &n.kind) {
                Some(NodeKind::Module { children }) => children
                    .iter()
                    .map(|n| Target::Node(n.id.clone()))
                    .collect(),
                Some(NodeKind::Fixture(f)) => f
                    .components
                    .iter()
                    .map(|c| Target::Component(c.id.clone()))
                    .collect(),
                Some(NodeKind::Output(o)) => o
                    .ports
                    .iter()
                    .map(|p| Target::Port {
                        output: id.clone(),
                        key: p.key.clone(),
                    })
                    .collect(),
                Some(NodeKind::Source(_) | NodeKind::Other(_)) | None => Vec::new(),
            },
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
                let mut objects = self.objects_of(id);
                if c.is_single_object() {
                    objects
                        .next()
                        .map(|o| lamp_targets(&o.id, o.lamps.len()))
                        .unwrap_or_default()
                } else {
                    objects.map(|o| Target::Object(o.id.clone())).collect()
                }
            }
            Target::Object(o) => self
                .object(o)
                .map(|obj| lamp_targets(o, obj.lamps.len()))
                .unwrap_or_default(),
            Target::Port { output, key } => {
                let n = self.port_pixels(output, key);
                (0..n)
                    .map(|index| Target::Pixel {
                        output: output.clone(),
                        key: key.clone(),
                        index,
                    })
                    .collect()
            }
            Target::Lamp { .. } | Target::Pixel { .. } => Vec::new(),
        }
    }

    /// What a click (or a box) considers at a level: the level's children,
    /// except that frames are see-through — a frame brings its contents
    /// along, all the way down through modules, and stands there itself
    /// too (which only its title and border hit).
    pub fn pick_level(&self, level: Option<&Target>) -> Vec<Target> {
        let mut out = Vec::new();
        for t in self.children(level) {
            self.see_through(t, &mut out);
        }
        out
    }

    fn see_through(&self, t: Target, out: &mut Vec<Target>) {
        let frame = self.is_frame(&t);
        out.push(t.clone());
        if frame {
            for c in self.children(Some(&t)) {
                self.see_through(c, out);
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
            Target::Component(id) => self
                .objects_under(id)
                .into_iter()
                .flat_map(|o| o.lamps.iter().map(|l| l.pos))
                .collect(),
            Target::Node(_) => self
                .children(Some(t))
                .iter()
                .flat_map(|c| self.lamps_under(c))
                .collect(),
            Target::Port { .. } | Target::Pixel { .. } => Vec::new(),
        }
    }

    /// How many lamps are under a target: `lamps_under(t).len()`, counted
    /// without gathering them.
    pub fn lamp_count_under(&self, t: &Target) -> usize {
        match t {
            Target::Lamp { .. } => self.lamps_under(t).len(),
            Target::Object(o) => self.object(o).map_or(0, |o| o.lamps.len()),
            Target::Component(id) => self.objects_under(id).iter().map(|o| o.lamps.len()).sum(),
            Target::Node(_) => self
                .children(Some(t))
                .iter()
                .map(|c| self.lamp_count_under(c))
                .sum(),
            Target::Port { .. } | Target::Pixel { .. } => 0,
        }
    }

    /// What the user calls this target, short: `tail`, `circle1`, `ring r2`,
    /// `lamp 14`, `D0`, `pixel 7`.
    pub fn label(&self, t: &Target) -> String {
        match t {
            Target::Node(id) => self.node_name(id),
            Target::Component(id) => id.to_string(),
            Target::Object(o) => format!("{} {}", self.object_word(o), o.key),
            Target::Lamp { index, .. } => format!("lamp {index}"),
            Target::Port { output, key } => self
                .output(output)
                .and_then(|o| o.port(key))
                .map(|p| p.name.clone())
                .unwrap_or_else(|| key.clone()),
            Target::Pixel { index, .. } => format!("pixel {index}"),
        }
    }

    /// The label with enough context to stand alone: `ring r2 of circle1`,
    /// `lamp 14 of line3`, `D0 of main`, `pixel 7 of D0 of main`.
    pub fn long_label(&self, t: &Target) -> String {
        match t {
            Target::Node(_) | Target::Component(_) => self.label(t),
            Target::Object(o) => format!("{} {} of {}", self.object_word(o), o.key, o.component),
            Target::Lamp { object, .. } => {
                let parent = self.object_target(object);
                format!("{} of {}", self.label(t), self.long_label(&parent))
            }
            Target::Port { output, .. } => {
                format!("{} of {}", self.label(t), self.node_name(output))
            }
            Target::Pixel { .. } => match t.port_of_pixel() {
                Some(port) => format!("{} of {}", self.label(t), self.long_label(&port)),
                None => self.label(t),
            },
        }
    }

    /// What one of a component's objects is called: a circle's are rings,
    /// a repeat's are copies.
    pub fn object_word(&self, o: &ObjectId) -> &'static str {
        match self.component(&o.component).map(|c| &c.kind) {
            Some(crate::ComponentKind::Repeat { .. }) => "copy",
            _ => "ring",
        }
    }

    /// The word for a target's kind, plural-able: "module", "fixture",
    /// "output", "port", "pixel", "group", "circle", "path", "polygon",
    /// "repeat", "ring", "copy", "lamp".
    pub fn kind_word(&self, t: &Target) -> &'static str {
        match t {
            Target::Node(id) => self.node(id).map(|n| n.kind_word()).unwrap_or("node"),
            Target::Component(id) => self.component(id).map(|c| c.kind_word()).unwrap_or("thing"),
            Target::Object(o) => self.object_word(o),
            Target::Lamp { .. } => "lamp",
            Target::Port { .. } => "port",
            Target::Pixel { .. } => "pixel",
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
