//! The frame device under the IP stack: the radio's, or (PR C) the
//! network seam's, behind one type.

use core::task::Context;

use embassy_net_driver::{Capabilities, Driver, HardwareAddress, LinkState, RxToken, TxToken};

/// The one driver type embassy-net is built over, whatever answers it
/// (plan MD4, the seams notes' "switch shape"): [`Self::Radio`] is the
/// chip's own station device (esp-radio's `WifiDevice` on the C6), and
/// [`Self::Seam`] the network seam's emulator-backed device, chosen at
/// start-up when the seam's engaged byte says so. Each call is one match
/// and a delegation: no copy per frame, no allocation.
///
/// PR B builds it with only the radio behind it: `S` defaults to
/// [`Unplugged`], which has no value, so the seam arm is compiled out.
///
/// What either arm must do to be equivalent (the contract the seam's
/// emulator answer is held to):
///
/// - frames are **Ethernet II**, the station's own MAC as source
///   ([`HardwareAddress::Ethernet`]);
/// - the link state is **up exactly while the station is associated**, and
///   wakes the context when it changes;
/// - `receive` hands one whole received frame per token and wakes the
///   context when the next arrives; `transmit` refuses (and wakes later)
///   rather than fail after granting a token;
/// - `capabilities` gives an MTU of at least 1514 bytes of frame.
pub enum NetFrameDevice<R, S = Unplugged> {
    /// The radio's station interface.
    Radio(R),
    /// The network seam's device (Wi-Fi roadmap M6, PR C).
    Seam(S),
}

/// A receive token from either arm.
pub enum EitherRx<A, B> {
    Radio(A),
    Seam(B),
}

/// A transmit token from either arm.
pub enum EitherTx<A, B> {
    Radio(A),
    Seam(B),
}

impl<R: Driver, S: Driver> Driver for NetFrameDevice<R, S> {
    type RxToken<'a>
        = EitherRx<R::RxToken<'a>, S::RxToken<'a>>
    where
        Self: 'a;
    type TxToken<'a>
        = EitherTx<R::TxToken<'a>, S::TxToken<'a>>
    where
        Self: 'a;

    fn receive(&mut self, cx: &mut Context) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        match self {
            Self::Radio(device) => device
                .receive(cx)
                .map(|(rx, tx)| (EitherRx::Radio(rx), EitherTx::Radio(tx))),
            Self::Seam(device) => device
                .receive(cx)
                .map(|(rx, tx)| (EitherRx::Seam(rx), EitherTx::Seam(tx))),
        }
    }

    fn transmit(&mut self, cx: &mut Context) -> Option<Self::TxToken<'_>> {
        match self {
            Self::Radio(device) => device.transmit(cx).map(EitherTx::Radio),
            Self::Seam(device) => device.transmit(cx).map(EitherTx::Seam),
        }
    }

    fn link_state(&mut self, cx: &mut Context) -> LinkState {
        match self {
            Self::Radio(device) => device.link_state(cx),
            Self::Seam(device) => device.link_state(cx),
        }
    }

    fn capabilities(&self) -> Capabilities {
        match self {
            Self::Radio(device) => device.capabilities(),
            Self::Seam(device) => device.capabilities(),
        }
    }

    fn hardware_address(&self) -> HardwareAddress {
        match self {
            Self::Radio(device) => device.hardware_address(),
            Self::Seam(device) => device.hardware_address(),
        }
    }
}

impl<A: RxToken, B: RxToken> RxToken for EitherRx<A, B> {
    fn consume<T, F>(self, f: F) -> T
    where
        F: FnOnce(&mut [u8]) -> T,
    {
        match self {
            Self::Radio(token) => token.consume(f),
            Self::Seam(token) => token.consume(f),
        }
    }
}

impl<A: TxToken, B: TxToken> TxToken for EitherTx<A, B> {
    fn consume<T, F>(self, len: usize, f: F) -> T
    where
        F: FnOnce(&mut [u8]) -> T,
    {
        match self {
            Self::Radio(token) => token.consume(len, f),
            Self::Seam(token) => token.consume(len, f),
        }
    }
}

/// The seam arm of an image with no network seam: a type with no values,
/// so [`NetFrameDevice::Seam`] can never be built and its arm costs
/// nothing.
pub enum Unplugged {}

impl Driver for Unplugged {
    type RxToken<'a> = Unplugged;
    type TxToken<'a> = Unplugged;

    fn receive(&mut self, _: &mut Context) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        match *self {}
    }

    fn transmit(&mut self, _: &mut Context) -> Option<Self::TxToken<'_>> {
        match *self {}
    }

    fn link_state(&mut self, _: &mut Context) -> LinkState {
        match *self {}
    }

    fn capabilities(&self) -> Capabilities {
        match *self {}
    }

    fn hardware_address(&self) -> HardwareAddress {
        match *self {}
    }
}

impl RxToken for Unplugged {
    fn consume<T, F>(self, _: F) -> T
    where
        F: FnOnce(&mut [u8]) -> T,
    {
        match self {}
    }
}

impl TxToken for Unplugged {
    fn consume<T, F>(self, _: usize, _: F) -> T
    where
        F: FnOnce(&mut [u8]) -> T,
    {
        match self {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::task::{RawWaker, RawWakerVTable, Waker};

    /// A loopback driver: whatever is transmitted is received next.
    struct Loop {
        queue: Vec<Vec<u8>>,
    }

    struct LoopRx(Vec<u8>);
    struct LoopTx<'a>(&'a mut Vec<Vec<u8>>);

    impl Driver for Loop {
        type RxToken<'a> = LoopRx;
        type TxToken<'a> = LoopTx<'a>;

        fn receive(&mut self, _: &mut Context) -> Option<(LoopRx, LoopTx<'_>)> {
            if self.queue.is_empty() {
                return None;
            }
            let frame = self.queue.remove(0);
            Some((LoopRx(frame), LoopTx(&mut self.queue)))
        }

        fn transmit(&mut self, _: &mut Context) -> Option<LoopTx<'_>> {
            Some(LoopTx(&mut self.queue))
        }

        fn link_state(&mut self, _: &mut Context) -> LinkState {
            LinkState::Up
        }

        fn capabilities(&self) -> Capabilities {
            let mut caps = Capabilities::default();
            caps.max_transmission_unit = 1514;
            caps
        }

        fn hardware_address(&self) -> HardwareAddress {
            HardwareAddress::Ethernet([0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30])
        }
    }

    impl RxToken for LoopRx {
        fn consume<T, F: FnOnce(&mut [u8]) -> T>(mut self, f: F) -> T {
            f(&mut self.0)
        }
    }

    impl TxToken for LoopTx<'_> {
        fn consume<T, F: FnOnce(&mut [u8]) -> T>(self, len: usize, f: F) -> T {
            let mut frame = vec![0; len];
            let out = f(&mut frame);
            self.0.push(frame);
            out
        }
    }

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

    #[test]
    fn the_radio_arm_passes_every_call_through() {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut device: NetFrameDevice<Loop> = NetFrameDevice::Radio(Loop { queue: Vec::new() });
        assert!(device.receive(&mut cx).is_none());
        device
            .transmit(&mut cx)
            .unwrap()
            .consume(3, |frame| frame.copy_from_slice(&[1, 2, 3]));
        let (rx, _tx) = device.receive(&mut cx).unwrap();
        assert_eq!(rx.consume(|frame| frame.to_vec()), [1, 2, 3]);
        assert!(device.link_state(&mut cx) == LinkState::Up);
        assert_eq!(device.capabilities().max_transmission_unit, 1514);
        assert_eq!(
            device.hardware_address(),
            HardwareAddress::Ethernet([0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30])
        );
    }

    #[test]
    fn an_unplugged_seam_costs_nothing() {
        assert_eq!(
            core::mem::size_of::<NetFrameDevice<Loop>>(),
            core::mem::size_of::<Loop>()
        );
    }
}
