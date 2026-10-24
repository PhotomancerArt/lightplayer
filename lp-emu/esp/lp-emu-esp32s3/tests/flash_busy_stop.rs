//! **The flash-busy model** ([`Esp32S3Builder::flash_busy`]) on a hand-built
//! fixture: a read through the DROM window while `SPI1` erases a sector is a
//! stop, exit code 9, naming the core, the pc and the address.
//!
//! The fixture is the ROM's own two commands, written straight into `SPI1`
//! from SRAM1 — write enable (`cmd` = `flash_wren`), then a sector erase
//! (`addr` = the sector, `cmd` = `flash_se | flash_pe`, the ROM's
//! `SPI_sector_erase` constant `0x0102_0000`) — then one load from the DROM
//! window:
//!
//! ```text
//! l32r   a2, &SPI1.cmd     ; 0x6000_2000
//! l32r   a3, WREN          ; 0x4000_0000
//! s32i.n a3, a2, 0         ; write enable
//! l32r   a4, &SPI1.addr    ; 0x6000_2004
//! l32r   a3, SECTOR        ; 0x0010_0000
//! s32i.n a3, a4, 0
//! l32r   a3, SE|PE         ; 0x0102_0000
//! s32i.n a3, a2, 0         ; the erase: WIP from here for the part's busy time
//! l32r   a5, DROM          ; 0x3C00_0140
//! l32i.n a6, a5, 0         ; the access the stop names
//! j      .
//! ```
//!
//! The bytes come from `lp_xt_inst::encode`, the repo's own encoder, as in
//! `tests/cache_off_stop.rs`, and the code lives at the same SRAM1 I-bus
//! address for the same reasons.

use lp_emu_esp32s3::machine::{
    BootFrame, BootMode, Esp32S3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32s3::memmap;
use lp_xt_inst::{Inst, Reg};

const LIT: u32 = 0x4037_9000;
const CODE: u32 = 0x4037_9020;
/// A snippet run after the erase has finished: the same load, then a
/// self-jump.
const LATER: u32 = 0x4037_9080;
const DROM_TARGET: u32 = 0x3C00_0140;
const SECTOR: u32 = 0x0010_0000;

const LITERALS: [u32; 6] = [
    memmap::periph::SPI1,
    0x4000_0000,
    memmap::periph::SPI1 + 4,
    SECTOR,
    0x0102_0000,
    DROM_TARGET,
];

fn reg(n: u8) -> Reg {
    Reg::new(n)
}

/// `l32r`'s raw 16-bit field: `(label - ((pc + 3) & !3)) / 4`, backward only.
fn l32r_field(pc: u32, label: u32) -> u16 {
    let base = (pc + 3) & !3;
    (((label as i64 - base as i64) >> 2) as i32 as u32 & 0xffff) as u16
}

fn assemble(start: u32, body: &[&dyn Fn(u32) -> Inst]) -> Vec<(u32, Inst)> {
    let mut at = start;
    let mut out = Vec::new();
    for make in body {
        let inst = make(at);
        let pc = at;
        at += lp_xt_inst::encode(&inst).len() as u32;
        out.push((pc, inst));
    }
    out
}

fn lit(n: u32) -> u32 {
    LIT + 4 * n
}

/// The erase, then the load.
fn erase_then_read() -> Vec<(u32, Inst)> {
    assemble(
        CODE,
        &[
            &|pc| Inst::L32r(reg(2), l32r_field(pc, lit(0))),
            &|pc| Inst::L32r(reg(3), l32r_field(pc, lit(1))),
            &|_| Inst::S32iN(reg(3), reg(2), 0),
            &|pc| Inst::L32r(reg(4), l32r_field(pc, lit(2))),
            &|pc| Inst::L32r(reg(3), l32r_field(pc, lit(3))),
            &|_| Inst::S32iN(reg(3), reg(4), 0),
            &|pc| Inst::L32r(reg(3), l32r_field(pc, lit(4))),
            &|_| Inst::S32iN(reg(3), reg(2), 0),
            &|pc| Inst::L32r(reg(5), l32r_field(pc, lit(5))),
            &|_| Inst::L32iN(reg(6), reg(5), 0),
            &|_| Inst::J(-4),
        ],
    )
}

/// The erase, then a self-jump: nothing touches a window.
fn erase_then_spin() -> Vec<(u32, Inst)> {
    let mut p = erase_then_read();
    p.truncate(8);
    let at = p
        .last()
        .map(|(pc, i)| pc + lp_xt_inst::encode(i).len() as u32)
        .unwrap();
    p.push((at, Inst::J(-4)));
    p
}

/// The load alone, for after the erase.
fn read_later() -> Vec<(u32, Inst)> {
    assemble(
        LATER,
        &[
            &|pc| Inst::L32r(reg(5), l32r_field(pc, lit(5))),
            &|_| Inst::L32iN(reg(6), reg(5), 0),
            &|_| Inst::J(-4),
        ],
    )
}

fn load_pc(program: &[(u32, Inst)]) -> u32 {
    program
        .iter()
        .find(|(_, i)| matches!(i, Inst::L32iN(r, ..) if *r == reg(6)))
        .expect("the fixture loads")
        .0
}

fn fixture(flash_busy: bool, program: &[(u32, Inst)]) -> Machine {
    let mut machine = Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .strict(true)
        .flash_busy(flash_busy)
        .build()
        .expect("a machine with no app still has a bus and a ROM");
    let bus = machine.bus_mut();
    for (i, word) in LITERALS.iter().enumerate() {
        bus.load_image(lit(i as u32), &word.to_le_bytes())
            .expect("SRAM1 holds the literals");
    }
    for (at, inst) in program.iter().chain(read_later().iter()) {
        bus.load_image(*at, &lp_xt_inst::encode(inst))
            .expect("SRAM1 holds the code");
    }
    bus.load_image(DROM_TARGET, &0xC0FF_EE00u32.to_le_bytes())
        .expect("the DROM window takes a host-side write");
    // Both caches on, as the bootloader hands over, so the cache-off stop is
    // not what fires.
    for at in [memmap::periph::EXTMEM + 0x60, memmap::periph::EXTMEM] {
        let word = machine.peek_word(at).unwrap_or(0);
        assert!(machine.poke_word(at, word | lp_emu_esp32s3::cache::CACHE_ENABLE));
    }
    machine
        .seed_boot_state(CODE, BootFrame::at(memmap::ROM_PRO_STACK_TOP))
        .expect("the hart seeds at the fixture");
    machine
}

/// A read through the DROM window during the erase stops the run: exit 9,
/// core 0, the load's pc, the window address.
#[test]
fn a_read_through_a_window_while_the_flash_erases_is_a_stop() {
    let program = erase_then_read();
    let mut m = fixture(true, &program);
    let outcome = m.run_until(&StopCondition::after_micros(1_000));
    let Outcome::FlashBusyFetch {
        core,
        pc,
        addr,
        fetch,
        busy_until,
        cycle,
        ..
    } = outcome.clone()
    else {
        panic!("expected the flash-busy stop, got {outcome:?}");
    };
    assert_eq!(outcome.exit_code(), 9);
    assert_eq!(core, 0);
    assert_eq!(pc, load_pc(&program));
    assert_eq!(addr, DROM_TARGET);
    assert!(!fetch, "a data read");
    assert!(busy_until > cycle, "the erase is still in progress");
    let message = m.flash_busy_message(cycle, core, pc, addr, fetch, busy_until);
    assert!(message.contains("FLASH-BUSY ACCESS  core=0"), "{message}");
}

/// The same fixture with the model off is the machine as it was: the erase
/// completes inside the write and the read is an ordinary read.
#[test]
fn without_the_model_the_same_read_is_ordinary() {
    let mut m = fixture(false, &erase_then_read());
    let outcome = m.run_until(&StopCondition::after_micros(1_000));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "no stop without the model, got {outcome:?}"
    );
}

/// Once the erase's busy time has passed, a read through the window is fine:
/// the model refuses the overlap, not the write.
#[test]
fn a_read_after_the_erase_finishes_is_ordinary() {
    let mut m = fixture(true, &erase_then_spin());
    // The erase (CX1's 21 ms) and margin, spinning in SRAM1.
    let outcome = m.run_until(&StopCondition::after_micros(30_000));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "spinning in SRAM touches no window, got {outcome:?}"
    );
    m.harts[0].set_pc(LATER);
    let outcome = m.run_until(&StopCondition::after_micros(31_000));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "the read after the erase is ordinary, got {outcome:?}"
    );
}
