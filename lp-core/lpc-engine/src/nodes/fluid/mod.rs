//! Fluid simulation node and solver support.

mod emit;
mod fluid_node;
mod sampler;
mod solver;

pub use emit::stamp_emitter;
pub use fluid_node::{FluidNode, fluid_emitters_path, fluid_output_path};
pub use sampler::{
    sample_rgb_bilinear_q16, sample_rgb_nearest_q16, sample_rgba16_bilinear_q16,
    sample_rgba16_nearest_q16,
};
pub use solver::{DEFAULT_SOLVER_ITERATIONS, MsaFluidSolver};
