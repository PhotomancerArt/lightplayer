#[cfg(feature = "esp32c6")]
pub mod esp32c6;

#[cfg(feature = "esp32c6")]
pub use esp32c6::{init_board, start_runtime};
