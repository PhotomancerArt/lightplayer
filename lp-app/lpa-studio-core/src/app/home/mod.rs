//! The home gallery (roadmap M4): a map of everywhere the user's light lives.
//!
//! The home screen replaces the old dev buttons with three sections —
//! *Connected* (device cards), *Your projects* (library packages), and
//! *Examples* (embedded packages until M6). The view model here is built by
//! [`StudioController`](crate::StudioController) over the M3 library API;
//! the web crate renders it and dispatches [`HomeOp`]s back through the
//! normal action path.

pub mod embedded_example;
pub mod home_op;
pub mod home_view_builder;
pub mod ui_device_card;
pub mod ui_example_card;
pub mod ui_home_view;
pub mod ui_package_card;

pub use embedded_example::{EmbeddedExample, embedded_example, embedded_examples};
pub use home_op::{HOME_NODE_ID, HomeOp, ZipBytes};
pub use ui_device_card::{UiDeviceCard, UiDeviceCardState};
pub use ui_example_card::UiExampleCard;
pub use ui_home_view::UiHomeView;
pub use ui_package_card::{UiCardConnection, UiPackageCard};
