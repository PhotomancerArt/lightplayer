//! The tree pane's rows: one tree (D18). Nodes — modules, fixtures,
//! outputs and their ports — then components, and the derived rings under
//! a circle. Lamps are reached on the canvas, not listed here.

use crate::component::ComponentKind;
use crate::node::NodeKind;
use crate::project::Project;
use crate::target::Target;

#[derive(Debug, Clone, PartialEq)]
pub struct TreeRow {
    pub target: Target,
    pub depth: usize,
    pub label: String,
    /// "25 lamps", "2 rings · 64 lamps".
    pub detail: String,
    /// Made by its component, not authored: drawn lighter, never dragged.
    pub derived: bool,
}

impl Project {
    pub fn tree_rows(&self) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        for t in self.children(None) {
            self.push_rows(&t, 0, &mut rows);
        }
        rows
    }

    fn push_rows(&self, t: &Target, depth: usize, rows: &mut Vec<TreeRow>) {
        if matches!(t, Target::Lamp { .. }) {
            return;
        }
        let lamps = self.lamps_under(t).len();
        let patch = self.effective_patch();
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
                    format!("{} ports · {used}/{} px", o.ports.len(), o.pixels())
                }
                None => String::new(),
            },
            Target::Port { output, key } => {
                let pixels = self
                    .output(output)
                    .and_then(|o| o.port(key))
                    .map_or(0, |p| p.pixels);
                format!("{}/{pixels} px", patch.used(output, key))
            }
            Target::Component(id) => match self.component(id).map(|c| &c.kind) {
                Some(ComponentKind::Circle { rings, .. }) => {
                    format!("{} rings · {lamps} lamps", rings.len())
                }
                Some(ComponentKind::Group { children }) => {
                    format!("{} parts · {lamps} lamps", children.len())
                }
                _ => format!("{lamps} lamps"),
            },
            Target::Object(o) => {
                let dir = match self.ring(&o.component, &o.key) {
                    Some(r) if r.reversed => " · counter-clockwise",
                    _ => "",
                };
                format!("{lamps} lamps{dir}")
            }
            Target::Lamp { .. } => String::new(),
        };
        rows.push(TreeRow {
            target: t.clone(),
            depth,
            label: self.label(t),
            detail,
            derived: !t.is_authored(),
        });
        for child in self.children(Some(t)) {
            self.push_rows(&child, depth + 1, rows);
        }
    }
}
