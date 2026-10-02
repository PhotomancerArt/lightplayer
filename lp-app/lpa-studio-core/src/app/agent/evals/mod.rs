//! The app-agent evals, stage A (plan `lp2025/2026-10-01-0126-app-agent-harness`,
//! PD8): scenarios, project checks, and the headless-Studio harness that
//! runs them and writes `target/app-agent-evals/<run>/<scenario>/`.
//!
//! Test-only: the harness drives the same in-process server the edit e2e
//! tests use. Stage B (the emulated C6 decoding pad 16) is
//! `lp-cli/tests/app_agent_emu_decode.rs`. How to run both:
//! `tests/fixtures/app_agent/README.md`.

pub(crate) mod app_agent_checks;
pub(crate) mod app_agent_eval_driver;
pub(crate) mod app_agent_eval_harness;
pub(crate) mod app_agent_project_tree;
pub(crate) mod app_agent_scenario;
pub(crate) mod app_agent_transcript;

#[cfg(test)]
mod app_agent_act_tests;
#[cfg(test)]
mod app_agent_eval_tests;
