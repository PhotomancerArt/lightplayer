//! The tree store's mount on the emulated C6, per firmware and per stored
//! image — `board_tree_store_mount.rs`'s twin with no board (plan
//! `lp2025/2026-10-16-0848-tree-store-adoption`, A2). For each ELF and each
//! raw `lpfs` image: boot the image on a chip whose `lpfs` holds it and read
//! the firmware's `[perf] fs mount` line.
//!
//! **What the number is:** `lp-emu:esp32c6:t1+net=lan`'s emulated time,
//! where a flash command takes none (`docs/debt/emulated-flash-commands-take-no-time.md`):
//! the mount's CPU, at one cycle an instruction. Report-only, never a gate;
//! silicon's figure is the board's. Env: `LP_MOUNT_ELFS` (`label=path,…`,
//! built with `heap_peak_diag`), `LP_MOUNT_IMAGES` (a directory of
//! `lpfs`-sized `.bin`).

#[path = "support/tree_store_board.rs"]
mod tree_store_board;

use std::path::PathBuf;

use tree_store_board::*;

#[test]
#[ignore = "needs fs-tree ELFs and images (module docs)"]
fn tree_store_mount_cpu_on_the_emulated_c6() {
    let (Ok(elfs), Ok(images)) = (
        std::env::var("LP_MOUNT_ELFS"),
        std::env::var("LP_MOUNT_IMAGES"),
    ) else {
        eprintln!("skipped: LP_MOUNT_ELFS and LP_MOUNT_IMAGES are not set");
        return;
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(images)
        .expect("the images directory")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "bin"))
        .collect();
    paths.sort();
    for e in elfs.split(',') {
        let (label, elf) = e.split_once('=').expect("label=path");
        for image in &paths {
            let region = std::fs::read(image).unwrap();
            let mut host = board(
                PathBuf::from(elf).as_path(),
                chip_with_lpfs(&region),
                0x7EE0_0A20,
            );
            let hello = boot(&mut host);
            let mount = line_with(&host, "[perf] fs mount ").cloned();
            let fs = line_with(&host, "[FS] ").cloned();
            // `LP_MOUNT_ECHO=<needle>`: the console lines carrying it, too.
            if let Ok(needle) = std::env::var("LP_MOUNT_ECHO") {
                for l in host.console().iter().filter(|l| l.contains(&needle)) {
                    println!("  {l}");
                }
            }
            println!(
                "{label} {}: {} | {} | fs {}",
                image.file_stem().unwrap().to_string_lossy(),
                mount.as_deref().unwrap_or("no [perf] fs mount line"),
                fs.as_deref().unwrap_or("-"),
                hello_fs(&hello)
            );
        }
    }
}
