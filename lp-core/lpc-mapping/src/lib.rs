//! 2D LED fixture mapping: authored document schema and the single
//! deterministic resolver shared by the engine, the device, and Studio.
//!
//! The mapping *document* ([`Map2dDoc`]) is an opaque, format-versioned JSON
//! asset authored per fixture (e.g. `fixture.map2d.json`). It contains
//! parametric objects — grids, multi-ring circles, sampled paths, and
//! rotational repeats of any of those — whose vec order **is** the wiring
//! order. [`resolve`] turns a document into the
//! ordered lamp list (positions in doc space plus derived DMX-style
//! `{universe, channel}` addresses); [`fit_points`] maps doc-space positions
//! into a fixture render target without stretching.
//!
//! The crate owns a second document: the per-fixture **patch**
//! ([`PatchDoc`], e.g. `fixture.patch.json`), which says where a fixture's
//! lamps land on an output's wire. Mapping answers "where is this lamp?" and
//! is built once; patching answers "which jack did that strand end up in?"
//! and changes on every install — so they are stored apart and
//! [`resolve_patch`] anchors sparse entries over auto-flow.
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
pub mod import;
mod map2d_doc;
mod map2d_error;
mod map2d_fit;
mod map2d_resolve;
mod patch;

pub use map2d_doc::{
    DEFAULT_SAMPLE_DIAMETER, GridCorner, GridRouting, GridShape, MAP2D_FORMAT, MAX_REPEAT_COUNT,
    Map2dDoc, Map2dObject, Map2dShape, PathShape, RepeatShape, RingDir, RingOrder, RingShape,
};
pub use map2d_error::Map2dError;
pub use map2d_fit::{Bounds2d, bounds_of_points, fit_points};
pub use map2d_resolve::{
    CHANNELS_PER_LAMP, LAMPS_PER_UNIVERSE, LampAddress, ObjectSpan, ResolvedLamp, ResolvedMap2d,
    Rotation2d, resolve,
};
pub use patch::{
    PATCH_FORMAT, PatchAnchor, PatchDoc, PatchEntry, PatchError, PatchRange, PatchedRange,
    resolve_patch,
};
