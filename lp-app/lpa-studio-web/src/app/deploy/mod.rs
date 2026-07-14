//! The deploy dialog (M5): the single surface for everything that moves
//! a project onto hardware.

pub mod deploy_dialog;
#[cfg(feature = "stories")]
pub(crate) mod deploy_dialog_stories;

pub use deploy_dialog::DeployDialog;
