//! The app-agent evals and the agent activity corpus (plans
//! `lp2025/2026-10-01-0126-app-agent-harness`, PD8, and
//! `lp2025/2026-10-01-1255-agentic-ui-roadmap/m-agent-activity-corpus`):
//! scenarios, checks, the person's scripted side, and the two seats a
//! scenario runs in — a headless Studio over an in-process server, and the
//! device bench with a fake board — writing
//! `target/app-agent-evals/<run>/<scenario>/`.
//!
//! Test-only: the harness drives the same in-process server the edit e2e
//! tests use. Stage B (the emulated C6 decoding the scenario's pad) is
//! `lp-cli/tests/app_agent_emu_decode.rs`. How to run them all:
//! `tests/fixtures/app_agent/README.md`.

pub(crate) mod app_agent_check_spec;
pub(crate) mod app_agent_checks;
pub(crate) mod app_agent_conversation_checks;
pub(crate) mod app_agent_eval_driver;
pub(crate) mod app_agent_eval_harness;
pub(crate) mod app_agent_project_tree;
pub(crate) mod app_agent_scenario;
pub(crate) mod app_agent_scenario_seat;
pub(crate) mod app_agent_transcript;
pub(crate) mod app_agent_user_side;

#[cfg(test)]
mod app_agent_act_tests;
#[cfg(test)]
mod app_agent_activity_tests;
#[cfg(test)]
mod app_agent_corpus_tests;
#[cfg(test)]
mod app_agent_eval_tests;
