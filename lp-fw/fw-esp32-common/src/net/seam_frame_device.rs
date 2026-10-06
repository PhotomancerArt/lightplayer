//! The network seam's frame device (`net=lan`; plan P10): the IP stack's
//! Ethernet frames carried by the emulator's virtual LAN.
//!
//! The [`NetFrameDevice::Seam`](crate::net::NetFrameDevice) arm, built at
//! network bring-up only when the seam's engaged byte reads 1; on silicon
//! nothing constructs it. It holds the contract that arm is held to:
//!
//! - frames are **Ethernet II** with the station's MAC (`net_mac`) as their
//!   source, the MTU one whole frame ([`MAX_FRAME_LEN`]);
//! - **one take per receive**: [`Driver::receive`] calls `net_take_frame`
//!   once, and a take of 0 (nothing waiting) is no token; the IP stack asks
//!   again until one is, and the next frame's wake brings it back;
//! - the link is `net_link`, read on every [`Driver::link_state`];
//! - `transmit` grants a token only while the link is up, and the token hands
//!   its frame to `net_give_frame` (a refusal drops it, as a radio would).
//!
//! Every call registers the runner's waker on
//! [`NET_FRAMES`](crate::seams::seam_wake::NET_FRAMES) **before** it asks the
//! seam, so a wake that lands between the two is never lost.
//!
//! **Memory.** One receive and one transmit buffer of [`MAX_FRAME_LEN`]
//! each, in one allocation made the first time the link is up and kept for
//! the boot. A board that never joins a network allocates only the device's
//! few bytes of state, at bring-up.

use alloc::boxed::Box;
use alloc::vec;
use core::task::Context;

use embassy_net_driver::{Capabilities, Driver, HardwareAddress, LinkState, RxToken, TxToken};
use lp_seam::net::{MAC_LEN, MAX_FRAME_LEN};

use crate::seams::net::{net_give_frame, net_link, net_mac, net_take_frame};
use crate::seams::seam_wake::NET_FRAMES;

/// Frames a TCP sender may have in flight back to back.
const MAX_BURST_SIZE: usize = 3;

/// The station's MAC, as the seam answers it. `None` when the call wrote
/// nothing (silicon, or an emulator that did not engage the seam).
pub fn station_mac() -> Option<[u8; MAC_LEN]> {
    let mut mac = [0u8; MAC_LEN];
    (net_mac::call(mac.as_mut_ptr()) != 0).then_some(mac)
}

/// The network seam's frame device. One pointer wide: its state is on the
/// heap, so the IP stack's runner, whose storage is static RAM, holds no
/// more on silicon (where this arm is never built) than the radio's handle.
pub struct SeamFrameDevice(Box<SeamFrameState>);

struct SeamFrameState {
    mac: [u8; MAC_LEN],
    /// The receive buffer, then the transmit buffer: `None` until the link
    /// is first up.
    buffers: Option<Box<[u8]>>,
    /// The link as the last [`Driver::link_state`] read it.
    up: bool,
}

impl SeamFrameDevice {
    /// A device sending as `mac` (from [`station_mac`]).
    pub fn new(mac: [u8; MAC_LEN]) -> Self {
        Self(Box::new(SeamFrameState {
            mac,
            buffers: None,
            up: false,
        }))
    }

    /// Whether the frame buffers exist yet.
    pub fn allocated(&self) -> bool {
        self.0.buffers.is_some()
    }
}

impl Driver for SeamFrameDevice {
    type RxToken<'a> = SeamRx<'a>;
    type TxToken<'a> = SeamTx<'a>;

    fn receive(&mut self, cx: &mut Context) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        NET_FRAMES.register(cx.waker());
        let (rx, tx) = self.0.buffers.as_deref_mut()?.split_at_mut(MAX_FRAME_LEN);
        let len = net_take_frame::call(rx.as_mut_ptr(), rx.len() as u32) as usize;
        // 0: nothing waiting. Longer than the buffer: not an answer the
        // seam may give; nothing was taken that could be read.
        if len == 0 || len > rx.len() {
            return None;
        }
        Some((SeamRx(&mut rx[..len]), SeamTx(tx)))
    }

    fn transmit(&mut self, cx: &mut Context) -> Option<Self::TxToken<'_>> {
        NET_FRAMES.register(cx.waker());
        if !self.0.up {
            return None;
        }
        let buffers = self.0.buffers.as_deref_mut()?;
        Some(SeamTx(&mut buffers[MAX_FRAME_LEN..]))
    }

    fn link_state(&mut self, cx: &mut Context) -> LinkState {
        NET_FRAMES.register(cx.waker());
        let state = &mut *self.0;
        state.up = net_link::call() != 0;
        if state.up {
            state
                .buffers
                .get_or_insert_with(|| vec![0u8; 2 * MAX_FRAME_LEN].into_boxed_slice());
            LinkState::Up
        } else {
            LinkState::Down
        }
    }

    fn capabilities(&self) -> Capabilities {
        let mut caps = Capabilities::default();
        caps.max_transmission_unit = MAX_FRAME_LEN;
        // The radio's (esp-radio's `wifi_max_burst_size` default), so a TCP
        // sender paces the same on either arm.
        caps.max_burst_size = Some(MAX_BURST_SIZE);
        caps
    }

    fn hardware_address(&self) -> HardwareAddress {
        HardwareAddress::Ethernet(self.0.mac)
    }
}

/// A received frame, already taken from the seam into the receive buffer.
pub struct SeamRx<'a>(&'a mut [u8]);

/// The transmit buffer; consuming it hands the frame to the seam.
pub struct SeamTx<'a>(&'a mut [u8]);

impl RxToken for SeamRx<'_> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        f(self.0)
    }
}

impl TxToken for SeamTx<'_> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        // The stack never asks for more than the MTU, which is the buffer.
        let frame = &mut self.0[..len.min(MAX_FRAME_LEN)];
        let out = f(frame);
        // 0: refused (the link went down under the token). The frame is
        // dropped, as a radio drops one; the stack's own retries cover it.
        net_give_frame::call(frame.as_ptr(), frame.len() as u32);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::task::{RawWaker, RawWakerVTable, Waker};

    // Off riscv32 every seam call runs its silicon body: the link reads
    // down and a take returns 0, which is a board whose emulator never
    // answers.

    #[test]
    fn a_board_that_never_joins_allocates_nothing() {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut device = SeamFrameDevice::new(MAC);
        assert!(device.link_state(&mut cx) == LinkState::Down);
        assert!(device.receive(&mut cx).is_none());
        assert!(device.transmit(&mut cx).is_none());
        assert!(!device.allocated());
    }

    #[test]
    fn it_is_an_ethernet_device_of_one_whole_frame() {
        let device = SeamFrameDevice::new(MAC);
        assert_eq!(device.hardware_address(), HardwareAddress::Ethernet(MAC));
        let caps = device.capabilities();
        assert!(caps.max_transmission_unit >= 1514);
        assert_eq!(caps.max_transmission_unit, MAX_FRAME_LEN);
    }

    #[test]
    fn silicon_has_no_station_mac() {
        assert_eq!(station_mac(), None);
    }

    #[test]
    fn a_transmit_token_hands_over_exactly_the_frame_asked_for() {
        let mut buffer = [0u8; MAX_FRAME_LEN];
        let seen = SeamTx(&mut buffer).consume(60, |frame| {
            frame.fill(0xab);
            frame.len()
        });
        assert_eq!(seen, 60);
        assert!(buffer[..60].iter().all(|&b| b == 0xab));
        assert!(buffer[60..].iter().all(|&b| b == 0));
    }

    const MAC: [u8; MAC_LEN] = [0x02, 0x4c, 0x50, 0x00, 0x00, 0x01];

    fn noop_waker() -> Waker {
        const VTABLE: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(core::ptr::null(), &VTABLE),
            |_| {},
            |_| {},
            |_| {},
        );
        // SAFETY: every vtable entry is a no-op on a null pointer.
        unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
    }
}
