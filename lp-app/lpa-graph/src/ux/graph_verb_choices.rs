//! What the patching verbs that take values can be given, in words: the
//! nodes each acts on, the ports, the flows (the layout verbs' are in
//! `layout_verb_choices.rs`). Studio's offers draw their
//! parameters from these lists (`graph_offers.rs`), so the agent and a
//! person choose among the same options, labelled as the Graph says them
//! ("ring r3 of circle1", "D1 of main").
//!
//! **What is on offer.** A verb is on offer while the project has
//! something it can act on: Turn on while some node is off, Remove
//! override while an override's node is gone, Flow while there is a
//! fixture. Not the selection: the selection only preselects the subject,
//! so the agent never has to select before it edits.
//!
//! **Values** are written in the forms `GraphEdit::bind` reads
//! (`graph_edit.rs`).

use lpc_mapping::patching::Flow;

use crate::domain::node::NodeId;
use crate::domain::part_ref::{PartRef, TreeRef};
use crate::domain::patch::ProblemKind;
use crate::domain::project::Project;
use crate::domain::target::{Target, is_single_segment, output_name_words};
use crate::ux::editor::Editor;
use crate::ux::graph_edit::{BackField, RuleSubject, flow_value, node_value, port_value};
use crate::ux::graph_verb::GraphVerb;

/// One option of a verb's parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerbChoice {
    /// What a press carries.
    pub value: String,
    /// What a person reads.
    pub label: String,
    /// Why it can't be chosen now, when it can't.
    pub disabled: Option<String>,
}

impl VerbChoice {
    pub(crate) fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: None,
        }
    }
}

impl Editor {
    /// The verbs that take values with something to act on now, in
    /// [`GraphVerb::WITH_VALUES`] order.
    pub fn verbs_with_values(&self) -> Vec<GraphVerb> {
        GraphVerb::WITH_VALUES
            .into_iter()
            .filter(|v| !self.subjects(*v).is_empty())
            .collect()
    }

    /// What `verb` can act on: its `subject` parameter's options.
    pub fn subjects(&self, verb: GraphVerb) -> Vec<VerbChoice> {
        let p = &self.project;
        let node = |n: &TreeRef| VerbChoice::new(node_value(n), self.node_label(n));
        match verb {
            GraphVerb::SetOutput
            | GraphVerb::SetPort
            | GraphVerb::SetStartsAt
            | GraphVerb::SetOffset => p
                .placed_segments()
                .iter()
                .filter(|(_, s)| !s.lamps.is_empty())
                .map(|(_, s)| node(&s.id))
                .collect(),
            GraphVerb::SetDirection => self.tree_nodes().iter().map(node).collect(),
            GraphVerb::TurnOff => self
                .tree_nodes()
                .iter()
                .filter(|n| !p.override_of(n).is_some_and(|o| o.off))
                .map(node)
                .collect(),
            GraphVerb::TurnOn => self
                .tree_nodes()
                .iter()
                .filter(|n| p.override_of(n).is_some_and(|o| o.off))
                .map(node)
                .collect(),
            GraphVerb::BackToTheFlow => self
                .tree_nodes()
                .iter()
                .filter(|n| p.override_of(n).is_some_and(|o| !o.is_empty()))
                .map(node)
                .collect(),
            GraphVerb::SetFlow => self
                .rule_subjects(|_, part| !is_single_segment(&part.kind))
                .into_iter()
                .map(|s| self.rule_choice(&s))
                .collect(),
            GraphVerb::SetFixtureStart => self
                .rule_subjects(|_, _| false)
                .into_iter()
                .map(|s| self.rule_choice(&s))
                .collect(),
            GraphVerb::SetPartStart => self
                .rule_subjects(|_, _| true)
                .into_iter()
                .filter(|s| match s {
                    RuleSubject::Fixture(_) => false,
                    RuleSubject::Part(part) => {
                        p.part(part).is_some_and(|c| !is_single_segment(&c.kind))
                            || p.in_repeat(part)
                    }
                })
                .map(|s| self.rule_choice(&s))
                .collect(),
            GraphVerb::RemoveOverride => p
                .effective_patch()
                .problems
                .iter()
                .filter_map(|x| match &x.kind {
                    ProblemKind::GoneNode(n) => Some(VerbChoice::new(
                        node_value(n),
                        format!("{}, which isn't there any more", n.id),
                    )),
                    _ => None,
                })
                .collect(),
            layout if GraphVerb::LAYOUT.contains(&layout) => self.layout_subjects(layout),
            _ => Vec::new(),
        }
    }

    /// The subject the selection is for `verb`, when it is one of
    /// `subjects` (its [`Editor::subjects`]): what its offer preselects.
    pub fn selected_subject(&self, verb: GraphVerb, subjects: &[VerbChoice]) -> Option<String> {
        let [t] = self.selection.as_slice() else {
            return None;
        };
        if GraphVerb::LAYOUT.contains(&verb) {
            return self.layout_selected_subject(t, subjects);
        }
        let value = match verb {
            GraphVerb::SetFlow | GraphVerb::SetFixtureStart | GraphVerb::SetPartStart => {
                let subject = match t {
                    Target::Node(f) if self.project.fixture(f).is_some() => {
                        RuleSubject::Fixture(f.clone())
                    }
                    Target::Part(_) | Target::Generated(_) => RuleSubject::Part(t.owner()?),
                    _ => return None,
                };
                subject.value()
            }
            _ => {
                let node = match t {
                    Target::Part(c) => c.node(),
                    Target::Generated(o)
                    | Target::Span {
                        segment: Some(o), ..
                    } => o.clone(),
                    _ => return None,
                };
                node_value(&node)
            }
        };
        subjects.iter().any(|s| s.value == value).then_some(value)
    }

    /// The ports `verb` can be given: every port of every named output
    /// some fixture reaches (`At <port>`), then, for `set-port`, Next port
    /// and Stays here. An output with no name lists its ports too: choosing
    /// one names it, and its label says with what (lean Q96).
    pub fn port_choices(&self, verb: GraphVerb) -> Vec<VerbChoice> {
        let p = &self.project;
        let mut out = Vec::new();
        if verb == GraphVerb::SetPartStart {
            out.push(VerbChoice::new("follows", "Follows"));
        }
        let names = p.next_output_name();
        for (output, port) in self.reachable_ports() {
            let mut label = p.port_words(&output, &port);
            if p.output(&output).is_some_and(|o| o.label.is_none()) {
                label = format!("{label} (names it {})", output_name_words(&names));
            }
            out.push(VerbChoice::new(port_value(&output, &port), label));
        }
        match verb {
            GraphVerb::SetPort => {
                out.push(VerbChoice::new("next-port", "Next port"));
                out.push(VerbChoice::new("stays-here", "Stays here"));
            }
            GraphVerb::SetPartStart => out.push(VerbChoice::new("next-port", "Next port")),
            _ => {}
        }
        out
    }

    /// The outputs `set-output` can name: every output some fixture
    /// reaches; choosing an unnamed one names it.
    pub fn output_choices(&self) -> Vec<VerbChoice> {
        let p = &self.project;
        let mut out: Vec<VerbChoice> = Vec::new();
        for (output, _) in self.reachable_ports() {
            if out.iter().any(|c| c.value == output.to_string()) {
                continue;
            }
            let mut label = p.output_words(&output);
            if p.output(&output).is_some_and(|o| o.label.is_none()) {
                label = format!(
                    "{label} (names it {})",
                    output_name_words(&p.next_output_name())
                );
            }
            out.push(VerbChoice::new(output.to_string(), label));
        }
        out
    }

    /// The flows `set-flow` offers: end to end, then one per port from
    /// each port a fixture reaches (a flow keeps only the port's key).
    pub fn flow_choices(&self) -> Vec<VerbChoice> {
        let p = &self.project;
        let mut out = vec![VerbChoice::new(flow_value(&Flow::EndToEnd), "End to end")];
        for (output, port) in self.reachable_ports() {
            let Some(from) = crate::domain::patch_edits::port_key(&port) else {
                continue;
            };
            let value = flow_value(&Flow::OnePerPort { from });
            if out.iter().any(|c| c.value == value) {
                continue;
            }
            out.push(VerbChoice::new(
                value,
                format!("One per port from {}", p.pin_words(&output, &port)),
            ));
        }
        out
    }

    /// `set-direction`'s choices.
    pub fn direction_choices(&self) -> Vec<VerbChoice> {
        vec![
            VerbChoice::new("forward", "Forward"),
            VerbChoice::new("reverse", "Reverse"),
        ]
    }

    /// `back-to-the-flow`'s fields.
    pub fn back_field_choices(&self) -> Vec<VerbChoice> {
        BackField::ALL
            .into_iter()
            .map(|f| VerbChoice::new(f.name(), f.label()))
            .collect()
    }

    // ---- helpers -------------------------------------------------------------

    /// Every node of every fixture's tree that is there, in tree order.
    pub(crate) fn tree_nodes(&self) -> Vec<TreeRef> {
        let p = &self.project;
        let mut out = Vec::new();
        for f in p.fixtures() {
            let Some(tree) = p.placed().tree(&f.id) else {
                continue;
            };
            out.extend(
                tree.nodes()
                    .iter()
                    .map(|n| TreeRef::new(&f.id, n.id.clone())),
            );
        }
        out
    }

    /// The fixtures, each followed by its parts that `keep` keeps, in
    /// tree order.
    fn rule_subjects(
        &self,
        keep: impl Fn(&Project, &lpc_mapping::part::Part) -> bool,
    ) -> Vec<RuleSubject> {
        let p = &self.project;
        let mut out = Vec::new();
        for f in p.fixtures() {
            out.push(RuleSubject::Fixture(f.id.clone()));
            let Some(fixture) = p.fixture(&f.id) else {
                continue;
            };
            for part in fixture.mapping.all_parts() {
                if keep(p, part) {
                    out.push(RuleSubject::Part(PartRef::new(&f.id, &part.id)));
                }
            }
        }
        out
    }

    pub(crate) fn rule_choice(&self, s: &RuleSubject) -> VerbChoice {
        let p = &self.project;
        let label = match s {
            RuleSubject::Fixture(f) => format!("{} (the fixture)", p.node_name(f)),
            RuleSubject::Part(part) => self.with_fixture(&part.fixture, part.part.to_string()),
        };
        VerbChoice::new(s.value(), label)
    }

    /// A node as the tree says it, with its fixture when there are several.
    pub(crate) fn node_label(&self, n: &TreeRef) -> String {
        let p = &self.project;
        self.with_fixture(&n.fixture, p.long_label(&p.tree_target(n)))
    }

    pub(crate) fn with_fixture(&self, fixture: &NodeId, words: String) -> String {
        if self.project.fixtures().len() > 1 {
            format!("{words} · {}", self.project.node_name(fixture))
        } else {
            words
        }
    }

    /// Every port of every output a fixture reaches: the named ones the
    /// port tables carry, in attach order, then every output with no name.
    fn reachable_ports(&self) -> Vec<(NodeId, String)> {
        let p = &self.project;
        let reached: Vec<String> = p
            .fixtures()
            .iter()
            .flat_map(|f| p.port_tables(&f.id))
            .map(|t| t.output.as_str().to_string())
            .collect();
        p.wire_ports()
            .into_iter()
            .filter(
                |(output, _)| match p.output(output).and_then(|o| o.label.as_deref()) {
                    Some(name) => reached.iter().any(|r| r == name),
                    None => true,
                },
            )
            .collect()
    }
}
