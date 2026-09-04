//! The gallery pages (P09 split, vision D9/D10/D14): Devices (the
//! runtime roster — being rebuilt, see [`devices_page`]), Projects (the
//! library), Explore (examples), and the Home landing stub — plus the
//! cards they share. One combined gallery page lived here until the
//! chrome C reorg split it.

pub(crate) mod brand_hero;
pub(crate) mod card_footer;
pub(crate) mod card_sheet;
pub(crate) mod card_thumb;
pub(crate) mod device_roster_card;
pub mod devices_page;
pub(crate) mod example_card;
pub mod explore_page;
pub(crate) mod gallery_paste;
pub(crate) mod gallery_preview;
#[cfg(feature = "stories")]
pub(crate) mod home_gallery_stories;
pub mod home_landing;
#[cfg(feature = "stories")]
pub(crate) mod home_landing_stories;
/// The `examples/logo-sign` mapping generator plus its drift gate. Test-only:
/// the running app reads the committed document, never this.
#[cfg(test)]
mod logo_sign_gen;
pub(crate) mod new_project_menu;
#[cfg(feature = "stories")]
pub(crate) mod new_project_menu_stories;
pub(crate) mod package_card;
#[cfg(feature = "stories")]
pub(crate) mod package_card_stories;
pub mod package_export;
pub mod project_opening_frame;
#[cfg(feature = "stories")]
pub(crate) mod project_opening_frame_stories;
pub mod projects_page;
pub(crate) mod sim_card;
pub(crate) mod sim_play_tab;
/// Poster capture is the wasm thumb path; host builds of this crate render
/// no live preview at all and only run the cache's unit tests.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "called only by the wasm poster-first thumb path; host builds render no preview and only run the cache unit tests"
    )
)]
pub(crate) mod thumb_poster;

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
