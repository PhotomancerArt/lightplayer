//! `dram2_seg` — the 72 KiB the second-stage bootloader ran in — as a second
//! heap region (feature `heap-dram2`, on by default).
//!
//! The image owns `dram_seg` (`0x3FC8_8000..0x3FCD_B700`) and leaves everything
//! above it to whoever runs first. esp-hal names the span `dram2_seg`
//! (`0x3FCD_B700..0x3FCE_D710`, 73,744 B,
//! `third_party/esp-hal/ld/esp32s3/memory.x`) and places `#[ram(reclaimed)]`
//! statics there (`.dram2_uninit`, `ld/sections/dram2.x`). Until this region,
//! nothing did: the ledger counted all of it idle (E1, 2026-10-09).
//!
//! # Who else uses these bytes, and when
//!
//! Read off the artifacts, not a datasheet:
//!
//! - **The mask ROM**, at boot only. `lp-emu/esp/roms/esp32s3_rev0_rom.elf`
//!   puts `.bss_shared_bufs` (`g_shared_buffers`, `0x3FCD_7E04 +0x1_1900`),
//!   `.stack_pro` (`0x3FCE_9704 +0x200C`) and `.stack_app`
//!   (`0x3FCE_B710 +0x2000`) inside the span, and its runtime data —
//!   `.bss_ets` onwards, which the ROM routines this image still calls
//!   (`esp_rom_spiflash_*`, the cache helpers) read — starts at
//!   `0x3FCE_D710`, exactly where `dram2_seg` ends. So the span stops below
//!   everything the ROM needs once the app runs.
//! - **The second-stage bootloader**, on every boot. espflash 3.3.0's
//!   `esp32s3-bootloader.bin` (the one `espflash save-image --merge` puts at
//!   offset 0 of the shipped chip) loads `0x3FCE_3818..0x3FCE_4F10` (data)
//!   and `0x403C_C700..0x403C_F5B0` (I-bus, = D `0x3FCD_C700..0x3FCD_F5B0`)
//!   into it, and runs on the ROM's PRO stack above them. The app's own
//!   contents of the span are therefore gone on the next boot — fine for a
//!   heap, which survives nothing.
//!
//! # Why this is not the C6's crash
//!
//! On the C6 the same reuse crashed the bootloader on warm resets: the
//! Bluetooth controller keeps running through an HP-system reset and its DMA
//! wrote into the bootloader's code
//! (`docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md`).
//! This chip runs no radio and no peripheral DMA at all — USB-Serial-JTAG
//! moves bytes through its FIFO, and the WS281x driver refills RMT RAM from
//! an interrupt — so nothing can write the span across a reset. The region
//! still carries **no capability tag**, the C6's rule: a request that asks
//! for `Internal` (esp-rtos's thread stacks today, a radio's buffers if this
//! chip ever gets one) cannot land in memory the bootloader takes back.
//!
//! # Why the JIT may use it
//!
//! This chip executes shader code straight out of the heap, through SRAM1's
//! I-bus alias (`lp-shader/lpvm-native/src/exec_addr.rs`: the window is
//! `0x3FC8_8000..0x3FCF_0000`). `dram2_seg` is inside that window, so a JIT
//! buffer that lands here is fetchable. SRAM2's low half
//! (`0x3FCF_0000..0x3FCF_8000`), the ledger's other idle span, is **not** —
//! SRAM2 has no I-bus view — which is why it is not a region here.

/// `dram2_seg`'s length: `0x3FCE_D710 - 0x3FCD_B700`.
pub const DRAM2_HEAP_BYTES: usize = 0x3FCE_D710 - 0x3FCD_B700;

#[esp_hal::ram(reclaimed)]
static mut HEAP_DRAM2: core::mem::MaybeUninit<[u8; DRAM2_HEAP_BYTES]> =
    core::mem::MaybeUninit::uninit();

/// Hand `dram2_seg` to the allocator, after the main arena (esp-alloc is
/// first-fit in registration order, so boot residents still pack into the
/// arena), and return `(start address, size)`.
pub fn add_region() -> (usize, usize) {
    let base = core::ptr::addr_of_mut!(HEAP_DRAM2).cast::<u8>();
    // SAFETY: the span is `'static`, exclusively the allocator's (no other
    // `.dram2_uninit` static exists in this image, and the ROM and the
    // bootloader are done with it before `main` — see the module docs), and
    // non-empty. No capability: see "Why this is not the C6's crash".
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            base,
            DRAM2_HEAP_BYTES,
            Default::default(),
        ));
    }
    (base as usize, DRAM2_HEAP_BYTES)
}
