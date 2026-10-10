use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rtc_cntl::{Rtc, Rwdt};
use esp_hal::timer::timg::{TimerGroup, TimerGroupInstance};

/// Initialize ESP32-C6 hardware
///
/// Sets up CPU clock, timers, and other board-specific hardware.
/// Returns runtime components needed for Embassy and hardware peripherals.
/// FLASH peripheral is included for persistent storage (default; disabled with memory_fs feature).
/// The RTC watchdog is returned unarmed; the recovery subsystem arms it.
// The BLE spike takes `BT`, which this does not hand out, so it inits alone.
#[cfg_attr(
    any(feature = "test_ble", feature = "test_comms_lab"),
    allow(
        dead_code,
        reason = "the BLE spike and the comms lab init their own peripherals"
    )
)]
pub fn init_board() -> (
    SoftwareInterruptControl<'static>,
    TimerGroup<'static, impl TimerGroupInstance>,
    esp_hal::peripherals::RMT<'static>,
    esp_hal::peripherals::USB_DEVICE<'static>,
    esp_hal::peripherals::GPIO18<'static>,
    esp_hal::peripherals::FLASH<'static>,
    esp_hal::peripherals::GPIO4<'static>,
    esp_hal::peripherals::GPIO20<'static>,
    esp_hal::peripherals::WIFI<'static>,
    Rwdt,
) {
    // Configure CPU clock to maximum speed (160MHz for ESP32-C6)
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The RAM split. Main RAM (`RAM` in esp-hal's memory.x, 0x6E610 B) holds
    // .data/.bss — this heap array included — and the main task's stack is
    // whatever is left above them: with a 300_000 B heap that was 32,776 B,
    // and the meteor example's steady-state tick (resolver recursion four
    // demand levels deep under the compute node) overflowed it by a few
    // hundred bytes into the heap array (2026-09-01 bench, `Stack overflow
    // detected … Stack pointer: 408664e0`, `_stack_end` = 40866610). At
    // 260_000 B the stack was 72,776 B. `stack_probe` paints it at boot and
    // the heartbeat logs the high-water mark, so the margin is a number in
    // the journal rather than a guess.
    //
    // 260_000 → 236_000 (2026-09-24): linking `ble` (in `default`, on every
    // board whether the device store enables it or not) put ~36 KB of static
    // RAM — the controller blob's IRAM link-layer code, its statics, the
    // packet pool — below the stack, which fell to 38,680 B against meteor's
    // ~35.5 KB high-water. The 24,000 B come out of the heap instead, one
    // image for every device (Yona's ruling: "a stack overflow crashes; a
    // smaller heap only narrows the compile margin"). See
    // docs/adr/2026-09-02-esp32c6-ram-split.md, "Amendment".
    // SAFETY: each array is handed to the allocator exactly once, here, and
    // nothing else ever names it except to read its address.
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            core::ptr::addr_of_mut!(HEAP_MAIN).cast::<u8>(),
            HEAP_MAIN_SIZE,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
    // The 40 KB the main region gave up comes back with interest from
    // `dram2_seg`: the 64 KB (0x4086E610..0x4087E610) the ESP-IDF
    // second-stage bootloader loads its code and data into, and runs its
    // stack in — esp-hal's `#[ram(reclaimed)]` exists for exactly this. A
    // second `esp_alloc` region: `HEAP.free()`/`used()` sum all three,
    // allocations fill the main region first. Heap total 301,536 B
    // (325,536 B before the 2026-09-24 cut).
    //
    // That segment is the bootloader's again the moment the HP system
    // resets, while the radio does not reset with it: a warm reset (the
    // host's RTS or a requested reboot) leaves the Bluetooth controller
    // running, and its DMA wrote two words into the bootloader's code
    // (docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md).
    // So nothing the radio can DMA into may live here: the segment carries
    // NO capability tag, so only capability-free requests (Rust's global
    // allocator) can land in it, and the radio blobs' C heap has its own
    // region in main RAM, `HEAP_RADIO`, below.
    //
    // All three regions are `esp_alloc::heap_allocator!` spelled out, so the
    // arrays have names: [`heap_regions`] reports where each one is.
    // SAFETY: as above.
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            core::ptr::addr_of_mut!(HEAP_DRAM2).cast::<u8>(),
            HEAP_DRAM2_SIZE,
            crate::c_heap::BOOTLOADER_RECLAIMED,
        ));
    }
    // The radio's own region, carved out of main RAM (`.bss`, far below the
    // lowest address any second-stage bootloader we ship loads into,
    // 0x4086B910). Registered last, so Rust reaches it only once the main
    // region and `dram2_seg` are both full; the C heap asks for it first by
    // its tag (see `c_heap`).
    // SAFETY: as above.
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            core::ptr::addr_of_mut!(HEAP_RADIO).cast::<u8>(),
            HEAP_RADIO_SIZE,
            crate::c_heap::RADIO,
        ));
    }

    // Extract peripherals we need before moving others
    let rmt = peripherals.RMT;
    let usb_device = peripherals.USB_DEVICE;
    let gpio18 = peripherals.GPIO18;
    let flash = peripherals.FLASH;
    let gpio4 = peripherals.GPIO4;
    let gpio20 = peripherals.GPIO20;
    let wifi = peripherals.WIFI;
    // The BLE controller's peripheral, beside WIFI. Parked rather than added
    // to the tuple every harness destructures; the product boot takes it
    // with [`take_bt`] only when the device store enables BLE.
    #[cfg(all(feature = "ble", feature = "server", not(fw_harness)))]
    critical_section::with(|cs| BT.borrow_ref_mut(cs).replace(peripherals.BT));

    // Set up software interrupt and timer for Embassy runtime
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);

    // RTC watchdog for the crash-recovery backstop (armed later by recovery).
    let rtc = Rtc::new(peripherals.LPWR);
    let rwdt = rtc.rwdt;

    (
        sw_int, timg0, rmt, usb_device, gpio18, flash, gpio4, gpio20, wifi, rwdt,
    )
}

/// The main heap region's size (see the RAM-split note in [`init_board`]):
/// the 236,000 B main RAM gives the heap, less the radio's region.
const HEAP_MAIN_SIZE: usize = 236_000 - HEAP_RADIO_SIZE - E06_HEAP_MAIN_SHRINK;
/// RAM research E6 (throwaway): bytes taken off the main region at build time
/// (`LP_E06_HEAP_MAIN_SHRINK=<bytes>`), so an image with a larger allocator
/// control block in `.bss` (esp-alloc's TLSF) keeps the LLFF image's stack.
/// Unset, it is 0 and the image is today's.
const E06_HEAP_MAIN_SHRINK: usize = parse_usize(option_env!("LP_E06_HEAP_MAIN_SHRINK"));

const fn parse_usize(text: Option<&str>) -> usize {
    let Some(text) = text else { return 0 };
    let bytes = text.as_bytes();
    let mut value = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        let digit = bytes[i];
        assert!(
            digit.is_ascii_digit(),
            "LP_E06_HEAP_MAIN_SHRINK must be decimal"
        );
        value = value * 10 + (digit - b'0') as usize;
        i += 1;
    }
    value
}
/// The radio blobs' C heap (`c_heap`), in main RAM where no bootloader
/// loads. Sized from silicon: 44,584 B of radio allocations with Bluetooth
/// up (2026-09-24), less the 10,320 B the lean ESP-NOW buffers gave back
/// (2026-10-01), is ~34.3 KB at no connections; each of the two links adds
/// up to ~3.5 KB. 48 KiB holds that with ~7 KB to spare, and the
/// `[radio-heap]` heartbeat line reports the real high-water. A request
/// that does not fit still succeeds, in the main region.
pub const HEAP_RADIO_SIZE: usize = 49_152;
/// The reclaimed bootloader segment's size.
const HEAP_DRAM2_SIZE: usize = 65_536;
/// A heap region's backing array, 8-aligned so the allocator loses nothing
/// to aligning its start (a bare byte array can land on an odd address).
#[repr(C, align(8))]
struct HeapArena<const N: usize>(core::mem::MaybeUninit<[u8; N]>);
static mut HEAP_MAIN: HeapArena<HEAP_MAIN_SIZE> = HeapArena(core::mem::MaybeUninit::uninit());
static mut HEAP_RADIO: HeapArena<HEAP_RADIO_SIZE> = HeapArena(core::mem::MaybeUninit::uninit());
#[esp_hal::ram(reclaimed)]
static mut HEAP_DRAM2: core::mem::MaybeUninit<[u8; HEAP_DRAM2_SIZE]> =
    core::mem::MaybeUninit::uninit();

/// The heap's three regions as `(start address, size)` — main, `dram2_seg`,
/// radio: the order Rust's allocator tries them in.
#[allow(dead_code, reason = "read only by the heap diagnostics")]
pub fn heap_regions() -> [(usize, usize); 3] {
    [
        (core::ptr::addr_of!(HEAP_MAIN) as usize, HEAP_MAIN_SIZE),
        (core::ptr::addr_of!(HEAP_DRAM2) as usize, HEAP_DRAM2_SIZE),
        radio_region(),
    ]
}

/// The radio's region as `(start address, size)`.
pub fn radio_region() -> (usize, usize) {
    (core::ptr::addr_of!(HEAP_RADIO) as usize, HEAP_RADIO_SIZE)
}

#[cfg(all(feature = "ble", feature = "server", not(fw_harness)))]
static BT: critical_section::Mutex<core::cell::RefCell<Option<esp_hal::peripherals::BT<'static>>>> =
    critical_section::Mutex::new(core::cell::RefCell::new(None));

/// The BT peripheral [`init_board`] parked, once. `None` before `init_board`
/// or after the first take.
#[cfg(all(feature = "ble", feature = "server", not(fw_harness)))]
pub fn take_bt() -> Option<esp_hal::peripherals::BT<'static>> {
    critical_section::with(|cs| BT.borrow_ref_mut(cs).take())
}

/// Start Embassy runtime
///
/// Starts the Embassy async runtime with the given timer and software interrupt.
#[cfg_attr(
    any(feature = "test_ble", feature = "test_comms_lab"),
    allow(
        dead_code,
        reason = "the BLE spike and the comms lab init their own peripherals"
    )
)]
pub fn start_runtime(
    timg0: TimerGroup<'static, impl TimerGroupInstance>,
    sw_int: SoftwareInterruptControl<'static>,
) {
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
}
