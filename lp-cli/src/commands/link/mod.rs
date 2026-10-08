//! `lp-cli link`: measure a board's host link.

pub mod args;
#[cfg(test)]
mod ble_pipe_fake_board;
pub mod ble_pipe_host;
pub mod blepipe_capture;
pub mod capture;
pub mod capture_requests;
pub mod capture_session;
pub mod engine_login_gate;
pub mod handler;
pub mod lab_cmd;
pub mod lab_port;
pub mod lab_run;
pub mod lan_reopen;
pub mod rtt;
mod rtt_emu_boards;

pub use args::LinkCli;
pub use handler::handle_link;
