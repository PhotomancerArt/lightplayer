//! The Graph's verbs, as offers (plan D8): `project/graph/<verb>`.
//!
//! **Key verbs.** One offer for each verb the editor has on offer right
//! now (`Editor::verbs`, which reads the same hints the page's hint bar
//! shows), with no values: it acts on the selection, as its key does.
//!
//! **Patching verbs that take values** (lean Q77, mapping rebuild M4 P06).
//! One offer for each verb with something to act on (its
//! `Editor::subjects` not empty), with typed parameters drawn from the
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
//! **Layout verbs that take values** (mapping rebuild M4 P07), the same
//! way, each with a `subject` (`layout_verb_choices.rs`): `rename` (`id`,
//! text), `repeat` and `set-count` (`count`, text), `unrepeat`, `expand`,
//! `add-ring` (`count` and `radius`, text), `add-centre-lamp`,
//! `remove-ring`, `set-split` (`split`, a choice), `set-drawn-from`
//! (`from`, a choice) and `delete`, whose ⌫ key deletes the selection and
//! whose offer takes a node, a part or a ring; and the numbers the
//! inspector's rows set (P08): `set-radius` (a ring, `radius` text),
//! `set-way-round` (a circle, `way` a choice), `set-lattice` (a grid or a
//! filled polygon; `columns`, `rows`, `spacing`, `angle`, each optional
//! text, at least one), `set-routing` (`run` a choice), `set-fixture-size`
//! (a fixture `<fixture>:`; `width`, `height`, optional text, at least
//! one), `set-colour-order` (`order` a choice) and `set-port-lamps` (a
//! port `<output>:<key>`, `lamps` text). A verb is offered whenever
//! something can take it, never because of the selection (ruling DD37),
//! which only preselects the subject.
//!
//! **Levels** (lean Q78). Delete, Everything off, Turn off, Back to the
//! flow, Remove override, Unrepeat and Remove ring are Undoable: each
//! takes something away (a part off the canvas, a segment off the line, a
//! value set here, an override, copies, a ring), and the Graph's own undo
//! puts it back. A count is Undoable when it drops copies an override is
//! on (the press says so: its action is priced by what it binds), Routine
//! otherwise. Unpatch (U) is
//! Routine: it puts the selected segments back to following their flow,
//! which turns nothing off. Everything else is Routine: selecting, the
//! tools, a count, a direction, an offset, a port, a flow, a start, Turn
//! on, undo and redo, and patching by hand (patch, swap, turn, shift),
//! which moves where things land without taking anything away. Nothing on
//! the Graph is Lasting. Moving the view (pan, zoom, fit, Canvas | Stack)
//! is view state, never an offer.

use lab_mapping_model::{Editor, GraphEdit, GraphVerb, VerbChoice, node_value};

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
    publish_key_offers(offers, editor);
    for offer in value_offers(editor) {
        offers.publish(offer);
    }
}

/// The key verbs on offer now (cheap: the hint bar's).
pub(crate) fn publish_key_offers(offers: &mut UiOfferTree, editor: &Editor) {
    for verb in editor.verbs() {
        // Delete's key acts on the selection; its offer takes a subject.
        if verb.takes_values() {
            continue;
        }
        let action = level(verb, GraphOp::action(verb));
        offers.publish(UiOffer::new(graph_verb_path(verb), icon(verb), action));
    }
}

/// The verbs that take values, each with its parameters, while it has
/// something to act on. Their lists are the costly part on a big project
/// (the radiance dome's run to thousands of options); they change only
/// with the project, its port tables and the selection, so the controller
/// keeps them between views (`GraphController::publish_offers`).
pub(crate) fn value_offers(editor: &Editor) -> Vec<UiOffer> {
    GraphVerb::WITH_VALUES
        .into_iter()
        .filter_map(|verb| {
            let subjects = editor.subjects(verb);
            (!subjects.is_empty()).then(|| value_offer(editor, verb, subjects))
        })
        .collect()
}

/// A patching verb's offer, with its parameters.
fn value_offer(editor: &Editor, verb: GraphVerb, subjects: Vec<VerbChoice>) -> UiOffer {
    let preselect = editor.selected_subject(verb, &subjects);
    let subject = OfferParam::choice("subject", subject_label(verb), options(subjects), preselect);
    let lamps = || OfferParam::text("lamps", "lamps", "a whole number of lamps");
    let count = || OfferParam::text("count", "how many", "a whole number, at least 1");
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
        GraphVerb::Rename => params.push(OfferParam::text(
            "id",
            "new id",
            "a to z first, then a to z, 0 to 9, _ or -; at most 24",
        )),
        GraphVerb::Repeat | GraphVerb::SetCount => params.push(count()),
        GraphVerb::AddRing => {
            params.push(count());
            params.push(OfferParam::text(
                "radius",
                "radius",
                "how far from the centre, in the layout's units",
            ));
        }
        GraphVerb::SetSplit => params.push(OfferParam::choice(
            "split",
            "split",
            options(editor.split_choices()),
            None,
        )),
        GraphVerb::SetDrawnFrom => params.push(OfferParam::choice(
            "from",
            "drawn from",
            options(editor.drawn_from_choices()),
            None,
        )),
        GraphVerb::SetRadius => params.push(OfferParam::text(
            "radius",
            "radius",
            "how far from the centre, in the layout's units",
        )),
        GraphVerb::SetWayRound => params.push(OfferParam::choice(
            "way",
            "way round",
            options(editor.way_round_choices()),
            None,
        )),
        GraphVerb::SetLattice => {
            let number = |name: &str, label: &str, placeholder: &str| {
                OfferParam::text(name, label, placeholder).optional()
            };
            params.push(number("columns", "columns", "a grid's, a whole number"));
            params.push(number("rows", "rows", "a grid's, a whole number"));
            params.push(number("spacing", "spacing", "lamp to lamp, more than 0"));
            params.push(number(
                "angle",
                "angle",
                "a filled polygon's lattice turned, in degrees",
            ));
        }
        GraphVerb::SetRouting => params.push(OfferParam::choice(
            "run",
            "rows run",
            options(editor.routing_choices()),
            None,
        )),
        GraphVerb::SetFixtureSize => {
            params.push(OfferParam::text("width", "width", "in the layout's units").optional());
            params.push(OfferParam::text("height", "height", "in the layout's units").optional());
        }
        GraphVerb::SetColourOrder => params.push(OfferParam::choice(
            "order",
            "colour order",
            options(editor.colour_order_choices()),
            None,
        )),
        GraphVerb::SetPortLamps => params.push(lamps()),
        _ => {}
    }
    // A count that drops copies an override is on takes something away.
    let keeps = if verb == GraphVerb::SetCount {
        editor.counts_that_keep()
    } else {
        Vec::new()
    };
    let binder = OfferBinder::new(move |args| {
        GraphEdit::bind(verb, &|name| args.get(name).map(str::to_string))
            .map(|edit| {
                let drops = match &edit {
                    GraphEdit::SetCount { subject, count } => {
                        let value = node_value(subject);
                        keeps.iter().any(|(v, keep)| *v == value && count < keep)
                    }
                    _ => false,
                };
                let action = level(verb, GraphOp::edit_action(edit));
                if drops { action.undoable() } else { action }
            })
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
        GraphVerb::Rename | GraphVerb::Repeat | GraphVerb::SetSplit | GraphVerb::SetDrawnFrom => {
            "part"
        }
        GraphVerb::Unrepeat | GraphVerb::Expand => "repeat",
        GraphVerb::SetCount => "repeat, path or ring",
        GraphVerb::AddRing | GraphVerb::AddCentreLamp | GraphVerb::SetWayRound => "circle",
        GraphVerb::RemoveRing | GraphVerb::SetRadius => "ring",
        GraphVerb::SetLattice | GraphVerb::SetRouting => "grid or filled polygon",
        GraphVerb::SetFixtureSize | GraphVerb::SetColourOrder => "fixture",
        GraphVerb::SetPortLamps => "port",
        GraphVerb::Delete => "node, part or ring",
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
        | GraphVerb::RemoveOverride
        | GraphVerb::Unrepeat
        | GraphVerb::RemoveRing => action.undoable(),
        _ => action,
    }
}

fn icon(verb: GraphVerb) -> &'static str {
    match verb {
        GraphVerb::Delete
        | GraphVerb::UnpatchAll
        | GraphVerb::RemoveOverride
        | GraphVerb::RemoveRing => "trash",
        GraphVerb::Unpatch | GraphVerb::Undo | GraphVerb::Redo | GraphVerb::BackToTheFlow => {
            "revert"
        }
        _ if verb.takes_values() => "edit",
        _ => "",
    }
}
