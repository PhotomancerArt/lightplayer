//! The home gallery (roadmap M4): a map of everywhere the user's light lives.
//!
//! Three sections — *Devices* (being rebuilt: only the live simulator's
//! card renders there today, per M2 of the device-model rebuild),
//! *Projects* (library packages), and *Examples* (embedded packages until
//! M6). The view model here is built by
//! [`StudioController`](crate::StudioController) over the M3 library API;
//! the web crate renders it and dispatches [`HomeOp`]s back through the
//! normal action path.

pub mod board_project;
pub mod card_ui_state;
pub mod embedded_example;
pub mod home_op;
pub mod home_view_builder;
pub mod pattern_from_export;
pub mod template_project;
pub mod ui_example_card;
pub mod ui_home_view;
pub mod ui_package_card;
pub mod ui_sim_card;

pub use board_project::{
    DEFAULT_STRIP_PIXELS, GenerateProjectError, GeneratedProject, generate_board_project,
};
pub use card_ui_state::{CardSheet, CardUiOp, CardUiState, CardVerb};
pub use embedded_example::{EmbeddedExample, embedded_example, embedded_examples};
pub use home_op::{HOME_NODE_ID, HomeOp, ProjectTemplate, ZipBytes};
pub use home_view_builder::{HomePoolEvidence, HomeSimEvidence, importable_patterns};
pub use pattern_from_export::project_files_from_export;
pub use template_project::template_project_files;
pub use ui_example_card::UiExampleCard;
pub use ui_home_view::UiHomeView;
pub use ui_package_card::UiPackageCard;
pub use ui_sim_card::{SIM_CARD_KEY, UiSimCard, UiSimProjectChip};
