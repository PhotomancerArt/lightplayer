//! The mapping design lab's model.
//!
//! Read this crate top-down as the vocabulary of the editor:
//!
//! - [`Project`] — the one authored thing, and the root module: its nodes
//!   are what's on the canvas.
//! - [`Node`] — a module (a frame around nodes), a fixture, or an output.
//!   One canvas for the whole graph (D36, D43).
//! - [`Fixture`] — one physical thing: its own box, component tree and
//!   patch rules.
//! - [`Output`] — where pixels leave the board: a card of ports that make
//!   one flat wire. [`OtherNode`] — anything else a real project holds (a
//!   shader, a clock): a plain card, never edited here.
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
//! - [`device_frame`] — the simulated board: what every port's pixels
//!   show, from the pattern, the patch, the selection and the time;
//!   [`data_path`] — the wiring order drawn inside a fixture.
//!
//! The behaviour is pinned by `tests/statements.rs`: one sentence, one test.
//!
//! Vision: `~/.photomancer/planning/lp2025/2026-10-10-1853-mapping-design-lab/`.

pub mod area;
pub mod camera;
pub mod color_order;
pub mod commands;
pub mod component;
pub mod component_props;
pub mod data_path;
pub mod device_frame;
pub mod edges;
pub mod editor;
pub mod fixture;
pub mod geom;
pub mod handle;
pub mod highlight;
pub mod hints;
pub mod layout;
pub mod node;
pub mod notice;
pub mod object;
pub mod other_node;
pub mod output;
pub mod patch;
pub mod pick;
pub mod pixel_grid;
pub mod project;
pub mod props;
pub mod resolve;
pub mod rgb;
pub mod target;
pub mod tree_rows;

pub use area::Area;
pub use camera::Camera;
pub use color_order::ColorOrder;
pub use component::{Component, ComponentId, ComponentKind, Ring};
pub use data_path::{DataEntry, DataPath};
pub use device_frame::{DeviceFrame, PortFrame};
pub use edges::Edge;
pub use editor::{
    Button, ContextMenu, Editor, Gesture, Key, MenuAction, MenuCommand, MenuItem, Mods, Tool,
};
pub use fixture::Fixture;
pub use geom::{Rect, Vec2};
pub use handle::{Handle, HandleKind, SizeEdge};
pub use hints::{Hint, HintBar, HintKind};
pub use layout::Layout;
pub use node::{Node, NodeId, NodeKind};
pub use notice::{Notice, NoticeKind};
pub use object::{Lamp, Object, ObjectId, ObjectShape};
pub use other_node::OtherNode;
pub use output::{Output, Port};
pub use patch::{EffectivePatch, Gone, PatchProblem, PatchRule, Span, Unpatched};
pub use pick::{PickMode, pick};
pub use pixel_grid::PixelGrid;
pub use project::Project;
pub use props::{Prop, PropDesc, PropEdit, PropKind, PropValue, common_props};
pub use rgb::Rgb;
pub use target::Target;
pub use tree_rows::TreeRow;
