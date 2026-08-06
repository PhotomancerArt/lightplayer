//! The gallery pages (P09 split, vision D9/D10/D14): Devices (the
//! roster + creation flow), Projects (the library), Explore (examples),
//! and the Home landing stub — plus the cards they share. One combined
//! gallery page lived here until the chrome C reorg split it.

pub(crate) mod card_sheet;
pub(crate) mod card_thumb;
pub(crate) mod device_card;
pub mod devices_page;
pub(crate) mod example_card;
pub mod explore_page;
pub(crate) mod failure_report;
pub(crate) mod gallery_paste;
pub(crate) mod gallery_preview;
#[cfg(feature = "stories")]
pub(crate) mod home_gallery_stories;
pub mod home_landing;
pub(crate) mod package_card;
pub(crate) mod package_export;
pub mod project_opening_frame;
#[cfg(feature = "stories")]
pub(crate) mod project_opening_frame_stories;
pub mod projects_page;
pub(crate) mod setup_wizard;
#[cfg(feature = "stories")]
pub(crate) mod setup_wizard_stories;

pub use devices_page::DevicesPage;
pub use explore_page::ExplorePage;
pub use home_landing::HomePage;
pub use project_opening_frame::ProjectOpeningFrame;
pub use projects_page::ProjectsPage;

/// Shared section-header treatment across the gallery pages.
pub(crate) fn section_title_class() -> &'static str {
    "tw:m-0 tw:text-xs tw:font-extrabold tw:uppercase tw:leading-none tw:text-heading"
}

/// The compact card grid (Projects / Explore).
pub(crate) fn card_grid_class() -> &'static str {
    "tw:grid tw:grid-cols-[repeat(auto-fill,minmax(200px,1fr))] tw:gap-3.5"
}

/// The DEVICE roster grid (Yona hardware-walk feedback, width revised
/// down with the 2026-07-26 state-flow review): wide-enough columns and a
/// stretched row floor so the common operations — tab switches, opening a
/// sheet — resize the card WITHIN its footprint instead of reflowing the
/// page. Cards stretch to the row (grid default `align-items: stretch`),
/// so the min-height lives on the row, not the card component.
pub(crate) fn device_grid_class() -> &'static str {
    "tw:grid tw:grid-cols-[repeat(auto-fill,minmax(320px,1fr))] tw:gap-3.5 tw:[grid-auto-rows:minmax(300px,auto)]"
}
