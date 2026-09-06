//! Silicon probe: can the classic's JIT code region live in **SRAM0**?
//!
//! `lpvm_native::codemem_esp32` places JIT code in SRAM1 today, written
//! through SRAM1's word-mirrored D-bus view, which spends byte-addressable
//! heap-grade memory on something SRAM0 could hold for free: SRAM0's IRAM
//! (`0x4008_0000..0x400A_0000`, 128 KiB) carries only `.vectors` + `.rwtext`
//! and has no D-bus view at all — its word-only instruction bus is exactly
//! what a code installer needs and nothing a heap can use. Moving the region
//! there rests on facts nobody has measured on this chip with *this*
//! toolchain, and this rig measures them, one `[SRAM0]` line each:
//!
//! 1. `word_rw` — aligned 32-bit stores across the candidate region land
//!    and read back through the same addresses.
//! 2. `byte_access` — a byte store/load there faults (`LoadStoreError`,
//!    EXCCAUSE 3). The fault resets the chip through the harness's
//!    print-and-reset panic path, so the verdict is reported by the *next*
//!    boot, from a marker parked in RTC fast RAM — the probe cannot catch
//!    the exception without displacing the runtime's handler.
//! 3. `exec` — a function written word by word at the region base executes
//!    when called from the PRO core, and returns the constant it was
//!    written with.
//! 4. `barrier` — whether execution needs a barrier after the writes: the
//!    constant is rewritten and the function re-called 1,000 times under
//!    each of three disciplines (no barrier; the `SeqCst` fence
//!    `DeviceCodeSink` already issues; fence + `isync`), counting stale
//!    results.
//! 5. `app_core` — skipped by design: the JIT executes on the PRO core only.
//! 6. `rwtext_end` — what esp-hal's `_rwtext_len` linker symbol actually
//!    means, so the firmware's boot-time "region sits above `.rwtext`"
//!    assert can be written against the right expression.
//!
//! The capture ends with `[SRAM0] END-SRAM0` (`just fwtest-sram0-esp32v3
//! <port>`). The rig asserts nothing — the capture is the result, read
//! against the plan in `2026-09-05-1635-classic-ram-track-b-jit-sram0`.
//!
//! Nothing here touches `codemem_esp32` or the emulator: this is the
//! measurement the placement change is built on, not the change.

use core::sync::atomic::{Ordering, fence};

use esp_hal::ram;

/// The candidate region: 64 KiB at 32 KiB into SRAM0's IRAM, leaving
/// `.rwtext` room to grow to `0x4008_8000` below and 32 KiB spare above.
const REGION_BASE: u32 = 0x4008_8000;
const REGION_LEN: u32 = 0x1_0000;
/// esp-hal's `iram_seg` (`ld/esp32/memory.x`): `.vectors` takes the first
/// 1 KiB of SRAM0, `.rwtext` starts here.
const IRAM_ORIGIN: u32 = 0x4008_0400;
/// End (exclusive) of SRAM0's instruction bus.
const IRAM_END: u32 = 0x400A_0000;

/// Marker parked in RTC fast RAM across the byte-access fault's reset.
/// `persistent`: skips load-time initialisation, so the previous boot's
/// value survives a software reset (the same discipline as the recovery
/// ledger in `recovery::esp32v3_recovery_backend`).
#[ram(unstable(rtc_fast, persistent))]
static mut BYTE_PROBE_STAGE: u32 = 0;
/// `BYTE_PROBE_STAGE` value meaning "the byte access was attempted and the
/// boot did not get to clear it" — i.e. it faulted.
const BYTE_PROBE_ARMED: u32 = 0xB17E_5A5A;

/// The immediate the template function returns. Must fit `movi`'s signed
/// 12-bit immediate (−2048..=2047) so the compiled template is exactly the
/// three-instruction shape [`encode`] reproduces.
const TEMPLATE_IMM: u32 = 0x5A5;

/// The function the probe installs, as rustc compiles it for this target —
/// the source of truth for the encoding table in [`encode`]. Its bytes are
/// read back from flash at runtime and compared against `encode(TEMPLATE_IMM)`
/// (`[SRAM0] encoding`), so a compiler that emits a different shape is
/// reported rather than silently probing the wrong instruction.
#[inline(never)]
extern "C" fn template() -> u32 {
    TEMPLATE_IMM
}

/// `entry a1, 32; movi a2, imm12; retw.n` as two little-endian words.
///
/// Taken from `xtensa-esp32-elf-objdump -d` of [`template`] in this crate's
/// `release-esp32v3` build, not hand-assembled:
///
/// ```text
/// 36 41 00    entry   a1, 32
/// 22 a5 a5    movi    a2, 0x5a5
/// 1d f0       retw.n
/// ```
///
/// `movi` is RRI8: `op0=2` in bits 3:0, `t` (the destination, a2) in 7:4,
/// `imm12[11:8]` in 11:8, `r=0xA` in 15:12, `imm12[7:0]` in 23:16 — so the
/// rewritable constant straddles the two words (byte 4 carries its high
/// nibble, byte 5 its low byte).
fn encode(imm12: u32) -> [u32; 2] {
    let b = [
        0x36,
        0x41,
        0x00,
        0x22,
        0xA0 | ((imm12 >> 8) & 0xF) as u8,
        (imm12 & 0xFF) as u8,
        0x1D,
        0xF0,
    ];
    [
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
    ]
}

/// How installed words are made visible to the fetch path.
#[derive(Clone, Copy)]
enum Barrier {
    /// Volatile stores only.
    None,
    /// `fence(SeqCst)` — what `DeviceCodeSink::sync` issues today (LLVM
    /// lowers it to `memw`).
    Fence,
    /// The fence plus an explicit `isync`, the ISA's instruction-fetch
    /// pipeline barrier.
    FenceIsync,
}

impl Barrier {
    fn apply(self) {
        match self {
            Barrier::None => {}
            Barrier::Fence => fence(Ordering::SeqCst),
            Barrier::FenceIsync => {
                fence(Ordering::SeqCst);
                // SAFETY: `isync` has no operands and no memory effects
                // beyond the pipeline flush it exists for.
                unsafe { core::arch::asm!("isync", options(nostack, preserves_flags)) };
            }
        }
    }

    fn name(self) -> &'static str {
        match self {
            Barrier::None => "none",
            Barrier::Fence => "fence",
            Barrier::FenceIsync => "fence+isync",
        }
    }
}

/// Write `words` at `base` with aligned volatile word stores — the only
/// access SRAM0 is expected to take.
fn write_words(base: u32, words: &[u32]) {
    for (i, w) in words.iter().enumerate() {
        // SAFETY: `base` is word-aligned and inside SRAM0's IRAM, which no
        // linker section reaches at these addresses (`.rwtext` ends far
        // below; `rwtext_end` reports exactly where).
        unsafe { ((base + 4 * i as u32) as *mut u32).write_volatile(*w) };
    }
}

/// Call the code installed at `base` as a `extern "C" fn() -> u32`.
fn call_at(base: u32) -> u32 {
    // SAFETY: `base` holds a complete windowed-ABI function installed by this
    // rig (`entry` … `retw.n`), which is exactly the shape a `call8` expects.
    let f: extern "C" fn() -> u32 = unsafe { core::mem::transmute(base as usize) };
    f()
}

/// Let UART0's TX FIFO clock out before something that may reset the chip.
/// Same dumb cycle count as the bare panic handler's `drain`, for the same
/// reason: this must not depend on peripherals whose state is in question.
fn drain_uart() {
    for _ in 0..2_000_000u32 {
        core::hint::spin_loop();
    }
}

/// The probe. Never returns; ends with `[SRAM0] END-SRAM0` and idles.
pub fn run() -> ! {
    esp_println::println!(
        "[SRAM0] probe: chip=esp32 region={REGION_BASE:#010x}+{REGION_LEN:#x} commit={} dirty={}",
        env!("LP_BUILD_COMMIT"),
        env!("LP_BUILD_DIRTY"),
    );
    let reset_reason = esp_hal::system::reset_reason();
    // SAFETY: single-threaded probe, the only reader/writer of the marker.
    let armed = unsafe { core::ptr::read_volatile(core::ptr::addr_of!(BYTE_PROBE_STAGE)) };
    esp_println::println!(
        "[SRAM0] boot: reset_reason={reset_reason:?} byte_probe_marker={armed:#010x}"
    );

    // 6. `_rwtext_len` semantics and the `.rwtext` end.
    rwtext_end();

    // 1. Aligned word stores across the candidate region, and the spare
    //    IRAM above it up to the bus end, read back through the same
    //    addresses. Pattern is address-dependent so a mirrored/aliased word
    //    would read as a mismatch rather than a coincidence.
    word_rw("region", REGION_BASE, REGION_LEN);
    word_rw(
        "spare_above",
        REGION_BASE + REGION_LEN,
        IRAM_END - (REGION_BASE + REGION_LEN),
    );

    // The encoding table matches what rustc actually emitted for `template`.
    encoding_check();

    // 3. Word-written code executes at the region base.
    let image = encode(TEMPLATE_IMM);
    write_words(REGION_BASE, &image);
    Barrier::Fence.apply();
    let got = call_at(REGION_BASE);
    esp_println::println!(
        "[SRAM0] exec: called {REGION_BASE:#010x} on PRO core -> {got:#x} (expected {TEMPLATE_IMM:#x}) {}",
        if got == TEMPLATE_IMM { "PASS" } else { "FAIL" }
    );
    // And at the region's last word-aligned slot that fits the image, so the
    // whole 64 KiB is proven fetchable, not only its first bytes.
    let top = REGION_BASE + REGION_LEN - 8;
    write_words(top, &encode(0x321));
    Barrier::Fence.apply();
    let got_top = call_at(top);
    esp_println::println!(
        "[SRAM0] exec_top: called {top:#010x} -> {got_top:#x} (expected 0x321) {}",
        if got_top == 0x321 { "PASS" } else { "FAIL" }
    );

    // 4. Barrier need: rewrite the constant and re-call, 1,000 times per
    //    discipline. `stale` = the previous iteration's value came back
    //    (the fetch path served old bytes); `other` = anything else.
    for barrier in [Barrier::None, Barrier::Fence, Barrier::FenceIsync] {
        barrier_trial(barrier);
    }

    // 5. APP core: not measured, by design.
    esp_println::println!(
        "[SRAM0] app_core: SKIPPED — the JIT compiles and executes on the PRO core only; \
         the APP core runs the RMT ISR and never fetches JIT code"
    );

    // 2. Byte access — last, because it is expected to reset the chip.
    if armed == BYTE_PROBE_ARMED {
        // SAFETY: as above.
        unsafe { core::ptr::write_volatile(core::ptr::addr_of_mut!(BYTE_PROBE_STAGE), 0) };
        esp_println::println!(
            "[SRAM0] byte_access: FAULTED on the previous boot (this boot's reset_reason={reset_reason:?}; \
             the exception dump printed before this boot is the record) — SRAM0 is word-only"
        );
    } else {
        // SAFETY: as above.
        unsafe {
            core::ptr::write_volatile(core::ptr::addr_of_mut!(BYTE_PROBE_STAGE), BYTE_PROBE_ARMED)
        };
        esp_println::println!(
            "[SRAM0] byte_access: attempting s8i/l8ui at {REGION_BASE:#010x} — a LoadStoreError \
             (EXCCAUSE 3) reset after this line IS the expected result; the next boot reports it"
        );
        drain_uart();
        let p = REGION_BASE as *mut u8;
        // SAFETY: the address is inside the region this rig owns; the whole
        // point is to learn whether the bus accepts the access at all.
        unsafe { p.write_volatile(0xA5) };
        let v = unsafe { p.read_volatile() };
        // SAFETY: as above.
        unsafe { core::ptr::write_volatile(core::ptr::addr_of_mut!(BYTE_PROBE_STAGE), 0) };
        esp_println::println!(
            "[SRAM0] byte_access: SURVIVED store+load (read back {v:#04x}) — SRAM0 took a byte access; \
             the word-only model is WRONG for this chip revision"
        );
    }

    esp_println::println!("[SRAM0] END-SRAM0");
    loop {
        core::hint::spin_loop();
    }
}

/// Fact 6. esp-hal's `rwtext.x` defines `_rwtext_len = . - ORIGIN(RWTEXT)`
/// *inside* the `.rwtext.wifi` output section, and GNU ld resolves a symbol
/// assigned inside an output section as section-relative — so the symbol's
/// address is not the length. This prints the raw value next to both
/// readings so the host-side `readelf -S` can say which one is right.
fn rwtext_end() {
    unsafe extern "C" {
        static _rwtext_len: u32;
    }
    let raw = core::ptr::addr_of!(_rwtext_len) as u32;
    // Reading A: the symbol is a plain length.
    let end_if_len = IRAM_ORIGIN.wrapping_add(raw);
    // Reading B: ld made it section-relative to `.rwtext.wifi`, which starts
    // where `.rwtext` ends, so the value is `rwtext_end + rwtext_len` =
    // `ORIGIN + 2 * len`.
    let len_if_rel = raw.wrapping_sub(IRAM_ORIGIN) / 2;
    let end_if_rel = IRAM_ORIGIN + len_if_rel;
    esp_println::println!(
        "[SRAM0] rwtext_end: _rwtext_len={raw:#010x} iram_origin={IRAM_ORIGIN:#010x} | \
         if_plain_length: len={raw} end={end_if_len:#010x} | \
         if_section_relative: len={len_if_rel} end={end_if_rel:#010x} | \
         region_base={REGION_BASE:#010x} (compare against readelf -S .rwtext)"
    );
    // Where a `.rwtext`-placed function actually sits: a lower bound on the
    // section's end that needs no linker-symbol interpretation.
    #[ram]
    fn in_rwtext() -> u32 {
        core::hint::black_box(7)
    }
    let probe_fn = in_rwtext as *const () as u32;
    let _ = in_rwtext();
    esp_println::println!(
        "[SRAM0] rwtext_probe_fn: a #[ram] fn lives at {probe_fn:#010x} (in [{IRAM_ORIGIN:#010x}, end)) {}",
        if probe_fn >= IRAM_ORIGIN && probe_fn < end_if_rel {
            "consistent with section_relative"
        } else {
            "NOT inside the section_relative reading"
        }
    );
}

/// Fact 1. Write an address-derived pattern with aligned word stores, read
/// it back, count mismatches. Restores nothing — the memory is unowned.
fn word_rw(what: &str, base: u32, len: u32) {
    let words = len / 4;
    let pat = |a: u32| a.wrapping_mul(0x9E37_79B9) ^ 0xA5A5_5A5A;
    for i in 0..words {
        let a = base + 4 * i;
        // SAFETY: word-aligned, inside SRAM0's IRAM, unowned by any section.
        unsafe { (a as *mut u32).write_volatile(pat(a)) };
    }
    fence(Ordering::SeqCst);
    let mut bad = 0u32;
    let mut first_bad = 0u32;
    for i in 0..words {
        let a = base + 4 * i;
        // SAFETY: as above.
        let v = unsafe { (a as *const u32).read_volatile() };
        if v != pat(a) {
            if bad == 0 {
                first_bad = a;
            }
            bad += 1;
        }
    }
    if bad > 0 {
        esp_println::println!("[SRAM0] word_rw[{what}]: first mismatch at {first_bad:#010x}");
    }
    esp_println::println!(
        "[SRAM0] word_rw[{what}]: {base:#010x}+{len:#x} words={words} mismatches={bad} {}",
        if bad == 0 { "PASS" } else { "FAIL" }
    );
}

/// The [`encode`] table against the bytes rustc emitted for [`template`].
fn encoding_check() {
    let src = template as *const () as u32;
    // ⚠️ Word loads only. `.text` is flash mapped through the INSTRUCTION
    // bus (`0x400D_0000..`), and a byte load there is a `LoadStoreError`
    // (EXCCAUSE 3, EXCVADDR = `template`'s address) — measured on the first
    // run of this rig, which read the function byte by byte and reset the
    // chip before reaching the SRAM0 facts. Same bus rule as SRAM0 itself.
    let word_base = src & !3;
    let skew = (src & 3) as usize;
    let mut raw = [0u8; 12];
    for (i, chunk) in raw.chunks_exact_mut(4).enumerate() {
        // SAFETY: aligned word loads covering `template`'s 8 bytes (three
        // instructions) plus alignment slack, inside the mapped `.text`.
        let w = unsafe { ((word_base + 4 * i as u32) as *const u32).read_volatile() };
        chunk.copy_from_slice(&w.to_le_bytes());
    }
    let mut got = [0u8; 8];
    got.copy_from_slice(&raw[skew..skew + 8]);
    let want = encode(TEMPLATE_IMM);
    let mut want_bytes = [0u8; 8];
    want_bytes[..4].copy_from_slice(&want[0].to_le_bytes());
    want_bytes[4..].copy_from_slice(&want[1].to_le_bytes());
    let live = core::hint::black_box(template)();
    esp_println::println!(
        "[SRAM0] encoding: template@{src:#010x} bytes={got:02x?} table={want_bytes:02x?} template()={live:#x} {}",
        if got[..] == want_bytes[..] {
            "MATCH"
        } else {
            "MISMATCH — the barrier trial rewrites a table rustc did not emit"
        }
    );
}

/// Fact 4, one discipline.
fn barrier_trial(barrier: Barrier) {
    let mut stale = 0u32;
    let mut other = 0u32;
    let mut prev = TEMPLATE_IMM;
    for i in 0..1_000u32 {
        // Distinct from the previous value every iteration, and from the
        // template constant, so "stale" is unambiguous.
        let imm = (i + 1) & 0x7FF;
        write_words(REGION_BASE, &encode(imm));
        barrier.apply();
        let got = call_at(REGION_BASE);
        if got != imm {
            if got == prev {
                stale += 1;
            } else {
                other += 1;
            }
        }
        prev = imm;
    }
    esp_println::println!(
        "[SRAM0] barrier[{}]: iterations=1000 stale={stale} other={other} {}",
        barrier.name(),
        if stale == 0 && other == 0 {
            "PASS"
        } else {
            "MISMATCH"
        }
    );
}
