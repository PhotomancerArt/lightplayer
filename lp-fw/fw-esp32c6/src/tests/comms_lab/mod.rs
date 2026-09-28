//! The comms lab: `lp-link` on the C6's own pipes, outside the product.
//!
//! Plan `reliable-device-link`, M3 (Yona, 2026-09-26: "develop a ble/usb/maybe
//! even wifi comm system … packet based, that covers the low-level logging and
//! printf … outside the core lp firmware, just to get it all working, then we
//! patch it back in as the comms layer").
//!
//! One embassy task per pipe owns one [`Link`](lp_link::Link) and runs the
//! lab's board half ([`lp_link::lab::LabBoard`]): it echoes the host's soak
//! messages, streams its own on request, answers `stats`, and can stall its
//! executor, write a burst of log lines or panic on command. Logging and
//! printf ([`lab_logger`]) ride the link's log channel; boot text and panics
//! go raw to USB, outside frames.
//!
//! - **USB-Serial-JTAG** ([`usb_pipe`]): always. Stream framing.
//! - **BLE NUS** (`ble_pipe`, `test_comms_lab_ble`): one frame per
//!   notification / write. Datagram framing.
//!
//! The host half is `lp-cli link lab` (native serial, or a TCP socket to an
//! emulated board), the emulator soak (`lp-cli/tests/emu_link_lab.rs`), and
//! the browser pages under `spikes/`.
//!
//! Build (from `lp-fw/fw-esp32c6/`):
//!
//! ```text
//! cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 \
//!     --no-default-features --features esp32c6,test_comms_lab            # USB only
//!     --no-default-features --features esp32c6,test_comms_lab_ble        # USB + BLE
//! ```

#[cfg(feature = "test_comms_lab_ble")]
mod ble_pipe;
mod lab_edge;
pub mod lab_logger;
mod usb_pipe;
#[cfg(feature = "test_comms_lab_wifi")]
mod wifi_pipe;

use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;

/// Every this often the board writes one `printf` line onto the log channel
/// (heap, uptime), so a quiet link still shows the log path working.
const TICK: Duration = Duration::from_secs(5);

pub async fn run_comms_lab(spawner: embassy_executor::Spawner) -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    // The product's heap, region for region (as `test_ble` does), so a BLE
    // build's controller finds the memory it finds in the product.
    esp_alloc::heap_allocator!(size: 236_000);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65_536);

    // Raw text before any link exists: the host sees it as `Text`.
    esp_println::println!(
        "[LAB] comms lab booting: fw={} heap_free={}",
        env!("LP_BUILD_COMMIT"),
        esp_alloc::HEAP.free()
    );
    lab_logger::init();

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let rng = Rng::new();
    let usb_nonce = rng.random();
    esp_println::println!("[LAB] usb link nonce={:08x}; frames follow", usb_nonce);
    spawner.spawn(usb_pipe::usb_link_task(peripherals.USB_DEVICE, usb_nonce).unwrap());
    spawner.spawn(tick_task().unwrap());

    #[cfg(feature = "test_comms_lab_ble")]
    ble_pipe::run(peripherals.BT, peripherals.GPIO3, peripherals.GPIO14, rng).await;
    #[cfg(feature = "test_comms_lab_wifi")]
    wifi_pipe::run(spawner, peripherals.WIFI, rng).await;

    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}

#[embassy_executor::task]
async fn tick_task() {
    let mut n = 0u32;
    loop {
        Timer::after(TICK).await;
        n += 1;
        crate::lab_printf!(
            "tick {} uptime_ms={} heap_free={} heap_used={}",
            n,
            embassy_time::Instant::now().as_millis(),
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used()
        );
    }
}
