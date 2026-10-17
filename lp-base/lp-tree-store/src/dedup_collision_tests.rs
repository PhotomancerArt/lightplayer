//! A dedup hit compares bytes (FORMAT.md "Ids"): ids are 64 bits of
//! SHA-256, so someone who can write can, with ~2³² work, make bytes whose
//! id equals a predictable file's. Under a hasher that collides on purpose
//! (`EVIL` hashes as `GOOD`), the second of two colliding writes — planted
//! first or second, stored or deflated — is refused, nothing is written,
//! and neither file reads the other's bytes. A real dedup (the same bytes,
//! stored or deflated) still shares one record.

use alloc::vec::Vec;

use lp_nor_sim::{NorFlashSim, NorGeometry};

use crate::node_write::ID_COLLISION;
use crate::object_hasher::ObjectHasher;
use crate::test_support::{deflate, text};
use crate::{SoftSha256, StoreConfig, StoreError, TreeStore};

#[test]
fn a_crafted_collision_never_aliases_a_victim_planted_later() {
    let mut st = store();
    let good = with(b"GOOD");
    st.put("/victim.json", &good).unwrap();
    let before = image(&st);
    assert_eq!(
        st.put("/attacker.json", &with(b"EVIL")),
        Err(StoreError::Corrupt(ID_COLLISION))
    );
    assert!(image(&st) == before, "a refused write wrote");
    assert_eq!(st.get("/victim.json").unwrap().unwrap(), good);
    assert_eq!(st.get("/attacker.json").unwrap(), None);
    let mut st = remount(st);
    assert_eq!(st.get("/victim.json").unwrap().unwrap(), good);
}

#[test]
fn a_crafted_collision_never_aliases_a_victim_planted_first() {
    let mut st = store();
    let evil = with(b"EVIL");
    st.put("/attacker.json", &evil).unwrap();
    assert_eq!(
        st.put("/victim.json", &with(b"GOOD")),
        Err(StoreError::Corrupt(ID_COLLISION))
    );
    assert_eq!(st.get("/victim.json").unwrap(), None);
    // Deflated over the wire: refused the same way.
    let good = with(b"GOOD");
    assert_eq!(
        st.put_chunk_deflated("/victim.json", 0, good.len() as u32, None, &deflate(&good)),
        Err(StoreError::Corrupt(ID_COLLISION))
    );
    assert_eq!(st.get("/victim.json").unwrap(), None);
    assert_eq!(st.get("/attacker.json").unwrap().unwrap(), evil);
}

/// A collision inside a multi-part file's chunks is caught per chunk, and
/// inside a transaction it fails the call, not the transaction.
#[test]
fn a_collision_in_one_chunk_of_a_big_file_is_refused() {
    let mut st = store();
    let mut good = text(1, 3000);
    good[1500..1504].copy_from_slice(b"GOOD");
    st.put("/a.bin", &good).unwrap();
    let mut evil = good.clone();
    evil[1500..1504].copy_from_slice(b"EVIL");
    st.begin().unwrap();
    st.put("/ok.json", b"{}").unwrap();
    assert_eq!(
        st.put("/b.bin", &evil),
        Err(StoreError::Corrupt(ID_COLLISION))
    );
    st.commit().unwrap();
    assert_eq!(st.get("/a.bin").unwrap().unwrap(), good);
    assert_eq!(st.get("/b.bin").unwrap(), None);
    assert_eq!(st.get("/ok.json").unwrap().unwrap(), b"{}");
}

/// The compare does not refuse what dedup is for: the same bytes again,
/// stored over stored, deflated over stored and stored over deflated.
#[test]
fn equal_bytes_still_share_one_record() {
    let mut st = store();
    let bytes = text(7, 600);
    st.put("/a.json", &bytes).unwrap();
    st.put("/b.json", &bytes).unwrap();
    let z = deflate(&bytes);
    st.put_chunk_deflated("/c.json", 0, bytes.len() as u32, None, &z)
        .unwrap();
    let other = text(8, 600);
    st.put_chunk_deflated("/d.json", 0, other.len() as u32, None, &deflate(&other))
        .unwrap();
    st.put("/e.json", &other).unwrap();
    let ids: Vec<_> = ["/a.json", "/b.json", "/c.json"]
        .iter()
        .map(|p| st.lookup(p).unwrap().unwrap().id)
        .collect();
    assert!(ids.windows(2).all(|w| w[0] == w[1]));
    let mut st = remount(st);
    for (p, want) in [
        ("/a.json", &bytes),
        ("/b.json", &bytes),
        ("/c.json", &bytes),
        ("/d.json", &other),
        ("/e.json", &other),
    ] {
        assert_eq!(&st.get(p).unwrap().unwrap(), want, "{p}");
    }
}

// ---- helpers -----------------------------------------------------------

type CollidingStore = TreeStore<NorFlashSim, CollidingSha>;

/// SHA-256, except that every `EVIL` hashes as `GOOD`: two different byte
/// strings with one id, as a crafted collision would make.
#[derive(Clone, Copy, Default)]
struct CollidingSha;

impl ObjectHasher for CollidingSha {
    fn sha256(&mut self, parts: &[&[u8]]) -> [u8; 32] {
        let mut all = parts.concat();
        for i in 0..all.len().saturating_sub(3) {
            if &all[i..i + 4] == b"EVIL" {
                all[i..i + 4].copy_from_slice(b"GOOD");
            }
        }
        SoftSha256.sha256(&[&all])
    }
}

/// A small JSON-ish file around `word`.
fn with(word: &[u8]) -> Vec<u8> {
    let mut v = Vec::from(&b"{\"wifi\": \""[..]);
    v.extend_from_slice(word);
    v.extend_from_slice(b"\"}");
    v
}

fn cfg() -> StoreConfig {
    StoreConfig::default()
}

fn store() -> CollidingStore {
    match TreeStore::format(NorFlashSim::new(NorGeometry::c6(16)), CollidingSha, cfg()) {
        Ok(st) => st,
        Err((e, ..)) => panic!("format: {e:?}"),
    }
}

fn remount(st: CollidingStore) -> CollidingStore {
    match TreeStore::mount(st.into_flash(), CollidingSha, cfg()) {
        Ok(st) => st,
        Err((e, ..)) => panic!("mount: {e:?}"),
    }
}

fn image(st: &CollidingStore) -> Vec<u8> {
    let mut v = alloc::vec![0u8; 16 * 4096];
    st.flash().peek(0, &mut v);
    v
}
