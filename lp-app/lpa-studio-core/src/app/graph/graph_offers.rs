//! The Graph's verbs, as offers (plan D8): `project/graph/<verb>`.
//!
//! **Key verbs.** One offer for each verb the editor has on offer right
//! now (`Editor::verbs`, which reads the same hints the page's hint bar
//! shows), with no values: it acts on the selection, as its key does.
//!
//! **Patching verbs that take values** (lean Q77, mapping rebuild M4 P06).
//! One offer for each verb with something to act on
//! (`Editor::verbs_with_values`), with typed parameters drawn from the
//! editor's own lists, in its words (`graph_verb_choices.rs`):
//!
//! - `subject`, a choice of what it acts on (a node, `<fixture>:<tree
//!   id>`, labelled "ring r3 of circle1"; a part or the fixture for a
//!   start or a flow), preselected to the selection when the selection is
//!   one. Tree ids hold `/` and `@`, so they are never a path segment.
//! - `port`, a choice of every port of every output a fixture reaches
//!   ("D1 of main"), and Next port and Stays here for `set-port`; an
//!   output with no name lists its ports disabled ("this output has no
//!   name yet", lean Q96).
//! - `lamps` as text (a whole number), `direction`, `flow`, `output`,
//!   `field` (what goes back to the flow) and `start` as choices.
//!
//! A press binds the values into the verb (`GraphEdit::bind`) and the
//! controller makes it with `Editor::edit`, the function the inspector's
//! rows and the keys run too.
//!
//! **Levels** (lean Q78). Delete, Everything off, Turn off, Back to the
//! flow and Remove override are Undoable: each takes something away (a
//! part off the canvas, a segment off the line, a value set here, an
//! override), and the Graph's own undo puts it back. Unpatch (U) is
//! Routine: it puts the selected segments back to following their flow,
//! which turns nothing off. Everything else is Routine: selecting, the
//! tools, a count, a direction, an offset, a port, a flow, a start, Turn
//! on, undo and redo, and patching by hand (patch, swap, turn, shift),
//! which moves where things land without taking anything away. Nothing on
//! the Graph is Lasting. Moving the view (pan, zoom, fit, Canvas | Stack)
//! is view state, never an offer.

use lab_mapping_model::{Editor, GraphEdit, GraphVerb, VerbChoice};

use crate::app::graph::GraphOp;
use crate::{
    OfferArgError, OfferBinder, OfferChoice, OfferParam, OfferPath, UiAction, UiOffer, UiOfferTree,
};

/// The group the Graph's verbs live in: `project/graph/…`.
pub const GRAPH_GROUP: &str = "graph";

/// Where the Graph's verb `verb` lives.
pub fn graph_verb_path(verb: GraphVerb) -> OfferPath {
    OfferPath::project().child(GRAPH_GROUP).child(verb.name())
}

/// Publish every verb the editor has on offer.
pub fn publish_graph_offers(offers: &mut UiOfferTree, editor: &Editor) {
    for verb in editor.verbs() {
        let action = level(verb, GraphOp::action(verb));
        offers.publish(UiOffer::new(graph_verb_path(verb), icon(verb), action));
    }
    for verb in editor.verbs_with_values() {
        offers.publish(value_offer(editor, verb));
    }
}

/// A patching verb's offer, with its parameters.
fn value_offer(editor: &Editor, verb: GraphVerb) -> UiOffer {
    let subject = OfferParam::choice(
        "subject",
        subject_label(verb),
        options(editor.subjects(verb)),
        editor.selected_subject(verb),
    );
    let lamps = || OfferParam::text("lamps", "lamps", "a whole number of lamps");
    let mut params = vec![subject];
    match verb {
        GraphVerb::SetOutput => params.push(OfferParam::choice(
            "output",
            "output",
            options(editor.output_choices()),
            None,
        )),
        GraphVerb::SetPort => params.push(OfferParam::choice(
            "port",
            "port",
            options(editor.port_choices(verb)),
            None,
        )),
        GraphVerb::SetStartsAt | GraphVerb::SetOffset => params.push(lamps()),
        GraphVerb::SetDirection => params.push(OfferParam::choice(
            "direction",
            "direction",
            options(editor.direction_choices()),
            None,
        )),
        GraphVerb::BackToTheFlow => params.push(OfferParam::choice(
            "field",
            "what goes back to the flow",
            options(editor.back_field_choices()),
            Some("all".to_string()),
        )),
        GraphVerb::SetFlow => params.push(OfferParam::choice(
            "flow",
            "flow",
            options(editor.flow_choices()),
            None,
        )),
        GraphVerb::SetFixtureStart => {
            params.push(OfferParam::choice(
                "port",
                "port",
                options(editor.port_choices(verb)),
                None,
            ));
            params.push(lamps());
        }
        GraphVerb::SetPartStart => {
            params.push(OfferParam::choice(
                "start",
                "start",
                options(editor.port_choices(verb)),
                None,
            ));
            params.push(lamps().optional());
        }
        _ => {}
    }
    let binder = OfferBinder::new(move |args| {
        GraphEdit::bind(verb, &|name| args.get(name).map(str::to_string))
            .map(|edit| level(verb, GraphOp::edit_action(edit)))
            .map_err(|reason| OfferArgError::Invalid {
                name: verb.name().to_string(),
                reason,
            })
    });
    UiOffer::with_params(
        graph_verb_path(verb),
        icon(verb),
        params,
        binder,
        level(verb, GraphOp::action(verb)),
    )
}

/// What a verb's `subject` is called.
fn subject_label(verb: GraphVerb) -> &'static str {
    match verb {
        GraphVerb::SetFlow => "fixture or part",
        GraphVerb::SetFixtureStart => "fixture",
        GraphVerb::SetPartStart => "part",
        GraphVerb::SetOutput
        | GraphVerb::SetPort
        | GraphVerb::SetStartsAt
        | GraphVerb::SetOffset => "segment",
        _ => "node",
    }
}

fn options(choices: Vec<VerbChoice>) -> Vec<OfferChoice> {
    choices
        .into_iter()
        .map(|c| {
            let option = OfferChoice::new(c.value, c.label);
            match c.disabled {
                Some(reason) => option.disabled(reason),
                None => option,
            }
        })
        .collect()
}

/// The verb's consequence (module docs).
fn level(verb: GraphVerb, action: UiAction) -> UiAction {
    match verb {
        GraphVerb::Delete
        | GraphVerb::UnpatchAll
        | GraphVerb::TurnOff
        | GraphVerb::BackToTheFlow
        | GraphVerb::RemoveOverride => action.undoable(),
        _ => action,
    }
}

fn icon(verb: GraphVerb) -> &'static str {
    match verb {
        GraphVerb::Delete | GraphVerb::UnpatchAll | GraphVerb::RemoveOverride => "trash",
        GraphVerb::Unpatch | GraphVerb::Undo | GraphVerb::Redo | GraphVerb::BackToTheFlow => {
            "revert"
        }
        _ if verb.takes_values() => "edit",
        _ => "",
    }
}
