//! `rtc_get_reset_reason`, executed for real — and the answer to the phase
//! brief's first hook question.
//!
//! `__pre_init` calls the ROM's `rtc_get_reset_reason` **before `.bss` is
//! zeroed** and zeroes `.rtc_fast.persistent` only when it returns 1
//! (`POWERON`). The brief asks: does the real ROM path give the right answer
//! against an accept-and-remember peripheral, or does it need a host hook?
//!
//! It does not need a hook. The routine is three instructions —
//!
//! ```text
//! 0x40019680  lui  a5, 0x600b0
//! 0x40019684  lw   a0, 0x410(a5)
//! 0x40019688  andi a0, a0, 31
//! 0x4001968a  ret
//! ```
//!
//! — so it returns `LP_CLKRST.reset_cause & 0x1f`, and the fix is one
//! **reset value** on that block: `with_reset(0x010, 1)`. That is the
//! brief's preferred outcome (seed the register, do not hook), and P5 owns
//! the line.
//!
//! Note the base. `0x600B_0410` is **not** LP_AON, which is at `0x600B_1000`
//! — it is `LP_CLKRST` (`0x600B_0400`) offset `0x10`, whose PAC accessor is
//! literally called `reset_cause`. The generated register-name table is what
//! caught that; reading the address as "LP_AON plus something" would have
//! sent P5 to model the wrong block.
//!
//! The failing half is pinned too, because it is the trap: with the register
//! reading zero — which is what an unmodelled block and a plain `RegFile`
//! both give — the ROM returns 0, `.rtc_fast.persistent` is never zeroed on a
//! genuine power-on, and the firmware's `resetReason` is quietly wrong. That
//! is a bug you would chase for a day from the symptom.

use lp_emu_core::Bus;
use lp_emu_esp_common::{RegFile, SocBus};
use lp_riscv_emu::mach::{MachineHart, SliceEnd};

use lp_emu_esp32c6::Esp32C6Machine;
use lp_emu_esp32c6::machine::Esp32C6Builder;

/// `LP_CLKRST`'s base and the register the ROM reads. `0x600B_0410` is the
/// address the trace shows on the first MMIO access of every boot.
const LP_CLKRST_BASE: u32 = 0x600B_0400;
const LP_CLKRST_LEN: u32 = 0x400;
const RESET_CAUSE_OFF: u32 = 0x010;

/// Somewhere in HP SRAM to park a return address at.
const RETURN_TO: u32 = 0x4080_0000;

/// Call the ROM routine and give back `a0`.
fn call_rtc_get_reset_reason(machine: &mut Esp32C6Machine) -> u32 {
    let entry = machine
        .rom()
        .symbol("rtc_get_reset_reason")
        .expect("the vendored ROM has the routine")
        .address;

    // `ret` is `jalr x0, 0(ra)`, so park an `ebreak` at the return address
    // and the slice ends exactly when the routine returns.
    machine
        .bus
        .load_image(RETURN_TO, &lp_emu_esp32c6::rom::EBREAK.to_le_bytes())
        .expect("HP SRAM takes a host-side write");
    machine.harts[0].set_pc(entry);
    machine.harts[0].regs_mut()[1] = RETURN_TO as i32; // ra

    match machine.harts[0].run_slice(&mut machine.bus, 1_000, u64::MAX) {
        SliceEnd::Ebreak { pc } => assert_eq!(pc, RETURN_TO, "returned somewhere unexpected"),
        other => panic!("the ROM routine did not return: {other:?}"),
    }
    machine.harts[0].regs()[10] as u32 // a0
}

fn machine_with_lp_clkrst(reset_cause: u32) -> Esp32C6Machine {
    Esp32C6Builder::bare()
        .peripheral(
            LP_CLKRST_BASE,
            LP_CLKRST_LEN,
            Box::new(
                RegFile::new("LP_CLKRST", LP_CLKRST_LEN)
                    .with_names(lp_emu_esp32c6::regs::LP_CLKRST)
                    .with_reset(RESET_CAUSE_OFF, reset_cause),
            ),
        )
        .build()
        .expect("a machine with one accept-and-remember block")
}

#[test]
fn the_routine_is_the_three_instructions_the_analysis_says_it_is() {
    let mut m = Esp32C6Builder::new().build().unwrap();
    let entry = m.rom().symbol("rtc_get_reset_reason").unwrap().address;
    assert_eq!(entry, 0x4001_9680);
    // `lui a5, 0x600b0` then `lw a0, 0x410(a5)`.
    assert_eq!(m.peek_word(entry).unwrap(), 0x600b_07b7);
    assert_eq!(m.peek_word(entry + 4).unwrap(), 0x4107_a503);
}

#[test]
fn seeding_lp_clkrst_makes_the_real_rom_return_power_on_with_no_hook() {
    let mut m = machine_with_lp_clkrst(1);
    assert_eq!(
        call_rtc_get_reset_reason(&mut m),
        1,
        "POWERON — the value `__pre_init` compares against before zeroing \
         .rtc_fast.persistent"
    );
    assert!(m.hooks().is_empty(), "and no hook was needed to get it");
    assert_eq!(m.hook_calls(), 0);
}

#[test]
fn an_unseeded_block_makes_it_answer_zero_which_is_the_trap() {
    let mut m = machine_with_lp_clkrst(0);
    assert_eq!(
        call_rtc_get_reset_reason(&mut m),
        0,
        "a plain accept-and-remember RegFile reads 0, so the ROM says \
         `no reset` on a genuine power-on"
    );
}

#[test]
fn the_low_five_bits_are_the_reason_and_the_rest_are_masked_off() {
    // `andi a0, a0, 31`: a reset-cause register carrying other flags in its
    // high bits still answers correctly, which is why seeding the whole
    // word is safe.
    let mut m = machine_with_lp_clkrst(0xDEAD_BEE1);
    assert_eq!(call_rtc_get_reset_reason(&mut m), 0xDEAD_BEE1 & 0x1f);
    assert_eq!(0xDEAD_BEE1u32 & 0x1f, 1, "…and this one is still POWERON");
}

#[test]
fn the_default_machine_answers_power_on_which_is_the_line_p5_owed() {
    // Director note 1: `LP_CLKRST.reset_cause` seeded so the real ROM says
    // POWERON on a machine built with no arguments at all.
    let mut m = Esp32C6Builder::new().build().unwrap();
    assert_eq!(call_rtc_get_reset_reason(&mut m), 1);
    assert!(m.hooks().is_empty());
    assert_eq!(m.bus.unmapped_reads(), 0);
}

#[test]
fn with_no_block_at_all_the_read_is_reported_as_an_unmodelled_mmio_site() {
    // A bare machine: nothing is mapped, so the first MMIO access of every
    // boot is this one, and it is visible rather than silent.
    let mut m = Esp32C6Builder::bare().build().unwrap();
    assert_eq!(call_rtc_get_reset_reason(&mut m), 0);
    assert_eq!(m.bus.unmapped_reads(), 1);
    assert_eq!(m.bus.unmapped_sites(), 1);
    assert!(
        m.bus.in_mmio_window(LP_CLKRST_BASE + RESET_CAUSE_OFF),
        "it is inside a declared MMIO window, so the log says `unmodelled block`"
    );
}

#[test]
fn a_hook_can_stand_in_for_the_routine_if_a_later_phase_ever_needs_one() {
    // The mechanism, exercised end to end: the machine gets first refusal on
    // the `ebreak`, the host function sets `a0`, and the machine performs
    // the `ret`. Nothing in the shipped table uses it — see rom.rs for the
    // rule — but a mechanism nobody has run is a mechanism that does not
    // work.
    fn power_on(machine: &mut Esp32C6Machine) -> lp_emu_esp32c6::HookResult {
        machine.harts[0].regs_mut()[10] = 1;
        lp_emu_esp32c6::HookResult::Ret
    }

    let mut m = Esp32C6Builder::new().build().unwrap();
    let rom = m.rom().clone();
    let mut hooks = std::mem::take(m.hooks_mut());
    hooks
        .install(&mut m.bus, &rom, "rtc_get_reset_reason", power_on)
        .unwrap();
    *m.hooks_mut() = hooks;

    let entry = rom.symbol("rtc_get_reset_reason").unwrap().address;
    m.bus
        .load_image(RETURN_TO, &lp_emu_esp32c6::rom::EBREAK.to_le_bytes())
        .unwrap();
    m.harts[0].set_pc(entry);
    m.harts[0].regs_mut()[1] = RETURN_TO as i32;

    let stop = lp_emu_esp32c6::StopCondition {
        stop_cycle: Some(1_000),
        ..Default::default()
    };
    m.run_until(&stop);

    assert_eq!(m.harts[0].regs()[10], 1, "the hook set a0");
    assert_eq!(m.hook_calls(), 1);
    assert_eq!(m.bus.unmapped_reads(), 0, "the ROM's own load never ran");
}

/// A hart of the machine's own bus type, so the file's imports are the ones
/// a reader would reach for.
#[allow(dead_code, reason = "documents the concrete instantiation")]
fn hart_type_is_concrete(h: &MachineHart<SocBus>) -> u32 {
    h.pc()
}

#[allow(dead_code, reason = "the trait is what makes peek/poke work")]
fn bus_is_a_bus(b: &mut SocBus) -> bool {
    b.take_sideband()
}
