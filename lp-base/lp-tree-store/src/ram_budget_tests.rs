//! RAM against the budget (plan D3: ≈ 8 KB resident at c40 on 128 sectors,
//! ≈ 5 KB transient per operation excluding the caller's file buffer), on a
//! c40-shaped synthetic tree, cross-checked with the counting allocator.
//! Simulator numbers (lp-nor-sim), not silicon.

extern crate std;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_nor_sim::NorGeometry;

use crate::test_support::{formatted, heap_use, mount, text};
use crate::{SoftSha256, StoreConfig, TreeStore};

/// G1's bound on mount's peak at 128 sectors, whatever the garbage (P8).
const MOUNT_PEAK_AT_128: usize = 16 * 1024;

/// c40's shape: 132 documents, ~216 KB, under one project, plus board
/// files (sizes from the spike corpus: many small JSON nodes, ~40 shaders,
/// a few big maps).
pub fn c40_like() -> Vec<(String, Vec<u8>)> {
    let mut v = Vec::new();
    v.push((String::from("/hardware.json"), text(1, 900)));
    v.push((String::from("/lightplayer.json"), text(2, 300)));
    v.push((String::from("/.lp/device.json"), text(3, 200)));
    v.push((String::from("/projects/a/project.json"), text(4, 2400)));
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
    v.push((String::from("/projects/a/.lp/panel.json"), text(400, 450)));
    v
}

#[test]
fn c40_resident_and_transient_are_inside_the_budget() {
    for sectors in [128, 176] {
        c40_budget_on(sectors);
    }
}

fn c40_budget_on(sectors: u32) {
    let c = StoreConfig::default();
    let mut st = mount(formatted(NorGeometry::c6(sectors), &c), &c);
    let files = c40_like();
    let total: usize = files.iter().map(|f| f.1.len()).sum();
    assert!((200_000..240_000).contains(&total), "{total}");
    st.begin().unwrap();
    for (p, b) in &files {
        st.put(p, b).unwrap();
    }
    st.commit().unwrap();
    let flash = st.into_flash();

    let (st, mount_peak, held) =
        heap_use(|| match TreeStore::mount(flash, SoftSha256, c.clone()) {
            Ok(s) => s,
            Err(_) => panic!("mount"),
        });
    let s = st.stats();
    std::println!(
        "c40/{sectors}: resident {} B (index {} × 12, paths {} × 20, sectors {}), mount peak {} B, \
         held {} B, mount read {} B in 1 + {} scans",
        s.resident_ram_bytes,
        s.index_entries,
        s.path_table_entries,
        s.sector_table_ram_bytes,
        mount_peak,
        held,
        s.mount_bytes_read,
        s.mount_scans
    );
    // D3 says ≈ 8 KB; this is a regression ceiling, not the budget (the
    // measured figure is reported at G1).
    // (12 B per sector past 128.)
    assert!(
        s.resident_ram_bytes <= 9 * 1024 + (sectors as usize - 128) * 12,
        "{s:?}"
    );
    // What the allocator holds after mount is the resident structures.
    assert!(
        (held as usize) <= s.resident_ram_bytes + 512,
        "held {held} vs resident {}",
        s.resident_ram_bytes
    );
    // Mount's peak (G1, P8): ≤ 16 KB at 128 sectors; a sector costs mount
    // ~30 B (the sector table, the sorted header list, the closed flags).
    assert!(
        mount_peak <= MOUNT_PEAK_AT_128 + (sectors as usize - 128) * 32,
        "mount peak {mount_peak}"
    );

    // Per-operation transient, by the allocator (the caller's buffer — the
    // bytes passed in, or `get`'s result — excluded).
    let mut st = st;
    let shader = text(999, 2900);
    let panel = text(998, 450);
    let piece = text(5, 4096);
    let mut worst = 0;
    let mut ops: Vec<(&str, usize)> = Vec::new();
    let (_, p, _) = heap_use(|| {
        st.put("/projects/a/modules/m07/shader.glsl", &shader)
            .unwrap()
    });
    ops.push(("put shader", p));
    let (_, p, _) = heap_use(|| st.put("/projects/a/.lp/panel.json", &panel).unwrap());
    ops.push(("put panel", p));
    let (_, p, _) = heap_use(|| st.append("/projects/a/maps/map0.json", &piece).unwrap());
    ops.push(("append 4 KiB", p));
    let (got, p, _) = heap_use(|| st.get("/projects/a/maps/map1.json").unwrap().unwrap());
    ops.push(("get 18 KB", p - got.capacity()));
    let (_, p, _) = heap_use(|| st.file_size("/projects/a/project.json").unwrap());
    ops.push(("file_size", p));
    for (name, p) in &ops {
        std::println!("  {name}: transient {p} B");
        worst = worst.max(*p);
    }
    // D3: ≈ 5 KB per operation, the caller's buffer excluded.
    assert!(worst <= 5 * 1024, "{ops:?}");
}

/// A full store: c40 (stored, then host-deflated), then panel writes and
/// saves until the 128 sectors have been written over about three times
/// (old sectors reclaimed by erasing; with this workload every reclaimed
/// sector is wholly garbage, so GC copies nothing). Resident must stay
/// bounded (the index is pruned back by a mark when it outgrows the live
/// set), and so must mount's peak: it indexes only the root's closure, so a
/// flash full of garbage costs it no more than a freshly pushed one.
#[test]
fn a_full_store_keeps_resident_and_the_mount_peak_bounded() {
    for deflated in [false, true] {
        let c = StoreConfig::default();
        let mut st = mount(formatted(NorGeometry::c6(128), &c), &c);
        st.begin().unwrap();
        for (p, b) in &c40_like() {
            if !deflated || crate::is_hot(p) {
                st.put(p, b).unwrap();
                continue;
            }
            let mut off = 0;
            for ch in crate::host_deflate_chunks(&mut SoftSha256, b, c.record_max) {
                st.put_chunk_deflated(p, off, ch.logical_len, Some(ch.id), &ch.deflated)
                    .unwrap();
                off += ch.logical_len;
            }
        }
        st.commit().unwrap();
        let at_rest = st.stats().resident_ram_bytes;
        let mut max_resident = 0;
        let mut max_peak = 0;
        let mut round = 0u64;
        while st.stats().erases < 400 {
            st.put("/projects/a/.lp/panel.json", &text(round, 450))
                .unwrap();
            if round % 10 == 0 {
                let p = format!("/projects/a/nodes/n{:02}.json", round % 44);
                st.put(&p, &text(round, 450)).unwrap();
            }
            let s = st.stats();
            max_resident = max_resident.max(s.resident_ram_bytes);
            max_peak = max_peak.max(s.transient_peak_bytes);
            round += 1;
            assert!(round < 200_000, "{s:?}");
        }
        let flash = st.into_flash();
        let in_use = flash.sectors_in_use();
        let (st, mount_peak, held) =
            heap_use(|| match TreeStore::mount(flash, SoftSha256, c.clone()) {
                Ok(s) => s,
                Err(_) => panic!("mount"),
            });
        std::println!(
            "full c40/128 deflated={deflated}: resident after the push {at_rest} B, max \
             {max_resident} B over {round} writes; largest buffer {max_peak} B; remount with \
             {in_use} sectors in use: peak {mount_peak} B, held {held} B ({} index entries), \
             mount read {} B in 1 + {} scans",
            st.stats().index_entries,
            st.stats().mount_bytes_read,
            st.stats().mount_scans
        );
        // A regression ceiling, not the budget: the measured figure is reported.
        assert!(max_resident <= 11 * 1024, "{max_resident}");
        assert_eq!(in_use, 128, "the flash is not full of garbage");
        assert!(mount_peak <= MOUNT_PEAK_AT_128, "mount peak {mount_peak}");
    }
}
