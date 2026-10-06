//! Test support (feature `test-support`): an in-memory NOR flash with power
//! cuts, a model board over it, and a rig that drives a [`BoardSession`]
//! the way the firmware does. `lpc-update`'s own tests and `lpa-update`'s
//! host × board simulation both use it.
//!
//! **Everything here is a model, documented as one** — never the split
//! image's formats. It exists so the session's rules can be tested against
//! a flash that cuts at any operation.
//!
//! [`BoardSession`]: crate::board::BoardSession

pub mod board_rig;
pub mod fake_board;
pub mod model_build;
pub mod nor_flash;

pub use board_rig::BoardRig;
pub use fake_board::{
    BootFault, FakeBoard, MODEL_BLOCK, MODEL_PROGRESS, MODEL_REGION_START, RunningCore,
};
pub use model_build::ModelBuild;
pub use nor_flash::NorFlash;
