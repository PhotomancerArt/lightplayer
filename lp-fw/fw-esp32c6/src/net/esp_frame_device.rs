//! The radio's station interface as the IP stack's frame device.

use core::sync::atomic::{AtomicU32, Ordering};
use core::task::Context;

use embassy_net::driver::{Capabilities, Driver, HardwareAddress, LinkState, RxToken, TxToken};
use esp_radio::wifi::Interface;
use fw_esp32_common::net::NetFrameDevice;

/// The frame device embassy-net runs over on this image: esp-radio's
/// station interface in the radio arm of the seam boundary (PR B has no
/// seam arm).
pub type C6FrameDevice = NetFrameDevice<CountedStation>;

/// Frames handed up by the radio, since boot.
static FRAMES_IN: AtomicU32 = AtomicU32::new(0);
/// Frames handed to the radio, since boot.
static FRAMES_OUT: AtomicU32 = AtomicU32::new(0);

/// Frames in and out since boot, for the heartbeat's `[wifi]` line.
pub fn frame_counts() -> (u32, u32) {
    (
        FRAMES_IN.load(Ordering::Relaxed),
        FRAMES_OUT.load(Ordering::Relaxed),
    )
}

/// esp-radio's station interface, counting the frames that cross it. The
/// count is one relaxed add per frame, in the token's `consume`, so a token
/// the stack never consumes is not counted. esp-radio keeps no count of the
/// frames it drops when its RX queue is full (its debug line `RX QUEUE FULL`
/// marks each), so the heartbeat can only report frames that got through.
pub struct CountedStation(pub Interface<'static>);

/// A counted receive token.
pub struct CountedRx<T>(T);
/// A counted transmit token.
pub struct CountedTx<T>(T);

impl Driver for CountedStation {
    type RxToken<'a>
        = CountedRx<<Interface<'static> as Driver>::RxToken<'a>>
    where
        Self: 'a;
    type TxToken<'a>
        = CountedTx<<Interface<'static> as Driver>::TxToken<'a>>
    where
        Self: 'a;

    fn receive(&mut self, cx: &mut Context) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        Driver::receive(&mut self.0, cx).map(|(rx, tx)| (CountedRx(rx), CountedTx(tx)))
    }

    fn transmit(&mut self, cx: &mut Context) -> Option<Self::TxToken<'_>> {
        Driver::transmit(&mut self.0, cx).map(CountedTx)
    }

    fn link_state(&mut self, cx: &mut Context) -> LinkState {
        self.0.link_state(cx)
    }

    fn capabilities(&self) -> Capabilities {
        self.0.capabilities()
    }

    fn hardware_address(&self) -> HardwareAddress {
        self.0.hardware_address()
    }
}

impl<T: RxToken> RxToken for CountedRx<T> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        FRAMES_IN.fetch_add(1, Ordering::Relaxed);
        self.0.consume(f)
    }
}

impl<T: TxToken> TxToken for CountedTx<T> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        FRAMES_OUT.fetch_add(1, Ordering::Relaxed);
        self.0.consume(len, f)
    }
}
