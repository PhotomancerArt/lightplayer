//! M5 P2: the block cache's machine-level claims — the fence contract, the
//! emulator's own code-writer funnels, and the `--strict-bus` checker.
//!
//! The hart's own conformance is in `lp-riscv-emu`'s `mach::tests`, where
//! every case is run twice and required to produce identical state with the
//! cache on and off. What can only be said *here* is the part that needs a
//! bus with regions, a hook table and a strict mode: that a page the guest
//! wrote and executed without a `fence.i` is named as a firmware bug, that a
//! host-side `load_image` under a live block invalidates it, and that a
//! snapshot restore starts with nothing cached.
//!
//! Tiny guests, no firmware, milliseconds — the `Emulator C6` CI job runs 27
//! minutes against a 35-minute budget and must not grow.

use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine, StopCondition};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::rom::{HookResult, HookTable};

const CODE: u32 = memmap::HP_SRAM_BASE + 0x1000;
/// The subroutine the tests rewrite, on the same 4 KiB page as `CODE`.
const SUB: u32 = CODE + 0x100;
const EBREAK: u32 = 0x0010_0073;
/// `fence.i` — `MISC-MEM`, funct3 1, imm 0x001, rs1 = rd = 0.
const FENCE_I: u32 = 0x0000_100f;

fn lui(rd: u32, imm: u32) -> u32 {
    (imm & 0xffff_f000) | (rd << 7) | 0x37
}
fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}
fn sw(rs2: u32, rs1: u32, imm: i32) -> u32 {
    let imm = imm as u32 & 0xfff;
    ((imm >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (0b010 << 12) | ((imm & 0x1f) << 7) | 0x23
}
/// `jal rd, offset` (offset relative to the instruction, even, ±1 MiB).
fn jal(rd: u32, offset: i32) -> u32 {
    let o = offset as u32;
    (((o >> 20) & 1) << 31)
        | (((o >> 1) & 0x3ff) << 21)
        | (((o >> 11) & 1) << 20)
        | (((o >> 12) & 0xff) << 12)
        | (rd << 7)
        | 0x6f
}
/// `jalr rd, rs1, imm`.
fn jalr(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x67
}
fn li(rd: u32, value: u32) -> [u32; 2] {
    let lo = (value & 0xfff) as i32;
    let lo = if lo >= 0x800 { lo - 0x1000 } else { lo };
    let hi = value.wrapping_sub(lo as u32);
    [lui(rd, hi), addi(rd, rd, lo)]
}

fn place(m: &mut Esp32C6Machine, at: u32, program: &[u32]) {
    let bytes: Vec<u8> = program.iter().flat_map(|w| w.to_le_bytes()).collect();
    m.bus.load_image(at, &bytes).unwrap();
}

/// The self-modifying guest both fence tests use.
///
/// `t0` (x5) counts. The subroutine at [`SUB`] adds `1` and returns; the main
/// program calls it, overwrites its first instruction with "add 16",
/// optionally fences, calls it again, and stops. So `t0` is **17** when the
/// rewrite was seen and **2** when a stale instruction ran.
fn self_modifying_guest(fenced: bool) -> (Vec<u32>, u32) {
    let mut main = Vec::new();
    main.extend(li(10, SUB)); // a0 = &sub
    main.extend(li(11, addi(5, 5, 16))); // a1 = the replacement word
    let call1 = main.len();
    main.push(0); // placeholder: jal ra, sub
    main.push(sw(11, 10, 0)); // *a0 = a1
    if fenced {
        main.push(FENCE_I);
    }
    let call2 = main.len();
    main.push(0); // placeholder: jal ra, sub
    main.push(EBREAK);

    let at = |i: usize| CODE + 4 * i as u32;
    main[call1] = jal(1, SUB.wrapping_sub(at(call1)) as i32);
    main[call2] = jal(1, SUB.wrapping_sub(at(call2)) as i32);
    (main, CODE)
}

fn subroutine() -> Vec<u32> {
    vec![addi(5, 5, 1), jalr(0, 1, 0)]
}

fn run_self_modifying(fenced: bool, strict: bool, cache: bool) -> Esp32C6Machine {
    let mut m = Esp32C6Builder::bare()
        .strict(strict)
        .block_cache(cache)
        .build()
        .unwrap();
    let (main, entry) = self_modifying_guest(fenced);
    place(&mut m, SUB, &subroutine());
    place(&mut m, entry, &main);
    m.harts[0].set_pc(entry);
    m.run_until(&StopCondition::after_micros(50));
    m
}

#[test]
fn a_fenced_rewrite_is_seen_and_the_strict_checker_says_nothing() {
    for cache in [true, false] {
        let m = run_self_modifying(true, true, cache);
        assert_eq!(
            m.harts[0].regs()[5],
            17,
            "cache={cache}: the second call must run the rewritten instruction"
        );
        assert_eq!(m.fence_i_count(), 1, "cache={cache}");
        assert_eq!(
            m.bus.missing_fence_reports(),
            0,
            "cache={cache}: the guest published its write, so there is nothing to report"
        );
    }
}

#[test]
fn an_unfenced_rewrite_is_named_as_a_firmware_bug_under_strict_bus() {
    for cache in [true, false] {
        let m = run_self_modifying(false, true, cache);
        assert_eq!(m.fence_i_count(), 0, "cache={cache}");
        assert!(
            m.bus.missing_fence_reports() >= 1,
            "cache={cache}: a code page written and then executed with no `fence.i` between \
             is exactly what the checker exists to name"
        );
    }
    // And the reason it matters: with the cache on, the stale block runs.
    // That is not a bug in the cache — it is the firmware bug the checker
    // just named, made visible.
    assert_eq!(run_self_modifying(false, true, true).harts[0].regs()[5], 2);
    assert_eq!(
        run_self_modifying(false, true, false).harts[0].regs()[5],
        17
    );
}

#[test]
fn the_checker_costs_nothing_and_reports_nothing_without_strict_bus() {
    let m = run_self_modifying(true, false, true);
    assert_eq!(m.bus.missing_fence_reports(), 0);
    assert_eq!(m.bus.code_pages_seen(), 0, "no page tracking off strict");
}

/// The emulator's own funnel: a host-side `load_image` over a live block.
///
/// This is `cache::fill`'s and `rom::install_at`'s path — every host write of
/// guest code goes through `SocBus::load_image`, is recorded, and is drained
/// by the machine at the slice boundary.
#[test]
fn a_host_side_load_image_under_a_cached_block_invalidates_it() {
    let mut m = Esp32C6Builder::bare().build().unwrap();
    place(&mut m, SUB, &subroutine());
    // A loop that calls the subroutine until it is stopped, so the block at
    // SUB is cached and hot before the host rewrites it.
    let mut main = Vec::new();
    main.push(jal(1, SUB.wrapping_sub(CODE) as i32));
    main.push(EBREAK);
    place(&mut m, CODE, &main);
    m.harts[0].set_pc(CODE);
    m.run_until(&StopCondition::after_micros(20));
    assert_eq!(m.harts[0].regs()[5], 1, "the subroutine ran once");
    assert!(m.block_stats().expect("a cache was built").decodes > 0);

    // The host rewrites the subroutine and the guest calls it again.
    place(&mut m, SUB, &[addi(5, 5, 16), jalr(0, 1, 0)]);
    m.harts[0].set_pc(CODE);
    m.run_until(&StopCondition::after_micros(40));
    assert_eq!(
        m.harts[0].regs()[5],
        17,
        "the funnel must have invalidated the block the host wrote over"
    );
    assert!(m.block_stats().unwrap().range_invalidations > 0);
}

/// F6: a ROM-hook `ebreak` patch over a block that is already cached.
///
/// `HookTable::install_at` writes a live instruction through the same
/// `load_image` funnel. It is the one the milestone brief missed, and it
/// patches mask ROM the guest is running.
#[test]
fn a_hook_installed_over_a_cached_block_takes_effect_immediately() {
    let mut m = Esp32C6Builder::bare().build().unwrap();
    place(&mut m, SUB, &subroutine());
    place(
        &mut m,
        CODE,
        &[jal(1, SUB.wrapping_sub(CODE) as i32), EBREAK],
    );
    m.harts[0].set_pc(CODE);
    m.run_until(&StopCondition::after_micros(20));
    assert_eq!(m.harts[0].regs()[5], 1);

    // Claim the subroutine's first instruction. `HookResult::Ret` returns to
    // `ra` without running it, so `t0` must NOT move on the second call.
    // Built beside the machine and moved in, because  needs the
    // bus and the table at once; the write it makes is the same one
    //  makes.
    let mut hooks = HookTable::new();
    hooks
        .install_at(&mut m.bus, SUB, "sub", |_| HookResult::Ret)
        .unwrap();
    *m.hooks_mut() = hooks;
    m.harts[0].set_pc(CODE);
    m.run_until(&StopCondition::after_micros(40));
    assert_eq!(
        m.harts[0].regs()[5],
        1,
        "the hook's `ebreak` must be what the hart fetches, not the cached \
         instruction it replaced"
    );
}

/// The cache is not architectural state: a snapshot restore starts empty.
#[test]
fn a_snapshot_restore_starts_with_an_empty_cache() {
    let mut m = Esp32C6Builder::bare().build().unwrap();
    place(&mut m, SUB, &subroutine());
    place(
        &mut m,
        CODE,
        &[jal(1, SUB.wrapping_sub(CODE) as i32), EBREAK],
    );
    m.harts[0].set_pc(CODE);
    let snapshot = m.snapshot();
    m.run_until(&StopCondition::after_micros(20));
    assert!(m.block_stats().expect("built").decodes > 0);

    m.restore(&snapshot);
    assert!(
        m.block_stats().is_none(),
        "a restored hart caches nothing yet — the snapshot never held a cache"
    );
    // And it still runs.
    m.run_until(&StopCondition::after_micros(20));
    assert_eq!(m.harts[0].regs()[5], 1);
}

/// `--no-block-cache` really does keep the machine from building one.
#[test]
fn no_block_cache_builds_nothing() {
    let mut m = Esp32C6Builder::bare().block_cache(false).build().unwrap();
    place(&mut m, SUB, &subroutine());
    place(
        &mut m,
        CODE,
        &[jal(1, SUB.wrapping_sub(CODE) as i32), EBREAK],
    );
    m.harts[0].set_pc(CODE);
    m.run_until(&StopCondition::after_micros(20));
    assert_eq!(m.harts[0].regs()[5], 1);
    assert!(!m.block_cache());
    assert!(m.block_stats().is_none());
}

/// MD13: a ROM-up boot runs with the cache off, because the mask ROM and the
/// real ESP-IDF second-stage bootloader copy code into RAM as *guest* code
/// and will never emit a `fence.i` — and we own neither.
#[test]
fn a_rom_up_boot_refuses_to_cache() {
    let m = Esp32C6Builder::bare()
        .boot_mode(lp_emu_esp32c6::machine::BootMode::RomUp)
        .build()
        .unwrap();
    assert!(!m.block_cache());
}

/// A store onto a code page that writes the bytes that were already there
/// publishes nothing and owes no fence. P1 measured 36 % of the render loop's
/// stores onto executed pages doing exactly this.
#[test]
fn a_store_that_changes_no_bytes_is_not_a_missing_fence() {
    let mut m = Esp32C6Builder::bare().strict(true).build().unwrap();
    place(&mut m, SUB, &subroutine());
    let mut main = Vec::new();
    main.extend(li(10, SUB));
    main.extend(li(11, addi(5, 5, 1))); // exactly what is already there
    let call1 = main.len();
    main.push(0);
    main.push(sw(11, 10, 0));
    let call2 = main.len();
    main.push(0);
    main.push(EBREAK);
    let at = |i: usize| CODE + 4 * i as u32;
    main[call1] = jal(1, SUB.wrapping_sub(at(call1)) as i32);
    main[call2] = jal(1, SUB.wrapping_sub(at(call2)) as i32);
    place(&mut m, CODE, &main);
    m.harts[0].set_pc(CODE);
    m.run_until(&StopCondition::after_micros(50));

    assert_eq!(m.harts[0].regs()[5], 2, "the subroutine ran twice");
    assert_eq!(m.fence_i_count(), 0);
    assert_eq!(
        m.bus.missing_fence_reports(),
        0,
        "nothing changed, so nothing needed publishing"
    );
}
