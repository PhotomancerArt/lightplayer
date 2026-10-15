//! A power cut in the middle of an upload, on today's littlefs image, and the
//! board power-cycled back to serving (plan
//! `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, P2).
//!
//! The run: the shipped image on a blank flash (it formats `lpfs` as
//! littlefs at boot), a host on its USB lp-link, the hello; THEN a flash cut
//! armed on the running machine — the [`CUT_AT`]th program or erase command
//! inside `lpfs` from that moment, torn the calibrated way — and
//! `projects/test/basic` uploaded until the cut stops the board. The test
//! power-cycles it (both domains back to power-on, the flash as the cut left
//! it), puts a fresh host on the link, and asks for the hello and the
//! loaded-project list.
//!
//! It asserts only that the whole chain works: the cut fired inside the
//! upload, the board came back, and it answered. Whether littlefs kept every
//! file is the cut walks' question (P7, and a littlefs control there), not
//! this smoke's. It prints the cut's `FLASH-CUT …` line and the in-range op
//! census. Numbers are `lp-emu:esp32c6:t1+flash-cut`.
//!
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`), and
//! `just test-emu-c6-cli-boards` runs it.

#[path = "support/editor_reads.rs"]
mod editor_reads;

use std::path::{Path, PathBuf};

use editor_reads::block_on;
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::Strap;
use lp_emu_esp_common::engine::flash_cut::TearModel;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::flash_cut_spec::FlashCutSpec;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpa_client::LpClient;

/// Where the cut lands: the 0-based in-range command index from the moment
/// the plan is armed (after the hello). An upload of `projects/test/basic`
/// sends hundreds of `lpfs` commands, so this is well inside it.
const CUT_AT: u64 = 40;

/// The tear's seed: one fixed seed, so two runs are the same run.
const SEED: u64 = 7;

/// The boot banner: one per power-on.
const BOOT_BANNER: &str = "[INIT] Initializing board...";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli-boards` runs it"]
fn a_cut_in_the_middle_of_an_upload_power_cycles_back_to_a_serving_board() {
    let Some(elf) = image(&FwImage::SHIPPED) else {
        return;
    };
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Blank)
        .strict(false)
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        // The power-on snapshot a power cycle restores.
        .reboot_on_reset(true)
        .build()
        .expect("the shipped image builds a machine");
    let mut host = EmuLinkHost::new(
        C6Board::new(machine).expect("a hosted board"),
        0xF1A5_C001,
        true,
    );
    let hello = host
        .wait_for_line("\"hello\":{", 5_000_000)
        .expect("the boot");
    assert!(hello.is_some(), "no hello:\n{}", host.console().join("\n"));

    let spec = FlashCutSpec::new(CUT_AT, TearModel::Calibrated, SEED);
    host.board
        .machine
        .arm_flash_cut(&spec)
        .expect("a machine that can power-cycle");
    let label = host.board.machine.configuration_label();
    assert!(label.ends_with("+flash-cut"), "{label}");

    let files = project_files("projects/test/basic");
    let upload = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
        block_on(client.replace_and_load_project("flash-cut-basic", &files))
    };
    let report = host
        .board
        .machine
        .last_flash_cut()
        .unwrap_or_else(|| panic!("the cut never fired; the upload said {upload:?}"));
    let summary = host.board.machine.flash_cut_summary();
    println!("{label} — the cut:\n  {}", summary.join("\n  "));
    assert!(
        upload.is_err(),
        "the board lost power mid-upload, so the upload cannot have finished: {report}"
    );
    assert_eq!(report.index, CUT_AT);
    let cut_boots = boots(host.console());

    // The power cycle: the supply back, the flash as the cut left it.
    let mut board = host.board;
    assert!(board.machine.power_cycle(Strap::App), "a power-on snapshot");
    assert!(!board.machine.flash().lock().unwrap().is_powered_off());
    let mut host = EmuLinkHost::new(board, 0xF1A5_C002, true);
    let hello = host
        .wait_for_line("\"hello\":{", 10_000_000)
        .expect("the reboot");
    let console = host.console().join("\n");
    assert!(
        hello.is_some(),
        "no hello after the power cycle:\n{console}"
    );
    let loaded = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(2_000);
        block_on(client.project_list_loaded())
    };
    let console = host.console().to_vec();
    let fs_lines: Vec<&String> = console
        .iter()
        .filter(|line| {
            let lower = line.to_lowercase();
            ["mount", "format", "littlefs", "lpfs", "filesystem", "fs:"]
                .iter()
                .any(|word| lower.contains(word))
        })
        .collect();
    println!(
        "after the power cycle ({} boot(s) before the cut, {} after; {}):\n  loaded: {loaded:?}\n  \
         filesystem lines:\n    {}",
        cut_boots,
        boots(&console),
        host.board.machine.configuration_label(),
        fs_lines
            .iter()
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n    ")
    );
    assert_eq!(
        boots(&console),
        1,
        "one boot on the new power:\n{}",
        console.join("\n")
    );
    assert!(
        loaded.is_ok(),
        "the board serves after the power cycle: {loaded:?}\n{}",
        console.join("\n")
    );
}

fn boots(console: &[String]) -> usize {
    console
        .iter()
        .filter(|line| line.contains(BOOT_BANNER))
        .count()
}

/// A project directory as the upload's `(relative path, bytes)` list, in
/// path order (as `lp-cli upload` sends it).
fn project_files(relative: &str) -> Vec<(String, Vec<u8>)> {
    let root = repo_root().join(relative);
    let mut files = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("the project directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let name = path
                    .strip_prefix(&root)
                    .expect("under the root")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((name, std::fs::read(&path).expect("a file")));
            }
        }
    }
    files.sort();
    files
}

fn image(image: &FwImage) -> Option<PathBuf> {
    match fw_esp32c6_image(image) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("emu_flash_cut_smoke: skipped — {reason}");
            None
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}
