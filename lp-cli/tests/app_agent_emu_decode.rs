//! Stage B of the app-agent evals (plan `lp2025/2026-10-01-0126-app-agent-harness`,
//! PD8): deploy a project tree to the shipped C6 image running in the
//! emulator and decode what reaches the XIAO C6's D6 pad (GPIO16).
//!
//! Stage A (`lpa-studio-core`, `app/agent/evals`) judges the project the
//! agent built at the model level and writes its tree to disk; this file is
//! the ground truth for "Sean's LEDs light": the image's own JIT compiles
//! the project's shaders, its RMT drives the pad, and the waveform decoder
//! reads the frames back.
//!
//! - **Golden leg** (deterministic, `just test-emu-c6-cli`): the committed
//!   golden projects (`lpa-studio-core/tests/fixtures/app_agent/golden/`)
//!   must drive complete, error-free, lit, changing frames of the right
//!   length on pad 16.
//! - **Live leg** (`just app-agent-eval`): `LP_APP_AGENT_PROJECT=<dir>` and
//!   `LP_APP_AGENT_LEDS=<n>` point at the tree a model run produced.
//!
//! `#[ignore]`d: they need a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`,
//! or CI's images via `LP_CI_IMAGES`). Times are emulated (`lp-emu:esp32c6:t1`);
//! nothing here reads the host clock.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::pins::{PadId, RouteSource, SignalId};
use lp_emu_esp_common::strip::ws281x::Frame;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};

/// One fixed host nonce, so two runs are the same run.
const NONCE: u32 = 0x4057_C6A7;

/// The XIAO ESP32-C6's D6 pin.
const PAD: u8 = 16;

/// The RMT output signals a WS281x channel routes onto its pad (two
/// channels on the C6).
const RMT_SIGNALS: [u16; 2] = [71, 72];

/// Emulated time rendered after the upload. Long enough for the first
/// pattern's compile and a second of frames; the emulated C6 builds a
/// graphics stage far slower than silicon (open defect
/// `docs/defects/2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md`),
/// so the window is in emulated time and generous.
const RENDER_WINDOW_US: u64 = 4_000_000;

/// Two frames at least this far apart (emulated) must differ: the patterns
/// animate.
const CHANGE_GAP_US: u64 = 500_000;

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn the_sean_250_golden_lights_250_leds_on_d6() {
    decode_and_check(&golden("sean-250-d6"), 250);
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn the_sean_300_golden_lights_300_leds_on_d6() {
    decode_and_check(&golden("sean-300-d6"), 300);
}

/// The judge can fail: the 250 golden is not a 300-LED strip.
#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn the_sean_golden_is_judged_against_its_own_count() {
    let Some(elf) = image() else {
        return;
    };
    let run = deploy_and_render(&elf, &golden("sean-250-d6"));
    let reason = judge(&run, 300).expect_err("250 LEDs are not 300");
    assert!(reason.contains("wrong LED count"), "{reason}");
}

/// The live leg: a project tree a model run wrote (stage A's
/// `target/app-agent-evals/<run>/<scenario>/project/`).
#[test]
#[ignore = "live leg: set LP_APP_AGENT_PROJECT and LP_APP_AGENT_LEDS (`just app-agent-eval`)"]
fn an_agent_built_project_lights_its_leds_on_d6() {
    let Ok(dir) = std::env::var("LP_APP_AGENT_PROJECT") else {
        panic!("LP_APP_AGENT_PROJECT is not set — `just app-agent-eval` sets it");
    };
    let leds: usize = std::env::var("LP_APP_AGENT_LEDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .expect("LP_APP_AGENT_LEDS=<led count>");
    decode_and_check(Path::new(&dir), leds);
}

/// Deploy `dir` to a booted C6, render for [`RENDER_WINDOW_US`], and judge
/// the frames on pad 16 against `leds`. Prints one summary line.
fn decode_and_check(dir: &Path, leds: usize) {
    let Some(elf) = image() else {
        return;
    };
    let run = deploy_and_render(&elf, dir);
    match judge(&run, leds) {
        Ok(summary) => println!(
            "app-agent stage B (lp-emu:esp32c6:t1, lp-emu {}): {} — {summary}",
            lp_emu_commit(),
            dir.display()
        ),
        Err(reason) => panic!("{}: {reason}\n{}", dir.display(), run.console.join("\n")),
    }
}

/// Whether the frames on pad 16 are `leds` LEDs long, whole, clean, lit and
/// changing: `Ok(summary)` or `Err(what is wrong)`.
fn judge(run: &RenderRun, leds: usize) -> Result<String, String> {
    if !run.routed.iter().any(|(pad, source)| {
        *pad == PadId(PAD)
            && RMT_SIGNALS
                .iter()
                .any(|sig| *source == RouteSource::Signal(SignalId(*sig), false))
    }) {
        return Err(format!(
            "gpio16 (D6) is not routed to an RMT channel: {:?}",
            run.routed
        ));
    }
    let frames = &run.frames;
    if frames.is_empty() {
        return Err("no frame reached gpio16".to_string());
    }
    let Some(lit) = frames.iter().position(|f| f.wire.iter().any(|b| *b != 0)) else {
        return Err(format!(
            "{} frames on gpio16 and every one of them black",
            frames.len()
        ));
    };
    // Every frame from the first lit one on, except a last frame the window
    // cut open, is whole, clean, and the strip's length.
    let last = frames.last().expect("frames").n;
    let mut judged = Vec::new();
    for f in &frames[lit..] {
        if f.reset_cycles.is_none() {
            if f.n != last {
                return Err(format!("frame {} is open but not the last", f.n));
            }
            continue;
        }
        if f.error_count != 0 {
            return Err(format!("frame {}: {:?}", f.n, f.errors));
        }
        if !f.is_complete() {
            return Err(format!("frame {} was cut short: {f:?}", f.n));
        }
        if f.leds() != leds {
            return Err(format!(
                "frame {} carries the wrong LED count: {} instead of {leds}",
                f.n,
                f.leds()
            ));
        }
        judged.push(f);
    }
    if judged.len() < 2 {
        return Err(format!(
            "only {} whole frames after the first lit one",
            judged.len()
        ));
    }
    let first = judged[0];
    let gap = CHANGE_GAP_US * u64::from(memmap::CYCLES_PER_US);
    if !judged
        .iter()
        .any(|f| f.start >= first.start + gap && f.wire != first.wire)
    {
        return Err(format!(
            "no frame {CHANGE_GAP_US} us after the first lit one differs from it — the strip is frozen"
        ));
    }
    Ok(format!(
        "{} frames on gpio16, first lit n={} at {:.3} ms, {} whole {leds}-LED frames after it",
        frames.len(),
        first.n,
        first.start as f64 / memmap::CYCLES_PER_US as f64 / 1_000.0,
        judged.len()
    ))
}

struct RenderRun {
    console: Vec<String>,
    frames: Vec<Frame>,
    routed: Vec<(PadId, RouteSource)>,
}

/// Boot, wait for the hello, deploy `dir` over the link as `lp-cli upload`
/// does, then render on for [`RENDER_WINDOW_US`] of emulated time.
fn deploy_and_render(elf: &Path, dir: &Path) -> RenderRun {
    let mut host = hosted(elf);
    let hello = host
        .wait_for_line("\"hello\":{", 3_000_000)
        .expect("the boot");
    assert!(hello.is_some(), "no hello:\n{}", host.console().join("\n"));

    let dir = dir.to_path_buf();
    let (uid, _) = lp_cli::commands::dev::validation::validate_local_project(&dir)
        .expect("the project validates");
    let files = lp_cli::commands::dev::collect_project_deploy_files(&lpfs::LpFsStd::new(dir))
        .expect("the project's files");
    {
        let mut client = lpa_client::LpClient::new(&mut host);
        block_on(client.deploy_project_files(&uid, files))
            .unwrap_or_else(|e| panic!("the deploy failed: {e}"));
    }
    host.set_queue_messages(false);
    let until = host.board.machine.micros() + RENDER_WINDOW_US;
    host.run_until(until, None).expect("rendering on");
    host.board.machine.flush_frames();
    assert_eq!(host.link_errors, 0, "{}", host.console().join("\n"));
    RenderRun {
        console: host.console().to_vec(),
        frames: host.board.machine.frames(PAD).to_vec(),
        routed: host.board.machine.routed_pads(),
    }
}

/// The shipped image, direct-loaded (not ROM-up: the mask ROM talks on
/// UART0, which is GPIO16 — the pad under test), attached and draining
/// from power-on with this process as the host on its link.
fn hosted(elf: &Path) -> EmuLinkHost<C6Board> {
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(FlashBacking::Blank)
        .strict(true)
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .build()
        .expect("the shipped image builds a machine");
    EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true)
}

fn image() -> Option<PathBuf> {
    match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("app_agent_emu_decode: skipped — {reason}");
            None
        }
    }
}

fn golden(name: &str) -> PathBuf {
    repo_root()
        .join("lp-app/lpa-studio-core/tests/fixtures/app_agent/golden")
        .join(name)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}

/// The `lp-emu` commit an emulated number is quoted against (AGENTS.md:
/// emulated measurements name their emulator and its commit).
fn lp_emu_commit() -> String {
    std::process::Command::new("git")
        .args(["log", "-1", "--format=%h", "--", "lp-emu"])
        .current_dir(repo_root())
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Drive a future whose every await completes synchronously (the host steps
/// the board inside `receive`): tests are edges, and a null waker is enough.
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
