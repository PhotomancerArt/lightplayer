//! Commands on the selection: delete, nudge a count, reverse, edit a
//! property. Each one does what it can and says why it didn't do the rest.

use crate::component::ComponentKind;
use crate::editor::Editor;
use crate::notice::Notice;
use crate::props::{PropEdit, common_props};
use crate::target::Target;

impl Editor {
    /// ⌫: delete what you authored — a node (a module with everything in
    /// it), a component; a ring goes through its circle and a port through
    /// its output; a lamp is refused with a reason and a way to do it (D18,
    /// D20). Deleting an output leaves the rules that named it as problems,
    /// said out loud; nothing is retargeted.
    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            self.notice = Some(Notice::refused(
                "Nothing to delete",
                "nothing is selected",
                "click something first, or ⌘A to select everything here",
            ));
            return;
        }
        let f = &self.project;
        let selection = self.selection.clone();
        // Things whose parent is also selected go with it.
        let roots: Vec<Target> = selection
            .iter()
            .filter(|t| {
                !selection
                    .iter()
                    .any(|o| o != *t && f.is_ancestor_or_self(o, t))
            })
            .cloned()
            .collect();

        let mut nodes = Vec::new();
        let mut components = Vec::new();
        let mut rings = Vec::new();
        let mut ports: Vec<(crate::NodeId, String)> = Vec::new();
        let mut refused: Vec<(Target, String, String)> = Vec::new();
        for t in &roots {
            match t {
                Target::Node(id) => nodes.push(id.clone()),
                Target::Port { output, key } => {
                    let total = f.output(output).map_or(0, |o| o.ports.len());
                    let going = ports.iter().filter(|(o, _)| o == output).count();
                    if going + 1 >= total {
                        let name = f.node_name(output);
                        refused.push((
                            t.clone(),
                            format!("it is {name}'s last port"),
                            format!("delete {name} itself (Esc selects it, then ⌫)"),
                        ));
                    } else {
                        ports.push((output.clone(), key.clone()));
                    }
                }
                Target::Component(id) => components.push(id.clone()),
                Target::Object(o) => {
                    let total = match f.component(&o.component).map(|c| &c.kind) {
                        Some(ComponentKind::Circle { rings, .. }) => rings.len(),
                        _ => 0,
                    };
                    let going = rings
                        .iter()
                        .filter(|r: &&crate::ObjectId| r.component == o.component)
                        .count();
                    if going + 1 >= total {
                        refused.push((
                            t.clone(),
                            format!("it is {}'s last ring", o.component),
                            format!("delete {} itself (Esc selects it, then ⌫)", o.component),
                        ));
                    } else {
                        rings.push(o.clone());
                    }
                }
                Target::Lamp { object, .. } => {
                    let owner = f.object_target(object);
                    let n = f.object(object).map(|o| o.lamps.len()).unwrap_or(0);
                    refused.push((
                        t.clone(),
                        format!("{} makes its lamps from a count", f.long_label(&owner)),
                        format!(
                            "press − to make it {} (lamps come off the end)",
                            n.saturating_sub(1).max(1)
                        ),
                    ));
                }
            }
        }

        let deleted: Vec<String> = nodes
            .iter()
            .map(|x| f.node_name(x))
            .chain(components.iter().map(|c| c.to_string()))
            .chain(
                rings
                    .iter()
                    .map(|o| f.long_label(&Target::Object(o.clone()))),
            )
            .chain(ports.iter().map(|(o, k)| {
                f.long_label(&Target::Port {
                    output: o.clone(),
                    key: k.clone(),
                })
            }))
            .collect();
        let problems_before = f.effective_patch().problems;
        if !deleted.is_empty() {
            self.checkpoint();
            for x in &nodes {
                self.project.remove_node(x);
            }
            for (o, k) in &ports {
                if let Some(out) = self.project.output_mut(o) {
                    out.ports.retain(|p| &p.key != k);
                }
            }
            for c in &components {
                self.project.remove(c);
            }
            for o in &rings {
                if let Some(c) = self.project.component_mut(&o.component)
                    && let ComponentKind::Circle { rings, .. } = &mut c.kind
                {
                    rings.retain(|r| r.key != o.key);
                }
            }
        }
        // What was refused stays selected, so you can act on it.
        self.selection = refused.iter().map(|(t, _, _)| t.clone()).collect();
        self.after_edit();

        let broken: Vec<String> = self
            .project
            .effective_patch()
            .problems
            .into_iter()
            .filter(|p| !problems_before.contains(p))
            .map(|p| self.project.node_name(&p.fixture))
            .collect();
        self.notice = Some(match (deleted.is_empty(), refused.first()) {
            (false, None) if !broken.is_empty() => Notice {
                why: Some(format!(
                    "{} still patch{} to it: {}, shown in the inspector",
                    join(&broken),
                    if broken.len() == 1 { "es" } else { "" },
                    if broken.len() == 1 {
                        "its rule is now a problem"
                    } else {
                        "their rules are now problems"
                    },
                )),
                help: Some("nothing was pointed anywhere else; ⌘Z brings it back".into()),
                ..Notice::info(format!("Deleted {}", join(&deleted)))
            },
            (false, None) => Notice::info(format!("Deleted {}", join(&deleted))),
            (deleted_none, Some((t, why, help))) => {
                let what = if refused.len() == 1 {
                    self.project.long_label(t)
                } else {
                    format!("{} {}s", refused.len(), self.project.kind_word(t))
                };
                let text = if deleted_none {
                    format!("Can't delete {what}")
                } else {
                    format!("Deleted {} — but not {what}", join(&deleted))
                };
                Notice::refused(text, why.clone(), help.clone())
            }
            (true, None) => unreachable!("selection was not empty"),
        });
    }

    /// The things a count or direction edit applies to: a selected lamp
    /// stands for its line or ring.
    fn edit_targets(&self) -> Vec<Target> {
        let mut out = Vec::new();
        for t in &self.selection {
            let t = match t {
                Target::Lamp { object, .. } => self.project.object_target(object),
                other => other.clone(),
            };
            if !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }

    /// − / =: nudge the count everything selected shares.
    pub fn nudge(&mut self, steps: i64) {
        let targets = self.edit_targets();
        if targets.is_empty() {
            self.notice = Some(Notice::refused(
                "Nothing to change",
                "nothing is selected",
                "select a line, a ring or a circle; − and = change its count",
            ));
            return;
        }
        let props = common_props(&self.project, &targets);
        let Some(prop) = props.iter().find(|p| p.desc.nudge) else {
            let t = &targets[0];
            self.notice = Some(if targets.len() == 1 {
                Notice::refused(
                    format!("{} has no count of its own", self.project.long_label(t)),
                    format!("a {} is made of other things", self.project.kind_word(t)),
                    "Enter goes into it; select a line or ring there",
                )
            } else {
                Notice::refused(
                    "These don't share a count",
                    "lines and rings count lamps, circles count rings",
                    "select only lines and rings (or only circles) to change them together",
                )
            });
            return;
        };
        let id = prop.desc.id;
        self.edit_selected(&targets, id, PropEdit::Nudge(steps));
        let now = common_props(&self.project, &targets);
        if let Some(p) = now.iter().find(|p| p.desc.id == id) {
            self.notice = Some(Notice::info(format!(
                "{}: {}",
                p.desc.label,
                p.value.describe()
            )));
        }
    }

    /// r: reverse the data direction. A line swaps its ends; a ring runs the
    /// other way round. A selected lamp stays the same physical lamp.
    pub fn reverse(&mut self) {
        let targets = self.edit_targets();
        if targets.is_empty() {
            self.notice = Some(Notice::refused(
                "Nothing to reverse",
                "nothing is selected",
                "select a line or a ring; r reverses which end the data enters",
            ));
            return;
        }
        if let Some(t) = targets.iter().find(|t| !self.project.has_direction(t)) {
            self.notice = Some(Notice::refused(
                format!("Can't reverse {}", self.project.long_label(t)),
                format!(
                    "a {} has no direction of its own",
                    self.project.kind_word(t)
                ),
                "select a line or a ring; r reverses which end the data enters",
            ));
            return;
        }
        self.checkpoint();
        for t in &targets {
            match t {
                Target::Component(id) => {
                    match self.project.component_mut(id).map(|c| &mut c.kind) {
                        Some(ComponentKind::Line { from, to, .. }) => std::mem::swap(from, to),
                        Some(ComponentKind::Circle { rings, .. }) => {
                            rings.iter_mut().for_each(|r| r.reversed = !r.reversed)
                        }
                        _ => {}
                    }
                }
                Target::Object(o) => {
                    if let Some(r) = self.project.ring_mut(&o.component, &o.key) {
                        r.reversed = !r.reversed;
                    }
                }
                Target::Lamp { .. } | Target::Node(_) | Target::Port { .. } => {}
            }
        }
        // Keep each selected lamp on the same physical lamp.
        let f = &self.project;
        for t in &mut self.selection {
            if let Target::Lamp { object, index } = t {
                let n = f.object(object).map(|o| o.lamps.len() as u32).unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let line = f
                    .component(&object.component)
                    .is_some_and(|c| c.is_single_object());
                *index = if line {
                    n - 1 - *index
                } else {
                    (n - *index) % n
                };
            }
        }
        self.after_edit();
        self.notice = Some(Notice::info(format!(
            "Reversed {}",
            join_targets(self, &targets)
        )));
    }

    /// The inspector's edit: one property, every selected thing.
    pub fn edit_selection_prop(&mut self, prop_id: &str, edit: PropEdit) {
        let targets = self.selection.clone();
        self.notice = None;
        self.edit_selected(&targets, prop_id, edit);
    }

    fn edit_selected(&mut self, targets: &[Target], prop_id: &str, edit: PropEdit) {
        self.checkpoint();
        let spacing = self.spacing;
        for t in targets {
            self.project.edit_prop(t, prop_id, edit, spacing);
        }
        self.after_edit();
    }
}

impl crate::Project {
    /// Lines, circles, rings and lamps have a data direction; groups,
    /// nodes and ports don't.
    pub fn has_direction(&self, t: &Target) -> bool {
        match t {
            Target::Component(_) => !matches!(self.kind_word(t), "group"),
            Target::Object(_) | Target::Lamp { .. } => true,
            Target::Node(_) | Target::Port { .. } => false,
        }
    }
}

fn join(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        3 => format!("{}, {} and {}", items[0], items[1], items[2]),
        n => format!("{}, … and {} more", items[..2].join(", "), n - 2),
    }
}

fn join_targets(e: &Editor, targets: &[Target]) -> String {
    let labels: Vec<String> = targets.iter().map(|t| e.project.long_label(t)).collect();
    join(&labels)
}
