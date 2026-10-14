//! The `LpFs` adapter against `LpFsMemory`: one script of calls through
//! both, the same answers (listings compared as sets; `LpFsMemory`'s
//! recursive listing names only the first level of directories, so the
//! script keeps recursive listings to one level, and a separate test checks
//! the trait's "every directory, recursively").

extern crate std;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::{format, vec};

use lp_nor_sim::{NorFlashSim, NorGeometry};
use lpfs::{AsLpPath, FsEventKind, FsVersion, LpFs, LpFsMemory, LpPathBuf};

use crate::test_support::{formatted, text};
use crate::{LpFsTree, SoftSha256, StoreConfig, TreeStore};

fn tree_fs() -> LpFsTree<NorFlashSim, SoftSha256> {
    let c = StoreConfig::default();
    let f = formatted(NorGeometry::c6(32), &c);
    let Ok(st) = TreeStore::mount(f, SoftSha256, c) else {
        panic!("mount")
    };
    LpFsTree::new(st)
}

fn sorted(v: Result<Vec<LpPathBuf>, lpfs::FsError>) -> Vec<String> {
    let mut v: Vec<String> = v
        .unwrap()
        .iter()
        .map(|p| String::from(p.as_str()))
        .collect();
    v.sort();
    v
}

/// Run `script` on both and compare every observable.
fn both(script: impl Fn(&dyn LpFs) -> Vec<String>) {
    let mem = LpFsMemory::new();
    let tree = tree_fs();
    assert_eq!(script(&tree), script(&mem));
}

#[test]
fn conformance_with_lp_fs_memory() {
    both(|fs| {
        let mut log = Vec::new();
        let p = |s: &str| String::from(s);
        fs.write_file("/projects/a/project.json".as_path(), b"{}")
            .unwrap();
        fs.write_file("/projects/a/src/x.glsl".as_path(), b"x")
            .unwrap();
        fs.write_file("/projects/a/src/y.glsl".as_path(), b"yy")
            .unwrap();
        fs.write_file("/top.json".as_path(), b"t").unwrap();
        fs.append_file("/projects/a/src/y.glsl".as_path(), b"+")
            .unwrap();
        fs.append_file("/new.log".as_path(), b"a").unwrap();
        log.push(format!(
            "{:?}",
            fs.read_file("/projects/a/src/y.glsl".as_path()).unwrap()
        ));
        log.push(format!(
            "{:?}",
            fs.file_size("/projects/a/src/y.glsl".as_path()).unwrap()
        ));
        log.push(format!("{:?}", fs.file_exists("/nope".as_path()).unwrap()));
        log.push(format!("{:?}", fs.is_dir("/projects".as_path()).unwrap()));
        log.push(format!("{:?}", fs.is_dir("/top.json".as_path()).unwrap()));
        log.push(format!("{:?}", fs.is_dir("/nope".as_path()).is_err()));
        log.push(format!("{:?}", fs.read_file("/nope".as_path()).is_err()));
        log.push(format!("{:?}", sorted(fs.list_dir("/".as_path(), false))));
        log.push(format!(
            "{:?}",
            sorted(fs.list_dir("/projects/a".as_path(), false))
        ));
        log.push(format!(
            "{:?}",
            sorted(fs.list_dir("/projects/a/src".as_path(), true))
        ));
        log.push(format!(
            "{:?}",
            fs.delete_file("/projects/a/src".as_path()).is_err()
        ));
        log.push(format!("{:?}", fs.delete_file("/nope".as_path()).is_err()));
        fs.delete_file("/top.json".as_path()).unwrap();
        log.push(format!("{:?}", fs.delete_dir("/nope".as_path()).is_err()));
        log.push(format!("{:?}", fs.delete_dir("/".as_path()).is_err()));
        fs.delete_dir("/projects/a/src".as_path()).unwrap();
        log.push(format!(
            "{:?}",
            sorted(fs.list_dir("/projects/a".as_path(), true))
        ));
        let view = fs.chroot("/projects/a".as_path()).unwrap();
        view.borrow()
            .write_file("/src/z.glsl".as_path(), b"z")
            .unwrap();
        log.push(format!(
            "{:?}",
            fs.read_file("/projects/a/src/z.glsl".as_path()).unwrap()
        ));
        log.push(format!(
            "{:?}",
            sorted(view.borrow().list_dir("/".as_path(), false))
        ));
        log.push(format!(
            "{:?}",
            view.borrow().read_file("/project.json".as_path()).unwrap()
        ));
        let mut ev: Vec<String> = fs
            .get_changes_since(FsVersion::default())
            .iter()
            .map(|e| format!("{} {:?}", e.path.as_str(), e.kind))
            .collect();
        ev.sort();
        log.extend(ev);
        log.push(p("end"));
        log
    });
}

#[test]
fn recursive_listing_names_every_directory() {
    let fs = tree_fs();
    fs.write_file("/a/b/c/d.json".as_path(), b"d").unwrap();
    fs.write_file("/a/e.json".as_path(), b"e").unwrap();
    assert_eq!(
        sorted(fs.list_dir("/a".as_path(), true)),
        vec!["/a/b", "/a/b/c", "/a/b/c/d.json", "/a/e.json"]
    );
}

#[test]
fn a_batch_commits_together_and_aborts_cleanly() {
    let fs = tree_fs();
    fs.write_file("/p/old.json".as_path(), b"old").unwrap();
    let view = fs.chroot("/p".as_path()).unwrap();
    view.borrow().begin_batch().unwrap();
    view.borrow().delete_dir("/".as_path()).unwrap_err();
    view.borrow()
        .write_file("/new.json".as_path(), b"new")
        .unwrap();
    assert_eq!(fs.read_file("/p/new.json".as_path()).unwrap(), b"new");
    view.borrow().abort_batch().unwrap();
    assert!(!fs.file_exists("/p/new.json".as_path()).unwrap());
    fs.begin_batch().unwrap();
    fs.delete_dir("/p".as_path()).unwrap();
    fs.write_file("/p/new.json".as_path(), b"new").unwrap();
    fs.commit_batch().unwrap();
    assert_eq!(
        sorted(fs.list_dir("/p".as_path(), false)),
        vec!["/p/new.json"]
    );
    let events = fs.get_changes_since(FsVersion::default());
    assert!(events.iter().any(|e| e.kind == FsEventKind::Delete));
}

#[test]
fn an_aborted_batch_leaves_the_change_log_as_it_was() {
    let fs = tree_fs();
    fs.write_file("/p/a.json".as_path(), b"a").unwrap();
    let events = |fs: &LpFsTree<NorFlashSim, SoftSha256>| {
        fs.get_changes_since(FsVersion::default())
            .into_iter()
            .map(|e| (e.path, e.kind))
            .collect::<Vec<_>>()
    };
    let before = events(&fs);
    fs.begin_batch().unwrap();
    fs.delete_file("/p/a.json".as_path()).unwrap();
    fs.write_file("/p/b.json".as_path(), b"b").unwrap();
    fs.abort_batch().unwrap();
    assert_eq!(
        events(&fs),
        before,
        "a pull after the abort hears nothing of the batch"
    );
    assert_eq!(fs.read_file("/p/a.json".as_path()).unwrap(), b"a");
    assert!(!fs.file_exists("/p/b.json".as_path()).unwrap());
    // A committed batch keeps its events.
    fs.begin_batch().unwrap();
    fs.write_file("/p/c.json".as_path(), b"c").unwrap();
    fs.commit_batch().unwrap();
    assert!(
        fs.get_changes_since(FsVersion::default())
            .iter()
            .any(|e| e.path.as_str() == "/p/c.json")
    );
}

#[test]
fn the_tree_store_says_its_batches_are_atomic_through_a_view() {
    let fs = tree_fs();
    assert!(fs.batches_are_atomic());
    assert!(
        fs.chroot("/p".as_path())
            .unwrap()
            .borrow()
            .batches_are_atomic()
    );
    assert!(!LpFsMemory::new().batches_are_atomic());
}

/// A deflated push in one batch — several files, one of them many chunks —
/// lands and reads back as the logical bytes, and the same bytes pushed
/// again (the next push of an unchanged file, a second slot) dedupe against
/// the chunks already stored. A *plain* write of the same bytes does not:
/// it cuts its chunks at record size, not at the deflate plan's boundaries,
/// so its ids differ.
#[test]
fn a_deflated_batch_round_trips_and_dedupes_against_itself() {
    let fs = tree_fs();
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("/projects/a/project.json", text(1, 300)),
        ("/projects/a/main.glsl", text(2, 3_000)),
        ("/projects/a/big.svg", text(3, 30_000)),
    ];
    fs.begin_batch().unwrap();
    for (path, bytes) in &files {
        write_deflated(&fs, path, bytes, 1024);
    }
    fs.commit_batch().unwrap();
    for (path, bytes) in &files {
        assert_eq!(&fs.read_file(path.as_path()).unwrap(), bytes, "{path}");
        assert_eq!(
            fs.file_size(path.as_path()).unwrap(),
            bytes.len() as u64,
            "{path}"
        );
    }
    let hits = fs.with_store(|st| st.stats().dedup_hits);
    let big = &files[2].1;
    let chunks = crate::plan_deflated_chunks(big, 1024, crate::DEFAULT_DEFLATE_LEVEL).len();
    write_deflated(&fs, "/projects/b/big.svg", big, 1024);
    let hits_after = fs.with_store(|st| st.stats().dedup_hits);
    assert!(
        hits_after >= hits + chunks as u64,
        "{hits} -> {hits_after} over {chunks} chunks"
    );
}

/// A stream that does not inflate to its length, a length past the cap and
/// an offset that is not the file's end write nothing and record nothing.
#[test]
fn a_bad_deflated_chunk_writes_nothing() {
    let fs = tree_fs();
    fs.write_file("/p/s.glsl".as_path(), b"0123").unwrap();
    let version = fs.current_version();
    let good = crate::plan_deflated_chunks(b"more text", 1024, 10).remove(0);
    let path = "/p/s.glsl".as_path();
    // Corrupt: the stream cut short.
    let cut = &good.deflated[..good.deflated.len() / 2];
    assert!(fs.write_deflated_chunk(path, 4, 9, cut).is_err());
    // A lying length.
    assert!(fs.write_deflated_chunk(path, 4, 8, &good.deflated).is_err());
    // Past the cap: refused before anything is allocated.
    assert!(
        fs.write_deflated_chunk(path, 4, 1 << 30, &good.deflated)
            .is_err()
    );
    // Not the file's end.
    assert!(fs.write_deflated_chunk(path, 3, 9, &good.deflated).is_err());
    assert_eq!(fs.read_file(path).unwrap(), b"0123");
    assert_eq!(fs.current_version(), version, "nothing recorded");
    // And the good one appends.
    fs.write_deflated_chunk(path, 4, 9, &good.deflated).unwrap();
    assert_eq!(fs.read_file(path).unwrap(), b"0123more text");
}

/// The wire plans chunks for the 1,024-byte hint; a board whose records are
/// smaller still stores every chunk — as plain bytes where the deflate does
/// not fit — and reads the file back whole.
#[test]
fn a_record_size_other_than_the_hint_still_stores_the_file() {
    let c = StoreConfig {
        record_max: 512,
        ..StoreConfig::default()
    };
    let f = formatted(NorGeometry::c6(32), &c);
    let Ok(st) = TreeStore::mount(f, SoftSha256, c) else {
        panic!("mount")
    };
    let fs = LpFsTree::new(st);
    let bytes = text(9, 20_000);
    fs.begin_batch().unwrap();
    write_deflated(&fs, "/p/big.svg", &bytes, 1024);
    fs.commit_batch().unwrap();
    assert_eq!(fs.read_file("/p/big.svg".as_path()).unwrap(), bytes);
}

/// The inflate-then-write default every other backend takes: the plain
/// bytes land at logical offsets, and a corrupt stream writes nothing.
#[test]
fn the_default_deflated_write_inflates_into_plain_bytes() {
    let fs = LpFsMemory::new();
    let bytes = text(4, 9_000);
    write_deflated(&fs, "/p/s.glsl", &bytes, 1024);
    assert_eq!(fs.read_file("/p/s.glsl".as_path()).unwrap(), bytes);
    assert!(
        fs.write_deflated_chunk("/p/t.glsl".as_path(), 0, 4, &[0xff, 0xff])
            .is_err()
    );
    assert!(!fs.file_exists("/p/t.glsl".as_path()).unwrap());
}

/// `bytes` to `path` as the wire sends them: the plan's chunks, in order.
fn write_deflated(fs: &dyn LpFs, path: &str, bytes: &[u8], record_max: u32) {
    for chunk in crate::plan_deflated_chunks(bytes, record_max, crate::DEFAULT_DEFLATE_LEVEL) {
        fs.write_deflated_chunk(
            path.as_path(),
            chunk.logical_range.start as u32,
            chunk.logical_range.len() as u32,
            &chunk.deflated,
        )
        .unwrap();
    }
}
