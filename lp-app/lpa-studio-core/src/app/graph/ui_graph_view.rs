//! The Graph tab's slice of the studio view.
//!
//! The canvas itself is drawn from core's editor through the
//! [`GraphHandle`](super::GraphHandle), not from this view: the editor's
//! view tree is built part by part, keyed, where the page draws it. What
//! rides the studio view is what the page cannot read off the editor: how
//! far the Graph has got with the open project, when core changed the
//! editor under it, and what the board shows.

use std::rc::Rc;

use lab_mapping_model::BoardFrame;

#[derive(Clone, Debug, PartialEq)]
pub struct UiGraphView {
    pub state: UiGraphState,
    /// Moves whenever core changed the editor itself (opened it, rebuilt
    /// it for a change made elsewhere): the page draws it again.
    pub generation: u64,
    /// What the board shows, from the frames it published.
    pub frame: UiGraphFrame,
}

/// How far the Graph has got with the open project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiGraphState {
    /// Reading its files.
    Opening,
    Open {
        /// The board has published frames, so the colours are its own.
        /// Until then lamps are dim and colourless, never a stand-in.
        live: bool,
    },
    /// The project couldn't be opened on the Graph, and why.
    Refused { why: String },
    /// A fixture's mapping is map2d format 6, which this Graph does not
    /// edit yet (project format 12; the new Graph is M4's): one plain
    /// sentence, and nothing that saves.
    NewMappingFormat { sentence: String },
}

/// What the Graph says over a project whose mapping is map2d format 6.
pub const NEW_MAPPING_FORMAT_SENTENCE: &str =
    "This fixture uses the new mapping format. Its editor is coming soon.";

/// The board's frame, shared: a refcount bump to clone, and equal only to
/// itself, so a new frame is always a change.
#[derive(Clone, Debug, Default)]
pub struct UiGraphFrame(pub Rc<BoardFrame>);

impl PartialEq for UiGraphFrame {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
