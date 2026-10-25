//! The shipped S3 image's second core (`two-core`, `fw-esp32s3/src/core1.rs`)
//! on `lp-emu:esp32s3:t1`: released once through the ROM's APP-CPU path,
//! woken into every render-samples batch and parked in `waiti` between them,
//! and — when it stalls, faults or reaches for the allocator — halted while
//! core 0 renders on alone.
//!
//! The faults are injected from outside the image (the machine's own
//! handles), so the product carries no test hook: a core 1 held by the
//! RUNSTALL bit stands in for a hung job, a core 1 sent to an unmapped
//! address for a crash, and a core 1 sent into `__rust_alloc` for the
//! forbidden allocation.
//!
//! `#[ignore]`d like the other link gates: it needs `LP_EMU_ESP32S3_ELF` and
//! `LP_EMU_ESP32S3_MERGED`, which `just test-emu-esp32s3-boot` builds and
//! exports before it runs this.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_cli::commands::emu::link_host::{EmuLinkHost, S3Board};
use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::machine::{AppSource, Esp32S3Builder, Machine, UsbHost};
use lp_emu_esp32s3::test_support;

/// Long enough for the boot, the link and the hello.
const GATE_US: u64 = 2_000_000;
/// `D10` on the XIAO ESP32-S3 Plus profile.
const PAD: u8 = 9;
/// The firmware's join timeout (`core1::JOIN_TIMEOUT_CYCLES`, 2 s) and margin.
const PAST_TIMEOUT_US: u64 = 2_600_000;

/// **Core 1 renders.** It comes out of reset once, takes jobs (it retires
/// instructions and parks in `waiti` between them: the cross-core wake), and
/// the frames are lit.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF and LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
fn core_one_is_released_once_and_takes_jobs_between_waits() {
    let Some(mut host) = rendering("core_one_is_released_once_and_takes_jobs_between_waits") else {
        return;
    };
    let m = &host.board.machine;
    let console = host.console().join("\n");
    assert!(
        console.contains("[core1] render helper on"),
        "core 1 started:\n{console}"
    );
    assert_eq!(m.core1_releases(), 1, "released once, through the ROM");
    assert!(
        m.harts[1].instruction_count() > 1_000_000,
        "core 1 ran jobs: {} instructions",
        m.harts[1].instruction_count()
    );
    assert!(
        m.wfi_ends(1) > 20,
        "core 1 parked in waiti between jobs and was woken: {} parks",
        m.wfi_ends(1)
    );
    let lit_before = lit(&mut host.board.machine);
    advance(&mut host, 500_000);
    assert!(
        lit(&mut host.board.machine) > lit_before,
        "frames keep coming"
    );
}

/// **A hung core 1 times out.** Held by RUNSTALL for good, its next job never
/// finishes: core 0's join gives up after 2 s, says so once, and renders on.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF and LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
fn a_stalled_core_one_times_out_and_core_zero_renders_on() {
    let Some(mut host) = rendering("a_stalled_core_one_times_out_and_core_zero_renders_on") else {
        return;
    };
    host.board
        .machine
        .core_1_control_handle()
        .lock()
        .expect("core_1_control")
        .runstall = true;
    advance(&mut host, PAST_TIMEOUT_US);
    assert_stopped_and_rendering(&mut host, "a job did not finish in 2 s");
}

/// **A crash on core 1 halts it.** Sent to an unmapped address, its next
/// wake faults; the panic handler halts core 1 alone (no reset), and core 0
/// renders on.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF and LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
fn a_fault_on_core_one_halts_it_and_core_zero_renders_on() {
    let Some(mut host) = rendering("a_fault_on_core_one_halts_it_and_core_zero_renders_on") else {
        return;
    };
    host.board.machine.harts[1].set_pc(0x4008_0000);
    advance(&mut host, PAST_TIMEOUT_US);
    assert_stopped_and_rendering(&mut host, "a panic on core 1");
}

/// **Core 1 never takes the allocator's lock.** Sent into `__rust_alloc`, the
/// guard halts it before the heap is touched, and core 0 renders on.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF and LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
fn core_one_in_the_allocator_is_halted_by_the_guard() {
    let Some(mut host) = rendering("core_one_in_the_allocator_is_halted_by_the_guard") else {
        return;
    };
    // `__rust_alloc` is v0-mangled in this image (`…7___rustc12___rust_alloc`).
    let alloc = host
        .board
        .machine
        .app()
        .and_then(|app| {
            app.symbols()
                .iter()
                .find(|s| s.name.ends_with("12___rust_alloc"))
                .map(|s| s.address)
        })
        .expect("the image's allocator entry");
    host.board.machine.harts[1].set_pc(alloc);
    advance(&mut host, PAST_TIMEOUT_US);
    assert_stopped_and_rendering(&mut host, "core 1 asked the allocator for memory");
}

/// **Every flash writer stalls core 1** (`core1::with_core1_parked`,
/// `FlashStorage::multicore_auto_park`). Under the flash-busy model, where a
/// window access by either core while `SPI1` programs or erases stops the
/// run, a second project is deployed while the first renders on both cores:
/// the deploy's writes run, nothing overlaps, and the new project renders.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF and LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
fn a_deploy_while_both_cores_render_never_overlaps_a_flash_write() {
    let Some(mut host) = rendering_with(
        "a_deploy_while_both_cores_render_never_overlaps_a_flash_write",
        true,
    ) else {
        return;
    };
    let jobs_before = host.board.machine.harts[1].instruction_count();
    deploy(&mut host, &repo_root().join("projects/test/basic"));
    advance(&mut host, 1_000_000);
    let console = host.console().join("\n");
    assert!(
        !console.contains("[core1] stopped"),
        "core 1 kept going:\n{console}"
    );
    assert!(
        host.board.machine.harts[1].instruction_count() > jobs_before,
        "core 1 took jobs across the deploy"
    );
    let lit_before = lit(&mut host.board.machine);
    advance(&mut host, 500_000);
    assert!(
        lit(&mut host.board.machine) > lit_before,
        "the new project renders"
    );
}

/// The shipped image direct-loaded on the merged chip, `projects/test/shader-oracle`
/// deployed, rendering lit frames on [`PAD`]. `None` (with a skip notice)
/// when the images are missing.
fn rendering(test: &str) -> Option<EmuLinkHost<S3Board>> {
    rendering_with(test, false)
}

/// [`rendering`], with the flash-busy model on or off.
fn rendering_with(test: &str, flash_busy: bool) -> Option<EmuLinkHost<S3Board>> {
    let images = (|| {
        Ok::<_, String>((
            test_support::fw_esp32s3_image()?,
            test_support::merged_chip_image()?,
        ))
    })();
    let (elf, merged) = match images {
        Ok(images) => images,
        Err(reason) => {
            test_support::skip_notice(test, &reason);
            return None;
        }
    };
    let builder = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Copy(merged))
        .strict(true)
        .flash_busy(flash_busy);
    let board = S3Board::build(builder, UsbHost::Attached { draining: true })
        .unwrap_or_else(|e| panic!("{test}: the image direct-loads: {e}"));
    let mut host = EmuLinkHost::new(board, 0x5E55_0304, true);
    deploy(&mut host, &repo_root().join("projects/test/shader-oracle"));
    while lit(&mut host.board.machine) < 10 {
        assert!(
            host.board.machine.micros() < 8_000_000,
            "{test}: ten lit frames by 8 s:\n{}",
            host.console().join("\n")
        );
        advance(&mut host, 100_000);
    }
    Some(host)
}

fn assert_stopped_and_rendering(host: &mut EmuLinkHost<S3Board>, why: &str) {
    let console = host.console().join("\n");
    assert!(
        console.contains(&format!("[core1] stopped: {why}")),
        "core 0 says why core 1 stopped (`{why}`):\n{console}"
    );
    assert!(
        !console.contains("[RECOVERY] boot: cause=panic"),
        "no reset:\n{console}"
    );
    let lit_before = lit(&mut host.board.machine);
    advance(host, 1_000_000);
    assert!(
        lit(&mut host.board.machine) > lit_before,
        "core 0 renders on alone"
    );
}

fn advance(host: &mut EmuLinkHost<S3Board>, us: u64) {
    let until = host.board.machine.micros() + us;
    host.run_until(until, None)
        .unwrap_or_else(|e| panic!("the run stops only at its deadline: {e}"));
}

fn lit(m: &mut Machine) -> usize {
    m.flush_frames();
    m.frames(PAD)
        .iter()
        .filter(|f| f.is_complete() && f.wire.iter().any(|b| *b != 0))
        .count()
}

/// Wait for the board's hello, then deploy `dir` over the link as `lp-cli
/// upload` deploys it.
fn deploy(host: &mut EmuLinkHost<S3Board>, dir: &Path) {
    let hello = host
        .wait_for_line("\"hello\":{", GATE_US)
        .expect("the boot");
    assert!(hello.is_some(), "no hello:\n{}", host.console().join("\n"));
    let (uid, _) = lp_cli::commands::dev::validation::validate_local_project(&dir.to_path_buf())
        .unwrap_or_else(|e| panic!("{} validates: {e:#}", dir.display()));
    let files =
        lp_cli::commands::dev::collect_project_deploy_files(&lpfs::LpFsStd::new(dir.to_path_buf()))
            .expect("the project's files");
    host.set_queue_messages(true);
    {
        let mut client = lpa_client::LpClient::new(&mut *host);
        block_on(client.deploy_project_files(&uid, files))
            .unwrap_or_else(|e| panic!("the deploy failed: {e}"));
    }
    host.set_queue_messages(false);
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}

/// Drive a future whose every await completes synchronously: tests are
/// edges, and a null waker is enough.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
}
