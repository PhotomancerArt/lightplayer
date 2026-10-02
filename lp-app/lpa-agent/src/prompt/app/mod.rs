//! The app agent's static system prompt: who it is, the doctrine, and
//! (from P05) the generated project-model reference. Byte-stable for a
//! session — everything that changes rides the `<app_state>` block.

pub mod app_system_prompt;

pub use app_system_prompt::build_app_system_prompt;
