//! The classic ESP32's UART0 host link on lp-link (plan
//! `lp2025/2026-09-28-2015-classic-uart-on-lp-link`, milestone M5 of
//! `docs/adr/2026-09-27-lp-link-one-comms-layer.md`).
//!
//! lp-link runs where UART0's bytes enter and leave the board, as it does on
//! the C6's USB (`crate::usb_link`), but across **two executors**, because
//! the classic's UART is serviced from a preempting one
//! (`docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`):
//!
//! - the chip crate's **I/O task** (swi2 interrupt executor, 1 ms hardware
//!   pacer) owns UART0 and only moves bytes, through
//!   [`uart_link_pipes`] — RX FIFO in, whole frames out, RX drained between
//!   the chunks of every write;
//! - the **link task** ([`uart_link_task::run_uart_link`], thread executor)
//!   owns the one [`Link`](lp_link::Link): it feeds it the bytes that
//!   arrived, queues its frames, moves log records from the ring onto the log
//!   channel, and sleeps until a timer, the I/O task's news, or a send
//!   doorbell;
//! - the **server transport** ([`uart_link_transport::UartLinkTransport`],
//!   thread executor) takes whole wire messages off the proto channel and
//!   queues replies onto it, sharing the link with the link task through a
//!   `RefCell` ([`UartLinkShared`]) — sound because both are on one executor
//!   and the I/O task never touches it (ruling DD20).
//!
//! The board's configuration is [`uart_board_link_config`], measured against
//! the classic's heap before any of this was built (P1).

pub mod uart_link_config;
pub mod uart_link_counters;
pub mod uart_link_nonce;
pub mod uart_link_pipes;
pub mod uart_link_shared;
pub mod uart_link_task;
#[cfg(feature = "server")]
pub mod uart_link_transport;

pub use uart_link_config::uart_board_link_config;
pub use uart_link_nonce::session_nonce;
pub use uart_link_shared::UartLinkShared;
pub use uart_link_task::{run_uart_link, when_drained};
#[cfg(feature = "server")]
pub use uart_link_transport::UartLinkTransport;
