//! Base UI building blocks.
//!
//! These components should stay independent of `lpa-studio-core`. They are
//! generic controls and display primitives that Studio could plausibly get
//! from a design-system package.

pub mod detail_popover;
#[cfg(feature = "stories")]
pub(crate) mod detail_popover_stories;
pub mod field_row;
pub mod icon;
pub mod icon_menu;
#[cfg(feature = "stories")]
pub(crate) mod icon_menu_stories;
pub mod popover;
#[cfg(feature = "stories")]
pub(crate) mod popover_stories;
pub mod tabs;

pub use detail_popover::{
    DetailPopover, DetailSection, DetailSectionTint, detail_popover_section_class,
};
pub use field_row::FieldRow;
pub use icon::{StudioIcon, StudioIconName, action_icon_name};
pub use icon_menu::{IconMenuButton, IconMenuTone, IconMenuVisualState};
pub use popover::{IconPopoverButton, PopoverButton, PopoverPlacement};
pub use tabs::{TabItem, Tabs};
