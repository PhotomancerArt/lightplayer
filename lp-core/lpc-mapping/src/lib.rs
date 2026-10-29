//! 2D LED fixture mapping, shared by the engine, the device, and Studio:
//! mapping = layout + patching.
//!
//! A fixture's mapping document (`fixture.map2d.json`) is map2d format 6
//! ([`map2d6`]): its parts render a tree whose leaves are segments
//! ([`part`]), and the patcher gives every segment its address on an
//! output's ports ([`patching`]), saved in the same file. Project format 12
//! holds format 6 only.
//!
//! The older document ([`Map2dDoc`], formats 1–5: parametric objects whose
//! vec order is the wiring order) stays for its resolver: [`resolve`] and
//! [`resolve_into`] are the new model's geometry (each shape's lamps), and
//! [`fit_points`] maps doc-space positions into a fixture render target
//! without stretching. Its reader and writer stay for what still builds
//! geometry as shapes before writing format 6 (the logo sign and small-dome
//! generators, Studio's fixture-face stories through [`corpus`]) and for the
//! format-12 upgrade's own statements, which read the old documents.
//!
//! A second document is project-level: the editor's `editor.json`
//! ([`EditorMetaDoc`]) — per-node surfaces for the editor (the Graph's
//! pinned canvas positions; an older build's Arrange placements are kept
//! unread), for human eyes only. The engine and the device never read it, and it is never a
//! sampling input (see the module doc in `editor_meta`).
//!
//! Boundary: schema + pure geometry only. No filesystem access, no engine
//! types, no UI. The crate is `no_std + alloc` and dependency-light because
//! the device resolves documents at project load.
//!
//! Determinism: resolution is pure `f32` arithmetic (trig via `libm`), so a
//! given document resolves to identical lamps on every platform. Editors
//! preview with the same code the device runs.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod corpus;
#[cfg(feature = "edit")]
pub mod edit;
mod editor_meta;
pub mod import;
pub mod map2d6;
mod map2d_doc;
mod map2d_error;
mod map2d_fit;
mod map2d_object_id;
mod map2d_resolve;
pub mod part;
pub mod patching;
mod sorted_set;

pub use editor_meta::{
    EDITOR_META_FORMAT, EditorFootprint, EditorMetaDoc, EditorMetaError, EditorNodeMeta,
    EditorSurfaceMeta, EditorTransform,
};
pub use map2d_doc::{
    DEFAULT_SAMPLE_DIAMETER, FilledPolygonShape, GridCorner, GridRouting, GridShape, MAP2D_FORMAT,
    MAX_REPEAT_COUNT, Map2dDoc, Map2dObject, Map2dShape, PathAlign, PathShape, PolygonShape,
    RepeatShape, RingDir, RingOrder, RingShape,
};
pub use map2d_error::Map2dError;
pub use map2d_fit::{Bounds2d, bounds_of_points, fit_points, fit_points_in_place, fit_scale};
pub use map2d_object_id::{MAP2D_OBJECT_ID_MAX_LEN, Map2dObjectId, ensure_object_ids};
pub use map2d_resolve::{
    ObjectInstanceSpan, ObjectSpan, ResolvedLamp, ResolvedMap2d, Rotation2d, filled_polygon_cells,
    object_instance_spans, object_instance_spans_of, object_stride, path_gap_breaks, resolve,
    resolve_into, shape_lamp_count, shape_stride,
};
