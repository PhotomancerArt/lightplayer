//! The `fs-tree` firmware cleaning its store up on quiet ticks
//! (`LpFs::idle_tick`, paced by `lpfs::IdlePacer`; the tree store's
//! `collect_step`, one piece a step), on the emulated C6
//! while a project plays:
//!
//! 1. Three uploads of a c40-shaped project (`projects/test/basic` and its
//!    documents, every document edited each time) leave garbage.
//! 2. **Under two quiet seconds** the board touches no `lpfs` sector.
//! 3. **Past them** it does (idle steps: forgets and collections), and every
//!    WS281x frame the pad carried meanwhile decodes complete, with no bad
//!    pulse — the C6 sends a frame blocking, inside the frame, and the step
//!    runs after it, so a step can lengthen a frame but never cut one.
//! 4. **After a power cycle** the project's files are as they were, and the
//!    mount reads fewer trusted sectors.
//!
//! Frame gaps are reported, never gated: emulated `t1` time, CPU only (the
//! emulator's flash commands take no time; on silicon an erase is ~19 ms).
//! Emulated: `lp-emu:esp32c6:t1+net=lan`. `#[ignore]`d: it needs a built
//! `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`).

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
use tree_store_board::*;
use tree_store_measure::*;

const PROJECT: &str = "tree-idle";
/// t1 runs the core at 160 MHz: cycles per µs.
const CYCLES_PER_US: u64 = 160;

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; run by hand (see the module docs)"]
fn quiet_ticks_clean_up_while_the_project_plays() {
    let Some(elf) = elf("emu_tree_store_idle", &FwImage::SHIPPED_HEAP_PEAK) else {
        return;
    };
    let mut host = board(&elf, Vec::new(), 0x1D1E_0001);
    boot(&mut host);
    // c40 (≈ 216 KB) three times: the store at ~ a third live, the rest
    // garbage the write path left, below the low water.
    let base = c40_files();
    let mut hash = String::new();
    for i in 0..3 {
        hash = upload(&mut host, PROJECT, &edited(&base, i));
    }
    // The mount as the uploads left the store (a power cycle).
    let mut host = power_cycle(host, 0x1D1E_0003);
    boot(&mut host);
    let mount_before = mount_us(&host).unwrap_or(0);
    // The load's own writes (its startup choice) land in the first frames;
    // then a file write over the wire ends the quiet again.
    let t = host.now_us();
    host.run_until(t + 1_000_000, None).expect("run");
    {
        let mut client = LpClient::new(&mut host).with_request_ids_from(3_000);
        block_on(client.fs_write(lpfs::LpPath::new("/notes.txt"), b"quiet from here".to_vec()))
            .expect("a write");
    }
    let trusted_before = trusted_sectors(&host);
    // What the host's own store would do with these bytes (a cross-check).
    let host_steps = {
        let region = lpfs(&chip(&host)).to_vec();
        let mut st = lp_tree_store::TreeStore::mount(
            nor_of(&region),
            lp_tree_store::SoftSha256,
            lp_tree_store::StoreConfig::default(),
        )
        .map_err(|(e, ..)| e)
        .unwrap();
        let mut n = 0;
        while st.collect_step().unwrap() {
            n += 1;
        }
        n
    };
    watch(&host);

    // Under two quiet seconds: nothing.
    let t0 = host.now_us();
    host.run_until(t0 + 1_500_000, None).expect("run");
    let quiet_short = flash_count(&host);
    assert_eq!(
        (quiet_short.programs, quiet_short.erases),
        (0, 0),
        "the store was written before two quiet seconds"
    );

    // Past them: idle steps, while the project plays.
    let pad_frames_before = frames_on_pads(&host);
    let t1 = host.now_us();
    host.run_until(t1 + 20_000_000, None).expect("run");
    let quiet = flash_count(&host).since(quiet_short);
    assert!(
        quiet.programs > 0,
        "no idle step touched the store in 20 quiet seconds"
    );
    let (frames, bad, gaps) = frames_since(&host, &pad_frames_before);
    assert!(frames > 0, "the pad carried no frame");
    assert_eq!(bad, 0, "{bad} frames did not decode complete");
    let mut sorted = gaps.clone();
    sorted.sort_unstable();
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0);
    let worst = sorted.last().copied().unwrap_or(0);
    println!(
        "idle window (20 s emulated): {} programs ({} B), {} erases (silicon estimate {:.0} ms of \
         flash work; the host store predicted {host_steps} steps, its mark one of them); {frames} frames, all \
         complete; frame gap median {median} µs, worst {worst} µs (t1, CPU only)",
        quiet.programs,
        quiet.program_bytes,
        quiet.erases,
        silicon_estimate_ms(quiet)
    );

    // After a power cycle: the same files; fewer trusted sectors to mount.
    let mut host = power_cycle(host, 0x1D1E_0002);
    boot(&mut host);
    let mut client = LpClient::new(&mut host).with_request_ids_from(5_000);
    assert_eq!(
        block_on(client.hash_package(PROJECT)).expect("hash").value,
        hash
    );
    let trusted_after = trusted_sectors(&host);
    println!(
        "trusted sectors {trusted_before} -> {trusted_after}; mount {mount_before} us -> {} us \
         (t1, CPU only)",
        mount_us(&host).unwrap_or(0)
    );
    assert!(trusted_after < trusted_before);
}

// ---- helpers ---------------------------------------------------------------

fn upload(host: &mut EmuLinkHost<C6Board>, id: &str, files: &[(String, Vec<u8>)]) -> String {
    let mut client = LpClient::new(host).with_request_ids_from(1_000);
    block_on(client.replace_and_load_project(id, files))
        .unwrap_or_else(|e| panic!("upload {id}: {e}"));
    block_on(client.hash_package(id)).expect("hash").value
}

/// `files` with every document under `docs/` changed (nothing loads them),
/// so each upload's documents are new and the last one's are garbage.
fn edited(files: &[(String, Vec<u8>)], i: u32) -> Vec<(String, Vec<u8>)> {
    files
        .iter()
        .map(|(p, b)| {
            let mut b = b.clone();
            if p.starts_with("docs/") {
                b.extend_from_slice(format!(" edit {i}").as_bytes());
            }
            (p.clone(), b)
        })
        .collect()
}

/// Sectors of `lpfs` whose header a mount trusts (the magic).
fn trusted_sectors(host: &EmuLinkHost<C6Board>) -> usize {
    lpfs(&chip(host))
        .chunks(4096)
        .filter(|s| s[..4] == *b"LTS1")
        .count()
}

/// Frames decoded so far, per routed pad.
fn frames_on_pads(host: &EmuLinkHost<C6Board>) -> Vec<(u8, usize)> {
    host.board
        .machine
        .routed_pads()
        .into_iter()
        .map(|(pad, _)| (pad.0, host.board.machine.frames(pad.0).len()))
        .collect()
}

/// Frames decoded since `before` (all pads): how many, how many not
/// complete, and the gaps between frame starts (µs) on each pad.
fn frames_since(host: &EmuLinkHost<C6Board>, before: &[(u8, usize)]) -> (usize, usize, Vec<u64>) {
    let (mut n, mut bad, mut gaps) = (0, 0, Vec::new());
    for &(pad, from) in before {
        let frames = host.board.machine.frames(pad);
        // The last frame may still be on the wire.
        let done = &frames[from..frames.len().saturating_sub(1)];
        // A pad that never carried a pixel is no strip (the status LED's
        // blink reads as a one-pulse "frame").
        if !done.iter().any(|f| f.bits >= 24) {
            continue;
        }
        n += done.len();
        bad += done.iter().filter(|f| !f.is_complete()).count();
        for f in done.iter().filter(|f| !f.is_complete()) {
            println!(
                "  pad {pad} frame {} at {} us: {} bits, {} errors, {} trailing, reset {:?}",
                f.n,
                f.start / CYCLES_PER_US,
                f.bits,
                f.error_count,
                f.trailing_bits,
                f.reset_cycles
            );
        }

        gaps.extend(
            done.windows(2)
                .map(|w| w[1].start.saturating_sub(w[0].start) / CYCLES_PER_US),
        );
    }
    (n, bad, gaps)
}
