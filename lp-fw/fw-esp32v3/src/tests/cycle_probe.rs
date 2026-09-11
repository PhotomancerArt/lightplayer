//! The `cycle-probe` payload's device half, on the classic ESP32 (LX6).
//!
//! The bracket, the repetition, the record and the kernels that are ordinary
//! Rust live in `fw_checks::checks::cycle_probe`, where they are `no_std` and
//! know nothing about this chip. What stays here is what cannot leave: the
//! clocks, and the kernels whose whole point is a chip fact. `fw-checks` is
//! **mirrored** — the C6's `src/tests/cycle_probe.rs` is another chip's file
//! and is not imported, only followed.
//!
//! Three kinds of chip fact keep a kernel here, and they are the C6's three:
//!
//! 1. **Where the code lives.** [`iram_loop`] and [`flash_loop`] are the same
//!    six instructions; the only difference between them is `#[esp_hal::ram]`,
//!    which is a linker section. That difference *is* the measurement.
//! 2. **An exact instruction count.** `insns` on a record has to be a fact,
//!    not an estimate, and the only way to know a loop's length exactly is to
//!    write the loop — which means an ISA. These are Xtensa inline assembly,
//!    counted by reading them; see [`alu_loop_body`] for why that count is a
//!    fact under rustc's assembler and what was tried first.
//! 3. **An address.** The MMIO polls read real peripheral registers.
//!
//! ## ⚠️ Recorded, never gated
//!
//! **No host gate reads a number this payload prints.** The counter below is
//! **CPU cycles at 240 MHz** — `CCOUNT`, the LX6's own free-running cycle
//! register; this part has no `mcycle` and no SYSTIMER. `lp-emu:esp32v3`
//! defines **`t1` only** (M5 ruling R1, director E1 answered `t1` only), and
//! `t1` counts one cycle per instruction, so there is no calibrated grade for
//! a cycle figure to be compared against and inventing one would be the exact
//! dishonesty the trust table exists to prevent. What this payload is FOR is
//! being the **input a future `t2` would calibrate from**: every kernel
//! isolates one term of a cycle model, and the transcript is the measurement
//! that model would have to reproduce. Until then the figures are recorded,
//! printed, and compared by a person.
//!
//! ## The two clocks
//!
//! `read_cycles` is **CCOUNT** at the CPU clock (240 MHz here, since the
//! harness entry point takes `CpuClock::max()`). `read_us` is
//! `esp_hal::time::Instant`, which on the classic is **TIMG0's LACT** counter
//! running off the 80 MHz APB clock through its own divider to 16 MHz
//! (`esp-hal-1.1.1/src/time.rs`, the `#[cfg(esp32)]` `implem`) — a different
//! counter off a different clock, not a rescaling of CCOUNT. That
//! independence is the point: the payload records both so that a
//! disagreement between them can be seen rather than argued about.
//!
//! ## What each assembly loop costs, per iteration
//!
//! Counted off the source, and that count is what reaches the record:
//!
//! | loop | instructions | the one that matters |
//! |---|---|---|
//! | `iram_loop` / `flash_loop` | 6 | none — this is the ALU floor |
//! | `muldiv/mul` | 5 | one `mull` |
//! | `muldiv/div` | 5 | one `quou` |
//! | `mmio_poll/*` | 3 | one `l32i` from a peripheral |
//!
//! `muldiv/mul` and `muldiv/div` are deliberately the same five instructions
//! apart from the one under test, so their difference is that instruction and
//! nothing else. Their operands change every iteration, so neither reports a
//! single operand pair's cost — but both are dependency chains, so what they
//! measure is *latency*, not throughput.

use fw_checks::checks::cycle_probe::{Clocks, Group, Kernel, PORTABLE_GROUPS, run_all};
use log::info;

/// The CPU clock the harness entry point configures (`CpuClock::max()` on
/// this part). Named here because it is what turns a CCOUNT delta into a
/// duration — and the record deliberately does not do that conversion.
const CPU_HZ: u32 = 240_000_000;

/// UART0 `status`, `0x3FF4_0000 + 0x01c`. The console's own register: on the
/// classic the host link IS UART0, so this is the APB read the firmware
/// actually spends its MMIO budget on, and the one a cost model most needs.
const UART0_STATUS: *const u32 = 0x3FF4_001C as *const u32;

/// TIMG0 `lactlo`, `0x3FF5_F000 + 0x078`. A second APB block, read the same
/// way, so that "the APB costs N" can be told apart from "UART0 costs N".
/// Read without a preceding `lactupdate` write on purpose — the value is
/// discarded, and what is being measured is the bus access.
const TIMG0_LACTLO: *const u32 = 0x3FF5_F078 as *const u32;

/// Iterations for the six-instruction ALU loops. ~1.5 M instructions, which
/// at any plausible cost is milliseconds — far above the bracket overhead and
/// far enough above the 16 MHz microsecond clock's resolution that the two
/// clocks can be compared to a fraction of a percent.
const ALU_ITERS: u32 = 250_000;

/// Iterations for the multiply chain. Same five-instruction shape as the
/// divide chain, so the two are directly comparable.
const MUL_ITERS: u32 = 250_000;

/// Iterations for the divide chain. Fewer, because `quou` is the most
/// expensive instruction here and the kernel should take about as long as the
/// others, not thirty times longer.
const DIV_ITERS: u32 = 40_000;

/// Iterations for the MMIO polls. One peripheral read each; sized on the
/// assumption that an APB access is tens of cycles, which is what this kernel
/// exists to find out — so generously rather than tuned to a number nobody
/// has measured.
const MMIO_ITERS: u32 = 40_000;

/// Six instructions per iteration: `addi`, `add`, `xor`, `slli`, `add`,
/// `bnez`. No memory operand of any kind, so the only thing between the
/// processor and this loop is instruction fetch — which is the difference the
/// IRAM/flash pair is built to expose.
///
/// **Nothing here may be transformed, and nothing is.** The C6's twin pins
/// its encoding with `.option norvc`; the Xtensa equivalent is GNU `as`'s
/// `.begin no-transform`, which rustc's assembler — LLVM's integrated one —
/// rejects outright ("region option must be a symbol"). It does not need it:
/// unlike GNU `as`, LLVM assembles the mnemonics as written and never
/// substitutes a narrow (`.n`) encoding or expands an operand, so what is
/// written IS what is emitted. `code_walk`'s size is checked off the built
/// ELF rather than assumed, which is the same check in a form that holds
/// whichever assembler ran.
///
/// `#[inline(always)]` so that both wrappers below hold *the same six
/// instructions*, not two compilations of one source.
#[inline(always)]
fn alu_loop_body(iters: u32) -> u32 {
    if iters == 0 {
        return 0;
    }
    let mut acc: u32 = 0x1234_5678;
    let mut n = iters;
    unsafe {
        core::arch::asm!(
            "2:",
            "addi {n}, {n}, -1",
            "add  {acc}, {acc}, {n}",
            "xor  {acc}, {acc}, {n}",
            "slli {t}, {acc}, 1",
            "add  {acc}, {acc}, {t}",
            "bnez {n}, 2b",
            n = inout(reg) n,
            acc = inout(reg) acc,
            t = out(reg) _,
            options(nostack, nomem),
        );
    }
    let _ = n;
    acc
}

/// The ALU floor with no flash in the fetch path. `#[esp_hal::ram]` already
/// implies `#[inline(never)]`, so this side carries no second attribute even
/// though [`flash_loop`] needs one.
#[esp_hal::ram]
fn iram_loop(iters: u32) -> u32 {
    alu_loop_body(iters)
}

/// The identical loop, left where the linker puts code by default — in the
/// flash-cache window. Every cycle by which this exceeds [`iram_loop`] is
/// fetch.
#[inline(never)]
fn flash_loop(iters: u32) -> u32 {
    alu_loop_body(iters)
}

/// Five instructions per iteration, one of them a `mull`, with operands that
/// change every iteration.
#[inline(never)]
fn mul_chain(iters: u32) -> u32 {
    if iters == 0 {
        return 0;
    }
    let mut acc: u32 = 0x1234_5679;
    let mut n = iters;
    unsafe {
        core::arch::asm!(
            "2:",
            "addi {n}, {n}, -1",
            "add  {acc}, {acc}, {x}",
            "mull {t}, {acc}, {d}",
            "xor  {acc}, {acc}, {t}",
            "bnez {n}, 2b",
            n = inout(reg) n,
            acc = inout(reg) acc,
            t = out(reg) _,
            x = in(reg) 0x9E37_79B1u32,
            d = in(reg) 0x0000_00ABu32,
            options(nostack, nomem),
        );
    }
    let _ = n;
    acc
}

/// The same five instructions with `quou` in place of `mull`. The divisor is
/// a non-zero constant, so nothing traps and nothing degenerates.
#[inline(never)]
fn div_chain(iters: u32) -> u32 {
    if iters == 0 {
        return 0;
    }
    let mut acc: u32 = 0x1234_5679;
    let mut n = iters;
    unsafe {
        core::arch::asm!(
            "2:",
            "addi {n}, {n}, -1",
            "add  {acc}, {acc}, {x}",
            "quou {t}, {acc}, {d}",
            "xor  {acc}, {acc}, {t}",
            "bnez {n}, 2b",
            n = inout(reg) n,
            acc = inout(reg) acc,
            t = out(reg) _,
            x = in(reg) 0x9E37_79B1u32,
            d = in(reg) 0x0000_00ABu32,
            options(nostack, nomem),
        );
    }
    let _ = n;
    acc
}

/// Three instructions per iteration, one of them a load from `addr`.
///
/// The value read is discarded rather than accumulated, and the kernel
/// returns its iteration count: a peripheral's status register says something
/// different on every read and on every machine, and a record field that
/// differed for that reason would look exactly like a model disagreeing. The
/// load cannot be optimised away regardless — it is written, by hand, inside
/// an `asm!` block.
#[inline(always)]
fn poll(addr: *const u32, iters: u32) -> u32 {
    if iters == 0 {
        return 0;
    }
    let mut n = iters;
    unsafe {
        core::arch::asm!(
            "2:",
            "addi {n}, {n}, -1",
            "l32i {t}, {p}, 0",
            "bnez {n}, 2b",
            n = inout(reg) n,
            t = out(reg) _,
            p = in(reg) addr,
            options(nostack, readonly),
        );
    }
    let _ = n;
    iters
}

/// Pairs of instructions in the code walk. `.rept` in the assembler, so the
/// count is the assembler's and not the optimiser's: 12,288 × 2 × 3 bytes =
/// exactly **72 KiB** of straight-line `.text`, since `add` and `xor` are
/// three-byte Xtensa encodings and LLVM emits what is written (see
/// [`alu_loop_body`]). Measured off the built ELF, not assumed.
const CODE_WALK_PAIRS: u32 = 12_288;

/// `code_walk` — 72 KiB of straight-line flash-resident code, walked once.
///
/// Sized to exceed a **32 KiB** instruction cache by more than 2×. On this
/// part 32 KiB is the larger of the two per-core sizes the DPORT cache
/// control selects between, so the walk exceeds either — but the kernel
/// cannot *locate* the cache's edge and does not claim to. It is deliberately
/// over-sized rather than tuned.
///
/// **It is assembly because its size is the measurement.** The C6's twin
/// records what happens otherwise: written as straight-line Rust, LLVM folded
/// the whole chain at compile time with a constant seed and re-rolled the
/// repeating pattern into a loop with an opaque one — 6 KiB legs came out 98
/// bytes. `.rept` cannot be folded, re-rolled or outlined.
///
/// The walk is a dependency chain, so nothing in it can issue around a
/// stalled fetch.
#[inline(never)]
fn code_walk(seed: u32) -> u32 {
    let mut a = seed;
    unsafe {
        core::arch::asm!(
            ".rept 12288",
            "add {a}, {a}, {b}",
            "xor {a}, {a}, {b}",
            ".endr",
            a = inout(reg) a,
            b = in(reg) 0x9E37_79B1u32,
            options(nostack, nomem),
        );
    }
    a
}

#[inline(never)]
fn poll_uart0_status(iters: u32) -> u32 {
    poll(UART0_STATUS, iters)
}

#[inline(never)]
fn poll_timg0_lact(iters: u32) -> u32 {
    poll(TIMG0_LACTLO, iters)
}

/// The IRAM/flash pair share a repetition, back to back: they are one
/// measurement of a difference, not two measurements.
static PLACEMENT: Group = Group {
    kernels: &[
        Kernel {
            name: "iram_loop",
            iters: ALU_ITERS,
            insns_per_iter: Some(6),
            body: iram_loop,
        },
        Kernel {
            name: "flash_loop",
            iters: ALU_ITERS,
            insns_per_iter: Some(6),
            body: flash_loop,
        },
    ],
};

/// The mul and div chains, likewise back to back: their difference is the one
/// instruction that differs between them.
static MULDIV: Group = Group {
    kernels: &[
        Kernel {
            name: "muldiv/mul",
            iters: MUL_ITERS,
            insns_per_iter: Some(5),
            body: mul_chain,
        },
        Kernel {
            name: "muldiv/div",
            iters: DIV_ITERS,
            insns_per_iter: Some(5),
            body: div_chain,
        },
    ],
};

/// The cold and warm walks share a repetition: they only mean anything back
/// to back, and their difference is what a fetch miss costs.
///
/// **Repetition 0 is the only genuinely cold pass.** Every later `cold`
/// reading walks code the previous repetition's `warm` pass has just touched,
/// so the spread between repetition 0 and the rest is itself part of the
/// measurement — one more reason nothing here is averaged.
static CODE_WALK: Group = Group {
    kernels: &[
        Kernel {
            name: "code_walk/cold",
            iters: CODE_WALK_PAIRS,
            insns_per_iter: Some(2),
            body: code_walk,
        },
        Kernel {
            name: "code_walk/warm",
            iters: CODE_WALK_PAIRS,
            insns_per_iter: Some(2),
            body: code_walk,
        },
    ],
};

/// Two APB blocks, reported separately — "the APB costs N" and "UART0 costs
/// N" are different claims and this payload refuses to conflate them.
static MMIO: Group = Group {
    kernels: &[
        Kernel {
            name: "mmio_poll/uart0-status",
            iters: MMIO_ITERS,
            insns_per_iter: Some(3),
            body: poll_uart0_status,
        },
        Kernel {
            name: "mmio_poll/timg0-lact",
            iters: MMIO_ITERS,
            insns_per_iter: Some(3),
            body: poll_timg0_lact,
        },
    ],
};

/// CCOUNT, through `xtensa_lx`'s `rsr.ccount` rather than a second copy of
/// that one instruction.
fn read_cycles() -> u32 {
    esp_hal::xtensa_lx::timer::get_cycle_count()
}

/// TIMG0 LACT microseconds — a clock that is not derived from CCOUNT.
fn read_us() -> u64 {
    esp_hal::time::Instant::now()
        .duration_since_epoch()
        .as_micros()
}

/// Entry point. Prints the header, runs every group, prints the done marker,
/// and idles.
pub fn run() -> ! {
    // A logger, because `fw-checks`'s runner emits its records through
    // `log`. This harness starts no runtime and no server, so nothing else
    // would install one and every record would go nowhere — the same failure
    // mode `write_header` exists for, one layer up. `esp_println`'s own
    // logger writes the UART0 FIFO whose divisor the entry point has just
    // programmed.
    esp_println::logger::init_logger(log::LevelFilter::Info);

    // The transcript header, first, before any record. Through
    // `esp_println::Printer` rather than `fw_checks::emit_header` even though
    // a logger IS installed above: the header must reach the port on every
    // payload harness whether or not that stays true, so all of them go
    // through the same sink-agnostic entry point.
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "cycle-probe",
            chip: super::CHIP,
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );
    info!(
        "[cycle-probe] esp32v3 (LX6) @ {CPU_HZ} Hz, cycle clock = CCOUNT, \
         us clock = TIMG0 LACT @ 16 MHz; RECORDED, NOT GATED (t1 only, M5 R1)"
    );

    let clocks = Clocks {
        read_cycles,
        read_us,
    };
    let placement = core::slice::from_ref(&PLACEMENT);
    let muldiv = core::slice::from_ref(&MULDIV);
    let code_walk = core::slice::from_ref(&CODE_WALK);
    let mmio = core::slice::from_ref(&MMIO);
    run_all(
        &clocks,
        &[placement, muldiv, code_walk, mmio, PORTABLE_GROUPS],
    );

    loop {
        core::hint::spin_loop();
    }
}
