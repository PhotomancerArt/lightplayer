//! What the layout verbs that take values can be given, in words: the
//! parts, rings and nodes each acts on, the splits and the "drawn from"s
//! (`graph_layout_edit.rs`). Studio's offers draw their parameters from
//! these lists, as they do the patching verbs' (`graph_verb_choices.rs`).
//!
//! **What is on offer** (ruling DD37). A layout verb is on offer while the
//! project has something it can act on — Unrepeat while there is a
//! repeat, Add centre lamp while a circle has none — never because of the
//! selection, which only preselects the subject.

use lpc_mapping::part::{Part, PartKind, Shape};

use crate::domain::node::NodeId;
use crate::domain::part_ref::{PartRef, TreeRef};
use crate::domain::target::Target;
use crate::ux::editor::Editor;
use crate::ux::graph_edit::node_value;
use crate::ux::graph_layout_edit::{
    DRAWN_FROM, DeleteSubject, Split, delete_value, drawn_from_label, drawn_from_name, part_value,
};
use crate::ux::graph_verb::GraphVerb;
use crate::ux::graph_verb_choices::VerbChoice;

impl Editor {
    /// What the layout verb `verb` can act on: its `subject` parameter's
    /// options.
    pub(crate) fn layout_subjects(&self, verb: GraphVerb) -> Vec<VerbChoice> {
        let parts = self.every_part();
        let part = |(f, p): &(NodeId, &Part)| {
            let r = PartRef::new(f, &p.id);
            VerbChoice::new(part_value(&r), self.with_fixture(f, p.id.to_string()))
        };
        let node = |n: TreeRef| VerbChoice::new(node_value(&n), self.node_label(&n));
        match verb {
            GraphVerb::Rename | GraphVerb::Repeat => parts.iter().map(part).collect(),
            GraphVerb::Unrepeat | GraphVerb::Expand => parts
                .iter()
                .filter(|(_, p)| matches!(p.kind, PartKind::Repeat(_)))
                .map(part)
                .collect(),
            GraphVerb::SetCount => {
                let mut out = Vec::new();
                for (f, p) in &parts {
                    match &p.kind {
                        PartKind::Repeat(_)
                        | PartKind::Shape(Shape::Path(_) | Shape::Polygon(_)) => {
                            out.push(node(PartRef::new(f, &p.id).node()));
                        }
                        PartKind::Shape(Shape::Circle(c)) => {
                            out.extend(c.rings.iter().filter(|r| !r.is_centre()).map(|r| {
                                node(TreeRef::new(
                                    f,
                                    lpc_mapping::part::TreeId::generated(&p.id, r.key.clone()),
                                ))
                            }));
                        }
                        _ => {}
                    }
                }
                out
            }
            GraphVerb::AddRing => parts.iter().filter(|(_, p)| circle(p)).map(part).collect(),
            GraphVerb::AddCentreLamp => parts
                .iter()
                .filter(|(_, p)| match &p.kind {
                    PartKind::Shape(Shape::Circle(c)) => !c.rings.iter().any(|r| r.is_centre()),
                    _ => false,
                })
                .map(part)
                .collect(),
            GraphVerb::RemoveRing => self.removable_rings().into_iter().map(node).collect(),
            GraphVerb::SetSplit | GraphVerb::SetDrawnFrom => parts
                .iter()
                .filter(|(_, p)| circle(p) || matches!(p.kind, PartKind::Shape(Shape::Path(_))))
                .map(part)
                .collect(),
            GraphVerb::Delete => {
                let p = &self.project;
                let mut out: Vec<VerbChoice> = p
                    .all_nodes()
                    .iter()
                    .map(|n| {
                        let subject = DeleteSubject::Node(n.id.clone());
                        VerbChoice::new(delete_value(&subject), p.node_name(&n.id))
                    })
                    .collect();
                let held = held_parts(&parts);
                out.extend(
                    parts
                        .iter()
                        .filter(|(_, x)| !held.contains(&x.id))
                        .map(part),
                );
                out.extend(self.removable_rings().into_iter().map(node));
                out
            }
            _ => Vec::new(),
        }
    }

    /// The subject `t` (the one selected thing) is for a layout verb,
    /// when it is one of `subjects`: the node itself, then its node in
    /// copy 0, then its part.
    pub(crate) fn layout_selected_subject(
        &self,
        t: &Target,
        subjects: &[VerbChoice],
    ) -> Option<String> {
        let candidates: Vec<String> = match t {
            Target::Node(id) => vec![delete_value(&DeleteSubject::Node(id.clone()))],
            Target::Part(part) => vec![part_value(part)],
            Target::Generated(o)
            | Target::Lamp { segment: o, .. }
            | Target::Span {
                segment: Some(o), ..
            } => vec![
                node_value(o),
                node_value(&o.original()),
                part_value(&o.part()),
            ],
            _ => Vec::new(),
        };
        candidates
            .into_iter()
            .find(|c| subjects.iter().any(|s| &s.value == c))
    }

    /// `set-split`'s choices.
    pub fn split_choices(&self) -> Vec<VerbChoice> {
        Split::ALL
            .into_iter()
            .map(|s| VerbChoice::new(s.name(), s.label()))
            .collect()
    }

    /// `set-drawn-from`'s choices.
    pub fn drawn_from_choices(&self) -> Vec<VerbChoice> {
        DRAWN_FROM
            .into_iter()
            .map(|f| VerbChoice::new(drawn_from_name(f), drawn_from_label(f)))
            .collect()
    }

    /// Each repeat (its node, as a value) with the fewest copies that keep
    /// every override on its copies, where that is more than one: a count
    /// below it leaves some kept and gone
    /// (`lpc_mapping::edit::overrides_past_count`), so `set-count` to it
    /// takes something away.
    pub fn counts_that_keep(&self) -> Vec<(String, u32)> {
        let mut out = Vec::new();
        for (f, part) in self.every_part() {
            let PartKind::Repeat(repeat) = &part.kind else {
                continue;
            };
            let Some(m) = self.project.fixture(&f).map(|x| &x.mapping) else {
                continue;
            };
            let past = |count| lpc_mapping::edit::overrides_past_count(m, &part.id, count);
            let (mut low, mut high) = (1, repeat.count.max(1));
            if past(low) == 0 || past(high) > 0 {
                continue;
            }
            // past(c) only falls as c grows.
            while low < high {
                let mid = (low + high) / 2;
                if past(mid) == 0 {
                    high = mid;
                } else {
                    low = mid + 1;
                }
            }
            out.push((node_value(&PartRef::new(&f, &part.id).node()), low));
        }
        out
    }

    /// Every part of every fixture, fixtures in tree order, parts depth
    /// first.
    fn every_part(&self) -> Vec<(NodeId, &Part)> {
        let p = &self.project;
        let mut out = Vec::new();
        for n in p.fixtures() {
            if let Some(f) = p.fixture(&n.id) {
                out.extend(f.mapping.all_parts().into_iter().map(|x| (n.id.clone(), x)));
            }
        }
        out
    }

    /// Every ring of a circle with more than one: a circle's last ring
    /// goes with the circle.
    fn removable_rings(&self) -> Vec<TreeRef> {
        let mut out = Vec::new();
        for (f, p) in self.every_part() {
            if let PartKind::Shape(Shape::Circle(c)) = &p.kind
                && c.rings.len() > 1
            {
                out.extend(c.rings.iter().map(|r| {
                    TreeRef::new(
                        &f,
                        lpc_mapping::part::TreeId::generated(&p.id, r.key.clone()),
                    )
                }));
            }
        }
        out
    }
}

fn circle(p: &Part) -> bool {
    matches!(p.kind, PartKind::Shape(Shape::Circle(_)))
}

/// The parts a repeat holds: each goes with its repeat, never alone.
fn held_parts(parts: &[(NodeId, &Part)]) -> Vec<lpc_mapping::part::PartId> {
    parts
        .iter()
        .filter_map(|(_, p)| match &p.kind {
            PartKind::Repeat(r) => Some(r.part.id.clone()),
            _ => None,
        })
        .collect()
}
