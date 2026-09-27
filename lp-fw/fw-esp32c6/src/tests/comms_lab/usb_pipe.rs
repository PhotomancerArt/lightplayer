//! The USB-Serial-JTAG pipe: a byte stream, so frames go out COBS-encoded
//! between `0x00` delimiters ([`LinkConfig::usb`]) and whatever arrives is fed
//! to the link byte for byte.
//!
//! Writes go through the product's IN-endpoint gate
//! ([`fw_esp32_common::serial::in_endpoint`]), one 64-byte packet at a time,
//! so esp-println's raw boot and panic text can share the endpoint without a
//! frame's packet being written over. A write that the host does not drain in
//! [`WRITE_TIMEOUT`] is abandoned: the link resends what matters.

use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_io_async::{Read, Write};
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use fw_esp32_common::serial::in_endpoint::{InEndpoint, InEndpointRegs};
use lp_link::LinkConfig;

use super::lab_edge::{LabEdge, now_us};

/// Longest a frame's write may wait for the host to drain the endpoint.
const WRITE_TIMEOUT: Duration = Duration::from_millis(250);
/// Frames written per pass before the task looks at the RX side again.
const FRAMES_PER_PASS: usize = 8;
/// The longest the task sleeps with nothing to do (the log ring's cadence).
const IDLE_CAP_US: u64 = 10_000;

#[embassy_executor::task]
pub async fn usb_link_task(usb: esp_hal::peripherals::USB_DEVICE<'static>, nonce: u32) {
    let (mut rx, tx) = UsbSerialJtag::new(usb).into_async().split();
    let mut tx = InEndpoint::<_, LabInEndpoint>::new(tx);
    let mut cfg = LinkConfig::usb();
    // The A/B control: plain COBS, 0xFF on the wire (`test_comms_lab_plain_cobs`).
    cfg.escape_ff = !cfg!(feature = "test_comms_lab_plain_cobs");
    let mut edge = LabEdge::new("usb", cfg, nonce);
    let mut buf = [0u8; 64];

    loop {
        drain_rx(&mut rx, &mut edge, &mut buf).await;
        edge.service();
        let mut more = false;
        let mut written = 0;
        while let Some(frame) = edge.next_frame() {
            match with_timeout(WRITE_TIMEOUT, tx.write_all(frame)).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => {
                    edge.write_errors += 1;
                    break;
                }
                Err(_) => {
                    edge.write_timeouts += 1;
                    log::warn!(
                        "usb: a frame write timed out ({} so far) at uptime {} ms; in_ep_free={}",
                        edge.write_timeouts,
                        Instant::now().as_millis(),
                        LabInEndpoint::in_ep_free()
                    );
                    break;
                }
            }
            written += 1;
            if written >= FRAMES_PER_PASS {
                more = true;
                break;
            }
            // Take the host's ACKs as they come: a frame is only overdue if
            // its ACK has not ARRIVED, not if it waits unread in the FIFO
            // while this task writes (that was every spurious resend on the
            // emulator's clean link).
            drain_rx(&mut rx, &mut edge, &mut buf).await;
        }
        let wake = if more { now_us() } else { edge.wake_at(IDLE_CAP_US) };
        match select(
            rx.read(&mut buf),
            Timer::at(Instant::from_micros(wake)),
        )
        .await
        {
            Either::First(Ok(n)) if n > 0 => edge.on_bytes(&buf[..n]),
            _ => {}
        }
    }
}

/// Feed the link whatever the RX FIFO already holds, without waiting.
async fn drain_rx<R: Read>(rx: &mut R, edge: &mut LabEdge, buf: &mut [u8; 64]) {
    loop {
        match select(rx.read(buf), core::future::ready(())).await {
            Either::First(Ok(n)) if n > 0 => edge.on_bytes(&buf[..n]),
            _ => return,
        }
    }
}

/// The two USB-Serial-JTAG register touches the IN-endpoint gate needs
/// (the same as the product io_task's `UsbSerialJtagInEndpoint`).
pub struct LabInEndpoint;

impl InEndpointRegs for LabInEndpoint {
    #[inline]
    fn in_ep_free() -> bool {
        esp_hal::peripherals::USB_DEVICE::regs()
            .ep1_conf()
            .read()
            .serial_in_ep_data_free()
            .bit_is_set()
    }

    #[inline]
    fn clear_serial_in_empty() {
        esp_hal::peripherals::USB_DEVICE::regs()
            .int_clr()
            .write(|w| w.serial_in_empty().clear_bit_by_one());
    }
}
