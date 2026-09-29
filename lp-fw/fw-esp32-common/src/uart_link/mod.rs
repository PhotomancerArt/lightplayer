//! The classic ESP32's UART0 host link on lp-link (plan
//! `lp2025/2026-09-28-2015-classic-uart-on-lp-link`, milestone M5 of
//! `docs/adr/2026-09-27-lp-link-one-comms-layer.md`).
//!
//! Only the board's link configuration lives here so far
//! ([`uart_link_config::uart_board_link_config`]), measured against the
//! classic's heap before any firmware is built around it (P1). The link task,
//! the server transport and the counters (the counterparts of
//! [`crate::usb_link`]'s) arrive with the firmware phase (P2), which also
//! decides how the link is shared between the classic's interrupt-executor
//! I/O task and the thread executor: `UsbLinkShared`'s `RefCell` is sound only
//! because both of its users run on one executor, and the classic's UART is
//! serviced from a preempting one
//! (`docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`).

pub mod uart_link_config;

pub use uart_link_config::uart_board_link_config;
