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
//! - [`Output`] — where pixels leave the board: a card of ports.
//! - [`layout`] — where every node sits: automatic, inside-out, derived.
//! - [`Component`] — what you draw: a path, a circle, a group. It hands out
//!   the ids of what it makes, and it holds every geometry edit.
//! - [`Object`] — what a component makes (a path, a ring), with lamps.
//!   Derived, never stored.
//! - [`Target`] — anything you can select, in ONE tree: module → fixture →
//!   group → path → lamp, fixture → circle → ring → lamp, output → port →
//!   pixel.
//! - [`patch`] — a fixture's rules, and the effective patch they make
//!   (derived, never stored).
//! - [`edges`] — the patch drawn as edges, bundled or per object;
//!   [`highlight`] — what a selection lights up across it.
//! - [`pixel_grid`] — where a port's pixels sit on its card: each one a
//!   square you can select.
//! - [`area`] — each thing's shape on screen: what's outlined is what a
//!   click hits.
//! - [`handle`] — the squares you drag to reshape the selection: a path's
//!   points, a ring's radius, a fixture's size.
//! - [`pick`] — which thing a click selects (the Figma-style drill-down,
//!   ⌥-click through overlaps).
//! - [`props`] — properties as data; the inspector is drawn from them, for
//!   one thing or many.
//! - [`Editor`] — the state, driven by input events, with no IO.
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
pub mod output;
pub mod patch;
pub mod pick;
pub mod pixel_grid;
pub mod project;
pub mod props;
pub mod rgb;
pub mod target;
pub mod tree_rows;

pub use area::Area;
pub use camera::Camera;
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
pub use output::{Output, PixelFormat, Port};
pub use patch::{EffectivePatch, PatchProblem, PatchRule, Span, Unpatched};
pub use pick::{PickMode, pick};
pub use pixel_grid::PixelGrid;
pub use project::Project;
pub use props::{Prop, PropDesc, PropEdit, PropKind, PropValue, common_props};
pub use rgb::Rgb;
pub use target::Target;
pub use tree_rows::TreeRow;
