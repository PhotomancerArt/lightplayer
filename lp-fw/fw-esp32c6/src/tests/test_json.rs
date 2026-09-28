//! JSON streaming validation test
//!
//! When `test_json` feature is enabled, validates ser-write-json on ESP32:
//! - Firmware boots (no ESP32 bootloader segment issues from ser-write-json)
//! - ServerMessage serializes correctly with ser-write-json (thread side,
//!   into the static frame buffer, as the product's transport does)
//! - Output is valid JSON parseable by our deserializer
//!
//! The heartbeat goes out as one proto-channel payload on the product's USB
//! host link (lp-link, `serial::usb_link_task`), so a reader needs an lp-link
//! host, not a serial monitor.
//! Run with: just fwtest-json-esp32c6

extern crate alloc;

use alloc::vec;
use lpc_wire::WireServerMessage;
use lpc_wire::server::{LoadedProject, MemoryStats, SampleStats, ServerMsgBody};
use lpfs::lp_path::AsLpPathBuf;

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::output::LedChannel;
use crate::serial::usb_link_task;
use fw_esp32_common::usb_link::UsbLinkShared;

/// Run JSON streaming validation test
///
/// Sends a Heartbeat ServerMessage on the USB host link every second.
pub async fn run_test_json(spawner: embassy_executor::Spawner) -> ! {
    let (_sw_int, timg0, rmt_peripheral, usb_device, gpio18, _flash, _gpio4, _gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, _sw_int);

    let rmt = esp_hal::rmt::Rmt::new(rmt_peripheral, crate::output::rmt::shared_driver::RMT_CLOCK)
        .expect("RMT init");
    let mut channel = LedChannel::new(rmt, gpio18, 1).expect("LED channel");

    let link = UsbLinkShared::leak(esp_hal::rng::Rng::new().random());
    spawner.spawn(usb_link_task(usb_device, link).unwrap());

    let mut frame_count: u64 = 0;
    let mut last_send = embassy_time::Instant::now();

    loop {
        let now = embassy_time::Instant::now();
        if now.duration_since(last_send).as_millis() >= 1000 {
            let msg = WireServerMessage::new(
                0,
                ServerMsgBody::Heartbeat {
                    fps: SampleStats {
                        avg: 60.0,
                        sdev: 0.5,
                        min: 59.0,
                        max: 61.0,
                    },
                    frame_count,
                    loaded_projects: vec![LoadedProject::new(
                        lpc_wire::WireProjectHandle::new(1),
                        "projects/test".as_path_buf(),
                    )],
                    uptime_ms: frame_count * 1000,
                    memory: Some(MemoryStats {
                        free_bytes: esp_alloc::HEAP.free() as u32,
                        used_bytes: esp_alloc::HEAP.used() as u32,
                        total_bytes: (esp_alloc::HEAP.free() + esp_alloc::HEAP.used()) as u32,
                        largest_free_block: None,
                        oom_retry_saves: None,
                    }),
                    // The harness boots without a recovery region installed,
                    // so there is no crash-recovery state to report — and no
                    // per-wire output attribution either.
                    recovery: None,
                    outputs: None,
                    link: None,
                    // No LpServer in a harness boot, so no identity to
                    // announce.
                    identity: None,
                },
            );

            // Serialized thread side into the static frame buffer, then
            // queued on the link, which reads it from there (no host, or the
            // last one still going out: skipped).
            if !link.frame_buf_in_use() {
                let len =
                    fw_esp32_common::serial::server_payload::serialize_server_payload(&msg, None)
                        .expect("harness heartbeat serializes");
                let _ = link.try_send_frame_buf(len);
            }

            frame_count += 1;
            last_send = now;
        }

        // LED indicator
        let led_state = (frame_count % 2) == 0;
        let mut led_data = [0u8; 3];
        if led_state {
            led_data = [2, 2, 2];
        }
        let tx = channel.start_transmission(&led_data);
        channel = tx.wait_complete();

        embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
    }
}
