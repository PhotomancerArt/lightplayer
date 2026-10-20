//! The home page: a map of everywhere the user's light lives.
//!
//! One page at `/` holds the sections in [`UiHomeSection`] (Online boards,
//! Connect a board, Offline boards, Other projects, Your patterns, then the
//! examples), and four tabs ([`UiHomeTab`]) that filter them. What each
//! section holds is [`UiHomeSections`], built by [`build_home_sections`] from
//! the library and the `lpa-devices` roster's own projection (sims included:
//! every runtime is a device, PD9), joined by the board↔project join
//! ([`BoardProjects`](crate::BoardProjects)). The view model here is built by
//! [`StudioController`](crate::StudioController) over the M3 library API;
//! the web crate renders it and dispatches [`HomeOp`]s back through the
//! normal action path.

pub mod board_project;
pub mod embedded_example;
pub mod home_offers;
pub mod home_op;
pub mod home_sections_builder;
pub mod home_view_builder;
pub mod pattern_from_export;
pub mod template_project;
pub mod ui_example_card;
pub mod ui_example_groups;
pub mod ui_home_section;
pub mod ui_home_sections;
pub mod ui_home_tab;
pub mod ui_home_view;
pub mod ui_open_mismatch;
pub mod ui_package_card;

pub use board_project::{
    DEFAULT_STRIP_PIXELS, GenerateProjectError, GeneratedProject, generate_board_project,
};
pub use embedded_example::{
    CatalogBucket, EmbeddedExample, canonical_example_id, embedded_example,
    embedded_example_by_slug, embedded_examples,
};
pub use home_offers::{
    NEW_PROJECT_NAME_PARAM, NEW_PROJECT_TEMPLATE_PARAM, OPEN_PROJECT_PARAM, home_offers,
    new_project_offer, open_project_offer,
};
pub use home_op::{HOME_NODE_ID, HomeOp, ProjectTemplate, ZipBytes};
pub use home_sections_builder::{build_home_sections, stamp_on_boards};
pub use home_view_builder::importable_patterns;
pub use pattern_from_export::project_files_from_export;
pub use template_project::template_project_files;
pub use ui_example_card::UiExampleCard;
pub use ui_example_groups::{
    EXAMPLE_PATTERNS_LABEL, EXAMPLE_PROJECTS_LABEL, PATTERNS_LABEL, PROJECTS_LABEL, UiExampleGroup,
    example_groups, example_page_label,
};
pub use ui_home_section::UiHomeSection;
pub use ui_home_sections::{UiHomeBoard, UiHomeBoardKind, UiHomeConnect, UiHomeSections};
pub use ui_home_tab::UiHomeTab;
pub use ui_home_view::UiHomeView;
pub use ui_open_mismatch::{UiOpenMismatch, UiRunningProject};
pub use ui_package_card::UiPackageCard;
