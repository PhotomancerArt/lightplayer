//! The classic's link task, as an embassy task on the thread executor.
//!
//! Everything it does is chip-free and lives in
//! `fw_esp32_common::uart_link::run_uart_link`; this file exists because an
//! `#[embassy_executor::task]` is declared by the crate that spawns it. It
//! runs beside the server loop on the thread executor, never on io_task's
//! swi2 interrupt executor: the `Link` it owns is shared with the server
//! transport through a `RefCell`, which is sound only while every user is on
//! one executor (ruling DD20 of plan `classic-uart-on-lp-link`).

use fw_esp32_common::uart_link::{UartLinkShared, run_uart_link};

/// The host link, for the life of the boot.
#[embassy_executor::task]
pub async fn uart_link_task(shared: &'static UartLinkShared) {
    run_uart_link(shared).await
}
