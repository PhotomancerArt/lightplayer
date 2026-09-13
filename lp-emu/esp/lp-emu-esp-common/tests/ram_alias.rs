//! `SocBus::add_ram_alias` — the bus answers twice (Xtensa plan M6 P02, D2/DD64).
//!
//! One arena, one region, two decodes. Every test here names the half it
//! holds, and each was run against a bus with that half removed (the PR body
//! says which line removed it) so that "passes" means "guards", not "passes
//! either way".
//!
//! The numbers are the ESP32-S3's SRAM1 dual map — D-bus `0x3FC8_8000`,
//! I-bus `0x4037_8000`, `0x6F_0000` apart — because that is the window the
//! capability exists for, and a reader of the S3 crate should recognise them.
//! They are test data: the crate under test holds no chip numbers, and the S3
//! crate registers its own.

use lp_emu_core::bus::{Bus, Watchpoint};
use lp_emu_core::memory::{MemoryAccessKind, MemoryError};
use lp_emu_esp_common::bus::{PERM_NONE, PERM_READ_WRITE, PERMISSION_PAGE_LEN};
use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp_common::{RamRegion, RegFile, SocBus, Trace};

/// The D-bus view: the one region, where the bytes live.
const DBUS: u32 = 0x3FC8_8000;
/// The I-bus view: the alias.
const IBUS: u32 = 0x4037_8000;
const LEN: u32 = 0x1000;

/// A bus with one executable region and the alias over it — the S3's shape.
fn dual_mapped() -> SocBus {
    let mut bus = SocBus::new();
    bus.add_region(RamRegion::new("sram1", DBUS, LEN).executable());
    bus.add_ram_alias(IBUS, LEN, DBUS);
    bus
}

fn traced(bus: &mut SocBus) -> SharedBuffer {
    let sink = SharedBuffer::new();
    bus.trace = Trace::to_sink(Box::new(sink.clone()));
    sink
}

// ---- the whole point -------------------------------------------------------

#[test]
fn a_write_through_one_view_reads_back_through_the_other() {
    let mut bus = dual_mapped();

    bus.write_word(DBUS + 0x10, 0x1234_5678).unwrap();
    assert_eq!(bus.read_word(IBUS + 0x10).unwrap(), 0x1234_5678);

    bus.write_word(IBUS + 0x20, 0x0bad_c0de_u32 as i32).unwrap();
    assert_eq!(bus.read_word(DBUS + 0x20).unwrap(), 0x0bad_c0de_u32 as i32);

    // Every width, both directions, and the bytes are the same bytes.
    bus.write_byte(IBUS + 0x30, 0x5a).unwrap();
    bus.write_halfword(DBUS + 0x32, 0x1234).unwrap();
    assert_eq!(bus.read_u8(DBUS + 0x30).unwrap(), 0x5a);
    assert_eq!(bus.read_halfword(IBUS + 0x32).unwrap(), 0x1234);
    assert_eq!(bus.read_word(IBUS + 0x30).unwrap() as u32, 0x1234_005a);

    // And the arena holds them once, at the canonical offset.
    let off = (DBUS + 0x10 - bus.guest_arena_base()) as usize;
    assert_eq!(
        &bus.guest_arena()[off..off + 4],
        &0x1234_5678u32.to_le_bytes()
    );
}

/// **The fetch path.** The S3's JIT stores through the D-bus view and
/// fetches at `write + 0x6F_0000`; a translation that covered loads and
/// stores but not fetch would boot the firmware and fault on the first
/// shader with no diagnostic. Both fetch shapes, because the bus serves two
/// instruction sets.
#[test]
fn a_fetch_through_the_alias_executes_bytes_written_through_the_target() {
    let mut bus = dual_mapped();

    // The word-assembling (RV32) fetch.
    bus.write_word(DBUS + 0x100, 0x0000_0013).unwrap(); // nop
    assert_eq!(bus.fetch_instruction(IBUS + 0x100).unwrap(), 0x0000_0013);

    // The byte-granular (Xtensa) fetch, at every alignment.
    let code: Vec<u8> = (0..8u8).map(|i| 0xc0 | i).collect();
    bus.load_image(DBUS + 0x200, &code).unwrap();
    for k in 0..4u32 {
        let mut out = [0u8; 3];
        assert_eq!(bus.fetch_bytes(IBUS + 0x200 + k, &mut out).unwrap(), 3);
        assert_eq!(
            out,
            [0xc0 | k as u8, 0xc0 | (k + 1) as u8, 0xc0 | (k + 2) as u8]
        );
    }

    // And the other direction: written through the alias, fetched through
    // the target — the same bytes, because there is only one set of them.
    bus.write_word(IBUS + 0x300, 0x0000_4501).unwrap();
    assert_eq!(bus.fetch_instruction(DBUS + 0x300).unwrap(), 0x0000_4501);
}

/// The fetch through the alias is bounded by the *region's* end, translated,
/// not by the arena's and not by the alias window's own arithmetic.
#[test]
fn a_fetch_through_the_alias_is_bounded_by_the_target_region() {
    let mut bus = dual_mapped();
    bus.load_image(DBUS + LEN - 2, &[0x01, 0x02]).unwrap();
    let mut out = [0xffu8; 3];
    assert_eq!(bus.fetch_bytes(IBUS + LEN - 2, &mut out).unwrap(), 2);
    assert_eq!(&out[..2], &[0x01, 0x02]);
    assert_eq!(out[2], 0xff, "nothing past the count");
    assert!(bus.fetch_bytes(IBUS + LEN, &mut out).is_err());
    assert!(bus.fetch_instruction(IBUS + LEN).is_err());
}

// ---- no second store --------------------------------------------------------

#[test]
fn the_region_list_still_has_one_region() {
    let bus = dual_mapped();
    assert_eq!(bus.regions().len(), 1);
    assert_eq!(bus.region_spans(), vec![(DBUS, LEN, true)]);
    assert_eq!(bus.ram_aliases(), vec![(IBUS, LEN, DBUS)]);
    // The arena covers the region and nothing more: an alias is not bytes.
    assert_eq!(bus.guest_arena_base(), DBUS);
    assert_eq!(bus.guest_arena().len(), LEN as usize);
}

/// The alias window is a gap in the arena as far as a translated core is
/// concerned: not plain RAM, so an inline access there goes out through the
/// bus and is translated like any other (the canonical-address rule).
#[test]
fn alias_pages_are_not_plain_ram_in_the_permission_table() {
    let mut bus = SocBus::new();
    bus.reserve_guest_span(0x4000_0000, 4 * PERMISSION_PAGE_LEN);
    bus.add_region(RamRegion::new("ram", 0x4000_0000, PERMISSION_PAGE_LEN));
    let alias_base = 0x4000_0000 + 2 * PERMISSION_PAGE_LEN;
    bus.add_ram_alias(alias_base, PERMISSION_PAGE_LEN, 0x4000_0000);
    let table = bus.permission_table();
    assert_eq!(table[0], PERM_READ_WRITE, "the region's own page");
    assert_eq!(table[2], PERM_NONE, "the alias window is not plain RAM");
    assert_eq!(table[3], PERM_NONE);
    // And the bus serves it all the same.
    bus.write_word(alias_base + 4, 7).unwrap();
    assert_eq!(bus.read_word(0x4000_0004).unwrap(), 7);
}

// ---- canonicalisation is before everything else ---------------------------

#[test]
fn a_watchpoint_on_the_canonical_address_fires_on_an_alias_write() {
    let mut bus = dual_mapped();
    bus.set_watchpoint(
        0,
        Some(Watchpoint {
            address: DBUS + 0x40,
            napot: false,
            on_store: true,
            on_load: false,
            on_execute: false,
        }),
    );
    let err = bus.write_word(IBUS + 0x40, 1).unwrap_err();
    assert!(
        matches!(
            err,
            MemoryError::Watchpoint {
                address,
                kind: MemoryAccessKind::Write,
                slot: 0,
            } if address == DBUS + 0x40
        ),
        "{err:?}"
    );
    // Before the access: the bytes are unchanged through either door.
    assert_eq!(bus.read_word(DBUS + 0x40).unwrap(), 0);

    // The same for a load and for a fetch trigger.
    bus.set_watchpoint(
        1,
        Some(Watchpoint {
            address: DBUS + 0x44,
            napot: false,
            on_store: false,
            on_load: true,
            on_execute: true,
        }),
    );
    assert!(matches!(
        bus.read_word(IBUS + 0x44).unwrap_err(),
        MemoryError::Watchpoint { slot: 1, .. }
    ));
    assert!(matches!(
        bus.fetch_instruction(IBUS + 0x44).unwrap_err(),
        MemoryError::Watchpoint { slot: 1, .. }
    ));
    let mut out = [0u8; 3];
    assert!(matches!(
        bus.fetch_bytes(IBUS + 0x44, &mut out).unwrap_err(),
        MemoryError::Watchpoint { slot: 1, .. }
    ));
}

/// The rule's stated consequence: a trigger armed at an alias address is
/// armed at an address no access reaches the check with.
#[test]
fn a_watchpoint_armed_at_the_alias_address_never_fires() {
    let mut bus = dual_mapped();
    bus.set_watchpoint(
        0,
        Some(Watchpoint {
            address: IBUS + 0x40,
            napot: false,
            on_store: true,
            on_load: true,
            on_execute: true,
        }),
    );
    bus.write_word(IBUS + 0x40, 1).unwrap();
    bus.write_word(DBUS + 0x40, 2).unwrap();
    assert_eq!(bus.read_word(IBUS + 0x40).unwrap(), 2);
    assert!(bus.fetch_instruction(IBUS + 0x40).is_ok());
}

/// M7's inheritance: a translator seeds its retranslation from the spans the
/// bus records, and those name the arena's own address — never the door.
#[test]
fn guest_code_writes_report_canonical_addresses() {
    let mut bus = dual_mapped();
    bus.watch_guest_code(true);
    // A code copy through the I-bus view, the way the S3's JIT would not do
    // it (it writes D-bus) but the way a rule that leaked the alias address
    // would be caught.
    for k in 0..4u32 {
        bus.write_word(IBUS + 0x400 + 4 * k, 0x0000_0013 | ((k as i32 + 1) << 8))
            .unwrap();
    }
    let spans = bus.take_guest_code_writes();
    assert_eq!(
        spans.len(),
        1,
        "one contiguous copy is one span: {spans:x?}"
    );
    let (lo, hi) = spans[0];
    assert_eq!(lo, DBUS + 0x400, "the span's base is canonical");
    assert_eq!(hi, DBUS + 0x410);
    assert!(
        (lo..hi).all(|a| a < IBUS || a >= IBUS + LEN),
        "no alias address leaks into the record"
    );

    // The host-side funnel says the same thing.
    bus.load_image(IBUS + 0x800, &[1, 2, 3, 4]).unwrap();
    assert_eq!(bus.take_code_writes(), vec![(DBUS + 0x800, DBUS + 0x804)]);
    assert_eq!(bus.read_word(DBUS + 0x800).unwrap() as u32, 0x0403_0201);
}

#[test]
fn a_fault_through_the_alias_names_the_canonical_address() {
    let mut bus = SocBus::new();
    bus.add_region(RamRegion::new("sram1", DBUS, LEN).executable().word_only());
    bus.add_ram_alias(IBUS, LEN, DBUS);
    // A refused width, raised downstream of the translation.
    let err = bus.read_halfword(IBUS + 0x10).unwrap_err();
    assert!(
        matches!(err, MemoryError::InvalidAccess { address, .. } if address == DBUS + 0x10),
        "{err:?}"
    );
    // A straddle of the region's end.
    let err = bus.write_word(IBUS + LEN - 2, 0).unwrap_err();
    assert!(
        matches!(err, MemoryError::InvalidAccess { address, .. } if address == DBUS + LEN - 2),
        "{err:?}"
    );
}

// ---- build-time bugs --------------------------------------------------------

#[test]
#[should_panic(expected = "not inside any one registered region")]
fn an_alias_over_an_unregistered_target_panics() {
    let mut bus = SocBus::new();
    bus.add_region(RamRegion::new("sram1", DBUS, LEN));
    bus.add_ram_alias(IBUS, LEN, 0x5000_0000);
}

#[test]
#[should_panic(expected = "not inside any one registered region")]
fn an_alias_whose_target_straddles_two_regions_panics() {
    let mut bus = SocBus::new();
    bus.add_region(RamRegion::new("a", DBUS, LEN));
    bus.add_region(RamRegion::new("b", DBUS + LEN, LEN));
    // Adjacent regions, one arena, still two regions: an alias is a view of
    // one of them.
    bus.add_ram_alias(IBUS, LEN, DBUS + LEN / 2);
}

#[test]
#[should_panic(expected = "overlaps the RAM alias")]
fn two_overlapping_aliases_panic() {
    let mut bus = dual_mapped();
    bus.add_ram_alias(IBUS + LEN / 2, LEN, DBUS);
}

#[test]
#[should_panic(expected = "overlaps region")]
fn an_alias_over_a_region_panics() {
    let mut bus = SocBus::new();
    bus.add_region(RamRegion::new("sram1", DBUS, LEN));
    bus.add_region(RamRegion::new("other", IBUS, LEN));
    bus.add_ram_alias(IBUS + 0x10, LEN, DBUS);
}

#[test]
#[should_panic(expected = "overlaps its own target")]
fn an_alias_over_its_own_target_panics() {
    let mut bus = SocBus::new();
    bus.add_region(RamRegion::new("sram1", DBUS, LEN));
    bus.add_ram_alias(DBUS + 0x100, 0x100, DBUS + 0x180);
}

#[test]
#[should_panic(expected = "RAM never answers at an MMIO address")]
fn an_alias_over_a_peripheral_panics() {
    let mut bus = SocBus::new();
    bus.add_region(RamRegion::new("sram1", DBUS, LEN));
    bus.add_peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)));
    bus.add_ram_alias(0x6000_0080, LEN, DBUS);
}

#[test]
#[should_panic(expected = "overlaps the MMIO window")]
fn an_alias_over_an_mmio_window_panics() {
    let mut bus = SocBus::new();
    bus.add_region(RamRegion::new("sram1", DBUS, LEN));
    bus.add_mmio_window(0x6000_0000, 0x10_0000);
    bus.add_ram_alias(0x6000_8000, LEN, DBUS);
}

#[test]
#[should_panic(expected = "overlaps the RAM alias")]
fn a_region_added_over_an_alias_panics() {
    let mut bus = dual_mapped();
    bus.add_region(RamRegion::new("late", IBUS + 0x800, 0x100));
}

#[test]
#[should_panic(expected = "overlaps the RAM alias")]
fn a_peripheral_added_over_an_alias_panics() {
    let mut bus = dual_mapped();
    bus.add_peripheral(IBUS + 0x800, 0x100, Box::new(RegFile::new("LATE", 0x100)));
}

#[test]
#[should_panic(expected = "has no length")]
fn an_empty_alias_panics() {
    let mut bus = SocBus::new();
    bus.add_region(RamRegion::new("sram1", DBUS, LEN));
    bus.add_ram_alias(IBUS, 0, DBUS);
}

// ---- the trace names the door ----------------------------------------------

#[test]
fn the_trace_names_the_door_once_per_site() {
    let mut bus = dual_mapped();
    let sink = traced(&mut bus);
    bus.set_time(41);
    bus.set_pc(0x4200_1000);

    // Two stores through the alias from one pc: one line.
    bus.write_word(IBUS + 0x10, 1).unwrap();
    bus.write_word(IBUS + 0x14, 2).unwrap();
    // A store through the target: no line — that is the canonical door.
    bus.write_word(DBUS + 0x18, 3).unwrap();
    // A load from the same pc is a different site, and so is a fetch.
    bus.read_word(IBUS + 0x10).unwrap();
    bus.set_time(42);
    bus.set_pc(IBUS + 0x10);
    bus.fetch_instruction(IBUS + 0x10).unwrap();
    bus.fetch_instruction(IBUS + 0x10).unwrap();
    // The host placing bytes is not the guest and writes no line.
    bus.load_image(IBUS + 0x20, &[0; 4]).unwrap();

    assert_eq!(
        sink.lines(),
        [
            "cyc=41 pc=0x42001000 ALIAS store 0x40378010 -> 0x3fc88010",
            "cyc=41 pc=0x42001000 ALIAS load 0x40378010 -> 0x3fc88010",
            "cyc=42 pc=0x40378010 ALIAS fetch 0x40378010 -> 0x3fc88010",
        ]
    );
}

#[test]
fn an_untraced_bus_writes_nothing_and_remembers_nothing() {
    let mut bus = dual_mapped();
    bus.write_word(IBUS + 0x10, 1).unwrap();
    bus.fetch_instruction(IBUS + 0x10).unwrap();
    assert!(bus.save_scalars().alias_sites.is_empty());
}

// ---- the snapshot carries the alias table ----------------------------------

/// A snapshot taken mid-run and restored into a fresh machine of the same
/// build reproduces the same bytes through both doors **and the same trace**
/// from that point on — which is what proves the alias sites came along.
#[test]
fn a_snapshot_taken_mid_run_restores_into_a_fresh_machine_with_the_same_trace() {
    let mut a = dual_mapped();
    let sink_a = traced(&mut a);
    a.set_time(10);
    a.set_pc(0x4200_0000);
    a.write_word(IBUS + 0x10, 0x1234_5678).unwrap(); // names the store door
    a.set_pc(0x4200_0004);
    a.read_word(IBUS + 0x10).unwrap(); // names the load door
    let lines_before = sink_a.lines().len();
    assert_eq!(lines_before, 2);

    let regions = a.save_regions();
    let periph = a.save_peripherals();
    let scalars = a.save_scalars();
    assert_eq!(scalars.ram_aliases, vec![(IBUS, LEN, DBUS)]);
    assert_eq!(scalars.alias_sites.len(), 2);

    let mut b = dual_mapped();
    let sink_b = traced(&mut b);
    b.restore_regions(&regions);
    b.restore_peripherals(&periph);
    b.restore_scalars(&scalars);

    // The bytes, through both doors.
    assert_eq!(b.read_word(DBUS + 0x10).unwrap(), 0x1234_5678);
    assert_eq!(b.read_word(IBUS + 0x10).unwrap(), 0x1234_5678);

    // The same script on both from here: the sites already named stay
    // silent on both, and a new site speaks on both.
    for bus in [&mut a, &mut b] {
        bus.set_time(20);
        bus.set_pc(0x4200_0000);
        bus.write_word(IBUS + 0x14, 9).unwrap(); // already named on A
        bus.set_pc(0x4200_0008);
        bus.fetch_instruction(IBUS + 0x14).unwrap(); // new on both
    }
    let tail_a = sink_a.lines()[lines_before..].to_vec();
    assert_eq!(
        tail_a,
        ["cyc=20 pc=0x42000008 ALIAS fetch 0x40378014 -> 0x3fc88014"]
    );
    assert_eq!(
        sink_b.lines(),
        tail_a,
        "the restored machine writes A's trace"
    );
    assert_eq!(a.save_scalars(), b.save_scalars());
}

#[test]
#[should_panic(expected = "snapshot has RAM aliases")]
fn a_snapshot_from_an_aliased_machine_is_refused_by_an_unaliased_one() {
    let a = dual_mapped();
    let scalars = a.save_scalars();
    let mut b = SocBus::new();
    b.add_region(RamRegion::new("sram1", DBUS, LEN).executable());
    b.restore_scalars(&scalars);
}

// ---- the C6 / classic guarantee, in-crate ----------------------------------

/// Every chip today registers no alias. A bus with none must behave, byte for
/// byte and line for line, as it did before the capability existed — and the
/// nearest thing this crate can compare against is a bus whose alias sits in
/// a window nothing touches: the same script must leave both identical in
/// every observable but the alias table itself.
#[test]
fn a_bus_with_no_alias_is_byte_identical() {
    fn build(alias: bool) -> (SocBus, SharedBuffer) {
        let mut bus = SocBus::new();
        bus.reserve_guest_span(0x3FC8_8000, 0x0100_0000);
        bus.add_region(RamRegion::new("sram1", DBUS, LEN).executable());
        bus.add_region(
            RamRegion::new("rom", 0x4000_0000, 0x100)
                .read_only()
                .executable(),
        );
        bus.add_peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)));
        bus.add_mmio_window(0x6000_0000, 0x10_0000);
        if alias {
            bus.add_ram_alias(IBUS, LEN, DBUS);
        }
        let sink = traced(&mut bus);
        (bus, sink)
    }
    fn script(bus: &mut SocBus) {
        bus.set_time(1);
        bus.set_pc(0x4000_0000);
        bus.write_word(DBUS + 0x10, 0x1234_5678).unwrap();
        bus.write_byte(DBUS + 0x12, 0x11).unwrap();
        assert_eq!(bus.read_word(DBUS + 0x10).unwrap() as u32, 0x1211_5678);
        assert!(bus.fetch_instruction(DBUS + 0x10).is_ok());
        let mut out = [0u8; 3];
        assert!(bus.fetch_bytes(DBUS + 0x11, &mut out).is_ok());
        assert!(bus.write_word(0x4000_0000, 1).is_err());
        bus.write_word(0x6000_0004, 0x77).unwrap();
        assert_eq!(bus.read_word(0x6000_0004).unwrap(), 0x77);
        assert_eq!(bus.read_word(0x7000_0000).unwrap(), 0); // unmapped
        assert!(bus.fetch_instruction(0x6000_0000).is_err());
        bus.load_image(0x4000_0000, &[1, 2, 3, 4]).unwrap();
        assert_eq!(bus.fetch_instruction(0x4000_0000).unwrap(), 0x0403_0201);
    }

    let (mut plain, sink_plain) = build(false);
    let (mut aliased, sink_aliased) = build(true);
    assert!(plain.ram_aliases().is_empty());
    script(&mut plain);
    script(&mut aliased);

    assert_eq!(plain.guest_arena(), aliased.guest_arena(), "the bytes");
    assert_eq!(sink_plain.lines(), sink_aliased.lines(), "the trace");
    assert_eq!(plain.permission_table(), aliased.permission_table());
    assert_eq!(plain.take_code_writes(), aliased.take_code_writes());
    assert_eq!(plain.unmapped_reads(), aliased.unmapped_reads());
    assert_eq!(plain.unmapped_sites(), aliased.unmapped_sites());
    assert_eq!(plain.save_regions(), aliased.save_regions());
    let (mut sp, sa) = (plain.save_scalars(), aliased.save_scalars());
    assert_eq!(sp.ram_aliases, Vec::new());
    assert_eq!(sa.ram_aliases, vec![(IBUS, LEN, DBUS)]);
    sp.ram_aliases = sa.ram_aliases.clone();
    assert_eq!(sp, sa, "every scalar but the table itself");
}
