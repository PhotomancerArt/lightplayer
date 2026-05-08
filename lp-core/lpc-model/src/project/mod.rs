pub mod config;

pub use config::ProjectConfig;
pub use crate::sync::revision::Revision;
pub use crate::sync::current_revision::{advance_revision, current_revision, set_current_revision};
