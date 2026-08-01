//! Board-profile tests: the classic-ESP32 (LX6) memory map, emulator-side.
//!
//! The classic map is P1's hardware-measured model (FINDINGS.md, classic
//! C1–C5 section): SRAM1's dual mapping is **word-mirrored** —
//! `iram = 0x400B_FFFC − (dram − 0x3FFE_0000)` — and the plain data RAM
//! (SRAM2) is not executable. These tests pin the emulator to that model;
//! agreement with silicon is P6's job.
//!
//! Payload bytes are assembler-derived golden vectors from FINDINGS.md,
//! never hand-encoded.

use lp_xt_emu::board::BoardProfile;
use lp_xt_emu::emu::{
    CODE_DBUS_BASE, CODE_REGION_LEN, INITIAL_SP, RunOutcome, STACK_DBUS_BASE, STACK_REGION_LEN,
};
use lp_xt_emu::memory::{AliasRule, EXC_INSTR_FETCH_ERROR, Memory};
use lp_xt_emu::{Emulator, TrapKind};

/// GV1 `spike_stub42` (FINDINGS.md): entry a1,32; movi a2,42; retw. f(_) = 42.
/// Ran byte-for-byte unmodified on LX6 in all three regions (C2x/C3).
const GV1: &[u8] = &[0x36, 0x41, 0x00, 0x22, 0xa0, 0x2a, 0x90, 0x00, 0x00];

/// The S3 profile is exactly the legacy constants — `Emulator::new()` is
/// unchanged for every pre-profile consumer.
#[test]
fn s3_profile_matches_legacy_constants() {
    let p = BoardProfile::esp32s3();
    assert_eq!(p.code_dbus_base, CODE_DBUS_BASE);
    assert_eq!(p.code_region_len, CODE_REGION_LEN);
    assert_eq!(p.stack_dbus_base, STACK_DBUS_BASE);
    assert_eq!(p.stack_region_len, STACK_REGION_LEN);
    assert_eq!(p.initial_sp(), INITIAL_SP);
    assert_eq!(p.code_ibus_base(), Memory::ibus_alias(CODE_DBUS_BASE));
    assert_eq!(Emulator::new().profile, p);
}

/// The word-mirror rule reproduces P1's C2b sentinel probes exactly.
///
/// FINDINGS C2b (dram probe base 0x3FFF_0000):
///   off=0x0    h2 @ 0x400A_FFFC   (h1-linear @ 0x400B_0000 read garbage)
///   off=0x100  h2 @ 0x400A_FEFC
#[test]
fn classic_alias_rule_matches_c2b_measurements() {
    let rule = BoardProfile::esp32().alias;
    assert_eq!(rule.dbus_to_ibus(0x3FFF_0000), 0x400A_FFFC);
    assert_eq!(rule.dbus_to_ibus(0x3FFF_0100), 0x400A_FEFC);
    // Byte offsets within a word are preserved (bytes are verbatim, C2b).
    assert_eq!(rule.dbus_to_ibus(0x3FFF_0001), 0x400A_FFFD);
    assert_eq!(rule.dbus_to_ibus(0x3FFF_0003), 0x400A_FFFF);
    // ibus_to_dbus is the exact inverse at byte granularity.
    for dbus in [0x3FFE_8000u32, 0x3FFF_0000, 0x3FFF_0102, 0x3FFF_EFFF] {
        assert_eq!(rule.ibus_to_dbus(rule.dbus_to_ibus(dbus)), dbus);
    }
    // Adjacent I-bus words come from D-bus words walking DOWNWARD.
    assert_eq!(rule.ibus_to_dbus(0x400A_FFFC), 0x3FFF_0000);
    assert_eq!(rule.ibus_to_dbus(0x400B_0000), 0x3FFE_FFFC);
}

/// Data written via the D-bus view reads back at the mirrored I-bus address
/// (H2) and NOT at the linear one (H1) — the emulator has the same *shape*
/// C2b measured, with word contents verbatim little-endian.
#[test]
fn classic_sram1_is_word_mirrored_not_linear() {
    let p = BoardProfile::esp32();
    let mut emu = Emulator::with_profile(p);

    // Two distinct sentinels at two D-bus offsets so the mapping's shape is
    // identifiable, not just one point (the C2b methodology).
    let d0: u32 = 0x3FFF_0000;
    let d1: u32 = 0x3FFF_0100;
    emu.mem.write_u32(d0, 0xC0DE_0000).unwrap();
    emu.mem.write_u32(d1, 0xC0DE_0100).unwrap();

    // H2 (word-mirrored) holds the sentinels...
    assert_eq!(
        emu.mem.read_u32(p.alias.dbus_to_ibus(d0)).unwrap(),
        0xC0DE_0000
    );
    assert_eq!(
        emu.mem.read_u32(p.alias.dbus_to_ibus(d1)).unwrap(),
        0xC0DE_0100
    );
    // ...the H1-linear addresses (0x400A_0000 + (dram − 0x3FFE_0000)) do not.
    assert_ne!(emu.mem.read_u32(0x400B_0000).unwrap(), 0xC0DE_0000);
    assert_ne!(emu.mem.read_u32(0x400B_0100).unwrap(), 0xC0DE_0100);
    // Bytes within the word are verbatim little-endian — no byte swap.
    let i0 = p.alias.dbus_to_ibus(d0);
    let bytes: Vec<u8> = (0..4).map(|k| emu.mem.read_u8(i0 + k).unwrap()).collect();
    assert_eq!(bytes, 0xC0DE_0000u32.to_le_bytes());
}

/// GV1 executes at classic addresses and returns 42 (the emulator-side half
/// of C2x `region=sram1_word_mirrored value=42`).
#[test]
fn classic_profile_runs_gv1() {
    let p = BoardProfile::esp32();
    // The executable image sits inside classic SRAM1's I-bus window.
    assert!((0x400A_0000..0x400C_0000).contains(&p.code_ibus_base()));

    let mut emu = Emulator::with_profile(p);
    match emu.run(GV1, 0, 0) {
        RunOutcome::Ok(v) => assert_eq!(v, 42),
        other => panic!("GV1 on the classic profile must return 42, got {other:?}"),
    }
    // The blob really is I-bus-contiguous: its first word (`entry a1,32` =
    // 0x00004136 plus the first movi byte) reads back verbatim at the I-bus
    // base, and the backing D-bus image sits at the TOP of the code region.
    assert_eq!(
        emu.mem.read_u32(p.code_ibus_base()).unwrap(),
        u32::from_le_bytes([GV1[0], GV1[1], GV1[2], GV1[3]])
    );
    assert_eq!(
        p.alias.ibus_to_dbus(p.code_ibus_base()),
        p.code_dbus_base + p.code_region_len as u32 - 4
    );
}

/// GV1 also runs entered at a nonzero offset within the blob (padding words
/// before the entry cross I-bus word boundaries under the mirror).
#[test]
fn classic_profile_runs_gv1_at_offset() {
    // 4 bytes of padding, then GV1: entry_offset = 4.
    let mut blob = vec![0u8; 4];
    blob.extend_from_slice(GV1);
    let mut emu = Emulator::with_profile(BoardProfile::esp32());
    match emu.run(&blob, 4, 7) {
        RunOutcome::Ok(v) => assert_eq!(v, 42),
        other => panic!("offset GV1 on the classic profile must return 42, got {other:?}"),
    }
}

/// Fetching from classic's plain data RAM (the stack region models SRAM2,
/// which has no I-bus view) faults with EXCCAUSE=2, the emulator-side mirror
/// of C2g (`InstrError` executing at a D-bus address).
#[test]
fn classic_data_ram_is_not_executable() {
    let p = BoardProfile::esp32();
    let emu = Emulator::with_profile(p);
    let mut out = [0u8; 3];
    // The stack region (SRAM2 model) — mapped for data, not fetchable.
    let err = emu.mem.fetch(p.stack_dbus_base, &mut out).unwrap_err();
    assert_eq!(err.kind, TrapKind::Exception);
    assert_eq!(err.cause, EXC_INSTR_FETCH_ERROR);
    // The code region's own D-bus addresses are not fetchable either (C2g
    // executed GV1 at its D-bus address and got EXCCAUSE=2).
    let err = emu.mem.fetch(p.code_dbus_base, &mut out).unwrap_err();
    assert_eq!(err.cause, EXC_INSTR_FETCH_ERROR);
}

/// The S3 default still runs GV1 exactly as before profiles existed.
#[test]
fn s3_default_still_runs_gv1() {
    let mut emu = Emulator::new();
    match emu.run(GV1, 0, 0) {
        RunOutcome::Ok(v) => assert_eq!(v, 42),
        other => panic!("GV1 on the S3 default must return 42, got {other:?}"),
    }
    // Loading via the I-bus base is byte-identical to the historical D-bus
    // write under the S3's offset alias.
    assert_eq!(
        emu.mem.read_u32(CODE_DBUS_BASE).unwrap(),
        u32::from_le_bytes([GV1[0], GV1[1], GV1[2], GV1[3]])
    );
}

/// Both profiles model flash, and none of the five regions collide.
///
/// `install` is what asserts disjointness, so building the emulator is the
/// test; the explicit checks below are about *which* region answers to what,
/// which a silent shadowing bug would get wrong without panicking.
#[test]
fn both_profiles_install_disjoint_flash_and_sram_regions() {
    for p in [BoardProfile::esp32s3(), BoardProfile::esp32()] {
        let emu = Emulator::with_profile(p);

        // Flash is readable and, for the instruction window, fetchable.
        assert!(
            emu.mem.read_u32(p.irom_base).is_ok(),
            "{}: IROM read",
            p.name
        );
        assert!(
            emu.mem.read_u32(p.drom_base).is_ok(),
            "{}: DROM read",
            p.name
        );
        let mut out = [0u8; 3];
        assert!(
            emu.mem.fetch(p.irom_base, &mut out).is_ok(),
            "{}: IROM must be executable at its own address",
            p.name
        );
        assert_eq!(
            emu.mem.fetch(p.drom_base, &mut out).unwrap_err().cause,
            EXC_INSTR_FETCH_ERROR,
            "{}: DROM is data only",
            p.name
        );

        // The image's .data/.bss region is plain SRAM: writable, not fetchable.
        let mut emu = emu;
        assert!(
            emu.mem.write_u32(p.image_data_base, 0x1234).is_ok(),
            "{}: image DRAM write",
            p.name
        );
        assert_eq!(emu.mem.read_u32(p.image_data_base).unwrap(), 0x1234);
        assert_eq!(
            emu.mem
                .fetch(p.image_data_base, &mut out)
                .unwrap_err()
                .cause,
            EXC_INSTR_FETCH_ERROR,
            "{}: image DRAM is not executable",
            p.name
        );

        // No flash address is mistaken for the JIT's code region, in either view.
        assert!(p.code_region_offset(p.irom_base).is_none(), "{}", p.name);
        assert!(p.code_region_offset(p.drom_base).is_none(), "{}", p.name);
        assert!(
            p.code_region_offset(p.code_ibus_base()).is_some(),
            "{}: the code region must recognise its own I-bus base",
            p.name
        );
    }
}

/// The flash bases are the ones the linker script and `lpc-shared`'s backtrace
/// validator already use. Pinned so a profile edit cannot drift away from the
/// image it has to load.
#[test]
fn flash_bases_match_the_documented_chip_maps() {
    let s3 = BoardProfile::esp32s3();
    assert_eq!(s3.irom_base, 0x4200_0000, "S3 irom_seg (esp-hal memory.x)");
    assert_eq!(s3.drom_base, 0x3C00_0000, "S3 drom_seg (esp-hal memory.x)");

    let classic = BoardProfile::esp32();
    assert_eq!(classic.irom_base, 0x400D_0000, "classic irom_seg");
    assert_eq!(classic.drom_base, 0x3F40_0000, "classic drom_seg");

    // Classic's IROM begins above SRAM1's I-bus window rather than inside it.
    assert!(classic.irom_base >= 0x400C_0000);
}

/// Offset and Identity rules stay trivial.
#[test]
fn offset_and_identity_rules() {
    let off = AliasRule::Offset(0x006F_0000);
    assert_eq!(off.dbus_to_ibus(0x3FC8_8000), 0x4037_8000);
    assert_eq!(off.ibus_to_dbus(0x4037_8000), 0x3FC8_8000);
    let id = AliasRule::Identity;
    assert_eq!(id.dbus_to_ibus(0x4008_0400), 0x4008_0400);
    assert_eq!(id.ibus_to_dbus(0x4008_0400), 0x4008_0400);
}
