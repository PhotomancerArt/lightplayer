//! Composed render data for reusable Studio view surfaces.
//!
//! View models in this module are larger than individual controls. They describe
//! pane bodies, workflows, activities, and other reusable surfaces that web
//! components can render without knowing the Studio app domain that produced
//! them.

pub mod activity_view;
pub mod pane_view;
pub mod progress_state;
pub mod steps_view;
pub mod view_content;
