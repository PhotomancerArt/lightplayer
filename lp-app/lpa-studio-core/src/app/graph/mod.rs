//! The Graph: the mapping lab's canvas on the open project, as a workbench
//! tab beside Nodes, Map and Patch (plan
//! `lp2025/2026-10-17-0202-mapping-lab-into-studio`).
//!
//! The lab's crates are used as they are: `lab-mapping-files` opens the
//! project's own files into the lab's model ([`GraphSource`] answers it
//! from Studio's asset cache), `lab-mapping-model`'s editor is the Graph's
//! state and verbs, and `lab-layout` places the cards. The editor is core's
//! ([`GraphController`]), shared with the page through a [`GraphHandle`]
//! so pointer input never waits on the actor; colours come from the
//! board's own published frames ([`graph_frame`]).

mod graph_canvas_meta;
mod graph_controller;
mod graph_files_key;
mod graph_frame;
mod graph_handle;
pub(crate) mod graph_highlight;
mod graph_note;
mod graph_offers;
mod graph_op;
mod graph_pictures;
mod graph_source;
mod ui_graph_view;

pub use graph_controller::GraphController;
pub use graph_files_key::GraphFilesKey;
pub use graph_handle::GraphHandle;
pub use graph_note::GraphNote;
pub use graph_offers::{GRAPH_GROUP, graph_verb_path, publish_graph_offers};
pub use graph_op::GraphOp;
pub use ui_graph_view::{UiGraphFrame, UiGraphState, UiGraphView};
