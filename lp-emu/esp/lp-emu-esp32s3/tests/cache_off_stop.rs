//! **D4** — the cache-off fetch stop, on a hand-built fixture, with the
//! **S3's** polarity.
//!
//! The fixture is nine instructions in the SRAM1 I-bus view that do what the
//! ROM's `Cache_Disable_ICache` does and then jump into the IROM window:
//!
//! ```text
//! 40379000:  .word 0x600c4060       ; &EXTMEM.icache_ctrl
//! 40379004:  .word 0x42050020       ; somewhere in the IROM window
//! 40379010:  l32r    a2, 0x40379000
//! 40379013:  l32i.n  a3, a2, 0
//! 40379015:  movi.n  a4, -2         ; ~ICACHE_ENABLE, the ROM's own constant
//! 40379017:  and     a3, a3, a4     ;   (`4004f2be: movi.n a2, -2`, Cache_Disable_ICache)
//! 4037901a:  s32i.n  a3, a2, 0      ; the write the stop's message names
//! 4037901c:  memw
//! 4037901f:  l32r    a5, 0x40379004
//! 40379022:  jx      a5             ; → 0x42050020, through the flash window
//! ```
//!
//! ⚠️ **`1` means ON on this chip.** `icache_ctrl.icache_enable` bit 0 is
//! "0: disable, 1: enable" (the PAC, and the ROM's `Cache_Enable_ICache`
//! `4004f315: or a8, a8, 1`); the C6's `l1_icache_shut_ibus0` is the other
//! way round. So the fixture *clears* the bit to turn the cache off, and
//! [`the_stop_does_not_fire_while_the_cache_is_on`] is the test that a watch
//! copied from the C6 would fail — it would arm on every access while the
//! bit is set.
//!
//! ⚠️ **Not built by running the real firmware into the condition.** A test
//! that depends on the firmware reaching a bug is not a test. It is also not
//! hand-encoded — M0 found a real misdecode in this repo that came from
//! trusting an encoding by analogy — so the bytes come from
//! `lp_xt_inst::encode`, the repo's own encoder, and
//! [`the_fixture_decodes_back_to_what_it_says_it_is`] holds it to that.
//!
//! The code lives in the SRAM1 **I-bus** view (`tests/boot.rs` says why: a
//! windowed call cannot cross a 1 GiB region, and `jx` into `0x42…` from
//! `0x4037…` stays in one), which also means the fixture is fetched through
//! the RAM alias M6 P02 exists for.

use lp_emu_esp32s3::cache::{CACHE_ENABLE, CacheOffPolicy, Which, Window};
use lp_emu_esp32s3::machine::{BootFrame, BootMode, Esp32S3Builder, Machine, Outcome, StopCondition};
use lp_emu_esp32s3::memmap;
use lp_xt_inst::{AluRrr, Inst, NullaryOp, Reg};

/// Where the fixture's two literals live: SRAM1 through the I-bus door,
/// above the vectors and clear of anything the ROM seeds (`tests/boot.rs`
/// puts its code at the same address for the same reason).
const LIT: u32 = 0x4037_9000;
/// Where its code lives.
const CODE: u32 = 0x4037_9010;
/// The instruction the `jx` lands on — an address inside the IROM window
/// (the shipped image's `.text` starts here), which on this machine is a
/// RAM region behind the window and on silicon is served by the ICache.
const TARGET: u32 = 0x4205_0020;
/// A data address inside the DROM window, for the DCache half.
const DROM_TARGET: u32 = 0x3C00_0140;

/// `EXTMEM.icache_ctrl`, whose bit 0 is the ICache's enable.
const ICACHE_CTRL: u32 = memmap::periph::EXTMEM + 0x60;
/// `EXTMEM.dcache_ctrl`, whose bit 0 is the DCache's enable.
const DCACHE_CTRL: u32 = memmap::periph::EXTMEM;

fn reg(n: u8) -> Reg {
    Reg::new(n)
}

/// `l32r`'s raw 16-bit field: `(label - ((pc + 3) & !3)) / 4`, backward only.
fn l32r_field(pc: u32, label: u32) -> u16 {
    let base = (pc + 3) & !3;
    (((label as i64 - base as i64) >> 2) as i32 as u32 & 0xffff) as u16
}

/// What the fixture does after clearing the bit: jump into the IROM window
/// (a fetch), or load a word from the DROM window (a data read).
#[derive(Clone, Copy)]
enum Then {
    JumpIntoIrom,
    LoadFromDrom,
}

/// The fixture's instructions, in order, with the pc each one sits at.
///
/// `mask` is what is ANDed into the control word: `-2` clears the enable
/// bit (the ROM's constant), `-1` leaves it alone — the "cache stays on"
/// control.
fn program(mask: i32, then: Then) -> Vec<(u32, Inst)> {
    let mut at = CODE;
    let mut out = Vec::new();
    let mut push = |at: &mut u32, inst: Inst| {
        let pc = *at;
        *at += lp_xt_inst::encode(&inst).len() as u32;
        out.push((pc, inst));
    };
    push(&mut at, Inst::L32r(reg(2), l32r_field(CODE, LIT)));
    push(&mut at, Inst::L32iN(reg(3), reg(2), 0));
    push(&mut at, Inst::MoviN(reg(4), mask));
    push(&mut at, Inst::Rrr(AluRrr::And, reg(3), reg(3), reg(4)));
    push(&mut at, Inst::S32iN(reg(3), reg(2), 0));
    push(&mut at, Inst::Nullary(NullaryOp::Memw));
    // The second `l32r`'s field depends on where it lands, so it is computed
    // from the running pc rather than from a constant.
    let l32r_pc = at;
    push(&mut at, Inst::L32r(reg(5), l32r_field(l32r_pc, LIT + 4)));
    match then {
        Then::JumpIntoIrom => push(&mut at, Inst::Jx(reg(5))),
        Then::LoadFromDrom => {
            // The load the stop names, then a self-jump so a `permit` run
            // reaches its deadline rather than running off the end.
            push(&mut at, Inst::L32iN(reg(6), reg(5), 0));
            push(&mut at, Inst::J(-4));
        }
    }
    out
}

/// The pc of the store that writes the control register — what the stop's
/// message has to name when the store cleared the enable.
fn store_pc(mask: i32, then: Then) -> u32 {
    program(mask, then)
        .into_iter()
        .find(|(_, i)| matches!(i, Inst::S32iN(..)))
        .expect("the fixture stores")
        .0
}

/// The pc of the access that reaches the window.
fn access_pc(mask: i32, then: Then) -> u32 {
    let p = program(mask, then);
    match then {
        Then::JumpIntoIrom => TARGET,
        Then::LoadFromDrom => {
            p.iter()
                .find(|(_, i)| matches!(i, Inst::L32iN(r, ..) if *r == reg(6)))
                .expect("the fixture loads")
                .0
        }
    }
}

/// A machine with the fixture placed and the hart pointed at it, and **both
/// caches on** — the state the bootloader hands over (loader item 11) — so
/// that clearing a bit is a real transition with a cycle and a pc rather
/// than the reset state.
fn fixture(policy: CacheOffPolicy, mask: i32, then: Then) -> Machine {
    let mut machine = Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .strict(true)
        .cache_off_fetch(policy)
        .build()
        .expect("a machine with no app still has a bus and a ROM");

    let ctrl = match then {
        Then::JumpIntoIrom => ICACHE_CTRL,
        Then::LoadFromDrom => DCACHE_CTRL,
    };
    let target = match then {
        Then::JumpIntoIrom => TARGET,
        Then::LoadFromDrom => DROM_TARGET,
    };
    let bus = machine.bus_mut();
    bus.load_image(LIT, &ctrl.to_le_bytes())
        .expect("SRAM1 holds the first literal");
    bus.load_image(LIT + 4, &target.to_le_bytes())
        .expect("SRAM1 holds the second literal");
    for (at, inst) in program(mask, then) {
        bus.load_image(at, &lp_xt_inst::encode(&inst))
            .expect("SRAM1 holds the code");
    }
    // Something to land on: a self-jump, so a `permit` run reaches its
    // deadline instead of decoding a zeroed window. `load_image` is the host
    // side and writes through the window's RAM, exactly as a cache fill does.
    bus.load_image(TARGET, &lp_xt_inst::encode(&Inst::J(-4)))
        .expect("the IROM window takes a host-side write");
    bus.load_image(DROM_TARGET, &0xC0FF_EE00u32.to_le_bytes())
        .expect("the DROM window takes a host-side write");

    // Item 11, through the bus, so the `EXTMEM` view and the model agree —
    // the same door `Machine::seed_cache_enabled` uses on a direct load.
    for at in [ICACHE_CTRL, DCACHE_CTRL] {
        let word = machine.peek_word(at).unwrap_or(0);
        assert!(machine.poke_word(at, word | CACHE_ENABLE));
    }
    {
        let cache = machine.cache().lock().expect("cache");
        assert!(cache.enabled(Which::ICache), "seeded on");
        assert!(cache.enabled(Which::DCache), "seeded on");
    }
    machine
        .seed_boot_state(CODE, BootFrame::at(memmap::ROM_PRO_STACK_TOP))
        .expect("the hart seeds at the fixture");
    machine
}

fn run(m: &mut Machine) -> Outcome {
    m.run_until(&StopCondition::after_micros(1_000))
}

/// The bytes the fixture is made of decode back to the instructions it
/// claims to be, one by one, at the pcs it claims — the encoder and the
/// decoder agree, so a misencoding cannot hide inside a passing test.
#[test]
fn the_fixture_decodes_back_to_what_it_says_it_is() {
    for then in [Then::JumpIntoIrom, Then::LoadFromDrom] {
        for (pc, inst) in program(-2, then) {
            let bytes = lp_xt_inst::encode(&inst);
            let (back, len) = lp_xt_inst::decode(&bytes).expect("the fixture decodes");
            assert_eq!(len, bytes.len(), "{inst:?} at {pc:#010x}");
            assert_eq!(back, inst, "at {pc:#010x}");
        }
    }
    // And the ROM's own constant is the one the fixture uses: -2 is
    // `~ICACHE_ENABLE` for a bit-0 enable.
    assert_eq!((-2i32) as u32, !CACHE_ENABLE);
}

/// **D4 on this chip.** A fetch through the IROM window after the guest
/// cleared `icache_enable` is a strict stop, exit code **6**, naming the
/// fetch, the cache, the cycle the cache went away and the store that did it.
#[test]
fn a_fetch_through_the_irom_window_with_the_icache_off_is_a_stop() {
    let mut m = fixture(CacheOffPolicy::Stop, -2, Then::JumpIntoIrom);
    let outcome = run(&mut m);
    let Outcome::CacheOffFetch {
        cycle,
        pc,
        access,
        ..
    } = outcome.clone()
    else {
        panic!("expected D4's stop, got {outcome:?}");
    };
    assert_eq!(outcome.exit_code(), 6, "the cross-machine contract");
    assert_eq!(pc, TARGET, "the instruction that made the access is the one at the target");
    assert_eq!(access.addr, TARGET);
    assert_eq!(access.window, Window::Irom);
    assert_eq!(access.cache, Which::ICache, "the IROM window is the ICache's");
    assert!(access.fetch, "a fetch, not a data read");
    assert_eq!(
        access.disabled_by,
        Some(store_pc(-2, Then::JumpIntoIrom)),
        "the store that cleared the bit"
    );
    assert!(access.disabled_at < cycle, "the cache went away before the fetch");
    assert!(access.disabled_at > 0, "and not at reset: it was a transition");

    // The register and the model agree about the state the stop reports.
    assert!(!m.cache().lock().expect("cache").enabled(Which::ICache));
    assert_eq!(
        m.peek_word(ICACHE_CTRL).expect("mapped") & CACHE_ENABLE,
        0,
        "icache_enable is clear"
    );
    assert!(
        m.cache().lock().expect("cache").enabled(Which::DCache),
        "the other cache was never touched"
    );
    assert!(
        m.first_strict_violation().is_none(),
        "D4 is its own outcome, not a strict-bus refusal"
    );

    let message = m.cache_off_message(cycle, pc, &access);
    println!("{message}");
    assert!(message.starts_with("CACHE-OFF FETCH  core=0"));
    assert!(message.contains(&format!("pc={TARGET:#010x}")));
    assert!(message.contains("fetch from IROM"));
    assert!(message.contains("served by the ICache"));
    assert!(message.contains("EXTMEM+0x060 (icache_ctrl.icache_enable <- 0)"));
    assert!(message.contains(&format!("from pc={:#010x}", store_pc(-2, Then::JumpIntoIrom))));
    assert!(message.contains("`--cache-off-fetch permit` continues"));
}

/// The DCache half: a **data read** from the DROM window with
/// `dcache_enable` clear is the same stop, naming the DROM window, the
/// DCache and `read` rather than `fetch`.
#[test]
fn a_read_from_the_drom_window_with_the_dcache_off_is_a_stop() {
    let mut m = fixture(CacheOffPolicy::Stop, -2, Then::LoadFromDrom);
    let outcome = run(&mut m);
    let Outcome::CacheOffFetch { pc, access, .. } = outcome.clone() else {
        panic!("expected D4's stop, got {outcome:?}");
    };
    assert_eq!(outcome.exit_code(), 6);
    assert_eq!(pc, access_pc(-2, Then::LoadFromDrom), "the load instruction");
    assert_eq!(access.addr, DROM_TARGET);
    assert_eq!(access.window, Window::Drom);
    assert_eq!(access.cache, Which::DCache, "the DROM window is the DCache's");
    assert!(!access.fetch, "a data read");
    assert_eq!(access.disabled_by, Some(store_pc(-2, Then::LoadFromDrom)));
    assert!(m.cache().lock().expect("cache").enabled(Which::ICache), "untouched");
    let message = m.cache_off_message(outcome.cycle(), pc, &access);
    assert!(message.contains("read from DROM"));
    assert!(message.contains("EXTMEM+0x000 (dcache_ctrl.dcache_enable <- 0)"));
}

/// **The polarity test.** The same fixture with a mask that leaves the bit
/// alone — the cache stays **on** — reaches its deadline through the window:
/// no stop, no strict violation. A watch that copied the C6's predicate
/// (bit set = shut) would fire here on the first fetch.
#[test]
fn the_stop_does_not_fire_while_the_cache_is_on() {
    for then in [Then::JumpIntoIrom, Then::LoadFromDrom] {
        let mut m = fixture(CacheOffPolicy::Stop, -1, then);
        let outcome = run(&mut m);
        assert!(
            matches!(outcome, Outcome::Deadline { .. }),
            "the cache is on (bit 0 set), so the window is served: {outcome:?}"
        );
        assert_eq!(outcome.exit_code(), 0);
        assert!(m.first_strict_violation().is_none());
        assert_eq!(
            m.peek_word(ICACHE_CTRL).expect("mapped") & CACHE_ENABLE,
            CACHE_ENABLE,
            "icache_enable is still set: 1 means ON on this chip"
        );
        assert_eq!(m.peek_word(DCACHE_CTRL).expect("mapped") & CACHE_ENABLE, CACHE_ENABLE);
        let cache = m.cache().lock().expect("cache");
        assert!(cache.enabled(Which::ICache) && cache.enabled(Which::DCache));
        assert!(!cache.watch_wanted(), "nothing is off, so nothing is armed");
    }
}

/// `--cache-off-fetch permit`: the same fixture, the same cleared bit, and
/// the run continues to its deadline — spinning on the self-jump behind the
/// window — with the register still saying the cache is off.
#[test]
fn permit_continues_through_the_window_and_claims_nothing() {
    let mut m = fixture(CacheOffPolicy::Permit, -2, Then::JumpIntoIrom);
    let outcome = run(&mut m);
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "permit does not check at all: {outcome:?}"
    );
    assert_eq!(outcome.exit_code(), 0);
    assert_eq!(m.harts[0].pc(), TARGET, "spinning on the self-jump behind the window");
    assert!(
        !m.cache().lock().expect("cache").enabled(Which::ICache),
        "the cache really is off; permit just does not stop"
    );
    assert!(
        !m.cache().lock().expect("cache").watch_wanted(),
        "and nothing is armed under permit"
    );
}

/// Turning the cache back on disarms the watch: a fixture that clears the
/// bit, sets it again and *then* reaches the window is not stopped.
#[test]
fn re_enabling_the_cache_disarms_the_watch() {
    let mut m = fixture(CacheOffPolicy::Stop, -2, Then::JumpIntoIrom);
    // Run only the first six instructions (through the `memw`): the bit is
    // clear and the watch is armed, but nothing has reached a window.
    let store = store_pc(-2, Then::JumpIntoIrom);
    let outcome = m.run_until(&StopCondition {
        stop_cycle: Some(7),
        ..Default::default()
    });
    assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
    assert!(m.harts[0].pc() > store, "past the store");
    assert!(m.cache().lock().expect("cache").watch_wanted(), "armed while off");

    // The guest turns it back on (here: the host writing through the same
    // register, as the ROM's `Cache_Enable_ICache` would).
    let word = m.peek_word(ICACHE_CTRL).expect("mapped");
    assert!(m.poke_word(ICACHE_CTRL, word | CACHE_ENABLE));
    assert!(!m.cache().lock().expect("cache").watch_wanted(), "disarmed once on");

    let outcome = run(&mut m);
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "the window is served again: {outcome:?}"
    );
    assert_eq!(m.harts[0].pc(), TARGET);
}
