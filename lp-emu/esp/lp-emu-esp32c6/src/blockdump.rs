//! THROWAWAY (M5 P1): dump the block-profile counters. Never merge.

use crate::machine::Esp32C6Machine;

/// Write the counters to `LP_EMU_BLOCKPROF` if it is set.
pub fn dump(m: &mut Esp32C6Machine) {
    let Ok(path) = std::env::var("LP_EMU_BLOCKPROF") else {
        return;
    };
    m.harts[0].prof.finish();
    let p = &m.harts[0].prof;
    let bp = &m.bus.prof;
    let mut s = String::with_capacity(1 << 20);

    s.push_str("# regions: name base len exec writable\n");
    for r in m.bus.regions() {
        s.push_str(&format!(
            "region {} {:#x} {} {} {}\n",
            r.name,
            r.base,
            r.len(),
            r.exec as u8,
            r.writable as u8
        ));
    }
    s.push_str(&format!("instructions {}\n", p.instructions));
    s.push_str(&format!("loads {}\n", p.loads));
    s.push_str(&format!("stores {}\n", p.stores));
    s.push_str(&format!("amo {}\n", p.amo));
    s.push_str(&format!("system {}\n", p.system));
    s.push_str(&format!("fence {}\n", p.fence));
    s.push_str(&format!("control {}\n", p.control));
    s.push_str(&format!("slice_entries {}\n", p.slice_entries));
    s.push_str(&format!("restarts {}\n", p.restarts));
    s.push_str(&format!("instr_nomem_b {}\n", p.instr_nomem_b));
    s.push_str(&format!("blocks_nomem_b {}\n", p.blocks_nomem_b));
    s.push_str(&format!("bus_stores {}\n", bp.stores));
    s.push_str(&format!("bus_stores_exec_region {}\n", bp.stores_exec_region));
    s.push_str(&format!("bus_fetches {}\n", bp.fetches));
    s.push_str(&format!("load_image_calls {}\n", bp.load_image_calls));
    s.push_str(&format!("load_image_bytes {}\n", bp.load_image_bytes));

    s.push_str("# len_a <length> <blocks>\n");
    for (i, c) in p.len_a.iter().enumerate() {
        if *c > 0 {
            s.push_str(&format!("len_a {i} {c}\n"));
        }
    }
    s.push_str("# len_b <length> <blocks>\n");
    for (i, c) in p.len_b.iter().enumerate() {
        if *c > 0 {
            s.push_str(&format!("len_b {i} {c}\n"));
        }
    }
    s.push_str("# exec_page <page> <retired instructions>\n");
    for (page, c) in &p.exec_pages {
        s.push_str(&format!("exec_page {page:#x} {c}\n"));
    }
    s.push_str("# smc_changed / smc_same <page> <stores>\n");
    for (page, c) in &bp.exec_page_changed {
        s.push_str(&format!("smc_changed {page:#x} {c}\n"));
    }
    for (page, c) in &bp.exec_page_same {
        s.push_str(&format!("smc_same {page:#x} {c}\n"));
    }
    // Block-start entry counts, bucketed: the whole map would be tens of MB.
    for (label, starts) in [("starts_a", &p.starts_a), ("starts_b", &p.starts_b)] {
        let mut hist: std::collections::BTreeMap<u32, (u64, u64)> = Default::default();
        let mut distinct = 0u64;
        let mut entries = 0u64;
        for (_pc, c) in starts.iter() {
            distinct += 1;
            entries += *c;
            let bucket = 64 - (c.leading_zeros()); // ilog2+1
            let e = hist.entry(bucket).or_insert((0, 0));
            e.0 += 1;
            e.1 += *c;
        }
        s.push_str(&format!("{label}_distinct {distinct}\n"));
        s.push_str(&format!("{label}_entries {entries}\n"));
        for (b, (n, tot)) in hist {
            s.push_str(&format!("{label}_hist {b} {n} {tot}\n"));
        }
    }
    // The 40 hottest block starts under rule (b).
    let mut hot: Vec<(u32, u64)> = p.starts_b.iter().map(|(k, v)| (*k, *v)).collect();
    hot.sort_by(|a, b| b.1.cmp(&a.1));
    for (pc, c) in hot.iter().take(40) {
        s.push_str(&format!("hot_b {pc:#x} {c}\n"));
    }
    let _ = std::fs::write(path, s);
}
