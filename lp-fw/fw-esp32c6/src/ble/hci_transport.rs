//! The HCI transport between the host stack and esp-radio's controller.
//!
//! esp-radio's `BleConnector` is bt-hci's `Transport` already; this wrapper
//! stands between it and the host for one reason: a **controller reset must
//! close every connection in the host too**.
//!
//! trouble-host 0.6 answers a runner error (`ble_task::runner_task`) by
//! running its bring-up again, and the bring-up begins with an HCI `Reset`.
//! The controller drops every connection on a reset and says nothing about
//! it, so without this the host kept each dead connection open for good: its
//! connection task never ended, its slot was never freed, and the board never
//! advertised again (docs/defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md).
//! Here, every event that opens or closes a connection is recorded
//! ([`HciConnectionLedger`]), and when the host writes a `Reset` the
//! connections still open are handed back to it as `Disconnection Complete`
//! events on its next reads — the same events it would have had if each link
//! had dropped by itself — so the connection tasks end, the slots free, and
//! the advertiser starts again.
//!
//! With `desk_ble_fault`, an ATT write carrying [`FORCE_RESTART_MARKER`]
//! fails the read the way an unparseable packet does, so the recovery path
//! can be driven on a desk board. Never in a shipped image.

use core::cell::RefCell;

use bt_hci::event::EventKind;
use bt_hci::transport::{Transport, WithIndicator};
use bt_hci::{ControllerToHostPacket, FromHciBytes, HostToControllerPacket, PacketKind, WriteHci};
use esp_radio::ble::controller::{BleConnector, BleConnectorError};
use fw_esp32_common::radio_link::RADIO_LINK_SLOTS;
use fw_esp32_common::radio_link::hci_connection_ledger::{
    DISCONNECTION_COMPLETE_LEN, HciConnectionLedger, RESET_DISCONNECT_REASON,
    disconnection_complete,
};

/// What a desk build's central writes to force a host-runner restart.
#[cfg(feature = "desk_ble_fault")]
pub const FORCE_RESTART_MARKER: &[u8] = b"LP-DESK-FORCE-BLE-HOST-RESTART";

/// The controller behind the host, and what it has said about connections.
pub struct LpHciTransport {
    inner: BleConnector<'static>,
    state: RefCell<LinkState>,
}

#[derive(Default)]
struct LinkState {
    ledger: HciConnectionLedger<RADIO_LINK_SLOTS>,
    /// Handles to report closed, from the last `Reset`.
    closing: heapless::Vec<u16, RADIO_LINK_SLOTS>,
}

impl LpHciTransport {
    pub fn new(inner: BleConnector<'static>) -> Self {
        Self {
            inner,
            state: RefCell::new(LinkState::default()),
        }
    }
}

impl embedded_io_07::ErrorType for LpHciTransport {
    type Error = BleConnectorError;
}

impl Transport for LpHciTransport {
    async fn read<'a>(&self, rx: &'a mut [u8]) -> Result<ControllerToHostPacket<'a>, Self::Error> {
        // A reset's connections first, one per read, before anything the
        // reset controller sends.
        let closing = self.state.borrow_mut().closing.pop();
        if let Some(handle) = closing {
            log::warn!("[ble] controller reset: telling the host connection {handle} is gone");
            let event = disconnection_complete(handle, RESET_DISCONNECT_REASON);
            rx[..DISCONNECTION_COMPLETE_LEN].copy_from_slice(&event);
            return ControllerToHostPacket::from_hci_bytes_complete(&rx[..DISCONNECTION_COMPLETE_LEN])
                .map_err(|_| BleConnectorError::Unknown);
        }

        let packet = Transport::read(&self.inner, rx).await?;
        match &packet {
            ControllerToHostPacket::Event(event) => {
                if event.kind == EventKind::Le {
                    self.state.borrow_mut().ledger.observe_le_meta(event.data);
                } else if event.kind == EventKind::DisconnectionComplete {
                    self.state
                        .borrow_mut()
                        .ledger
                        .observe_disconnection_complete(event.data);
                }
            }
            #[cfg(feature = "desk_ble_fault")]
            ControllerToHostPacket::Acl(acl) => {
                if acl
                    .data()
                    .windows(FORCE_RESTART_MARKER.len())
                    .any(|w| w == FORCE_RESTART_MARKER)
                {
                    log::warn!("[ble-desk] forcing a host-runner error");
                    return Err(BleConnectorError::Unknown);
                }
            }
            _ => {}
        }
        Ok(packet)
    }

    async fn write<T: HostToControllerPacket>(&self, val: &T) -> Result<(), Self::Error> {
        if T::KIND == PacketKind::Cmd && is_reset(val) {
            let mut state = self.state.borrow_mut();
            let LinkState { ledger, closing } = &mut *state;
            for handle in ledger.take_open() {
                let _ = closing.push(handle);
            }
        }
        Transport::write(&self.inner, val).await
    }
}

/// Is this command an HCI `Reset`? `Reset` has no parameters, so only a
/// command of exactly indicator + header is serialized to look.
fn is_reset<T: HostToControllerPacket>(val: &T) -> bool {
    const PARAMLESS_COMMAND_LEN: usize = 4;
    let packet = WithIndicator::new(val);
    if packet.size() != PARAMLESS_COMMAND_LEN {
        return false;
    }
    // A slice writer refuses a write it cannot take whole, so the buffer is
    // exactly the packet's size.
    let mut bytes = [0u8; PARAMLESS_COMMAND_LEN];
    packet.write_hci(&mut bytes[..]).is_ok()
        && HciConnectionLedger::<RADIO_LINK_SLOTS>::is_reset_command(&bytes)
}
