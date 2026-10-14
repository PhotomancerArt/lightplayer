//! The tree store's emulated perf, `fs-tree` vs littlefs (plan
//! `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, P8, Q17/Q18).
//! The unsplit images with `heap_peak_diag` (for the `[perf] fs mount` line),
//! same project, same emulator.
//!
//! Figures are **emulated time** (`t1`: one cycle per instruction at
//! 160 MHz) and **flash commands** (in-range programs, their bytes, erases;
//! SPI1 reads chip-wide). The emulator's flash commands take no time, so
//! every emulated time here is CPU only; a separately labelled silicon
//! ESTIMATE adds what the commands would cost on a part (erase ≈ 21 ms,
//! program ≈ 0.5 ms per 256 B). Round trips are a request's send to its
//! answer, at the link host's 250 µs slice, averaged over repeats; they
//! include the link and the server's tick, the same for both images.
//!
//! - mount: empty (after a first format), c40 (the c40-shaped project),
//!   full (c40 plus [`FILL_FILES`] incompressible 4 KiB files, or until the
//!   store says no space: about two thirds of the partition);
//! - commit: a save (one 1.7 KB document), a 4 KiB append (`WriteChunk` at
//!   an offset: the `file_size` check walks the directories);
//! - the largest c40 file read; a whole-project load (`LoadProject`, which
//!   compiles).
//!
//! Q18: the one-entry directory cache is built only if the c40 load or the
//! per-chunk appends cost more than 1.5x littlefs's emulated time.
//!
//! Emulated: `lp-emu:esp32c6:t1+net=lan`. Never hardware-validated, never a
//! gate. `#[ignore]`d; `LP_TREE_PERF_FS=fs-tree|littlefs` runs one image.

#[path = "support/editor_reads.rs"]
mod editor_reads;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;
#[path = "support/tree_store_measure.rs"]
mod tree_store_measure;

use editor_reads::block_on;
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::test_support::FwImage;
use lpa_client::LpClient;
use lpfs::LpPath;
use tree_store_board::*;
use tree_store_measure::*;

/// The "full" mount's fill: 4 KiB files on top of c40.
const FILL_FILES: usize = 60;

/// One operation's figures.
#[derive(Clone, Copy, Debug, Default)]
struct Op {
    us: f64,
    flash: FlashCount,
}

impl Op {
    /// The table row (also on stderr as it is made: a long walk shows its
    /// progress).
    fn row(&self, label: &str, what: &str) -> String {
        let row = format!(
            "| {label} | {what} | {:.0} | {} | {} | {} | {} | {:.1} |",
            self.us,
            self.flash.programs,
            self.flash.program_bytes,
            self.flash.erases,
            self.flash.reads,
            silicon_estimate_ms(self.flash)
        );
        eprintln!("{row}");
        row
    }
}

#[test]
#[ignore = "needs built fw-esp32c6 ELFs; run by hand (see the module docs)"]
fn perf_mount_commit_read_and_load() {
    let only = std::env::var("LP_TREE_PERF_FS")
        .ok()
        .filter(|s| !s.is_empty());
    let mut rows = Vec::new();
    for (label, image) in [
        ("fs-tree", FwImage::FS_TREE_HEAP_PEAK),
        ("littlefs", FwImage::SHIPPED_HEAP_PEAK),
    ] {
        if only.as_deref().is_some_and(|o| o != label) {
            continue;
        }
        let Some(elf) = elf("emu_tree_store_perf", &image) else {
            return;
        };
        for line in measure(label, &elf) {
            rows.push(line);
        }
    }
    println!(
        "\nperf — configuration=lp-emu:esp32c6:t1+net=lan, unsplit, emulated µs are CPU only \
         (flash commands take no emulated time)"
    );
    println!(
        "| image | operation | emulated µs | programs | program bytes | erases | SPI1 reads | silicon estimate ms (flash only) |"
    );
    println!("|---|---|---:|---:|---:|---:|---:|---:|");
    for row in rows {
        println!("{row}");
    }
}

fn measure(label: &str, elf: &std::path::Path) -> Vec<String> {
    let mut rows = Vec::new();
    let c40 = c40_files();

    // Mount, empty: format on the first boot, mount on the second.
    let host = board(elf, Vec::new(), 0x9E7F_0001);
    let host = boot_counted(host, &mut rows, label, "first boot (format)");
    let mut host = power_cycle(host, 0x9E7F_0002);
    watch(&host);
    let before = flash_count(&host);
    boot(&mut host);
    mount_row(&host, &mut rows, label, "mount, empty", before);

    // c40: the push, then the mount.
    watch(&host);
    let before = flash_count(&host);
    let t0 = host.board.machine.micros();
    {
        let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
        block_on(client.replace_and_load_project("demo", &c40)).expect("upload c40");
    }
    rows.push(
        Op {
            us: (host.board.machine.micros() - t0) as f64,
            flash: flash_count(&host).since(before),
        }
        .row(label, "push c40 (132 files, ~216 KB) and load"),
    );
    let mut host = power_cycle(host, 0x9E7F_0003);
    watch(&host);
    let before = flash_count(&host);
    boot(&mut host);
    mount_row(&host, &mut rows, label, "mount, c40", before);

    // Commit: a save, five times.
    let doc = c40
        .iter()
        .find(|(p, _)| p == "docs/doc-000.json")
        .unwrap()
        .1
        .clone();
    let op = repeat(&mut host, 5, |client, i| {
        let mut d = doc.clone();
        d.extend_from_slice(format!(" {i}").as_bytes());
        block_on(client.fs_write(LpPath::new("/projects/demo/docs/doc-000.json"), d)).map(drop)
    });
    rows.push(op.row(label, "save (1.7 KB document), each"));

    // Commit: 4 KiB appends (a 20 KiB file: one write, four appends).
    let big: Vec<u8> = (0..20_480u32)
        .map(|i| b"lightplayer "[(i % 12) as usize])
        .collect();
    let requests = lpa_client::project_deploy::project_write_requests(
        "demo",
        [lpa_client::ProjectDeployFile::new(
            "big.txt".to_string(),
            big,
        )],
    );
    let first = requests[0].clone();
    {
        let mut client = LpClient::new(&mut host).with_request_ids_from(20_000);
        block_on(client.send_request(first)).expect("the first chunk");
    }
    let appends = requests[1..].to_vec();
    let n = appends.len();
    let mut it = appends.into_iter();
    let op = repeat(&mut host, n, |client, _| {
        block_on(client.send_request(it.next().expect("a chunk"))).map(drop)
    });
    rows.push(op.row(label, "4 KiB append (with its file_size check), each"));

    // The largest file read, five times.
    let (largest, _) = c40.iter().max_by_key(|(_, b)| b.len()).unwrap().clone();
    let path = format!("/projects/demo/{largest}");
    let op = repeat(&mut host, 5, |client, _| {
        block_on(client.fs_read(LpPath::new(&path))).map(drop)
    });
    rows.push(op.row(label, "read the largest c40 file, each"));

    // A whole-project load.
    let op = repeat(&mut host, 1, |client, _| {
        block_on(client.stop_all_projects())?;
        block_on(client.project_load(&lpa_client::project_deploy::project_load_path("demo")))
            .map(drop)
    });
    rows.push(op.row(label, "whole-project load (stop + LoadProject c40)"));

    // Full: 4 KiB files until no space (or FILL_FILES), then the mount.
    let fill: Vec<u8> = (0..4_096u32)
        .map(|i| ((i * 2_654_435_761) >> 24) as u8)
        .collect();
    let mut written = 0;
    {
        let mut client = LpClient::new(&mut host).with_request_ids_from(40_000);
        for i in 0..FILL_FILES {
            let p = format!("/fill/f{i:03}.bin");
            if block_on(client.fs_write(LpPath::new(&p), fill.clone())).is_err() {
                break;
            }
            written += 1;
        }
    }
    let mut host = power_cycle(host, 0x9E7F_0004);
    watch(&host);
    let before = flash_count(&host);
    boot(&mut host);
    mount_row(
        &host,
        &mut rows,
        label,
        &format!("mount, full (c40 + {written} × 4 KiB)"),
        before,
    );
    rows
}

fn boot_counted(
    mut host: EmuLinkHost<C6Board>,
    rows: &mut Vec<String>,
    label: &str,
    what: &str,
) -> EmuLinkHost<C6Board> {
    watch(&host);
    let before = flash_count(&host);
    boot(&mut host);
    mount_row(&host, rows, label, what, before);
    host
}

fn mount_row(
    host: &EmuLinkHost<C6Board>,
    rows: &mut Vec<String>,
    label: &str,
    what: &str,
    before: FlashCount,
) {
    let us = mount_us(host)
        .unwrap_or_else(|| panic!("no [perf] fs mount line:\n{}", host.console().join("\n")));
    rows.push(
        Op {
            us: us as f64,
            // Boot's commands to the hello, mount included (no more is
            // written before the hello but what the boot writes).
            flash: flash_count(host).since(before),
        }
        .row(label, what),
    );
}

/// `n` requests, each timed send to answer; the mean and the flash per one.
fn repeat(
    host: &mut EmuLinkHost<C6Board>,
    n: usize,
    mut f: impl FnMut(&mut LpClient<&mut EmuLinkHost<C6Board>>, usize) -> lpa_client::ClientResult<()>,
) -> Op {
    watch(host);
    let before = flash_count(host);
    let mut total = 0u64;
    for i in 0..n {
        let t0 = host.board.machine.micros();
        {
            let mut client =
                LpClient::new(&mut *host).with_request_ids_from(60_000 + i as u64 * 10);
            f(&mut client, i).unwrap_or_else(|e| panic!("request {i}: {e}"));
        }
        total += host.board.machine.micros() - t0;
    }
    let flash = flash_count(host).since(before);
    Op {
        us: total as f64 / n as f64,
        flash: FlashCount {
            programs: flash.programs / n as u64,
            program_bytes: flash.program_bytes / n as u64,
            erases: flash.erases / n as u64,
            reads: flash.reads / n as u64,
        },
    }
}
