mod args;
mod flash;
mod handler;
/// Chip-filtered serial port resolution. Public because every host-driven
/// hardware loop needs it: with several boards on the desk, first-match
/// picking has flashed the wrong one.
pub mod port;
mod process;
mod report;
mod trace_dir;

pub use args::FwcheckCli;
pub use handler::handle_fwcheck;
