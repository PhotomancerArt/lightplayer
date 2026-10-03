//! **`--strict-bus` does not call a store-published word a missing fence.**
//!
//! The S3 publishes JIT code by **store**, the way the classic does
//! (`lp-emu-esp32v3`'s `dual_core.rs` has the classic's twin of this test):
//! the product writes a shader into the `esp_alloc` heap through SRAM1's D-bus
//! view and calls it through the I-bus alias `+0x6F_0000`
//! (`lp-shader/lpvm-native/src/exec_addr.rs`), and on Xtensa
//! `JitBuffer::from_code` issues **no barrier at all** — its `fence.i` is
//! `#[cfg(target_arch = "riscv32")]` (`lp-shader/lpvm-native/src/rt_jit/buffer.rs`).
//! Internal SRAM has no cache in front of it on this chip: the ICache and
//! DCache serve the flash/PSRAM windows the MMU table maps
//! (`crate::cache`), not SRAM1. Silicon renders the shader-oracle project
//! correctly on that path (`projects/test/shader-oracle/README.md`,
//! "Known-good", 2026-07-30, XIAO ESP32-S3 Plus: 192 of 192 bytes). And the
//! machine's own invalidation keys off the store, not off an `isync`.
//!
//! So the bus's RV32 missing-fence checker, which waits for a `fence.i` this
//! chip never owes (and which the Xtensa `isync` does not reach), is not armed
//! here. The same fixture with the RV32 contract armed by hand does report:
//! that leg is what says the fixture really executes a word the guest
//! rewrote on a page it had already run from, and that the zero is not a
//! fixture that never reaches the checker.

use lp_emu_esp32s3::machine::{BootFrame, Esp32S3Builder, Machine, Outcome, StopCondition};
use lp_emu_esp32s3::memmap;
use lp_xt_inst::{Inst, Reg, StoreOp, encode};

/// The driver, fetched through the SRAM1 **I-bus** view like every JIT'd
/// shader (and see `boot.rs`'s `CODE` for why a test's code lives there).
const CODE: u32 = 0x4037_9000;
/// The stub the driver runs, rewrites and runs again — on [`CODE`]'s 4 KiB
/// page, which the guest has executed from by the time it is rewritten.
const STUB: u32 = CODE + 0x100;
/// [`STUB`]'s D-bus address: the door the product's JIT writes through.
const STUB_DBUS: u32 = STUB - memmap::SRAM1_IBUS_OFFSET;
/// Where the driver ends.
const DONE: u32 = CODE + 12;

#[test]
fn strict_bus_does_not_report_a_store_published_word_as_a_missing_fence() {
    for fence_contract in [false, true] {
        let mut m = fixture();
        if fence_contract {
            m.bus_mut().set_fence_contract(true);
        }
        let outcome = m.run_until(&StopCondition {
            stop_cycle: Some(100_000),
            ..Default::default()
        });
        assert!(
            matches!(outcome, Outcome::Breakpoint { .. }),
            "the driver reached its break; got {outcome:?} (first violation {:?})",
            m.first_strict_violation()
        );
        assert_eq!(
            m.harts[0].cpu().a(4),
            222,
            "the second call ran the bytes written through the D-bus view"
        );
        let reports = m.bus().missing_fence_reports();
        if fence_contract {
            assert!(
                reports >= 1,
                "armed by hand, the RV32 checker sees the store-published word, \
                 or this fixture is not testing the checker"
            );
        } else {
            assert_eq!(reports, 0, "a store into SRAM1 is the publish on this chip");
        }
    }
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

/// A strict machine with no application: the driver calls the stub, stores
/// one word over it through the D-bus view — no `memw`, no `isync`, which is
/// what the product's S3 publish emits — and calls it again.
fn fixture() -> Machine {
    let mut m = Esp32S3Builder::new()
        .strict(true)
        .build()
        .expect("a machine with the vendored ROM and no application");
    let driver: Vec<u8> = [
        Inst::Jx(a(5)),                            // CODE+0: run the stub
        Inst::Store(StoreOp::S32i, a(7), a(6), 0), // CODE+3: publish by store
        Inst::Addi(a(3), a(3), 9),                 // CODE+6: next return is DONE
        Inst::Jx(a(5)),                            // CODE+9: run it again
        Inst::Break(1, 15),                        // CODE+12
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
