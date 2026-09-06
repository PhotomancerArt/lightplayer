//! `lp-cli validate` — the front door to the hardware-validation system.
//!
//! Argument parsing and nothing else; every command is a function in
//! `lp_emu_validate::run`, which is testable without a process. See
//! `lp-emu/lp-emu-validate/README.md` for what a payload, a configuration, a
//! transcript and a replay are.

mod args;
mod handler;

pub use args::ValidateCli;
pub use handler::handle_validate;
