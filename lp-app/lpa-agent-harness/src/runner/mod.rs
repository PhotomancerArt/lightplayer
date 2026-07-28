//! The headless agent-session runner (`agent-run` bin; `runner` feature).
//!
//! `just agent-run prompt="..."` drives ONE full agent session headlessly
//! against the real in-process studio stack and leaves triage artifacts
//! (`debug.json`, `final.glsl`, `run.md`, `triage.txt`) in the out dir.
//!
//! Module map (concept per file):
//! - [`cli_args`] — flag parsing and `--help` (spend policy included).
//! - [`spend_gate`] — the real-provider spend gate (BOTH provider env AND
//!   `LPA_SPEND_OK=1`, or a clean refusal with instructions).
//! - [`run_script`] — scripted-mode script files over the P1 doubles.
//! - [`session_runner`] — the actor/controller drive loop and artifacts.
//! - [`run_artifacts`] — out-dir layout, timestamps, writes.

pub mod cli_args;
pub mod run_artifacts;
pub mod run_script;
pub mod session_runner;
pub mod spend_gate;

pub use cli_args::{RunnerArgs, help_text, parse_args};
pub use run_script::{RunScript, ScriptEvent};
pub use session_runner::{RunSummary, run};
pub use spend_gate::{ModeDecision, RunMode, SPEND_POLICY_LINE, decide_mode};
