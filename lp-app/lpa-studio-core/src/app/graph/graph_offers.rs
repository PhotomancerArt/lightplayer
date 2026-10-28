//! The Graph's verbs, as offers (plan D8): `project/graph/<verb>`, one for
//! each verb the editor has on offer right now (`Editor::verbs`, which
//! reads the same hints the page's hint bar shows), so the bar, the
//! toolbar and the agent see one set.
//!
//! **Levels.** Delete and Unpatch all are Undoable: they take something
//! away (a part off the canvas, or every segment of a fixture turned off),
//! and the Graph's own undo puts it back. Unpatch is Routine: it puts the
//! selected segments back to following their rule, which turns nothing off.
//! Everything else is Routine too: selecting, the tools, a count, a
//! direction, undo and redo, and the rest of patching by hand (patch, swap,
//! turn, shift), which moves where things land without taking anything
//! away. Nothing on the Graph is Lasting. Moving the view (pan, zoom, fit,
//! Canvas | Stack) is view state, never an offer.

use lab_mapping_model::{Editor, GraphVerb};

use crate::app::graph::GraphOp;
use crate::{OfferPath, UiOffer, UiOfferTree};

/// The group the Graph's verbs live in: `project/graph/…`.
pub const GRAPH_GROUP: &str = "graph";

/// Where the Graph's verb `verb` lives.
pub fn graph_verb_path(verb: GraphVerb) -> OfferPath {
    OfferPath::project().child(GRAPH_GROUP).child(verb.name())
}

/// Publish every verb the editor has on offer.
pub fn publish_graph_offers(offers: &mut UiOfferTree, editor: &Editor) {
    for verb in editor.verbs() {
        let action = GraphOp::action(verb);
        let action = match verb {
            GraphVerb::Delete | GraphVerb::UnpatchAll => action.undoable(),
            _ => action,
        };
        offers.publish(UiOffer::new(graph_verb_path(verb), icon(verb), action));
    }
}

fn icon(verb: GraphVerb) -> &'static str {
    match verb {
        GraphVerb::Delete | GraphVerb::UnpatchAll => "trash",
        GraphVerb::Unpatch | GraphVerb::Undo | GraphVerb::Redo => "revert",
        _ => "",
    }
}
