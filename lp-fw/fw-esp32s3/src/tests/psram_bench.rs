//! PSRAM spike harness (never shipped; branch `claude/spike-s3-psram-*`).
//!
//! What the S3's 8 MB of octal PSRAM gives and costs, measured on silicon
//! against internal SRAM and flash through the cache. Every result is one
//! `[PSRAM] …` line; the host reads the transcript. Nothing here grades
//! itself except the memtest, the atomics probe and the JIT corpus goldens.
//!
//! `LP_PSRAM_MHZ` (build time, 40 or 80; default 40) picks the PSRAM clock.
//! `LP_PSRAM_DCACHE` is reported only — the D-cache size is esp-hal's
//! `ESP_HAL_CONFIG_DATA_CACHE_SIZE`, set in the environment at build time.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use esp_hal::psram::{Psram, PsramConfig, PsramMode, PsramSize, SpiRamFreq};
use esp_println::println;
use lpir::FloatMode;
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_native::native_options::NativeCompileOptions;
use lpvm_native::rt_jit::{BuiltinTable, NativeJitEngine};
use lpvm_native::xt_corpus::CASES;

use crate::board::esp32s3::{cycle_counter, fpu};

const TAG: &str = "[PSRAM]";
const CPU_MHZ: u64 = 240;

unsafe extern "C" {
    fn Cache_WriteBack_All();
    fn Cache_Invalidate_DCache_All();
    fn Cache_Invalidate_ICache_All();
    fn rom_Cache_WriteBack_Addr(addr: u32, size: u32);
    fn Cache_Invalidate_Addr(addr: u32, size: u32) -> i32;
    fn Cache_Suspend_DCache_Autoload() -> u32;
    fn Cache_Resume_DCache_Autoload(value: u32);
    fn Cache_Get_DCache_Line_Size() -> u32;
    fn Cache_Get_ICache_Line_Size() -> u32;
}

const MMU_TABLE: usize = 0x600C_5000;
const EXT_DBUS: usize = 0x3C00_0000;
const EXT_IBUS: usize = 0x4200_0000;
const SRAM1_IBUS_ALIAS: usize = 0x6F_0000;

fn mhz() -> SpiRamFreq {
    match option_env!("LP_PSRAM_MHZ") {
        Some("80") => SpiRamFreq::Freq80m,
        Some("120") => SpiRamFreq::Freq120m,
        _ => SpiRamFreq::Freq40m,
    }
}

fn now_us() -> u64 {
    esp_hal::time::Instant::now().duration_since_epoch().as_micros()
}

/// Cycles → nanoseconds ×100 (two decimals without floats).
fn ns_x100(cycles: u64, n: u64) -> u64 {
    cycles * 100_000 / CPU_MHZ / n.max(1)
}

fn mb_per_s(bytes: u64, cycles: u64) -> u64 {
    // bytes / (cycles / 240e6) / 1e6
    bytes * CPU_MHZ / cycles.max(1)
}

fn fmt2(x100: u64) -> (u64, u64) {
    (x100 / 100, x100 % 100)
}

/// Write every dirty PSRAM line back and drop the whole D-cache, so the next
/// read of PSRAM or flash misses. Internal SRAM is not cached.
fn cold_dcache() {
    unsafe {
        Cache_WriteBack_All();
        Cache_Invalidate_DCache_All();
    }
}

pub fn run(
    psram_peri: esp_hal::peripherals::PSRAM<'static>,
    flash_peri: esp_hal::peripherals::FLASH<'static>,
    cpu_peri: esp_hal::peripherals::CPU_CTRL<'static>,
) -> ! {
    cycle_counter::setup();
    // Give the host time to open the re-enumerated port.
    let wait = now_us() + 4_000_000;
    while now_us() < wait {}

    println!("{TAG} spike harness: commit {} dirty {}", env!("LP_BUILD_COMMIT"), env!("LP_BUILD_DIRTY"));
    let (dl, il) = unsafe { (Cache_Get_DCache_Line_Size(), Cache_Get_ICache_Line_Size()) };
    println!(
        "{TAG} caches: dcache {} line {dl} B, icache line {il} B, psram clock {:?}",
        option_env!("LP_PSRAM_DCACHE").unwrap_or("32KB(default)"),
        mhz()
    );

    // ── Init cost ────────────────────────────────────────────────────────
    let boot_us = now_us();
    let t0 = cycle_counter::read();
    let psram = Psram::new(
        psram_peri,
        PsramConfig {
            mode: PsramMode::OctalSpi,
            size: PsramSize::AutoDetect,
            ram_frequency: mhz(),
            ..Default::default()
        },
    );
    let init_cycles = cycle_counter::read().wrapping_sub(t0) as u64;
    let (base, size) = psram.raw_parts();
    let base = base as usize;
    println!(
        "{TAG} init: {} us ({} cycles), at {} us since boot; mapped {:#010x}..{:#010x} ({} B), MMU entries {:#x}..{:#x}",
        init_cycles / CPU_MHZ,
        init_cycles,
        boot_us,
        base,
        base + size,
        size,
        (base - EXT_DBUS) / 0x1_0000,
        (base + size - EXT_DBUS) / 0x1_0000
    );
    if size == 0 {
        println!("{TAG} no PSRAM — stopping");
        park();
    }
    dump_mmu();

    // ── Memtest, whole part ──────────────────────────────────────────────
    memtest(base, size);

    // ── Atomics on PSRAM ─────────────────────────────────────────────────
    atomics(base + size - 64);

    // ── Data: bandwidth ──────────────────────────────────────────────────
    let mut sram: Vec<u32> = alloc::vec![0x5A5A_5A5Au32; 128 * 1024 / 4];
    let sram_ptr = sram.as_mut_ptr();
    let flash_ptr = FLASH_BLOB.0.as_ptr();
    println!("{TAG} buffers: sram {:#010x} psram {:#010x} flash {:#010x}", sram_ptr as usize, base, flash_ptr as usize);
    let psram_ptr = base as *mut u32;
    bandwidth(sram_ptr, psram_ptr, flash_ptr);

    // ── Data: latency ────────────────────────────────────────────────────
    latency(sram_ptr, psram_ptr, flash_ptr);
    drop(sram);

    // ── Code: fetch through each path ────────────────────────────────────
    code_fetch(base + 4 * 1024 * 1024);

    // ── Flash writes with PSRAM dirty in the cache ───────────────────────
    let mut fs = esp_storage::FlashStorage::new(flash_peri);
    flash_write_hazard(&mut fs, base + 5 * 1024 * 1024);

    // The frame copy an output path would add (PSRAM render → internal).
    for n in [1860usize, 7440] {
        let mut d = alloc::vec![0u8; n];
        cold_dcache();
        let c = time(|| unsafe { core::ptr::copy_nonoverlapping(base as *const u8, d.as_mut_ptr(), n) });
        println!("{TAG} frame copy {n} B psram->sram cold: {} us", c / CPU_MHZ);
    }

    // ── The JIT corpus, code and data in internal SRAM, then in PSRAM ────
    // The top 2 MB become an `External` heap region; everything above this
    // line used raw PSRAM below it.
    let heap_base = base + size - 2 * 1024 * 1024;
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            heap_base as *mut u8,
            2 * 1024 * 1024 - 4096,
            esp_alloc::MemoryCapability::External.into(),
        ));
    }
    lpvm_native::XT_CODE_PUBLISH.store(publish_code as usize, Ordering::Relaxed);
    let cpenable = fpu::arm();
    println!("{TAG} cpenable {cpenable:#010x}");
    jit_corpus("internal", usize::MAX);
    jit_corpus("psram", 0);
    esp_alloc::EXTERNAL_FIRST_MIN.store(usize::MAX, Ordering::Relaxed);

    core1_contention(cpu_peri, base, &mut fs);

    println!("{TAG} done");
    park();
}

fn park() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

fn dump_mmu() {
    let mut runs = alloc::string::String::new();
    let mut i = 0usize;
    while i < 512 {
        let v = unsafe { core::ptr::read_volatile((MMU_TABLE + 4 * i) as *const u32) };
        if v & 0x4000 == 0 {
            let start = i;
            let psram = v & 0x8000 != 0;
            while i + 1 < 512 {
                let w = unsafe { core::ptr::read_volatile((MMU_TABLE + 4 * (i + 1)) as *const u32) };
                if w & 0x4000 != 0 || (w & 0x8000 != 0) != psram {
                    break;
                }
                i += 1;
            }
            use core::fmt::Write;
            let _ = write!(runs, " {:#x}..={:#x}{}", start, i, if psram { "(psram)" } else { "" });
        }
        i += 1;
    }
    println!("{TAG} mmu valid entries:{runs}");
}

fn memtest(base: usize, size: usize) {
    let words = size / 4;
    let p = base as *mut u32;
    for pass in 0..2u32 {
        let key = 0x9E37_79B9u32.wrapping_mul(pass + 1);
        let t0 = cycle_counter::read();
        for i in 0..words {
            unsafe { p.add(i).write_volatile((i as u32).wrapping_mul(key) ^ (i as u32 >> 7)) };
        }
        let tw = cycle_counter::read().wrapping_sub(t0) as u64;
        cold_dcache();
        let t1 = cycle_counter::read();
        let mut errors = 0u32;
        let mut first = None;
        for i in 0..words {
            let want = (i as u32).wrapping_mul(key) ^ (i as u32 >> 7);
            let got = unsafe { p.add(i).read_volatile() };
            if got != want {
                errors += 1;
                if first.is_none() {
                    first = Some((i, want, got));
                }
            }
        }
        let tr = cycle_counter::read().wrapping_sub(t1) as u64;
        println!(
            "{TAG} memtest pass {pass}: {size} B, write {} MB/s, read {} MB/s, errors {errors} first {:?}",
            mb_per_s(size as u64, tw),
            mb_per_s(size as u64, tr),
            first
        );
    }
}

fn atomics(addr: usize) {
    let a = unsafe { &*(addr as *const AtomicU32) };
    a.store(7, Ordering::SeqCst);
    // One `s32c1i` each: no retry loop, so a broken CAS cannot hang here.
    let r1 = a.compare_exchange(7, 8, Ordering::SeqCst, Ordering::SeqCst);
    let v1 = a.load(Ordering::SeqCst);
    let r2 = a.compare_exchange(7, 9, Ordering::SeqCst, Ordering::SeqCst);
    let v2 = a.load(Ordering::SeqCst);
    println!("{TAG} atomics on psram: cas(7->8)={r1:?} now {v1}; cas(7->9)={r2:?} now {v2}");
    if r1 == Ok(7) && v1 == 8 && r2 == Err(8) && v2 == 8 {
        let mut n = 0;
        for _ in 0..1000 {
            a.fetch_add(1, Ordering::SeqCst);
            n += 1;
        }
        println!("{TAG} atomics on psram: fetch_add x{n} -> {} (want 1008)", a.load(Ordering::SeqCst));
    } else {
        println!("{TAG} atomics on psram: CAS BROKEN — skipping fetch_add (its retry loop would spin)");
    }
    // The same through a cold line.
    a.store(3, Ordering::SeqCst);
    cold_dcache();
    let r3 = a.compare_exchange(3, 4, Ordering::SeqCst, Ordering::SeqCst);
    println!("{TAG} atomics on psram (cold line): cas(3->4)={r3:?} now {}", a.load(Ordering::SeqCst));
}

#[inline(never)]
fn seq_read(p: *const u32, words: usize) -> u32 {
    let mut s = 0u32;
    let mut i = 0;
    while i + 8 <= words {
        unsafe {
            s = s
                .wrapping_add(p.add(i).read_volatile())
                .wrapping_add(p.add(i + 1).read_volatile())
                .wrapping_add(p.add(i + 2).read_volatile())
                .wrapping_add(p.add(i + 3).read_volatile())
                .wrapping_add(p.add(i + 4).read_volatile())
                .wrapping_add(p.add(i + 5).read_volatile())
                .wrapping_add(p.add(i + 6).read_volatile())
                .wrapping_add(p.add(i + 7).read_volatile());
        }
        i += 8;
    }
    s
}

#[inline(never)]
fn seq_write(p: *mut u32, words: usize, v: u32) {
    let mut i = 0;
    while i + 8 <= words {
        unsafe {
            p.add(i).write_volatile(v);
            p.add(i + 1).write_volatile(v);
            p.add(i + 2).write_volatile(v);
            p.add(i + 3).write_volatile(v);
            p.add(i + 4).write_volatile(v);
            p.add(i + 5).write_volatile(v);
            p.add(i + 6).write_volatile(v);
            p.add(i + 7).write_volatile(v);
        }
        i += 8;
    }
}

fn time<F: FnMut()>(mut f: F) -> u64 {
    let t0 = cycle_counter::read();
    f();
    cycle_counter::read().wrapping_sub(t0) as u64
}

fn bandwidth(sram: *mut u32, psram: *mut u32, flash: *const u32) {
    let sizes: [usize; 6] = [4 << 10, 16 << 10, 32 << 10, 64 << 10, 256 << 10, 1 << 20];
    for &bytes in &sizes {
        let words = bytes / 4;
        let mut line = alloc::string::String::new();
        use core::fmt::Write;
        // Each read: one cold pass (cache dropped) then one warm pass.
        let mut row = |name: &str, p: *const u32, ok: bool| {
            if !ok {
                return;
            }
            cold_dcache();
            let cold = time(|| {
                core::hint::black_box(seq_read(p, words));
            });
            let warm = time(|| {
                core::hint::black_box(seq_read(p, words));
            });
            let _ = write!(line, " {name} cold {} warm {} |", mb_per_s(bytes as u64, cold), mb_per_s(bytes as u64, warm));
        };
        row("sram", sram, bytes <= 128 << 10);
        row("psram", psram, true);
        row("flash", flash, bytes <= FLASH_BLOB_BYTES);
        println!("{TAG} read MB/s {bytes:>8} B:{line}");

        let mut wline = alloc::string::String::new();
        if bytes <= 128 << 10 {
            let c = time(|| seq_write(sram, words, 0x1234_5678));
            let _ = write!(wline, " sram {} |", mb_per_s(bytes as u64, c));
        }
        cold_dcache();
        let c = time(|| seq_write(psram, words, 0x1234_5678));
        let wb = time(|| unsafe { Cache_WriteBack_All() });
        let _ = write!(
            wline,
            " psram {} (+writeback {} us → {} incl.)",
            mb_per_s(bytes as u64, c),
            wb / CPU_MHZ,
            mb_per_s(bytes as u64, c + wb)
        );
        println!("{TAG} write MB/s {bytes:>8} B:{wline}");
    }

    // memcpy: the firmware's own `copy_nonoverlapping` (compiler-builtins,
    // optimize_for_size), 64 KB blocks, in every direction that matters.
    let n = 64 * 1024;
    let s8 = sram as *mut u8;
    let p8 = psram as *mut u8;
    let f8 = flash as *const u8;
    let s_half = unsafe { s8.add(64 * 1024) };
    let p_far = unsafe { p8.add(2 * 1024 * 1024) };
    let mut cp = |name: &str, dst: *mut u8, src: *const u8| {
        cold_dcache();
        let c = time(|| unsafe { core::ptr::copy_nonoverlapping(src, dst, n) });
        let wb = time(|| unsafe { Cache_WriteBack_All() });
        let w = time(|| unsafe { word_copy(dst as *mut u32, src as *const u32, n / 4) });
        println!(
            "{TAG} memcpy 64 KB {name}: copy_nonoverlapping {} MB/s (+writeback {} us), word loop {} MB/s",
            mb_per_s(n as u64, c),
            wb / CPU_MHZ,
            mb_per_s(n as u64, w)
        );
    };
    cp("sram->sram", s_half, s8);
    cp("psram->psram", p_far, p8);
    cp("sram->psram", p8, s8);
    cp("psram->sram", s8, p8);
    cp("flash->sram", s8, f8);
    cp("flash->psram", p8, f8);
}

#[inline(never)]
unsafe fn word_copy(dst: *mut u32, src: *const u32, words: usize) {
    let mut i = 0;
    while i + 4 <= words {
        unsafe {
            let a = src.add(i).read_volatile();
            let b = src.add(i + 1).read_volatile();
            let c = src.add(i + 2).read_volatile();
            let d = src.add(i + 3).read_volatile();
            dst.add(i).write_volatile(a);
            dst.add(i + 1).write_volatile(b);
            dst.add(i + 2).write_volatile(c);
            dst.add(i + 3).write_volatile(d);
        }
        i += 4;
    }
}

/// Dependent random reads: the next index is an LCG step plus the loaded
/// word masked by a zero the compiler cannot see, so every load waits for the
/// one before it. The LCG costs the same few cycles in every memory; the
/// `sram` row is that floor.
#[inline(never)]
fn chase(p: *const u32, mask: u32, hops: u32, zero: u32) -> u32 {
    let mut i = 0u32;
    for _ in 0..hops {
        let v = unsafe { p.add(i as usize).read_volatile() };
        i = (i.wrapping_mul(1_664_525).wrapping_add(1_013_904_223).wrapping_add(v & zero)) & mask;
    }
    i
}

fn latency(sram: *mut u32, psram: *mut u32, flash: *const u32) {
    let zero = core::hint::black_box(0u32);
    let sets: [usize; 11] =
        [4 << 10, 16 << 10, 24 << 10, 32 << 10, 48 << 10, 64 << 10, 128 << 10, 512 << 10, 1 << 20, 4 << 20, 0];
    for &bytes in sets.iter().filter(|b| **b != 0) {
        let mask = (bytes / 4 - 1) as u32;
        let hops = 200_000u32;
        let mut line = alloc::string::String::new();
        use core::fmt::Write;
        let mut row = |name: &str, p: *const u32, ok: bool| {
            if !ok {
                return;
            }
            // Warm pass first (fills what fits), then the timed pass.
            chase(p, mask, hops / 4, zero);
            let c = time(|| {
                core::hint::black_box(chase(p, mask, hops, zero));
            });
            let (a, b) = fmt2(ns_x100(c, hops as u64));
            let _ = write!(line, " {name} {a}.{b:02} ns |");
        };
        row("sram", sram, bytes <= 128 << 10);
        row("psram", psram, true);
        row("flash", flash, bytes <= FLASH_BLOB_BYTES);
        println!("{TAG} random read latency {bytes:>8} B:{line}");
    }
}

// ── Code ────────────────────────────────────────────────────────────────

const ENTRY: [u8; 3] = [0x36, 0x41, 0x00]; // entry a1, 32
const ADDI_N: [u8; 2] = [0x1B, 0x22]; // addi.n a2, a2, 1
const RETW_N: [u8; 2] = [0x1D, 0xF0]; // retw.n

/// `entry; addi.n a2,a2,1 × k; retw.n` filling `n` bytes (n odd-safe:
/// pads with a trailing `addi.n` count chosen to fit).
const fn code_blob<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    out[0] = ENTRY[0];
    out[1] = ENTRY[1];
    out[2] = ENTRY[2];
    let body = (N - 3 - 2) / 2;
    let mut i = 0;
    while i < body {
        out[3 + 2 * i] = ADDI_N[0];
        out[3 + 2 * i + 1] = ADDI_N[1];
        i += 1;
    }
    out[3 + 2 * body] = RETW_N[0];
    out[3 + 2 * body + 1] = RETW_N[1];
    out
}

const fn addis(n: usize) -> u32 {
    ((n - 5) / 2) as u32
}

#[repr(C, align(64))]
struct Code<const N: usize>([u8; N]);

#[unsafe(link_section = ".text.psram_spike_code_8k")]
static FLASH_CODE_8K: Code<8192> = Code(code_blob::<8192>());
#[unsafe(link_section = ".text.psram_spike_code_24k")]
static FLASH_CODE_24K: Code<24576> = Code(code_blob::<24576>());
#[unsafe(link_section = ".text.psram_spike_code_64k")]
static FLASH_CODE_64K: Code<65536> = Code(code_blob::<65536>());
#[unsafe(link_section = ".text.psram_spike_code_256k")]
static FLASH_CODE_256K: Code<262144> = Code(code_blob::<262144>());

/// 512 KB of non-zero rodata in flash for the data tests.
const FLASH_BLOB_BYTES: usize = 512 * 1024;
#[repr(C, align(64))]
struct Blob([u32; FLASH_BLOB_BYTES / 4]);
static FLASH_BLOB: Blob = Blob(flash_words());
const fn flash_words() -> [u32; FLASH_BLOB_BYTES / 4] {
    let mut out = [0u32; FLASH_BLOB_BYTES / 4];
    let mut i = 0;
    while i < out.len() {
        out[i] = (i as u32).wrapping_mul(0x9E37_79B9) | 1;
        i += 1;
    }
    out
}

fn call(addr: usize, arg: u32) -> u32 {
    let f: extern "C" fn(u32) -> u32 = unsafe { core::mem::transmute(addr) };
    f(arg)
}

fn publish_code(write: usize, len: usize) {
    unsafe {
        let autoload = Cache_Suspend_DCache_Autoload();
        rom_Cache_WriteBack_Addr(write as u32, len as u32);
        Cache_Resume_DCache_Autoload(autoload);
        Cache_Invalidate_Addr((write - EXT_DBUS + EXT_IBUS) as u32, len as u32);
    }
}

fn run_code(name: &str, size: usize, exec: usize, expect_adds: u32) {
    unsafe { Cache_Invalidate_ICache_All() };
    let mut out = 0;
    let cold = time(|| out = call(exec, 0));
    let reps = (2_000_000 / expect_adds).clamp(4, 2000);
    let warm = time(|| {
        for _ in 0..reps {
            core::hint::black_box(call(exec, 0));
        }
    });
    let per_insn_warm = ns_x100(warm, reps as u64 * expect_adds as u64);
    let per_insn_cold = ns_x100(cold, expect_adds as u64);
    println!(
        "{TAG} code {name:>6} {size:>7} B @ {exec:#010x}: result {out} (want {expect_adds}); cold {}.{:02} ns/insn ({} us), warm {}.{:02} ns/insn, {} MB/s fetched warm",
        fmt2(per_insn_cold).0,
        fmt2(per_insn_cold).1,
        cold / CPU_MHZ,
        fmt2(per_insn_warm).0,
        fmt2(per_insn_warm).1,
        mb_per_s(reps as u64 * size as u64, warm)
    );
}

fn code_fetch(psram_area: usize) {
    for &size in &[8192usize, 24576, 65536, 262144] {
        let blob: &[u8] = match size {
            8192 => &FLASH_CODE_8K.0,
            24576 => &FLASH_CODE_24K.0,
            65536 => &FLASH_CODE_64K.0,
            _ => &FLASH_CODE_256K.0,
        };
        let adds = addis(size);
        // Flash (its own I-bus address: it is linked into `.text`).
        run_code("flash", size, blob.as_ptr() as usize, adds);
        // Internal SRAM through SRAM1's I-bus alias.
        if size <= 65536 {
            let mut v: Vec<u8> = Vec::with_capacity(size + 64);
            let pad = (64 - (v.as_ptr() as usize % 64)) % 64;
            v.resize(pad, 0);
            v.extend_from_slice(blob);
            let w = v.as_ptr() as usize + pad;
            run_code("iram", size, w + SRAM1_IBUS_ALIAS, adds);
        }
        // PSRAM through its I-bus twin.
        let dst = psram_area as *mut u8;
        unsafe { core::ptr::copy_nonoverlapping(blob.as_ptr(), dst, size) };
        publish_code(psram_area, size);
        run_code("psram", size, psram_area - EXT_DBUS + EXT_IBUS, adds);
    }
    // Coherence: overwrite the PSRAM copy with a SHORTER function at the same
    // address, publish, and call — a stale I-cache line would return the old
    // count.
    let short = code_blob::<64>();
    unsafe { core::ptr::copy_nonoverlapping(short.as_ptr(), psram_area as *mut u8, 64) };
    publish_code(psram_area, 64);
    let r = call(psram_area - EXT_DBUS + EXT_IBUS, 0);
    println!("{TAG} code psram republish: result {r} (want {})", addis(64));
    // …and without the publish, to see what a missed sync looks like.
    let short2 = code_blob::<32>();
    unsafe { core::ptr::copy_nonoverlapping(short2.as_ptr(), psram_area as *mut u8, 32) };
    let r = call(psram_area - EXT_DBUS + EXT_IBUS, 0);
    println!("{TAG} code psram NO publish: result {r} (fresh would be {}, stale {})", addis(32), addis(64));
}

// ── Flash writes ────────────────────────────────────────────────────────

/// A sector in the 8 MB layout's slack (`0x790000..0x800000`), outside every
/// partition, so the board's `lpfs` is untouched.
const SPARE_SECTOR: u32 = 0x7F_0000;

fn flash_write_hazard(fs: &mut esp_storage::FlashStorage<'static>, area: usize) {
    use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
    let n = 64 * 1024;
    let p = area as *mut u32;
    // Dirty 64 KB of PSRAM in the cache (more than the cache holds, so some
    // lines are dirty in the cache and some already written back).
    for i in 0..n / 4 {
        unsafe { p.add(i).write_volatile(0xC0DE_0000 ^ i as u32) };
    }
    let mut src_internal = alloc::vec![0u8; 4096];
    for (i, b) in src_internal.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(7);
    }
    let check_psram = |what: &str| {
        let mut bad = 0;
        for i in 0..n / 4 {
            if unsafe { p.add(i).read_volatile() } != 0xC0DE_0000 ^ i as u32 {
                bad += 1;
            }
        }
        println!("{TAG} flash hazard: psram after {what}: {bad} bad words of {}", n / 4);
    };
    let t = time(|| fs.erase(SPARE_SECTOR, SPARE_SECTOR + 4096).unwrap());
    println!("{TAG} flash hazard: erase 4 KB {} us", t / CPU_MHZ);
    check_psram("erase");
    let t = time(|| fs.write(SPARE_SECTOR, &src_internal).unwrap());
    println!("{TAG} flash hazard: program 4 KB from internal {} us", t / CPU_MHZ);
    check_psram("program from internal");
    let mut back = alloc::vec![0u8; 4096];
    fs.read(SPARE_SECTOR, &mut back).unwrap();
    println!("{TAG} flash hazard: readback (internal source) ok={}", back == src_internal);

    // Program straight from a PSRAM buffer (esp-storage hands an aligned
    // buffer to the ROM as is).
    let src_psram = unsafe { core::slice::from_raw_parts_mut((area + n) as *mut u8, 4096) };
    for (i, b) in src_psram.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(13).wrapping_add(1);
    }
    fs.erase(SPARE_SECTOR, SPARE_SECTOR + 4096).unwrap();
    let r = fs.write(SPARE_SECTOR, src_psram);
    fs.read(SPARE_SECTOR, &mut back).unwrap();
    let mismatches = back.iter().zip(src_psram.iter()).filter(|(a, b)| a != b).count();
    println!("{TAG} flash hazard: program from PSRAM source: {r:?}, {mismatches} bytes differ of 4096");
    check_psram("program from psram");
    // And the same with the source line cold (not in the cache).
    fs.erase(SPARE_SECTOR, SPARE_SECTOR + 4096).unwrap();
    cold_dcache();
    let r = fs.write(SPARE_SECTOR, src_psram);
    fs.read(SPARE_SECTOR, &mut back).unwrap();
    let mismatches = back.iter().zip(src_psram.iter()).filter(|(a, b)| a != b).count();
    println!("{TAG} flash hazard: program from cold PSRAM source: {r:?}, {mismatches} bytes differ");
    check_psram("cold program");
    fs.erase(SPARE_SECTOR, SPARE_SECTOR + 4096).unwrap();
}

// ── JIT ─────────────────────────────────────────────────────────────────

fn jit_corpus(label: &str, external_first_min: usize) {
    esp_alloc::EXTERNAL_FIRST_MIN.store(external_first_min, Ordering::Relaxed);
    let mut table = BuiltinTable::new();
    table.populate();
    let engine = NativeJitEngine::new(
        Arc::new(table),
        NativeCompileOptions {
            float_mode: FloatMode::Q32,
            ..Default::default()
        },
    );
    let (mut pass, mut fail) = (0, 0);
    let mut compile_total = 0u64;
    let mut run_total = 0u64;
    for case in CASES {
        let (ir, sig) = (case.build)();
        let t0 = cycle_counter::read();
        let module = match engine.compile(&ir, &sig) {
            Ok(m) => m,
            Err(e) => {
                println!("{TAG} jit {label} FAIL {} compile: {e}", case.name);
                fail += 1;
                continue;
            }
        };
        compile_total += cycle_counter::read().wrapping_sub(t0) as u64;
        for inv in case.invocations {
            let mut inst = module.instantiate().unwrap();
            let t1 = cycle_counter::read();
            let got = inst.call_q32(case.entry, inv.args);
            run_total += cycle_counter::read().wrapping_sub(t1) as u64;
            match got {
                Ok(v) if v.as_slice() == inv.golden => pass += 1,
                other => {
                    println!("{TAG} jit {label} FAIL {} args={:?} got {:?}", case.name, inv.args, other);
                    fail += 1;
                }
            }
        }
    }
    println!(
        "{TAG} jit {label}: passed {pass} failed {fail}; compile {} us total, calls {} us total; heap internal free {} external free {}",
        compile_total / CPU_MHZ,
        run_total / CPU_MHZ,
        esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::Internal.into()),
        esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::External.into()),
    );
}

// ── Shared helpers for the LED stress and the core-1 test ───────────────

/// Bring PSRAM up (the bench's config) and return its D-bus base and size.
pub fn init(psram_peri: esp_hal::peripherals::PSRAM<'static>) -> (usize, usize) {
    let psram = Psram::new(
        psram_peri,
        PsramConfig {
            mode: PsramMode::OctalSpi,
            size: PsramSize::AutoDetect,
            ram_frequency: mhz(),
            ..Default::default()
        },
    );
    let (b, s) = psram.raw_parts();
    core::mem::forget(psram);
    (b as usize, s)
}

/// A cheap PRNG step (xorshift32).
#[inline(always)]
fn xs(mut x: u32) -> u32 {
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

/// `n` random read-modify-writes across `words` words of PSRAM at `base`:
/// every one a likely miss, half of them evicting a dirty line.
#[inline(never)]
fn psram_thrash(base: usize, words: u32, n: u32, seed: &mut u32) {
    let p = base as *mut u32;
    for _ in 0..n {
        *seed = xs(*seed);
        let i = (*seed & (words - 1)) as usize;
        unsafe { p.add(i).write_volatile(p.add(i).read_volatile().wrapping_add(1)) };
    }
}

fn flash_code_64k() -> (usize, u32) {
    (FLASH_CODE_64K.0.as_ptr() as usize, addis(65536))
}

fn flash_code_256k() -> (usize, u32) {
    (FLASH_CODE_256K.0.as_ptr() as usize, addis(262144))
}

// ── LED output under PSRAM load (called from the loopback harness) ──────

#[cfg(feature = "test_loopback")]
#[derive(Clone, Copy, Debug)]
enum Load {
    None,
    PsramThrash,
    FlashCode,
    FlashErase,
    Core1Thrash,
}

#[cfg(feature = "test_loopback")]
pub fn led_stress(
    psram_peri: esp_hal::peripherals::PSRAM<'static>,
    flash_peri: esp_hal::peripherals::FLASH<'static>,
    cpu_peri: esp_hal::peripherals::CPU_CTRL<'static>,
) {
    use crate::output::rmt::shared_driver::DRIVER;
    use embedded_storage::nor_flash::NorFlash;
    const LEDS: usize = 155; // ×4 channels = 620 pixels
    const N: usize = LEDS * 3;
    const FRAMES: usize = 40;

    let (base, size) = init(psram_peri);
    println!("{TAG} led stress: psram {base:#010x} {size} B, {LEDS} LEDs × 4 channels, {FRAMES} frames per row");
    let mut fs = esp_storage::FlashStorage::new(flash_peri);
    core1_start(cpu_peri, base);

    let mut internal = alloc::boxed::Box::new([[0u8; N]; 4]);
    let ext: &mut [[u8; N]; 4] = unsafe { &mut *((base + 6 * 1024 * 1024) as *mut [[u8; N]; 4]) };
    for ch in 0..4 {
        for j in 0..N {
            let v = ((j * 7 + ch * 97 + 3) % 251) as u8;
            internal[ch][j] = v;
            ext[ch][j] = v;
        }
    }
    let thrash_base = base;
    let mut seed = 0x1234_5678u32;

    // The copy the "render into PSRAM, copy into an internal output buffer"
    // rule would add, per frame.
    cold_dcache();
    let c = time(|| unsafe {
        core::ptr::copy_nonoverlapping(ext.as_ptr() as *const u8, internal.as_mut_ptr() as *mut u8, 4 * N)
    });
    println!("{TAG} led stress: copy {} B PSRAM→internal (cold) {} us", 4 * N, c / CPU_MHZ);

    for (src_name, src_ptr) in [("internal", internal.as_ptr() as usize), ("psram", ext.as_ptr() as usize)] {
        let src: &[[u8; N]; 4] = unsafe { &*(src_ptr as *const [[u8; N]; 4]) };
        for load in [Load::None, Load::PsramThrash, Load::FlashCode, Load::Core1Thrash, Load::FlashErase] {
            let before: [_; 4] = core::array::from_fn(|ch| DRIVER.stats(ch as u8));
            if matches!(load, Load::Core1Thrash) {
                CORE1_CMD.store(4, Ordering::Release);
            }
            let rows = if matches!(load, Load::FlashErase) { 10 } else { FRAMES };
            let t0 = now_us();
            let mut spins = 0u32;
            for _ in 0..rows {
                let frames = [(0u8, &src[0][..]), (1, &src[1][..]), (2, &src[2][..]), (3, &src[3][..])];
                let mut erased = false;
                let started = esp_hal::time::Instant::now();
                let _ = DRIVER.send_blocking_all(&frames, || {
                    spins += 1;
                    match load {
                        Load::None | Load::Core1Thrash => {}
                        Load::PsramThrash => psram_thrash(thrash_base, 1 << 20, 64, &mut seed),
                        Load::FlashCode => {
                            let (a, _) = flash_code_64k();
                            core::hint::black_box(call(a, 0));
                        }
                        Load::FlashErase => {
                            // Once a frame, ~1 ms in (well past the prefill).
                            if !erased && started.elapsed() > esp_hal::time::Duration::from_micros(1000) {
                                erased = true;
                                let _ = fs.erase(SPARE_SECTOR, SPARE_SECTOR + 4096);
                            }
                        }
                    }
                    if started.elapsed() > esp_hal::time::Duration::from_millis(200) {
                        for ch in 0..4 {
                            DRIVER.abort(ch);
                        }
                    }
                });
            }
            let elapsed = now_us() - t0;
            CORE1_CMD.store(0, Ordering::Release);
            let mut line = alloc::string::String::new();
            use core::fmt::Write;
            for ch in 0..4u8 {
                let a = DRIVER.stats(ch);
                let b = &before[ch as usize];
                let eh: [u32; 9] = core::array::from_fn(|i| a.entry_delay_hist[i] - b.entry_delay_hist[i]);
                let lh: [u32; 9] = core::array::from_fn(|i| a.lag_hist[i] - b.lag_hist[i]);
                let _ = write!(
                    line,
                    " ch{ch}: frames {} trips {} errors {} trip_bit {} entry {:?} lag {:?};",
                    a.frames - b.frames,
                    a.guard_trips - b.guard_trips,
                    a.errors - b.errors,
                    a.trip_bits_last,
                    eh,
                    lh
                );
            }
            println!(
                "{TAG} led {src_name:>8} {:<12} {} us/frame spins {}:{line}",
                alloc::format!("{load:?}"),
                elapsed / rows as u64,
                spins
            );
        }
    }
    println!("{TAG} led stress done");
}

// ── Core 1: contention across the MSPI bus ──────────────────────────────

static CORE1_CMD: AtomicU32 = AtomicU32::new(0);
static CORE1_COUNT: AtomicU32 = AtomicU32::new(0);
static CORE1_MAXGAP: AtomicU32 = AtomicU32::new(0);
static CORE1_PSRAM: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static CORE1_FLASH_CODE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static mut CORE1_STACK: esp_hal::system::Stack<8192> = esp_hal::system::Stack::new();

fn core1_start(cpu_peri: esp_hal::peripherals::CPU_CTRL<'static>, psram_base: usize) {
    CORE1_PSRAM.store(psram_base + 7 * 1024 * 1024, Ordering::Release);
    CORE1_FLASH_CODE.store(flash_code_64k().0, Ordering::Release);
    let mut cc = esp_hal::system::CpuControl::new(cpu_peri);
    let guard = cc
        .start_app_core(unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) }, || core1_main())
        .unwrap();
    core::mem::forget(guard);
    core::mem::forget(cc);
}

/// Core 1's loop. In IRAM, its counters in internal SRAM: in mode 0 and 1 it
/// touches neither flash nor PSRAM.
///
/// - 0: idle (spin on the command word)
/// - 1: count loop iterations and the longest gap between two `CCOUNT` reads
/// - 2: stream code from flash (the 64 KB blob), count calls
/// - 3: sequential reads of 512 KB of PSRAM, count KB
/// - 4: random PSRAM read-modify-writes over 1 MB, count thousands
#[esp_hal::ram]
fn core1_main() -> ! {
    let mut seed = 0x8765_4321u32;
    loop {
        let cmd = CORE1_CMD.load(Ordering::Acquire);
        match cmd {
            1 => {
                let mut last = cycle_counter::read();
                let mut n = 0u32;
                let mut max = 0u32;
                while CORE1_CMD.load(Ordering::Relaxed) == 1 {
                    let now = cycle_counter::read();
                    let gap = now.wrapping_sub(last);
                    if gap > max {
                        max = gap;
                    }
                    last = now;
                    n = n.wrapping_add(1);
                    if n & 255 == 0 {
                        CORE1_COUNT.store(n, Ordering::Relaxed);
                        CORE1_MAXGAP.store(max, Ordering::Relaxed);
                    }
                }
            }
            2 => {
                let f: extern "C" fn(u32) -> u32 =
                    unsafe { core::mem::transmute(CORE1_FLASH_CODE.load(Ordering::Relaxed)) };
                let mut n = 0u32;
                while CORE1_CMD.load(Ordering::Relaxed) == 2 {
                    f(0);
                    n += 1;
                    CORE1_COUNT.store(n, Ordering::Relaxed);
                }
            }
            3 => {
                let p = CORE1_PSRAM.load(Ordering::Relaxed) as *const u32;
                let mut kb = 0u32;
                let mut off = 0usize;
                while CORE1_CMD.load(Ordering::Relaxed) == 3 {
                    let mut s = 0u32;
                    for i in 0..256 {
                        s = s.wrapping_add(unsafe { p.add(off + i).read_volatile() });
                    }
                    core::hint::black_box(s);
                    off = (off + 256) & (128 * 1024 - 1);
                    kb += 1;
                    CORE1_COUNT.store(kb, Ordering::Relaxed);
                }
            }
            4 => {
                let p = CORE1_PSRAM.load(Ordering::Relaxed) as *mut u32;
                let mut n = 0u32;
                while CORE1_CMD.load(Ordering::Relaxed) == 4 {
                    for _ in 0..1000 {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        let i = (seed & ((1 << 18) - 1)) as usize;
                        unsafe { p.add(i).write_volatile(p.add(i).read_volatile().wrapping_add(1)) };
                    }
                    n += 1;
                    CORE1_COUNT.store(n, Ordering::Relaxed);
                }
            }
            _ => {}
        }
    }
}

/// Core 0's side: what each core-0 load does to each core-1 workload, and
/// what core-1 PSRAM traffic does to core 0's code fetch.
fn core1_contention(cpu_peri: esp_hal::peripherals::CPU_CTRL<'static>, base: usize, fs: &mut esp_storage::FlashStorage<'static>) {
    use embedded_storage::nor_flash::NorFlash;
    core1_start(cpu_peri, base);
    let wait_ms = |ms: u64| {
        let t = now_us() + ms * 1000;
        while now_us() < t {}
    };
    let mut seed = 0xACE1u32;
    // Core-1 workloads × core-0 loads.
    for (c1, c1_name) in [(1u32, "iram-loop"), (2, "flash-code"), (3, "psram-seq"), (4, "psram-rmw")] {
        for load in ["idle", "psram-rmw", "flash-code", "flash-erase"] {
            if c1 != 1 && load == "flash-erase" {
                continue; // run last, separately
            }
            CORE1_COUNT.store(0, Ordering::Relaxed);
            CORE1_MAXGAP.store(0, Ordering::Relaxed);
            CORE1_CMD.store(c1, Ordering::Release);
            wait_ms(5);
            let n0 = CORE1_COUNT.load(Ordering::Relaxed);
            let t0 = now_us();
            let mut core0_units = 0u64;
            match load {
                "idle" => wait_ms(100),
                "psram-rmw" => {
                    while now_us() - t0 < 100_000 {
                        psram_thrash(base, 1 << 20, 1000, &mut seed);
                        core0_units += 1;
                    }
                }
                "flash-code" => {
                    let (a, _) = flash_code_256k();
                    while now_us() - t0 < 100_000 {
                        core::hint::black_box(call(a, 0));
                        core0_units += 1;
                    }
                }
                _ => {
                    CORE1_MAXGAP.store(0, Ordering::Relaxed);
                    let _ = fs.erase(SPARE_SECTOR, SPARE_SECTOR + 4096);
                }
            }
            let dt = now_us() - t0;
            let n1 = CORE1_COUNT.load(Ordering::Relaxed).wrapping_sub(n0);
            let gap = CORE1_MAXGAP.load(Ordering::Relaxed);
            CORE1_CMD.store(0, Ordering::Release);
            wait_ms(2);
            println!(
                "{TAG} core1 {c1_name:<10} while core0 {load:<11}: {} per ms (window {dt} us){}; core0 {} units",
                n1 as u64 * 1000 / dt.max(1),
                if c1 == 1 { alloc::format!(", max gap {} cycles", gap) } else { alloc::string::String::new() },
                core0_units
            );
        }
    }
    // Core 0's code fetch with core 1 thrashing PSRAM.
    for (c1, c1_name) in [(0u32, "idle"), (3, "psram-seq"), (4, "psram-rmw"), (2, "flash-code")] {
        CORE1_CMD.store(c1, Ordering::Release);
        wait_ms(5);
        let (fa, fadds) = flash_code_256k();
        let cf = time(|| {
            for _ in 0..4 {
                core::hint::black_box(call(fa, 0));
            }
        });
        // PSRAM copy of the 64 KB blob (streams: larger than the I-cache).
        let pc = base + 5 * 1024 * 1024;
        unsafe { core::ptr::copy_nonoverlapping(FLASH_CODE_64K.0.as_ptr(), pc as *mut u8, 65536) };
        publish_code(pc, 65536);
        let cp = time(|| {
            for _ in 0..16 {
                core::hint::black_box(call(pc - EXT_DBUS + EXT_IBUS, 0));
            }
        });
        let mut d = alloc::vec![0u32; 4096];
        let ci = time(|| {
            for _ in 0..64 {
                core::hint::black_box(seq_read(d.as_mut_ptr(), 4096));
            }
        });
        CORE1_CMD.store(0, Ordering::Release);
        wait_ms(2);
        println!(
            "{TAG} core0 with core1 {c1_name:<10}: flash code {}.{:02} ns/insn, psram code {}.{:02} ns/insn, sram reads {} MB/s",
            fmt2(ns_x100(cf, 4 * fadds as u64)).0,
            fmt2(ns_x100(cf, 4 * fadds as u64)).1,
            fmt2(ns_x100(cp, 16 * addis(65536) as u64)).0,
            fmt2(ns_x100(cp, 16 * addis(65536) as u64)).1,
            mb_per_s(64 * 16384, ci)
        );
    }
    // Last, because it may not survive: core 1 on flash code, then on PSRAM,
    // while core 0 erases a sector with only its own interrupts masked.
    for (c1, c1_name) in [(4u32, "psram-rmw"), (3, "psram-seq"), (2, "flash-code")] {
        println!("{TAG} core1 {c1_name} during a core-0 erase: starting");
        CORE1_COUNT.store(0, Ordering::Relaxed);
        CORE1_CMD.store(c1, Ordering::Release);
        wait_ms(5);
        let n0 = CORE1_COUNT.load(Ordering::Relaxed);
        let t = time(|| {
            let _ = fs.erase(SPARE_SECTOR, SPARE_SECTOR + 4096);
        });
        let n1 = CORE1_COUNT.load(Ordering::Relaxed).wrapping_sub(n0);
        wait_ms(5);
        let n2 = CORE1_COUNT.load(Ordering::Relaxed);
        CORE1_CMD.store(0, Ordering::Release);
        println!(
            "{TAG} core1 {c1_name} during a core-0 erase ({} us): {} units during it ({} per ms), still running after: {}",
            t / CPU_MHZ,
            n1,
            n1 as u64 * 1000 * CPU_MHZ / t.max(1),
            n2 != n0 + n1
        );
    }
}
