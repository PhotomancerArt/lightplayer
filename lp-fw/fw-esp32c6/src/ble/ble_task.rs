//! Bringing BLE up, and the tasks that keep it running.
//!
//! [`start`] runs only when the device store enables BLE (PQ2): it builds the
//! controller, the host and the GATT server, and spawns three kinds of task
//! on the thread executor (never the interrupt executor — esp-rtos rules):
//!
//! - the host **runner** (trouble-host's RX/TX/control loops);
//! - the **advertiser**, which advertises whenever a connection slot is free
//!   and hands each new connection to
//! - a **connection** task (one per slot, [`RADIO_LINK_SLOTS`] of them),
//!   which is that connection's link: see `ble_connection`.
//!
//! With every slot taken the board stops advertising, so a third central
//! simply finds nothing to connect to.
//!
//! **A runner error is survivable.** The runner is run again, which resets
//! the controller; `hci_transport` turns that reset into a `Disconnection
//! Complete` for every open connection (their tasks end and free their
//! slots), and the advertiser drops whatever it was waiting on and
//! advertises again. Before 2026-09-25 the restart left the dead connections
//! open in the host and the board silent until a USB reboot
//! (docs/defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md).

use core::sync::atomic::{AtomicBool, Ordering};

use alloc::boxed::Box;
use embassy_executor::Spawner;
use embassy_futures::select::{Either3, select3};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use esp_radio::ble::controller::BleConnector;
use fw_esp32_common::radio_link::RADIO_LINK_SLOTS;
use trouble_host::prelude::*;

use super::advertising;
use super::ble_connection;
use super::hci_transport::LpHciTransport;
use super::nus_service::NusServer;
use crate::board::esp32c6::board_quirks::BoardQuirksApplied;

/// The HCI controller: esp-radio's BLE blob behind bt-hci's external
/// controller, 20 command slots (the spike's figure).
pub type BleController = ExternalController<LpHciTransport, 20>;
/// The host stack, for the life of the image.
pub type BleStack = Stack<'static, BleController, DefaultPacketPool>;
/// What the host's calls fail with.
pub type BleStackError = BleHostError<esp_radio::ble::controller::BleConnectorError>;

/// L2CAP channels: the signalling and ATT channels of every connection.
const L2CAP_CHANNELS_MAX: usize = 2 * RADIO_LINK_SLOTS;

// The host's resources, the stack and the GATT server live on the HEAP,
// leaked at `start`, not in `.bss`: static RAM on this chip comes straight out
// of the main task's stack (it is whatever `.data`/`.bss` leave), and every
// image links this module whether or not the device store turns BLE on. On
// the heap, only a board that starts BLE pays for them (~3.5 KB).

/// Which connection slots hold a connection.
static SLOT_BUSY: [AtomicBool; RADIO_LINK_SLOTS] = [AtomicBool::new(false), AtomicBool::new(false)];
/// A connection task ended and gave its slot back.
static SLOT_FREED: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// The host runner failed and is being run again: the controller has been
/// reset, so whatever the advertiser was doing is gone.
static HOST_RESTARTED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Bring BLE up and start serving it. Needs the RF-switch quirk to have run
/// first (`_quirks` is the proof). A controller that fails to start is
/// logged and leaves the board USB-only.
pub fn start(spawner: Spawner, bt: esp_hal::peripherals::BT<'static>, _quirks: BoardQuirksApplied) {
    let heap_before = esp_alloc::HEAP.used();
    let connector = match BleConnector::new(bt, Default::default()) {
        Ok(connector) => connector,
        Err(_) => {
            log::error!("[ble] controller init FAILED — BLE stays off this boot");
            return;
        }
    };
    let heap_controller = esp_alloc::HEAP.used();
    let controller: BleController = ExternalController::new(LpHciTransport::new(connector));

    let resources: &'static mut HostResources<
        DefaultPacketPool,
        RADIO_LINK_SLOTS,
        L2CAP_CHANNELS_MAX,
    > = Box::leak(Box::new(HostResources::new()));
    let stack: &'static BleStack = Box::leak(Box::new(
        trouble_host::new(controller, resources).set_random_address(advertising::static_address()),
    ));
    let Host {
        peripheral, runner, ..
    } = stack.build();

    let gap_name: &'static str = Box::leak(Box::new(advertising::mac_name())).as_str();
    let server: &'static NusServer<'static> =
        match NusServer::new_with_config(GapConfig::Peripheral(PeripheralConfig {
            name: gap_name,
            appearance: &appearance::UNKNOWN,
        })) {
            Ok(server) => Box::leak(Box::new(server)),
            Err(error) => {
                log::error!("[ble] GATT server build FAILED ({error}) — BLE stays off this boot");
                return;
            }
        };
    let heap_after = esp_alloc::HEAP.used();
    log::info!(
        "[ble] up: controller +{} B, host + GATT +{} B, heap used {} B (slots {})",
        heap_controller.saturating_sub(heap_before),
        heap_after.saturating_sub(heap_controller),
        heap_after,
        RADIO_LINK_SLOTS
    );

    spawner.spawn(runner_task(runner).unwrap());
    spawner.spawn(advertise_task(spawner, peripheral, server, stack).unwrap());
}

#[embassy_executor::task]
async fn runner_task(mut runner: Runner<'static, BleController, DefaultPacketPool>) {
    let mut restarts: u32 = 0;
    loop {
        if runner.run().await.is_err() {
            restarts = restarts.wrapping_add(1);
            log::error!(
                "[ble] host runner error — restarting it (restart {restarts}, heap free {} B)",
                esp_alloc::HEAP.free()
            );
            HOST_RESTARTED.signal(());
            Timer::after_millis(100).await;
        }
    }
}

#[embassy_executor::task]
async fn advertise_task(
    spawner: Spawner,
    mut peripheral: Peripheral<'static, BleController, DefaultPacketPool>,
    server: &'static NusServer<'static>,
    stack: &'static BleStack,
) {
    loop {
        let Some(slot) = free_slot() else {
            SLOT_FREED.wait().await;
            continue;
        };
        // A name change restarts advertising under the new name; a host
        // restart restarts it on the reset controller.
        match select3(
            advertising::advertise(&mut peripheral, server),
            advertising::name_changed(),
            HOST_RESTARTED.wait(),
        )
        .await
        {
            Either3::First(Ok(conn)) => {
                SLOT_BUSY[slot].store(true, Ordering::Relaxed);
                match connection_task(conn, slot, server, stack) {
                    Ok(token) => spawner.spawn(token),
                    Err(_) => {
                        // The pool is sized to the slots, so this is a bug;
                        // the connection is dropped (disconnected) here.
                        log::error!("[ble] no connection task free for slot {slot}");
                        SLOT_BUSY[slot].store(false, Ordering::Relaxed);
                    }
                }
            }
            Either3::First(Err(_)) => {
                log::warn!("[ble] advertising failed; retrying in 1 s");
                Timer::after_secs(1).await;
            }
            Either3::Second(()) => {}
            Either3::Third(()) => log::warn!("[ble] host restarted — advertising again"),
        }
    }
}

#[embassy_executor::task(pool_size = RADIO_LINK_SLOTS)]
async fn connection_task(
    conn: GattConnection<'static, 'static, DefaultPacketPool>,
    slot: usize,
    server: &'static NusServer<'static>,
    stack: &'static BleStack,
) {
    ble_connection::serve(conn, slot, server, stack).await;
    SLOT_BUSY[slot].store(false, Ordering::Relaxed);
    SLOT_FREED.signal(());
}

fn free_slot() -> Option<usize> {
    SLOT_BUSY
        .iter()
        .position(|busy| !busy.load(Ordering::Relaxed))
}
