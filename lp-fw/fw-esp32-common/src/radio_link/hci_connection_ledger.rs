//! Which BLE connections the controller has open, read off the HCI events
//! that pass — so a controller reset can close them in the host too.
//!
//! The host stack (trouble-host 0.6) recovers from a runner error by running
//! its whole bring-up again, and that bring-up starts with an HCI `Reset`. A
//! reset drops every connection in the controller **without** a
//! `Disconnection Complete` event, so the host never learns they are gone:
//! each connection's task waits forever on a link that no longer exists, its
//! slot is never freed, and the board stops advertising for good
//! (docs/defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md).
//!
//! The ledger closes that gap from the HCI transport, where every packet
//! already passes: it records each connection the controller reports opened
//! and each one it reports closed, and when the host writes a `Reset` it hands
//! back the handles still open, for the transport to deliver to the host as
//! `Disconnection Complete` events — exactly what the host would have heard
//! had the links dropped one by one. Pure bytes in, bytes out: host-tested.

/// H4 packet indicator: HCI event.
const H4_EVENT: u8 = 0x04;
/// H4 packet indicator: HCI command.
const H4_COMMAND: u8 = 0x01;
/// Event code: Disconnection Complete.
const EVT_DISCONNECTION_COMPLETE: u8 = 0x05;
/// Event code: LE Meta (the tests' synthesised controller events).
#[cfg(test)]
const EVT_LE_META: u8 = 0x3E;
/// LE subevents that open a connection and share the leading layout
/// `status, handle(2), …`: Connection Complete, Enhanced Connection Complete
/// (v1 and v2).
const LE_CONNECTION_COMPLETE_SUBEVENTS: [u8; 3] = [0x01, 0x0A, 0x29];
/// Command opcode `Reset` (OGF 0x03, OCF 0x003), little-endian on the wire.
const OPCODE_RESET: [u8; 2] = [0x03, 0x0C];

/// Reason code on the events synthesised for a reset: 0x16, "connection
/// terminated by local host" — the host did ask for the reset.
pub const RESET_DISCONNECT_REASON: u8 = 0x16;

/// Length of a synthesised Disconnection Complete, H4 indicator included.
pub const DISCONNECTION_COMPLETE_LEN: usize = 7;

/// The connection handles the controller has reported open, up to `N`.
pub struct HciConnectionLedger<const N: usize> {
    open: [Option<u16>; N],
}

impl<const N: usize> Default for HciConnectionLedger<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> HciConnectionLedger<N> {
    pub const fn new() -> Self {
        Self { open: [None; N] }
    }

    /// Record an LE Meta event's parameters (subevent first): a connection
    /// the controller opened. Other subevents, failed connections and
    /// anything malformed are ignored.
    ///
    /// Taken per event kind rather than by event code on purpose: bt-hci 0.8
    /// reports LE Meta (0x3E on the wire) as `EventKind(0x3F)`, so a raw code
    /// read off its parsed packet is not the wire's. (The first silicon run
    /// of this ledger keyed on 0x3E and never saw a connection.)
    pub fn observe_le_meta(&mut self, params: &[u8]) {
        if let [sub, 0x00, lo, hi, ..] = params
            && LE_CONNECTION_COMPLETE_SUBEVENTS.contains(sub)
        {
            self.opened(handle(*lo, *hi));
        }
    }

    /// Record a Disconnection Complete event's parameters: a connection the
    /// controller closed.
    pub fn observe_disconnection_complete(&mut self, params: &[u8]) {
        if let [0x00, lo, hi, ..] = params {
            self.closed(handle(*lo, *hi));
        }
    }

    /// Is this host → controller packet (H4, indicator first) an HCI `Reset`?
    #[must_use]
    pub fn is_reset_command(packet: &[u8]) -> bool {
        matches!(packet, [H4_COMMAND, lo, hi, ..] if [*lo, *hi] == OPCODE_RESET)
    }

    /// The controller is being reset: forget every open connection, returning
    /// their handles for the host to be told.
    pub fn take_open(&mut self) -> impl Iterator<Item = u16> {
        let open = core::mem::replace(&mut self.open, [None; N]);
        open.into_iter().flatten()
    }

    /// How many connections the ledger holds open.
    #[must_use]
    pub fn open_count(&self) -> usize {
        self.open.iter().flatten().count()
    }

    fn opened(&mut self, handle: u16) {
        if self.open.contains(&Some(handle)) {
            return;
        }
        if let Some(slot) = self.open.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(handle);
        }
    }

    fn closed(&mut self, handle: u16) {
        for slot in &mut self.open {
            if *slot == Some(handle) {
                *slot = None;
            }
        }
    }
}

/// The H4 bytes of a successful Disconnection Complete for `handle`.
#[must_use]
pub fn disconnection_complete(handle: u16, reason: u8) -> [u8; DISCONNECTION_COMPLETE_LEN] {
    let [lo, hi] = handle.to_le_bytes();
    [
        H4_EVENT,
        EVT_DISCONNECTION_COMPLETE,
        4,
        0x00,
        lo,
        hi,
        reason,
    ]
}

/// A connection handle is 12 bits; the rest of the field is flags.
fn handle(lo: u8, hi: u8) -> u16 {
    u16::from_le_bytes([lo, hi]) & 0x0FFF
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn a_reset_closes_every_connection_the_controller_reported_open() {
        let mut ledger = HciConnectionLedger::<2>::new();
        observe(&mut ledger, &le_connection_complete(0x01, 0x0000));
        observe(&mut ledger, &le_connection_complete(0x0A, 0x0001));
        assert_eq!(ledger.open_count(), 2);

        assert!(HciConnectionLedger::<2>::is_reset_command(&[
            0x01, 0x03, 0x0C, 0x00
        ]));
        let open: Vec<u16> = ledger.take_open().collect();
        assert_eq!(open, [0x0000, 0x0001]);
        assert_eq!(ledger.open_count(), 0);
        // …and a second reset has nothing left to close.
        assert_eq!(ledger.take_open().count(), 0);
    }

    #[test]
    fn a_connection_the_controller_closed_is_not_closed_again() {
        let mut ledger = HciConnectionLedger::<2>::new();
        observe(&mut ledger, &le_connection_complete(0x01, 0x0000));
        observe(&mut ledger, &le_connection_complete(0x01, 0x0001));
        observe(&mut ledger, &disconnection_complete(0x0000, 0x13));
        assert_eq!(ledger.take_open().collect::<Vec<_>>(), [0x0001]);
    }

    #[test]
    fn a_failed_connection_attempt_is_not_recorded() {
        let mut ledger = HciConnectionLedger::<2>::new();
        let mut failed = le_connection_complete(0x01, 0x0000);
        failed[4] = 0x3E; // status: connection failed to be established
        observe(&mut ledger, &failed);
        assert_eq!(ledger.open_count(), 0);
    }

    #[test]
    fn other_events_and_truncated_ones_are_ignored() {
        let mut ledger = HciConnectionLedger::<2>::new();
        // An LE subevent that is not a connection (Data Length Change).
        ledger.observe_le_meta(&[0x07, 0x00, 0x00, 0xFB, 0x00]);
        // An LE connection complete cut before its handle.
        ledger.observe_le_meta(&[0x01, 0x00, 0x00]);
        observe(&mut ledger, &le_connection_complete(0x01, 0x0003));
        // A failed disconnection (status 0x0C, command disallowed).
        ledger.observe_disconnection_complete(&[0x0C, 0x03, 0x00, 0x16]);
        assert_eq!(ledger.open_count(), 1);
        ledger.take_open().for_each(drop);
        assert_eq!(ledger.open_count(), 0);
    }

    #[test]
    fn handle_flags_are_masked_and_repeats_counted_once() {
        let mut ledger = HciConnectionLedger::<2>::new();
        let mut event = le_connection_complete(0x01, 0x0002);
        event[6] |= 0x20; // flag bits above the 12-bit handle
        observe(&mut ledger, &event);
        observe(&mut ledger, &le_connection_complete(0x01, 0x0002));
        assert_eq!(ledger.take_open().collect::<Vec<_>>(), [0x0002]);
    }

    #[test]
    fn only_a_reset_command_is_a_reset() {
        // LE Set Advertising Enable (0x200A) and an event that ends in 03 0C.
        assert!(!HciConnectionLedger::<2>::is_reset_command(&[
            0x01, 0x0A, 0x20, 0x01, 0x01
        ]));
        assert!(!HciConnectionLedger::<2>::is_reset_command(&[
            0x04, 0x03, 0x0C
        ]));
        assert!(!HciConnectionLedger::<2>::is_reset_command(&[0x01]));
    }

    #[test]
    fn the_synthesised_event_is_a_well_formed_disconnection_complete() {
        assert_eq!(
            disconnection_complete(0x0001, RESET_DISCONNECT_REASON),
            [0x04, 0x05, 0x04, 0x00, 0x01, 0x00, 0x16]
        );
    }

    /// Feed the ledger an H4 event packet as the controller would send it.
    fn observe(ledger: &mut HciConnectionLedger<2>, packet: &[u8]) {
        assert_eq!(packet[0], 0x04);
        assert_eq!(usize::from(packet[2]), packet.len() - 3);
        match packet[1] {
            EVT_LE_META => ledger.observe_le_meta(&packet[3..]),
            EVT_DISCONNECTION_COMPLETE => ledger.observe_disconnection_complete(&packet[3..]),
            other => panic!("not an event the ledger reads: {other}"),
        }
    }

    /// An LE (Enhanced) Connection Complete from the controller, status 0.
    fn le_connection_complete(subevent: u8, handle: u16) -> Vec<u8> {
        let [lo, hi] = handle.to_le_bytes();
        let mut params = alloc::vec![subevent, 0x00, lo, hi, 0x01, 0x01];
        params.extend_from_slice(&[0xAA; 6]); // peer address
        params.extend_from_slice(&[0x18, 0x00, 0x00, 0x00, 0x48, 0x00, 0x00]); // interval, latency, timeout, clock accuracy
        let mut packet = alloc::vec![0x04, 0x3E, params.len() as u8];
        packet.extend_from_slice(&params);
        packet
    }
}
