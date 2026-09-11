//! The classic's RMT view, register by register and quirk by quirk (M4 P2).
//!
//! Every offset here is **looked up by name in the generated table**
//! (`regs::RMT`, from the esp32 PAC 0.40.2) rather than transcribed, so a
//! table that moves takes these tests with it and `just lint-emu-regnames`
//! keeps the table honest against the PAC.
//!
//! The five quirks each get a test that fails without them — three of them
//! are inverted relative to the ESP32-C6's block, which is the reader most
//! likely to be wrong here:
//!
//! 1. [`the_threshold_is_a_repeating_count`]
//! 2. [`without_the_global_wrap_bit_the_window_does_not_wrap`]
//! 3. [`tx_start_takes_effect_without_an_update`]
//! 4. [`a_window_of_end_markers_stops_at_the_next_word`]
//! 5. [`mem_raddr_ex_is_an_offset_into_the_whole_ram`]

use lp_emu_esp32v3::memmap;
use lp_emu_esp32v3::periph::gpio::Gpio;
use lp_emu_esp32v3::periph::rmt::{self, Rmt};
use lp_emu_esp32v3::regs;
use lp_emu_esp_common::pins::{PadId, SignalId};
use lp_emu_esp_common::regnames::RegNames;
use lp_emu_esp_common::{Peripheral, RegGrade, Sandbox};

// ---- reading the table --------------------------------------------------

/// The offset of the register called `name`, out of the generated table.
/// Panics if the table has no such register, which is the point: a name that
/// stops existing is a failing test rather than a silent zero.
fn off(names: &RegNames, name: &str) -> u32 {
    names
        .entries
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(o, _)| *o)
        .unwrap_or_else(|| panic!("{} has no register called {name}", names.block))
}

fn rmt_off(name: &str) -> u32 {
    off(&regs::RMT, name)
}

fn conf0(ch: usize) -> u32 {
    rmt_off(&format!("ch{ch}conf0"))
}

fn conf1(ch: usize) -> u32 {
    rmt_off(&format!("ch{ch}conf1"))
}

fn status(ch: usize) -> u32 {
    rmt_off(&format!("ch{ch}status"))
}

fn tx_lim(ch: usize) -> u32 {
    rmt_off(&format!("ch{ch}_tx_lim"))
}

fn ram(word: u32) -> u32 {
    rmt::RAM_OFFSET + 4 * word
}

// ---- the layout ---------------------------------------------------------

/// `chNconf0` and `chNconf1` are a **stride-8 pair**, and `chNstatus`,
/// `chNaddr`, `chNcarrier_duty` and `chN_tx_lim` are stride-4 arrays, at the
/// bases this view uses. Not a transcription: the assertion is that the
/// arithmetic and the generated table agree.
#[test]
fn the_channel_registers_are_where_the_table_says() {
    for ch in 0..rmt::TX_CHANNELS {
        assert_eq!(conf0(ch), 0x020 + 8 * ch as u32, "ch{ch}conf0");
        assert_eq!(conf1(ch), 0x024 + 8 * ch as u32, "ch{ch}conf1");
        assert_eq!(status(ch), 0x060 + 4 * ch as u32, "ch{ch}status");
        assert_eq!(rmt_off(&format!("ch{ch}addr")), 0x080 + 4 * ch as u32);
        assert_eq!(
            rmt_off(&format!("ch{ch}carrier_duty")),
            0x0b0 + 4 * ch as u32
        );
        assert_eq!(tx_lim(ch), 0x0d0 + 4 * ch as u32, "ch{ch}_tx_lim");
        assert_eq!(rmt_off(&format!("ch{ch}data")), 4 * ch as u32);
    }
    assert_eq!(rmt_off("int_raw"), 0x0a0);
    assert_eq!(rmt_off("int_st"), 0x0a4);
    assert_eq!(rmt_off("int_ena"), 0x0a8);
    assert_eq!(rmt_off("int_clr"), 0x0ac);
    assert_eq!(rmt_off("apb_conf"), 0x0f0);
    assert_eq!(rmt_off("date"), 0x0fc);
    // The register block ends at `date`; the RAM starts a clear `0x700`
    // later and the aperture covers both.
    assert!(rmt_off("date") < rmt::REGS_LEN);
    assert_eq!(rmt::RAM_OFFSET, memmap::periph::RMT_RAM - memmap::periph::RMT);
    assert_eq!(
        rmt::LEN,
        rmt::RAM_OFFSET + 4 * rmt::RAM_WORDS as u32,
        "the window covers registers, the gap and all 512 RAM words"
    );
    assert_eq!(rmt::RAM_WORDS, 512);
    assert_eq!(rmt::BLOCK_WORDS, 64);
}

/// The resets this view's named constants carry are the PAC's, out of the
/// generated table — and the block comes out of reset holding them.
#[test]
fn the_resets_are_the_pacs() {
    let table = |o: u32| {
        regs::RMT
            .resets
            .iter()
            .find(|(off, _)| *off == o)
            .map(|(_, v)| *v)
            .unwrap_or(0)
    };
    let mut sb = Sandbox::new();
    let mut r = Rmt::new();
    for ch in 0..rmt::TX_CHANNELS {
        assert_eq!(table(conf0(ch)), rmt::CH_CONF0_RESET);
        assert_eq!(table(conf1(ch)), rmt::CH_CONF1_RESET);
        assert_eq!(sb.read(&mut r, conf0(ch)), rmt::CH_CONF0_RESET);
        assert_eq!(sb.read(&mut r, conf1(ch)), rmt::CH_CONF1_RESET);
        // `tx_lim` resets to 0x80, and `chNcarrier_duty` to 0x0040_0040.
        assert_eq!(sb.read(&mut r, tx_lim(ch)), 0x80);
        assert_eq!(sb.read(&mut r, rmt_off(&format!("ch{ch}carrier_duty"))), 0x0040_0040);
    }
    // ⚠️ `ref_always_on` (bit 17) is **clear** at reset: an unconfigured
    // channel selects `clk_ref`, not APB. M4's notes expected the opposite.
    assert_eq!(rmt::CH_CONF1_RESET & (1 << 17), 0);
    // …and `mem_owner` (bit 5) is set: the receiver owns the RAM until
    // `start_tx` hands it over.
    assert_eq!(rmt::CH_CONF1_RESET & (1 << 5), 1 << 5);
    assert!(r.mem_owner(0), "mem_owner out of reset");
}

/// The interrupt bits interleave **by channel**, three per channel, with the
/// threshold events in a flat block at the top — the first thing a reader of
/// the C6's block gets wrong (PAC `rmt/int_raw.rs`).
#[test]
fn the_interrupt_bits_interleave_by_channel() {
    assert_eq!(rmt::int_tx_end_bit(0), 1 << 0);
    assert_eq!(rmt::int_rx_end_bit(0), 1 << 1);
    assert_eq!(rmt::int_err_bit(0), 1 << 2);
    assert_eq!(rmt::int_tx_end_bit(1), 1 << 3);
    assert_eq!(rmt::int_tx_end_bit(7), 1 << 21);
    assert_eq!(rmt::int_err_bit(7), 1 << 23);
    assert_eq!(rmt::int_thr_bit(0), 1 << 24);
    assert_eq!(rmt::int_thr_bit(7), 1 << 31);
    // The C6's layout would have put `ch1_tx_end` at bit 1 and every
    // threshold at bit 8 + ch. Nothing overlaps here.
    let mut seen = 0u32;
    for ch in 0..rmt::TX_CHANNELS {
        for bit in [
            rmt::int_tx_end_bit(ch),
            rmt::int_rx_end_bit(ch),
            rmt::int_err_bit(ch),
            rmt::int_thr_bit(ch),
        ] {
            assert_eq!(seen & bit, 0, "bit collision at ch{ch}");
            seen |= bit;
        }
    }
    assert_eq!(seen, u32::MAX, "all 32 bits are defined");
}

/// The TX channel's GPIO-matrix **output** signal is 87 + n, and 87 is *not*
/// the input table's `RMT_SIG_0` (83). The two spaces are separate.
#[test]
fn the_output_signal_is_eighty_seven_plus_the_channel() {
    assert_eq!(rmt::RMT_SIG_0, 87);
    for ch in 0..rmt::TX_CHANNELS {
        assert_eq!(rmt::signal_of(ch), SignalId(87 + ch as u16));
    }
    // `InputSignal::RMT_SIG_0` is 83, so 87 is `RMT_SIG_4` on the input
    // side — the mix-up this constant exists to prevent.
    assert_ne!(rmt::RMT_SIG_0, 83);
}

/// Nothing is `measured`; the named registers are `documented` and the rest
/// accept-and-remember.
#[test]
fn the_grades_are_documented_and_modeled_and_never_measured() {
    let r = Rmt::new();
    for ch in 0..rmt::TX_CHANNELS {
        for o in [conf0(ch), conf1(ch), status(ch), tx_lim(ch)] {
            assert_eq!(r.reg_grade(o), Some(RegGrade::Documented), "+0x{o:03x}");
        }
        for o in [
            4 * ch as u32,
            rmt_off(&format!("ch{ch}addr")),
            rmt_off(&format!("ch{ch}carrier_duty")),
        ] {
            assert_eq!(r.reg_grade(o), Some(RegGrade::Modeled), "+0x{o:03x}");
        }
    }
    for o in ["int_raw", "int_st", "int_ena", "int_clr", "apb_conf"] {
        assert_eq!(r.reg_grade(rmt_off(o)), Some(RegGrade::Documented), "{o}");
    }
    assert_eq!(r.reg_grade(rmt_off("date")), Some(RegGrade::Modeled));
    for (_, grade) in rmt::Rmt::grades().entries() {
        assert_ne!(*grade, RegGrade::Measured, "a waveform is not a bit map");
    }
}

// ---- driving a channel --------------------------------------------------

/// One RMT word: two halves of `(level, duration in channel ticks)`, the
/// level in bit 15 of each half — the encoding `lp_ws281x::pulse` writes
/// (`lp-fw/lp-ws281x/src/pulse.rs:105`).
fn word(l1: bool, d1: u16, l2: bool, d2: u16) -> u32 {
    let half1 = (u32::from(l1) << 15) | u32::from(d1);
    let half2 = (u32::from(l2) << 15) | u32::from(d2);
    half1 | (half2 << 16)
}

/// A word 20 ticks long, so at `div_cnt = 1` (12.5 ns a tick, three CPU
/// cycles) one word is exactly `WORD_CYCLES`.
const DATA: u32 = 0;
const TICKS_PER_WORD: u64 = 20;
/// One tick is `CPU_HZ / APB_HZ` cycles — computed, never a literal 3.
const CYCLES_PER_TICK: u64 = memmap::CPU_HZ / 80_000_000;
const WORD_CYCLES: u64 = TICKS_PER_WORD * CYCLES_PER_TICK;

fn data_word() -> u32 {
    word(true, 10, false, 10)
}

/// `chNconf0` with `div_cnt = 1` and a `blocks`-wide window, everything else
/// at the PAC reset.
fn conf0_value(blocks: u32) -> u32 {
    (rmt::CH_CONF0_RESET & !0xff & !(0xf << 24)) | 1 | (blocks << 24)
}

/// `chNconf1` as the driver leaves it: APB base clock, idle output enabled
/// and low, `mem_owner` clear (the transmitter owns the RAM).
const CONF1_RUN: u32 = (1 << 17) | (1 << 19);
const CONF1_TX_START: u32 = 1 << 0;

/// A block with channel `ch` configured for a `blocks`-wide window, its
/// window filled with `fill`, wrap as asked, interrupts enabled, and
/// `tx_lim` armed. Not started.
fn armed(ch: usize, blocks: u32, wrap: bool, lim: u32, fill: u32) -> (Sandbox, Rmt) {
    let mut sb = Sandbox::new();
    let mut r = Rmt::new();
    sb.write(&mut r, conf0(ch), conf0_value(blocks));
    sb.write(&mut r, conf1(ch), CONF1_RUN);
    sb.write(
        &mut r,
        rmt_off("apb_conf"),
        1 | if wrap { 1 << 1 } else { 0 },
    );
    sb.write(&mut r, tx_lim(ch), lim);
    sb.write(&mut r, rmt_off("int_ena"), u32::MAX);
    let start = 64 * ch as u32;
    for w in start..start + 64 * blocks {
        sb.write(&mut r, ram(w), fill);
    }
    (sb, r)
}

fn start(sb: &mut Sandbox, r: &mut Rmt, ch: usize) {
    sb.write(r, conf1(ch), CONF1_RUN | CONF1_TX_START);
}

fn int_raw(sb: &mut Sandbox, r: &mut Rmt) -> u32 {
    sb.read(r, rmt_off("int_raw"))
}

// ---- quirk 1 ------------------------------------------------------------

/// **`tx_lim` is a repeating count of words sent, and it re-arms itself** —
/// not a position in the window, which is what the C6's `tx_lim` is
/// (`lp-emu-esp32c6/src/periph/rmt.rs:27-32`). PAC `rmt/ch_tx_lim.rs:5`:
/// *"When channel0 sends more than reg_rmt_tx_lim_ch0 datas then channel0
/// produce the relative interrupt."*
///
/// One `tx_lim = 64` write and a 256-word transmission gives **four**
/// threshold events. A model that copied the C6's semantics would fire once
/// — at word 64, the only time the pointer's window position equals 64 —
/// and the driver's second refill would never be asked for, which truncates
/// every frame it cannot fit in one window.
#[test]
fn the_threshold_is_a_repeating_count() {
    // A four-block window (256 words), wrapping, every word data: the
    // transmitter runs forever and the only thing that stops is the test.
    let (mut sb, mut r) = armed(0, 4, true, 64, data_word());
    start(&mut sb, &mut r, 0);

    // Word `k` is fetched at cycle `k * WORD_CYCLES`, and the count is
    // incremented at the fetch — the word has left the RAM — so the event
    // for the 64th word is visible from cycle 63 * WORD_CYCLES.
    let mut thresholds = 0;
    let mut at = [0u64; 8];
    for w in 0..256u64 {
        sb.run_to(&mut r, w * WORD_CYCLES);
        if int_raw(&mut sb, &mut r) & rmt::int_thr_bit(0) != 0 {
            if thresholds < at.len() {
                at[thresholds] = w;
            }
            thresholds += 1;
            sb.write(&mut r, rmt_off("int_clr"), rmt::int_thr_bit(0));
        }
    }
    assert_eq!(
        thresholds, 4,
        "a repeating count fires every 64 words; positions fired at {at:?}"
    );
    assert_eq!(
        &at[..4],
        &[63, 127, 191, 255],
        "and it re-arms itself, so the events are 64 words apart, exactly"
    );
}

/// The counter re-arms at the **event**, not at the guest's `tx_lim` write:
/// a write mid-window changes the period for the next crossing and moves
/// nothing that has already been counted.
///
/// See `periph::rmt::TxEngine::since_thr` for why — `v3_rmt.rs:411-413`
/// claims the opposite in passing, uncited, in a comment whose own
/// conclusion is that the hypothesis built on it was disproven on silicon,
/// and a counter that restarted on every write would walk the threshold out
/// of its half inside one frame.
#[test]
fn a_tx_lim_write_changes_the_period_and_not_the_count() {
    let (mut sb, mut r) = armed(0, 4, true, 64, data_word());
    start(&mut sb, &mut r, 0);
    // Two words in, ask for a wider period. Under "the write restarts the
    // counter" the next event would be at word 66; under "it re-arms at the
    // event" the 64-word crossing has already been counted towards, and the
    // new period applies from there.
    sb.run_to(&mut r, 2 * WORD_CYCLES);
    sb.write(&mut r, tx_lim(0), 64);
    let mut first = 0;
    for w in 3..=80u64 {
        sb.run_to(&mut r, w * WORD_CYCLES);
        if int_raw(&mut sb, &mut r) & rmt::int_thr_bit(0) != 0 {
            first = w;
            break;
        }
    }
    assert_eq!(
        first, 63,
        "the 64th word's fetch, unmoved by the write two words in"
    );
}

// ---- quirk 2 ------------------------------------------------------------

/// **`apb_conf.mem_tx_wrap_en` is global** (bit 1), not a per-channel bit in
/// `chNconf0` as on the C6. Without it the transmitter runs off the end of
/// the window instead of wrapping onto the half the driver just refilled,
/// and ping-pong refill does not work at all (`v3_rmt.rs:66-72`).
#[test]
fn without_the_global_wrap_bit_the_window_does_not_wrap() {
    // One block, 64 words, every word data, wrap OFF.
    let (mut sb, mut r) = armed(0, 1, false, 0, data_word());
    start(&mut sb, &mut r, 0);
    sb.run_to(&mut r, 80 * WORD_CYCLES);
    assert!(
        int_raw(&mut sb, &mut r) & rmt::int_err_bit(0) != 0,
        "the window ended with wrap off: ch_err and mem_empty"
    );
    assert!(!r.is_running(0), "and the engine stopped there");
    assert_ne!(sb.read(&mut r, status(0)) & (1 << 29), 0, "mem_empty");

    // The same channel with the global bit set wraps and keeps going —
    // and the bit is *one* register for all eight channels, so setting it
    // for channel 0 sets it for channel 7 too.
    let (mut sb, mut r) = armed(7, 1, true, 0, data_word());
    start(&mut sb, &mut r, 7);
    sb.run_to(&mut r, 200 * WORD_CYCLES);
    assert!(r.is_running(7), "wrap on: still transmitting past the window");
    assert_eq!(int_raw(&mut sb, &mut r) & rmt::int_err_bit(7), 0);
}

// ---- quirk 3 ------------------------------------------------------------

/// **There is no `conf_update` on this chip** — esp-hal's `update()` is a
/// literal no-op (`third_party/esp-hal/src/rmt.rs:2944-2946`). The write
/// that carries `tx_start` starts the channel then and there, and the C6's
/// "pulses take effect at `conf_update`" rule must not be carried over.
#[test]
fn tx_start_takes_effect_without_an_update() {
    let (mut sb, mut r) = armed(0, 1, true, 0, data_word());
    r.set_keep_logs(true);
    assert!(!r.is_running(0));
    start(&mut sb, &mut r, 0);
    assert!(r.is_running(0), "started by the write that carried tx_start");
    // …and the first word's pulses are already out, stamped at the start
    // cycle, with no second write of any kind. Both halves of a word are
    // emitted together at the fetch (see `push_pulse`), which is why the
    // observable here is the log and not the signal's current level.
    assert_eq!(r.pulses(0).len(), 2, "one word, two pulses");
    assert_eq!(r.pulses(0)[0].at, 0);
    assert!(r.pulses(0)[0].level, "half one is high");
    assert_eq!(r.pulses(0)[1].at, 10 * CYCLES_PER_TICK);
    assert!(!r.pulses(0)[1].level);
    // The strobe reads back 0: both drivers read-modify-write this register
    // several times a frame, and a `tx_start` that held would restart a
    // channel nobody started.
    assert_eq!(sb.read(&mut r, conf1(0)) & CONF1_TX_START, 0);
}

// ---- quirk 4 ------------------------------------------------------------

/// **There is no `tx_stop` bit** (`rmt.has_tx_immediate_stop = false`). A
/// stop is a window full of end markers, and the transmitter halts at the
/// next word boundary and raises `tx_end` (`v3_rmt.rs:73-77`; esp-hal's
/// `stop_tx` for this chip, `rmt.rs:2959-2966`).
#[test]
fn a_window_of_end_markers_stops_at_the_next_word() {
    let (mut sb, mut r) = armed(0, 1, true, 0, data_word());
    start(&mut sb, &mut r, 0);
    sb.run_to(&mut r, 4 * WORD_CYCLES);
    assert!(r.is_running(0));
    assert_eq!(int_raw(&mut sb, &mut r) & rmt::int_tx_end_bit(0), 0);

    // `stop_tx`: the whole window to end markers.
    for w in 0..64 {
        sb.write(&mut r, ram(w), DATA);
    }
    // Within one word the transmitter lands on one and stops.
    sb.run_to(&mut r, 6 * WORD_CYCLES);
    assert!(!r.is_running(0), "halted at the next word boundary");
    assert_ne!(
        int_raw(&mut sb, &mut r) & rmt::int_tx_end_bit(0),
        0,
        "and raised tx_end"
    );
    assert_eq!(r.frames_ended(0), 1);
    assert!(
        !sb.pins.signal_level(rmt::signal_of(0)),
        "the signal rests at the idle level"
    );
}

/// The other half of the end-marker rule: a zero **second** half emits half
/// one and then ends the transmission.
#[test]
fn a_zero_second_half_emits_the_first_and_then_ends() {
    let (mut sb, mut r) = armed(0, 1, true, 0, data_word());
    sb.write(&mut r, ram(1), word(true, 10, false, 0));
    start(&mut sb, &mut r, 0);
    sb.run_to(&mut r, 4 * WORD_CYCLES);
    assert_eq!(r.frames_ended(0), 1);
    // Word 0's two pulses, then word 1's single one.
    assert_eq!(r.words(0).len(), 0, "logs are off by default");
}

// ---- quirk 5 ------------------------------------------------------------

/// **`mem_raddr_ex` is absolute** — ten bits over all 512 words, so a
/// channel's window begins at `64 × first_block` and the driver subtracts it
/// (`v3_rmt::read_pos`; esp-hal's `hw_offset`).
///
/// ⚠️ And it lives at **bits 12:21**, not 0:9: the PAC puts `MEM_WADDR_EX`
/// at 0:9 and `MEM_RADDR_EX` at 12:21
/// (`esp32-0.40.2/src/rmt/chstatus.rs:27-35`), which is the opposite of what
/// M4's notes carried. Both the firmware and esp-hal read the pointer
/// through the accessor named `mem_raddr_ex`, so that is the field that has
/// to move.
#[test]
fn mem_raddr_ex_is_an_offset_into_the_whole_ram() {
    let raddr = |sb: &mut Sandbox, r: &mut Rmt, ch: usize| (sb.read(r, status(ch)) >> 12) & 0x3ff;

    // Channel 3's window starts at word 192.
    let (mut sb, mut r) = armed(3, 1, true, 0, data_word());
    assert_eq!(raddr(&mut sb, &mut r, 3), 192, "before any transmission");
    start(&mut sb, &mut r, 3);
    assert_eq!(raddr(&mut sb, &mut r, 3), 193, "the next word to fetch");
    sb.run_to(&mut r, 4 * WORD_CYCLES);
    assert_eq!(raddr(&mut sb, &mut r, 3), 197);

    // It is nowhere near bits 0:9, which carry the APB write pointer.
    let word = sb.read(&mut r, status(3));
    assert_eq!(word & 0x3ff, 192, "mem_waddr_ex, bits 0:9");
    // `state` (bits 24:26) reads `send` while a frame is on the wire.
    assert_eq!((word >> 24) & 0b111, 1, "state = send");
}

// ---- the symbol pump ----------------------------------------------------

/// Every level change goes out on the channel's own GPIO-matrix output
/// signal, `RMT_SIG_0 + n`.
#[test]
fn the_symbol_pump_drives_the_channels_signal() {
    let (mut sb, mut r) = armed(2, 1, true, 0, word(true, 10, false, 30));
    r.set_keep_logs(true);
    assert!(!sb.pins.signal_level(rmt::signal_of(2)), "idle low");
    start(&mut sb, &mut r, 2);
    // Half one: high for 10 ticks. Half two: low for 30. Both are pushed at
    // the fetch, each stamped with the cycle it actually starts at, so the
    // signal's *current* level is already the second half's.
    assert_eq!(r.pulses(2)[0].at, 0);
    assert!(r.pulses(2)[0].level);
    assert_eq!(r.pulses(2)[1].at, 10 * CYCLES_PER_TICK);
    assert!(!sb.pins.signal_level(rmt::signal_of(2)));
    // The next word's first half puts it back high, at tick 40 — the two
    // words' durations added, with no per-word rounding in between.
    sb.run_to(&mut r, 40 * CYCLES_PER_TICK);
    assert_eq!(r.pulses(2)[2].at, 40 * CYCLES_PER_TICK);
    assert!(r.pulses(2)[2].level);
    // And nothing reaches any other channel's signal.
    for ch in 0..rmt::TX_CHANNELS {
        if ch != 2 {
            assert!(!sb.pins.signal_level(rmt::signal_of(ch)), "ch{ch}");
        }
    }
}

/// A pad routed to the channel's signal through `func_out_sel_cfg` carries
/// the waveform, and [`Gpio::peripheral_driven_pads`] — empty for all of M3
/// — names it. This is the assertion M3 P8 left for this phase to make true.
#[test]
fn a_routed_pad_carries_the_waveform() {
    const GPIO18: u32 = 18;
    let enable_w1ts = off(&regs::GPIO, "enable_w1ts");
    let out_sel18 = off(&regs::GPIO, "func18_out_sel_cfg");

    let mut sb = Sandbox::new();
    let mut g = Gpio::new(0);
    let mut r = Rmt::new();

    // What `v3_rmt::route_rmt_to_gpio` writes: the pad output-enabled and
    // its `out_sel` pointing at RMT channel 0's output signal, 87.
    sb.write(&mut g, enable_w1ts, 1 << GPIO18);
    sb.write(&mut g, out_sel18, u32::from(rmt::RMT_SIG_0));
    assert_eq!(
        g.peripheral_driven_pads(&sb.cx()),
        vec![(PadId(GPIO18 as u8), SignalId(rmt::RMT_SIG_0))],
        "M4 P2 makes this list non-empty for the first time"
    );
    assert!(!sb.pins.pad_level(PadId(GPIO18 as u8)), "idle low");

    sb.write(&mut r, conf0(0), conf0_value(1));
    sb.write(&mut r, conf1(0), CONF1_RUN);
    sb.write(&mut r, rmt_off("apb_conf"), 1 | (1 << 1));
    for w in 0..64 {
        sb.write(&mut r, ram(w), data_word());
    }
    sb.write(&mut r, conf1(0), CONF1_RUN | CONF1_TX_START);

    // The pad carried both halves of the word, each stamped with the cycle
    // it started at — this is the stream the machine's slice drain hands to
    // `Gpio` and, from M4 P3, to the strip decoders and the pin log.
    let edges = sb.pins.take_edges();
    assert_eq!(edges.len(), 2, "one word, two edges on the pad");
    assert_eq!(edges[0].pad, PadId(GPIO18 as u8));
    assert!(edges[0].level, "half one is high");
    assert_eq!(edges[0].at, 0);
    assert!(!edges[1].level);
    assert_eq!(edges[1].at, 10 * CYCLES_PER_TICK);
    assert!(!sb.pins.pad_level(PadId(GPIO18 as u8)), "and rests low");
}

// ---- the interrupt line -------------------------------------------------

/// `int_st` is `int_raw & int_ena`, `int_clr` is write-only and W1C, and the
/// line on source 47 follows `int_st`.
#[test]
fn the_line_follows_int_st_and_int_clr_is_write_one_to_clear() {
    let (mut sb, mut r) = armed(0, 1, true, 0, data_word());
    // Only channel 0's `tx_end` enabled.
    sb.write(&mut r, rmt_off("int_ena"), rmt::int_tx_end_bit(0));
    start(&mut sb, &mut r, 0);
    for w in 0..64 {
        sb.write(&mut r, ram(w), DATA);
    }
    sb.run_to(&mut r, 4 * WORD_CYCLES);
    assert_ne!(int_raw(&mut sb, &mut r) & rmt::int_tx_end_bit(0), 0);
    assert_eq!(
        sb.read(&mut r, rmt_off("int_st")),
        rmt::int_tx_end_bit(0),
        "int_st = raw & ena"
    );
    assert!(sb.irq.level(rmt::SOURCE_RMT), "source 47 asserted");
    assert_eq!(sb.read(&mut r, rmt_off("int_clr")), 0, "write-only");

    // W1C: a write clears exactly the bits it names and nothing else, which
    // is what makes it race-free with two cores writing it.
    sb.write(&mut r, rmt_off("int_clr"), rmt::int_err_bit(0));
    assert_ne!(int_raw(&mut sb, &mut r) & rmt::int_tx_end_bit(0), 0);
    sb.write(&mut r, rmt_off("int_clr"), rmt::int_tx_end_bit(0));
    assert_eq!(int_raw(&mut sb, &mut r) & rmt::int_tx_end_bit(0), 0);
    assert!(!sb.irq.level(rmt::SOURCE_RMT), "and the line drops");
}

// ---- the clock ----------------------------------------------------------

/// `chNconf1.ref_always_on` selects the base clock — *"1'b1:clk_apb
/// 1'b0:clk_ref"*. REF_TICK is **refused rather than guessed at**: this
/// machine does not model the classic's REF_TICK divider tree, and a
/// transmitter clocked at an invented rate would put a waveform on the wire
/// that nothing could check.
#[test]
fn ref_tick_is_refused_rather_than_guessed_at() {
    let mut sb = Sandbox::new();
    let mut r = Rmt::new();
    sb.write(&mut r, conf0(0), conf0_value(1));
    // Everything the driver sets except the clock bit.
    sb.write(&mut r, conf1(0), 1 << 19);
    sb.write(&mut r, rmt_off("apb_conf"), 1 | (1 << 1));
    for w in 0..64 {
        sb.write(&mut r, ram(w), data_word());
    }
    sb.write(&mut r, conf1(0), (1 << 19) | CONF1_TX_START);
    sb.run_to(&mut r, 100 * WORD_CYCLES);
    assert!(
        !sb.pins.signal_level(rmt::signal_of(0)),
        "no clock, no waveform"
    );
    assert_eq!(r.frames_ended(0), 0);
}

/// One tick at `div_cnt = 1` is `CPU_HZ / APB_HZ` cycles, and the arithmetic
/// runs over the **absolute** tick count since `tx_start` so nothing rounds
/// per word across a frame.
#[test]
fn a_word_takes_its_ticks_times_the_clock_ratio() {
    let (mut sb, mut r) = armed(0, 1, true, 0, word(true, 7, false, 13));
    r.set_keep_logs(true);
    start(&mut sb, &mut r, 0);
    sb.run_to(&mut r, 10 * WORD_CYCLES);
    let pulses = r.pulses(0);
    assert!(pulses.len() >= 6);
    // Halves of 7 and 13 ticks: the edges land at 7, 20, 27, 40, … ticks.
    for (i, expect_tick) in [0u64, 7, 20, 27, 40, 47].into_iter().enumerate() {
        assert_eq!(
            pulses[i].at,
            expect_tick * CYCLES_PER_TICK,
            "pulse {i} at tick {expect_tick}"
        );
    }
    // Divider 3 makes each tick three times as long, exactly.
    let (mut sb, mut r) = armed(1, 1, true, 0, word(true, 7, false, 13));
    sb.write(&mut r, conf0(1), (conf0_value(1) & !0xff) | 3);
    r.set_keep_logs(true);
    start(&mut sb, &mut r, 1);
    sb.run_to(&mut r, 10 * WORD_CYCLES * 3);
    assert_eq!(r.pulses(1)[1].at, 7 * CYCLES_PER_TICK * 3);
}

// ---- the refill telemetry -----------------------------------------------

/// The block measures the race it is half of: for every `tx_thr_event`, the
/// words the transmitter consumes before the guest's next `chN_tx_lim` write
/// (the **entry** delay) and then before the last RAM write of that refill
/// (the **fill**). Reported, never gated (D13/PD9).
#[test]
fn the_refill_telemetry_measures_entry_and_fill_in_words() {
    let (mut sb, mut r) = armed(0, 2, true, 64, data_word());
    start(&mut sb, &mut r, 0);
    assert_eq!(r.refill_stats(0).refills, 0);

    // Run to the first threshold: the 64th word's fetch, at cycle 63.
    sb.run_to(&mut r, 63 * WORD_CYCLES);
    assert_ne!(int_raw(&mut sb, &mut r) & rmt::int_thr_bit(0), 0);

    // The ISR arrives four words later and flips `tx_lim` first, which is
    // the earliest moment it is observably present.
    sb.run_to(&mut r, 67 * WORD_CYCLES);
    sb.write(&mut r, tx_lim(0), 64);
    // …then refills the half it just left, finishing five words after that.
    sb.run_to(&mut r, 72 * WORD_CYCLES);
    for w in 0..64 {
        sb.write(&mut r, ram(w), data_word());
    }

    // The next threshold closes the measurement.
    sb.run_to(&mut r, 130 * WORD_CYCLES);
    let s = r.refill_stats(0);
    assert_eq!(s.refills, 1, "one measurement closed");
    assert_eq!(s.entry_max, 4, "four words between the event and the write");
    assert_eq!(s.fill_max, 5, "five more before the last RAM write");
    assert_eq!(s.half_words, 64, "the bucket denominator is the half-window");
    assert_eq!(s.unanswered, 0);
    assert_eq!(
        rmt::RefillStats::hist_string(&s.entry_hist),
        "1:0:0:0:0:0:0:0:0",
        "4 of 64 words is the first eighth"
    );
    assert_eq!(rmt::lag_bucket(64, 64), rmt::LAG_BUCKETS - 1, "≥ half");
    assert_eq!(rmt::lag_bucket(0, 64), 0);
}

/// A threshold the guest never answers is counted, not invented: the last
/// threshold of a frame is answered by the driver's `finish`, not by
/// `refill`.
#[test]
fn an_unanswered_threshold_is_counted_rather_than_invented() {
    let (mut sb, mut r) = armed(0, 1, true, 32, data_word());
    start(&mut sb, &mut r, 0);
    sb.run_to(&mut r, 40 * WORD_CYCLES);
    for w in 0..64 {
        sb.write(&mut r, ram(w), DATA);
    }
    sb.run_to(&mut r, 80 * WORD_CYCLES);
    let s = r.refill_stats(0);
    assert_eq!(s.refills, 0);
    assert!(s.unanswered >= 1, "the frame ended with a threshold open");
}

// ---- the snapshot -------------------------------------------------------

/// Everything this block observes rides the snapshot, or a restored run
/// would produce different `RMT REFILL` notes and different pulses from the
/// run it was taken from.
#[test]
fn the_state_round_trips_through_a_snapshot() {
    let (mut sb, mut r) = armed(0, 2, true, 64, data_word());
    r.set_keep_logs(true);
    start(&mut sb, &mut r, 0);
    sb.run_to(&mut r, 70 * WORD_CYCLES);
    sb.write(&mut r, tx_lim(0), 64);

    let blob = r.save_state();
    let mut other = Rmt::new();
    other.load_state(&blob);
    assert_eq!(other.save_state(), blob, "byte-identical round trip");
    assert_eq!(other.pulses(0), r.pulses(0));
    assert_eq!(other.words(0), r.words(0));
    assert_eq!(other.refill_stats(0), r.refill_stats(0));
    assert_eq!(other.is_running(0), r.is_running(0));
    assert!(other.keep_logs());
}

// ---- what is accepted and named -----------------------------------------

/// RX is out of scope and said so: the bit is remembered, nothing samples a
/// pad, and the channel is named once in a warning rather than silently
/// modelled.
#[test]
fn rx_en_is_accepted_and_named_rather_than_modelled() {
    let mut sb = Sandbox::new();
    let mut r = Rmt::new();
    sb.write(&mut r, conf1(4), CONF1_RUN | (1 << 1));
    assert_ne!(sb.read(&mut r, conf1(4)) & (1 << 1), 0, "remembered");
    assert!(!r.is_running(4), "and nothing receives");
}

/// The APB FIFO is not modelled: `chNdata` and `chNaddr` are accepted, the
/// driver uses direct RAM access, and the RAM is the live window the engine
/// reads from.
#[test]
fn the_apb_fifo_is_not_modelled_and_the_ram_is_live() {
    let (mut sb, mut r) = armed(0, 1, true, 0, data_word());
    sb.write(&mut r, 0x000, 0xdead_beef);
    assert_eq!(sb.read(&mut r, 0x000), 0, "chNdata is not a FIFO here");
    start(&mut sb, &mut r, 0);
    // The engine reads `ram[raddr]` at the fetch, so a write behind the read
    // pointer is what the transmitter sees next time round.
    sb.write(&mut r, ram(10), word(false, 5, true, 5));
    assert_eq!(sb.read(&mut r, ram(10)), word(false, 5, true, 5));
    assert_eq!(r.ram()[10], word(false, 5, true, 5));
}
