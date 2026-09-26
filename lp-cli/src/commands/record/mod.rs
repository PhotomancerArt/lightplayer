//! `lp-cli record`: the receiving end of Studio's session recorder
//! (`?record=<url>`).

pub mod args;
pub mod handler;
pub mod serve;

pub use args::RecordCli;
pub use handler::handle_record;
