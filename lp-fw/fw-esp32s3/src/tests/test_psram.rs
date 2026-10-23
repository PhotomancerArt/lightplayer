//! `test_psram`: the XIAO ESP32-S3's octal PSRAM, brought up and used. Never
//! ships (lane P of the S3 radio roadmap).
//!
//! Three things, each printed as one `[PSRAM]` line a test or a person reads:
//!
//! 1. **Bring-up through esp-hal's own driver** (`esp_hal::psram::Psram`,
//!    auto mode: octal first, then quad): the mode-register reads, the timing
//!    set-up, the size the chip reports and the MMU entries that map it are
//!    all the driver's, and its own `info!` decode of the chip is logged. It
//!    prints the range the driver mapped, or `absent` when it found no chip.
//!    The driver's decode (`vendor id : 0d (AP)` …) is the capture the
//!    emulated chip's mode registers are taken from
//!    (`lp-emu-esp32s3/src/psram.rs`).
//! 2. **Every mapped byte written and read back, twice**: a word pattern
//!    derived from the word's own index (so a page mapped at the wrong place,
//!    or two pages aliased onto one, reads back another word's value), then
//!    its complement (so every bit is seen both ways). The first wrong word is
//!    printed with its address.
//! 3. **Code run out of PSRAM through the instruction bus**: ten bytes of
//!    Xtensa (`entry; addx2; addi.n 7; retw.n`, i.e. `x * 3 + 7`) copied in
//!    through the data bus, the data cache written back and the instruction
//!    cache invalidated (the ROM's `Cache_WriteBack_All` /
//!    `Cache_Invalidate_ICache_All`), then called at the same page's
//!    instruction-bus address. The S3's one MMU table serves both buses at
//!    one index, so the instruction-bus address is the data-bus address plus
//!    `0x4200_0000 - 0x3C00_0000`. Then the same code rewritten in place,
//!    called before and after the publish: the publish rule's case.
//!
//! The cycle counts beside each pass are this core's `CCOUNT`, printed for a
//! reader comparing runs on one configuration. An emulated run's are the
//! emulator's model, ungraded, and say nothing about silicon's PSRAM speed.

use core::ptr::{read_volatile, write_volatile};

use esp_hal::psram::{Psram, PsramConfig};

/// The data bus's external-memory window (`psram.extmem_origin`, esp-hal's
/// metadata) and the instruction bus's, the same table index apart.
const DBUS_BASE: usize = 0x3C00_0000;
const IBUS_BASE: usize = 0x4200_0000;

/// `f(x) = x * 3 + 7`, assembled with `xtensa-esp32s3-elf-as`:
///
/// ```text
/// 0:  004136   entry   a1, 32
/// 3:  902220   addx2   a2, a2, a2
/// 6:  227b     addi.n  a2, a2, 7
/// 8:  f01d     retw.n
/// ```
const CODE: [u8; 10] = [0x36, 0x41, 0x00, 0x20, 0x22, 0x90, 0x7b, 0x22, 0x1d, 0xf0];

unsafe extern "C" {
    fn Cache_WriteBack_All() -> i32;
    fn Cache_Invalidate_ICache_All();
}

pub fn run(peri: esp_hal::peripherals::PSRAM<'static>) -> ! {
    esp_println::logger::init_logger(log::LevelFilter::Info);
    esp_println::println!("[PSRAM] start");
    let psram = Psram::new(peri, PsramConfig::default());
    let (ptr, len) = psram.raw_parts();
    if len == 0 {
        esp_println::println!("[PSRAM] absent");
        finish(true);
    }
    esp_println::println!(
        "[PSRAM] mapped {:#010x}..{:#010x} ({len} bytes)",
        ptr as usize,
        ptr as usize + len
    );

    let words = len / 4;
    let base = ptr as *mut u32;
    let mut ok = true;
    for (pass, seed) in [(1u32, 0u32), (2, u32::MAX)] {
        let t0 = cycles();
        for i in 0..words {
            unsafe { write_volatile(base.add(i), pattern(i, seed)) };
        }
        let t1 = cycles();
        let mut bad = 0usize;
        let mut first = None;
        for i in 0..words {
            let got = unsafe { read_volatile(base.add(i)) };
            if got != pattern(i, seed) {
                bad += 1;
                first.get_or_insert((i, got));
            }
        }
        let t2 = cycles();
        esp_println::println!(
            "[PSRAM] pass {pass}: {words} words, {bad} wrong, write {} cycles, read {} cycles",
            t1.wrapping_sub(t0),
            t2.wrapping_sub(t1)
        );
        if let Some((i, got)) = first {
            esp_println::println!(
                "[PSRAM] pass {pass}: first wrong at {:#010x}: {got:#010x}, wanted {:#010x}",
                base as usize + i * 4,
                pattern(i, seed)
            );
            ok = false;
        }
    }

    // The code goes at the start of the last page mapped, so it is a page the
    // pattern passes wrote through, not one only this step touched.
    let code_d = (ptr as usize + len - 0x1_0000) as *mut u8;
    for (i, b) in CODE.iter().enumerate() {
        unsafe { write_volatile(code_d.add(i), *b) };
    }
    unsafe {
        Cache_WriteBack_All();
        Cache_Invalidate_ICache_All();
    }
    let code_i = code_d as usize - DBUS_BASE + IBUS_BASE;
    let f: extern "C" fn(u32) -> u32 = unsafe { core::mem::transmute(code_i) };
    let got = f(core::hint::black_box(5));
    esp_println::println!("[PSRAM] exec at {code_i:#010x}: f(5) = {got} (want 22)");
    ok &= got == 22;

    // The publish rule: rewrite the code in place (`g(x) = x * 3 + 1`, one
    // byte differs) and call it before and after publishing. Before, silicon
    // may still run the cached `f` (the spike saw it stale); the emulator's
    // windows are never stale, so it prints `fresh`. Only the published call
    // counts towards `ok`.
    unsafe { write_volatile(code_d.add(6), 0x1b) };
    let before = f(core::hint::black_box(5));
    unsafe {
        Cache_WriteBack_All();
        Cache_Invalidate_ICache_All();
    }
    let after = f(core::hint::black_box(5));
    esp_println::println!(
        "[PSRAM] rewrite: before publish f(5) = {before} ({}), after f(5) = {after} (want 16)",
        match before {
            22 => "stale",
            16 => "fresh",
            _ => "neither",
        }
    );
    ok &= after == 16;

    drop(psram);
    finish(ok);
}

fn finish(ok: bool) -> ! {
    esp_println::println!("[PSRAM] done {}", if ok { "ok" } else { "FAILED" });
    let delay = esp_hal::delay::Delay::new();
    loop {
        delay.delay_millis(1000);
    }
}

/// The word written at index `i`: a multiplicative hash of the index, so no
/// two words in 8 MB share a value and an aliased page reads back the wrong
/// one, then `seed` (0, then all ones).
fn pattern(i: usize, seed: u32) -> u32 {
    (i as u32).wrapping_mul(0x9E37_79B1).wrapping_add(0x5EED) ^ seed
}

fn cycles() -> u32 {
    crate::board::esp32s3::cycle_counter::read()
}
