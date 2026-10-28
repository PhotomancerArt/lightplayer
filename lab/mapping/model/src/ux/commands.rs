//! Commands on the selection: delete, nudge a count, reverse, edit a
//! property. Each one does what it can and says why it didn't do the rest.

use crate::domain::component::ComponentKind;
use crate::domain::notice::Notice;
use crate::domain::target::Target;
use crate::ux::editor::Editor;
use crate::ux::props::{PropEdit, common_props};

impl Editor {
    /// ⌫: delete what you authored — a node (a module with everything in
    /// it), a component; a ring goes through its circle and a port through
    /// its output; a lamp or a pixel is refused with a reason and a way to
    /// do it (D18, D20). Deleting an output leaves the rules that named it as problems,
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
                // A repeat's copies come from its count (D11): one can't
                // go on its own.
                Target::Object(o) if f.ring(&o.component, &o.key).is_none() => {
                    refused.push((
                        t.clone(),
                        format!("{} makes its copies from a count", o.component),
                        format!(
                            "set {}'s Copies in the inspector (Esc selects it), or delete it whole",
                            o.component
                        ),
                    ));
                }
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
                // A span is where an object lands, a view of the wire:
                // there is nothing of its own to delete.
                Target::Span { object, .. } => {
                    refused.push((
                        t.clone(),
                        "a span is where an object lands on the wire".into(),
                        match object {
                            Some(_) => "U puts its object back to following; choose Not on the wire in its Patch section to take it off".into(),
                            None => "nothing is there to delete".into(),
                        },
                    ));
                }
                Target::Pixel { output, key, .. } => {
                    let n = f.port_pixels(output, key);
                    let port = t.port_of_pixel().unwrap_or_else(|| t.clone());
                    refused.push((
                        t.clone(),
                        format!("{} makes its pixels from a count", f.long_label(&port)),
                        format!(
                            "press − to make it {} (pixels come off the end)",
                            n.saturating_sub(1).max(1)
                        ),
                    ));
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
        let problems_before = f.effective_patch().problems.clone();
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
            .iter()
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
    /// stands for its path or ring, a selected pixel for its port.
    fn edit_targets(&self) -> Vec<Target> {
        let mut out = Vec::new();
        for t in &self.selection {
            let t = match t {
                Target::Lamp { object, .. } => self.project.object_target(object),
                Target::Pixel { .. } => t.port_of_pixel().unwrap_or_else(|| t.clone()),
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
                "select a path, a ring or a circle; − and = change its count",
            ));
            return;
        }
        let props = common_props(&self.project, &targets);
        let Some(prop) = props.iter().find(|p| p.desc.nudge) else {
            return self.refuse_no_count(&targets);
        };
        if prop.desc.reshapes && !self.move_draw {
            let label = prop.desc.label.to_lowercase();
            self.refuse_move_draw(&format!("change the {label}"));
            return;
        }
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

    /// − / = on something with no count of its own: say what to do.
    fn refuse_no_count(&mut self, targets: &[Target]) {
        let t = &targets[0];
        self.notice = Some(if targets.len() == 1 {
            Notice::refused(
                format!("{} has no count of its own", self.project.long_label(t)),
                format!("a {} is made of other things", self.project.kind_word(t)),
                "Enter goes into it; select a path or ring there",
            )
        } else {
            Notice::refused(
                "These don't share a count",
                "paths and rings count lamps, circles count rings",
                "select only paths and rings (or only circles) to change them together",
            )
        });
    }

    /// r: reverse the data direction. A path runs from its last point to
    /// its first (its `reversed` flag: no point moves); a ring runs the
    /// other way round; a repeat of a path reverses the path, so every copy.
    /// A selected lamp stays the same physical lamp.
    pub fn reverse(&mut self) {
        let targets = self.edit_targets();
        if targets.is_empty() {
            self.notice = Some(Notice::refused(
                "Nothing to reverse",
                "nothing is selected",
                "select a path or a ring; r reverses which end the data enters",
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
                "select a path or a ring; r reverses which end the data enters",
            ));
            return;
        }
        self.checkpoint();
        // A repeat's copies share one shape: reverse it once.
        let mut whole: Vec<crate::ComponentId> = Vec::new();
        for t in &targets {
            match t {
                Target::Component(id) => {
                    if let Some(c) = self.project.component_mut(id) {
                        reverse_kind(&mut c.kind);
                    }
                }
                Target::Object(o) => {
                    if let Some(r) = self.project.ring_mut(&o.component, &o.key) {
                        r.reversed = !r.reversed;
                    } else if !whole.contains(&o.component)
                        && let Some(c) = self.project.component_mut(&o.component)
                    {
                        reverse_kind(&mut c.kind);
                        whole.push(o.component.clone());
                    }
                }
                Target::Lamp { .. }
                | Target::Node(_)
                | Target::Port { .. }
                | Target::Span { .. }
                | Target::Pixel { .. } => {}
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
                let ring = f.ring(&object.component, &object.key).is_some();
                *index = if ring {
                    (n - *index) % n
                } else {
                    n - 1 - *index
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
        let reshapes = common_props(&self.project, &targets)
            .into_iter()
            .find(|p| p.desc.id == prop_id && p.desc.reshapes);
        if let Some(p) = reshapes
            && !self.move_draw
        {
            self.refuse_move_draw(&format!("change the {}", p.desc.label.to_lowercase()));
            return;
        }
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
    /// Paths, circles, rings and lamps have a data direction; groups,
    /// nodes and ports don't.
    pub fn has_direction(&self, t: &Target) -> bool {
        let kind_has = |id: &crate::ComponentId| {
            self.component(id)
                .is_some_and(|c| kind_has_direction(&c.kind))
        };
        match t {
            Target::Component(id) => kind_has(id),
            Target::Object(o) | Target::Lamp { object: o, .. } => kind_has(&o.component),
            Target::Node(_) | Target::Port { .. } | Target::Span { .. } | Target::Pixel { .. } => {
                false
            }
        }
    }
}

/// A path, a ring and a ring set have a direction of their own; a polygon (as in the
/// product) and a group don't; a repeat has its child's.
fn kind_has_direction(kind: &ComponentKind) -> bool {
    match kind {
        ComponentKind::Path { .. } | ComponentKind::Circle { .. } | ComponentKind::RingSet(_) => {
            true
        }
        ComponentKind::Repeat { child, .. } => kind_has_direction(&child.kind),
        // A lattice's wiring is its start corner and routing, not a flag.
        ComponentKind::Polygon { .. }
        | ComponentKind::Group { .. }
        | ComponentKind::Grid(_)
        | ComponentKind::Filled(_) => false,
    }
}

/// Reverse a whole component's direction: a path's flag, every ring of a
/// circle, a repeat's shape.
fn reverse_kind(kind: &mut ComponentKind) {
    match kind {
        ComponentKind::Path { reversed, .. } => *reversed = !*reversed,
        ComponentKind::Circle { rings, .. } => {
            rings.iter_mut().for_each(|r| r.reversed = !r.reversed)
        }
        ComponentKind::RingSet(set) => set.reversed = !set.reversed,
        ComponentKind::Repeat { child, .. } => reverse_kind(&mut child.kind),
        ComponentKind::Polygon { .. }
        | ComponentKind::Group { .. }
        | ComponentKind::Grid(_)
        | ComponentKind::Filled(_) => {}
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
