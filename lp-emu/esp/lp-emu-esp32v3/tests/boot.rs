//! The classic machine builds, the ROM places, the seeds land, the hart's
//! reset and boot states are right, and a strict run stops somewhere honest.
//!
//! The last one is the phase's real deliverable. M3 P2 does not model a
//! single peripheral, so a `--strict-bus` run **is expected to stop** — at
//! the first block the boot touches. That stop, with its pc, symbol and
//! cycle, is what P3 reads and turns into an accept-block table.

use lp_emu_esp32v3::machine::{
    BootFrame, BootMode, CORES, Esp32V3Builder, Machine, Outcome, RESET_VECTOR_OFS, StopCondition,
    TimeGrade,
};
use lp_emu_esp32v3::{bus_setup, memmap, rom};
use lp_xt_emu::mach::sr::{PS_BOOT, PS_RESET};

/// The vector table, as the vendored ROM's own program headers place it
/// (`m3/notes.md` §2). Read from the ROM rather than from `xtensa-lx-rt`'s
/// generated config, so this is the second, independent source for the same
/// `VECOFS` table M1 pinned.
const VECTORS: &[(&str, u32, u32)] = &[
    ("WindowVectors", 0x000, 0x170),
    ("Level2InterruptVector", 0x180, 6),
    ("Level3InterruptVector", 0x1C0, 6),
    ("Level4InterruptVector", 0x200, 6),
    ("Level5InterruptVector", 0x240, 6),
    ("DebugExceptionVector", 0x280, 11),
    ("NMIExceptionVector", 0x2C0, 3),
    ("KernelExceptionVector", 0x300, 6),
    ("UserExceptionVector", 0x340, 0x17),
    ("DoubleExceptionVector", 0x3C0, 9),
    ("ResetVector", 0x400, 0x15D),
];

fn rom_up() -> Machine {
    Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .build()
        .expect("the vendored ROM builds a machine")
}

// ---------------------------------------------------------------------------
// The ROM
// ---------------------------------------------------------------------------

#[test]
fn rom_places_every_loadable_segment() {
    let machine = rom_up();
    let image = machine.rom();
    assert_eq!(
        image.segments.len(),
        39,
        "the vendored esp32_rev300_rom.elf has 39 PT_LOADs (m3/notes.md §2)"
    );
    // ⚠️ MEASURED, and it is not `m3/notes.md` §2's "twenty empty". Thirteen
    // PT_LOADs have `memsz == 0`: twelve at `vaddr = 0` and one `.bss_phyrom`
    // at `0x3FFA_E270`. The notes' twenty counted `filesz == 0`, which is a
    // different set — seven of the eight `.bss_*` segments carry no file
    // bytes but do have a `memsz`, so they are placed as zero-fill and are
    // not "empty" to a loader. Reported to the director as a notes
    // correction, not a bug.
    assert_eq!(
        rom::empty_segments(image),
        13,
        "thirteen PT_LOADs have memsz == 0: twelve at vaddr 0 and `.bss_phyrom`"
    );
    assert_eq!(
        machine.rom_segments().len(),
        39 - 13,
        "every non-empty PT_LOAD is placed, and the empty ones are skipped and counted"
    );

    // The one segment whose file bytes are the ELF's own headers. Placing it
    // the ordinary way writes them 1,284 bytes below anything the map claims.
    let header_mapped: Vec<_> = machine
        .rom_segments()
        .iter()
        .filter(|s| s.header_mapped)
        .collect();
    assert_eq!(
        header_mapped.len(),
        1,
        "exactly one PT_LOAD maps the ELF headers (see rom::is_header_map)"
    );
    let hm = header_mapped[0];
    assert_eq!(hm.vaddr, 0x3FFA_DAFC);
    assert_eq!(
        hm.filesz,
        52 + 39 * 32,
        "its file bytes are exactly the 52-byte ELF header plus 39 program headers — which is \
         what makes `p_offset = 0` the right reading and not a coincidence"
    );
    assert_eq!(hm.memsz, 0x534);

    // The highest code byte: the end of the second `.secureboot_*` chunk at
    // `0x4006_5000 + 0xD90` (m3/notes.md §2).
    let highest_code = machine
        .rom_segments()
        .iter()
        .filter(|s| s.execute)
        .map(|s| s.vaddr + s.memsz)
        .max()
        .expect("the ROM has executable segments");
    assert_eq!(highest_code, 0x4006_5D90);
    assert_eq!(
        highest_code,
        memmap::ROM_MASK_BASE + memmap::ROM_MASK_LEN,
        "memmap's ROM_MASK_LEN is that same extent"
    );
}

#[test]
fn the_two_relocated_segments_are_recorded_and_placed_by_vaddr() {
    let machine = rom_up();
    let relocated: Vec<_> = machine
        .rom_segments()
        .iter()
        .filter(|s| s.relocated())
        .collect();
    // FOUR, not the two `m3/notes.md` §2 names in that sentence: the
    // four-byte `.from_now_on_*` pair at `0x3FFA_E000` / `0x3FFE_0000`, AND
    // the two read-only `.rodata` chunks at `0x3FF9_6000` / `0x3FF9_F100`,
    // whose vaddr/paddr difference the same section of the notes records
    // separately. All four are placed by vaddr and all four say so.
    assert_eq!(
        relocated.len(),
        4,
        "a loader must place by vaddr and SAY it did, not pick silently: {:?}",
        relocated
            .iter()
            .map(|s| (s.vaddr, s.paddr))
            .collect::<Vec<_>>()
    );
    let mut bases: Vec<u32> = relocated.iter().map(|s| s.vaddr).collect();
    bases.sort_unstable();
    assert_eq!(
        bases,
        vec![0x3FF9_6000, 0x3FF9_F100, 0x3FFA_E000, 0x3FFE_0000]
    );
    for seg in relocated {
        assert!(
            !seg.regions.is_empty(),
            "a relocated segment still lands in a named region"
        );
    }
}

#[test]
fn rom_seeds_non_alloc_data() {
    let mut machine = rom_up();
    let seeded = machine.rom_data().to_vec();
    assert!(
        seeded.len() >= 8,
        "at least the eight non-empty non-alloc `.data_*` sections are seeded, got {}: {:?}",
        seeded.len(),
        seeded.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    let xtos = seeded
        .iter()
        .find(|s| s.name == ".data_xtos_pro")
        .expect("`.data_xtos_pro` is seeded: it is the ROM's xtos dispatch tables");
    assert_eq!(xtos.address, 0x3FFE_0440);
    assert_eq!(xtos.len, 0x420, "1,056 bytes");

    // Not merely "a section was recorded": the bytes are actually in memory.
    // A zero here does not fail the boot; it fails somewhere else, later,
    // unrecognisably.
    let words: Vec<u32> = (0..xtos.len / 4)
        .filter_map(|i| machine.peek_word(xtos.address + i * 4))
        .collect();
    assert_eq!(words.len() as u32, xtos.len / 4);
    assert!(
        words.iter().any(|w| *w != 0),
        "`.data_xtos_pro` was seeded with real bytes, not zeros"
    );

    let image = machine.rom_data_image();
    assert_eq!(image.sections, seeded.len());
    assert_eq!(image.bytes, seeded.iter().map(|s| s.len).sum::<u32>());
    assert!(
        image.bytes >= 7_800,
        "the eight non-empty non-alloc sections total about 7.8 KiB (m3/notes.md §2), got {}",
        image.bytes
    );
}

#[test]
fn rom_entry_is_the_reset_vector() {
    let machine = rom_up();
    assert_eq!(machine.rom().entry, 0x4000_0400);
    assert_eq!(
        machine.rom().entry,
        memmap::ROM_MASK_BASE + RESET_VECTOR_OFS,
        "e_entry and XCHAL_RESET_VECTOR_VADDR agree — two independent sources"
    );
    assert_eq!(
        machine.harts[0].pc(),
        0x4000_0400,
        "a rom-up machine starts at the reset vector"
    );
}

#[test]
fn vector_table_offsets() {
    let machine = rom_up();
    for (name, ofs, size) in VECTORS {
        let vaddr = memmap::ROM_MASK_BASE + ofs;
        let seg = machine
            .rom_segments()
            .iter()
            .find(|s| s.vaddr == vaddr)
            .unwrap_or_else(|| {
                panic!(
                    "`.{name}.text` has no PT_LOAD at {vaddr:#010x} — the ROM's vector table \
                        is one PT_LOAD per vector (m3/notes.md §2)"
                )
            });
        assert_eq!(
            seg.memsz, *size,
            "`.{name}.text` at {vaddr:#010x} is {size} bytes"
        );
        assert!(seg.execute, "`.{name}.text` is executable");
    }
}

#[test]
fn the_break_patch_is_the_three_byte_form_and_matches_the_encoder() {
    // The oracle is the encoder, never analogy: M0 found a real misdecode in
    // this repo that came from deriving an encoding by hand.
    let bytes = lp_xt_inst::encode(&lp_xt_inst::Inst::Break(1, 15));
    assert_eq!(
        bytes.len(),
        3,
        "`break 1, 15` is the three-byte form; `break.n` is the two-byte one"
    );
    assert_eq!(bytes.as_slice(), &rom::BREAK_1_15_BYTES);
    assert_eq!(
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]),
        rom::BREAK_1_15
    );
}

#[test]
fn the_hook_table_ships_empty() {
    let machine = rom_up();
    assert!(
        machine.hooks().is_empty(),
        "the hook table ships EMPTY and stays empty: try the real ROM path before any hook"
    );
}

// ---------------------------------------------------------------------------
// The bus
// ---------------------------------------------------------------------------

#[test]
fn the_bus_maps_the_map_and_declares_the_mmio_window() {
    let machine = rom_up();
    let names: Vec<&str> = machine.bus().regions().iter().map(|r| r.name).collect();
    for span in memmap::RAM_SPANS {
        let mapped = names.contains(&span.name);
        let skipped = bus_setup::deliberately_unmapped()
            .iter()
            .any(|(s, _)| s.name == span.name);
        assert!(
            mapped != skipped,
            "`{}` is either mapped or deliberately unmapped, never both and never neither",
            span.name
        );
    }
    assert_eq!(
        machine.bus().watchpoint_slots(),
        2,
        "XCHAL_NUM_DBREAK on the classic LX6 is 2"
    );
    assert!(
        machine.bus().in_mmio_window(memmap::periph::UART0),
        "UART0's base is inside the declared MMIO window"
    );
    assert_eq!(
        machine.bus().peripheral_count(),
        lp_emu_esp32v3::machine::PERIPHERAL_REGISTRATION_ORDER.len(),
        "P3 registers exactly the accept blocks the strict runs demanded, in the declared order"
    );
    let names: Vec<&str> = machine
        .peripheral_map()
        .iter()
        .map(|(n, _, _)| *n)
        .collect();
    assert_eq!(
        names,
        lp_emu_esp32v3::machine::PERIPHERAL_REGISTRATION_ORDER,
        "the registration order is the contract"
    );
    for (name, base, len) in machine.peripheral_map() {
        assert!(
            machine.bus().in_mmio_window(*base) && machine.bus().in_mmio_window(base + len - 1),
            "`{name}` lies inside the declared MMIO window"
        );
    }

    // And a bare machine is still P2's: an empty window, for reading the
    // first stop of each path against nothing.
    let bare = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .bare()
        .build()
        .expect("builds");
    assert_eq!(bare.bus().peripheral_count(), 0);
}

#[test]
fn the_windows_this_machine_will_not_map_are_named() {
    let unmapped = bus_setup::deliberately_unmapped();
    let names: Vec<&str> = unmapped.iter().map(|(s, _)| s.name).collect();
    assert!(
        names.contains(&"sram1-ibus-alias"),
        "the SRAM1 I-bus alias is named and unmapped (DD24)"
    );
    assert!(
        names.contains(&"rtc-fast-ibus"),
        "RTC fast memory's I-bus view is named and unmapped: SocBus cannot alias two windows \
         onto one store, and two independent stores would disagree silently"
    );
    // A strict stop inside one of them can say which it was.
    let (span, why) = bus_setup::unmapped_window(memmap::SRAM1_IBUS_ALIAS_BASE + 4)
        .expect("an address inside a named window resolves to it");
    assert_eq!(span.name, "sram1-ibus-alias");
    assert!(!why.is_empty());
}

// ---------------------------------------------------------------------------
// The harts
// ---------------------------------------------------------------------------

#[test]
fn hart_slots() {
    let mut machine = rom_up();
    assert_eq!(machine.harts.len(), CORES);
    assert_eq!(CORES, 2, "the classic is dual-core");
    assert!(!machine.core_stalled(0));
    assert!(
        machine.core_stalled(1),
        "slot 1 is stalled for the whole of M3 (Q5)"
    );
    assert!(
        machine.core_report()[1].contains("stalled"),
        "core 1 is never silently absent from a report: {:?}",
        machine.core_report()
    );

    let before = machine.harts[1].cycle_count();
    let _ = machine.run_until(&StopCondition::after_micros(50));
    assert_eq!(
        machine.harts[1].cycle_count(),
        before,
        "a stalled hart consumes NO guest time across a run"
    );
}

#[test]
fn the_two_cores_answer_different_prids() {
    // Not hart indices: esp-hal keys on bit 13 of PRID.
    let machine = rom_up();
    assert_eq!(machine.harts[0].hart_id(), 0);
    assert_eq!(machine.harts[1].hart_id(), 1);
    assert_eq!(lp_emu_esp32v3::machine::PRID_PRO & (1 << 13), 0);
    assert_ne!(lp_emu_esp32v3::machine::PRID_APP & (1 << 13), 0);
}

#[test]
fn a_rom_up_hart_is_at_the_architectural_reset_state() {
    let machine = rom_up();
    assert_eq!(
        machine.harts[0].ps(),
        PS_RESET,
        "rom-up seeds nothing: the ROM's own reset vector sets PS itself"
    );
    assert_eq!(machine.harts[0].pc(), 0x4000_0400);
}

#[test]
fn direct_seed_ps() {
    // `seed_boot_state` is the direct load's seam, exercised here without an
    // application image so the assertion is about the seam and not about
    // whatever `fw-esp32v3` happens to be today.
    let mut machine = rom_up();
    let frame = BootFrame::rom_pro_stack(machine.rom());
    machine
        .seed_boot_state(memmap::SRAM0_IRAM, frame)
        .expect("the boot frame seeds");

    assert_eq!(
        machine.harts[0].ps(),
        PS_BOOT,
        "PS = WOE | UM | CALLINC(2) = 0x00060020 — the Xtensa twin of the C6's mstatus = 0x1888"
    );
    assert_eq!(machine.harts[0].ps(), 0x0006_0020);
    assert_eq!(machine.harts[0].pc(), memmap::SRAM0_IRAM);
}

#[test]
fn the_boot_frame_is_the_roms_own_pro_stack() {
    let machine = rom_up();
    let frame = BootFrame::rom_pro_stack(machine.rom());
    assert_eq!(
        frame.sp, 0x3FFE_3F20,
        "`__stack` in the vendored ROM ELF, which is also `reserved_rom_stack_pro`'s end in \
         third_party/esp-hal/ld/esp32/memory.x:32"
    );
    assert_eq!(frame.sp, memmap::ROM_PRO_STACK_TOP);
    assert_eq!(
        machine.rom().symbol("__stack").map(|s| s.address),
        Some(frame.sp),
        "the value is RESOLVED from the ROM, not hardcoded next to it"
    );
}

/// **The `a1` seam.** M1 P4's finding, made a test.
///
/// With `PS_BOOT`'s `CALLINC = 2` the app's `Reset` runs as frame 2 and the
/// bootloader's frame 0 stays live. A hart left with `a1 = 0` has a live
/// outermost frame whose stack pointer is null, and the first register spill
/// inside any exception does `l32e a0, a1, -12` against `0xFFFF_FFF4` — an
/// unmapped address whose own fault faults again. A double exception,
/// forever, nowhere near its cause.
#[test]
fn a_seeded_hart_has_a_mapped_spill_target_and_an_unseeded_one_does_not() {
    let mut machine = rom_up();
    let frame = BootFrame::rom_pro_stack(machine.rom());
    machine
        .seed_boot_state(memmap::SRAM0_IRAM, frame)
        .expect("the boot frame seeds");

    assert_ne!(machine.harts[0].cpu().a(1), 0, "a1 is seeded");
    assert_eq!(machine.harts[0].cpu().a(1), frame.sp);

    // `_WindowOverflow8`'s `l32e a0, a1, -12`: the word the spill reads to
    // find the next frame's stack pointer.
    let spill_source = frame.sp.wrapping_sub(12);
    assert_eq!(
        machine.peek_word(spill_source),
        Some(frame.sp),
        "the outermost frame's base save area holds a mapped stack pointer at a1-12"
    );
    // And the three words either side of it are readable too — the whole
    // save area `[a1-16, a1)` that `s32e a0..a3` writes.
    for i in 0..4u32 {
        let at = frame.sp - 16 + 4 * i;
        assert_eq!(machine.peek_word(at), Some(frame.save_area[i as usize]));
    }

    // The unseeded reading of the same instruction, for contrast: `a1 = 0`
    // makes the spill read address 0xFFFFFFF4, which no region of this map
    // claims.
    let null_spill = 0u32.wrapping_sub(12);
    assert_eq!(null_spill, 0xFFFF_FFF4);
    // Region containment rather than a read: without `--strict-bus` an
    // unmapped read answers zero, which is precisely the silence this seam
    // exists to avoid relying on.
    assert!(
        !machine
            .bus()
            .regions()
            .iter()
            .any(|r| r.contains(null_spill)),
        "with a1 = 0 the spill reads an address no region of this map claims — the double \
         fault's root"
    );
    assert!(
        machine
            .bus()
            .regions()
            .iter()
            .any(|r| r.contains(spill_source)),
        "with a1 seeded it reads one that is claimed"
    );
}

/// The live half of the seam: a **real** exception on a seeded hart reaches
/// the ROM's user exception vector rather than dying at the double
/// exception vector.
#[test]
fn a_seeded_hart_survives_its_first_exception() {
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        // The point is the exception, not the encoding: an undecodable word
        // must raise the architectural illegal-instruction exception here,
        // not the bring-up stop.
        .strict_unsupported(false)
        .build()
        .expect("builds");

    let at = memmap::SRAM0_IRAM;
    let mut program = lp_xt_inst::encode(&lp_xt_inst::Inst::Entry(lp_xt_inst::Reg::new(1), 0x20));
    // An encoding no Xtensa decoder accepts: op0 = 0xF is the narrow group,
    // and 0xFFFF is not one of its instructions.
    program.extend_from_slice(&[0xFF, 0xFF]);
    machine
        .bus_mut()
        .load_image(at, &program)
        .expect("SRAM0 takes a host-side byte write");

    let frame = BootFrame::rom_pro_stack(machine.rom());
    machine.seed_boot_state(at, frame).expect("seeds");

    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(2_000),
        ..Default::default()
    });

    if let Outcome::Fault { fault, .. } = &outcome {
        assert!(
            !matches!(fault, lp_xt_emu::mach::HartFault::TrapVectorFetch { .. }),
            "a seeded hart must not double-fault on its first exception: {fault:?}"
        );
    }
    // Whatever it did next, it left the synthetic program: the exception was
    // delivered rather than swallowed.
    assert_ne!(
        machine.harts[0].pc(),
        at,
        "the exception moved the pc off the faulting instruction"
    );
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

#[test]
fn strict_stops_somewhere_honest_from_the_reset_vector() {
    // A BARE machine: P2's reading of the ROM path, kept as the reference
    // the P3 ledger starts from.
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .bare()
        .strict(true)
        .build()
        .expect("builds");

    let outcome = machine.run_until(&StopCondition::after_micros(50_000));

    // With no peripheral a strict run IS expected to stop. What this test
    // pins is that it stops with a NAMED stop rather than running on
    // through zeros or dying without a report.
    match &outcome {
        Outcome::StrictBus { violation } => {
            assert!(
                machine.symbolize(violation.pc).is_some(),
                "the stop names a ROM symbol: {violation:?}"
            );
            assert!(
                violation.in_mmio_window,
                "the classic ROM's first strict stop is an UNMODELLED BLOCK inside the \
                 declared peripheral window, not a wild pointer: {violation:?}"
            );
        }
        Outcome::Fault { .. } => {
            // Also honest: the ROM reached an instruction this emulator does
            // not implement, or a vector it cannot fetch. P3 reads it.
        }
        other => {
            panic!("a strict rom-up run with no peripherals should stop, not finish: {other:?}")
        }
    }
    assert_eq!(
        outcome.exit_code() != 0,
        true,
        "a stop is a non-zero exit code"
    );
}

/// Where the ROM-up path stands at the end of P3, pinned: with EFUSE an
/// accept block, the mask ROM's `_reload_efuses_and_check` writes `conf`
/// (`+0xfc`) = `0x5aa5` (the read opcode) and `cmd` (`+0x104`) = 1
/// (`read_cmd`), requires `cmd` to read back **non-zero** (a zero sends it
/// to `_rtc_trigger_sw_system_reset`), then spins until it reads **zero**:
///
/// ```text
/// 4000fc90:  l32r    a1, (3ff5a0fc)     ; EFUSE.conf
/// 4000fc93:  l32r    a2, (3ff5a104)     ; EFUSE.cmd
/// 4000fc9b:  s32i.n  a3, a1, 0          ; conf = 0x5aa5
/// 4000fc9d:  s32i.n  a4, a2, 0          ; cmd = 1 (read_cmd)
/// 4000fca2:  l32i.n  a1, a2, 0
/// 4000fca6:  beqz    a1, _rtc_trigger_sw_system_reset
/// 4000fcac:  l32i.n  a1, a2, 0          ; +0x1c
/// 4000fcae:  bnez    a1, 4000fca9       ; +0x1e, the spin
/// ```
///
/// No constant satisfies both reads, and no `RegFile` rule expresses "1
/// until the read completes, then 0": the eFuse controller's read command
/// is a **completion**, which is P5's eFuse view. So a strict rom-up run
/// does not stop — it runs to its deadline in that loop, and this test
/// holds that reading until P5 moves it.
#[test]
fn rom_up_stands_at_the_efuse_read_command_until_p5() {
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .strict(true)
        .build()
        .expect("builds");
    let outcome = machine.run_until(&StopCondition::after_micros(2_000));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "no strict stop and no fault before the deadline: {outcome:?}"
    );
    let pc = machine.harts[0].pc();
    let sym = machine.symbolize(pc).unwrap_or_default();
    assert!(
        sym.contains("_reload_efuses_and_check"),
        "the ROM is spinning in _reload_efuses_and_check, got pc={pc:#010x} ({sym})"
    );
    assert!(
        (0x4000_FCA9..=0x4000_FCB0).contains(&pc),
        "inside the `l32i; bnez` loop on EFUSE.cmd: {pc:#010x}"
    );
    // The accept block holds the 1 the ROM wrote, which is why it spins.
    assert_eq!(
        machine.peek_word(memmap::periph::EFUSE + 0x104),
        Some(1),
        "EFUSE.cmd.read_cmd, remembered"
    );
    assert_eq!(
        machine.peek_word(memmap::periph::EFUSE + 0x0fc),
        Some(0x5aa5),
        "EFUSE.conf, the read opcode the ROM wrote first"
    );
}

#[test]
fn time_grade_t1_is_the_only_grade() {
    assert_eq!(TimeGrade::parse("t1"), Ok(TimeGrade::T1));
    let err = TimeGrade::parse("t2").expect_err("t2 does not exist on this machine");
    assert!(
        err.contains("measured"),
        "the refusal names the calibration that does not exist yet: {err}"
    );
    assert_eq!(TimeGrade::T1.configuration(), "lp-emu:esp32v3:t1");
    assert_eq!(memmap::CYCLES_PER_US, 240, "micros = cycles / 240");
}

#[test]
fn snapshot_round_trip() {
    let mut machine = rom_up();
    let stop = StopCondition {
        stop_cycle: Some(5_000),
        ..Default::default()
    };
    let _ = machine.run_until(&stop);

    let snap = machine.snapshot();
    let pc = machine.harts[0].pc();
    let cycle = machine.cycles();
    let ps = machine.harts[0].ps();
    assert_eq!(snap.cycle(), cycle);
    assert_eq!(snap.stalled, [false, true]);
    assert!(snap.bytes() > 0);

    // Run on, then restore: the machine is back where the snapshot was.
    let _ = machine.run_until(&StopCondition {
        stop_cycle: Some(cycle + 5_000),
        ..Default::default()
    });
    machine.restore(&snap);

    assert_eq!(machine.harts[0].pc(), pc);
    assert_eq!(machine.cycles(), cycle);
    assert_eq!(machine.harts[0].ps(), ps);
    assert!(machine.core_stalled(1));
}

// ---------------------------------------------------------------------------
// The direct load (P3): what `loader.rs` reproduces, on the shipped image
// ---------------------------------------------------------------------------

use lp_emu_esp32v3::loader::{
    BOOTLOADER_FRAME_CHAIN, BOOTLOADER_SP_AT_APP_ENTRY, DEFAULT_FLASH_SIZE, ROM_DEFAULT_FLASH_SIZE,
    ROM_FLASH_CHIP_SYMBOL,
};
use lp_emu_esp32v3::machine::AppSource;
use lp_emu_esp32v3::test_support::{fw_esp32v3_image, skip_notice};

fn direct(strict: bool) -> Option<Machine> {
    let elf = match fw_esp32v3_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("direct load", &reason);
            return None;
        }
    };
    Some(
        Esp32V3Builder::new()
            .boot_mode(BootMode::Direct)
            .app(AppSource::Path(elf))
            .strict(strict)
            .build()
            .expect("the shipped image builds a machine"),
    )
}

/// `.data` is a self-copy on the classic: `_sidata == _data_start`, so the
/// bytes must already be at their vaddr when `Reset` runs, which is what
/// placing the DRAM `PT_LOAD` does. Verified on the image itself, not on the
/// linker script alone — if the two symbols ever differ the design changes
/// and this is the test that says so.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn data_is_a_self_copy_and_is_placed_at_its_vaddr() {
    let Some(mut machine) = direct(false) else {
        return;
    };
    let app = machine.app().expect("a direct machine has an app");
    let sidata = app.symbol("_sidata").expect("_sidata").address;
    let data_start = app.symbol("_data_start").expect("_data_start").address;
    let data_end = app.symbol("_data_end").expect("_data_end").address;
    assert_eq!(
        sidata, data_start,
        "`.data` is placed > RWDATA with no AT>, so its LMA is its VMA and the app's copy loop \
         moves every word onto itself"
    );
    assert_eq!(data_start, memmap::DRAM_SEG_BASE, "`.data` opens dram_seg");
    assert!(data_end > data_start);

    // And the bytes are there: the segment that carries `.data` was placed
    // by vaddr, with real bytes in it.
    let seg = machine
        .app_segments()
        .iter()
        .find(|s| s.vaddr == data_start)
        .expect("the DRAM PT_LOAD starts at _data_start");
    assert!(seg.filesz > 0);
    let words: Vec<u32> = (0..16)
        .filter_map(|i| machine.peek_word(data_start + 4 * i))
        .collect();
    assert!(
        words.iter().any(|w| *w != 0),
        "`.data` was placed, not left zero"
    );
}

/// The one segment whose `paddr` differs from its `vaddr` is
/// `.rtc_fast.persistent`: NOBITS, linked to RTC fast memory with a load
/// address in the DROM window. Placed by vaddr, and recorded.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn the_only_relocated_segment_is_rtc_fast_persistent() {
    let Some(machine) = direct(false) else {
        return;
    };
    let relocated: Vec<_> = machine
        .app_segments()
        .iter()
        .filter(|s| s.relocated())
        .collect();
    assert_eq!(relocated.len(), 1, "{relocated:?}");
    let seg = relocated[0];
    assert_eq!(seg.vaddr, memmap::RTC_FAST_DBUS);
    assert!(
        memmap::Span {
            name: "drom",
            base: memmap::DROM_BASE,
            len: memmap::DROM_LEN
        }
        .contains(seg.paddr),
        "its load address is in the DROM window: {:#010x}",
        seg.paddr
    );
    assert_eq!(seg.filesz, 0, "NOBITS: nothing to copy either way");
    assert_eq!(seg.regions, vec!["rtc-fast-dbus"]);
    assert_eq!(
        machine.app_segments().len(),
        6,
        "readelf -l: seven headers, one GNU_STACK"
    );
}

/// The flash chip's size goes into the ROM's own chip description, resolved
/// by the ROM's own name for it — `spi_w25q16`, not the `g_rom_flashchip`
/// alias ESP-IDF's linker script provides — and the word it replaces is the
/// ROM's 2 MiB default, proving `.data_spi_flash` was seeded first.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn the_flash_chip_size_is_seeded_over_the_roms_default() {
    let Some(mut machine) = direct(false) else {
        return;
    };
    let seed = machine.flash_seed().expect("a direct load seeds the chip");
    let symbol = machine
        .rom()
        .symbol(ROM_FLASH_CHIP_SYMBOL)
        .expect("the vendored ROM names its chip description");
    assert_eq!(seed.chip, symbol.address);
    assert_eq!(seed.chip, 0x3FFA_E270, "= _data_start_spi_flash");
    assert!(
        machine.rom().symbol("g_rom_flashchip").is_none(),
        "the ROM ELF does NOT carry ESP-IDF's alias; the notes' claim is corrected here"
    );
    assert_eq!(
        seed.previous, ROM_DEFAULT_FLASH_SIZE,
        "2 MiB, the ROM's default"
    );
    assert_eq!(seed.chip_size, DEFAULT_FLASH_SIZE, "4 MiB, the desk board");
    assert_eq!(machine.peek_word(seed.chip + 4), Some(DEFAULT_FLASH_SIZE));
    // The rest of the struct is the ROM's: device_id first.
    assert_eq!(machine.peek_word(seed.chip), Some(0x0015_40EF));
}

/// The hart at entry: the bootloader's `callx8` frame, on the ROM's stack
/// 672 bytes down. `BOOTLOADER_FRAME_CHAIN` is the derivation.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn the_direct_load_enters_the_app_where_the_bootloader_would() {
    let Some(mut machine) = direct(false) else {
        return;
    };
    let app_entry = machine.app().expect("app").entry;
    assert_eq!(machine.harts[0].pc(), app_entry);
    assert_eq!(app_entry, 0x4008_0844, "`Reset` in the shipped image");
    assert_eq!(machine.harts[0].ps(), PS_BOOT);
    let frame = machine.boot_frame().expect("a direct load seeds a frame");
    assert_eq!(frame.sp, BOOTLOADER_SP_AT_APP_ENTRY);
    assert_eq!(machine.harts[0].cpu().a(1), 0x3FFE_3C80);
    let used: u32 = BOOTLOADER_FRAME_CHAIN.iter().map(|(_, _, n)| n).sum();
    assert_eq!(memmap::ROM_PRO_STACK_TOP - used, frame.sp);
    assert!(
        memmap::ROM_PRO_STACK_BASE < frame.sp && frame.sp < memmap::ROM_PRO_STACK_TOP,
        "inside the ROM's PRO stack"
    );
    assert_eq!(machine.peek_word(frame.sp - 12), Some(frame.sp));
}

/// P2's recorded first strict stop of the direct load, held: the loader
/// change moved nothing the boot reads before its first MMIO access.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn the_first_strict_stop_of_the_direct_load_is_dport() {
    let Some(mut machine) = direct(true) else {
        return;
    };
    let outcome = machine.run_until(&StopCondition::after_micros(1_000));
    let Outcome::StrictBus { violation } = outcome else {
        panic!("no peripheral is modelled at this commit: expected a strict stop, got {outcome:?}");
    };
    assert_eq!(
        violation.address,
        memmap::periph::DPORT + 0x218,
        "core_1_intr_map[0]"
    );
    assert_eq!(violation.cycle, 29);
    assert!(
        machine
            .symbolize(violation.pc)
            .is_some_and(|s| s.contains("esp32_init")),
        "{violation:?}"
    );
}
