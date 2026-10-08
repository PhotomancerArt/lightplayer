//! Transactions, streaming appends, host-deflated chunks and path-hash
//! collisions.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lp_nor_sim::NorGeometry;

use crate::object_hasher::ObjectHasher;
use crate::object_id::{IdTag, ObjectId};
use crate::test_support::{deflate, formatted, mount, noise, snapshot, text};
use crate::{SoftSha256, StoreConfig, StoreError, TreeStore};

fn cfg() -> StoreConfig {
    StoreConfig::default()
}

#[test]
fn a_transaction_commits_once_and_reads_its_own_writes() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(32), &c), &c);
    let commits = st.stats().commits;
    st.begin().unwrap();
    assert_eq!(st.begin(), Err(StoreError::InTransaction));
    for i in 0..10u64 {
        st.put(&alloc::format!("/p/m{i}/shader.glsl"), &text(i, 700))
            .unwrap();
    }
    assert_eq!(st.get("/p/m3/shader.glsl").unwrap().unwrap(), text(3, 700));
    assert_eq!(st.list("/p/").unwrap().len(), 10);
    assert_eq!(st.stats().commits, commits, "no root inside the transaction");
    // A remount now (as after a cut) sees none of it.
    let mut other = mount(st.flash().clone(), &c);
    assert!(other.list("/").unwrap().is_empty());
    st.commit().unwrap();
    assert_eq!(st.stats().commits, commits + 1);
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.list("/p/").unwrap().len(), 10);
}

#[test]
fn abort_puts_everything_back() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(32), &c), &c);
    st.put("/a/keep.json", b"keep").unwrap();
    st.put("/a/edit.json", b"old").unwrap();
    let before = snapshot(&mut st);
    st.begin().unwrap();
    st.put("/a/edit.json", b"new").unwrap();
    st.put("/a/added.json", b"added").unwrap();
    st.delete_prefix("/a/").unwrap();
    st.put("/a/again.json", b"again").unwrap();
    assert_eq!(st.list("/").unwrap(), vec![String::from("/a/again.json")]);
    st.abort().unwrap();
    assert_eq!(snapshot(&mut st), before);
    assert_eq!(st.file_size("/a/edit.json").unwrap(), Some(3));
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(snapshot(&mut st), before);
}

#[test]
fn a_big_transaction_flushes_its_delta_and_stays_bounded() {
    let c = StoreConfig {
        txn_delta_max: 512,
        ..cfg()
    };
    let mut st = mount(formatted(NorGeometry::c6(64), &c), &c);
    st.begin().unwrap();
    for i in 0..120u64 {
        st.put(&alloc::format!("/projects/a/modules/m{i:03}/node.json"), &text(i, 200))
            .unwrap();
        assert!(st.delta.ram_bytes() < 1024, "{}", st.delta.ram_bytes());
    }
    st.commit().unwrap();
    let mut st = mount(st.into_flash(), &c);
    let all = st.list("/projects/a/").unwrap();
    assert_eq!(all.len(), 120);
    assert_eq!(
        st.get("/projects/a/modules/m077/node.json").unwrap().unwrap(),
        text(77, 200)
    );
}

#[test]
fn appends_write_only_the_new_chunks_and_match_a_single_put() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(64), &c), &c);
    let whole = noise(7, 40_000);
    for piece in whole.chunks(4096) {
        let before = st.stats().record_bytes_written;
        st.append("/big.bin", piece).unwrap();
        let wrote = st.stats().record_bytes_written - before;
        assert!(wrote < 4096 + 3 * 1024, "an append rewrote the file: {wrote}");
    }
    assert_eq!(st.get("/big.bin").unwrap().unwrap(), whole);
    // Fixed chunk offsets: the same bytes put at once are the same node.
    let id_appended = st.lookup("/big.bin").unwrap().unwrap().id;
    st.put("/copy.bin", &whole).unwrap();
    assert_eq!(st.lookup("/copy.bin").unwrap().unwrap().id, id_appended);
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.get("/big.bin").unwrap().unwrap(), whole);
    // Small appends grow one stored tail.
    st.append("/log.txt", b"one ").unwrap();
    st.append("/log.txt", b"two").unwrap();
    assert_eq!(st.get("/log.txt").unwrap().unwrap(), b"one two");
}

#[test]
fn deflated_chunks_are_verified_stored_coded_and_read_back() {
    let c = cfg();
    let mut st = mount(formatted(NorGeometry::c6(32), &c), &c);
    let a = text(1, 4096);
    let b = text(2, 3000);
    let id_a = ObjectId::of(&mut SoftSha256, IdTag::Blob, &[&a]);
    st.put_chunk_deflated("/p/s.glsl", 0, 4096, Some(id_a), &deflate(&a))
        .unwrap();
    st.put_chunk_deflated("/p/s.glsl", 4096, 3000, None, &deflate(&b))
        .unwrap();
    let mut whole = a.clone();
    whole.extend_from_slice(&b);
    assert_eq!(st.get("/p/s.glsl").unwrap().unwrap(), whole);
    let flash_bytes = st.stats().record_bytes_written;
    assert!(flash_bytes < 4000, "stored coded: {flash_bytes}");

    let before = st.flash().stats().program_calls;
    // A wrong id, a short stream, a wrong length, a bad offset: refused,
    // nothing written.
    assert_eq!(
        st.put_chunk_deflated("/p/t", 0, 4096, Some(ObjectId(42)), &deflate(&a)),
        Err(StoreError::Corrupt("chunk id"))
    );
    let z = deflate(&a);
    assert!(matches!(
        st.put_chunk_deflated("/p/t", 0, 4096, None, &z[..z.len() / 2]),
        Err(StoreError::Corrupt(_))
    ));
    assert!(matches!(
        st.put_chunk_deflated("/p/t", 0, 4095, None, &z),
        Err(StoreError::Corrupt(_))
    ));
    assert_eq!(
        st.put_chunk_deflated("/p/s.glsl", 5, 4096, None, &z),
        Err(StoreError::BadOffset)
    );
    assert_eq!(st.flash().stats().program_calls, before);
    // Incompressible: stored as plain bytes under the same id.
    let n = noise(3, 2000);
    st.put_chunk_deflated("/p/n.bin", 0, 2000, None, &deflate(&n))
        .unwrap();
    let mut st = mount(st.into_flash(), &c);
    assert_eq!(st.get("/p/s.glsl").unwrap().unwrap(), whole);
    assert_eq!(st.get("/p/n.bin").unwrap().unwrap(), n);
}

/// SHA-256, except every path containing `zz` hashes to one value.
struct CollidingHasher;

impl ObjectHasher for CollidingHasher {
    fn sha256(&mut self, parts: &[&[u8]]) -> [u8; 32] {
        if parts.first() == Some(&&[IdTag::Path as u8][..])
            && parts[1].windows(2).any(|w| w == b"zz")
        {
            return [7; 32];
        }
        SoftSha256.sha256(parts)
    }
}

#[test]
fn colliding_path_hashes_fall_back_to_the_walk() {
    let c = cfg();
    let mut f = lp_nor_sim::NorFlashSim::new(NorGeometry::c6(32));
    TreeStore::format(&mut f, &mut CollidingHasher, &c).unwrap();
    let Ok(mut st) = TreeStore::mount(f, CollidingHasher, c.clone()) else {
        panic!("mount")
    };
    st.put("/a/zz1.json", b"one").unwrap();
    st.put("/b/zz2.json", b"two").unwrap();
    st.put("/plain.json", b"plain").unwrap();
    assert_eq!(st.get("/a/zz1.json").unwrap().unwrap(), b"one");
    assert_eq!(st.get("/b/zz2.json").unwrap().unwrap(), b"two");
    assert_eq!(st.file_size("/b/zz2.json").unwrap(), Some(3));
    st.put("/a/zz1.json", b"one again").unwrap();
    assert_eq!(st.get("/b/zz2.json").unwrap().unwrap(), b"two");
    assert!(st.delete("/b/zz2.json").unwrap());
    assert_eq!(st.get("/b/zz2.json").unwrap(), None);
    assert_eq!(st.get("/a/zz1.json").unwrap().unwrap(), b"one again");
    st.put("/c/zz3.json", b"three").unwrap();
    // Mount rebuilds the table and finds the collision itself.
    let (f, _) = st.into_parts();
    let Ok(mut st) = TreeStore::mount(f, CollidingHasher, c) else {
        panic!("remount")
    };
    let got: Vec<Vec<u8>> = ["/a/zz1.json", "/c/zz3.json", "/plain.json"]
        .iter()
        .map(|p| st.get(p).unwrap().unwrap())
        .collect();
    assert_eq!(got, vec![b"one again".to_vec(), b"three".to_vec(), b"plain".to_vec()]);
    assert_eq!(st.get("/b/zz2.json").unwrap(), None);
}
