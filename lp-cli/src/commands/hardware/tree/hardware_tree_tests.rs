//! `hardware tree` over images built in the test: an empty store, a
//! c40-shaped one, one after GC, with a torn root, with a retired sector,
//! with a sector at a newer version, with a directory holding bad names. The
//! rendered output is printed (`--nocapture` shows it).

use std::path::Path;

use clap::Parser;
use lp_crc32::crc32;
use lp_nor_sim::{NorFlashSim, NorGeometry, WearMode, WearOut};
use lp_tree_store::{ObjectHasher, SoftSha256, StoreConfig, StoreImage, TreeStore};

use super::super::args::{HardwareCli, HardwareSubcommand, TreeCommand, TreeSourceArgs};
use super::check::check_bytes;
use super::extract::extract_bytes;
use super::inspect::render;
use super::tree_source::load_image_file;

type Store = TreeStore<NorFlashSim, SoftSha256>;

const SECTOR: usize = 4096;

fn source() -> TreeSourceArgs {
    TreeSourceArgs {
        port: None,
        image: None,
        sector_size: None,
    }
}

fn store(sectors: u32) -> Store {
    let cfg = StoreConfig::default();
    let mut f = NorFlashSim::new(NorGeometry::c6(sectors));
    TreeStore::format(&mut f, &mut SoftSha256, &cfg).unwrap();
    mount(f)
}

fn mount(f: NorFlashSim) -> Store {
    match TreeStore::mount(f, SoftSha256, StoreConfig::default()) {
        Ok(s) => s,
        Err((e, _, _)) => panic!("mount: {e:?}"),
    }
}

fn image_of(st: &Store) -> Vec<u8> {
    let f = st.flash();
    let mut out = vec![0u8; f.geometry().capacity() as usize];
    f.peek(0, &mut out);
    out
}

/// Deterministic JSON-ish text.
fn text(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    let mut out = Vec::new();
    while out.len() < len {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        out.extend_from_slice(format!("\"k{}\": {}, ", x >> 50, x >> 40 & 0xFFF).as_bytes());
    }
    out.truncate(len);
    out
}

fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed ^ 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (x >> 24) as u8
        })
        .collect()
}

/// c40's shape: ~130 documents, ~216 KB, one project and board files.
fn c40_like() -> Vec<(String, Vec<u8>)> {
    let mut v = vec![
        ("/hardware.json".to_string(), text(1, 900)),
        ("/.lp/device.json".to_string(), text(3, 200)),
        ("/projects/a/project.json".to_string(), text(4, 2400)),
        ("/projects/a/.lp/panel.json".to_string(), text(400, 450)),
    ];
    for i in 0..40u64 {
        let m = format!("/projects/a/modules/m{i:02}");
        v.push((format!("{m}/node.json"), text(10 + i, 600)));
        v.push((format!("{m}/shader.glsl"), text(100 + i, 2900)));
    }
    for i in 0..44u64 {
        v.push((
            format!("/projects/a/nodes/n{i:02}.json"),
            text(200 + i, 450),
        ));
    }
    for i in 0..4u64 {
        v.push((
            format!("/projects/a/maps/map{i}.json"),
            text(300 + i, 18_000),
        ));
    }
    v
}

fn c40_store() -> Store {
    let mut st = store(128);
    st.begin().unwrap();
    for (p, b) in c40_like() {
        st.put(&p, &b).unwrap();
    }
    st.commit().unwrap();
    st
}

fn gc_store() -> Store {
    let mut st = c40_store();
    for round in 0..80u64 {
        st.put("/churn.bin", &noise(round, 6000)).unwrap();
        st.put(&format!("/keep/k{round:02}.json"), &text(round, 200))
            .unwrap();
    }
    assert!(st.stats().gc_copies > 3, "{:?}", st.stats());
    st
}

/// `HARDWARE_TREE_SAMPLE_DIR=<dir>` also writes each test image there, to
/// run the real binary on (`lp-cli hardware tree check --image <dir>/x.bin`).
fn run_all(name: &str, image: &[u8]) -> (String, String, bool) {
    if let Some(dir) = std::env::var_os("HARDWARE_TREE_SAMPLE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(Path::new(&dir).join(format!("{name}.bin")), image).unwrap();
    }
    let img = StoreImage::open(image, None).unwrap();
    let inspect = render(img.report(), name, false);
    let check = check_bytes(image, name, &source(), None, false).unwrap();
    println!("===== {name}: inspect =====\n{inspect}");
    println!("===== {name}: check =====\n{}", check.output);
    (inspect, check.output, check.consistent)
}

#[test]
fn hardware_tree_empty_store() {
    let image = image_of(&store(16));
    let (inspect, check, ok) = run_all("empty", &image);
    assert!(ok, "{check}");
    assert!(inspect.contains("mounts; root seq 1"), "{inspect}");
    assert!(inspect.contains("(empty)"), "{inspect}");
    assert!(inspect.contains("blank (through sector"), "{inspect}");
    assert!(check.contains("CONSISTENT (0 warning(s))"), "{check}");
}

#[test]
fn hardware_tree_c40_like_store() {
    let image = image_of(&c40_store());
    let (inspect, check, ok) = run_all("c40", &image);
    assert!(ok, "{check}");
    assert!(
        inspect.contains("/projects/a/modules/m39/shader.glsl  2900 B"),
        "{inspect}"
    );
    assert!(
        inspect.contains("/projects/a/.lp/panel.json  450 B  (hot)"),
        "{inspect}"
    );
    assert!(check.contains("verified: 132 files"), "{check}");
    // The full record list is there when asked for.
    let img = StoreImage::open(&image, None).unwrap();
    let records = render(img.report(), "c40", true);
    assert!(
        records.contains("codec 0 len") && records.contains("crc ok"),
        "{records}"
    );
}

#[test]
fn hardware_tree_after_gc_has_garbage_and_older_copies() {
    let image = image_of(&gc_store());
    let (inspect, check, ok) = run_all("after-gc", &image);
    assert!(ok, "{check}");
    assert!(check.contains("[orphans]"), "{check}");
    let img = StoreImage::open(&image, None).unwrap();
    assert!(img.report().garbage_bytes > 0);
    assert!(inspect.contains("garbage"), "{inspect}");
}

#[test]
fn hardware_tree_torn_root() {
    let mut st = store(16);
    st.put("/a.json", b"{\"v\": 1}").unwrap();
    st.put("/b.json", b"{\"v\": 2}").unwrap();
    let mut image = image_of(&st);
    let root = StoreImage::open(&image, None).unwrap().report().roots[0].clone();
    image[root.sector as usize * SECTOR + root.offset as usize + 12] ^= 0x5A;
    let (inspect, check, ok) = run_all("torn-root", &image);
    assert!(ok, "a torn tail is a handled crash: {check}");
    assert!(inspect.contains("closed at"), "{inspect}");
    assert!(
        inspect.contains("/a.json") && !inspect.contains("/b.json"),
        "{inspect}"
    );
    assert!(check.contains("warning"), "{check}");
}

#[test]
fn hardware_tree_retired_sector() {
    let cfg = StoreConfig::default();
    let mut f = NorFlashSim::new(NorGeometry::c6(16));
    TreeStore::format(&mut f, &mut SoftSha256, &cfg).unwrap();
    f.add_wear_out(WearOut {
        sector: 9,
        after_erases: 0,
        mode: WearMode::EraseFails,
        seed: 3,
    });
    let mut st = mount(f);
    for round in 0..80u64 {
        st.put("/churn.bin", &noise(round, 7000)).unwrap();
        st.put("/keep.json", &text(round, 900)).unwrap();
        if st.stats().retired_sectors > 0 && round > 20 {
            break;
        }
    }
    let image = image_of(&st);
    let (inspect, check, ok) = run_all("retired", &image);
    assert!(ok, "{check}");
    assert!(inspect.contains("RETIRED"), "{inspect}");
    assert!(check.contains("[retired-sector]"), "{check}");
}

#[test]
fn hardware_tree_newer_sector_refuses_but_extracts() {
    let mut st = store(16);
    st.put("/projects/a/project.json", b"{\"name\": \"a\"}")
        .unwrap();
    st.put("/hardware.json", &text(1, 500)).unwrap();
    let mut image = image_of(&st);
    let blank = StoreImage::open(&image, None)
        .unwrap()
        .report()
        .sectors
        .iter()
        .find(|s| s.state.label() == "blank")
        .unwrap()
        .index as usize;
    // A leaked bit in a blank sector's header: the magic, then version 4.
    image[blank * SECTOR..blank * SECTOR + 4].copy_from_slice(&0x3153_544Cu32.to_le_bytes());
    image[blank * SECTOR + 4..blank * SECTOR + 6].copy_from_slice(&4u16.to_le_bytes());
    image[blank * SECTOR + 6..blank * SECTOR + 24].fill(0xA5);
    let (inspect, check, ok) = run_all("newer-sector", &image);
    assert!(!ok);
    assert!(
        inspect.contains(&format!("REFUSED by sector {blank} (format version 4)")),
        "{inspect}"
    );
    assert!(
        inspect.contains("DO hold a complete version-3 store"),
        "{inspect}"
    );
    assert!(
        check.contains("[newer-sector]") && check.contains("[rest-complete]"),
        "{check}"
    );
    assert!(check.contains("INCONSISTENT"), "{check}");

    let out = tempfile::tempdir().unwrap();
    let dir = out.path().join("x");
    let summary = extract_bytes(&image, "newer-sector", &source(), &dir).unwrap();
    println!("===== newer-sector: extract =====\n{}", summary.output);
    for w in &summary.warnings {
        println!("warning: {w}");
    }
    assert_eq!(summary.skipped, 0);
    assert!(
        summary
            .warnings
            .iter()
            .any(|w| w.contains("refuses to mount"))
    );
    assert_eq!(
        std::fs::read(dir.join("projects/a/project.json")).unwrap(),
        b"{\"name\": \"a\"}"
    );
    assert_eq!(
        std::fs::read(dir.join("hardware.json")).unwrap(),
        text(1, 500)
    );
}

#[test]
fn hardware_tree_bad_directory_names() {
    let mut st = store(16);
    st.put("/ok.json", b"{\"fine\": true}").unwrap();
    st.put("/p/.lp/panel.json", b"{}").unwrap();
    let mut image = image_of(&st);
    let (file, hot) = {
        let img = StoreImage::open(&image, None).unwrap();
        let find = |p: &str| {
            let e = img.report().tree.iter().find(|e| e.path == p).unwrap();
            (e.id, e.size)
        };
        (find("/ok.json"), find("/p/.lp/panel.json"))
    };
    forge_root(
        &mut image,
        &[
            (b"".to_vec(), file.1, file.0),
            (b"a/b".to_vec(), file.1, file.0),
            (vec![0xFF, b'x'], file.1, file.0),
            (b"ok.json".to_vec(), file.1, file.0),
        ],
        &[
            (b"/p/.lp/panel.json".to_vec(), hot.1, hot.0),
            (b"/not-hot.json".to_vec(), hot.1, hot.0),
        ],
    );
    let (inspect, check, ok) = run_all("bad-names", &image);
    assert!(!ok);
    assert_eq!(check.matches("[dir-name]").count(), 3, "{check}");
    assert_eq!(check.matches("[hot-entry]").count(), 1, "{check}");
    assert!(inspect.contains("<- name is not UTF-8"), "{inspect}");

    let out = tempfile::tempdir().unwrap();
    let dir = out.path().join("x");
    let summary = extract_bytes(&image, "bad-names", &source(), &dir).unwrap();
    println!("===== bad-names: extract =====\n{}", summary.output);
    assert_eq!(summary.skipped, 3);
    assert!(dir.join("ok.json").exists() && dir.join("p/.lp/panel.json").exists());
}

#[test]
fn hardware_tree_extract_c40_round_trips_every_byte() {
    let image = image_of(&gc_store());
    let out = tempfile::tempdir().unwrap();
    let dir = out.path().join("backup");
    let summary = extract_bytes(&image, "gc", &source(), &dir).unwrap();
    assert_eq!(summary.skipped, 0);
    for (p, b) in c40_like() {
        assert_eq!(
            std::fs::read(dir.join(p.trim_start_matches('/'))).unwrap(),
            b,
            "{p}"
        );
    }
    // A second extraction into the now-full directory refuses.
    assert!(extract_bytes(&image, "gc", &source(), &dir).is_err());
}

#[test]
fn hardware_tree_image_file_and_chip_image_load() {
    let image = image_of(&store(16));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("raw-lpfs-0x0.bin");
    std::fs::write(&path, &image).unwrap();
    let loaded = load_image_file(&path).unwrap();
    assert_eq!(loaded.bytes, image);
    assert!(StoreImage::open(&loaded.bytes, None).is_ok());
    // A truncated file is "too small", not a panic.
    assert!(super::tree_source::open_image(&[0; 10], &source()).is_err());
    let bad = TreeSourceArgs {
        sector_size: Some(1000),
        ..source()
    };
    assert!(super::tree_source::open_image(&image, &bad).is_err());
}

#[test]
fn hardware_tree_check_json_and_reread() {
    let image = image_of(&store(8));
    let out = check_bytes(&image, "x", &source(), None, true).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out.output).unwrap();
    assert_eq!(v["consistent"], true);
    assert_eq!(v["mount"], "mounts");
    assert_eq!(v["chosen_root_seq"], 1);
    let mut second = image.clone();
    second[2 * SECTOR + 50] ^= 1;
    let out = check_bytes(&image, "x", &source(), Some(&second), false).unwrap();
    assert!(!out.consistent);
    assert!(out.output.contains("[weak-sector]"), "{}", out.output);
}

#[test]
fn hardware_tree_inspect_json_carries_every_record() {
    let image = image_of(&store(8));
    let img = StoreImage::open(&image, None).unwrap();
    let v = serde_json::to_value(img.report()).unwrap();
    assert_eq!(v["sector_size"], 4096);
    assert_eq!(v["mount"], "mounts");
    let rec = &v["sectors"][0]["records"];
    assert!(rec.is_array());
    let first = v["sectors"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["records"].as_array().unwrap().iter())
        .next()
        .unwrap();
    assert_eq!(first["crc_ok"], true);
    assert_eq!(first["id"].as_str().unwrap().len(), 16);
}

#[test]
fn hardware_tree_commands_parse() {
    let cli = HardwareCli::try_parse_from([
        "hardware",
        "tree",
        "inspect",
        "--image",
        "a.bin",
        "--records",
        "--json",
    ])
    .unwrap();
    let Some(HardwareSubcommand::Tree(t)) = cli.subcommand else {
        panic!("not tree")
    };
    assert!(matches!(t.command, TreeCommand::Inspect(_)));
    let cli = HardwareCli::try_parse_from([
        "hardware", "tree", "check", "--image", "a.bin", "--reread", "b.bin",
    ])
    .unwrap();
    assert!(matches!(
        cli.subcommand,
        Some(HardwareSubcommand::Tree(t)) if matches!(t.command, TreeCommand::Check(_))
    ));
    HardwareCli::try_parse_from([
        "hardware", "tree", "extract", "--port", "/dev/x", "--out", "dir",
    ])
    .unwrap();
    // One source only; the board is read twice by `check --port` itself.
    assert!(
        HardwareCli::try_parse_from([
            "hardware", "tree", "inspect", "--port", "/dev/x", "--image", "a.bin"
        ])
        .is_err()
    );
    assert!(
        HardwareCli::try_parse_from([
            "hardware", "tree", "check", "--port", "/dev/x", "--reread", "b.bin"
        ])
        .is_err()
    );
}

// ---- forging what the writer never writes ----------------------------------------

/// One record's bytes: kind, codec 0, len, id, CRC, payload.
fn record(kind: u8, id: u64, payload: &[u8]) -> Vec<u8> {
    let mut h = vec![kind, 0];
    h.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    h.extend_from_slice(&id.to_le_bytes());
    let mut all = h.clone();
    all.extend_from_slice(payload);
    // The CRC covers header bytes 0..12 then the payload.
    let mut covered = h[..12].to_vec();
    covered.extend_from_slice(payload);
    h.extend_from_slice(&crc32(&covered).to_le_bytes());
    h.extend_from_slice(payload);
    let _ = all;
    h
}

/// `H(tag ++ bytes)`'s first 8 bytes, big-endian (0 becomes 1).
fn id_of(tag: u8, bytes: &[u8]) -> u64 {
    let d = SoftSha256.sha256(&[&[tag], bytes]);
    match u64::from_be_bytes(d[..8].try_into().unwrap()) {
        0 => 1,
        v => v,
    }
}

/// A directory node's bytes from `(name, size, id)` file entries.
fn dir_bytes(entries: &[(Vec<u8>, u32, u64)]) -> Vec<u8> {
    let mut sorted = entries.to_vec();
    sorted.sort();
    let mut out = (sorted.len() as u16).to_le_bytes().to_vec();
    for (name, size, id) in sorted {
        out.push(1);
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&name);
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&id.to_le_bytes());
    }
    out
}

/// Append a cold directory, a hot directory and a root naming them (seq one
/// past the newest) where the newest root's sector is still erased.
fn forge_root(image: &mut [u8], cold: &[(Vec<u8>, u32, u64)], hot: &[(Vec<u8>, u32, u64)]) {
    let (sector, end, seq) = {
        let img = StoreImage::open(image, None).unwrap();
        let r = &img.report().roots[0];
        (
            r.sector as usize,
            img.report().sectors[r.sector as usize].records_end as usize,
            r.seq,
        )
    };
    let cold_bytes = dir_bytes(cold);
    let hot_bytes = dir_bytes(hot);
    let (cold_id, hot_id) = (id_of(2, &cold_bytes), id_of(2, &hot_bytes));
    let mut root = seq.wrapping_add(1).to_le_bytes().to_vec();
    root.extend_from_slice(&cold_id.to_le_bytes());
    root.extend_from_slice(&hot_id.to_le_bytes());
    root.extend_from_slice(&0u16.to_le_bytes());
    let mut forged = record(3, cold_id, &cold_bytes);
    forged.extend(record(3, hot_id, &hot_bytes));
    forged.extend(record(4, id_of(4, &root), &root));
    let at = sector * SECTOR + end;
    assert!(end + forged.len() <= SECTOR, "the forged records fit");
    image[at..at + forged.len()].copy_from_slice(&forged);
}
