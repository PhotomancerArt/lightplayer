//! The GATT server: one Nordic-UART-shaped service carrying the wire.
//!
//! The UUIDs are the spike's (`tests/test_ble.rs`) on purpose, so
//! `spikes/ble-lab` and every generic BLE terminal (nRF Connect, Bluefy's
//! samples) talk to the product unchanged:
//!
//! - `6E400002-…` RX: the host writes `M!{json}\n` bytes here, in chunks of at
//!   most one ATT value (write or write-without-response), or as a long write
//!   (Prepare … Execute), which `ble_connection` re-assembles;
//! - `6E400003-…` TX: the board notifies framed server lines back, chunked to
//!   the connection's MTU.

// `#[gatt_server]` names `embassy_sync::…` by relative path and means
// trouble-host's 0.7, not the firmware's 0.8; a module-scope `use` shadows
// the extern-prelude name for this file only.
use embassy_sync_07 as embassy_sync;

use heapless::Vec;
use trouble_host::prelude::*;

/// The largest value one characteristic holds, and so one notification's
/// payload: ATT MTU 247 − 3. The host's ATT MTU is its packet size − 4, and
/// the packet pool is 251 bytes — the controller's largest ACL packet — so
/// no ATT PDU the board sends can outgrow what the controller takes
/// (docs/defects/2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md:
/// with a 255-byte pool the MTU was 251, and a 251-byte Prepare Write
/// Response failed the host's send and restarted it).
pub const NUS_VALUE_MAX: usize = 244;

/// The NUS service UUID, little-endian as it goes on air in the scan
/// response (a Web Bluetooth chooser filters on it).
pub const NUS_SERVICE_UUID_LE: [u8; 16] = [
    0x9e, 0xca, 0xdc, 0x24, 0x0e, 0xe5, 0xa9, 0xe0, 0x93, 0xf3, 0xa3, 0xb5, 0x01, 0x00, 0x40, 0x6e,
];

#[gatt_server(connections_max = 2)]
pub struct NusServer {
    pub uart: UartService,
}

#[gatt_service(uuid = "6e400001-b5a3-f393-e0a9-e50e24dcca9e")]
pub struct UartService {
    #[characteristic(
        uuid = "6e400002-b5a3-f393-e0a9-e50e24dcca9e",
        write,
        write_without_response
    )]
    pub rx: Vec<u8, NUS_VALUE_MAX>,
    #[characteristic(uuid = "6e400003-b5a3-f393-e0a9-e50e24dcca9e", notify)]
    pub tx: Vec<u8, NUS_VALUE_MAX>,
}

// The server's connection table must match the link mux's slot count.
const _: () = assert!(fw_esp32_common::radio_link::RADIO_LINK_SLOTS == 2);

/// Has the central on `conn` enabled notifications on TX? Until it has, a
/// notification is silently skipped by the host stack, so no frame may be
/// sent — the link is not open yet.
pub fn tx_subscribed(server: &NusServer<'_>, conn: &Connection<'_, DefaultPacketPool>) -> bool {
    let Some(cccd_handle) = server.uart.tx.cccd_handle else {
        return false;
    };
    server.get_cccd_table(conn).is_some_and(|table| {
        table
            .inner()
            .iter()
            .any(|(handle, cccd)| *handle == cccd_handle && cccd.raw() & 0x0001 != 0)
    })
}
