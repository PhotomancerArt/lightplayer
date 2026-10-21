//! Fluid simulation node and solver support.
//!
//! # Two frames
//!
//! A Fluid's `coords` key picks the frame its grid and emitters live in —
//! the same key and values as a shader's (`docs/adr/2026-09-24-pattern-space.md`):
//!
//! - **Pixels** (absent, or `"pixels"` — every fluid authored before the key
//!   existed, byte-identical): the solver grid is stretched over whatever
//!   the consumer renders, margin included, and an emitter's `pos` and
//!   `radius` are normalized 0…1 over the grid, y down; `dir` is y down.
//! - **Pattern** (`"coords": "pattern"`): the grid spans the lamps' box
//!   ([`PatternGridFit`]), and emitters are read in pattern space — `pos`
//!   and `dir` y up, `radius` and the push (`velocity × intensity`) in
//!   pattern units — so `(0, 0)` is the middle of the lamps, `±extent` their
//!   edges, and a stamp is round on the lamps. Samples off the box clamp to
//!   its edge. A strip (a 1D request) lies along the grid's centre line,
//!   x −1…1 at y 0, as it does for a 2D pattern shader. Pattern mode samples
//!   at cell centres; pixels mode reads its dye half a cell up-left of where
//!   it stamps it, and keeps doing so.
//!
//! Emitters are stamped in `produce`, before this frame's consumers have
//! asked for anything, so pattern mode places them through the lamp box it
//! saw at its last sample ([`FluidNode`]); before any sample it uses the
//! grid's own texel centres.

mod emit;
mod fluid_node;
mod pattern_grid_fit;
mod sampler;
mod solver;

pub use emit::{for_each_pattern_stamp_cell, stamp_emitter, stamp_emitter_pattern};
pub use fluid_node::{FluidNode, fluid_emitters_path, fluid_output_path};
pub use pattern_grid_fit::{PatternGridFit, PatternRequestFit};
pub use sampler::{
    sample_rgb_bilinear_centred_q16, sample_rgb_bilinear_q16, sample_rgb_nearest_q16,
    sample_rgba16_bilinear_centred_q16, sample_rgba16_bilinear_q16, sample_rgba16_nearest_q16,
};
pub use solver::{DEFAULT_SOLVER_ITERATIONS, MsaFluidSolver};
