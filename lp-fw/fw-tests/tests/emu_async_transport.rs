//! Regression test for the production `lp-cli ... emu` transport shape:
//! `release-emu` `fw-emu`, `TimeMode::RealTime`, and the background-thread
//! `create_emulator_serial_transport_pair` transport from
//! `lpa_client::transport_serial`.
//!
//! `tests/scene_render_emu.rs` proves the firmware server path with the
//! *synchronous* test transport in `TimeMode::Simulated`; nothing else
//! covers the async transport `lp-cli` actually uses. That gap hid a framing
//! regression (`docs/reports/2026-09-06-lp-riscv-emu-speed-probe.md` §4):
//! the async transport wrote bare JSON lines while the firmware's
//! `SerialTransport::receive` only accepts `M!`-prefixed lines, so every
//! client request was silently dropped and the first round-trip timed out.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use lp_emu_core::{LogLevel, TimeMode};
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    Riscv32Emulator,
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;
use lpa_client::TokioLpClient;
use lpa_client::transport_serial::{BacktraceInfo, create_emulator_serial_transport_pair};
use lpc_model::AsLpPath;

/// Generous wall-clock bound for the first round-trip. Boot in the host
/// interpreter is well under this even on a loaded machine; the failure mode
/// this guards against is *no response ever*, not slowness.
const FIRST_ROUND_TRIP_BOUND: Duration = Duration::from_secs(60);

#[tokio::test]
#[test_log::test]
async fn async_realtime_transport_first_round_trip() {
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true),
    )
    .expect("Failed to build fw-emu");

    let elf_data = std::fs::read(&fw_emu_path).expect("Failed to read fw-emu ELF");
    let load_info = load_elf(&elf_data).expect("Failed to load ELF");
    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::None)
        .with_time_mode(TimeMode::RealTime)
        .with_allow_unaligned_access(true);
    let sp_value = 0x8000_0000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);
    emulator.set_pc(load_info.entry_point);

    let backtrace_info = BacktraceInfo {
        symbol_map: load_info.symbol_map.clone(),
        code_end: load_info.code_end,
    };
    let transport =
        create_emulator_serial_transport_pair(Arc::new(Mutex::new(emulator)), Some(backtrace_info))
            .expect("Failed to create emulator serial transport");
    let client = TokioLpClient::new(Box::new(transport));

    // A trivial file write: no project load, no JIT — just one request and
    // one response over the async transport.
    let write = client.fs_write(
        "/projects/roundtrip/hello.txt".as_path(),
        b"hello\n".to_vec(),
    );
    tokio::time::timeout(FIRST_ROUND_TRIP_BOUND, write)
        .await
        .expect(
            "first client round-trip over the async RealTime emulator transport never completed",
        )
        .expect("fs_write failed");

    // And a second one, so we know the transport keeps working past boot.
    let write = client.fs_write(
        "/projects/roundtrip/again.txt".as_path(),
        b"again\n".to_vec(),
    );
    tokio::time::timeout(FIRST_ROUND_TRIP_BOUND, write)
        .await
        .expect("second client round-trip never completed")
        .expect("second fs_write failed");
}
