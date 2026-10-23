//! **The block cache hears a store through either door of SRAM1, and only a
//! store that touches cached code.**
//!
//! The product publishes a JIT'd shader by storing it into the heap through
//! SRAM1's **D-bus** view and calling it through the **I-bus** alias
//! `+0x6F_0000` (`lp-shader/lpvm-native/src/exec_addr.rs`), with no barrier
//! (`fence_contract.rs` has the whole story). The bus records the store at
//! its canonical address — the D-bus one (DD81) — while the hart keys the
//! blocks it decoded by the `pc` it fetched through, the I-bus one. So the
//! record has to name both doors, or a shader rewritten in place keeps
//! running its old bytes out of the cache.
//!
//! `fence_contract.rs` cannot see that: its machine has both caches off, D4's
//! watch is armed, and with a memory-cost model installed the hart decodes no
//! block at all. This fixture turns both caches on first, so the stub really
//! is served from a cached block when it is rewritten.
//!
//! The other half is the speed contract (2026-10-22): a store into SRAM1 that
//! touches no cached instruction — every stack push and heap write on this
//! chip, because the whole of SRAM1 is one executable region — must not reach
//! the cache as an invalidation at all.

use lp_emu_esp32s3::cache::{CACHE_ENABLE, Which};
use lp_emu_esp32s3::machine::{BootFrame, Esp32S3Builder, Machine, Outcome, StopCondition};
use lp_emu_esp32s3::memmap;
use lp_xt_inst::{Inst, Reg, StoreOp, encode};

/// The driver, fetched through the SRAM1 **I-bus** view like every JIT'd
/// shader.
const CODE: u32 = 0x4037_9000;
/// The stub the driver runs, rewrites and runs again.
const STUB: u32 = CODE + 0x100;
/// [`STUB`]'s D-bus address: the door the product's JIT writes through.
const STUB_DBUS: u32 = STUB - memmap::SRAM1_IBUS_OFFSET;
/// A data word on the same SRAM1 page, nowhere near any instruction: what a
/// stack push looks like to the bus.
const DATA_DBUS: u32 = CODE + 0x800 - memmap::SRAM1_IBUS_OFFSET;
/// Where the driver ends.
const DONE: u32 = CODE + 18;

#[test]
fn a_stub_rewritten_through_the_dbus_door_runs_its_new_bytes() {
    let mut m = fixture();
    let outcome = m.run_until(&StopCondition {
        stop_cycle: Some(100_000),
        ..Default::default()
    });
    assert!(
        matches!(outcome, Outcome::Breakpoint { .. }),
        "the driver reached its break; got {outcome:?}"
    );
    let stats = m.harts[0]
        .block_stats()
        .expect("the hart ran out of its block cache — this fixture tests nothing without it");
    assert!(
        stats.hits + stats.decodes > 0,
        "blocks were decoded and served: {stats:?}"
    );
    assert_eq!(
        m.harts[0].cpu().a(4),
        222,
        "the second call ran the bytes written through the D-bus view, not the cached block \
         decoded at the I-bus address ({stats:?})"
    );
}

#[test]
fn a_data_store_beside_cached_code_does_not_reach_the_cache() {
    // Point the publish at a data word instead, so the stub is never
    // rewritten and the second call runs the first stub again. The control
    // stores the bytes already there, which no version of the record has
    // ever counted; the other run changes them. The fixture's own host-side
    // loads reach the cache identically in both, so any difference between
    // the two is the data store.
    let run = |value: u32| {
        let mut m = fixture();
        m.harts[0].cpu_mut().set_a(6, DATA_DBUS);
        m.harts[0].cpu_mut().set_a(7, value);
        let outcome = m.run_until(&StopCondition {
            stop_cycle: Some(100_000),
            ..Default::default()
        });
        assert!(
            matches!(outcome, Outcome::Breakpoint { .. }),
            "the driver reached its break; got {outcome:?}"
        );
        assert_eq!(m.harts[0].cpu().a(4), 111, "the stub was not rewritten");
        assert_eq!(m.peek_word(DATA_DBUS), Some(value), "the data store landed");
        m.harts[0].block_stats().expect("a block cache")
    };
    let control = run(0);
    let changed = run(0x5a5a_5a5a);
    assert_eq!(
        changed.range_entries_dropped, 0,
        "a store that touches no cached instruction drops nothing: {changed:?}"
    );
    assert_eq!(
        changed.range_invalidations, control.range_invalidations,
        "…and is not handed to the cache at all: {changed:?} against {control:?}"
    );
}

fn a(n: u8) -> Reg {
    Reg::new(n)
}

/// `movi a4, <marker>; jx a3`, padded to two words.
fn stub(marker: i32) -> Vec<u8> {
    let mut bytes: Vec<u8> = [Inst::Movi(a(4), marker), Inst::Jx(a(3))]
        .iter()
        .flat_map(encode)
        .collect();
    bytes.resize(8, 0);
    bytes
}

/// A machine with no application and **both caches on**: the driver calls
/// the stub, stores one word through the D-bus view (over the stub, unless a
/// test points `a6` elsewhere), and calls it again.
fn fixture() -> Machine {
    let mut m = Esp32S3Builder::new()
        .build()
        .expect("a machine with the vendored ROM and no application");
    for which in [Which::ICache, Which::DCache] {
        let at = memmap::periph::EXTMEM + which.ctrl_offset();
        let word = m.peek_word(at).unwrap_or(0);
        assert!(
            m.poke_word(at, word | CACHE_ENABLE),
            "enabling the {which:?}"
        );
    }
    let driver: Vec<u8> = [
        Inst::Jx(a(5)),                            // CODE+0: run the stub
        Inst::Store(StoreOp::S32i, a(7), a(6), 0), // CODE+3: publish by store
        Inst::Addi(a(3), a(3), 9),                 // CODE+6: next return is CODE+12
        Inst::Jx(a(5)),                            // CODE+9: run it again
        Inst::Nullary(lp_xt_inst::NullaryOp::Nop), // CODE+12
        Inst::Nullary(lp_xt_inst::NullaryOp::Nop), // CODE+15
        Inst::Break(1, 15),                        // CODE+18
    ]
    .iter()
    .flat_map(encode)
    .collect();
    m.bus_mut().load_image(CODE, &driver).expect("the driver");
    m.bus_mut().load_image(STUB, &stub(111)).expect("the stub");
    m.seed_boot_state(CODE, BootFrame::rom_pro_stack(m.rom()))
        .expect("seeding the boot state");
    m.break_at_address(DONE)
        .expect("claiming the driver's break");
    let new = stub(222);
    let cpu = m.harts[0].cpu_mut();
    cpu.set_a(3, CODE + 3);
    cpu.set_a(5, STUB);
    cpu.set_a(6, STUB_DBUS);
    cpu.set_a(7, u32::from_le_bytes([new[0], new[1], new[2], new[3]]));
    m
}
