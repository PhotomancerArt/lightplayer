//! The mapping canvas as components a host mounts: the lab's own page
//! (`main.rs`), and Studio's Graph tab.
//!
//! A host owns the editor and hands the components a [`GraphEditor`]:
//! they read the editor through it and say what happened as
//! [`GraphInput`](lab_mapping_model::GraphInput)s, which the host applies.
//! The colours are the host's too: it hands [`Canvas`] the board's frame
//! (the lab simulates one with [`use_simulated_frame`]). No editor logic
//! and no detail decisions live here; what's dimmed, lit or shown is the
//! view tree's (`Editor::view_tree`).
//!
//! Everything this crate draws sits under an element with the `graph`
//! class, and its stylesheet ([`GRAPH_CSS`]) is scoped to it, so it cannot
//! leak into a host's page.

mod board_clock;
mod canvas_container;
mod dense_canvas;
mod dots_layer;
mod edge_layer;
mod faces;
mod graph_editor;
pub mod input;
mod inspector_debug;
mod lamp_cells;
mod node_card;
mod overlay;
mod panels;
mod picture_canvas;
mod piece_view;
mod text_measure;
mod view;
mod world_layer;

pub use board_clock::use_simulated_frame;
pub use canvas_container::Canvas;
pub use graph_editor::{GraphEditor, GraphSink};
pub use inspector_debug::{InspectorDebug, use_remembered_inspector_debug};
pub use panels::{HintBar, Inspector, TreePane};
pub use piece_view::PieceView;
pub use view::{View, use_view};

/// The canvas's, the panels' and the hint bar's styles, all under
/// `.graph`.
pub const GRAPH_CSS: &str = include_str!("graph.css");
