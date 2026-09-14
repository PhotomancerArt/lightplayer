//! **The rule about which runs translate** (M7 P07, XD4/XD10).
//!
//! Three facts, each a test rather than a comment:
//!
//! 1. **Nothing translates unless `--jit` asked**, on either boot path. The
//!    translator is off by default natively, for the reason JD24 gives on the
//!    C6: cranelift needs minutes per image, so the interpreter stays the
//!    native default and `--jit` is an identity door, never a speed one.
//! 2. **`BootMode::RomUp` keeps translation off even with `--jit`** (XD10, and
//!    XD4 is its counterpart for the block cache, which stays *on* there). The
//!    ROM and the second-stage bootloader put the image in place with guest
//!    stores, so a core installed before they run holds blocks of bytes that
//!    are about to be overwritten. P07's publish-by-store event could now
//!    answer that — it is exactly the "the guest rewrote this" case — but
//!    whether ROM-up should translate is Q5's to schedule and this plan does
//!    not move it. What the emulator must not do is translate ROM-up
//!    *quietly*.
//! 3. **`--trace` refuses the core**, so a traced run is always the
//!    interpreter's. The trace is the interpreter's own instruction-by-
//!    instruction reading and a translated stay cannot emit it. ⚠️ This is
//!    why the CI identity cell (XD14) does **not** use a `--trace` window on
//!    the classic: a `--jit --trace` leg installs no core, and comparing it
//!    against `--interpreter` would compare a run against itself and pass.
//!    See `scripts/emu/xt-jit-identity-image.sh`, which refuses one.
//!
//! (2) and (3) need the shipped image, because a direct load needs an app;
//! they are `#[ignore]`d and run by `just test-emu-esp32v3-boot`.

use lp_emu_esp32v3::machine::{AppSource, BootMode, Esp32V3Builder, Machine};
use lp_emu_esp32v3::test_support::{fw_esp32v3_image, skip_notice};

fn installed(m: &Machine) -> Vec<bool> {
    m.harts.iter().map(|h| h.has_translated_core()).collect()
}

/// A ROM-up machine, whatever `--jit` says, holds no translated core.
#[test]
fn rom_up_never_installs_a_core() {
    for jit in [false, true] {
        let m = Esp32V3Builder::new()
            .boot_mode(BootMode::RomUp)
            .jit(jit)
            .build()
            .expect("the vendored ROM builds a machine");
        assert_eq!(
            installed(&m),
            vec![false, false],
            "ROM-up translated with --jit={jit}"
        );
        assert_eq!(m.jit_retranslations(), 0, "no core, no event");
    }
}

/// A direct load with no `--jit` holds no translated core either: the
/// interpreter is the native default.
#[test]
#[ignore = "needs the shipped image; `just test-emu-esp32v3-boot`"]
fn a_direct_load_without_the_flag_never_installs_a_core() {
    let test = "a_direct_load_without_the_flag_never_installs_a_core";
    let Ok(elf) = fw_esp32v3_image() else {
        skip_notice(test, "the shipped image is not built");
        return;
    };
    let m = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .build()
        .expect("a direct load of the shipped image");
    assert_eq!(installed(&m), vec![false, false]);
}

/// A direct load **with** `--jit` installs on core 0 at the boot event; core
/// 1 waits for the DPORT release, which hands it a fresh hart and therefore
/// its own event.
#[cfg(feature = "jit")]
#[test]
#[ignore = "needs the shipped image and pays cranelift; `just test-emu-esp32v3-boot`"]
fn a_direct_load_with_the_flag_installs_on_core_zero_at_boot() {
    let test = "a_direct_load_with_the_flag_installs_on_core_zero_at_boot";
    let Ok(elf) = fw_esp32v3_image() else {
        skip_notice(test, "the shipped image is not built");
        return;
    };
    let m = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .jit(true)
        // Bounded hard: what is under test is *which* runs install, and the
        // whole image is 140,282 blocks and 150 seconds of cranelift.
        .jit_blocks(256)
        .build()
        .expect("a direct load of the shipped image");
    assert_eq!(
        installed(&m),
        vec![true, false],
        "core 0 at boot; core 1 at the DPORT release"
    );
}

/// A traced direct load with `--jit` installs nothing, and says so.
#[cfg(feature = "jit")]
#[test]
#[ignore = "needs the shipped image; `just test-emu-esp32v3-boot`"]
fn a_traced_run_refuses_the_core() {
    let test = "a_traced_run_refuses_the_core";
    let Ok(elf) = fw_esp32v3_image() else {
        skip_notice(test, "the shipped image is not built");
        return;
    };
    let m = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .jit(true)
        .jit_blocks(256)
        .trace(Box::new(std::io::sink()), Vec::new())
        .build()
        .expect("a direct load of the shipped image");
    assert_eq!(
        installed(&m),
        vec![false, false],
        "a --trace run is the interpreter's, whatever --jit said"
    );
}
