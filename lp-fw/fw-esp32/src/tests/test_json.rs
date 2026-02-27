//! JSON streaming validation test
//!
//! When `test_json` feature is enabled, validates ser-write-json on ESP32:
//! - Firmware boots (no ESP32 bootloader segment issues from ser-write-json)
//! - ServerMessage serializes correctly with ser-write-json (in io_task)
//! - Output is valid JSON parseable by our deserializer
//!
//! Uses shared serial::io_task which drains OUTGOING_SERVER_MSG and streams to serial.
//! Run with: just fwtest-json-esp32c6
//! Flash and connect with screen/minicom to see M! prefixed JSON every second.

extern crate alloc;

use alloc::vec;
use lp_model::ServerMessage;
use lp_model::path::AsLpPathBuf;
use lp_model::server::{LoadedProject, MemoryStats, SampleStats, ServerMsgBody};

use crate::board::{init_board, start_runtime};
use crate::output::LedChannel;
use crate::serial::io_task;

/// Run JSON streaming validation test
///
/// Sends Heartbeat ServerMessage to OUTGOING_SERVER_MSG every second.
/// serial::io_task receives and serializes with ser-write-json directly to USB serial.
pub async fn run_test_json(spawner: embassy_executor::Spawner) -> ! {
    let (_sw_int, timg0, rmt_peripheral, usb_device, gpio18) = init_board();
    start_runtime(timg0, _sw_int);

    let rmt = esp_hal::rmt::Rmt::new(rmt_peripheral, esp_hal::time::Rate::from_mhz(80))
        .expect("RMT init");
    let mut channel = LedChannel::new(rmt, gpio18, 1).expect("LED channel");

    spawner.spawn(io_task::io_task(usb_device)).ok();

    let mut frame_count: u64 = 0;
    let mut last_send = embassy_time::Instant::now();

    loop {
        let now = embassy_time::Instant::now();
        if now.duration_since(last_send).as_millis() >= 1000 {
            let msg = ServerMessage {
                id: 0,
                msg: ServerMsgBody::Heartbeat {
                    fps: SampleStats {
                        avg: 60.0,
                        sdev: 0.5,
                        min: 59.0,
                        max: 61.0,
                    },
                    frame_count,
                    loaded_projects: vec![LoadedProject {
                        handle: lp_model::project::ProjectHandle::new(1),
                        path: "projects/test".as_path_buf(),
                    }],
                    uptime_ms: frame_count * 1000,
                    memory: Some(MemoryStats {
                        free_bytes: esp_alloc::HEAP.free() as u32,
                        used_bytes: esp_alloc::HEAP.used() as u32,
                        total_bytes: (esp_alloc::HEAP.free() + esp_alloc::HEAP.used()) as u32,
                    }),
                },
            };

            // Send to OUTGOING_SERVER_MSG - io_task will serialize with ser-write-json
            let server_channel = io_task::get_server_msg_channel();
            let _ = server_channel.sender().try_send(msg);

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
