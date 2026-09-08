//! ESP32-C6 `cycle-probe`: the payload's device half.
//!
//! The bracket, the repetition, the record and the three kernels that are
//! ordinary Rust live in `fw_checks::checks::cycle_probe`, where they are
//! `no_std` and know nothing about this chip. What stays here is what cannot
//! leave: board init, the USB-Serial-JTAG link, the clocks, and the kernels
//! whose whole point is a chip fact.
//!
//! Three kinds of chip fact keep a kernel here:
//!
//! 1. **Where the code lives.** `iram_loop` and `flash_loop` are the same six
//!    instructions; the only difference between them is `#[esp_hal::ram]`,
//!    which is a linker section. That difference *is* the measurement, so the
//!    pair has to be written where the attribute exists.
//! 2. **An exact instruction count.** `insns` on a record has to be a fact,
//!    not an estimate, and the only way to know a loop's length exactly is to
//!    write the loop — which means an ISA. These four are inline assembly,
//!    counted by reading them.
//! 3. **An address.** The MMIO polls read real peripheral registers.
//!
//! ## The two clocks
//!
//! `read_cycles` is the PMU counter (`board::esp32c6::cycle_counter`,
//! `mpccr`). `read_us` is `embassy_time::Instant`, which on this chip is
//! **SYSTIMER Unit0** at XTAL/2.5 = 16 MHz — a different counter off a
//! different clock, not a rescaling of `mpccr`. (`esp_rtos` is started on
//! `timg0.timer0`, so the RTOS tick is TIMG0 and SYSTIMER serves
//! `Instant::now` alone.) That independence is the point: the payload records
//! both so that a disagreement between them can be seen rather than argued
//! about.
//!
//! ## What each assembly loop costs, per iteration
//!
//! Counted off the source, and that count is what reaches the record:
//!
//! | loop | instructions | the one that matters |
//! |---|---|---|
//! | `iram_loop` / `flash_loop` | 6 | none — this is the ALU floor |
//! | `muldiv/mul` | 5 | one `mul` |
//! | `muldiv/div` | 5 | one `divu` |
//! | `mmio_poll/*` | 3 | one `lw` from a peripheral |
//!
//! `muldiv/mul` and `muldiv/div` are deliberately the same five instructions
//! apart from the one under test, so their difference is that instruction and
//! nothing else. Their operands change every iteration, so neither reports a
//! single operand pair's cost — but both are dependency chains, so what they
//! measure is *latency*, not throughput. The calibration report says so.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;

use esp_hal::usb_serial_jtag::UsbSerialJtag;
use fw_checks::checks::cycle_probe::{Clocks, Group, Kernel, PORTABLE_GROUPS, run_all};
use log::info;

use crate::board::esp32c6::constants::CPU_HZ;
use crate::board::esp32c6::cycle_counter;
use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
// Through the module rather than the re-export: `serial::Esp32UsbSerialIo` is
// gated to a named list of harnesses, and adding a name to that list would be
// an edit to a file this phase is fenced out of. `serial::usb_serial` is
// public on `esp32c6` alone, so the concrete type is reachable without one.
use crate::serial::usb_serial::Esp32UsbSerialIo;

/// UART0 `status`, `0x6000_0000 + 0x01c`. The register `notes.md` F9 names:
/// 86 % of this harness's MMIO reads are of this one address, so it is the
/// APB read whose cost the model most needs.
const UART0_STATUS: *const u32 = 0x6000_001C as *const u32;

/// SYSTIMER `unit0_value.lo`, `0x6000_A000 + 0x044`. A second APB block, read
/// the same way, so that "the APB costs N" can be told apart from "UART0
/// costs N".
const SYSTIMER_UNIT0_LO: *const u32 = 0x6000_A044 as *const u32;

/// Iterations for the six-instruction ALU loops. ~1.5 M instructions, which
/// at any plausible cost is milliseconds — four orders of magnitude above the
/// bracket overhead, and far enough above the microsecond clock's resolution
/// that the two clocks can be compared to a fraction of a percent.
const ALU_ITERS: u32 = 250_000;

/// Iterations for the multiply chain. Same five-instruction shape as the
/// divide chain, so the two are directly comparable.
const MUL_ITERS: u32 = 250_000;

/// Iterations for the divide chain. Fewer, because a divide is the most
/// expensive instruction here — the `t2` model charges 32 for it — and the
/// kernel should take about as long as the others, not thirty times longer.
const DIV_ITERS: u32 = 40_000;

/// Iterations for the MMIO polls. One peripheral read each; sized on the
/// assumption that an APB access is tens of cycles, which is exactly what
/// this kernel exists to find out — so it is sized generously rather than
/// tuned to a number nobody has measured.
const MMIO_ITERS: u32 = 40_000;

/// Six instructions per iteration: `addi`, `add`, `xor`, `slli`, `add`,
/// `bnez`. No memory operand of any kind, so the only thing between the
/// processor and this loop is instruction fetch — which is the difference the
/// IRAM/flash pair is built to expose.
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
/// though `flash_loop` needs one.
#[esp_hal::ram]
fn iram_loop(iters: u32) -> u32 {
    alu_loop_body(iters)
}

/// The identical loop, left where the linker puts code by default. Every
/// cycle by which this exceeds `iram_loop` is fetch.
#[inline(never)]
fn flash_loop(iters: u32) -> u32 {
    alu_loop_body(iters)
}

/// Five instructions per iteration, one of them a `mul`, with operands that
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
            "mul  {t}, {acc}, {d}",
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

/// The same five instructions with `divu` in place of `mul`. The divisor is a
/// non-zero constant, so nothing traps and nothing degenerates.
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
            "divu {t}, {acc}, {d}",
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
/// differs for that reason would look exactly like a model disagreeing. The
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
            "lw   {t}, 0({p})",
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

#[inline(never)]
fn poll_uart0_status(iters: u32) -> u32 {
    poll(UART0_STATUS, iters)
}

#[inline(never)]
fn poll_systimer(iters: u32) -> u32 {
    poll(SYSTIMER_UNIT0_LO, iters)
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
            name: "mmio_poll/systimer",
            iters: MMIO_ITERS,
            insns_per_iter: Some(3),
            body: poll_systimer,
        },
    ],
};

fn read_us() -> u64 {
    embassy_time::Instant::now().as_micros()
}

pub async fn run_cycle_probe(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt, usb_device, _gpio18, _flash, _gpio4, _gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);

    let usb_serial = UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));

    logger::set_log_serial(serial_io_shared);
    logger::init(logger::log_write_bytes);

    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
    cycle_counter::setup();

    // The transcript header, first, before any record.
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "cycle-probe",
            chip: "esp32c6",
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );
    info!("[cycle-probe] esp32c6 @ {CPU_HZ} Hz, us clock = SYSTIMER unit0 @ 16 MHz");

    let clocks = Clocks {
        read_cycles: cycle_counter::read,
        read_us,
    };
    let placement = core::slice::from_ref(&PLACEMENT);
    let muldiv = core::slice::from_ref(&MULDIV);
    let mmio = core::slice::from_ref(&MMIO);
    run_all(&clocks, &[placement, muldiv, mmio, PORTABLE_GROUPS]);

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(60)).await;
    }
}
