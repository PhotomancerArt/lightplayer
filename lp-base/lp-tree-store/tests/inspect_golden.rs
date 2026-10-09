//! The inspector against the pinned format: `format_golden.hex` is read back
//! into a flash image and `StoreImage` must find its root, its tree and a
//! clean `check` (every id hashes, every chunk inflates) — the second
//! reader the golden exists to keep honest.

use lp_tree_store::{
    EntryKindReport, MountVerdict, RecordKindReport, SoftSha256, StoreImage,
};

const GOLDEN: &str = include_str!("format_golden.hex");
const SECTOR: usize = 4096;

/// Each `sector N len` block, then `len` bytes in hex lines, the rest `0xFF`.
fn golden_image() -> Vec<u8> {
    let mut sectors: Vec<Vec<u8>> = Vec::new();
    for line in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        if let Some(rest) = line.strip_prefix("sector ") {
            let n: usize = rest.split(' ').next().unwrap().parse().unwrap();
            assert_eq!(n, sectors.len());
            sectors.push(Vec::new());
        } else {
            let bytes = (0..line.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&line[i..i + 2], 16).unwrap());
            sectors.last_mut().unwrap().extend(bytes);
        }
    }
    let mut image = Vec::new();
    for mut s in sectors {
        s.resize(SECTOR, 0xFF);
        image.extend(s);
    }
    image
}

#[test]
fn the_golden_image_reads_and_checks_clean() {
    let image = golden_image();
    let img = StoreImage::open(&image, None).unwrap();
    let r = img.report();
    assert_eq!(r.sector_size, 4096);
    assert_eq!(r.sector_count, 8);
    assert_eq!(r.mount, MountVerdict::Mounts);
    let files: Vec<&str> = r
        .tree
        .iter()
        .filter(|e| e.kind == EntryKindReport::File)
        .map(|e| e.path.as_str())
        .collect();
    assert!(files.contains(&"/projects/a/s.glsl"), "{files:?}");
    assert!(files.contains(&"/projects/a/m/module-11.json"));
    assert!(!files.contains(&"/hardware.json"), "it was deleted");
    // Every record kind and both codecs are in the image.
    let kinds: Vec<RecordKindReport> = r
        .sectors
        .iter()
        .flat_map(|s| s.records.iter().map(|r| r.kind))
        .collect();
    for k in [
        RecordKindReport::Blob,
        RecordKindReport::Multi,
        RecordKindReport::Dir,
        RecordKindReport::Root,
    ] {
        assert!(kinds.contains(&k), "{k:?}");
    }
    assert!(r.sectors.iter().flat_map(|s| &s.records).any(|r| r.codec == 1));
    let check = img.check(&mut SoftSha256, None);
    assert!(check.is_consistent(), "{:#?}", check.findings);
    assert_eq!(check.files_verified as usize, files.len());
    let ex = img.extract().unwrap();
    let shader = ex
        .files
        .iter()
        .find(|f| f.path == "/projects/a/s.glsl")
        .unwrap();
    assert_eq!(shader.bytes.len(), 672);
}
