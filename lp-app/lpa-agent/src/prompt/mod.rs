//! System prompt assembly.

pub mod app;
pub mod builtin_reference;
pub mod system_prompt;

pub use app::build_app_system_prompt;
pub use system_prompt::build_system_prompt;
