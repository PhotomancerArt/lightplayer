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
//! - [`Fixture`] — one physical thing: its own box, component tree and
//!   patch rules.
//! - [`Output`] — where pixels leave the board: a card of ports that make
//!   one flat wire. [`RenderSource`] — a shader or other node that makes a
//!   picture, shown as a stand-in [`Pattern`]. [`OtherNode`] — anything
//!   else a real project holds (a clock, a button): a plain card, never
//!   edited here.
//! - [`Component`] — what you draw: a path, a polygon, a circle, a repeat,
//!   a group. It hands out the ids of what it makes, and it holds every
//!   geometry edit. [`resolve`] — where its lamps land, in the product's
//!   own f32 arithmetic, so they are the device's lamps bit for bit.
//! - [`Object`] — what a component makes (a path, a ring), with lamps.
//!   Derived, never stored.
//! - [`Target`] — anything you can select, in ONE tree: module → fixture →
//!   group → path → lamp, fixture → circle → ring → lamp, output → port →
//!   pixel.
//! - [`patch`] — where lamps land: a fixture's placements
//!   ([`PatchPlace`]: wire, direction and turn, on the fixture, a component
//!   or an object, each inherited down the tree), and the effective patch
//!   they make (derived, never stored); [`port_runs`] — a port's pixels as
//!   one run per object.
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
    bindings, board, fixture_overrides, output_overrides, patch, port_runs, render_product, resolve,
};
pub use ux::{
    area, board_view, card_geometry, data_path, edges, handle, highlight, hints, layout_input,
    pick, pixel_grid, props, ui, view_tree,
};

pub use domain::bindings::Binding;
pub use domain::board::{BoardFrame, PortFrame};
pub use domain::channel::Channel;
pub use domain::color_order::ColorOrder;
pub use domain::component::{Component, ComponentId, ComponentKind, Ring};
pub use domain::control_product::ControlProduct;
pub use domain::fixture::{Fixture, Unsupported};
pub use domain::geom::{Rect, Vec2};
pub use domain::node::{Node, NodeId, NodeKind};
pub use domain::notice::{Notice, NoticeKind};
pub use domain::object::{Lamp, Object, ObjectId, ObjectShape};
pub use domain::other_node::OtherNode;
pub use domain::output::{Output, Port};
pub use domain::patch::{
    EffectivePatch, Gone, Landing, ObjectRun, Overlap, PatchProblem, Span, Unpatched,
};
pub use domain::patch_edits::{OnWire, PatchEdit, PortEdge};
pub use domain::patch_place::{Direction, PatchPlace, PatchTarget, Wire};
pub use domain::patch_resolve::Resolved;
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
pub use ux::area::Area;
pub use ux::camera::Camera;
pub use ux::data_path::{DataEntry, DataPath};
pub use ux::debug_flags::DebugFlags;
pub use ux::edges::Edge;
pub use ux::editor::{
    Button, ContextMenu, Editor, Gesture, Key, MenuAction, MenuCommand, MenuItem, Mods, Tool,
};
pub use ux::graph_input::GraphInput;
pub use ux::graph_verb::GraphVerb;
pub use ux::handle::{Handle, HandleKind, SizeEdge};
pub use ux::hints::{Hint, HintBar, HintKind};
pub use ux::patching::{ArmVerb, Armed, PairEnd};
pub use ux::pick::{PickMode, pick};
pub use ux::pixel_grid::PixelGrid;
pub use ux::port_grouping::PortGrouping;
pub use ux::props::{Prop, PropDesc, PropEdit, PropKind, PropValue, common_props};
pub use ux::tree_rows::TreeRow;
pub use ux::ui::*;
pub use ux::view_keys::{EdgesKey, NodesKey, PanelsKey};
