//! The tree pane for the view tree: one row per node, port and component,
//! with what's selected, lit and the level you're in marked.

use crate::domain::target::Target;
use crate::ux::editor::Editor;
use crate::ux::ui::{UiTree, UiTreeRow};

impl Editor {
    pub fn view_tree_pane(&self) -> UiTree {
        let p = &self.project;
        let context = self.context();
        // A selected lamp lights its path or ring's row.
        let lit: Vec<Target> = self
            .selection
            .iter()
            .map(|t| match t {
                Target::Lamp {
                    segment: object, ..
                } => p.tree_target(object),
                other => other.clone(),
            })
            .collect();
        let rows: Vec<UiTreeRow> = p
            .tree_rows()
            .into_iter()
            .map(|r| {
                let selected = self.selection.contains(&r.target);
                UiTreeRow {
                    kind: match &r.target {
                        Target::Node(_) => p.kind_word(&r.target),
                        Target::Port { .. } => "port",
                        _ => "",
                    },
                    lit: !selected && lit.contains(&r.target),
                    context: context.as_ref() == Some(&r.target),
                    selected,
                    target: r.target,
                    depth: r.depth,
                    label: r.label,
                    detail: r.detail,
                    derived: r.derived,
                    problems: r.problems,
                }
            })
            .collect();
        UiTree {
            title: p.name.clone(),
            empty: rows
                .is_empty()
                .then(|| "Nothing here yet. Press P to draw a path.".to_string()),
            rows,
        }
    }
}
