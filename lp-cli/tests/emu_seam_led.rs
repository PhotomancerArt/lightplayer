//! The LED performance seam is honest (A-2): `led=fast` against no seam, on
//! the shipped **split** image booted ROM-up the way a Studio board boots,
//! with a host on its USB link, at `t2`, for 5.5 s emulated.
//!
//! The project is `projects/test/shader-oracle`, placed in the image's lpfs
//! (the board loads it at boot, no upload): its shader consumes **no clock**,
//! so every frame is the same bytes and the two runs can be compared byte for
//! byte. With the seam on the board must show:
//!
//! - the same number of frames on the strip's pad (within one) and identical
//!   decoded bytes, frame by frame;
//! - zero decoder errors;
//! - identical heap and stack figures (the heartbeat's memory fields and the
//!   `[stack]` lines);
//! - `seam_calls > 0` on and `== 0` off, and the `SEAM led=fast engaged` line
//!   on and no such line off.
//!
//! **Frame timestamps are not compared**, on purpose: the park ends at the
//! interrupt that wakes the hart, a few cycles off where the spinning loop
//! would have noticed the flag, so frame edges drift by about 0.13 µs a frame
//! (the M0 spike's measurement). The wire time is still billed by the RMT
//! model; what moves is when the render thread notices it.
//!
//! It lives in lp-cli, not the emulator, because the lpfs fixture is built
//! with lp-cli's own tools and the link host is a product crate (the MIT
//! fence), as `emu_split_boot.rs`. `#[ignore]`d: it needs a split build
//! (`LP_EMU_BUILD_FW=1`, or `LP_EMU_C6_SPLIT_ESP32C6_SERVER_RADIO`); `just
//! test-emu-c6-cli` runs it.

use std::path::{Path, PathBuf};

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_cli::commands::hardware::lpfs::fixture::build_chip;
use lp_cli::commands::hardware::lpfs::lpfs_target::target_table;
use lp_emu_esp_common::Strap;
use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp_common::strip::ws281x::Frame;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{AppSource, BootMode, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, SplitImage, split_image};
use lpa_link::layout_migration::lpfs_tree::LpfsTree;

/// One fixed host nonce, so a run is a function of the image.
const NONCE: u32 = 0x5EA0_0001;
/// The end-user row's window.
const RUN_US: u64 = 5_500_000;
/// `ws281x:local:D10` on the XIAO C6.
const PAD: u8 = 18;

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6-cli` runs it"]
fn led_fast_renders_the_same_frames_heap_and_fps_as_no_seam() {
    let split = match split_image(&FwImage::SHIPPED) {
        Ok(split) => split,
        Err(reason) => {
            eprintln!("emu_seam_led: skipped — {reason}");
            return;
        }
    };
    let chip = chip_with_project(&split);
    let off = run(&split, &chip, SeamRequest::none());
    let on = run(&split, &chip, SeamRequest::strict("led=fast").unwrap());

    assert!(
        off.seam_lines.is_empty(),
        "no seam line off: {:?}",
        off.seam_lines
    );
    assert_eq!(off.seam_calls, 0);
    assert!(
        on.seam_lines
            .iter()
            .any(|l| l.starts_with("SEAM led=fast engaged (performance")),
        "{:?}",
        on.seam_lines
    );
    assert!(on.seam_calls > 0, "the wait seam was answered");
    assert_eq!(on.label, "lp-emu:esp32c6:t2+led=fast");
    assert_eq!(off.label, "lp-emu:esp32c6:t2");

    assert!(
        off.frames.len() > 10,
        "the board rendered: {}",
        off.frames.len()
    );
    assert!(
        off.frames.len().abs_diff(on.frames.len()) <= 1,
        "the same frame count within one: {} off, {} on",
        off.frames.len(),
        on.frames.len()
    );
    assert_eq!(off.errors, 0);
    assert_eq!(on.errors, 0, "no decoder errors with the seam on");
    let n = off.frames.len().min(on.frames.len());
    for i in 0..n {
        assert_eq!(
            off.frames[i].wire, on.frames[i].wire,
            "frame {i}'s bytes differ with the seam on"
        );
    }
    assert!(
        !off.heap.is_empty(),
        "the heartbeat's figures were read:\n{}",
        off.console.join("\n")
    );
    assert_eq!(off.heap, on.heap, "identical heap and stack figures");

    println!(
        "emu_seam_led (lp-emu:esp32c6:t2, ROM-up split image, shader-oracle): {} frames off, \
         {} on; {} seam calls; heap lines {:?}",
        off.frames.len(),
        on.frames.len(),
        on.seam_calls,
        off.heap
    );
}

struct Run {
    /// The frames a reset closed.
    frames: Vec<Frame>,
    /// Decoder errors over every frame, the cut-off last one included.
    errors: u64,
    heap: Vec<String>,
    console: Vec<String>,
    seam_lines: Vec<String>,
    seam_calls: u64,
    label: String,
}

fn run(split: &SplitImage, chip: &[u8], seams: SeamRequest) -> Run {
    let machine = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::Path(split.p2_elf()))
        .flash(FlashBacking::Bytes(chip.to_vec()))
        .flash_len(chip.len() as u32)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .time_grade(TimeGrade::T2)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .seams(seams)
        .build()
        .expect("the split image builds a machine");
    let mut host = EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true);
    host.set_queue_messages(false);
    host.run_until(RUN_US, None).expect("the run");
    let m = &mut host.board.machine;
    m.flush_frames();
    // Every frame a reset closed: the run's end can cut the last one off
    // mid-frame, at a different bit on each side.
    let errors = m.frames(PAD).iter().map(|f| f.error_count).sum();
    let frames = m
        .frames(PAD)
        .iter()
        .filter(|f| f.reset_cycles.is_some())
        .cloned()
        .collect();
    let seam_lines = m.take_seam_lines();
    let seam_calls = m.seams().calls;
    let label = m.configuration_label();
    let console = host.console().to_vec();
    let heap = console
        .iter()
        .filter_map(|l| heap_figures(l))
        .collect::<Vec<_>>();
    Run {
        frames,
        errors,
        heap,
        console,
        seam_lines,
        seam_calls,
        label,
    }
}

/// The figures in a heap or stack line, without anything that carries time:
/// a heartbeat's memory object, or a `[stack]` line.
fn heap_figures(line: &str) -> Option<String> {
    if let Some(at) = line.find("[stack]") {
        return Some(line[at..].to_string());
    }
    if line.contains("\"heartbeat\"") {
        let at = line.find("\"memory\"")?;
        let rest = &line[at..];
        let end = rest.find('}')?;
        return Some(rest[..=end].to_string());
    }
    None
}

/// The split image's merged bytes with `shader-oracle` in its lpfs, built the
/// way `lp-cli hardware lpfs fixture` builds a chip.
fn chip_with_project(split: &SplitImage) -> Vec<u8> {
    let merged = std::fs::read(split.merged()).expect("the merged image");
    let table = target_table(Some(&repo_root().join("lp-fw/fw-esp32c6/partitions.csv")))
        .expect("the C6 table");
    let project = repo_root().join("projects/test/shader-oracle");
    let mut files = Vec::new();
    walk(&project, &project, &mut files);
    let tree = LpfsTree::from_files(files);
    build_chip(&merged, &table, &tree).expect("the chip")
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(root, &path, out);
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            out.push((
                format!("/projects/shader-oracle/{rel}"),
                std::fs::read(&path).unwrap(),
            ));
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}
