//! The station's address, for the services on `lp-net`.
//!
//! embassy-net's `Stack::wait_config_up` / `wait_config_down` park their
//! task on ONE waker slot shared by every waiter, and a second waiter's
//! registration wakes the first: with the LAN endpoint's tasks, the refuser
//! and mDNS all waiting, they woke each other forever and `lp-net` (above
//! the main task's priority) spun the CPU, so the main task never ran
//! (found booting the emulated C6, 2026-10-05). So the station task is the
//! one waiter on the stack, and publishes what it learns here: a
//! [`Watch`] whose every receiver has its own waker.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::{Receiver, Watch};

/// How many services wait on the address: the LAN slots' tasks, the
/// refuser, mDNS and the relay.
pub const WATCHERS: usize = fw_esp32_common::radio_link::NETWORK_LINK_SLOTS + 3;

/// The station's IPv4 address while it has one; `None` otherwise.
static ADDRESS: Watch<CriticalSectionRawMutex, Option<[u8; 4]>, WATCHERS> = Watch::new_with(None);

/// A service's view of the address.
pub type AddressWatch = Receiver<'static, CriticalSectionRawMutex, Option<[u8; 4]>, WATCHERS>;

/// The station task: the address changed (`None`: lost).
pub fn publish(address: Option<[u8; 4]>) {
    ADDRESS.sender().send(address);
}

/// One service's receiver. Panics past [`WATCHERS`] (a wiring mistake,
/// caught on the first boot).
pub fn watch() -> AddressWatch {
    match ADDRESS.receiver() {
        Some(receiver) => receiver,
        None => panic!("more address watchers than WATCHERS"),
    }
}

/// Wait until the station has an address; it.
pub async fn wait_up(watch: &mut AddressWatch) -> [u8; 4] {
    match watch.get_and(Option::is_some).await {
        Some(address) => address,
        None => unreachable!("get_and(is_some) returned None"),
    }
}

/// Wait until the station has no address.
pub async fn wait_down(watch: &mut AddressWatch) {
    watch.get_and(Option::is_none).await;
}
