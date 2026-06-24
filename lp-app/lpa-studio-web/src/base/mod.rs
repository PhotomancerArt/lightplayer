//! Base UI building blocks.
//!
//! These components should stay independent of `lpa-studio-core`. They are
//! generic controls and display primitives that Studio could plausibly get
//! from a design-system package.

pub mod field_row;
pub mod icon;
pub mod popover;
#[cfg(feature = "stories")]
pub(crate) mod popover_stories;
pub mod tabs;

pub use field_row::FieldRow;
pub use icon::{StudioIcon, StudioIconName, action_icon_name};
pub use popover::{IconPopoverButton, PopoverPlacement};
pub use tabs::{TabItem, Tabs};
