//! P07's register gate: **every** register in `regs::RMT` answers, at the
//! PAC's reset value, and the whole window is mapped.
//!
//! The unit tests in `src/periph/rmt.rs` are per-behaviour and per-delta —
//! the by-event interrupt grouping, `sys_conf`'s divider, the two `mem_size`
//! widths, the two status layouts. This file makes the other claim, the one
//! a per-behaviour test cannot: that **nothing in the block is missing**. It
//! walks the generated table rather than a list written here, so a register
//! the PAC has and this view forgot fails without anyone remembering to add
//! an assertion.
//!
//! It needs no firmware image and is **not** `#[ignore]`d: the block is a
//! register file with engines on it, and a bare `cargo test --workspace`
//! should run it.

use lp_emu_esp_common::{Peripheral, Sandbox, Width};
use lp_emu_esp32s3::memmap;
use lp_emu_esp32s3::periph::rmt::{
    BLOCK_WORDS, CONFIG, LEN, RAM_OFFSET, RAM_WORDS, REGS_LEN, RX_CH_BASE, RX_CHANNELS,
    TX_CHANNELS, new,
};
use lp_emu_esp32s3::regs;

/// Registers the block answers **live** rather than out of its file, so a
/// reset-value walk must not expect the table's word from them: the two
/// status arrays (computed from the engines) and `int_raw` / `int_st` /
/// `int_clr` (the sticky word and its mask).
fn is_live(off: u32) -> bool {
    off == CONFIG.int_raw
        || off == CONFIG.int_st
        || off == CONFIG.int_clr
        || CONFIG.ch_tx_status.contains(&off)
        || CONFIG.ch_rx_status.contains(&off)
}

/// Every register the generated table names is inside the aperture, has a
/// name back, and reads what the PAC resets it to.
#[test]
fn every_register_in_the_table_answers_at_its_pac_reset() {
    let mut sb = Sandbox::new();
    let mut r = new();
    assert!(!regs::RMT.is_empty(), "the generated table is not empty");
    for (off, name) in regs::RMT.entries {
        assert!(
            *off < REGS_LEN,
            "{name} at +0x{off:03x} is outside the register aperture 0x{REGS_LEN:03x}"
        );
        assert_eq!(r.reg_name(*off), Some(*name));
        if is_live(*off) {
            continue;
        }
        let want = regs::RMT.reset(*off).unwrap_or(0);
        assert_eq!(
            sb.read(&mut r, *off),
            want,
            "{name} at +0x{off:03x} must read its PAC reset"
        );
    }
}

/// The register file remembers what a guest writes, everywhere the block
/// does not give the register behaviour — which is what "accept-and-remember
/// at the PAC's reset value" means when it is checked rather than asserted.
#[test]
fn the_accept_registers_read_back_what_was_written() {
    let mut sb = Sandbox::new();
    let mut r = new();
    // `ch0carrier_duty` … `ch3carrier_duty`, `tx_sim` and `date` are the
    // block's `modeled` registers with no behaviour at all.
    for off in [0x080, 0x084, 0x088, 0x08c, 0x0c4, 0x0cc] {
        let name = regs::RMT.name(off).expect("a named register");
        sb.write(&mut r, off, 0x1234_5678);
        assert_eq!(sb.read(&mut r, off), 0x1234_5678, "{name}");
    }
}

/// The aperture is registers, the gap and the RAM — and the gap is neither.
///
/// ⚠️ The C6's RAM offset (`+0x400`) is in the gap on this chip. A write
/// there is dropped with a note rather than stored, so a run that carried
/// the C6's constant over transmits nothing rather than transmitting the
/// tail of the register file.
#[test]
fn the_window_is_the_registers_the_gap_and_three_hundred_and_eighty_four_words() {
    assert_eq!(RAM_OFFSET, 0x800, "re-derived from rmt.ram_start");
    assert_eq!(RAM_WORDS, 384);
    assert_eq!(BLOCK_WORDS, 48);
    assert_eq!(
        RAM_WORDS as u32,
        BLOCK_WORDS * (TX_CHANNELS + RX_CHANNELS) as u32
    );
    assert_eq!(LEN, RAM_OFFSET + 4 * RAM_WORDS as u32);
    assert_eq!(LEN, 0xe00);
    // …and the whole of it is one mapped window on the bus.
    assert_eq!(memmap::periph::RMT_RAM, memmap::periph::RMT + RAM_OFFSET);

    let mut sb = Sandbox::new();
    let mut r = new();
    for word in 0..RAM_WORDS as u32 {
        sb.write(&mut r, RAM_OFFSET + 4 * word, 0xa5a5_0000 | word);
    }
    for word in 0..RAM_WORDS as u32 {
        assert_eq!(sb.read(&mut r, RAM_OFFSET + 4 * word), 0xa5a5_0000 | word);
    }
    // The gap between `date` (+0x0cc) and the RAM.
    for off in [REGS_LEN, 0x400, RAM_OFFSET - 4] {
        sb.write(&mut r, off, 0xffff_ffff);
        assert_eq!(sb.read(&mut r, off), 0, "+0x{off:03x} is the gap");
    }
    assert!(r.ram().iter().all(|w| *w & 0xffff_0000 == 0xa5a5_0000));
}

/// Byte and half-word lanes reach the register file and the RAM, because the
/// bus hands the block the lane and not a word.
#[test]
fn a_byte_lane_reaches_a_register_and_a_ram_word() {
    let mut sb = Sandbox::new();
    let mut r = new();
    let date = 0x0cc;
    sb.write(&mut r, date, 0);
    r.write(date + 1, Width::Byte, 0xab, &mut sb.cx());
    assert_eq!(sb.read(&mut r, date), 0x0000_ab00);
    sb.write(&mut r, RAM_OFFSET, 0);
    r.write(RAM_OFFSET + 2, Width::Half, 0x1234, &mut sb.cx());
    assert_eq!(sb.read(&mut r, RAM_OFFSET), 0x1234_0000);
    assert_eq!(r.read(RAM_OFFSET + 2, Width::Half, &mut sb.cx()), 0x1234);
}

/// The channel counts and the two indexing schemes the PAC uses, asserted
/// against the table's own names rather than against this file's prose.
///
/// The configuration registers carry the **absolute** channel number
/// (`ch4_rx_conf0`) and the status, limit and carrier registers carry the
/// **receiver index** (`ch0_rx_status` is channel 4's). Getting those two the
/// wrong way round names the wrong register in a trace and passes every
/// behaviour test.
#[test]
fn the_two_channel_numbering_schemes_are_the_pacs() {
    let r = new();
    assert_eq!((TX_CHANNELS, RX_CHANNELS, RX_CH_BASE), (4, 4, 4));
    for ch in 0..TX_CHANNELS {
        assert_eq!(
            r.reg_name(CONFIG.ch_tx_conf0[ch]),
            Some(format!("ch{ch}_tx_conf0").as_str())
        );
        assert_eq!(
            r.reg_name(CONFIG.ch_tx_status[ch]),
            Some(format!("ch{ch}_tx_status").as_str())
        );
        assert_eq!(
            r.reg_name(CONFIG.ch_tx_lim[ch]),
            Some(format!("ch{ch}_tx_lim").as_str())
        );
    }
    for rxi in 0..RX_CHANNELS {
        let absolute = RX_CH_BASE + rxi;
        assert_eq!(
            r.reg_name(CONFIG.ch_rx_conf0[rxi]),
            Some(format!("ch{absolute}_rx_conf0").as_str()),
            "the configuration registers carry the absolute channel number"
        );
        assert_eq!(
            r.reg_name(CONFIG.ch_rx_conf1[rxi]),
            Some(format!("ch{absolute}_rx_conf1").as_str())
        );
        assert_eq!(
            r.reg_name(CONFIG.ch_rx_status[rxi]),
            Some(format!("ch{rxi}_rx_status").as_str()),
            "…and the status registers carry the receiver index"
        );
        assert_eq!(
            r.reg_name(CONFIG.ch_rx_lim[rxi]),
            Some(format!("ch{rxi}_rx_lim").as_str())
        );
        assert_eq!(
            r.reg_name(CONFIG.ch_rx_carrier_rm[rxi]),
            Some(format!("ch{rxi}_rx_carrier_rm").as_str())
        );
    }
}

/// Grades: `documented` where the PAC's bit map is read out loud, `modeled`
/// otherwise, and **nothing `measured`** — a waveform is not a register's bit
/// map, and no S3 silicon has been read at all.
#[test]
fn nothing_in_this_block_is_measured() {
    use lp_emu_esp_common::periph::RegGrade;
    let r = new();
    let mut documented = 0usize;
    for (off, name) in regs::RMT.entries {
        let grade = r.reg_grade(*off).expect("every register is graded");
        assert_ne!(grade, RegGrade::Measured, "{name} at +0x{off:03x}");
        if grade == RegGrade::Documented {
            documented += 1;
        }
    }
    // The four TX channels' three registers each, the four receivers' four
    // each, the four interrupt words and `sys_conf`.
    assert_eq!(documented, 3 * 4 + 4 * 4 + 4 + 1);
}
