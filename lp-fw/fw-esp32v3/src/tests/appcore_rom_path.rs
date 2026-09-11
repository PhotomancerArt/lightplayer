//! Silicon discriminator: **does starting the APP core re-run the mask ROM's
//! unpack/bss tables over heap region 0?**
//!
//! The classic's mask ROM reset path runs on both cores, and its unpack and
//! bss tables carry per-entry flags. Read out of the vendored rev300 ROM ELF,
//! the `flag = 1` entries re-copy `.data_xtos_pro`
//! (`0x3ffe0440..0x3ffe0858`) and zero `.bss_xtos_pro`
//! (`0x3ffe0860..0x3ffe1320`) — 3,808 bytes that this firmware hands to
//! `esp_alloc` as **heap region 0, first, before `start_app_core_isr`**
//! (`main.rs::add_rom_pro_stack_region`). esp-alloc is first-fit in
//! registration order, so the boot residents live exactly there.
//!
//! Two readings of the APP core's start disagree, and only silicon decides:
//!
//! * **Reading A** — esp-hal's `appcpu_resetting` pulse
//!   (`DPORT.appcpu_ctrl_a`) puts the APP core through `_ResetVector`, the
//!   ROM re-runs the tables, and the firmware's live heap objects are
//!   rewritten. This is what `lp-emu-esp32v3` models (M4 P1, PR #692): the
//!   shipped image dies in `LpFs::read_file` reading address 8 about 30 k
//!   cycles after the release.
//! * **Reading B** — the APP core has been parked in ROM `main`'s
//!   `appcpu_boot_addr` poll since power-on; the pulse does not re-run the
//!   tables and only the `ctrl_d` write releases it. The desk board boots the
//!   same bytes dual-core and runs fine, which is the evidence for this
//!   reading.
//!
//! Under **both** readings ROM `main` on the APP core writes seven
//! `_xtos_set_exception_handler` pairs at `0x3ffe0448 + 4n` and
//! `0x3ffe0548 + 4n` (n ∈ {0, 2, 3, 8, 9, 28, 29}), so those words alone
//! discriminate nothing — the zeroed `.bss_xtos_pro` span does.
//!
//! ## What this rig does
//!
//! 1. Initialises the board through the product's own `init_board`, then
//!    registers the heap regions in the product's order — region 0 (the ROM
//!    PRO stack span) **first**, then the `dram_seg` arena, then the SRAM1
//!    tail. Nothing is reordered: the whole question is about that order.
//! 2. Allocates a [`CANARY_LEN`]-byte `Vec<u8>` filled with `0xA5`. First-fit
//!    across regions in registration order puts it at the head of region 0,
//!    on top of the span the ROM would rewrite. The address range is printed
//!    and checked against the span rather than assumed.
//! 3. Prints the four witness words before the start.
//! 4. Calls the product's real
//!    [`crate::output::rmt::shared_driver::start_app_core_isr`] and waits for
//!    the same `ISR_ON_APP_CORE` bind the product waits for.
//! 5. Re-reads the witness words and scans the canary for every byte that is
//!    no longer `0xA5`, printing contiguous rewritten ranges.
//!
//! The verdict line is computed from the scan:
//!
//! * `verdict=A` — bytes inside `.data_xtos_pro` or `.bss_xtos_pro` were
//!   rewritten: the ROM's tables ran on the APP core.
//! * `verdict=B` — the tables did not run. Either nothing at all changed
//!   (`reason=nothing_rewritten`, which is what the DOM-Z-102 reported on
//!   2026-09-11 — ROM `main` did not re-run either) or only the seven
//!   handler-pointer pairs did (`reason=handler_pairs_only`).
//! * `verdict=other` — the core never bound, the canary missed the span, or a
//!   pattern neither reading predicts. The `reason=` line says which.
//!
//! The capture ends with `[APPCORE-CANARY] done`
//! (`just fwtest-appcore-rom-path-esp32v3 <port>`). The rig asserts nothing:
//! the capture is the result. Nothing here touches the product boot path —
//! it *calls* it.

use alloc::vec::Vec;

use crate::board::esp32v3::init::init_board;
use crate::output::rmt::shared_driver;

/// The ROM PRO-stack span the firmware registers as heap region 0.
/// Restated from `lpvm_native::codemem_esp32::SRAM1_ROM_PRO_STACK_SPAN` at
/// runtime, never transcribed — see [`run`].
///
/// The span the ROM's `flag = 1` tables cover, from the vendored rev300 ROM
/// ELF (`docs/defects/2026-09-10-the-app-cores-rom-boot-rewrites-heap-region-0.md`).
const ROM_REWRITE_LO: u32 = 0x3ffe_0440;
/// Exclusive end of the ROM-rewrite span (`.bss_xtos_pro`'s end).
const ROM_REWRITE_HI: u32 = 0x3ffe_1320;
/// `.data_xtos_pro` — the unpack table's `flag = 1` range (re-copied under
/// Reading A).
const DATA_XTOS_PRO: (u32, u32) = (0x3ffe_0440, 0x3ffe_0858);
/// `.bss_xtos_pro` — the bss table's `flag = 1` range (zeroed under Reading
/// A). **This is the discriminator**: ROM `main`'s handler writes never reach
/// it.
const BSS_XTOS_PRO: (u32, u32) = (0x3ffe_0860, 0x3ffe_1320);

/// The two `_xtos_set_exception_handler` tables ROM `main` writes on whichever
/// core runs it. Both readings predict these.
const HANDLER_TABLES: [u32; 2] = [0x3ffe_0448, 0x3ffe_0548];
/// The seven handler indices ROM `main` sets (`table + 4n`).
const HANDLER_INDICES: [u32; 7] = [0, 2, 3, 8, 9, 28, 29];

/// The four witness words, read before and after the start.
///
/// `0x3ffe0448` and `0x3ffe0548` are the first handler-table entries
/// (`_xtos_c_wrapper_handler` = `0x40000de8` and `ets_fatal_exception_handler`
/// = `0x40006840` when the ROM writes them); `0x3ffe0860` is the first word of
/// `.bss_xtos_pro`; `0x3ffe09a8` sits inside the live-pointer block
/// (`0x3ffe08f0..0x3ffe09c4`) the emulator saw zeroed.
const WITNESS_WORDS: [u32; 4] = [0x3ffe_0448, 0x3ffe_0548, 0x3ffe_0860, 0x3ffe_09a8];

/// Canary length: 4,096 B, which from region 0's base
/// (`0x3ffe0440`) covers the whole `0x3ffe0440..0x3ffe1320` ROM-rewrite span
/// (3,808 B) with 288 B of margin, and fits inside region 0's 15,072 B with
/// room to spare.
///
/// Larger than the ~3,500 B the lab brief sketched, deliberately: 3,500 B from
/// the base stops at `0x3ffe1204` and leaves the last 284 B of
/// `.bss_xtos_pro` unwatched, which is exactly the evidence the verdict turns
/// on.
const CANARY_LEN: usize = 4096;

/// The fill byte. `0xA5` rather than `0x00` or `0xFF`: the ROM's bss table
/// writes zeros and its `.data_xtos_pro` copy includes a `0xff`-filled buffer,
/// so neither of those would be distinguishable from "untouched".
const FILL: u8 = 0xA5;

/// How many rewritten ranges are printed before the list is truncated. Under
/// Reading A the `.data_xtos_pro` copy alone produces dozens.
const MAX_RANGES_PRINTED: usize = 48;

/// Read a word the way a witness must: volatile, so nothing is folded away.
fn word(addr: u32) -> u32 {
    // SAFETY: a fixed, mapped SRAM1 address inside heap region 0; the read is
    // aligned and the region is RW data memory.
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

fn print_witnesses(tag: &str) {
    esp_println::println!(
        "[APPCORE-CANARY] {tag} w[{:#010x}]={:#010x} w[{:#010x}]={:#010x} \
         w[{:#010x}]={:#010x} w[{:#010x}]={:#010x}",
        WITNESS_WORDS[0],
        word(WITNESS_WORDS[0]),
        WITNESS_WORDS[1],
        word(WITNESS_WORDS[1]),
        WITNESS_WORDS[2],
        word(WITNESS_WORDS[2]),
        WITNESS_WORDS[3],
        word(WITNESS_WORDS[3]),
    );
}

/// Is `addr` one of the four bytes of a word ROM `main`'s
/// `_xtos_set_exception_handler` calls write?
fn in_handler_word(addr: u32) -> bool {
    HANDLER_TABLES.iter().any(|&table| {
        HANDLER_INDICES
            .iter()
            .any(|&n| addr.wrapping_sub(table + 4 * n) < 4)
    })
}

fn in_span(addr: u32, span: (u32, u32)) -> bool {
    addr >= span.0 && addr < span.1
}

/// The rig. Never returns: a harness that falls off the end would be parked
/// by the runtime with no output to say why.
pub fn run() -> ! {
    // The product's own init: `esp_hal::init` at `CpuClock::max()`, the FPU
    // arm, and — load-bearing for every line below — UART0 at 921,600, which
    // is what programs the baud divisor `esp-println` depends on and never
    // sets. Bound, not dropped.
    let (_sw_int, _timg0, uart0, _flash, _rmt, cpu_ctrl) = init_board();
    let _uart0 = uart0;

    esp_println::println!(
        "[APPCORE-CANARY] begin chip=esp32 commit={} dirty={} profile={}",
        env!("LP_BUILD_COMMIT"),
        env!("LP_BUILD_DIRTY"),
        env!("LP_BUILD_PROFILE"),
    );

    // Before anything is allocated on top of them: what the PRO core's own ROM
    // boot left behind.
    print_witnesses("pre_alloc");

    // The product's heap order, unchanged. Region 0 (the ROM PRO stack span)
    // FIRST — that ordering is the whole subject of the experiment — then the
    // `dram_seg` arena, then the SRAM1 tail. Region 3 (the ROM APP stack) is
    // the product's post-bind region and is deliberately not registered here:
    // it is above the canary and plays no part.
    let rom_pro_heap = crate::add_rom_pro_stack_region();
    esp_alloc::heap_allocator!(size: crate::HEAP_SIZE);
    let sram1_heap = crate::add_sram1_heap_region();
    let (region0_base, region0_len) = lpvm_native::codemem_esp32::SRAM1_ROM_PRO_STACK_SPAN;
    esp_println::println!(
        "[APPCORE-CANARY] heap regions: 0 {:#010x}+{rom_pro_heap} (ROM PRO stack), \
         1 .bss+{} (dram_seg arena), 2 +{sram1_heap} (SRAM1 tail)",
        region0_base,
        crate::HEAP_SIZE,
    );

    // First-fit across regions in registration order puts this at the head of
    // region 0 — on top of the span the ROM would rewrite. Verified below
    // rather than assumed.
    let mut canary: Vec<u8> = Vec::with_capacity(CANARY_LEN);
    canary.resize(CANARY_LEN, FILL);
    let base = canary.as_ptr() as u32;
    let end = base + CANARY_LEN as u32;
    let overlap_lo = base.max(ROM_REWRITE_LO);
    let overlap_hi = end.min(ROM_REWRITE_HI);
    let covered = overlap_hi.saturating_sub(overlap_lo);
    esp_println::println!(
        "[APPCORE-CANARY] canary {base:#010x}..{end:#010x} ({CANARY_LEN} B) fill={FILL:#04x} \
         region0={region0_base:#010x}..{:#010x} ({region0_len} B)",
        region0_base + region0_len,
    );
    esp_println::println!(
        "[APPCORE-CANARY] coverage rom_span={ROM_REWRITE_LO:#010x}..{ROM_REWRITE_HI:#010x} \
         ({} B) covered={covered} B overlaps={}",
        ROM_REWRITE_HI - ROM_REWRITE_LO,
        if covered > 0 { "yes" } else { "NO" },
    );
    if covered == 0 {
        esp_println::println!(
            "[APPCORE-CANARY] WARNING the canary does not overlap the ROM-rewrite span; \
             this run discriminates nothing"
        );
    }

    print_witnesses("pre_start");

    // The product's real call, on the product's real path.
    let started = esp_hal::time::Instant::now();
    let bound = shared_driver::start_app_core_isr(cpu_ctrl);
    let elapsed_us = started.elapsed().as_micros();
    esp_println::println!("[APPCORE-CANARY] start_app_core_isr bound={bound} wait_us={elapsed_us}");

    print_witnesses("post_start");

    // The scan. Byte-wise volatile: the writes came from the other core.
    let mut changed = 0usize;
    let mut ranges = 0usize;
    let mut in_data_xtos = 0usize;
    let mut in_bss_xtos = 0usize;
    let mut in_handler = 0usize;
    let mut outside = 0usize;
    let mut run_start: Option<usize> = None;
    let mut index = 0usize;
    while index <= CANARY_LEN {
        let differs = index < CANARY_LEN && {
            // SAFETY: inside the canary's own allocation.
            let byte = unsafe { core::ptr::read_volatile(canary.as_ptr().add(index)) };
            if byte != FILL {
                let addr = base + index as u32;
                changed += 1;
                if in_handler_word(addr) {
                    in_handler += 1;
                } else if in_span(addr, BSS_XTOS_PRO) {
                    in_bss_xtos += 1;
                } else if in_span(addr, DATA_XTOS_PRO) {
                    in_data_xtos += 1;
                } else {
                    outside += 1;
                }
                true
            } else {
                false
            }
        };
        match (differs, run_start) {
            (true, None) => run_start = Some(index),
            (false, Some(start)) => {
                ranges += 1;
                if ranges <= MAX_RANGES_PRINTED {
                    let len = index - start;
                    let mut first = [0u8; 8];
                    for (slot, offset) in first.iter_mut().zip(start..index) {
                        // SAFETY: inside the canary's own allocation.
                        *slot = unsafe { core::ptr::read_volatile(canary.as_ptr().add(offset)) };
                    }
                    let shown = len.min(8);
                    esp_println::println!(
                        "[APPCORE-CANARY] range {:#010x}..{:#010x} len={len} first={:02x} {:02x} \
                         {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} shown={shown}",
                        base + start as u32,
                        base + index as u32,
                        first[0],
                        first[1],
                        first[2],
                        first[3],
                        first[4],
                        first[5],
                        first[6],
                        first[7],
                    );
                }
                run_start = None;
            }
            _ => {}
        }
        index += 1;
    }
    if ranges > MAX_RANGES_PRINTED {
        esp_println::println!(
            "[APPCORE-CANARY] ranges truncated: {ranges} total, {MAX_RANGES_PRINTED} printed"
        );
    }

    esp_println::println!(
        "[APPCORE-CANARY] scan changed={changed} ranges={ranges} handler_words={in_handler} \
         data_xtos_pro={in_data_xtos} bss_xtos_pro={in_bss_xtos} outside={outside}"
    );

    // The verdict. `.bss_xtos_pro` is the discriminator: ROM `main`'s handler
    // writes cannot reach it, so a single rewritten byte there means the ROM's
    // tables ran on the APP core.
    //
    // ⚠️ Reading B's claim is "the tables did not re-run", NOT "the handler
    // pairs were rewritten". The first bench run (2026-09-11, DOM-Z-102)
    // reported `changed=0` — not even ROM `main`'s handler pairs — so "nothing
    // was rewritten" has to land on B rather than on `other`, which is what an
    // earlier arm here did. The `reason=` line keeps the two B sub-cases apart
    // instead of hiding the difference behind one token.
    let (verdict, reason) = if !bound {
        ("other", "not_bound: the APP core never reported its bind")
    } else if covered == 0 {
        (
            "other",
            "no_coverage: the canary missed the ROM-rewrite span",
        )
    } else if in_bss_xtos > 0 || in_data_xtos > 0 {
        (
            "A",
            "rom_tables_ran: the unpack and/or bss table rewrote live heap",
        )
    } else if changed == 0 {
        (
            "B",
            "nothing_rewritten: the APP core's start touched no byte of the span \
             — not even ROM main's exception-handler pairs, so ROM main did not \
             re-run on it either",
        )
    } else if changed == in_handler {
        (
            "B",
            "handler_pairs_only: ROM main re-ran and set its seven handler pairs, \
             but the unpack and bss tables did not",
        )
    } else {
        (
            "other",
            "unexpected_pattern: read the ranges above before believing either reading",
        )
    };
    esp_println::println!("[APPCORE-CANARY] verdict={verdict}");
    esp_println::println!("[APPCORE-CANARY] reason={reason}");
    esp_println::println!("[APPCORE-CANARY] done");

    // `canary` stays in scope — the loop never ends, so it is never dropped
    // and the span stays claimed for anything that reads the board afterwards.
    loop {
        core::hint::spin_loop();
    }
}
