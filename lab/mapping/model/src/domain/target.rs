//! Targets: every selectable thing, and the one tree they form.
//!
//! The UI shows ONE tree (D18): the project holds nodes, a module holds
//! nodes, a fixture holds parts, groups hold parts, a circle holds its
//! rings, a repeat its copies, a segment holds its lamps; an output holds
//! its ports, and a port its spans (`span.rs`: a segment's pixels on it, or
//! a gap), a span its pixels. A path is its own segment, so its lamps sit
//! directly under it. Lamps and pixels are part of the tree for navigation
//! (Enter, arrows, drill-down) but the tree *pane* stops above them.
//!
//! On the canvas a frame — a module, a fixture — is see-through, like a
//! frame in Figma: a click inside it selects what is in it. A frame itself
//! is picked by its title or border, from the tree, or with Esc
//! ([`Project::pick_level`]). An output card is not a frame, but its spans
//! are picked directly, as a fixture's contents are (plan D13): a click on
//! a span selects it, the next goes into its pixels (D47) — the hardware,
//! reachable like anything else — and the card's title selects the output.
//!
//! A target is a path of stable ids, so a selection survives edits: change a
//! path's count and lamp 7 is still lamp 7.

use crate::domain::geom::Vec2;
use crate::domain::node::{NodeId, NodeKind};
use crate::domain::part_ref::{PartRef, TreeRef};
use crate::domain::project::Project;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    /// A node: a module, a fixture, an output.
    Node(NodeId),
    /// An authored part, as itself (copy 0): a path, a circle, a group.
    Part(PartRef),
    /// A node a part generated: a ring, a subpath, a repeat's copy (and
    /// anything in a copy).
    Generated(TreeRef),
    /// One lamp, by its index in its segment's data order.
    Lamp { segment: TreeRef, index: u32 },
    /// One port of an output, by the key its output minted.
    Port { output: NodeId, key: String },
    /// A span of a port (`span.rs`): one segment's pixels on it, by its
    /// segment (`start` 0), or a gap, by where it starts.
    Span {
        output: NodeId,
        key: String,
        segment: Option<TreeRef>,
        start: u32,
    },
    /// One pixel of a port, by its index on the strip (D47). Like a lamp,
    /// it comes from a count: the port's.
    Pixel {
        output: NodeId,
        key: String,
        index: u32,
    },
}

impl Target {
    /// The authored part this target belongs to. Moving or editing a
    /// target always lands here (D11). Nodes and ports have none: the
    /// layout places them.
    pub fn owner(&self) -> Option<PartRef> {
        match self {
            Target::Node(_) | Target::Port { .. } | Target::Span { .. } | Target::Pixel { .. } => {
                None
            }
            Target::Part(id) => Some(id.clone()),
            Target::Generated(o) | Target::Lamp { segment: o, .. } => Some(o.part()),
        }
    }

    /// Authored by the user, not derived from something else.
    pub fn is_authored(&self) -> bool {
        matches!(
            self,
            Target::Part(_) | Target::Node(_) | Target::Port { .. }
        )
    }

    /// The port a pixel or a span is on, as a target.
    pub fn port_of_pixel(&self) -> Option<Target> {
        match self {
            Target::Pixel { output, key, .. } | Target::Span { output, key, .. } => {
                Some(Target::Port {
                    output: output.clone(),
                    key: key.clone(),
                })
            }
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
            Target::Part(id) => self.part(id).is_some(),
            Target::Generated(o) => self.placed().node(o).is_some(),
            Target::Lamp { segment, index } => self
                .segment(segment)
                .is_some_and(|o| (*index as usize) < o.lamps.len()),
            Target::Port { output, key } => {
                self.output(output).is_some_and(|o| o.port(key).is_some())
            }
            Target::Span { .. } => self.span_of(t).is_some(),
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
            Target::Port { .. } | Target::Span { .. } | Target::Pixel { .. } => None,
            other => Some(other.owner()?.fixture),
        }
    }

    /// The target that stands for a tree node: its part when it is the
    /// part's own node in copy 0 (a path, a circle, copy 0 of a repeat's
    /// part), the generated node otherwise (a ring, a copy `@k`).
    pub fn tree_target(&self, o: &TreeRef) -> Target {
        if o.is_part_node() {
            Target::Part(o.part())
        } else {
            Target::Generated(o.clone())
        }
    }

    /// The tree node a target stands for, when it stands for one: a part's
    /// own node, or a generated one.
    pub fn tree_node_of_target(&self, t: &Target) -> Option<TreeRef> {
        match t {
            Target::Part(id) => Some(id.node()),
            Target::Generated(o) => Some(o.clone()),
            _ => None,
        }
    }

    /// The targets for a tree node's children: lamps under a segment, tree
    /// nodes under anything else.
    fn tree_children(&self, o: &TreeRef) -> Vec<Target> {
        let Some((_, node)) = self.placed().node(o) else {
            return Vec::new();
        };
        if node.is_leaf() {
            let n = self.segment(o).map_or(0, |s| s.lamps.len());
            return lamp_targets(o, n);
        }
        node.children()
            .iter()
            .map(|c| self.tree_target(c))
            .collect()
    }

    /// One level up; `None` means the project itself.
    pub fn parent(&self, t: &Target) -> Option<Target> {
        match t {
            Target::Node(id) => self.parent_module(id).cloned().map(Target::Node),
            Target::Part(id) => match self.parent_part(id) {
                Some(parent) => Some(Target::Part(parent)),
                None => self
                    .fixture(&id.fixture)
                    .map(|_| Target::Node(id.fixture.clone())),
            },
            Target::Generated(o) => self.placed().parent_of(o).map(|p| self.tree_target(&p)),
            Target::Lamp { segment, .. } => Some(self.tree_target(segment)),
            Target::Port { output, .. } => Some(Target::Node(output.clone())),
            Target::Span { .. } => t.port_of_pixel(),
            Target::Pixel { output, key, index } => {
                // The span the pixel is in: a segment's first, else its gap.
                let spans = self.spans(output, key);
                let span = spans
                    .iter()
                    .find(|s| s.segment.is_some() && s.pixels.contains(index))
                    .or_else(|| spans.iter().find(|s| s.pixels.contains(index)));
                span.map(|s| s.target()).or_else(|| t.port_of_pixel())
            }
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
                    .parts()
                    .iter()
                    .map(|c| Target::Part(PartRef::new(id, &c.id)))
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
            Target::Part(id) => self.tree_children(&id.node()),
            Target::Generated(o) => self.tree_children(o),
            Target::Port { output, key } => {
                self.spans(output, key).iter().map(|s| s.target()).collect()
            }
            Target::Span { output, key, .. } => match self.span_of(t) {
                Some(s) => s
                    .pixels
                    .map(|index| Target::Pixel {
                        output: output.clone(),
                        key: key.clone(),
                        index,
                    })
                    .collect(),
                None => Vec::new(),
            },
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
        let output = self.is_output(&t);
        out.push(t.clone());
        if frame {
            for c in self.children(Some(&t)) {
                self.see_through(c, out);
            }
        }
        // An output card's spans are picked directly, like a fixture's
        // contents (plan D13).
        if output {
            for port in self.children(Some(&t)) {
                out.extend(self.children(Some(&port)));
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
            Target::Lamp { segment, index } => self
                .segment(segment)
                .and_then(|o| o.lamps.get(*index as usize).map(|l| l.pos))
                .into_iter()
                .collect(),
            // A part's own node: in a repeat, copy 0 (the copies are their
            // own nodes); a repeat's node holds every copy.
            Target::Generated(_) | Target::Part(_) => self
                .tree_node_of_target(t)
                .and_then(|o| {
                    self.placed().node(&o).map(|(_, n)| {
                        n.leaves()
                            .into_iter()
                            .flat_map(|o| o.lamps.iter().map(|l| l.pos))
                            .collect()
                    })
                })
                .unwrap_or_default(),
            Target::Node(_) => self
                .children(Some(t))
                .iter()
                .flat_map(|c| self.lamps_under(c))
                .collect(),
            Target::Port { .. } | Target::Span { .. } | Target::Pixel { .. } => Vec::new(),
        }
    }

    /// How many lamps are under a target: `lamps_under(t).len()`, counted
    /// without gathering them.
    pub fn lamp_count_under(&self, t: &Target) -> usize {
        match t {
            Target::Lamp { .. } => self.lamps_under(t).len(),
            Target::Generated(_) | Target::Part(_) => self
                .tree_node_of_target(t)
                .and_then(|o| {
                    self.placed()
                        .node(&o)
                        .map(|(_, n)| n.leaves().iter().map(|o| o.lamps.len()).sum())
                })
                .unwrap_or(0),
            Target::Node(_) => self
                .children(Some(t))
                .iter()
                .map(|c| self.lamp_count_under(c))
                .sum(),
            Target::Port { .. } | Target::Span { .. } | Target::Pixel { .. } => 0,
        }
    }

    /// What the user calls this target, short: `tail`, `circle1`, `ring r2`,
    /// `lamp 14`, `D0`, `pixel 7`.
    pub fn label(&self, t: &Target) -> String {
        match t {
            Target::Node(id) => self.node_name(id),
            Target::Part(id) => id.to_string(),
            Target::Generated(o) => self.generated_label(o),
            Target::Lamp { index, .. } => format!("lamp {index}"),
            Target::Port { output, key } => self
                .output(output)
                .and_then(|o| o.port(key))
                .map(|p| p.name.clone())
                .unwrap_or_else(|| key.clone()),
            Target::Span { .. } => match self.span_of(t) {
                Some(s) => {
                    let what = match &s.segment {
                        Some(o) => self.long_label(&self.tree_target(o)),
                        None => "free".into(),
                    };
                    format!("{what} · {}", pixel_words(&s.pixels))
                }
                None => "span".into(),
            },
            Target::Pixel { index, .. } => format!("pixel {index}"),
        }
    }

    /// The label with enough context to stand alone: `ring r2 of circle1`,
    /// `lamp 14 of line3`, `D0 of main`, `pixel 7 of D0 of main`.
    pub fn long_label(&self, t: &Target) -> String {
        match t {
            Target::Node(_) | Target::Part(_) => self.label(t),
            Target::Generated(o) => match self.placed().parent_of(o) {
                Some(p) => format!(
                    "{} of {}",
                    self.generated_label(o),
                    self.long_label(&self.tree_target(&p))
                ),
                None => self.generated_label(o),
            },
            Target::Lamp { segment, .. } => {
                let parent = self.tree_target(segment);
                format!("{} of {}", self.label(t), self.long_label(&parent))
            }
            Target::Port { output, .. } => {
                format!("{} of {}", self.label(t), self.node_name(output))
            }
            Target::Pixel { .. } | Target::Span { .. } => match t.port_of_pixel() {
                Some(port) => format!("{} of {}", self.label(t), self.long_label(&port)),
                None => self.label(t),
            },
        }
    }

    /// What a generated node is called: a circle's are rings, a split
    /// path's subpaths, a repeat's copies are copies.
    pub fn generated_word(&self, o: &TreeRef) -> &'static str {
        match o.id.key.as_ref().map(|k| k.as_str()) {
            None => "copy",
            Some(k) if k.starts_with('s') => "subpath",
            Some(_) => "ring",
        }
    }

    /// A generated node's short label: `ring r2`, `copy 2` (the innermost
    /// copy it is), and a ring in a copy says both (`ring r2 · copy 2`).
    fn generated_label(&self, o: &TreeRef) -> String {
        let copy = o.id.copies.first().map(|k| format!("copy {k}"));
        match (&o.id.key, copy) {
            (None, Some(copy)) => copy,
            (None, None) => o.id.part.to_string(),
            (Some(key), None) => format!("{} {key}", self.generated_word(o)),
            (Some(key), Some(copy)) => format!("{} {key} · {copy}", self.generated_word(o)),
        }
    }

    /// The word for a target's kind, plural-able: "module", "fixture",
    /// "output", "port", "pixel", "group", "circle", "path", "polygon",
    /// "repeat", "ring", "copy", "lamp".
    pub fn kind_word(&self, t: &Target) -> &'static str {
        match t {
            Target::Node(id) => self.node(id).map(|n| n.kind_word()).unwrap_or("node"),
            Target::Part(id) => self.part(id).map(|c| part_word(&c.kind)).unwrap_or("thing"),
            Target::Generated(o) => self.generated_word(o),
            Target::Lamp { .. } => "lamp",
            Target::Port { .. } => "port",
            Target::Span { .. } => "span",
            Target::Pixel { .. } => "pixel",
        }
    }
}

/// What a person calls a kind of part.
pub fn part_word(kind: &lpc_mapping::part::PartKind) -> &'static str {
    use lpc_mapping::part::{PartKind, Shape};
    match kind {
        PartKind::Shape(Shape::Path(_)) => "path",
        PartKind::Shape(Shape::Polygon(_)) => "polygon",
        PartKind::Shape(Shape::Circle(_)) => "circle",
        PartKind::Shape(Shape::Grid(_)) => "grid",
        PartKind::Shape(Shape::FilledPolygon(_)) => "filled polygon",
        PartKind::Shape(Shape::Points(_)) => "points",
        PartKind::Group { .. } => "group",
        PartKind::Repeat(_) => "repeat",
    }
}

/// Does a part of this kind render exactly one segment, which is its own
/// node (D18's 1:1 case: the tree shows it as ONE row, its lamps directly
/// under it)? Decided by kind and split, not by the current count, so the
/// tree never changes shape under you.
pub fn is_single_segment(kind: &lpc_mapping::part::PartKind) -> bool {
    use lpc_mapping::part::{CircleSplit, PartKind, PathSplit, Shape};
    match kind {
        PartKind::Shape(Shape::Path(path)) => path.split == PathSplit::One,
        PartKind::Shape(Shape::Circle(circle)) => circle.split == CircleSplit::One,
        PartKind::Shape(_) => true,
        PartKind::Group { .. } | PartKind::Repeat(_) => false,
    }
}

fn lamp_targets(segment: &TreeRef, n: usize) -> Vec<Target> {
    (0..n as u32)
        .map(|index| Target::Lamp {
            segment: segment.clone(),
            index,
        })
        .collect()
}

/// Pixels in words: `pixel 4`, `pixels 40–63`.
fn pixel_words(px: &std::ops::Range<u32>) -> String {
    if px.len() == 1 {
        format!("pixel {}", px.start)
    } else {
        format!("pixels {}–{}", px.start, px.end.saturating_sub(1))
    }
}
