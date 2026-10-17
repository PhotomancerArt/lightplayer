//! EXPERIMENT ONLY — branch `exp/c6-flash-mode`, never merged.
//!
//! Plan `lp2025/2026-10-16-0848-tree-store-adoption`: does the C6's flash run
//! DIO at 40 MHz, and what do 80 MHz and QIO buy? The bootloader espflash
//! bundles (ESP-IDF v5.1-beta1) was built without QIO support, so a QIO
//! header alone changes nothing; this switches from the app instead.
//!
//! At boot (before the mount) it prints the flash controller's registers
//! (`[flashx] …`), and by the build-time `LP_FLASH_EXP` it may switch:
//! `read` (default, nothing), `clk80` (SPI0+SPI1 clock to the MSPI clock),
//! `qio` (set the part's QE bit, then QIO reads), `qio80` (both).
//! With `heap_peak_diag` it also times 256 KiB of cache line fills and sums
//! 1 MiB of the image, before and after the switch.

use core::ptr::{read_volatile, write_volatile};

const SPI0: usize = 0x6000_2000;
const SPI1: usize = 0x6000_3000;
const PCR: usize = 0x6009_6000;

const CMD_USR: u32 = 1 << 18;

unsafe extern "C" {
    fn Cache_Suspend_ICache() -> u32;
    fn Cache_Resume_ICache(autoload: u32);
    fn esp_rom_spiflash_config_readmode(mode: u32) -> u32;
    fn esp_rom_spiflash_select_qio_pins(wp_gpio_num: u8, ishspi: u32);
}

#[inline(always)]
fn rd(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}
#[inline(always)]
fn wr(a: usize, v: u32) {
    unsafe { write_volatile(a as *mut u32, v) }
}

fn regs_line(tag: &str) {
    esp_println::println!(
        "[flashx] {tag}: spi0 ctrl={:08x} clock={:08x} user={:08x} user1={:08x} user2={:08x} fctrl={:08x} din_mode={:08x} din_num={:08x} | spi1 ctrl={:08x} clock={:08x} user1={:08x} | pcr mspi_conf={:08x} mspi_clk={:08x} sysclk={:08x}",
        rd(SPI0 + 0x08),
        rd(SPI0 + 0x14),
        rd(SPI0 + 0x18),
        rd(SPI0 + 0x1c),
        rd(SPI0 + 0x20),
        rd(SPI0 + 0x3c),
        rd(SPI0 + 0x184),
        rd(SPI0 + 0x188),
        rd(SPI1 + 0x08),
        rd(SPI1 + 0x14),
        rd(SPI1 + 0x1c),
        rd(PCR + 0x18),
        rd(PCR + 0x1c),
        rd(PCR + 0x110),
    );
}

/// One SPI1 user command: `cmd`, then `mosi_bits` of `mosi`, then
/// `miso_bits` read back. RAM-only (called with the cache suspended).
#[inline(always)]
fn spi1_cmd(cmd: u8, mosi_bits: u32, mosi: u32, miso_bits: u32) -> u32 {
    while rd(SPI1) & CMD_USR != 0 {}
    let ctrl = rd(SPI1 + 0x08);
    let user = rd(SPI1 + 0x18);
    let user2 = rd(SPI1 + 0x20);
    let mosi_dlen = rd(SPI1 + 0x24);
    let miso_dlen = rd(SPI1 + 0x28);
    // Single-line, WP held high while idle.
    wr(SPI1 + 0x08, 1 << 21);
    let mut u = user & !((0x1f << 27) | (0b11 << 24) | (0xf << 12));
    u |= 1 << 31;
    if mosi_bits > 0 {
        u |= 1 << 27;
    }
    if miso_bits > 0 {
        u |= 1 << 28;
    }
    wr(SPI1 + 0x18, u);
    wr(SPI1 + 0x20, (7 << 28) | cmd as u32);
    wr(SPI1 + 0x24, mosi_bits.saturating_sub(1));
    wr(SPI1 + 0x28, miso_bits.saturating_sub(1));
    wr(SPI1 + 0x58, mosi);
    wr(SPI1, CMD_USR);
    while rd(SPI1) & CMD_USR != 0 {}
    let r = rd(SPI1 + 0x58);
    wr(SPI1 + 0x08, ctrl);
    wr(SPI1 + 0x18, user);
    wr(SPI1 + 0x20, user2);
    wr(SPI1 + 0x24, mosi_dlen);
    wr(SPI1 + 0x28, miso_dlen);
    if miso_bits >= 32 { r } else { r & ((1u32 << miso_bits) - 1) }
}

#[inline(always)]
fn wait_wip() -> bool {
    let mut n = 0u32;
    while spi1_cmd(0x05, 0, 0, 8) & 1 != 0 {
        n += 1;
        if n > 4_000_000 {
            return false;
        }
    }
    true
}

/// Result bits: 1 = QE was already set, 2 = set by 0x31, 4 = set by 16-bit
/// 0x01, 8 = QE could not be set (QIO not entered), 16 = clock switched,
/// 32 = QIO entered. `sr` gets (sr1, sr2) before and after.
#[esp_hal::ram]
#[inline(never)]
fn switch(qio: bool, clk80: bool, sr: &mut [u32; 4]) -> u32 {
    let mut out = 0;
    let autoload = unsafe { Cache_Suspend_ICache() };
    sr[0] = spi1_cmd(0x05, 0, 0, 8);
    sr[1] = spi1_cmd(0x35, 0, 0, 8);
    let mut qe = sr[1] & 2 != 0;
    if qio {
        if qe {
            out |= 1;
        } else {
            spi1_cmd(0x06, 0, 0, 0);
            spi1_cmd(0x31, 8, sr[1] | 2, 0);
            wait_wip();
            if spi1_cmd(0x35, 0, 0, 8) & 2 != 0 {
                qe = true;
                out |= 2;
            } else {
                spi1_cmd(0x06, 0, 0, 0);
                spi1_cmd(0x01, 16, sr[0] | ((sr[1] | 2) << 8), 0);
                wait_wip();
                if spi1_cmd(0x35, 0, 0, 8) & 2 != 0 {
                    qe = true;
                    out |= 4;
                } else {
                    out |= 8;
                }
            }
        }
        if qe {
            unsafe {
                esp_rom_spiflash_config_readmode(0); // ESP_ROM_SPIFLASH_QIO_MODE
                esp_rom_spiflash_select_qio_pins(0, 0);
            }
            out |= 32;
        }
    }
    if clk80 {
        // 1-division: the SPI bus clock equals the MSPI module clock.
        wr(SPI0 + 0x14, 1 << 31);
        wr(SPI1 + 0x14, 1 << 31);
        out |= 16;
    }
    sr[2] = spi1_cmd(0x05, 0, 0, 8);
    sr[3] = spi1_cmd(0x35, 0, 0, 8);
    unsafe { Cache_Resume_ICache(autoload) };
    out
}

fn jedec() -> u32 {
    critical_section::with(|_| {
        wr(SPI1, 1 << 28);
        while rd(SPI1) & (1 << 28) != 0 {}
        rd(SPI1 + 0x58) & 0x00ff_ffff
    })
}

#[cfg(feature = "heap_peak_diag")]
fn bench(tag: &str) {
    if option_env!("LP_FLASH_BENCH") == Some("0") {
        return;
    }
    // 256 KiB of distinct 32-byte lines (8 x the 32 KiB cache): line fills.
    let base = 0x4208_0000usize;
    let t0 = esp_hal::time::Instant::now();
    let mut acc = 0u32;
    let mut a = base;
    while a < base + 0x4_0000 {
        acc = acc.wrapping_add(rd(a));
        a += 32;
    }
    let fill_us = t0.elapsed().as_micros();
    // 1 MiB summed word by word: the bytes, compared across the switch.
    let t1 = esp_hal::time::Instant::now();
    let mut sum = 0u32;
    let mut a = 0x4200_0000usize;
    while a < 0x4210_0000 {
        sum = sum.rotate_left(5) ^ rd(a);
        a += 4;
    }
    let sum_us = t1.elapsed().as_micros();
    esp_println::println!(
        "[flashx] bench {tag}: 8192 line fills in {fill_us} us ({} ns/line), 1 MiB sum {sum:08x} in {sum_us} us (acc {acc:08x})",
        fill_us * 1000 / 8192
    );
}

pub fn run() {
    let mode = option_env!("LP_FLASH_EXP").unwrap_or("read");
    let pkg: u8 = esp_hal::efuse::read_field_le(esp_hal::efuse::PKG_VERSION);
    let cap: u8 = esp_hal::efuse::read_field_le(esp_hal::efuse::FLASH_CAP);
    let vendor: u8 = esp_hal::efuse::read_field_le(esp_hal::efuse::FLASH_VENDOR);
    let temp: u8 = esp_hal::efuse::read_field_le(esp_hal::efuse::FLASH_TEMP);
    esp_println::println!(
        "[flashx] mode={mode} jedec={:06x} efuse pkg={pkg} flash_cap={cap} flash_vendor={vendor} flash_temp={temp}",
        jedec()
    );
    regs_line("before");
    #[cfg(feature = "heap_peak_diag")]
    bench("before");
    let (qio, clk80) = match mode {
        "clk80" => (false, true),
        "qio" => (true, false),
        "qio80" => (true, true),
        _ => (false, false),
    };
    let mut sr = [0u32; 4];
    let t = esp_hal::time::Instant::now();
    let out = critical_section::with(|_| switch(qio, clk80, &mut sr));
    esp_println::println!(
        "[flashx] switch out={out:#x} sr1={:02x} sr2={:02x} -> sr1={:02x} sr2={:02x} in {} us",
        sr[0],
        sr[1],
        sr[2],
        sr[3],
        t.elapsed().as_micros()
    );
    regs_line("after");
    #[cfg(feature = "heap_peak_diag")]
    bench("after");
}
