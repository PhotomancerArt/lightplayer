//! The mapping design lab's model.
//!
//! Read this crate top-down as the vocabulary of the editor:
//!
//! - [`Fixture`] — the one authored thing: a bounding box and the
//!   component tree.
//! - [`Component`] — what you draw: a line, a circle, a group. It hands out
//!   the ids of what it makes, and it holds every geometry edit.
//! - [`Object`] — what a component makes (a line, a ring), with lamps.
//!   Derived, never stored.
//! - [`Target`] — anything you can select, in ONE tree: group → line →
//!   lamp, circle → ring → lamp.
//! - [`pick`] — which level a click selects (the Figma-style drill-down).
//! - [`props`] — properties as data; the inspector is drawn from them, for
//!   one thing or many.
//! - [`Editor`] — the state, driven by input events, with no IO.
//! - [`hints`] — what the screen says you can do right now.
//!
//! The behaviour is pinned by `tests/statements.rs`: one sentence, one test.
//!
//! Vision: `~/.photomancer/planning/lp2025/2026-10-10-1853-mapping-design-lab/`.

pub mod camera;
pub mod commands;
pub mod component;
pub mod component_props;
pub mod editor;
pub mod fixture;
pub mod geom;
pub mod hints;
pub mod hit_test;
pub mod notice;
pub mod object;
pub mod pick;
pub mod props;
pub mod target;
pub mod tree_rows;

pub use camera::Camera;
pub use component::{Component, ComponentId, ComponentKind, Ring};
pub use editor::{Button, Editor, Gesture, Key, Mods, Tool};
pub use fixture::Fixture;
pub use geom::{Rect, Vec2};
pub use hints::{Hint, HintBar};
pub use notice::{Notice, NoticeKind};
pub use object::{Lamp, Object, ObjectId, ObjectShape};
pub use pick::{PickMode, pick};
pub use props::{Prop, PropDesc, PropEdit, PropKind, PropValue, common_props};
pub use target::Target;
pub use tree_rows::TreeRow;
