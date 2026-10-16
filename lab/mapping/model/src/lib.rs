//! The mapping design lab's model.
//!
//! Read this crate top-down as the vocabulary of the editor:
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
//! - [`layout`] — where every node sits: automatic and inside-out, outputs
//!   beside what feeds them, except where you pinned a node by dragging it.
//! - [`Component`] — what you draw: a path, a polygon, a circle, a repeat,
//!   a group. It hands out the ids of what it makes, and it holds every
//!   geometry edit. [`resolve`] — where its lamps land, in the product's
//!   own f32 arithmetic, so they are the device's lamps bit for bit.
//! - [`Object`] — what a component makes (a path, a ring), with lamps.
//!   Derived, never stored.
//! - [`Target`] — anything you can select, in ONE tree: module → fixture →
//!   group → path → lamp, fixture → circle → ring → lamp, output → port →
//!   pixel.
//! - [`patch`] — a fixture's rules (auto, and place: this object on that
//!   port's pixel, reversed, turned), and the effective patch they make
//!   (derived, never stored).
//! - [`edges`] — the patch drawn as edges, first lamp to pixel, one per
//!   object, bundled only where they would crowd;
//!   [`highlight`] — what a selection lights up across it.
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
//! - [`Editor`] — the state, driven by input events, with no IO; "Move &
//!   draw" off holds the drawing still.
//! - [`hints`] — what the screen says you can do right now.
//! - [`board`] — the simulated board, a small recreation of the engine:
//!   render products ([`render_product`]) sampled by fixtures, fixture
//!   overrides ([`fixture_overrides`]), control products
//!   ([`ControlProduct`]) placed by the patch onto outputs, output
//!   overrides ([`output_overrides`]); [`data_path`] — the wiring order
//!   drawn inside a fixture.
//!
//! The behaviour is pinned by `tests/statements.rs`: one sentence, one test.
//!
//! Vision: `~/.photomancer/planning/lp2025/2026-10-10-1853-mapping-design-lab/`.

pub mod domain;
pub mod layout;
pub mod ux;

pub use ux::area::Area;
pub use domain::bindings::Binding;
pub use domain::board::{BoardFrame, PortFrame};
pub use ux::camera::Camera;
pub use domain::channel::Channel;
pub use domain::color_order::ColorOrder;
pub use domain::component::{Component, ComponentId, ComponentKind, Ring};
pub use domain::control_product::ControlProduct;
pub use ux::data_path::{DataEntry, DataPath};
pub use ux::edges::Edge;
pub use ux::editor::{
    Button, ContextMenu, Editor, Gesture, Key, MenuAction, MenuCommand, MenuItem, Mods, Tool,
};
pub use domain::fixture::Fixture;
pub use domain::geom::{Rect, Vec2};
pub use ux::handle::{Handle, HandleKind, SizeEdge};
pub use ux::hints::{Hint, HintBar, HintKind};
pub use layout::Layout;
pub use domain::node::{Node, NodeId, NodeKind};
pub use domain::notice::{Notice, NoticeKind};
pub use domain::object::{Lamp, Object, ObjectId, ObjectShape};
pub use domain::other_node::OtherNode;
pub use domain::output::{Output, Port};
pub use domain::patch::{EffectivePatch, Gone, PatchProblem, PatchRule, Span, Unpatched};
pub use domain::pattern::Pattern;
pub use ux::pick::{PickMode, pick};
pub use ux::pixel_grid::PixelGrid;
pub use domain::port_runs::PortRun;
pub use domain::project::Project;
pub use ux::props::{Prop, PropDesc, PropEdit, PropKind, PropValue, common_props};
pub use domain::render_product::RenderProduct;
pub use domain::render_source::RenderSource;
pub use domain::rgb::Rgb;
pub use domain::target::Target;
pub use ux::tree_rows::TreeRow;
