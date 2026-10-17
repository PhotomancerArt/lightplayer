//! The tree-store cutover's one silicon run, on the sacrificial C6, CX1
//! (plan `lp2025/2026-10-16-0848-tree-store-adoption`, A6): a board on
//! today's littlefs release, holding a c40-sized project beside the basic
//! one, moved into the tree store at 128 sectors by `lp-cli hardware lpfs
//! migrate --to tree` — then every file, every project's `HashPackage`, the
//! board's own boot words. Silicon, not emulated.
//!
//! The run, in order (each step's record in `LP_MIGRATE_DIR`):
//!
//! 1. **The board as found:** its hello must be a littlefs build (no
//!    `fs.tree`), its filesystem mounted.
//! 2. **Setup** (`LP_MIGRATE_SETUP=1`): upload `projects/test/basic` as
//!    `basic` and a c40-sized project as `c40` (`replace_and_load_project`),
//!    `basic` loaded last.
//! 3. **Before:** each project's `HashPackage`; the raw `lpfs` over the
//!    bootloader (`lp-cli hardware lpfs save`), mounted on the host: the
//!    board's whole file set.
//! 4. **Migrate:** `lp-cli hardware lpfs migrate --to tree --yes` with the
//!    `fs-tree` package (`LP_MIGRATE_MANIFEST`); it waits for the board's
//!    hello itself and requires `fs: mounted` and the same uid.
//! 5. **After:** the raw `lpfs` again, now the tree store at `0x380000`,
//!    mounted on the host with `lp-tree-store`: every file equal; `hardware
//!    tree check` passes; the hello lists `fs.tree`, `fs` is `mounted`; each
//!    project's `HashPackage` is the same; the board's own `[FS] tree store
//!    mounted (…)` and `[LOADER] core @0x18000` lines; the loaded project
//!    runs (its console, kept with the run).
//! 6. **First light, both sides:** `LP_MIGRATE_BOOTS` power cycles (`board
//!    power-cycle`; default 1) before the migration and as many after it,
//!    each read to the firmware's `[perf] first light …` line (said on
//!    power-on boots only), so the record carries the littlefs and the
//!    tree-store boots of the same board and files, and the median of each.
//!
//! Env: `LP_MIGRATE_DIR` (required), `LP_MIGRATE_MANIFEST` (the shipped
//! package's manifest.json — the tree store since the default switch;
//! required), `LP_MIGRATE_LP_CLI` (an `lp-cli` copied out of `target/`,
//! which the desk's hourly clean deletes), `LP_MIGRATE_SETUP=1`,
//! `LP_MIGRATE_BOOTS` (power-on boots a side), `BOARD_HOLDER` (the lease's
//! holder; required), `LP_MIGRATE_SLUG` + `LP_MIGRATE_MAC` (another board
//! than CX1; one not tagged `sacrificial` also needs `LP_MIGRATE_ERASE_OK`
//! = its slug).
//!
//! `#[ignore]`d: it needs the desk, CX1 leased, and today's littlefs
//! release flashed (`lp-cli firmware install --release latest`).

#[path = "support/desk_board.rs"]
mod desk_board;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use desk_board::{Console, Desk, close, connect, lpfs_save, power_up_to_first_light, tree_check};
use lpa_client::LpClient;
use lpa_link::layout_migration::{LpfsGeometry, LpfsTree, read_tree};
use tree_store_board::{c40_files, project_files};

/// The projects the run carries, by id: `basic` loaded (it renders).
const PROJECTS: [&str; 2] = ["c40", "basic"];

#[test]
#[ignore = "silicon: CX1 leased, today's littlefs release on it"]
fn cx1_moves_from_littlefs_into_the_tree_store_with_every_file() {
    let dir = PathBuf::from(std::env::var("LP_MIGRATE_DIR").expect("LP_MIGRATE_DIR"));
    let manifest =
        PathBuf::from(std::env::var("LP_MIGRATE_MANIFEST").expect("LP_MIGRATE_MANIFEST"));
    let holder = std::env::var("BOARD_HOLDER").expect("BOARD_HOLDER");
    let lp_cli = std::env::var("LP_MIGRATE_LP_CLI")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_BIN_EXE_lp-cli")));
    std::fs::create_dir_all(&dir).unwrap();
    let desk = desk_for(&holder);
    let port = desk.port_wait(Duration::from_secs(10)).expect("CX1's port");
    let epoch = Instant::now();
    let console = Console::new(epoch);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();

    // 1–3: as found, setup, before.
    let before_hashes = local.block_on(&runtime, async {
        let conn = connect(&port, &console).await.expect("the board as found");
        let hello = conn.hello().expect("a hello");
        assert!(
            !hello.build.features.contains(&lpc_model::LpFeature::FsTree),
            "the board must run a littlefs build: {:?}",
            hello.build.features
        );
        // Mounted, or formatted on its first boot after the release went on.
        assert!(matches!(
            hello.hardware.fs,
            lpc_wire::FsBootState::Mounted | lpc_wire::FsBootState::Formatted
        ));
        record(&dir, "hello-before.json", &format!("{hello:#?}"));
        let mut client = LpClient::new(conn.client_io());
        if std::env::var("LP_MIGRATE_SETUP").as_deref() == Ok("1") {
            let c40 = c40_files();
            client
                .replace_and_load_project("c40", &c40)
                .await
                .expect("upload c40");
            let basic = project_files("projects/test/basic");
            client
                .replace_and_load_project("basic", &basic)
                .await
                .expect("upload basic");
        }
        let mut hashes = BTreeMap::new();
        for id in PROJECTS {
            let hash = client.hash_package(id).await.expect("hash").value;
            hashes.insert(id.to_string(), hash);
        }
        drop(client);
        // First light on littlefs: power-on boots of the board as it is.
        let mut lights = Vec::new();
        let mut conn = Some(conn);
        for _ in 0..boots() {
            let (c, light) =
                power_up_to_first_light(&desk, &port, conn.take(), &console, "littlefs").await;
            lights.push(light);
            conn = Some(c);
        }
        if let Some(c) = conn {
            close(c).await;
        }
        (hashes, serde_json::Value::Array(lights))
    });
    let (before_hashes, light_before) = before_hashes;
    record(&dir, "hashes-before.json", &format!("{before_hashes:#?}"));
    record(&dir, "first-light-before.json", &light_before.to_string());
    let raw_before = lpfs_save(&lp_cli, &port, &dir.join("before")).expect("lpfs save before");
    let before = host_files(
        &raw_before,
        LpfsGeometry {
            offset: 0x35_0000,
            block_count: 176,
        },
    );
    eprintln!(
        "before: {} files, {} B (littlefs at 0x350000)",
        before.len(),
        before.values().map(Vec::len).sum::<usize>()
    );

    // 4: migrate.
    let started = Instant::now();
    let out = Command::new(&lp_cli)
        .args([
            "hardware", "lpfs", "migrate", "--to", "tree", "--yes", "--port", &port,
        ])
        .arg("--manifest")
        .arg(&manifest)
        .arg("--backup-dir")
        .arg(dir.join("backups"))
        .output()
        .expect("lp-cli hardware lpfs migrate");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    record(&dir, "migrate.txt", &said);
    eprintln!(
        "migrate took {:.1} s (host wall clock)",
        started.elapsed().as_secs_f64()
    );
    assert!(out.status.success(), "migrate failed:\n{said}");

    // 5: after.
    let raw_after = lpfs_save(&lp_cli, &port, &dir.join("after")).expect("lpfs save after");
    let (checked, verdict) = tree_check(&lp_cli, &raw_after);
    record(&dir, "tree-check.txt", &verdict);
    assert!(checked, "hardware tree check: {verdict}");
    let after = host_files(
        &raw_after,
        LpfsGeometry {
            offset: 0x38_0000,
            block_count: 128,
        },
    );
    assert_eq!(after, before, "every file, byte for byte");
    let (after_hashes, lines, light_after) = local.block_on(&runtime, async {
        // First light on the tree store: power-on boots after the move.
        let mut lights = Vec::new();
        let mut last = None;
        for _ in 0..boots() {
            let (c, light) =
                power_up_to_first_light(&desk, &port, last.take(), &console, "tree store").await;
            lights.push(light);
            last = Some(c);
        }
        let conn = last.expect("at least one boot");
        let light = serde_json::Value::Array(lights);
        let hello = conn.hello().expect("a hello");
        record(&dir, "hello-after.json", &format!("{hello:#?}"));
        assert!(hello.build.features.contains(&lpc_model::LpFeature::FsTree));
        assert_eq!(hello.hardware.fs, lpc_wire::FsBootState::Mounted);
        let mut client = LpClient::new(conn.client_io());
        let mut hashes = BTreeMap::new();
        for id in PROJECTS {
            let hash = client.hash_package(id).await.expect("hash").value;
            hashes.insert(id.to_string(), hash);
        }
        drop(client);
        // A few seconds of the loaded project running: its boot words and
        // heartbeats land on the console.
        tokio::time::sleep(Duration::from_secs(5)).await;
        let lines: Vec<String> = console
            .lines
            .borrow()
            .iter()
            .map(|(_, l)| l.clone())
            .collect();
        close(conn).await;
        (hashes, lines, light)
    });
    record(&dir, "console.txt", &lines.join("\n"));
    record(&dir, "first-light-after.json", &light_after.to_string());
    eprintln!(
        "first light, median of {} power-on boots: littlefs {:?} ms, tree store {:?} ms",
        boots(),
        median_ms(&light_before),
        median_ms(&light_after)
    );
    assert_eq!(after_hashes, before_hashes, "every project's HashPackage");
    eprintln!("after: {} files, hashes {after_hashes:?}", after.len());
}

/// The board: CX1, or the one `LP_MIGRATE_SLUG` + `LP_MIGRATE_MAC` name.
/// A board the registry does not tag `sacrificial` is driven only when
/// `LP_MIGRATE_ERASE_OK` names its slug too (the sitting saved its flash
/// first), and then only by its MAC and under our lease.
fn desk_for(holder: &str) -> Desk {
    let Ok(slug) = std::env::var("LP_MIGRATE_SLUG") else {
        let desk = Desk::cx1(holder);
        desk.check().expect("the lease");
        return desk;
    };
    let desk = Desk {
        mac: std::env::var("LP_MIGRATE_MAC").expect("LP_MIGRATE_MAC with LP_MIGRATE_SLUG"),
        slug,
        holder: holder.to_string(),
    };
    if std::env::var("LP_MIGRATE_ERASE_OK").as_deref() == Ok(desk.slug.as_str()) {
        let info = desk.show().unwrap_or_else(|e| panic!("{e}"));
        assert!(
            info["mac"]
                .as_str()
                .is_some_and(|m| m.eq_ignore_ascii_case(&desk.mac)),
            "REFUSED: {} is not {}",
            desk.slug,
            desk.mac
        );
        assert_eq!(
            info["lease"]["holder"].as_str(),
            Some(desk.holder.as_str()),
            "REFUSED: {} is not leased by us",
            desk.slug
        );
    } else {
        desk.check().expect("the lease");
    }
    desk
}

/// Power-on boots a side (`LP_MIGRATE_BOOTS`, default 1).
fn boots() -> usize {
    std::env::var("LP_MIGRATE_BOOTS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1)
        .max(1)
}

/// The median `first light at <n> ms` of a side's boots.
fn median_ms(lights: &serde_json::Value) -> Option<u64> {
    let mut ms: Vec<u64> = lights
        .as_array()?
        .iter()
        .filter_map(|l| l["first_light"].as_str())
        .filter_map(|line| {
            let at = line.find("first light at ")? + "first light at ".len();
            line[at..].split(' ').next()?.parse().ok()
        })
        .collect();
    ms.sort_unstable();
    ms.get(ms.len() / 2).copied()
}

/// The files a raw `lpfs` region holds, mounted on the host by its magic.
fn host_files(raw: &Path, geometry: LpfsGeometry) -> BTreeMap<String, Vec<u8>> {
    let bytes = std::fs::read(raw).unwrap();
    let tree = if lpa_link::layout_migration::fs_kind::fs_in_region(&bytes)
        == Some(lpa_link::FsKind::Tree)
    {
        read_tree(&bytes, geometry).expect("the tree store mounts")
    } else {
        LpfsTree::from_image(&bytes, geometry)
            .expect("the littlefs mounts")
            .0
    };
    tree.files()
        .map(|(p, b)| (p.to_string(), b.to_vec()))
        .collect()
}

fn record(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}
