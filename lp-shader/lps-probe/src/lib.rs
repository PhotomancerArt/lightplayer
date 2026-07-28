//! Experiment/probe evaluation for GLSL shaders on the LPIR f32 interpreter.
//!
//! Given user GLSL and an [`ExperimentSpec`], [`run_experiment`] compiles the
//! shader (naga frontend → LPIR) plus per-probe wrapper functions, evaluates
//! them on the f32 LPIR interpreter, and returns structured diagnostics, an
//! always-on [`HealthReport`], and per-probe results. This is the pure core of
//! the shader agent's `iterate` tool: sans-IO, `no_std` + `alloc`, and it also
//! compiles for `wasm32-unknown-unknown` (it runs inside the Studio wasm app).
//!
//! See `README.md` for the API overview, the determinism contract, the
//! oracle-semantics stance (f32 interpreter, not Q32 device semantics), and
//! the evaluation caps.

#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod canonical_unit;
pub mod diff;
pub mod experiment;
pub mod experiment_result;
pub mod experiment_spec;
pub mod health;
pub mod interp_harness;
pub mod probe_domain;
pub mod probe_reduce;
pub mod probe_spec;
pub mod probe_wrapper;
pub mod uniform_walk;

pub use diff::{DiffReport, diff_experiments};
pub use experiment::{
    ExperimentRunner, MAX_EVALS_PER_PROBE, MAX_OPS_PER_EVAL, MAX_PROBES, MAX_RAW_VALUES,
    MAX_TOTAL_EVALS, run_experiment,
};
pub use experiment_result::{
    ExperimentResult, ProbeHistogram, ProbeOutcome, ProbeStats, ProbeStepHistogram, ProbeStepStats,
    ProbeValueRow, ProbeValues, ShaderCompileOutcome, round_sig4,
};
pub use experiment_spec::{BindingValue, ExperimentSpec, LedPoint};
pub use health::HealthReport;
pub use interp_harness::{CompiledShader, InterpInstance};
pub use probe_domain::ProbeDomain;
pub use probe_reduce::ProbeReduce;
pub use probe_spec::{ProbeSpec, ProbeType, ProbeVary};
pub use uniform_walk::{RESERVED_UNIFORM, UniformLeaf, top_level_uniform_names, uniform_leaves};
