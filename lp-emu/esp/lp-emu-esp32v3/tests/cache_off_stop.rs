//! **D4** — the cache-off fetch stop, on a hand-built fixture.
//!
//! The fixture is eight instructions in SRAM0 that do what
//! `Cache_Read_Disable` does and then jump into the IROM window:
//!
//! ```text
//! 40089000:  .word 0x3ff00040       ; &DPORT.pro_cache_ctrl
//! 40089004:  .word 0x400d1a2c       ; somewhere in the IROM window
//! 40089010:  l32r    a2, 0x40089000
//! 40089013:  l32i.n  a3, a2, 0
//! 40089015:  movi.n  a4, -9         ; ~PRO_CACHE_ENABLE, the ROM's own constant
//! 40089017:  and     a3, a3, a4     ;   (`40009abe: movi.n a10, -9`)
//! 4008901a:  s32i.n  a3, a2, 0      ; the write the stop's message names
//! 4008901c:  memw
//! 4008901f:  l32r    a5, 0x40089004
//! 40089022:  jx      a5             ; → 0x400d1a2c, through the flash window
//! ```
//!
//! ⚠️ **Not built by running the real firmware into the condition.** The
//! phase file is explicit about why: a test that depends on the firmware
//! reaching a bug is not a test. It is also not hand-encoded — M0 found a
//! real misdecode in this repo that came from trusting an encoding by
//! analogy, so the bytes come from `lp_xt_inst::encode`, the repo's own
//! encoder, and [`the_fixture_decodes_back_to_what_it_says_it_is`] holds it
//! to that.
//!
//! `-9` is the ROM's own mask: `Cache_Read_Disable` at `0x4000_9AB8` loads
//! it with `40009abe: movi.n a10, -9` and ANDs it into `DPORT+0x40`.

use lp_emu_esp32v3::cache::{CACHE_ENABLE, CacheOffPolicy, Window};
use lp_emu_esp32v3::machine::{
    BootFrame, BootMode, Esp32V3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32v3::{loader, memmap};
use lp_xt_inst::{AluRrr, Inst, NullaryOp, Reg};

/// Where the fixture's two literals live: SRAM0, above the vectors and well
/// clear of anything the ROM seeds.
const LIT: u32 = 0x4008_9000;
/// Where its code lives.
const CODE: u32 = 0x4008_9010;
/// The instruction the `jx` lands on — an address inside the IROM window,
/// which on this machine is a read-only RAM region and on silicon is served
/// by the cache.
const TARGET: u32 = 0x400D_1A2C;

fn reg(n: u8) -> Reg {
    Reg::new(n)
}

/// `l32r`'s raw 16-bit field: `(label - ((pc + 3) & !3)) / 4`, backward only.
fn l32r_field(pc: u32, label: u32) -> u16 {
    let base = (pc + 3) & !3;
    (((label as i64 - base as i64) >> 2) as i32 as u32 & 0xffff) as u16
}

/// The fixture's instructions, in order, with the pc each one sits at.
fn program() -> Vec<(u32, Inst)> {
    let mut at = CODE;
    let mut out = Vec::new();
    let mut push = |at: &mut u32, inst: Inst| {
        let pc = *at;
        *at += lp_xt_inst::encode(&inst).len() as u32;
        out.push((pc, inst));
    };
    push(&mut at, Inst::L32r(reg(2), l32r_field(CODE, LIT)));
    push(&mut at, Inst::L32iN(reg(3), reg(2), 0));
    push(&mut at, Inst::MoviN(reg(4), -9));
    push(&mut at, Inst::Rrr(AluRrr::And, reg(3), reg(3), reg(4)));
    push(&mut at, Inst::S32iN(reg(3), reg(2), 0));
    push(&mut at, Inst::Nullary(NullaryOp::Memw));
    // The second `l32r`'s field depends on where it lands, so it is computed
    // from the running pc rather than from a constant.
    let l32r_pc = at;
    push(&mut at, Inst::L32r(reg(5), l32r_field(l32r_pc, LIT + 4)));
    push(&mut at, Inst::Jx(reg(5)));
    out
}

/// The pc of the store that clears `pro_cache_enable` — what the stop's
/// message has to name.
fn disabling_store_pc() -> u32 {
    program()
        .into_iter()
        .find(|(_, i)| matches!(i, Inst::S32iN(..)))
        .expect("the fixture stores")
        .0
}

/// A machine with the fixture placed and the hart pointed at it, and the
/// PRO core's cache **on** — the state the bootloader hands over
/// (`loader::seed_cache_enabled`), so that clearing it is a real transition
/// with a cycle and a pc rather than the reset state.
fn fixture(policy: CacheOffPolicy) -> Machine {
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .strict(true)
        .cache_off_fetch(policy)
        .build()
        .expect("a machine with no app still has a bus and a ROM");

    let bus = machine.bus_mut();
    bus.load_image(LIT, &memmap::periph::DPORT.wrapping_add(0x40).to_le_bytes())
        .expect("SRAM0 holds the first literal");
    bus.load_image(LIT + 4, &TARGET.to_le_bytes())
        .expect("SRAM0 holds the second literal");
    for (at, inst) in program() {
        bus.load_image(at, &lp_xt_inst::encode(&inst))
            .expect("SRAM0 holds the code");
    }
    // Something to land on: a self-jump, so a `permit` run reaches its
    // deadline instead of decoding a zeroed window. `load_image` is the host
    // side and writes through a read-only region, exactly as a cache fill
    // will in P7.
    bus.load_image(TARGET, &lp_xt_inst::encode(&Inst::J(-4)))
        .expect("the IROM window takes a host-side write");

    loader::seed_cache_enabled(machine.cache());
    machine
        .seed_boot_state(CODE, BootFrame::at(memmap::ROM_PRO_STACK_TOP))
        .expect("a seeded hart");
    machine
}

/// The encoder is the oracle, and the decoder is the check on it: every
/// instruction the fixture places decodes back to the instruction the module
/// doc says it is, at the length the layout assumed.
#[test]
fn the_fixture_decodes_back_to_what_it_says_it_is() {
    let mut at = CODE;
    for (pc, inst) in program() {
        assert_eq!(
            pc, at,
            "the layout and the encoder agree on where {inst:?} sits"
        );
        let bytes = lp_xt_inst::encode(&inst);
        let (back, len) = lp_xt_inst::decode(&bytes).expect("the fixture decodes");
        assert_eq!(back, inst, "{inst:?} round-trips");
        assert_eq!(len, bytes.len(), "{inst:?} is {} bytes", bytes.len());
        at += len as u32;
    }
    // And the pcs the module doc quotes are these pcs.
    assert_eq!(disabling_store_pc(), 0x4008_901a);
    assert_eq!(at, 0x4008_9025);
}

/// **P4's acceptance for D4.** The stop fires on the fetch through the flash
/// window, names the core, the pc, the cycle, the window and the write that
/// disabled the cache, and exits 6.
#[test]
fn the_cache_off_fetch_stop_fires_and_names_the_write_that_disabled_the_cache() {
    let mut machine = fixture(CacheOffPolicy::Stop);
    let outcome = machine.run_until(&StopCondition::after_micros(10));

    let Outcome::CacheOffFetch {
        cycle,
        pc,
        ref symbol,
        access,
    } = outcome
    else {
        panic!("expected the cache-off stop, got {outcome:?}");
    };

    assert_eq!(access.core, 0);
    assert_eq!(access.addr, TARGET);
    assert_eq!(access.window, Window::Irom);
    assert!(access.fetch, "the trigger was the instruction fetch");
    assert_eq!(
        pc, TARGET,
        "the pc is the instruction that made the access, not the slice's start"
    );
    assert_eq!(
        access.disabled_by,
        Some(disabling_store_pc()),
        "the message names the store that cleared pro_cache_enable"
    );
    assert!(
        access.disabled_at > 0 && access.disabled_at <= cycle,
        "the cache went off before the fetch: disabled_at={} cycle={cycle}",
        access.disabled_at
    );
    // Eight instructions in the fixture, and the fetch is the ninth.
    assert!(
        cycle < 32,
        "the fixture is eight instructions long: {cycle}"
    );
    assert_eq!(outcome.exit_code(), 6);
    assert_eq!(outcome.cycle(), cycle);

    // The message says all of it, and says what it does not claim.
    let text = machine.cache_off_message(cycle, pc, &access);
    for fragment in [
        "CACHE-OFF FETCH  core=0",
        "fetch from IROM 0x400d1a2c",
        "pro_cache_ctrl.pro_cache_enable <- 0",
        "from pc=0x4008901a",
        "On silicon this core stalls until the cache returns",
        "`--cache-off-fetch permit` continues (and claims nothing about the stall).",
    ] {
        assert!(text.contains(fragment), "missing {fragment:?} in:\n{text}");
    }
    // The symbol is honest about there being none: the fixture is not in any
    // ELF, and a `~name+0x…` from the ROM would be a lie about a hole.
    assert!(
        symbol.is_none() || symbol.as_deref().is_some_and(|s| s.starts_with('~')),
        "an unnamed address is unnamed or explicitly nearest-match: {symbol:?}"
    );

    // The state behind the stop is exactly what the message quotes.
    let cache = machine.cache().lock().expect("cache");
    assert!(!cache.enabled(0));
    assert_eq!(cache.ctrl(0) & CACHE_ENABLE, 0);
    assert_eq!(cache.disabled(0).1, Some(disabling_store_pc()));
}

/// **`--cache-off-fetch permit` silences it.** Not "runs the check and drops
/// the answer": with nothing to report there is nothing to arm, so the same
/// fixture runs into its self-jump and reaches the deadline.
#[test]
fn permit_continues_through_the_same_fetch() {
    let mut machine = fixture(CacheOffPolicy::Permit);
    let outcome = machine.run_until(&StopCondition::after_micros(10));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "permit continues: {outcome:?}"
    );
    assert_eq!(outcome.exit_code(), 0);
    assert_eq!(
        machine.harts[0].pc(),
        TARGET,
        "the guest is spinning in the flash window, with the cache off"
    );
    assert!(
        !machine.cache().lock().expect("cache").enabled(0),
        "the cache really is off — permit changes what is reported, not what happened"
    );
}

/// The bootloader executes from SRAM0, which is **not** a flash window, so a
/// core running there with its cache off does not trigger. Same fixture, same
/// disabled cache, and the run reaches its deadline in SRAM0.
#[test]
fn running_from_sram0_with_the_cache_off_does_not_trigger() {
    let mut machine = fixture(CacheOffPolicy::Stop);
    // Replace the `jx` with a self-jump: everything up to it is identical,
    // including the store that turns the cache off.
    let jx_pc = program()
        .into_iter()
        .find(|(_, i)| matches!(i, Inst::Jx(_)))
        .expect("the fixture jumps")
        .0;
    machine
        .bus_mut()
        .load_image(jx_pc, &lp_xt_inst::encode(&Inst::J(-4)))
        .expect("SRAM0 takes it");

    let outcome = machine.run_until(&StopCondition::after_micros(10));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "SRAM0 is not a flash window: {outcome:?}"
    );
    assert!(
        !machine.cache().lock().expect("cache").enabled(0),
        "and the cache is off, which is what makes this the interesting case"
    );
    assert_eq!(machine.harts[0].pc(), jx_pc);
}

/// A **host-side** write into the window is not a guest access. P7's cache
/// fill goes through `SocBus::load_image`, and `Machine::peek_word` marks
/// itself; neither may arm the check.
#[test]
fn a_host_side_read_of_the_window_does_not_trigger() {
    let mut machine = fixture(CacheOffPolicy::Stop);
    // Put the core in the cache-off state without letting it reach the
    // window: the SRAM0-only variant above.
    let jx_pc = program()
        .into_iter()
        .find(|(_, i)| matches!(i, Inst::Jx(_)))
        .expect("the fixture jumps")
        .0;
    machine
        .bus_mut()
        .load_image(jx_pc, &lp_xt_inst::encode(&Inst::J(-4)))
        .expect("SRAM0 takes it");
    machine.run_until(&StopCondition::after_micros(10));
    assert!(!machine.cache().lock().expect("cache").enabled(0));

    // Now read and write the window from the host side, twice over.
    assert!(machine.peek_word(TARGET).is_some());
    machine
        .bus_mut()
        .load_image(TARGET + 16, &[0u8; 4])
        .expect("a host-side fill");
    assert!(machine.peek_word(memmap::DROM_BASE).is_some());

    let outcome = machine.run_until(&StopCondition::after_micros(20));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "a host-side access is the emulator, not the guest: {outcome:?}"
    );
}

/// Two runs of the same fixture stop at the same cycle with the same message.
#[test]
fn the_stop_is_deterministic() {
    let mut a = fixture(CacheOffPolicy::Stop);
    let mut b = fixture(CacheOffPolicy::Stop);
    let oa = a.run_until(&StopCondition::after_micros(10));
    let ob = b.run_until(&StopCondition::after_micros(10));
    assert_eq!(oa, ob);
    let (
        Outcome::CacheOffFetch {
            cycle, pc, access, ..
        },
        _,
    ) = (oa, ())
    else {
        panic!("both stopped");
    };
    assert_eq!(
        a.cache_off_message(cycle, pc, &access),
        b.cache_off_message(cycle, pc, &access)
    );
}
