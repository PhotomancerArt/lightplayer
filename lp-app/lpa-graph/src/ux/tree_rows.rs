//! The tree pane's rows: one tree (D18). Nodes — modules, fixtures,
//! outputs and their ports — then components, and the derived rings under
//! a circle. Lamps and pixels are reached on the canvas, not listed here.

use lpc_mapping::part::{PartKind, Shape};

use crate::domain::node::NodeKind;
use crate::domain::project::Project;
use crate::domain::target::Target;

#[derive(Debug, Clone, PartialEq)]
pub struct TreeRow {
    pub target: Target,
    pub depth: usize,
    pub label: String,
    /// "25 lamps", "2 rings · 64 lamps".
    pub detail: String,
    /// Made by its part, not authored: drawn lighter, never dragged.
    pub derived: bool,
    /// A fixture's problems, while it has any: "2 problems" (loud).
    pub problems: Option<String>,
}

impl Project {
    pub fn tree_rows(&self) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        let patch = self.effective_patch();
        for t in self.children(None) {
            self.push_rows(&t, 0, &patch, &mut rows);
        }
        rows
    }

    fn push_rows(
        &self,
        t: &Target,
        depth: usize,
        patch: &crate::EffectivePatch,
        rows: &mut Vec<TreeRow>,
    ) {
        if matches!(
            t,
            Target::Lamp { .. } | Target::Span { .. } | Target::Pixel { .. }
        ) {
            return;
        }
        let lamps = self.lamp_count_under(t);
        let detail = match t {
            Target::Node(id) => match self.node(id).map(|n| &n.kind) {
                Some(NodeKind::Fixture(f)) => {
                    format!("{} × {} · {lamps} lamps", f.size.x, f.size.y)
                }
                Some(NodeKind::Module { children }) => {
                    format!("{} nodes · {lamps} lamps", children.len())
                }
                Some(NodeKind::Output(o)) => {
                    let used: u32 = o.ports.iter().map(|p| patch.used(id, &p.key)).sum();
                    format!(
                        "{} ports · {used}/{} lamps",
                        o.ports.len(),
                        self.output_pixels(id)
                    )
                }
                Some(NodeKind::Source(s)) => format!("{} → {}", s.kind_words(), s.writes),
                Some(NodeKind::Other(o)) => o.kind_words(),
                None => String::new(),
            },
            Target::Port { output, key } => {
                let pixels = self.port_pixels(output, key);
                format!("{}/{pixels} lamps", patch.used(output, key))
            }
            Target::Part(id) => match self.part(id).map(|c| &c.kind) {
                Some(PartKind::Shape(Shape::Circle(circle))) => {
                    let rings = &circle.rings;
                    format!("{} rings · {lamps} lamps", rings.len())
                }
                Some(PartKind::Group { parts }) => {
                    format!("{} parts · {lamps} lamps", parts.len())
                }
                Some(PartKind::Repeat(repeat)) => {
                    format!("{} copies", repeat.count) + &format!(" · {lamps} lamps")
                }
                _ => format!("{lamps} lamps"),
            },
            Target::Generated(o) => {
                let dir = match patch.walked(o) {
                    Some(w) if w.direction.is_reverse() => " · reversed",
                    _ => "",
                };
                format!("{lamps} lamps{dir}")
            }
            Target::Lamp { .. } | Target::Span { .. } | Target::Pixel { .. } => String::new(),
        };
        rows.push(TreeRow {
            target: t.clone(),
            depth,
            label: self.label(t),
            detail,
            derived: !t.is_authored(),
            problems: match t {
                Target::Node(id) if self.fixture(id).is_some() => self.problem_count_words(id),
                _ => None,
            },
        });
        // A port's children are pixels and an object's are lamps, never
        // listed: don't make the dome's 44,000 of them only to skip them.
        let leaves = match t {
            Target::Port { .. } | Target::Generated(_) => true,
            Target::Part(id) => self
                .part(id)
                .is_some_and(|c| crate::domain::target::is_single_segment(&c.kind)),
            _ => false,
        };
        if leaves {
            return;
        }
        for child in self.children(Some(t)) {
            self.push_rows(&child, depth + 1, patch, rows);
        }
    }
}
