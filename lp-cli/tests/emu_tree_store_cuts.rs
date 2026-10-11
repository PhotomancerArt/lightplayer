//! Power cuts at flash operations on the emulated C6, against the `fs-tree`
//! firmware, each followed by a power cycle (plan
//! `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, P7). **Zero
//! failures is the only pass.**
//!
//! The oracle (two layers, both required) and the dry run that builds its
//! model are `support/tree_store_oracle.rs`'s module docs. In short: the
//! cut image, mounted on the host with `lp-tree-store`, is a committed state
//! of the dry run between the last acknowledged step and the one in flight;
//! the rebooted board says `mounted`, serves exactly what its flash holds and
//! what the cut image held; and the project `/lightplayer.json` names runs a
//! package that slot had, with a panel value that was written.
//!
//! **Scenarios:** `format` (cuts during the first boot's format: the board
//! must find `NoStore` and format again), `push` (Studio's two-slot push of
//! `projects/test/basic` with an edited shader: stop, clear `demo-b`, write,
//! load `demo-b`, remove `demo`), `save` (one file written whole), `panel`
//! (two panel writes, each auto-saved), `switch` (load the other project).
//!
//! **Sampling (D6):** per scenario a dry run counts the in-range commands
//! (`T`). The forced set is the first and last op, every erase, and the two
//! programs before each root was seen committed; `calibrated` cuts the
//! forced set and a seeded sample, each forced erase shape
//! (`calibrated_zeroing`, `_all_zero`, `_erasing`, `_reads_ff_weak`,
//! `_reads_ff`) cuts every erase, `clean` (the control model) the forced
//! set. The long form (`LP_TREE_CUTS=long`, `just
//! walk-tree-store-cuts-emu`) adds a larger sample and `byte_prefix` and
//! `random_bits`.
//!
//! **littlefs control** (`LP_TREE_CUTS_CONTROL=1`; the long form sets it):
//! the same scenarios but `format` on today's shipped image, report-only —
//! never asserted.
//!
//! **Replay:** a failure prints its key `(scenario, seed, index, model)` and
//! keeps the cut chip under `target/tree-store-cuts/`; `REPLAY=<scenario>:
//! <seed>:<index>:<model>` runs that one cut.
//!
//! Emulated: `lp-emu:esp32c6:t1+net=lan+flash-cut`, direct load. Never
//! hardware-validated; emulated flash commands take no time. `#[ignore]`d:
//! it needs built `fw-esp32c6` ELFs (`LP_EMU_BUILD_FW=1`);
//! `just test-emu-c6-cli-boards` runs the CI subset.

#[path = "support/editor_reads.rs"]
mod editor_reads;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;
#[path = "support/tree_store_oracle.rs"]
mod tree_store_oracle;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use editor_reads::block_on;
use lp_emu_esp_common::engine::flash_cut::TearModel;
use lp_emu_esp32c6::test_support::FwImage;
use lpa_client::LpClient;
use tree_store_board::*;
use tree_store_oracle::*;

/// The sample's seed (one fixed seed: two runs are the same run).
const SEED: u64 = 0x5EED_0005;

/// Erases the CI subset cuts per scenario (each under every erase shape).
const CI_ERASES: usize = 6;

/// Emulated time after a `LoadProject` for its first frames (and the
/// `startup_project` write they trigger).
const LOAD_SETTLE_US: u64 = 3_000_000;

/// Emulated time after a panel write for its auto-save: the first save
/// waits out `PANEL_STATE_WRITE_INTERVAL_MS` (10 s) from the boot.
const PANEL_SETTLE_US: u64 = 11_000_000;

#[test]
#[ignore = "needs built fw-esp32c6 ELFs; `just test-emu-c6-cli-boards` runs it"]
fn a_cut_at_any_flash_operation_loses_nothing_committed() {
    let Some(tree) = fs_tree_elf("emu_tree_store_cuts") else {
        return;
    };
    let long = std::env::var("LP_TREE_CUTS").as_deref() == Ok("long");
    let control = match std::env::var("LP_TREE_CUTS_CONTROL").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => long,
    };
    let replay = std::env::var("REPLAY").ok().filter(|r| !r.is_empty());
    let only: Option<Vec<String>> = std::env::var("LP_TREE_CUTS_SCENARIOS")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(str::to_string).collect());
    let started = Instant::now();

    let mut failures = Vec::new();
    let mut table = Vec::new();
    for scenario in scenarios(&tree, Fs::Tree, long) {
        if only
            .as_ref()
            .is_some_and(|o| !o.iter().any(|n| n == scenario.name))
        {
            continue;
        }
        let row = walk(&tree, &scenario, Fs::Tree, long, replay.as_deref());
        failures.extend(row.failures.iter().cloned());
        table.push(row);
    }
    let mut control_rows = Vec::new();
    if control && replay.is_none() {
        if let Some(shipped) = elf("emu_tree_store_cuts control", &FwImage::SHIPPED) {
            for scenario in scenarios(&shipped, Fs::Littlefs, long) {
                if scenario.name == "format"
                    || only
                        .as_ref()
                        .is_some_and(|o| !o.iter().any(|n| n == scenario.name))
                {
                    continue;
                }
                control_rows.push(walk(&shipped, &scenario, Fs::Littlefs, long, None));
            }
        }
    }

    println!(
        "\ntree-store cut walk — configuration=lp-emu:esp32c6:t1+net=lan+flash-cut, \
         {} form, {:.0} s wall",
        if long { "long" } else { "CI" },
        started.elapsed().as_secs_f64()
    );
    print_table("fs-tree", &table);
    if !control_rows.is_empty() {
        print_table("littlefs control (report-only)", &control_rows);
        for row in &control_rows {
            for f in &row.failures {
                println!("  control: {f}");
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} cut failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---- the walk ----------------------------------------------------------------

/// One scenario's results.
struct Row {
    name: &'static str,
    total_ops: u64,
    forced: usize,
    erases: usize,
    /// model → (cuts, failures)
    by_model: BTreeMap<&'static str, (u32, u32)>,
    /// landed → count
    landed: BTreeMap<&'static str, u32>,
    not_reached: u32,
    failures: Vec<String>,
    seconds: f64,
}

fn walk(elf: &Path, scenario: &Scenario, fs: Fs, long: bool, replay: Option<&str>) -> Row {
    let started = Instant::now();
    let dry = dry_run(elf, scenario, fs, 0xC07_0001);
    // CI: the panel scenario lives 11 emulated seconds a cut (the auto-save
    // waits out its 10 s spacing from boot), so it cuts its forced set only.
    let sample = match (long, scenario.name) {
        (true, _) => 120,
        (false, "panel") => 0,
        (false, _) => 12,
    };
    // The forced set: every erase in the long form; in CI a seeded handful
    // of them (the first boot's format alone erases all 176 sectors, and
    // six models on each is the long form's ~1,000 cuts).
    let all_erases: BTreeSet<u64> = dry.erases.iter().copied().collect();
    let erases: BTreeSet<u64> = if long {
        all_erases.clone()
    } else {
        pick(&all_erases, CI_ERASES, SEED)
    };
    let mut forced: BTreeSet<u64> = cut_indices(&dry, SEED, 0)
        .into_iter()
        .filter(|i| !all_erases.contains(i) || erases.contains(i))
        .collect();
    forced.extend(erases.iter().copied());
    let mut indices: BTreeSet<u64> = cut_indices(&dry, SEED, sample)
        .into_iter()
        .filter(|i| !all_erases.contains(i) || erases.contains(i))
        .collect();
    indices.extend(forced.iter().copied());

    let mut plan: Vec<CutKey> = Vec::new();
    let mut push = |index: u64, tear: TearModel| {
        plan.push(CutKey {
            index,
            tear,
            seed: SEED ^ index.wrapping_mul(0x9E37_79B9) ^ tear as u64,
        });
    };
    for &i in &indices {
        push(i, TearModel::Calibrated);
    }
    for tear in [
        TearModel::CalibratedZeroing,
        TearModel::CalibratedAllZero,
        TearModel::CalibratedErasing,
        TearModel::CalibratedReadsFfWeak,
        TearModel::CalibratedReadsFf,
    ] {
        for &i in &erases {
            push(i, tear);
        }
    }
    for &i in &forced {
        push(i, TearModel::Clean);
    }
    if long {
        for &i in &indices {
            push(i, TearModel::BytePrefix);
            push(i, TearModel::RandomBits);
        }
    }
    if let Some(key) = replay {
        let parts: Vec<&str> = key.split(':').collect();
        assert_eq!(parts.len(), 4, "REPLAY=<scenario>:<seed>:<index>:<model>");
        plan.clear();
        if parts[0] == scenario.name {
            plan.push(CutKey {
                index: parts[2].parse().expect("an index"),
                tear: TearModel::from_name(parts[3]).expect("a model"),
                seed: parts[1].parse().expect("a seed"),
            });
        }
    }

    let threads: usize = std::env::var("LP_TREE_CUTS_THREADS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get().min(8)));
    let results = Mutex::new(Vec::new());
    let next = Mutex::new(0usize);
    let keep = keep_dir();
    std::thread::scope(|s| {
        for t in 0..threads {
            let (plan, results, next, dry, keep) = (&plan, &results, &next, &dry, &keep);
            s.spawn(move || {
                loop {
                    let at = {
                        let mut n = next.lock().unwrap();
                        let at = *n;
                        *n += 1;
                        at
                    };
                    let Some(&key) = plan.get(at) else {
                        break;
                    };
                    let verdict = cut(elf, scenario, fs, dry, key, 0xC07_1000 + t as u32, keep);
                    results.lock().unwrap().push((key, verdict));
                }
            });
        }
    });

    let mut row = Row {
        name: scenario.name,
        total_ops: dry.total_ops,
        forced: forced.len(),
        erases: erases.len(),
        by_model: BTreeMap::new(),
        landed: BTreeMap::new(),
        not_reached: 0,
        failures: Vec::new(),
        seconds: 0.0,
    };
    for (key, verdict) in results.into_inner().unwrap() {
        let entry = row.by_model.entry(key.tear.name()).or_default();
        match verdict {
            CutVerdict::Pass { landed, .. } => {
                entry.0 += 1;
                *row.landed.entry(landed).or_default() += 1;
            }
            CutVerdict::NotReached => row.not_reached += 1,
            CutVerdict::Fail(why) => {
                entry.0 += 1;
                entry.1 += 1;
                row.failures
                    .push(format!("{} {:?}: {why}", scenario.name, fs));
            }
        }
    }
    row.seconds = started.elapsed().as_secs_f64();
    row
}

fn print_table(title: &str, rows: &[Row]) {
    println!("\n{title}");
    println!(
        "| scenario | T (ops) | forced | erases | cuts | failures | landed old/between/new | by model (cuts/failures) | wall s |"
    );
    println!("|---|---:|---:|---:|---:|---:|---|---|---:|");
    let (mut cuts, mut fails) = (0, 0);
    for r in rows {
        let c: u32 = r.by_model.values().map(|v| v.0).sum();
        let f: u32 = r.by_model.values().map(|v| v.1).sum();
        cuts += c;
        fails += f;
        let models = r
            .by_model
            .iter()
            .map(|(m, (c, f))| format!("{m} {c}/{f}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "| {} | {} | {} | {} | {} | {} | {}/{}/{} | {} | {:.0} |",
            r.name,
            r.total_ops,
            r.forced,
            r.erases,
            c,
            f,
            r.landed.get("old").unwrap_or(&0),
            r.landed.get("between").unwrap_or(&0),
            r.landed.get("new").unwrap_or(&0),
            models,
            r.seconds
        );
    }
    println!("| **total** | | | | **{cuts}** | **{fails}** | | | |");
}

// ---- the scenarios -------------------------------------------------------------

/// The scenarios, on `elf`'s filesystem.
fn scenarios(elf: &Path, fs: Fs, long: bool) -> Vec<Scenario> {
    // The partition's size (Q23): the real table's 176 sectors, or a
    // fixture table's `lpfs` of `LP_TREE_CUTS_SECTORS` (the long form's 128)
    // on the chip, which the firmware and the cut both read at runtime.
    let sectors: u32 = std::env::var("LP_TREE_CUTS_SECTORS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(176);
    let blank = if sectors == 176 {
        Vec::new()
    } else {
        chip_with_lpfs_sectors(sectors)
    };
    let basic = project_files("projects/test/basic");
    let edited: Vec<(String, Vec<u8>)> = basic
        .iter()
        .map(|(p, b)| {
            if p == "shader.glsl" {
                let mut b = b.clone();
                b.extend_from_slice(b"\n// pushed again: the second version\n");
                (p.clone(), b)
            } else {
                (p.clone(), b.clone())
            }
        })
        .collect();
    // The starting chip: `demo` (and `other`) uploaded, `demo` loaded.
    let (chip, demo_hash) = {
        let mut host = board(elf, blank.clone(), 0xC07_0100);
        boot(&mut host);
        let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
        block_on(client.replace_and_load_project("other", &basic)).expect("upload other");
        block_on(client.replace_and_load_project("demo", &basic)).expect("upload demo");
        let hash = block_on(client.hash_package("demo")).expect("hash").value;
        (chip(&host), hash)
    };
    let edited_hash = {
        let mut host = board(elf, chip.clone(), 0xC07_0101);
        boot(&mut host);
        let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
        block_on(client.replace_and_load_project("probe", &edited)).expect("upload probe");
        block_on(client.hash_package("probe")).expect("hash").value
    };
    let set = |pairs: &[(&str, &str)]| -> BTreeSet<(String, String)> {
        pairs
            .iter()
            .map(|(s, h)| (s.to_string(), h.to_string()))
            .collect()
    };
    let shader = "/projects/demo/shader.glsl".to_string();
    let saved = edited
        .iter()
        .find(|(p, _)| p == "shader.glsl")
        .unwrap()
        .1
        .clone();
    let mut out = Vec::new();
    if fs == Fs::Tree {
        out.push(Scenario {
            name: "format",
            chip: blank.clone(),
            from_power_on: true,
            steps: Vec::new(),
            hashes: BTreeSet::new(),
            panel_values: Vec::new(),
        });
    }
    out.push(Scenario {
        name: "push",
        chip: chip.clone(),
        from_power_on: false,
        steps: vec![
            Step::Stop,
            Step::DeleteDir("demo-b".to_string()),
            Step::WriteProject("demo-b".to_string(), edited.clone()),
            Step::Load("demo-b".to_string()),
            // The server persists `startup_project` once the load has
            // survived its first frames: after the answer.
            Step::Wait(LOAD_SETTLE_US),
            Step::DeleteDir("demo".to_string()),
        ],
        hashes: set(&[("demo", &demo_hash), ("demo-b", &edited_hash)]),
        panel_values: Vec::new(),
    });
    if long {
        // A c40-class push: `catalog/projects/rocaille` (36 KB, a heavier
        // shader than basic's), edited, into the other slot.
        let mut c40 = project_files("catalog/projects/rocaille");
        for (p, b) in &mut c40 {
            if p.ends_with(".glsl") {
                b.extend_from_slice(b"\n// pushed by the cut walk\n");
            }
        }
        let c40_hash = {
            let mut host = board(elf, chip.clone(), 0xC07_0102);
            boot(&mut host);
            let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
            block_on(client.replace_and_load_project("probe", &c40)).expect("upload probe c40");
            block_on(client.hash_package("probe")).expect("hash").value
        };
        out.push(Scenario {
            name: "push-c40",
            chip: chip.clone(),
            from_power_on: false,
            steps: vec![
                Step::Stop,
                Step::DeleteDir("demo-b".to_string()),
                Step::WriteProject("demo-b".to_string(), c40),
                Step::Load("demo-b".to_string()),
                Step::Wait(LOAD_SETTLE_US),
                Step::DeleteDir("demo".to_string()),
            ],
            hashes: set(&[("demo", &demo_hash), ("demo-b", &c40_hash)]),
            panel_values: Vec::new(),
        });
    }
    out.push(Scenario {
        name: "save",
        chip: chip.clone(),
        from_power_on: false,
        steps: vec![Step::WriteFile(shader, saved)],
        hashes: set(&[("demo", &demo_hash), ("demo", &edited_hash)]),
        panel_values: Vec::new(),
    });
    out.push(Scenario {
        name: "panel",
        chip: chip.clone(),
        from_power_on: false,
        // The first write saves on the next frames; a second within 10 s
        // waits out `PANEL_STATE_WRITE_INTERVAL_MS` (the long form only).
        steps: if long {
            vec![
                Step::PanelTime(1.5),
                Step::Wait(PANEL_SETTLE_US),
                Step::PanelTime(2.5),
                Step::Wait(11_000_000),
            ]
        } else {
            vec![Step::PanelTime(1.5), Step::Wait(PANEL_SETTLE_US)]
        },
        hashes: set(&[("demo", &demo_hash)]),
        panel_values: vec![1.5, 2.5],
    });
    out.push(Scenario {
        name: "switch",
        chip,
        from_power_on: false,
        steps: vec![
            Step::Stop,
            Step::Load("other".to_string()),
            Step::Wait(LOAD_SETTLE_US),
        ],
        hashes: set(&[("demo", &demo_hash), ("other", &demo_hash)]),
        panel_values: Vec::new(),
    });
    out
}

/// `n` of `set`, seeded.
fn pick(set: &BTreeSet<u64>, n: usize, seed: u64) -> BTreeSet<u64> {
    let items: Vec<u64> = set.iter().copied().collect();
    if items.len() <= n {
        return set.clone();
    }
    let mut out = BTreeSet::new();
    let mut r = seed | 1;
    while out.len() < n {
        r = r
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        out.insert(items[((r >> 33) % items.len() as u64) as usize]);
    }
    out
}

/// A blank chip whose partition table is the C6's with `lpfs` cut to
/// `sectors` sectors (at the same offset).
fn chip_with_lpfs_sectors(sectors: u32) -> Vec<u8> {
    let csv = std::fs::read_to_string(repo_root().join("lp-fw/fw-esp32c6/partitions.csv"))
        .expect("partitions.csv");
    let table = lpa_link::PartitionTable::from_csv(&csv).expect("the C6's table");
    let entries = table
        .entries()
        .iter()
        .cloned()
        .map(|mut e| {
            if e.label == "lpfs" {
                e.size = sectors * 4096;
            }
            e
        })
        .collect();
    let bytes = lpa_link::PartitionTable::new(entries).to_bytes();
    let mut chip = vec![0xFFu8; lp_emu_esp32c6::flash::DEFAULT_FLASH_LEN as usize];
    let at = lpa_link::PARTITION_TABLE_OFFSET as usize;
    chip[at..at + bytes.len()].copy_from_slice(&bytes);
    chip
}
