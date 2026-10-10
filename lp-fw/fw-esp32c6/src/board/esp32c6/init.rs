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

    // SAFETY: called once, here, before anything allocates.
    unsafe { add_heap_regions() };

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

/// Register the heap's three regions with esp-alloc — main, the reclaimed
/// tail, radio: the order Rust's allocator tries them in. Every image that
/// boots this chip's heap calls it (the product through [`init_board`], and
/// the BLE spike and comms lab, which init alone), so a harness has the
/// product's heap region for region.
///
/// The RAM split (docs/adr/2026-09-02-esp32c6-ram-split.md; RAM research E4
/// for this layout). The linker (`build.rs`, `patched_stack_x`) lays it out:
///
/// - **The main stack is `dram2_seg`, all 64 KiB** (0x4086E610..0x4087E610),
///   the segment the ESP-IDF second-stage bootloader loads into and then
///   vacates. Until E4 the stack was whatever `.data`/`.bss` left at the top
///   of main RAM (~49 KB, against meteor's ~35 KB steady-state high-water,
///   and 32,776 B once, which overflowed: the 2026-09-01 bench), and
///   `dram2_seg` was a second heap region of its own. `stack_probe` paints
///   the stack at boot and the heartbeat logs its high-water.
/// - **The main region is main RAM's residual**, `.heap_main`: from the end of
///   the statics up to the lowest address a bootloader we ship loads into
///   (0x4086B910). Its size moves with every byte of statics, as the stack's
///   did; it holds what the old main region and the old stack held, as one
///   block.
/// - **The reclaimed tail**, `.heap_reclaimed`: main RAM's last 11,520 B
///   (0x4086B910..0x4086E610), which the bootloader loads into on every reset
///   while the radio — which does not reset with the HP system — keeps
///   running. A radio DMA write that outlives a warm reset landed on the
///   bootloader there once
///   (docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md),
///   so this region carries NO capability tag: only capability-free requests
///   (Rust's global allocator) can land in it, never the radio's C heap or
///   esp-radio's `InternalMemory`, as `dram2_seg` was before.
/// - **The radio's region**, `HEAP_RADIO`, in `.bss` far below any bootloader
///   load address. Registered last, so Rust reaches it only once the other two
///   are full; the C heap asks for it first by its tag (see `c_heap`).
///
/// # Safety
///
/// Once per boot, before the first allocation: each span is handed to the
/// allocator exactly once and nothing else ever names it except to read its
/// address.
pub unsafe fn add_heap_regions() {
    let (main, main_size) = main_region();
    let (reclaimed, reclaimed_size) = reclaimed_region();
    // SAFETY: the caller's; the spans are the linker's, disjoint from every
    // static and from the stack (`build.rs`'s ASSERTs).
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            main as *mut u8,
            main_size,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            reclaimed as *mut u8,
            reclaimed_size,
            crate::c_heap::BOOTLOADER_RECLAIMED,
        ));
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            core::ptr::addr_of_mut!(HEAP_RADIO).cast::<u8>(),
            HEAP_RADIO_SIZE,
            crate::c_heap::RADIO,
        ));
    }
}

/// The radio blobs' C heap (`c_heap`), in main RAM where no bootloader
/// loads. Sized from silicon: 44,584 B of radio allocations with Bluetooth
/// up (2026-09-24), less the 10,320 B the lean ESP-NOW buffers gave back
/// (2026-10-01), is ~34.3 KB at no connections; each of the two links adds
/// up to ~3.5 KB. 48 KiB holds that with ~7 KB to spare, and the
/// `[radio-heap]` heartbeat line reports the real high-water. A request
/// that does not fit still succeeds, in the main region.
pub const HEAP_RADIO_SIZE: usize = 49_152;
/// A heap region's backing array, 8-aligned so the allocator loses nothing
/// to aligning its start (a bare byte array can land on an odd address).
#[repr(C, align(8))]
struct HeapArena<const N: usize>(core::mem::MaybeUninit<[u8; N]>);
static mut HEAP_RADIO: HeapArena<HEAP_RADIO_SIZE> = HeapArena(core::mem::MaybeUninit::uninit());

// The main and reclaimed regions' bounds (`build.rs`, `patched_stack_x`).
unsafe extern "C" {
    static _heap_main_start: u8;
    static _heap_main_end: u8;
    static _heap_reclaimed_start: u8;
    static _heap_reclaimed_end: u8;
}

/// The heap's three regions as `(start address, size)` — main, the reclaimed
/// tail, radio: the order Rust's allocator tries them in.
#[allow(dead_code, reason = "read only by the heap diagnostics")]
pub fn heap_regions() -> [(usize, usize); 3] {
    [main_region(), reclaimed_region(), radio_region()]
}

/// The main region as `(start address, size)`: main RAM's residual.
pub fn main_region() -> (usize, usize) {
    span(&raw const _heap_main_start, &raw const _heap_main_end)
}

/// The reclaimed tail as `(start address, size)`: main RAM's last 11,520 B,
/// which the second-stage bootloader loads into.
pub fn reclaimed_region() -> (usize, usize) {
    span(
        &raw const _heap_reclaimed_start,
        &raw const _heap_reclaimed_end,
    )
}

/// The radio's region as `(start address, size)`.
pub fn radio_region() -> (usize, usize) {
    (core::ptr::addr_of!(HEAP_RADIO) as usize, HEAP_RADIO_SIZE)
}

fn span(start: *const u8, end: *const u8) -> (usize, usize) {
    (start as usize, end as usize - start as usize)
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
