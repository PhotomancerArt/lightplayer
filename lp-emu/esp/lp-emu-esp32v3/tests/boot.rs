//! The classic machine builds, the ROM places, the seeds land, the hart's
//! reset and boot states are right, a strict run on a bare machine stops
//! somewhere honest — and, since P3, the direct load says its whole hello
//! into an accept block and stands at the flash, the ROM path stands at the
//! eFuse read command, and two runs are the same run.
//!
//! The tests that need the shipped image are `#[ignore]`d and run through
//! `just test-emu-esp32v3-boot`, which builds it and names the file.

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

/// **DD38 acceptance.** Every block from `0x3FF4_0000` up answers at its AHB
/// address as well, and it is the **same state**: a write through one door is
/// read back through the other. The four blocks below that base — `DPORT`
/// itself, `AES`, `RSA`, `SHA` — get no alias, which is why the ROM reaches
/// them only through the DPORT bus (P3 §4.3).
#[test]
fn the_ahb_mirror_is_the_same_block_at_a_second_base() {
    use lp_emu_core::Bus;

    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .build()
        .expect("builds");

    let aliased: Vec<&str> = machine
        .peripheral_alias_map()
        .iter()
        .map(|(n, _, _)| *n)
        .collect();
    assert!(
        !aliased.contains(&"DPORT"),
        "DPORT is at 0x3FF0_0000, below the mirror's low end — it has no AHB address"
    );
    for (name, ahb, _) in machine.peripheral_alias_map() {
        let dport = memmap::ahb_to_dport(*ahb).expect("an alias is inside the AHB window");
        let (_, base, _) = machine
            .peripheral_map()
            .iter()
            .find(|(n, _, _)| n == name)
            .expect("every alias names a registered block");
        assert_eq!(dport, *base, "`{name}`'s alias mirrors its own base");
    }
    // The analog master's only cited address is the AHB one, so it gets no
    // DPORT-side alias: a claim about 0x3FF4_E000 is a claim nothing backs.
    assert!(!aliased.contains(&"I2C_ANA_MST"));

    // One state, two decodes: `UART0.clkdiv` written through DPORT reads back
    // through AHB, and again the other way round.
    let dport = memmap::periph::UART0 + 0x14;
    let ahb = memmap::dport_to_ahb(memmap::periph::UART0).expect("UART0 is mirrored") + 0x14;
    assert_eq!(ahb, 0x6000_0014);
    machine.bus_mut().write_word(dport, 0x0000_2b6a).unwrap();
    assert_eq!(machine.bus_mut().read_word(ahb).unwrap(), 0x0000_2b6a);
    machine.bus_mut().write_word(ahb, 0x0000_0057).unwrap();
    assert_eq!(machine.bus_mut().read_word(dport).unwrap(), 0x0000_0057);

    // The blocks nothing maps stay a strict stop through either door: an
    // alias needs a registered block to point at (`SENS` is P5's).
    let sens_ahb = memmap::dport_to_ahb(memmap::periph::SENS).expect("SENS is mirrored");
    assert!(
        machine
            .peripheral_alias_map()
            .iter()
            .all(|(_, base, len)| sens_ahb < *base || sens_ahb >= base + len)
    );
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

/// **Where the ROM-up path stands at the end of P5.** P3 left it spinning
/// seven instructions after the reset vector, inside the mask ROM's
/// anti-glitch check on its own fuses: `_reload_efuses_and_check` writes
/// `conf` (`+0xfc`) = `0x5aa5` and `cmd` (`+0x104`) = 1, requires `cmd` to
/// read back **1** exactly once — the value is added to a checksum the
/// caller compares against `0xee10101a`, three calls from a seed of
/// `0xee101017` — and then spins until it reads **0**:
///
/// ```text
/// 4000fc9b:  s32i.n  a3, a1, 0          ; conf = 0x5aa5
/// 4000fc9d:  s32i.n  a4, a2, 0          ; cmd = 1 (read_cmd)
/// 4000fca2:  l32i.n  a1, a2, 0
/// 4000fca4:  add.n   a13, a13, a1       ; the checksum
/// 4000fca6:  beqz    a1, _rtc_trigger_sw_system_reset
/// 4000fcac:  l32i.n  a1, a2, 0          ; +0x1c
/// 4000fcae:  bnez    a1, 4000fca9       ; +0x1e, the spin
/// ```
///
/// `crate::periph::efuse` makes the read command a completion, so the check
/// passes, the three reloads compare equal, and the ROM walks on into
/// `main`. P5 left it stopped at the **next** unmodelled block: `uartAttach`
/// (`0x4000_9013`) writing `UART1 +0x10`, which the P3 ledger's §4.3 named
/// in advance.
///
/// **P6 modelled both UARTs, and the ROM walked past `uartAttach` and
/// `Uart_Init`.** Its next strict stop is a block the P3 ledger did *not*
/// predict — §4.3 expected `GPIO.strap` or `spi_flash_attach` next — and
/// that is the reading this test now holds: the ROM's `gpio_pad_unhold`
/// reads **`RTC_IO +0x74` (`dig_pad_hold`)** at `0x4000_a67d`, cycle 7,430.
/// `RTC_IO` is on P5's "not reached by P3 and still unmapped" list; it is
/// reached now, and it is P7's or P8's to answer, not P6's.
#[test]
fn rom_up_walks_past_uart_attach_and_stands_at_rtc_io() {
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .strict(true)
        .build()
        .expect("builds");
    let outcome = machine.run_until(&StopCondition::after_micros(20_000));
    let Outcome::StrictBus { violation } = outcome else {
        panic!("the ROM-up path stops on RTC_IO, not on {outcome:?}");
    };
    assert_eq!(violation.pc, 0x4000_a67d);
    assert_eq!(
        violation.address,
        memmap::periph::RTC_IO + 0x74,
        "RTC_IO.dig_pad_hold, read by the ROM's own pad-unhold routine"
    );
    assert_eq!(violation.cycle, 7_430);
    assert!(
        violation.in_mmio_window,
        "an unmodelled block, not a wild pointer"
    );
    // And `uartAttach`'s two writes are behind it: both UARTs took them.
    assert_eq!(
        machine.peek_word(memmap::periph::UART1 + 0x10),
        Some(0),
        "UART1.int_clr is write-only and reads back zero"
    );

    // The eFuse check is behind it: the read command self-cleared and the
    // opcode the ROM wrote is remembered.
    assert_eq!(
        machine.peek_word(memmap::periph::EFUSE + 0x104),
        Some(0),
        "EFUSE.cmd.read_cmd cleared itself"
    );
    assert_eq!(
        machine.peek_word(memmap::periph::EFUSE + 0x0fc),
        Some(0x5aa5),
        "EFUSE.conf, the read opcode the ROM wrote"
    );
    // And the identity the check reloaded and compared three times over.
    assert_eq!(
        machine.peek_word(memmap::periph::EFUSE + 0x04),
        Some(0xf5ec_f634),
        "blk0_rdata1: MAC[2..6] of the desk board, big-endian"
    );
}

/// The one number the ROM's check pins exactly: `cmd`'s first read must be
/// 1 on each of the three calls, because the caller adds it to a checksum.
#[test]
fn the_efuse_checks_accumulator_lands_on_the_roms_own_constant() {
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .strict(true)
        .build()
        .expect("builds");
    // `_ResetHandler_efuse_check_patch` seeds a13 with 0xee101017 and
    // requires 0xee10101a after three calls (`4000fdef`, `4000fdfe`); the
    // difference is three ones. Reaching `main` at all is the proof —
    // a wrong first read sends the ROM to `_rtc_trigger_sw_system_reset`
    // (`0x4000_FDC7`), which writes RTC_CNTL +0x00 and then `ill.n`.
    let outcome = machine.run_until(&StopCondition::after_micros(20_000));
    assert!(
        matches!(outcome, Outcome::StrictBus { .. }),
        "no fault: the anti-glitch check passed rather than resetting: {outcome:?}"
    );
    let pc = machine.harts[0].pc();
    assert!(
        !(0x4000_FDC7..=0x4000_FDD4).contains(&pc),
        "the ROM took its software-reset path: {pc:#010x}"
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

/// P2's recorded first strict stop of the direct load, held on a **bare**
/// machine: the loader change moved nothing the boot reads before its
/// first MMIO access, and the P3 ledger starts where P2's reading ended.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn the_first_strict_stop_of_a_bare_direct_load_is_dport() {
    let Ok(elf) = fw_esp32v3_image() else {
        skip_notice("bare direct load", "no image");
        return;
    };
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .bare()
        .strict(true)
        .build()
        .expect("builds");
    let outcome = machine.run_until(&StopCondition::after_micros(1_000));
    let Outcome::StrictBus { violation } = outcome else {
        panic!("a bare machine has no peripheral: expected a strict stop, got {outcome:?}");
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

/// A trace sink a test can read back: the bus trace is the one record of
/// what the guest wrote into an accept block byte by byte, because the
/// block itself remembers only the last write.
#[derive(Clone, Default)]
struct SharedSink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for SharedSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedSink {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// The bytes the guest wrote to `UART0.fifo`, in order, read out of a
/// `--trace-block UART0` trace: every `W… UART0+0x000 fifo = 0x…` line.
fn uart0_fifo_bytes(trace: &str) -> Vec<u8> {
    trace
        .lines()
        .filter(|l| l.starts_with("cyc=") && l.contains(" UART0+0x000 fifo = 0x"))
        .filter(|l| l.split_whitespace().any(|w| w.starts_with('W')))
        .filter_map(|l| l.rsplit("= 0x").next())
        .filter_map(|hex| u32::from_str_radix(hex.trim(), 16).ok())
        .map(|v| (v & 0xff) as u8)
        .collect()
}

/// Run the shipped image to `micros` with UART0 traced into a sink, and hand
/// back the machine, the outcome and the trace text.
///
/// `chip` is what is behind SPI1 (P7). **It changes what the boot prints**:
/// with a blank chip there is no partition table at `0x8000`, so the
/// firmware falls back to its memory filesystem; with the merged image there
/// is one, and the mount succeeds. Both are real readings of this machine,
/// and both are pinned below.
fn direct_traced_on(
    micros: u64,
    chip: lp_emu_esp32v3::flash::FlashBacking,
) -> Option<(Machine, Outcome, String)> {
    let elf = match fw_esp32v3_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("direct load, traced", &reason);
            return None;
        }
    };
    let sink = SharedSink::default();
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .flash(chip)
        .strict(true)
        .trace(Box::new(sink.clone()), vec!["UART0".into()])
        .build()
        .expect("builds");
    let outcome = machine.run_until(&StopCondition::after_micros(micros));
    Some((machine, outcome, sink.text()))
}

/// [`direct_traced_on`] with a blank chip.
fn direct_traced(micros: u64) -> Option<(Machine, Outcome, String)> {
    direct_traced_on(micros, lp_emu_esp32v3::flash::FlashBacking::Blank)
}

/// **Where the direct load stands at the end of P7**, and it is one
/// instruction short of the idle heartbeat.
///
/// `rer a9, a8` with `a8 = 0x0010_200C` is `xtensa_lx::is_debugger_attached()`
/// (`xtensa-lx-0.13.0/src/lib.rs:98-104`, whose `XDM_OCD_DCR_SET` that is),
/// reached through `esp_hal::debugger::debugger_connected()` from
/// `CpuControl::start_app_core` — the firmware's attempt to release core 1,
/// which M3 does not grant (Q5) and which the image is entitled to try.
///
/// The instruction is not in `lp-xt-inst`'s `Inst` at all, so it is not a
/// two-line addition to the hart: it needs a decode arm, an encode arm, a
/// disassembly arm, an executor arm and the translator's coverage, in the
/// **Xtensa ISA crate the shader backend also uses**. M3's own concurrency
/// rules (`m3/notes.md` §12) put `lp-xt-emu` and its ISA crate out of this
/// milestone's reach and call a hart change "a finding for the director". So
/// it is pinned here rather than fixed here, and the pin is exact — a
/// different word or a different pc means something else moved.
const RER_PC: u32 = 0x4010_01bd;
/// The word: `rer a9, a8`.
const RER_WORD: u32 = 0x0040_6890;

fn is_the_rer_stop(outcome: &Outcome) -> bool {
    matches!(
        outcome,
        Outcome::Fault {
            pc,
            fault: lp_xt_emu::mach::HartFault::UnsupportedInstruction { word, .. },
            ..
        } if *pc == RER_PC && *word == RER_WORD
    )
}

/// **The flash controller answers, and a blank chip has no partition table.**
///
/// P3 left the direct load spinning here: esp-storage's
/// `esp_rom_spiflash_read_status` wrote `SPI1.cmd.flash_rdsr` (bit 27) and an
/// accept block held the bit for ever —
///
/// ```text
/// 40083861:  s32i    a12, a10, 0        ; SPI1.cmd = 1 << 27
/// 40083864:  memw                       ; +0x38, the spin
/// 40083867:  l32i.n  a8, a10, 0
/// 40083869:  bnez    a8, 40083864
/// ```
///
/// P7's SPI1 view completes the command inside the write, so `cmd` reads 0
/// and the spin ends on its first pass. What the boot finds next is the
/// honest consequence of an **empty** part: no partition table at `0x8000`,
/// so `esp-bootloader-esp-idf`'s lookup fails and the firmware takes its own
/// documented fallback. The merged-image reading is the test below.
///
/// The hello was **P6's**, and it landed: the bytes below left the chip
/// through UART0's shifter at 921,600 baud and are read back off the host
/// stream. `fifo` reads the **receive** side on the read path, and an empty
/// receive FIFO reads zero.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn the_flash_status_spin_ends_and_a_blank_chip_has_no_partition_table() {
    let Some((mut machine, outcome, trace)) = direct_traced(300_000) else {
        return;
    };
    assert!(
        is_the_rer_stop(&outcome),
        "past the flash, as far as `start_app_core`'s `rer`: {outcome:?}"
    );
    assert_eq!(
        machine.peek_word(memmap::periph::SPI1),
        Some(0),
        "SPI1.cmd reads idle: every trigger completed inside its own write"
    );
    let census = machine.flash().lock().expect("flash").command_census();
    assert!(
        census.status_reads > 0,
        "the status read the spin was waiting on really ran: {census}"
    );

    let text = String::from_utf8_lossy(&uart0_fifo_bytes(&trace)).into_owned();
    assert!(
        text.starts_with("[INIT] fw-esp32v3 boot\n"),
        "the first line the boot prints: {text:?}"
    );
    assert!(
        text.contains("partition in the flashed table"),
        "a blank chip has no partition table; the firmware says so:\n{text}"
    );
    assert!(
        !text.contains("flash filesystem mounted"),
        "and does not claim to have mounted one:\n{text}"
    );
    // And the block holds none of it: `fifo` is the receive side on the read
    // path, the host sent nothing, and an empty receive FIFO reads zero.
    // Every byte above left the chip through the shifter at 921,600 baud.
    assert_eq!(
        machine
            .peek_word(memmap::periph::UART0)
            .map(|w| (w & 0xff) as u8),
        Some(0),
        "the view's `fifo` read is a receive pop, not the last thing written"
    );
    assert_eq!(
        machine.uart0().len(),
        text.len(),
        "and what the wire carried is what the guest wrote, byte for byte"
    );
}

/// **P7's acceptance, item 1.** With the merged image in the chip the direct
/// load passes the flash spin, mounts `lpfs` and prints the line P6's gate
/// stopped one short of.
#[test]
#[ignore = "needs the shipped image and espflash; run through `just test-emu-esp32v3-boot`"]
fn the_direct_load_mounts_the_flash_filesystem() {
    let merged = match lp_emu_esp32v3::test_support::merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("the_direct_load_mounts_the_flash_filesystem", &reason);
            return;
        }
    };
    let Some((machine, outcome, trace)) =
        direct_traced_on(300_000, lp_emu_esp32v3::flash::FlashBacking::Copy(merged))
    else {
        return;
    };
    assert!(is_the_rer_stop(&outcome), "{outcome:?}");
    let text = String::from_utf8_lossy(&uart0_fifo_bytes(&trace)).into_owned();
    assert!(
        text.ends_with("[INIT] flash filesystem mounted\n"),
        "the line P6 stopped one short of:\n{text}"
    );
    // The mount really read the part, and the first boot formatted it.
    let census = machine.flash().lock().expect("flash").command_census();
    assert!(census.reads > 50, "{census}");
    assert!(
        census.sector_erases > 0,
        "an empty `lpfs` is formatted on the first boot: {census}"
    );
}

/// The sha256 of the bytes the direct load prints, **pinned** — one golden
/// per chip, because the chip changes what the firmware finds.
///
/// P3 measured a 543-byte chain and printed it in
/// `docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md` §1.1, ending at
/// `[INIT] I/O task spawned`, because there was no flash chip and the boot
/// spun at the door of the mount. P7 gave it one, so the chain continues —
/// and where it goes depends on what is on the part. **Re-measured, not
/// widened**: each golden is a whole byte stream with a sha, and the cause of
/// the change is the line that follows `I/O task spawned` in each.
///
/// A blank chip: 543 bytes plus the `[ERROR] no lpfs partition …` fallback.
///
/// Both are properties of **this image**, not of the machine: the desk board
/// runs a different commit (ruling R7), and a firmware change moves them. A
/// failure means "the boot printed something else", and the text the test
/// prints is what says whether that is a regression or a rebuild.
const INIT_CHAIN_BLANK_SHA256: &str =
    "bfb8d720edc298a3d902860525be2fa05fe701202babeb8e9b0cfee10d2aa40a";
const INIT_CHAIN_BLANK_LEN: usize = 676;

/// The merged image: 543 bytes plus `[INIT] flash filesystem mounted`, and
/// **fewer** bytes than the blank-chip chain because the error line it
/// replaces is longer than the success line.
const INIT_CHAIN_MERGED_SHA256: &str =
    "87fb3c418b9c2755d5e465dca1906e9f1d4ee64c96b53d1121f7fa23bbf3b373";
const INIT_CHAIN_MERGED_LEN: usize = 575;

/// The boot threshold, pinned on both chips.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn the_init_chain_is_the_golden_bytes() {
    use sha2::{Digest, Sha256};

    let Some((_, outcome, trace)) = direct_traced(300_000) else {
        return;
    };
    assert!(is_the_rer_stop(&outcome), "{outcome:?}");
    let bytes = uart0_fifo_bytes(&trace);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    assert_eq!(bytes.len(), INIT_CHAIN_BLANK_LEN, "the chain is:\n{text}");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        INIT_CHAIN_BLANK_SHA256,
        "the chain is:\n{text}"
    );

    let merged = match lp_emu_esp32v3::test_support::merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("the_init_chain_is_the_golden_bytes (merged half)", &reason);
            return;
        }
    };
    let Some((_, outcome, trace)) =
        direct_traced_on(300_000, lp_emu_esp32v3::flash::FlashBacking::Copy(merged))
    else {
        return;
    };
    assert!(is_the_rer_stop(&outcome), "{outcome:?}");
    let bytes = uart0_fifo_bytes(&trace);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    assert_eq!(bytes.len(), INIT_CHAIN_MERGED_LEN, "the chain is:\n{text}");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        INIT_CHAIN_MERGED_SHA256,
        "the chain is:\n{text}"
    );
}

/// **P4's acceptance for the software interrupts.** `swi2` drives the io
/// task's `InterruptExecutor` at Priority2, so it has to actually fire — the
/// phase file's words. It does: the guest writes `cpu_intr_from_cpu2`, the
/// matrix routes source 26, the hart takes the interrupt, and the handler
/// reads `core_0_intr_status0` with bit 26 set and then clears the source.
///
/// The whole path is here rather than in a unit test because the unit test
/// can only prove the matrix reports it; only the boot proves the hart takes
/// it.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn swi2_fires_and_the_handler_sees_it_in_the_status_word() {
    let elf = match fw_esp32v3_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("direct load, DPORT traced", &reason);
            return;
        }
    };
    let sink = SharedSink::default();
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .strict(true)
        .trace(Box::new(sink.clone()), vec!["DPORT".into()])
        .build()
        .expect("builds");
    machine.run_until(&StopCondition::after_micros(20_000));
    let trace = sink.text();

    let raised = trace
        .lines()
        .find(|l| l.contains("W4 DPORT+0x0e4 cpu_intr_from_cpu2 = 0x00000001"))
        .expect("the io task raises swi2");
    let seen = trace
        .lines()
        .find(|l| l.contains("R4 DPORT+0x0ec core_0_intr_status0 = 0x04000000"))
        .unwrap_or_else(|| {
            panic!(
                "the handler must see source 26 (FROM_CPU_INTR2) in the status word; the \
                 raise was:\n{raised}\nthe DPORT trace:\n{trace}"
            )
        });
    let cleared = trace
        .lines()
        .find(|l| l.contains("W4 DPORT+0x0e4 cpu_intr_from_cpu2 = 0x00000000"))
        .expect("and clears it");
    // In that order, and the raise is what the level came from.
    let at = |line: &str| {
        trace
            .lines()
            .position(|l| l == line)
            .expect("a line of this trace")
    };
    assert!(at(raised) < at(seen) && at(seen) < at(cleared));

    // swi0 (esp-rtos) takes the same path, one source lower.
    assert!(
        trace.contains("R4 DPORT+0x0ec core_0_intr_status0 = 0x01000000"),
        "swi0 = FROM_CPU_INTR0 = source 24"
    );
}

/// Determinism, pinned: two runs of the same image with the same flags end
/// at the same cycle and the same pc and wrote the same UART0 bytes.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn two_runs_of_the_direct_load_are_the_same_run() {
    let Some((a, oa, ta)) = direct_traced(16_000) else {
        return;
    };
    let Some((b, ob, tb)) = direct_traced(16_000) else {
        return;
    };
    assert_eq!(oa, ob);
    assert_eq!(a.harts[0].pc(), b.harts[0].pc());
    assert_eq!(a.cycles(), b.cycles());
    assert_eq!(a.instructions(), b.instructions());
    let (ba, bb) = (uart0_fifo_bytes(&ta), uart0_fifo_bytes(&tb));
    assert!(!ba.is_empty(), "the boot printed something");
    assert_eq!(ba, bb, "identical UART bytes");
    assert_eq!(ta, tb, "identical traces, cycle for cycle");
}
