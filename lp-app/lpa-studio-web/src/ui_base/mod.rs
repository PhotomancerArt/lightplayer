//! Base UI building blocks.
//!
//! These components should stay independent of `lpa-studio-ux`. They are
//! generic controls and display primitives that Studio could plausibly get
//! from a design-system package.

pub mod field_row;
pub mod icon;
pub mod tabs;

pub use field_row::FieldRow;
pub use icon::{StudioIcon, StudioIconName, action_icon_name};
pub use tabs::{TabItem, Tabs};
