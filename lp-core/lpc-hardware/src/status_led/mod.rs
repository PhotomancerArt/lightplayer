//! A board's status LED: which pin it is, and what it shows.
//!
//! Some dev boards carry a small LED of their own, separate from any LED
//! strip a project drives. LightPlayer uses it to say what the firmware is
//! doing — booting, running, powering off — so a board with its strip dark
//! still answers "is LightPlayer alive?" at a glance.
//!
//! This is the pure half: [`board_status_led_for`] says where the LED is, and
//! [`StatusLedState::lit_at`] says whether it is lit at a moment. The
//! firmware owns the pin and the clock (`fw-esp32c6`'s `status_led.rs`).

pub mod board_status_led;
pub mod status_led_state;
