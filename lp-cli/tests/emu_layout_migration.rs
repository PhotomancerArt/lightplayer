//! The C6 repartition's host path, end to end against the emulated ESP32-C6
//! (plan `lp2025/2026-10-01-1843-c6-repartition`, P08): a "fielded board"
//! — the current firmware on the frozen pre-2026-10 table, its files in a
//! 240-block littlefs at `0x310000` — read and migrated by the REAL
//! `lp-cli hardware lpfs` commands, through espflash and its stub, over a
//! pty into the emulated mask ROM's download console.
//!
//! - **Step 0** (`the_bootloader_reads_back_the_chip_byte_for_byte`): no test
//!   had ever run an esptool READ against the emulated ROM/stub. `lpfs save`
//!   reads the table and the whole old filesystem region; both must be the
//!   chip file's bytes.
//! - **W10**: `lpfs migrate` moves every file to the new layout — table at
//!   `0x8000` is the new one, the filesystem at `0x350000` mounts with every
//!   file byte for byte, the old superblock pair at `0x310000` is erased —
//!   and the board then boots its files without formatting.
//! - **W11**: a preflight for an image with the OLD table, against the
//!   migrated chip, refuses (exit 3) and writes nothing.
//! - **W3**: a board whose files do not fit the new layout is refused, and
//!   the chip is byte-identical afterwards.
//!
//! Emulated: `lp-emu:esp32c6:t1`, non-strict (espflash's stub reads one
//! block the C6 boot set does not map — `flash_over_socket.rs`). The
//! emulator models no USB re-enumeration and no flash wear; a pty carries no
//! DTR/RTS, so the reset into the app is the modelled dance on the
//! machine's control channel.
//!
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`) and
//! espflash 3.3.0 for the merged image. `just test-emu-c6-cli` runs it.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::{EfuseIdentity, ResetCause};
use lp_emu_esp32c6::machine::{
    BootMode, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, Uart0Sink, UsbHost,
    UsbSjDrain, UsbSjSink,
};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, workspace_root};
use lpa_link::PartitionTable;
use lpa_link::layout_migration::{LpfsGeometry, LpfsTree, legacy_c6_v1_table};
use serialport::{SerialPort, TTYPort};

/// The board's efuse MAC (the walk's own; any will do).
const MAC: [u8; 6] = [0x60, 0x55, 0xf9, 0x0a, 0x0b, 0x0c];

/// The uid the fixture's `/.lp/device.json` stamps.
const UID: &str = "dev0000000000000011";

/// The old filesystem: `0x310000`, 240 blocks.
const LEGACY_OFFSET: usize = 0x31_0000;
const LEGACY_LEN: usize = 0xF_0000;

/// Emulated-time ceiling for one CLI run against the machine (a wedge
/// detector, never a measurement).
const CLI_BUDGET_US: u64 = 3_000_000_000;

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and espflash 3.3.0; `just test-emu-c6-cli` runs it"]
fn the_bootloader_reads_back_the_chip_byte_for_byte() {
    let Some(work) = Work::new("step0", false) else {
        return;
    };
    let chip_before = std::fs::read(&work.chip).unwrap();
    let out = work.dir.join("saved");
    let (cli, early) =
        work.run_cli_in_download(&os(&["hardware", "lpfs", "save", "--out"], &[&out]));
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(cli.status.success(), "lpfs save failed:\n{}", told(&cli));

    let raw = only_file(&out, "raw-lpfs-");
    let table = only_file(&out, "partition-table-");
    assert_eq!(
        std::fs::read(&table).unwrap(),
        chip_before[0x8000..0x8000 + 0xC00],
        "the partition table read over the bootloader is not the chip's"
    );
    assert_eq!(
        std::fs::read(&raw).unwrap(),
        chip_before[LEGACY_OFFSET..LEGACY_OFFSET + LEGACY_LEN],
        "the filesystem region read over the bootloader is not the chip's"
    );
    assert!(
        told(&cli).contains("files"),
        "the saved region mounts as a filesystem:\n{}",
        told(&cli)
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and espflash 3.3.0; `just test-emu-c6-cli` runs it"]
fn migrate_moves_every_file_and_the_board_boots_them() {
    let Some(work) = Work::new("w10", false) else {
        return;
    };
    let backups = work.dir.join("backups");
    let (cli, early) = work.migrate(&backups);
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(cli.status.success(), "lpfs migrate failed:\n{}", told(&cli));
    assert!(
        std::fs::read_dir(&backups)
            .map(|dir| dir.count() > 0)
            .unwrap_or(false),
        "the mandatory backup was stored:\n{}",
        told(&cli)
    );

    let chip = std::fs::read(&work.chip).unwrap();
    // The new table, the files at the new filesystem, the old one retired.
    let target = target_table();
    assert_eq!(
        &chip[0x8000..0x8000 + target.to_bytes().len()],
        &target.to_bytes()[..],
        "the chip carries the new partition table"
    );
    let geometry = LpfsGeometry::from_table(&target).unwrap();
    let start = geometry.offset as usize;
    let (tree, _) = LpfsTree::from_image(&chip[start..start + geometry.len() as usize], geometry)
        .expect("the new filesystem mounts");
    assert_eq!(files_of(&tree), work.files, "every file, byte for byte");
    assert!(
        chip[LEGACY_OFFSET..LEGACY_OFFSET + 0x2000]
            .iter()
            .all(|b| *b == 0xFF),
        "the old superblock pair at 0x310000 is erased"
    );

    // The board boots what was written: its files mount, nothing formats.
    let console = work.boot_app();
    assert!(
        !console.contains("[FS] Mount failed"),
        "the migrated filesystem did not mount:\n{console}"
    );
    assert!(
        !console.contains("legacy-layout filesystem found"),
        "the board held an old-layout filesystem instead of mounting:\n{console}"
    );
    assert!(
        console.contains("[INIT] Flash filesystem mounted"),
        "the board did not mount its filesystem:\n{console}"
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and espflash 3.3.0; `just test-emu-c6-cli` runs it"]
fn a_preflight_for_the_old_table_refuses_a_migrated_chip_and_writes_nothing() {
    // A board already on the new layout (what W10 leaves), built directly:
    // migrating one first would double this test's cost for no new claim.
    let Some(work) = Work::new_on("w11", false, Layout::Current) else {
        return;
    };
    let migrated = std::fs::read(&work.chip).unwrap();

    let legacy_csv = work.dir.join("legacy.csv");
    std::fs::write(&legacy_csv, legacy_csv_text()).unwrap();
    let (cli, early) = work.run_cli_in_download(&os(
        &["hardware", "lpfs", "preflight", "--table"],
        &[&legacy_csv],
    ));
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert_eq!(
        cli.status.code(),
        Some(3),
        "the preflight refuses:\n{}",
        told(&cli)
    );
    assert!(
        std::fs::read(&work.chip).unwrap() == migrated,
        "a refused preflight wrote to the chip"
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and espflash 3.3.0; `just test-emu-c6-cli` runs it"]
fn a_board_whose_files_do_not_fit_is_refused_and_left_byte_identical() {
    let Some(work) = Work::new("w3", true) else {
        return;
    };
    let before = std::fs::read(&work.chip).unwrap();
    let (cli, early) = work.migrate(&work.dir.join("backups"));
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(
        !cli.status.success(),
        "an over-full board must be refused:\n{}",
        told(&cli)
    );
    assert!(
        told(&cli).contains("fit"),
        "the refusal says why:\n{}",
        told(&cli)
    );
    assert!(
        std::fs::read(&work.chip).unwrap() == before,
        "a refused migration wrote to the chip"
    );
}

/// Which table the fixture chip carries.
#[derive(Clone, Copy)]
enum Layout {
    /// The frozen pre-2026-10 table; the files at `0x310000`.
    Legacy,
    /// This tree's table; the files at its `lpfs`.
    Current,
}

/// One scenario's scratch: the fixture chip, the merged image to write, the
/// fixture's files.
struct Work {
    dir: PathBuf,
    chip: PathBuf,
    merged: PathBuf,
    files: Vec<(String, Vec<u8>)>,
}

impl Work {
    fn new(name: &str, over_full: bool) -> Option<Self> {
        Self::new_on(name, over_full, Layout::Legacy)
    }

    fn new_on(name: &str, over_full: bool, layout: Layout) -> Option<Self> {
        let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
            Ok(path) => path,
            Err(reason) => {
                eprintln!("emu_layout_migration: skipped — {reason}");
                return None;
            }
        };
        let root = workspace_root().expect("the workspace root");
        let dir = std::env::temp_dir().join(format!(
            "lp-emu-layout-migration-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // The current tree's image, merged the way a package is (no
        // padding past what it writes).
        let padded = dir.join("merged-padded.bin");
        let status = Command::new(root.join("scripts/emu/build-merged-image.sh"))
            .arg(&elf)
            .arg(&padded)
            .status()
            .expect("running build-merged-image.sh");
        assert!(status.success(), "build-merged-image.sh failed");
        let bytes = std::fs::read(&padded).unwrap();
        let end = bytes
            .iter()
            .rposition(|b| *b != 0xFF)
            .map_or(0, |at| at + 1);
        let merged = dir.join("merged.bin");
        std::fs::write(&merged, &bytes[..end]).unwrap();

        let tree = dir.join("tree");
        let files = fixture_tree(&root, &tree, over_full);
        let chip = dir.join("chip.bin");
        let made = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
            .args(["hardware", "lpfs", "fixture", "--merged"])
            .arg(&merged)
            .arg("--tree")
            .arg(&tree)
            .arg("--out")
            .arg(&chip)
            .args(match layout {
                Layout::Legacy => Vec::new(),
                Layout::Current => vec![
                    OsString::from("--table"),
                    root.join("lp-fw/fw-esp32c6/partitions.csv")
                        .into_os_string(),
                ],
            })
            .output()
            .expect("running lp-cli hardware lpfs fixture");
        assert!(made.status.success(), "fixture failed:\n{}", told(&made));
        Some(Self {
            dir,
            chip,
            merged,
            files,
        })
    }

    /// Run `lp-cli <args…> --port <pty>` against the chip held in its
    /// download console; the machine runs on this thread in slices.
    fn run_cli_in_download(&self, args: &[OsString]) -> (Output, Option<Outcome>) {
        let addr = format!("127.0.0.1:{}", free_port());
        let mut m = machine(&self.chip, &addr, Strap::Download);
        let (master, slave) = TTYPort::pair().expect("a pty pair");
        let port = slave.name().expect("the pty's name");
        let sock = TcpStream::connect(&addr).expect("the machine's byte socket");
        let done = Arc::new(AtomicBool::new(false));
        let pump_done = done.clone();
        let pump_thread = std::thread::spawn(move || pump(master, sock, pump_done));

        let mut command = Command::new(env!("CARGO_BIN_EXE_lp-cli"));
        command.args(args).arg("--port").arg(port);
        let worker_done = done.clone();
        let worker = std::thread::spawn(move || {
            let out = command.output().expect("running lp-cli");
            worker_done.store(true, Ordering::Relaxed);
            out
        });

        let slice = 200u64 * lp_emu_esp32c6::memmap::CYCLES_PER_US;
        let ceiling = CLI_BUDGET_US * lp_emu_esp32c6::memmap::CYCLES_PER_US;
        let mut early = None;
        while !done.load(Ordering::Relaxed) && m.cycles() < ceiling {
            let outcome = m.run_until(&StopCondition {
                stop_cycle: Some((m.cycles() + slice).min(ceiling)),
                ..Default::default()
            });
            if !matches!(outcome, Outcome::Deadline { .. }) {
                early = Some(outcome);
                break;
            }
        }
        done.store(true, Ordering::Relaxed);
        let out = worker.join().expect("the lp-cli thread");
        let _ = pump_thread.join();
        drop(slave);
        m.flush_flash().expect("the chip is written back");
        let words: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        eprintln!(
            "emu_layout_migration: `lp-cli {}` ran {:.2} s emulated (lp-emu:esp32c6:t1)",
            words.join(" "),
            m.cycles() as f64 / (lp_emu_esp32c6::memmap::CYCLES_PER_US as f64 * 1e6)
        );
        (out, early)
    }

    /// `lp-cli hardware lpfs migrate` with this scenario's image, no
    /// questions and no hello wait (the test boots the board itself).
    fn migrate(&self, backups: &Path) -> (Output, Option<Outcome>) {
        let mut args = os(
            &[
                "hardware",
                "lpfs",
                "migrate",
                "--yes",
                "--no-verify",
                "--merged",
            ],
            &[&self.merged],
        );
        args.push("--backup-dir".into());
        args.push(backups.as_os_str().to_owned());
        self.run_cli_in_download(&args)
    }

    /// Boot the chip in the app strap and return what the firmware said.
    fn boot_app(&self) -> String {
        let addr = format!("127.0.0.1:{}", free_port());
        let mut m = machine(&self.chip, &addr, Strap::App);
        let outcome =
            m.run_until(&StopCondition::after_micros(20_000_000).exit_on("starting server loop"));
        let text = String::from_utf8_lossy(&m.usb_sj().bytes()).into_owned();
        assert!(
            matches!(outcome, Outcome::ExitMatched { .. }),
            "{outcome:?}\n{text}"
        );
        text
    }
}

/// The fixture board's files (plan Q4), written under `tree`; returns them
/// as absolute paths with their bytes, sorted.
fn fixture_tree(root: &Path, tree: &Path, over_full: bool) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    copy_dir(
        &root.join("projects/test/basic"),
        "/projects/basic",
        &mut files,
    );
    copy_dir(
        &root.join("catalog/projects/playful-choker"),
        "/projects/playful-choker",
        &mut files,
    );
    files.push((
        "/hardware.json".to_string(),
        std::fs::read(root.join("lp-core/lpc-hardware/boards/seeed/xiao-esp32-c6.json")).unwrap(),
    ));
    files.push((
        "/.lp/device.json".to_string(),
        format!("{{\"uid\":\"{UID}\",\"name\":\"Porch\"}}").into_bytes(),
    ));
    files.push((
        "/.lp/access.json".to_string(),
        br#"{"version":2,"bleEnabled":true,"open":false,"secrets":[{"label":"walk browser","kind":"browser","tier":"edit","salt":"AAECAwQFBgcICQoLDA0ODw==","iterations":1,"k":"ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8="}]}"#.to_vec(),
    ));
    // More than one 4 KB block.
    files.push(("/notes/long.txt".to_string(), noise(9_000, 1)));
    if over_full {
        // Fits 240 blocks, not 176 with the free-block floor.
        files.push(("/projects/big/blob.bin".to_string(), noise(720 * 1024, 2)));
    }
    for (path, bytes) in &files {
        let at = tree.join(path.trim_start_matches('/'));
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, bytes).unwrap();
    }
    files.sort();
    files
}

fn copy_dir(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}/{name}");
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &path, out);
        } else {
            out.push((path, std::fs::read(entry.path()).unwrap()));
        }
    }
}

/// Bytes that do not compress (xorshift).
fn noise(len: usize, seed: u32) -> Vec<u8> {
    let mut state = 0x9E37_79B9u32 ^ seed;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

fn files_of(tree: &LpfsTree) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = tree
        .files()
        .map(|(path, bytes)| (path.to_string(), bytes.to_vec()))
        .collect();
    files.sort();
    files
}

fn target_table() -> PartitionTable {
    let root = workspace_root().unwrap();
    let csv = std::fs::read_to_string(root.join("lp-fw/fw-esp32c6/partitions.csv")).unwrap();
    PartitionTable::from_csv(&csv).unwrap()
}

/// The frozen pre-2026-10 table as CSV (what a pre-repartition image's
/// package carries).
fn legacy_csv_text() -> String {
    let root = workspace_root().unwrap();
    let text = std::fs::read_to_string(
        root.join("lp-app/lpa-link/testdata/partitions-esp32c6-legacy-v1.csv"),
    )
    .unwrap();
    assert_eq!(
        PartitionTable::from_csv(&text).unwrap(),
        legacy_c6_v1_table()
    );
    text
}

fn machine(chip: &Path, addr: &str, strap: Strap) -> Esp32C6Machine {
    Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(strap)
        .efuse(EfuseIdentity {
            mac: MAC,
            ..EfuseIdentity::default()
        })
        .flash(FlashBacking::File(chip.to_path_buf()))
        .uart0(Uart0Sink::Memory)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj(UsbSjSink::Tcp(addr.to_string()))
        .usb_sj_drain(UsbSjDrain::Auto)
        // espflash's stub reads one block the C6 boot set does not map
        // (flash_over_socket.rs); non-strict reads it as zero.
        .strict(false)
        .build()
        .expect("the machine builds")
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    listener.local_addr().expect("its address").port()
}

/// Pump the pty's master end to the machine's socket and back until `done`:
/// one thread per direction, so neither waits behind the other's read
/// timeout (a bootloader read is one round trip per 4 KB block).
fn pump(tty: TTYPort, sock: TcpStream, done: Arc<AtomicBool>) {
    let mut tty_in = tty.try_clone_native().expect("a second handle on the pty");
    let mut tty_out = tty;
    let mut sock_in = sock.try_clone().expect("a second handle on the socket");
    let mut sock_out = sock;
    let _ = tty_in.set_timeout(Duration::from_millis(5));
    let _ = sock_in.set_read_timeout(Some(Duration::from_millis(5)));
    let up_done = done.clone();
    let up = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while !up_done.load(Ordering::Relaxed) {
            if let Ok(n) = tty_in.read(&mut buf)
                && n > 0
                && sock_out.write_all(&buf[..n]).is_err()
            {
                return;
            }
        }
    });
    let mut buf = [0u8; 4096];
    while !done.load(Ordering::Relaxed) {
        match sock_in.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if tty_out.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(_) => {}
        }
    }
    let _ = up.join();
}

fn only_file(dir: &Path, prefix: &str) -> PathBuf {
    let found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
        })
        .collect();
    assert_eq!(found.len(), 1, "one {prefix}* in {}", dir.display());
    found.into_iter().next().unwrap()
}

/// `words` then `paths`, as one argument list.
fn os(words: &[&str], paths: &[&Path]) -> Vec<OsString> {
    words
        .iter()
        .map(OsString::from)
        .chain(paths.iter().map(|p| p.as_os_str().to_owned()))
        .collect()
}

fn told(out: &Output) -> String {
    format!(
        "--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}
