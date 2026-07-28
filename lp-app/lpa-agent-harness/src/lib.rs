//! Dev-only shader-agent test harness.
//!
//! One home for the test doubles and assertion helpers the shader-agent
//! suites share, so every consumer exercises the SAME fakes:
//!
//! - [`ScriptedProvider`]: a [`lpa_agent::ModelProvider`] that plays back
//!   scripted turn events — the free, deterministic stand-in for the model
//!   (lpa-agent session tests, lpa-studio-core agent e2e tests).
//! - [`FakeHost`]: the canonical scripted [`lpa_agent::AgentHost`] double —
//!   source/staging/param/verdict knobs plus call recording (lpa-agent tool
//!   and session tests, the eval harness).
//! - [`eval_tasks`]: the live eval corpus — task definitions, probe
//!   assertions, env-based provider resolution with skip-cleanly logic, and
//!   the report renderer. `lpa-agent/tests/evals.rs` is the `#[ignore]`d
//!   entrypoint over this module.
//! - [`parse_dump`]: a typed parser over the agent debug-export JSON
//!   (format 1) for asserting on stop reasons, edits, and cache buckets.
//!
//! Spend policy: nothing in this crate bills tokens on its own — the
//! scripted doubles are token-free, and the eval harness only resolves a
//! live provider from explicit environment configuration (no env, clean
//! skip). Live-provider runs are supervised-only; agents enable spend only
//! with Yona's explicit per-session permission (full policy text lands with
//! the P4 docs).
//!
//! Host-side only: this crate is consumed exclusively as a dev-dependency
//! of host test builds and never rides wasm or firmware graphs.
//!
//! The doubles themselves are DEFINED in `lpa_agent::test_double` (behind
//! its `test-doubles` feature, which this crate enables) and re-exported
//! here: lpa-agent's own in-src unit tests compile the crate a second time
//! under `cfg(test)`, so a double defined downstream could never satisfy
//! that build's trait bounds. External consumers should keep importing
//! them from this crate.

pub mod dump;
pub mod eval_tasks;

pub use dump::{Dump, DumpEdit, DumpTurn, parse_dump};
pub use eval_tasks::{
    Check, EvalTask, ProviderCfg, TaskOutcome, render_report, ring_points, run_task, tasks,
    write_report,
};
pub use lpa_agent::test_double::{FakeHost, ScriptedProvider};
