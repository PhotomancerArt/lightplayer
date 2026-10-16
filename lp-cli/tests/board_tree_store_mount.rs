//! The tree store's mount time on CX1, per firmware and per stored image
//! (plan `lp2025/2026-10-16-0848-tree-store-adoption`, A2). **Silicon, not
//! emulated.** For each firmware ELF and each raw `lpfs` image: flash the
//! firmware, write the image over `lpfs` (`espflash write-bin`, so every
//! firmware mounts the same bytes), then power-cycle the board `n` times
//! and read the firmware's own `[perf] fs mount` line at each power-on boot.
//! The images come from the host (`lp-tree-store`'s
//! `mount_cost_tests::write_scenario_images`: empty, c40, used, full) or from
//! a board (`lp-cli hardware lpfs save`: the soak's written-out partition).
//!
//! **It overwrites the board's firmware and `lpfs`.** Run it only on the
//! sacrificial board, leased. Env: `LP_MOUNT_ELFS` (`label=path,…`; the
//! images need `heap_peak_diag` for the perf line), `LP_MOUNT_IMAGES` (a
//! directory of `.bin`, each `lpfs`-sized), `LP_MOUNT_REPEATS` (default 3),
//! `LP_SOAK_DIR` (records), `BOARD_HOLDER`.

#[path = "support/desk_board.rs"]
mod desk_board;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use desk_board::{Console, Desk, close, power_up};

/// `lpfs` on the C6's 4 MB table (`lp-fw/fw-esp32c6/partitions.csv`).
const LPFS_OFFSET: &str = "0x350000";
const LPFS_LEN: u64 = 0xB_0000;

#[test]
#[ignore = "drives the sacrificial CX1 on the desk (module docs)"]
fn tree_store_mount_time_on_cx1() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(run()));
}

async fn run() {
    let dir = PathBuf::from(std::env::var("LP_SOAK_DIR").expect("LP_SOAK_DIR"));
    std::fs::create_dir_all(&dir).expect("the records directory");
    let elfs: Vec<(String, PathBuf)> = std::env::var("LP_MOUNT_ELFS")
        .expect("LP_MOUNT_ELFS")
        .split(',')
        .map(|e| {
            let (l, p) = e.split_once('=').expect("label=path");
            (l.to_string(), PathBuf::from(p))
        })
        .collect();
    let mut images: Vec<PathBuf> =
        std::fs::read_dir(std::env::var("LP_MOUNT_IMAGES").expect("LP_MOUNT_IMAGES"))
            .expect("the images directory")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "bin"))
            .collect();
    images.sort();
    for i in &images {
        assert_eq!(
            std::fs::metadata(i).unwrap().len(),
            LPFS_LEN,
            "{}: not an lpfs image",
            i.display()
        );
    }
    let repeats: usize = std::env::var("LP_MOUNT_REPEATS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let holder = std::env::var("BOARD_HOLDER").expect("BOARD_HOLDER");
    let desk = Desk::cx1(&holder);
    desk.check().unwrap_or_else(|e| panic!("{e}"));
    let epoch = Instant::now();
    let console = Console::new(epoch);
    let mut rows = Vec::new();
    for (label, elf) in &elfs {
        desk.renew(60).expect("the lease");
        let port = desk.port_wait(Duration::from_secs(30)).expect("CX1's port");
        espflash(&[
            "flash",
            "--chip",
            "esp32c6",
            "--partition-table",
            "lp-fw/fw-esp32c6/partitions.csv",
            "--flash-size",
            "4mb",
            "--port",
            &port,
            &elf.to_string_lossy(),
        ]);
        for image in &images {
            let port = desk.port_wait(Duration::from_secs(30)).expect("CX1's port");
            espflash(&[
                "write-bin",
                "--chip",
                "esp32c6",
                "--port",
                &port,
                LPFS_OFFSET,
                &image.to_string_lossy(),
            ]);
            let port = desk.port_wait(Duration::from_secs(30)).expect("CX1's port");
            let name = image.file_stem().unwrap().to_string_lossy().into_owned();
            let mut conn = None;
            let mut samples = Vec::new();
            for i in 0..repeats {
                let (c, m) =
                    power_up(&desk, &port, conn, &console, &format!("{label} {name} #{i}")).await;
                conn = Some(c);
                samples.push(m);
            }
            if let Some(c) = conn {
                close(c).await;
            }
            let us: Vec<u64> = samples
                .iter()
                .filter_map(|m| m["mount_us"].as_u64())
                .collect();
            eprintln!("== {label} {name}: mount {us:?} us");
            rows.push(serde_json::json!({
                "firmware": label,
                "elf": elf,
                "image": name,
                "mount_us": us,
                "boots": samples,
            }));
        }
    }
    let out = dir.join("mount-times.json");
    std::fs::write(&out, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
    let mut log = String::new();
    for (t, l) in console.take() {
        log.push_str(&format!("{t:.3} {l}\n"));
    }
    std::fs::write(dir.join("mount-console.txt"), log).unwrap();
    eprintln!("records: {}", out.display());
}

/// `espflash <args>` from the workspace root, which must succeed.
fn espflash(args: &[&str]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let out = Command::new("espflash")
        .args(args)
        .current_dir(root)
        .output()
        .expect("espflash");
    assert!(
        out.status.success(),
        "espflash {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
