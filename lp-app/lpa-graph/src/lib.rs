//! The mapping design lab's model, in two halves (architecture §2). Read
//! it top-down as the vocabulary of the editor.
//!
//! **[`domain`]: what is stored, and the logic over it.**
//!
//! - [`Project`] — the one authored thing, and the root module: its nodes
//!   are what's on the canvas.
//! - [`Node`] — a module (a frame around nodes), a render source (a
//!   shader), a fixture, or an output: the product's node kinds. One
//!   canvas for the whole graph (D36, D43).
//! - [`Channel`] — the bus channels nodes write and read, the product's
//!   wiring; [`bindings`] — which node feeds which over them, the edges
//!   between nodes.
//! - [`Fixture`] — one physical thing: its own box, and its mapping (an
//!   `lpc_mapping::part::Mapping`: its parts and its patching, as its
//!   map2d format 6 file holds them).
//! - [`Output`] — where pixels leave the board: a card of ports that make
//!   one flat wire. [`RenderSource`] — a shader or other node that makes a
//!   picture, shown as a stand-in [`Pattern`]. [`OtherNode`] — anything
//!   else a real project holds (a clock, a button): a plain card, never
//!   edited here.
//! - Parts — what you draw: a path, a polygon, a circle, a grid, points, a
//!   repeat, a group (`lpc_mapping::part::Part`, the engine's own model).
//!   Rendered by `lpc_mapping::part::render` into a tree whose leaves are
//!   segments, with the engine's arithmetic, so their lamps are the
//!   device's lamps bit for bit. [`PartRef`] and [`TreeRef`] name a part or
//!   a tree node with its fixture; [`DrawnSegment`] is a segment as the
//!   canvas draws it, in world units.
//! - [`Target`] — anything you can select, in ONE tree: module → fixture →
//!   group → path → lamp, fixture → circle → ring → lamp, output → port →
//!   pixel.
//! - [`patch`] — where lamps land: each fixture's mapping patched by
//!   `lpc_mapping::patching::patch_mapping` (the engine's patcher) against
//!   its port tables ([`port_tables`]), into the effective patch (derived,
//!   never stored); [`walked`] — what each node did on the walk, and why;
//!   [`port_runs`] — a port's pixels as one run per segment.
//! - [`Placement`] — where each node sits, in world units, and each
//!   fixture's frame (where its own units land, scaled when the stack draws
//!   it bigger): a fact the view hands over. The model never lays anything
//!   out (D63).
//! - [`board`] — the simulated board, a small recreation of the engine:
//!   render products ([`render_product`]) sampled by fixtures, fixture
//!   overrides ([`fixture_overrides`]), control products
//!   ([`ControlProduct`]) placed by the patch onto outputs, output
//!   overrides ([`output_overrides`]).
//!
//! **[`ux`]: the editor's state and every user verb.**
//!
//! - [`Editor`] — the state, driven by input events, with no IO; "Move &
//!   draw" off holds the drawing still. [`GraphInput`] is every input as
//!   one value, so a page in the lab and one in Studio say the same thing.
//! - [`layout_input`] — what the view lays out: each node's role, size and
//!   module, the dataflow, and pins while pinning is on (D64). The page
//!   runs `lab-layout` and hands the placement back.
//! - [`card_geometry`] — a card's own size and rows, declared by core.
//! - [`edges`] — the wiring between nodes, one edge per pair, and what each
//!   says; [`highlight`] — what a selection lights up across the patch.
//! - [`pixel_grid`] — where a port's pixels sit on its card: each one a
//!   square you can select.
//! - [`area`] — each thing's shape on screen: what's outlined is what a
//!   click hits.
//! - [`handle`] — the squares you drag to reshape the selection: a path's
//!   points (added and removed too), a ring's radius, a fixture's size.
//! - [`pick`] — which thing a click selects (the Figma-style drill-down,
//!   ⌥-click through overlaps).
//! - [`props`] — properties as data; the inspector is drawn from them, for
//!   one thing or many.
//! - [`hints`] — what the screen says you can do right now.
//! - [`data_path`] — the wiring order drawn inside a fixture;
//!   [`board_view`] — the board as the editor shows it.
//! - [`view_tree`] — what the page draws, as one tree with no positions
//!   ([`UiCanvasView`], the `Ui…` types in [`ui`]): every node with its
//!   face, detail and emphasis, the edges and what they say, the overlay,
//!   the inspector, the tree pane, the hints. Every decision the page used
//!   to make is a field here.
//!
//! The behaviour is pinned by `tests/statements.rs`: one sentence, one test.
//!
//! Vision: `~/.photomancer/planning/lp2025/2026-10-10-1853-mapping-design-lab/`.

pub mod domain;
pub mod ux;

pub use domain::{
    bindings, board, fixture_overrides, output_overrides, patch, port_runs, port_tables,
    render_product, walked,
};
pub use ux::{
    area, board_view, card_geometry, data_path, edges, handle, highlight, hints, layout_input,
    pick, pixel_grid, props, ui, view_tree,
};

pub use domain::bindings::Binding;
pub use domain::board::{BoardFrame, PortFrame};
pub use domain::channel::Channel;
pub use domain::color_order::ColorOrder;
pub use domain::control_product::ControlProduct;
pub use domain::drawn_segment::{DrawnSegment, DrawnShape, Lamp};
pub use domain::fixture::{Fixture, Unsupported};
pub use domain::geom::{Rect, Vec2};
pub use domain::node::{Node, NodeId, NodeKind};
pub use domain::notice::{Notice, NoticeKind};
pub use domain::other_node::OtherNode;
pub use domain::output::{Output, Port};
pub use domain::part_ref::{PartRef, TreeRef};
pub use domain::patch::{
    EffectivePatch, Landing, Overlap, PatchProblem, PixelRun, ProblemKind, Unpatched,
};
pub use domain::patch_edits::{OnWire, PatchEdit, PortEdge};
pub use domain::pattern::Pattern;
pub use domain::picture::{Picture, picture_rect};
pub use domain::piece::{Difference, DifferenceKind, Piece};
pub use domain::placement::Placement;
pub use domain::port_runs::PortRun;
pub use domain::project::Project;
pub use domain::render_product::RenderProduct;
pub use domain::render_source::RenderSource;
pub use domain::rgb::Rgb;
pub use domain::target::Target;
pub use domain::walked::{Cursor, Walked};
pub use ux::area::Area;
pub use ux::camera::Camera;
pub use ux::data_path::{DataEntry, DataPath};
pub use ux::debug_flags::{DebugFlags, Presentation};
pub use ux::edges::Edge;
pub use ux::editor::{
    Button, ContextMenu, Editor, Gesture, Key, MenuAction, MenuCommand, MenuItem, Mods, Tool,
};
pub use ux::graph_edit::{
    BackField, GraphEdit, PartStart, PortChoice, RuleSubject, flow_value, node_value, port_value,
};
pub use ux::graph_input::GraphInput;
pub use ux::graph_layout_edit::{
    DRAWN_FROM, DeleteSubject, LatticeValues, Length, ROUTINGS, Split, Units, WAYS_ROUND,
    delete_value, drawn_from_label, drawn_from_name, part_value, routing_name, way_round_name,
};
pub use ux::graph_verb::GraphVerb;
pub use ux::graph_verb_choices::VerbChoice;
pub use ux::handle::{Handle, HandleKind, SizeEdge};
pub use ux::hints::{Hint, HintBar, HintKind};
pub use ux::patch_section::{PatchField, PatchInput, PatchOption};
pub use ux::patching::{ArmVerb, Armed, PairEnd};
pub use ux::pick::{PickMode, pick};
pub use ux::pixel_grid::PixelGrid;
pub use ux::port_grouping::PortGrouping;
pub use ux::port_rows::{PortRow, RowOverlap, RowSpan};
pub use ux::problem_words::ProblemLine;
pub use ux::props::{Prop, PropDesc, PropEdit, PropKind, PropValue, common_props};
pub use ux::tree_rows::TreeRow;
pub use ux::ui::*;
pub use ux::view_keys::{EdgesKey, NodesKey, PanelsKey};
