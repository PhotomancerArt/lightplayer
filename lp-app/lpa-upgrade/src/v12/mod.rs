//! Format 12, frozen: the coordinate a shader sees at each lamp of a
//! format-12 fixture, computed from numbers alone.
//!
//! The fixture and shader space plan
//! (`lp2025/2026-10-28-0935-fixture-space-theory`, PD11) changes what a
//! lamp samples: the fixture's box, a fit and one transform per source
//! replace today's contain-fit into `render_size` and pattern mode's
//! lamp-box frame. The step that rewrites projects for it, and its
//! sample-parity test, compare against **this** copy, never against the
//! live engine, whose code changes meaning in the same change.
//!
//! Frozen from origin/main `f90a0388a` (#1319, project format 12), operation
//! for operation, so its roundings are the engine's:
//!
//! - the load-time fit (`lpc-mapping` `map2d_fit.rs` `fit_points_in_place`,
//!   framed by the view box as `lpc-engine`'s `parts_carrier.rs` reads it);
//! - the per-render pixel coordinates (`lpc-engine` `coordinates.rs`
//!   `normalized_f32_to_q16`, `normalized_q16_to_pixel_q16`;
//!   `fixture_node.rs` `DirectCoordFill`, `strip_point_coord`,
//!   `select_request_space`);
//! - the projections between a 1D and a 2D side (`coordinates.rs`
//!   `project_2d_to_1d`, `centre_scanline`; `shader_node.rs`
//!   `project_batch`; `space.rs` `resolve_1d_to_2d_with_origin`);
//! - pattern space (`products/visual/pattern_space.rs` `ScopeGeometry`,
//!   `PatternAxis`, `PatternFrame`; `shader_node.rs` `pattern_frame`) and
//!   the uniforms they bind (`shader_abi/uniforms.rs` `build_uniforms`:
//!   `outputSize`, `patternExtent`, `patternPitch`, `lampCount`).
//!
//! No live model type appears here (lamps arrive as plain positions, the
//! fixture and the shader as plain fields), so nothing the plan changes can
//! reach in. `tests/v12_sampling_live.rs` proves the copy against the live
//! engine of the commit it was frozen from, bit for bit, on every project in
//! the repo.

mod v12_coordinates;
mod v12_lamp_fit;
mod v12_pattern_frame;
mod v12_projection;
mod v12_request_space;
mod v12_sampling;

pub use v12_lamp_fit::fit_lamps;
pub use v12_pattern_frame::{V12PatternFrame, V12ScopeGeometry};
pub use v12_projection::{V12Projection, V12ProjectionShape};
pub use v12_request_space::{V12ConsumerPolicy, V12Space, select_request_space};
pub use v12_sampling::{
    V12Coords, V12Fixture, V12PatternUniforms, V12Sampled, V12Sampling, V12Source, V12SourceSpace,
    V12Uniforms, sample_coordinates,
};
