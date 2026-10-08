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

use crate::test_support::formatted;
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
    let mut v: Vec<String> = v.unwrap().iter().map(|p| String::from(p.as_str())).collect();
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
        fs.write_file("/projects/a/project.json".as_path(), b"{}").unwrap();
        fs.write_file("/projects/a/src/x.glsl".as_path(), b"x").unwrap();
        fs.write_file("/projects/a/src/y.glsl".as_path(), b"yy").unwrap();
        fs.write_file("/top.json".as_path(), b"t").unwrap();
        fs.append_file("/projects/a/src/y.glsl".as_path(), b"+").unwrap();
        fs.append_file("/new.log".as_path(), b"a").unwrap();
        log.push(format!("{:?}", fs.read_file("/projects/a/src/y.glsl".as_path()).unwrap()));
        log.push(format!("{:?}", fs.file_size("/projects/a/src/y.glsl".as_path()).unwrap()));
        log.push(format!("{:?}", fs.file_exists("/nope".as_path()).unwrap()));
        log.push(format!("{:?}", fs.is_dir("/projects".as_path()).unwrap()));
        log.push(format!("{:?}", fs.is_dir("/top.json".as_path()).unwrap()));
        log.push(format!("{:?}", fs.is_dir("/nope".as_path()).is_err()));
        log.push(format!("{:?}", fs.read_file("/nope".as_path()).is_err()));
        log.push(format!("{:?}", sorted(fs.list_dir("/".as_path(), false))));
        log.push(format!("{:?}", sorted(fs.list_dir("/projects/a".as_path(), false))));
        log.push(format!("{:?}", sorted(fs.list_dir("/projects/a/src".as_path(), true))));
        log.push(format!("{:?}", fs.delete_file("/projects/a/src".as_path()).is_err()));
        log.push(format!("{:?}", fs.delete_file("/nope".as_path()).is_err()));
        fs.delete_file("/top.json".as_path()).unwrap();
        log.push(format!("{:?}", fs.delete_dir("/nope".as_path()).is_err()));
        log.push(format!("{:?}", fs.delete_dir("/".as_path()).is_err()));
        fs.delete_dir("/projects/a/src".as_path()).unwrap();
        log.push(format!("{:?}", sorted(fs.list_dir("/projects/a".as_path(), true))));
        let view = fs.chroot("/projects/a".as_path()).unwrap();
        view.borrow().write_file("/src/z.glsl".as_path(), b"z").unwrap();
        log.push(format!("{:?}", fs.read_file("/projects/a/src/z.glsl".as_path()).unwrap()));
        log.push(format!("{:?}", sorted(view.borrow().list_dir("/".as_path(), false))));
        log.push(format!("{:?}", view.borrow().read_file("/project.json".as_path()).unwrap()));
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
    view.borrow().write_file("/new.json".as_path(), b"new").unwrap();
    assert_eq!(fs.read_file("/p/new.json".as_path()).unwrap(), b"new");
    view.borrow().abort_batch().unwrap();
    assert!(!fs.file_exists("/p/new.json".as_path()).unwrap());
    fs.begin_batch().unwrap();
    fs.delete_dir("/p".as_path()).unwrap();
    fs.write_file("/p/new.json".as_path(), b"new").unwrap();
    fs.commit_batch().unwrap();
    assert_eq!(sorted(fs.list_dir("/p".as_path(), false)), vec!["/p/new.json"]);
    let events = fs.get_changes_since(FsVersion::default());
    assert!(events.iter().any(|e| e.kind == FsEventKind::Delete));
}
