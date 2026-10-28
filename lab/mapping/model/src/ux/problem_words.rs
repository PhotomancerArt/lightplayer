//! Problems in plain words (lean Q85): each of the patcher's problems, and
//! each overlap on a port row (`port_rows.rs`), as one sentence naming its
//! segment or port — "ring r5 doesn't fit on port D1: it lands as far as
//! the port goes".
//!
//! Where they show (`docs/ux/README.md` P4, quiet when good and loud when
//! bad, never by colour alone, P14): a count on the fixture's card and its
//! tree row while there are any; every one in the fixture's inspector; a
//! node's own in its section. An override whose node is gone carries
//! Remove.
//!
//! The words are the Graph's own; nothing has to match the engine's text.

use lpc_mapping::patching::PatchTarget;

use crate::domain::node::NodeId;
use crate::domain::part_ref::{PartRef, TreeRef};
use crate::domain::patch::{PatchProblem, ProblemKind};
use crate::domain::project::Project;
use crate::domain::target::{Target, output_name_words};
use crate::ux::port_rows::lamps_words;
use crate::ux::ui::UiProblem;

/// One problem, with the nodes it names.
#[derive(Debug, Clone, PartialEq)]
pub struct ProblemLine {
    /// The fixtures it is listed on: one, or both of an overlap's.
    pub fixtures: Vec<NodeId>,
    /// The nodes it is about, shown in their own sections too.
    pub names: Vec<TreeRef>,
    pub problem: UiProblem,
}

impl Project {
    /// Every problem of every fixture, said once each.
    pub fn problem_lines(&self) -> Vec<ProblemLine> {
        let mut out: Vec<ProblemLine> = self
            .effective_patch()
            .problems
            .iter()
            .map(|x| self.problem_line(x))
            .collect();
        for (output, port, x) in self.row_overlaps() {
            let [a, b] = &x.segments;
            let mut fixtures = vec![a.fixture.clone()];
            if b.fixture != a.fixture {
                fixtures.push(b.fixture.clone());
            }
            let lamps = (x.channels.end - x.channels.start)
                .div_ceil(crate::domain::port_tables::PORT_CHANNELS_PER_LAMP);
            // Two fixtures' segments say whose they are.
            let whose = |s: &TreeRef| {
                if fixtures.len() > 1 {
                    format!(
                        "{} of {}",
                        self.segment_words(s),
                        self.node_name(&s.fixture)
                    )
                } else {
                    self.segment_words(s)
                }
            };
            let (wa, wb) = (whose(a), whose(b));
            out.push(ProblemLine {
                fixtures,
                names: vec![a.clone(), b.clone()],
                problem: UiProblem {
                    text: format!(
                        "{wa} and {wb} share {} on port {}: the board shows only one",
                        lamps_words(lamps),
                        self.port_words(&output, &port)
                    ),
                    about: Some(Target::Port { output, key: port }),
                    remove: None,
                },
            });
        }
        out
    }

    /// A fixture's problems, each one sentence.
    pub fn problems_of_fixture(&self, fixture: &NodeId) -> Vec<UiProblem> {
        self.problem_lines()
            .into_iter()
            .filter(|l| l.fixtures.contains(fixture))
            .map(|l| l.problem)
            .collect()
    }

    /// The problems that name one node: its own section shows them.
    pub fn problems_naming(&self, node: &TreeRef) -> Vec<UiProblem> {
        self.problem_lines()
            .into_iter()
            .filter(|l| l.names.contains(node))
            .map(|l| l.problem)
            .collect()
    }

    /// "1 problem", "3 problems": a fixture's card and tree row say it
    /// while there are any, and nothing while there are none.
    pub fn problem_count_words(&self, fixture: &NodeId) -> Option<String> {
        match self.problems_of_fixture(fixture).len() {
            0 => None,
            1 => Some("1 problem".into()),
            n => Some(format!("{n} problems")),
        }
    }

    fn problem_line(&self, x: &PatchProblem) -> ProblemLine {
        let fixture = &x.fixture;
        let seg = |s: &TreeRef| (self.segment_words(s), Some(self.tree_target(s)));
        let (text, about, names, remove) = match &x.kind {
            ProblemKind::NoStart => (
                format!(
                    "nothing says where {} starts: its lamps aren't lit",
                    self.node_name(fixture)
                ),
                Some(Target::Node(fixture.clone())),
                Vec::new(),
                None,
            ),
            ProblemKind::GoneOutput { on, output } => (
                format!(
                    "{} is on {}, which this fixture can't reach: it isn't lit",
                    self.start_owner_words(fixture, on),
                    output_name_words(output)
                ),
                self.start_owner_target(fixture, on),
                self.start_owner_node(fixture, on).into_iter().collect(),
                None,
            ),
            ProblemKind::GonePort { on, output, port } => (
                format!(
                    "{} is on port {port} of {}, which isn't there: it isn't lit",
                    self.start_owner_words(fixture, on),
                    output_name_words(output)
                ),
                self.start_owner_target(fixture, on),
                self.start_owner_node(fixture, on).into_iter().collect(),
                None,
            ),
            ProblemKind::GoneNode(o) => (
                format!("an override on {}, which isn't there any more", o.id),
                None,
                Vec::new(),
                Some(o.clone()),
            ),
            ProblemKind::DoesntFit { segment, port } => {
                let (words, about) = seg(segment);
                (
                    format!(
                        "{words} doesn't fit on port {}: it lands as far as the port goes",
                        self.landed_pin_words(segment, port)
                    ),
                    about,
                    vec![segment.clone()],
                    None,
                )
            }
            ProblemKind::PastLastPort { segment } => {
                let (words, about) = seg(segment);
                let output = self
                    .effective_patch()
                    .walked(segment)
                    .and_then(|w| w.at.output_name.clone())
                    .map_or_else(|| "its output".into(), |n| output_name_words(&n));
                (
                    format!("{words} is past the last port of {output}: it isn't lit"),
                    about,
                    vec![segment.clone()],
                    None,
                )
            }
            ProblemKind::MidPixel { segment } => {
                let (words, about) = seg(segment);
                (
                    format!("{words} starts part-way into a lamp: it isn't lit"),
                    about,
                    vec![segment.clone()],
                    None,
                )
            }
            ProblemKind::OrderMismatch { segment } => {
                let (words, about) = seg(segment);
                (
                    format!("{words}'s colour order doesn't fit its colours"),
                    about,
                    vec![segment.clone()],
                    None,
                )
            }
        };
        ProblemLine {
            fixtures: vec![fixture.clone()],
            names,
            problem: UiProblem {
                text,
                about,
                remove,
            },
        }
    }

    /// Whose start a problem is about: "the fixture's start", "circle1's
    /// start", "ring r3's own start".
    fn start_owner_words(&self, fixture: &NodeId, on: &PatchTarget) -> String {
        match on {
            PatchTarget::Fixture => "the fixture's start".into(),
            PatchTarget::Part(part) => format!("{part}'s start"),
            PatchTarget::Node(id) => format!(
                "{}'s own start",
                self.segment_words(&TreeRef::new(fixture, id.clone()))
            ),
        }
    }

    fn start_owner_target(&self, fixture: &NodeId, on: &PatchTarget) -> Option<Target> {
        Some(match on {
            PatchTarget::Fixture => Target::Node(fixture.clone()),
            PatchTarget::Part(part) => Target::Part(PartRef::new(fixture, part)),
            PatchTarget::Node(id) => self.tree_target(&TreeRef::new(fixture, id.clone())),
        })
    }

    fn start_owner_node(&self, fixture: &NodeId, on: &PatchTarget) -> Option<TreeRef> {
        match on {
            PatchTarget::Fixture => None,
            PatchTarget::Part(part) => Some(PartRef::new(fixture, part).node()),
            PatchTarget::Node(id) => Some(TreeRef::new(fixture, id.clone())),
        }
    }

    /// The pin a segment was set on, by its port key there.
    fn landed_pin_words(&self, segment: &TreeRef, key: &str) -> String {
        let output = self
            .effective_patch()
            .walked(segment)
            .and_then(|w| w.landed.as_ref().or(Some(&w.at))?.output.clone());
        match output {
            Some(o) => self.pin_words(&o, key),
            None => key.to_string(),
        }
    }
}
