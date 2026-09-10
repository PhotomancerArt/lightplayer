//! Fixture (e) — a two-task context switch, in esp-rtos's own shape.
//!
//! # Why this is not literally `esp-rtos`
//!
//! The brief asks for "an esp-rtos two-task context switch". `esp-rtos` 0.3
//! cannot be linked here: it depends on `esp-hal`, `esp-alloc`, `esp-sync`,
//! `esp-rom-sys` and the embassy family, all of which need a **SoC** — clocks,
//! peripherals, an interrupt matrix — and this phase's own rule is that the bus
//! is `Memory` and nothing else (no memory map, no peripherals). Linking it
//! would either not boot or drag the whole SoC in, which is M3's work.
//!
//! What *is* reproduced is the mechanism, exactly:
//!
//! - The switch happens **inside a level-1 interrupt handler**, on the frame
//!   xtensa-lx-rt's own `SAVE_CONTEXT` built — the same
//!   `xtensa_lx_rt::exception::Context` that `esp_hal::trapframe::TrapFrame`
//!   is (`esp-rtos-0.3.0/src/task/xtensa.rs:19`), and the switch is a swap of
//!   that frame with the other task's saved copy, which is what
//!   `esp-rtos`'s `task_switch` does.
//! - `save_context` spills **every live register window** to the running
//!   task's own stack before the swap (`SPILL_REGISTERS` in
//!   `xtensa-lx-rt-0.22.0/src/exception/asm.rs`), which is what makes swapping
//!   a 16-register frame plus `A1` a complete switch on a windowed machine.
//! - A new task's context is seeded exactly as `esp-rtos`'s
//!   `new_task_context` seeds one: `PC` at the entry point, `A1` at a
//!   16-aligned stack top with a zeroed base save area below it, and
//!   `PS = EXCM | UM | WOE | CALLINC(call4)` — `EXCM` set because the `rfe`
//!   at the end of the level-1 handler is what clears it.
//!
//! # What it asserts
//!
//! `LBEG`, `LEND` and `LCOUNT` are **per-task** state: `save_context` and
//! `restore_context` round-trip all three on every preemption. Each task
//! therefore runs a zero-overhead loop of its own length, at its own address,
//! and **preempts itself from inside the loop body** (`wsr.intset` at a chosen
//! iteration — the hart's poll point (b), so the interrupt lands on the very
//! next instruction and not somewhere a scheduler chose).
//!
//! Each round then checks three things that only hold if the loop state
//! survived the switch:
//!
//! - the body ran exactly `n` times;
//! - the fold of `LCOUNT` over the body equals `n(n-1)/2` — every intermediate
//!   count, not just the endpoints;
//! - `LBEG`/`LEND` after the loop are this task's, not the other's (the two
//!   loops are separate copies at different addresses, so a leak is visible).
//!
//! A hart that dropped `LCOUNT` across a preemption would corrupt whichever
//! task it resumed, and the corruption would look like a miscompiled loop.
//!
//! ## Result slots
//!
//! | slot | value |
//! |---|---|
//! | 1 | rounds task A completed |
//! | 2 | rounds task B completed |
//! | 3 | switches taken |
//! | 4 | **cross-task loop-state sightings — must be 0** |
//! | 5 | task A's `LCOUNT` fold on its last round |
//! | 6 | task B's `LCOUNT` fold on its last round |
//! | 7 | 1 if task A's `LBEG`/`LEND` were stable across its rounds |
//! | 8 | 1 if task B's `LBEG`/`LEND` were stable across its rounds |

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

use mach::{finish, record, sr};
use xtensa_lx_rt::exception::Context;

/// Line 7: level 1, `IntKind::Software`. Raised by the running task itself.
const LINE_SOFTWARE_L1: u32 = 1 << 7;
/// Line 0: level 1, `IntKind::Level` — the host test's **scripted** external
/// line, asserted at chosen guest cycles. Asynchronous preemption on top of
/// the deterministic self-preemption, so the switch is exercised at points the
/// fixture did not choose.
const LINE_EXTERNAL_L1: u32 = 1 << 0;

// PS fields, as `esp-rtos-0.3.0/src/task/xtensa.rs:51-60` spells them.
const PS_EXCM: u32 = 1 << 4;
const PS_UM: u32 = 1 << 5;
const PS_WOE: u32 = 1 << 18;
/// `CALLINC` for `call4`: the task entry is entered as if it had been `call4`'d.
const PS_CALLINC_CALL4: u32 = 1 << 16;

const N_A: u32 = 64;
const PREEMPT_A: u32 = 10;
const N_B: u32 = 48;
const PREEMPT_B: u32 = 7;
const ROUNDS_A: u32 = 6;

/// `sum(0..n)` — what folding `LCOUNT` over an `n`-iteration loop must give,
/// because `LCOUNT` reads `n-1, n-2, .., 0` on successive iterations.
const fn expected_fold(n: u32) -> u32 {
    n * (n - 1) / 2
}

const STACK_LEN: usize = 4096;

/// The field is never *named*: only the storage matters, and a task stack is
/// addressed as `&raw mut STACK_B` plus its length.
#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; STACK_LEN]);

static mut STACK_B: Stack = Stack([0; STACK_LEN]);
/// The *other* task's saved context — whichever is not running.
static mut CTX_OTHER: Context = Context::new();

static mut SWITCHES: u32 = 0;
static mut A_ROUNDS: u32 = 0;
static mut B_ROUNDS: u32 = 0;
static mut SIGHTINGS: u32 = 0;
static mut A_BOUNDS: (u32, u32) = (0, 0);
static mut B_BOUNDS: (u32, u32) = (0, 0);
static mut A_UNSTABLE: u32 = 0;
static mut B_UNSTABLE: u32 = 0;

/// The scheduler. Swap the interrupted task's frame with the other task's
/// saved context; `RESTORE_CONTEXT` + `rfe` then resume the other task,
/// including its `A1`, its `PC`, its `PS` and its `LBEG`/`LEND`/`LCOUNT`.
#[unsafe(no_mangle)]
extern "C" fn __level_1_interrupt(frame: &mut Context) {
    // Software lines stay latched until INTCLEAR; the external level line is
    // lowered by the host's script, not from here.
    sr::set_intclear(LINE_SOFTWARE_L1);
    // SAFETY: single-threaded bare-metal fixture, and level 1 cannot nest
    // (the naked handler raised PS.INTLEVEL to 1 before calling).
    unsafe {
        SWITCHES += 1;
        record(3, SWITCHES);
        core::mem::swap(&mut *(&raw mut CTX_OTHER), frame);
    }
}

/// One task's zero-overhead loop, instantiated once per task so the two have
/// **different** `LBEG`/`LEND`.
///
/// Returns `(iterations, LCOUNT fold, LBEG, LEND)`.
macro_rules! spin {
    ($name:ident) => {
        #[inline(never)]
        #[unsafe(no_mangle)]
        fn $name(n: u32, preempt_at: u32) -> (u32, u32, u32, u32) {
            let iterations: u32;
            let fold: u32;
            let lbeg: u32;
            let lend: u32;
            // SAFETY: registers only, plus one `wsr.intset` whose consequence
            // — a level-1 interrupt on the very next instruction — is the
            // point of the fixture.
            unsafe {
                core::arch::asm!(
                    "movi   {it}, 0",
                    "movi   {fold}, 0",
                    "loopnez {n}, 92f",
                    // --- body ---
                    "  addi {it}, {it}, 1",
                    "  rsr.lcount {t}",
                    "  add  {fold}, {fold}, {t}",
                    "  bne  {it}, {pa}, 91f",
                    "  wsr.intset {line}",   // preempt HERE, mid-loop
                    "91:",
                    "  nop",
                    "92:",
                    "rsr.lbeg {lbeg}",
                    "rsr.lend {lend}",
                    it = out(reg) iterations,
                    fold = out(reg) fold,
                    lbeg = out(reg) lbeg,
                    lend = out(reg) lend,
                    t = out(reg) _,
                    n = in(reg) n,
                    pa = in(reg) preempt_at,
                    line = in(reg) LINE_SOFTWARE_L1,
                    options(nostack),
                );
            }
            (iterations, fold, lbeg, lend)
        }
    };
}

spin!(spin_a);
spin!(spin_b);

/// Compare a round against what it must be, and remember any disagreement.
///
/// `bounds` is the task's remembered `(LBEG, LEND)`; the first round seeds it.
fn check(
    n: u32,
    got: (u32, u32, u32, u32),
    bounds: &mut (u32, u32),
    unstable: &mut u32,
    first: bool,
) {
    let (iterations, fold, lbeg, lend) = got;
    if iterations != n || fold != expected_fold(n) {
        // SAFETY: single-threaded bare-metal fixture.
        unsafe {
            SIGHTINGS += 1;
            record(4, SIGHTINGS);
        }
    }
    if first {
        *bounds = (lbeg, lend);
    } else if *bounds != (lbeg, lend) {
        *unstable += 1;
    }
}

extern "C" fn task_b() -> ! {
    loop {
        // SAFETY: single-threaded bare-metal fixture.
        let first = unsafe { B_ROUNDS == 0 };
        let got = spin_b(N_B, PREEMPT_B);
        // SAFETY: as above.
        unsafe {
            check(N_B, got, &mut *(&raw mut B_BOUNDS), &mut *(&raw mut B_UNSTABLE), first);
            B_ROUNDS += 1;
            record(2, B_ROUNDS);
            record(6, got.1);
            record(8, u32::from(B_UNSTABLE == 0));
        }
    }
}

#[xtensa_lx_rt::entry]
fn main() -> ! {
    record(4, 0);

    // Seed task B's context — `esp-rtos`'s `new_task_context`, verbatim in
    // shape: a 16-aligned stack top with a zeroed base save area under it, so
    // the first window underflow in task B terminates instead of reading
    // whatever the stack happened to hold.
    // SAFETY: single-threaded bare-metal fixture; `STACK_B` is this image's
    // own `.bss` and is not otherwise touched.
    unsafe {
        let top = (((&raw mut STACK_B) as u32) + STACK_LEN as u32) & !15;
        ((top - 4) as *mut u32).write_volatile(0);
        ((top - 8) as *mut u32).write_volatile(0);
        ((top - 12) as *mut u32).write_volatile(top);
        ((top - 16) as *mut u32).write_volatile(0);

        let ctx = &mut *(&raw mut CTX_OTHER);
        ctx.PC = task_b as *const () as u32;
        ctx.A0 = 0;
        ctx.A1 = top;
        ctx.PS = PS_EXCM | PS_UM | PS_WOE | PS_CALLINC_CALL4;
    }

    // `Reset` leaves INTENABLE at 0. Both level-1 sources: the task's own
    // software line and the host's scripted external one.
    sr::set_intenable(LINE_SOFTWARE_L1 | LINE_EXTERNAL_L1);

    for round in 0..ROUNDS_A {
        let got = spin_a(N_A, PREEMPT_A);
        // SAFETY: as above.
        unsafe {
            check(
                N_A,
                got,
                &mut *(&raw mut A_BOUNDS),
                &mut *(&raw mut A_UNSTABLE),
                round == 0,
            );
            A_ROUNDS += 1;
            record(1, A_ROUNDS);
            record(5, got.1);
            record(7, u32::from(A_UNSTABLE == 0));
        }
    }

    finish()
}
