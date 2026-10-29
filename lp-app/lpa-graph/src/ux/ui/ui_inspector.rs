//! The inspector: where you are, and the selection's sections (Studio's
//! tabs and sections, flattened: the lab has one tab).

use crate::domain::part_ref::TreeRef;
use crate::domain::target::Target;
use crate::ux::graph_edit::GraphEdit;
use crate::ux::graph_verb::GraphVerb;
use crate::ux::patch_section::PatchField;
use crate::ux::props::Prop;

#[derive(Debug, Clone, PartialEq)]
pub struct UiInspector {
    /// The project, then the path down to the selection (or to the level a
    /// multiple selection is in). Clicking one selects it.
    pub crumbs: Vec<UiCrumb>,
    /// `3 selected`, at the end of the crumbs.
    pub many: Option<String>,
    pub sections: Vec<UiSection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiCrumb {
    /// `None` for the project: selects nothing.
    pub target: Option<Target>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiSection {
    pub title: Option<String>,
    /// The id in the title, set in monospace after it: `circle1/r3` in
    /// "Segment · circle1/r3". Ids show here and nowhere else (lean Q80).
    pub id: Option<String>,
    pub items: Vec<UiItem>,
}

impl UiSection {
    /// A section titled in words.
    pub fn titled(title: impl Into<String>, items: Vec<UiItem>) -> Self {
        Self {
            title: Some(title.into()),
            id: None,
            items,
        }
    }

    /// A section with no title.
    pub fn untitled(items: Vec<UiItem>) -> Self {
        Self {
            title: None,
            id: None,
            items,
        }
    }

    /// The title as it reads: "Segment · circle1/r3".
    pub fn heading(&self) -> String {
        match (&self.title, &self.id) {
            (Some(t), Some(id)) => format!("{t} · {id}"),
            (Some(t), None) => t.clone(),
            (None, _) => String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiItem {
    /// One line of plain fact.
    Fact(String),
    /// A port, how full it is: `50/60 px`, and 0–1.
    Port {
        name: String,
        fill: String,
        ratio: f64,
    },
    /// A patch rule, or where the lamps land.
    Rule(String),
    /// A problem, in one plain sentence (`problem_words.rs`): loud.
    Problem(UiProblem),
    /// A shader's GLSL, as it is.
    Code(String),
    /// A property you can edit, locked while Move & draw is off and it
    /// reshapes.
    Prop { prop: Prop, locked: bool },
    /// A quiet line saying what to do, or (presentation B) the start the
    /// flow gives a node.
    Hint(String),
    /// A fact with the verb that changes it beside it, as a button that
    /// presses the verb (`GraphInput::Verb`).
    Action { text: String, verb: GraphVerb },
    /// A fact with a verb that takes values beside it, bound: a button
    /// that makes the edit (`GraphInput::Edit`), as its offer does.
    Edit { text: String, edit: GraphEdit },
    /// A patching row (`patch_section.rs`): its value, and where the value
    /// came from (`start_words.rs`).
    Field(UiField),
    /// Presentation B's way to set one more field here: the fields not set
    /// here yet, each pressing `PatchInput::Override` (it becomes its own,
    /// as it is now).
    SetHere { options: Vec<(String, PatchField)> },
    /// A button that selects something else: a span's segment.
    Select { text: String, target: Target },
    /// A gap's "Patch here": the segments not on the line, the chosen one
    /// put at the gap's start.
    PatchHere { options: Vec<String> },
}

/// A problem, said once, plainly (lean Q85).
#[derive(Debug, Clone, PartialEq)]
pub struct UiProblem {
    pub text: String,
    /// What it names, to select from the sentence.
    pub about: Option<Target>,
    /// An override whose node is gone: Remove takes it away
    /// (`GraphEdit::RemoveOverride`, the `remove-override` verb).
    pub remove: Option<TreeRef>,
}

/// One patching row.
#[derive(Debug, Clone, PartialEq)]
pub struct UiField {
    /// `Port`, `Starts at`, `Flow`.
    pub label: &'static str,
    pub field: PatchField,
    pub value: UiFieldValue,
    /// Where the value came from; `None` says nothing (a default nothing
    /// sets, or a fixture's own start, which has nothing above it).
    pub source: Option<UiSource>,
    /// What moves with it: "ring r4 and the centre follow it".
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiFieldValue {
    /// The option at `chosen`, or none chosen (the selection doesn't share
    /// one, or it lands nowhere).
    Choice {
        options: Vec<String>,
        chosen: Option<usize>,
    },
    /// A number with − + (`None` when mixed), in `unit`. `stays` offers
    /// Stays here: hold the cursor where it is, whatever the flow says.
    Number {
        value: Option<i64>,
        unit: &'static str,
        stays: bool,
    },
    /// A value read, not edited here.
    Text(String),
}

/// Where a value came from.
#[derive(Debug, Clone, PartialEq)]
pub enum UiSource {
    /// From somewhere else, in words; the words select `target`.
    From {
        words: String,
        target: Option<Target>,
    },
    /// Set on this node, with Back to the flow (`PatchInput::Reset`).
    SetHere,
    /// The selected things don't agree on where it came from.
    Mixed,
}

impl UiField {
    /// The value in words, as the row shows it.
    pub fn value_words(&self) -> String {
        match &self.value {
            UiFieldValue::Choice { options, chosen } => chosen
                .and_then(|i| options.get(i).cloned())
                .unwrap_or_else(|| "mixed".into()),
            UiFieldValue::Number { value, unit, .. } => match value {
                Some(v) => format!("{v} {unit}").trim_end().to_string(),
                None => "mixed".into(),
            },
            UiFieldValue::Text(t) => t.clone(),
        }
    }

    /// Where it came from, in words: "set here", "from circle1's flow: one
    /// per port"; empty for nothing.
    pub fn source_words(&self) -> String {
        match &self.source {
            Some(UiSource::From { words, .. }) => words.clone(),
            Some(UiSource::SetHere) => "set here".into(),
            Some(UiSource::Mixed) => "mixed".into(),
            None => String::new(),
        }
    }
}
