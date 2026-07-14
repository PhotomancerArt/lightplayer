//! Contract tests for the file-sync operations (roadmap M2b).
//!
//! These pin the server-side semantics the studio's places layer (M3) and
//! the device push path (M5) build on: revision gating, pagination,
//! tombstones, chunk reassembly, stateless chunked writes, and hash
//! verification.

use lpa_server::file_sync::{handle_changes_since, handle_hash_package, handle_write_chunk};
use lpc_model::{AsLpPath, AsLpPathBuf, FsVersion};
use lpc_wire::budget::{
    FILE_SYNC_CHUNK_BYTES, FILE_SYNC_PAGE_MAX_ENTRIES, FILE_SYNC_PAGE_RAW_BYTES,
};
use lpc_wire::server::{FileChangeKind, FileChunk, FileCursor, FsResponse};
use lpfs::{LpFs, LpFsMemory};
use std::collections::BTreeMap;

const PREFIX: &str = "/projects/x";

fn write(fs: &LpFsMemory, rel: &str, data: &[u8]) {
    fs.write_file(format!("{PREFIX}{rel}").as_str().as_path(), data)
        .unwrap();
}

/// Pull every page, asserting per-page invariants; returns (files, version).
fn pull_all(fs: &LpFsMemory, since: FsVersion) -> (BTreeMap<String, Option<Vec<u8>>>, FsVersion) {
    let mut files: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
    let mut cursor: Option<FileCursor> = None;
    let mut first_version: Option<FsVersion> = None;
    loop {
        let response = handle_changes_since(fs, PREFIX.as_path(), since, cursor.take());
        let FsResponse::Changes {
            entries,
            next,
            version,
            error,
        } = response
        else {
            panic!("wrong response variant");
        };
        assert_eq!(error, None);
        assert!(
            entries.len() <= FILE_SYNC_PAGE_MAX_ENTRIES,
            "page entry cap violated"
        );
        let raw: usize = entries.iter().map(|e| e.data.len()).sum();
        // one final chunk may straddle the raw cap boundary by < chunk size
        assert!(
            raw <= FILE_SYNC_PAGE_RAW_BYTES + FILE_SYNC_CHUNK_BYTES,
            "page raw cap violated"
        );
        first_version.get_or_insert(version.expect("version on every page"));
        for FileChunk {
            path,
            kind,
            offset,
            total,
            data,
        } in entries
        {
            match kind {
                FileChangeKind::Delete => {
                    files.insert(path.as_str().to_string(), None);
                }
                FileChangeKind::Upsert => {
                    let entry = files
                        .entry(path.as_str().to_string())
                        .or_insert_with(|| Some(Vec::new()));
                    let buffer = entry.get_or_insert_with(Vec::new);
                    assert_eq!(buffer.len(), offset as usize, "chunks must arrive in order");
                    buffer.extend_from_slice(&data);
                    assert!(buffer.len() <= total as usize);
                }
            }
        }
        if next.is_none() {
            break;
        }
        cursor = next;
    }
    (files, first_version.unwrap())
}

#[test]
fn since_current_transfers_nothing() {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", b"{}");
    let current = fs.current_version();

    let response = handle_changes_since(&fs, PREFIX.as_path(), current, None);
    let FsResponse::Changes {
        entries,
        next,
        version,
        error,
    } = response
    else {
        panic!("wrong variant");
    };
    assert!(entries.is_empty());
    assert_eq!(next, None);
    assert_eq!(version, Some(current));
    assert_eq!(error, None);
}

#[test]
fn full_pull_then_incremental_then_empty() {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", b"{\"kind\":\"Project\"}");
    write(&fs, "/shader.glsl", b"void main() {}");

    // full pull (since = 0) enumerates everything
    let (files, version) = pull_all(&fs, FsVersion::new(0));
    assert_eq!(files.len(), 2);
    assert_eq!(
        files["/shader.glsl"].as_deref(),
        Some(b"void main() {}".as_ref())
    );

    // reads don't consume: an identical pull yields the same result
    let (files2, _) = pull_all(&fs, FsVersion::new(0));
    assert_eq!(files, files2);

    // incremental: an edit surfaces exactly the delta
    write(&fs, "/shader.glsl", b"void main() { /*2*/ }");
    let (delta, version2) = pull_all(&fs, version);
    assert_eq!(delta.len(), 1);
    assert_eq!(
        delta["/shader.glsl"].as_deref(),
        Some(b"void main() { /*2*/ }".as_ref())
    );

    // adopting the returned version → nothing further
    let (empty, _) = pull_all(&fs, version2);
    assert!(empty.is_empty());
}

#[test]
fn delete_arrives_as_tombstone() {
    let fs = LpFsMemory::new();
    write(&fs, "/keep.json", b"{}");
    write(&fs, "/gone.json", b"{}");
    let version = fs.current_version();

    fs.delete_file(format!("{PREFIX}/gone.json").as_str().as_path())
        .unwrap();
    let (delta, _) = pull_all(&fs, version);
    assert_eq!(delta.len(), 1);
    assert_eq!(delta["/gone.json"], None);
}

#[test]
fn changes_outside_prefix_are_invisible() {
    let fs = LpFsMemory::new();
    write(&fs, "/a.json", b"{}");
    let version = fs.current_version();
    fs.write_file("/projects/other/b.json".as_path(), b"{}")
        .unwrap();
    fs.write_file("/projects/xy/sneaky.json".as_path(), b"{}")
        .unwrap();

    let (delta, _) = pull_all(&fs, version);
    assert!(delta.is_empty());
}

#[test]
fn large_file_paginates_and_reassembles_byte_identically() {
    let fs = LpFsMemory::new();
    // > 2 pages worth: 3 × page raw cap, patterned so reassembly errors show
    let big: Vec<u8> = (0..(3 * FILE_SYNC_PAGE_RAW_BYTES))
        .map(|i| (i % 251) as u8)
        .collect();
    write(&fs, "/big.bin", &big);
    write(&fs, "/small.json", b"{}");

    let (files, _) = pull_all(&fs, FsVersion::new(0));
    assert_eq!(files["/big.bin"].as_deref(), Some(big.as_slice()));
    assert_eq!(files["/small.json"].as_deref(), Some(b"{}".as_ref()));
}

#[test]
fn many_small_files_respect_entry_cap() {
    let fs = LpFsMemory::new();
    for i in 0..40 {
        write(&fs, &format!("/n{i:02}.json"), b"{\"v\":1}");
    }
    let (files, _) = pull_all(&fs, FsVersion::new(0));
    assert_eq!(files.len(), 40);
}

#[test]
fn write_chunk_reassembles_and_validates_offsets() {
    let mut fs = LpFsMemory::new();
    let path = || format!("{PREFIX}/big.svg").as_str().as_path_buf();
    let part1 = vec![1u8; FILE_SYNC_CHUNK_BYTES];
    let part2 = vec![2u8; 100];

    let response = handle_write_chunk(&mut fs, path(), 0, &part1);
    let FsResponse::WriteChunk { written, error, .. } = response else {
        panic!()
    };
    assert_eq!(error, None);
    assert_eq!(written as usize, part1.len());

    // wrong offset → error, file untouched
    let response = handle_write_chunk(&mut fs, path(), 17, &part2);
    let FsResponse::WriteChunk { error, .. } = response else {
        panic!()
    };
    assert!(error.unwrap().contains("offset mismatch"));

    let response = handle_write_chunk(&mut fs, path(), part1.len() as u32, &part2);
    let FsResponse::WriteChunk { error, .. } = response else {
        panic!()
    };
    assert_eq!(error, None);

    let stored = fs
        .read_file(format!("{PREFIX}/big.svg").as_str().as_path())
        .unwrap();
    assert_eq!(stored.len(), part1.len() + part2.len());
    assert_eq!(&stored[..part1.len()], part1.as_slice());
    assert_eq!(&stored[part1.len()..], part2.as_slice());

    // offset 0 truncates: a fresh upload replaces, never appends
    let response = handle_write_chunk(&mut fs, path(), 0, b"fresh");
    let FsResponse::WriteChunk { error, .. } = response else {
        panic!()
    };
    assert_eq!(error, None);
    let stored = fs
        .read_file(format!("{PREFIX}/big.svg").as_str().as_path())
        .unwrap();
    assert_eq!(stored, b"fresh");
}

#[test]
fn hash_package_matches_direct_computation() {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", b"{\"kind\":\"Project\"}");
    write(&fs, "/shader.glsl", b"void main() {}");
    write(&fs, "/.lp/meta.json", b"{\"origin\":\"x\"}"); // excluded from hash

    let response = handle_hash_package(&fs, PREFIX.as_path_buf());
    let FsResponse::PackageHash { hash, error, .. } = response else {
        panic!()
    };
    assert_eq!(error, None);

    let view = fs.chroot(PREFIX.as_path()).unwrap();
    let (expected, _) = lpc_history::hash_package(&*view.borrow()).unwrap();
    assert_eq!(hash, expected.to_string());
}
