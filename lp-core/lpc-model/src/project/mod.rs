pub mod config;

pub use crate::sync::current_revision::{advance_revision, current_revision, set_current_revision};
pub use crate::sync::revision::Revision;
pub use config::ProjectConfig;
