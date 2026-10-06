//! The `lp-net` thread: the IP stack and everything on it (plan MD6, Q5).
//!
//! A second preemptive esp-rtos thread beside the USB link's `lp-io`
//! ([`crate::io_thread`]), created the same way
//! (`esp_radio_rtos_driver::task_create`), at [`PRIORITY`] — above the main
//! task (0), below esp-radio's own threads — with its own
//! [`esp_rtos::embassy::Executor`]. It runs embassy-net's runner over the
//! station's frame device (the radio's, or the network seam's on an emulated
//! board that engaged it: `net_bringup`), the station task, and (P04/P05)
//! the LAN endpoint and mDNS. G0 rule (a) of the seams foundation: whatever
//! the network wakes runs here, never on the main or render executor (the
//! network seam's wake handler wakes only tasks on this thread).
//!
//! - **In the core.** `core_boot` starts it, so a split image's core holds
//!   the station, the IP stack and the endpoints; an engine-less core still
//!   comes up on Wi-Fi (OTA M8's rule).
//! - **The stack** is [`STACK_BYTES`], starting at 8 KB (plan P03): the
//!   secure handshake (P04) needs ~3.9 KB on top of the tasks' own frames.
//!   `net_thread_stack_diag` paints it and logs its high-water as
//!   `[netstack]`; P08 sets the figure from that measurement.
//! - **Memory.** embassy-net's `StackResources` and every socket buffer come
//!   off the heap once, here, at bring-up — never per connection, never in
//!   `.bss` (static RAM comes out of the main stack on this chip).

use alloc::boxed::Box;
use alloc::string::String;
use core::ffi::c_void;

use embassy_net::{Config, Runner, Stack, StackResources};
use esp_radio::wifi::{Interface, WifiController};
use fw_esp32_common::radio_link::SharedPort;

use super::esp_frame_device::C6FrameDevice;

/// The thread's stack, bytes.
pub const STACK_BYTES: usize = 8 * 1024;
/// The thread's priority: the link thread's.
pub const PRIORITY: u32 = 1;
/// embassy-net's socket slots: DHCP, two LAN links and the listener that
/// refuses a third (P04), mDNS (P05), and one spare.
pub const SOCKET_SLOTS: usize = 6;

/// What the thread takes ownership of.
struct Args {
    controller: WifiController<'static>,
    station: Interface<'static>,
    host: String,
    seed: u64,
    port: SharedPort,
}

// SAFETY: `Args` is moved to the new thread exactly once, through
// `task_create`'s parameter, and never touched by the creating thread again.
// esp-radio's controller and interface are handles onto driver state that
// esp-radio itself guards; they are not tied to the thread that made them.
unsafe impl Send for Args {}

/// Start `lp-net` with the radio's controller and station interface. `host`
/// is the board's LAN name; `seed` seeds the IP stack's port and sequence
/// randomness.
pub fn start(
    controller: WifiController<'static>,
    station: Interface<'static>,
    host: String,
    seed: u64,
    port: SharedPort,
) {
    let args = Box::into_raw(Box::new(Args {
        controller,
        station,
        host,
        seed,
        port,
    }));
    esp_println::println!("[INIT] net thread: stack {STACK_BYTES} B, priority {PRIORITY}");
    // SAFETY: `args` is a leaked `Box<Args>` handed to the new thread, which
    // takes it back exactly once, in `entry`; `entry` never returns.
    unsafe {
        esp_radio_rtos_driver::task_create(
            "lp-net",
            entry,
            args.cast::<c_void>(),
            PRIORITY,
            None,
            STACK_BYTES,
        );
    }
}

extern "C" fn entry(param: *mut c_void) {
    #[cfg(feature = "net_thread_stack_diag")]
    crate::net::net_thread_stack_diag::paint();
    // SAFETY: `param` is the `Box<Args>` `start` leaked for this thread, and
    // this is the one place that takes it back.
    let args = unsafe { Box::from_raw(param.cast::<Args>()) };
    let Args {
        controller,
        station,
        host,
        seed,
        port,
    } = *args;
    // No IPv4 config until the station associates: DHCP starts on link-up.
    let resources = Box::leak(Box::new(StackResources::<SOCKET_SLOTS>::new()));
    // The radio, or (an emulated board that engaged `net=lan`) the network
    // seam: one read of its engaged byte (`net_bringup`).
    let (device, control) = super::net_bringup::choose(controller, station);
    let (stack, runner) = embassy_net::new(device, Config::default(), resources, seed);
    let executor = Box::leak(Box::new(esp_rtos::embassy::Executor::new()));
    executor.run(move |spawner| {
        spawner.spawn(net_runner(runner).unwrap());
        spawner.spawn(super::station_task::station_task(control, stack, host).unwrap());
        spawn_services(spawner, stack, port);
    })
}

/// The base MAC (the BLE name's and the mDNS name's source).
pub fn base_mac() -> [u8; 6] {
    let mac = esp_hal::efuse::base_mac_address();
    let mut bytes = [0u8; 6];
    let have = mac.as_bytes();
    let n = have.len().min(6);
    bytes[..n].copy_from_slice(&have[..n]);
    bytes
}

/// The services on the stack: the LAN endpoint (one task per LAN slot, and
/// the one that turns a third connection away) and the mDNS responder.
fn spawn_services(spawner: embassy_executor::Spawner, stack: Stack<'static>, port: SharedPort) {
    for lan in 0..fw_esp32_common::radio_link::LAN_LINK_SLOTS {
        spawner.spawn(super::lan_endpoint_task::lan_link_task(stack, port, lan).unwrap());
    }
    spawner.spawn(super::lan_endpoint_task::refuse_task(stack).unwrap());
    let mac = base_mac();
    let label = fw_esp32_common::net::mdns::mdns_label(mac);
    spawner.spawn(super::mdns_task::mdns_task(stack, label, mac).unwrap());
}

/// The lock the radio-link port takes around every borrow once the LAN's
/// links are served from this thread (`RadioLinkPort::leak_locked`): the
/// link thread's lock family (`io_thread`), a priority-limited lock at
/// priority 1, so no thread switch lands while either side holds a link and
/// the RMT refill (top priority) is never held off. Plan A1: it stays this
/// lock unless the desk walk measures a hold over 50 µs or Wi-Fi RX drops
/// under it.
pub fn port_lock(f: &mut dyn FnMut()) {
    PORT_LOCK.lock(f);
}

static PORT_LOCK: esp_hal::sync::RawPriorityLimitedMutex =
    esp_hal::sync::RawPriorityLimitedMutex::new(esp_hal::interrupt::Priority::Priority1);

#[embassy_executor::task]
async fn net_runner(mut runner: Runner<'static, C6FrameDevice>) -> ! {
    runner.run().await
}
