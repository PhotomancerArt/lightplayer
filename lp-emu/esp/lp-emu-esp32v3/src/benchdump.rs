//! The `bench` build's report: what the guest did, in four sections, written
//! to `LP_EMU_V3_BENCHPROF=<path>` at the end of a run.
//!
//! The host-time question is [`crate::selfprof`]'s. This is the guest-side
//! half — the counters in [`lp_emu_esp_common::benchprof`] plus the machine's
//! own interleave bookkeeping — and it is what the roadmap's four
//! classic-specific numbers are computed from:
//!
//! | number | section | how |
//! |---|---|---|
//! | MMIO share and top sites | `[mmio]` | directly: accesses by address, named |
//! | window-overflow/underflow rate | `[fetch]` | fetches landing on `_WindowOverflow{4,8,12}` / `_WindowUnderflow{4,8,12}`, whose addresses come from the firmware ELF |
//! | LOOP hotness | `[fetch]` | fetches inside a `loop`/`loopnez`/`loopgtz` body, whose extents come from the firmware's disassembly |
//! | cross-core switch rate | `[cores]` | windows given per core, against instructions retired per core |
//!
//! The two that read the ELF are **not** computed here on purpose. This
//! machine has no instruction tracer and no symbolizing disassembler, and
//! inventing one to classify a pc would be a second decoder to keep honest.
//! `scripts/emu/bench-esp32v3-counts.py` asks `xtensa-esp32-elf-nm` and
//! `xtensa-esp32-elf-objdump` — the toolchain that built the image — and
//! intersects their answers with the histogram below. What the emulator
//! claims is only "this pc retired this many times", which is a fact it owns.
//!
//! ⚠️ The `[fetch]` section is one line per distinct pc and a render-loop run
//! produces tens of thousands of them. It is a file, never stdout.

use std::fmt::Write as _;

use crate::machine::{CORES, Machine};

/// Write the report if `LP_EMU_V3_BENCHPROF` names a path.
pub fn dump(machine: &mut Machine) {
    let Ok(path) = std::env::var("LP_EMU_V3_BENCHPROF") else {
        return;
    };
    let mut out = String::with_capacity(1 << 22);

    // --- [run] ------------------------------------------------------------
    out.push_str("[run]\n");
    let _ = writeln!(out, "cycles {}", machine.cycles());
    let _ = writeln!(out, "instructions {}", machine.instructions());
    let _ = writeln!(out, "idle_skips {}", machine.idle_skips());
    let _ = writeln!(out, "quantum {}", machine.core_quantum());

    // --- [cores] ----------------------------------------------------------
    // One row per core: the instructions it retired, the windows it was
    // given, and the windows it ended parked. `windows` is the interleave's
    // switch count — every window is one `run_slice`, and the run loop hands
    // core 0 then core 1 one each per iteration.
    out.push_str("\n[cores]\n");
    out.push_str("# core instructions windows wfi_ends\n");
    for core in 0..CORES {
        let _ = writeln!(
            out,
            "{core} {} {} {}",
            machine.core_instructions(core),
            machine.bench_windows(core),
            machine.wfi_ends(core),
        );
    }

    // --- [mmio] -----------------------------------------------------------
    // Every MMIO address the guest touched, with the block and register name
    // resolved once here rather than on the access path.
    let (reads, writes, ram_reads, ram_writes, fetch_total) = {
        let b = &machine.bus().bench;
        (
            b.mmio_reads.clone(),
            b.mmio_writes.clone(),
            b.ram_reads,
            b.ram_writes,
            b.fetch_total,
        )
    };
    let mmio_total: u64 = reads.values().sum::<u64>() + writes.values().sum::<u64>();
    out.push_str("\n[mmio]\n");
    let _ = writeln!(out, "# total {mmio_total}");
    let _ = writeln!(out, "# ram_reads {ram_reads} ram_writes {ram_writes}");
    let _ = writeln!(out, "# fetches {fetch_total}");
    out.push_str("# address reads writes block register\n");
    let mut sites: Vec<u32> = reads.keys().chain(writes.keys()).copied().collect();
    sites.sort_unstable();
    sites.dedup();
    // Sorted by traffic, so the head of the section is the answer to "the top
    // sites" without a second pass.
    sites.sort_by_key(|a| {
        std::cmp::Reverse(reads.get(a).copied().unwrap_or(0) + writes.get(a).copied().unwrap_or(0))
    });
    for address in sites {
        let (block, reg) = machine
            .bus_mut()
            .bench_mmio_site(address)
            .map(|(b, r)| (b, r.unwrap_or("?")))
            .unwrap_or(("?", "?"));
        let _ = writeln!(
            out,
            "{address:#010x} {} {} {block} {reg}",
            reads.get(&address).copied().unwrap_or(0),
            writes.get(&address).copied().unwrap_or(0),
        );
    }

    // --- [fetch] ----------------------------------------------------------
    // One line per distinct pc, ascending, so the post-processor can walk it
    // against a sorted list of address ranges in one pass.
    out.push_str("\n[fetch]\n");
    out.push_str("# pc count\n");
    let mut pcs: Vec<(u32, u64)> = machine
        .bus()
        .bench
        .fetches
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    pcs.sort_unstable();
    for (pc, count) in pcs {
        let _ = writeln!(out, "{pc:#010x} {count}");
    }

    match std::fs::write(&path, out) {
        Ok(()) => eprintln!("benchprof: wrote {path}"),
        Err(e) => eprintln!("benchprof: could not write {path}: {e}"),
    }
}
