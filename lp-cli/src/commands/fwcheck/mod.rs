mod args;
mod flash;
mod handler;
// `pub(crate)`: `firmware install` reuses the port resolver (`resolve_by_mac`
// / `resolve_checked`) rather than re-implementing board-port discovery.
pub(crate) mod port;
mod process;
mod report;
mod trace_dir;

pub use args::FwcheckCli;
pub use handler::handle_fwcheck;
