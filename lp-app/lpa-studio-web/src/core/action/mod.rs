//! Action controls for generic `UiAction` values.

pub mod action_button;
pub mod action_strip;
#[cfg(feature = "stories")]
pub(crate) mod action_strip_stories;

pub(crate) use action_button::confirmation_confirmed;
pub use action_button::{
    ActionButton, ActionButtonVariant, inline_link_row_class, menu_item_action_class,
    menu_item_destructive_action_class, outline_action_class, quiet_action_class,
    quiet_destructive_action_class, solid_action_class,
};
pub use action_strip::ActionStrip;
