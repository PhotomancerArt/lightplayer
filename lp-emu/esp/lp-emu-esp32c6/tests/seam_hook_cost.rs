//! What one hooked call costs the host (emulator seams M0, measurement S1).
//!
//! A synthetic guest — no firmware — calls a one-instruction function
//! `CALLS` times. Run once with nothing hooked and once with the function's
//! entry patched to `ebreak` and answered by a host hook (`Ret`); the
//! difference over `CALLS` is the per-call price of the seam mechanism: the
//! `ebreak` ending the slice, the machine's boundary, the table lookup and
//! the return. The seam's own answer (`ParkThenReturn`) adds an idle skip on
//! top, which the firmware runs measure in situ.
//!
//! `#[ignore]`: a timing probe, never a gate (host wall-clock). Run with
//! `cargo test --release -p lp-emu-esp32c6 --test seam_hook_cost -- --ignored --nocapture`.

use std::time::Instant;

use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::rom::HookResult;

const CODE: u32 = memmap::HP_SRAM_BASE + 0x1000;
const FUNC: u32 = CODE + 0x200;
const DONE: u32 = CODE + 0x300;
const CALLS: u32 = 200_000;

#[test]
#[ignore = "timing probe (host wall-clock); run by hand under low load"]
fn the_per_call_price_of_a_hooked_function() {
    for (name, cache, compressed) in [
        ("interpreter (= ROM-up's path)", false, false),
        ("block cache", true, false),
        ("block cache, c.ebreak entry", true, true),
    ] {
        let mut base = f64::MAX;
        let mut hooked = f64::MAX;
        for _ in 0..5 {
            base = base.min(run(cache, false, compressed));
            hooked = hooked.min(run(cache, true, compressed));
        }
        let per_call = (hooked - base) / CALLS as f64 * 1e9;
        println!(
            "S1 {name}: {CALLS} calls, unhooked {:.1} ms, hooked {:.1} ms, \
             {per_call:.0} ns per hooked call",
            base * 1e3,
            hooked * 1e3
        );
    }
}

#[test]
fn a_hooked_call_returns_to_the_caller_every_time() {
    let mut m = machine(true, true, false);
    let out = m.run_until(&StopCondition::after_micros(1_000_000));
    assert!(matches!(out, Outcome::Breakpoint { pc: DONE, .. }), "{out:?}");
    assert_eq!(m.hook_calls(), CALLS as u64 + 1, "every call, plus the stop");
    assert_eq!(m.harts[0].regs()[9], 0, "the loop ran to the end");
}

fn run(cache: bool, hook: bool, compressed: bool) -> f64 {
    let mut m = machine(cache, hook, compressed);
    let t = Instant::now();
    let out = m.run_until(&StopCondition::after_micros(10_000_000));
    let secs = t.elapsed().as_secs_f64();
    assert!(matches!(out, Outcome::Breakpoint { pc: DONE, .. }), "{out:?}");
    secs
}

fn machine(cache: bool, hook: bool, compressed: bool) -> Esp32C6Machine {
    let mut m = Esp32C6Builder::bare().block_cache(cache).build().unwrap();
    // s1 = CALLS; loop: jal ra, FUNC; addi s1, s1, -1; bnez s1, loop; j DONE
    let mut main = Vec::new();
    main.extend(li(9, CALLS));
    let top = main.len();
    let at = |i: usize| CODE + 4 * i as u32;
    main.push(jal(1, FUNC.wrapping_sub(at(top)) as i32));
    main.push(addi(9, 9, -1));
    let back = main.len();
    main.push(bne(9, 0, at(top).wrapping_sub(at(back)) as i32));
    let j = main.len();
    main.push(jal(0, DONE.wrapping_sub(at(j)) as i32));
    place(&mut m, CODE, &main);
    // FUNC: `ret`, or `c.addi sp,-16; c.addi sp,16; ret` with a compressed
    // entry (the firmware's seam starts with `c.addi sp, -16`).
    if compressed {
        let half: Vec<u8> = [0x1141u16, 0x0141u16]
            .iter()
            .flat_map(|h| h.to_le_bytes())
            .chain(jalr(0, 1, 0).to_le_bytes())
            .collect();
        m.bus.load_image(FUNC, &half).unwrap();
    } else {
        place(&mut m, FUNC, &[jalr(0, 1, 0)]);
    }
    place(&mut m, DONE, &[0x0010_0073]);
    if hook {
        m.hooks_mut().install_at(&mut m.bus, FUNC, "func", |_| HookResult::Ret).ok();
        if compressed {
            // What the seam arming plants on a compressed entry: `c.ebreak`
            // over the first half only.
            m.bus.load_image(FUNC, &0x9002u16.to_le_bytes()).unwrap();
            m.bus.load_image(FUNC + 2, &0x0141u16.to_le_bytes()).unwrap();
        }
    }
    m.hooks_mut()
        .install_at(&mut m.bus, DONE, "done", |_| HookResult::Stop)
        .unwrap();
    m.harts[0].set_pc(CODE);
    m
}

fn place(m: &mut Esp32C6Machine, at: u32, program: &[u32]) {
    let bytes: Vec<u8> = program.iter().flat_map(|w| w.to_le_bytes()).collect();
    m.bus.load_image(at, &bytes).unwrap();
}

fn lui(rd: u32, imm: u32) -> u32 {
    (imm & 0xffff_f000) | (rd << 7) | 0x37
}
fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}
fn li(rd: u32, value: u32) -> [u32; 2] {
    let lo = (value & 0xfff) as i32;
    let lo = if lo >= 0x800 { lo - 0x1000 } else { lo };
    let hi = value.wrapping_sub(lo as u32);
    [lui(rd, hi), addi(rd, rd, lo)]
}
fn jal(rd: u32, offset: i32) -> u32 {
    let o = offset as u32;
    (((o >> 20) & 1) << 31)
        | (((o >> 1) & 0x3ff) << 21)
        | (((o >> 11) & 1) << 20)
        | (((o >> 12) & 0xff) << 12)
        | (rd << 7)
        | 0x6f
}
fn jalr(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x67
}
fn bne(rs1: u32, rs2: u32, offset: i32) -> u32 {
    let o = offset as u32;
    (((o >> 12) & 1) << 31)
        | (((o >> 5) & 0x3f) << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (0b001 << 12)
        | (((o >> 1) & 0xf) << 8)
        | (((o >> 11) & 1) << 7)
        | 0x63
}
