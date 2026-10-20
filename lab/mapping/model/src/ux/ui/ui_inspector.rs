//! The inspector: where you are, and the selection's sections (Studio's
//! tabs and sections, flattened: the lab has one tab).

use crate::domain::notice::Notice;
use crate::domain::target::Target;
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
    pub items: Vec<UiItem>,
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
    Problem(Notice),
    /// A shader's GLSL, as it is.
    Code(String),
    /// A property you can edit, locked while Move & draw is off and it
    /// reshapes.
    Prop {
        prop: Prop,
        locked: bool,
    },
    /// A quiet line saying what to do.
    Hint(String),
    /// A fact with the verb that changes it beside it, as a button that
    /// presses the verb (`GraphInput::Verb`): "On D10 at 0 · Unpatch".
    Action {
        text: String,
        verb: GraphVerb,
    },
    /// A Patch row with a choice (`patch_section.rs`): `chosen` is `None`
    /// when the selection doesn't share one.
    PatchChoice {
        label: &'static str,
        options: Vec<String>,
        chosen: Option<usize>,
        field: PatchField,
    },
    /// A button that selects something else: a span's object.
    Select {
        text: String,
        target: Target,
    },
    /// A gap's "Patch here": the objects not on the wire, the chosen one put
    /// at the gap's start.
    PatchHere {
        options: Vec<String>,
    },
    /// A Patch row with a number and − + (`None` when mixed).
    PatchNumber {
        label: &'static str,
        value: Option<i64>,
        note: Option<String>,
        field: PatchField,
    },
}
