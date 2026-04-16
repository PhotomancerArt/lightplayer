//! Integration test: verify stack unwinding works in the RISC-V emulator.
//!
//! Builds fw-emu with the test_unwind feature, sends a magic command, and asserts
//! that catch_unwind successfully catches a panic. This validates the unwinding
//! infrastructure (.eh_frame, personality, landing pads) without needing ESP32 hardware.

use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    LogLevel, Riscv32Emulator, TimeMode,
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;

/// Validates that stack unwinding works in the RISC-V emulator (catch_unwind catches a panic).
///
/// The guest firmware calls `panic!()` and catches it with `catch_unwind`, validating the
/// full unwinding infrastructure: `.eh_frame`, personality routines, LSDA landing pads, and
/// unwinding through `core::panicking` frames (rebuilt with `panic=unwind` via `build-std`).
#[test]
fn test_unwind_caught_in_emulator() {
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true)
            .with_unwind_support(true)
            .with_build_std(true)
            .with_features(&["test_unwind"]),
    )
    .expect("Failed to build fw-emu with test_unwind");

    let elf_data = std::fs::read(&fw_emu_path).expect("Failed to read fw-emu ELF");
    let load_info = load_elf(&elf_data).expect("Failed to load ELF");

    let ram_size = load_info.ram.len();
    let mut emu = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::None)
        .with_time_mode(TimeMode::Simulated(0))
        .with_allow_unaligned_access(true);

    let sp_value = 0x80000000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emu.set_register(Gpr::Sp, sp_value as i32);
    emu.set_pc(load_info.entry_point);

    emu.serial_write(b"__test_unwind\n");
    emu.run_until_yield(50_000_000).unwrap_or_else(|e| {
        println!("{}", emu.dump_state());
        panic!("Emulator error: {:?}", e);
    });

    let output = emu.serial_read_line();
    assert_eq!(
        output, "unwind: ok",
        "catch_unwind should have caught the panic; got: {:?}",
        output
    );
}
