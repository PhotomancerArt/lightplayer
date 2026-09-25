//! `USB_DEVICE` (USB-Serial-JTAG) — the honest model of the host's side:
//! **absent**, **attached with the port closed**, and **attached with an
//! application draining it**, with the transitions between them. C6 P6 built
//! the absent state; M6 P2 grew it; M6 P3 added the control channel that
//! drives the transitions from outside. Xtensa M6 P05 **moved** the file here
//! and parameterised it: every number that differs between two parts is in
//! [`Config`], and the file's behaviour is unchanged (ruling D1 (b)/DD64).
//!
//! Register facts are the esp32c6 PAC 0.23.2 `usb_device` block (offsets in
//! the chip's `regs` table; reset values per register below) and the two
//! drivers that touch it (M6 discovery §2–§3): esp-println's raw-MMIO printer
//! (`esp-println-0.17.0/src/lib.rs:235-337`) and esp-hal 1.1.1's
//! `usb_serial_jtag.rs` (blocking `:173-231`, async + ISR `:806-961`). The
//! firmware's own reading of a host is `UsbConnectionMonitor`
//! (`fw-esp32c6/src/board/esp32c6/usb_connection.rs`): SOF every 1 ms means
//! a cable, two 250 ms write timeouts in a row mean nobody is draining. The
//! S3's monitor reads `int_raw.sof` and clears it and does nothing else
//! (`fw-esp32s3/src/board/esp32s3/usb_connection.rs:29-32`).
//!
//! # Why one file serves two chips (the layout identity, quoted)
//!
//! [`crate::ip`]'s rule is that a layout may live here only when two chips'
//! PACs agree on it offset-for-offset, verified and quoted. They do
//! (`m6/notes.md` §3.1): `ep1 0x00`, `ep1_conf 0x04`, `int_raw 0x08`,
//! `int_st 0x0c`, `int_ena 0x10`, `int_clr 0x14`, `conf0 0x18`, `test 0x1c`,
//! `jfifo_st 0x20`, `fram_num 0x24`, `in_ep0..3_st 0x28..0x34`,
//! `out_ep0..2_st 0x38..0x40`, `misc_conf 0x44`, `mem_conf 0x48`,
//! `date 0x80` — identical offsets, identical bitfields, identical reset
//! values (`conf0 = 0x4200`, `ep1_conf = 0x02`, `in_epN_st = 0x01`,
//! `int_raw = 0x08`, `jfifo_st = 0x44`, `mem_conf = 0x02`). `int_raw` bits
//! 0–11 match by name and position, including the four the model lives on:
//! `sof` 1, `serial_out_recv_pkt` 2, `serial_in_empty` 3, `usb_bus_reset` 9.
//!
//! ⚠️ **Where they part.** `0x4c`…`0x7c` is **reserved** on the S3
//! (`esp32s3-0.35.2/src/usb_device.rs:22`): no `chip_rst 0x4c`, no
//! `set/get_line_code` quad, no `config_update`, no `ser_afifo_config`, no
//! `bus_reset_st 0x68`; and `int_raw` bits 12–15 (`rts_chg`, `dtr_chg`,
//! `get_line_code`, `set_line_code`) are C6-only. The C6 view names exactly
//! two of those as live behaviour — `chip_rst` and `bus_reset_st` — so they
//! sit behind [`HostReset`], a capability the chip supplies or withholds,
//! and the interrupt bits a chip does not have are held out by
//! [`Config::int_mask`]. `test` resets to `0x30` on the C6 and `0` on the
//! S3, which is the chip's `regs` table's business and never this file's.
//!
//! ⚠️ **A chip with no [`HostReset`] cannot refuse a reset over the serial
//! channel**: there is no `chip_rst` bit 2 (`disable_usb_serial_chip_reset`)
//! to set, so [`UsbSerialJtag::reset`] and
//! [`UsbSerialJtag::download_mode`] are unconditional there and
//! [`UsbSerialJtag::chip_reset_disabled`] is always `false`.
//!
//! # The host at the registers (discovery §5, made code)
//!
//! | state | `int_raw.sof` | `ep1_conf` free (bit 1) after `wr_done` | `serial_in_empty` (bit 3) | OUT path |
//! |---|---|---|---|---|
//! | **absent** | never | 0 for ever | set at reset, never again after the first commit | never |
//! | **attached, port closed** | every 1 ms | 0 until `open()` | not raised → esp-hal's write future waits out its 250 ms | never (a closed port sends nothing; bytes a script delivers wait for `open`) |
//! | **attached, draining** | every 1 ms | back to 1 [`IN_DRAIN_LATENCY_US`] after the commit | raised when the packet leaves | host bytes land as ≤ 64 B packets: `avail` = 1, `serial_out_recv_pkt`, `out_ep1_st` says how many |
//! | **detach** | stops | a committed packet stays committed | — | staged host bytes are dropped |
//! | **re-attach** | bus reset, then SOF | a committed packet is **dropped** by the bus reset (modeled: the reset empties the endpoint) | set by the drop | — |
//!
//! **A full FIFO commits itself.** Writing the 64th byte to `ep1` is the same
//! as writing `wr_done`: the block sends the packet without being asked. That
//! is not a convenience — esp-hal's `write_byte_nb`, the only API that
//! touches this register a byte at a time, documents it ("Requires manual
//! flushing (automatically flushed every 64 bytes)",
//! `esp-hal-1.1.1/src/usb_serial_jtag.rs:191-192`) and a driver written
//! against that doc spins for ever on a full endpoint otherwise. M5 P3 found
//! it: the `rmt-chase` harness's `Esp32UsbSerialIo` writes byte by byte and
//! flushes only at the end of a write, so every log line over 64 bytes stalled
//! until its 250 ms drain timeout and dropped its tail. The shipped image's
//! `ChunkedWriter` chunks below 64 and writes `wr_done` itself, which is why
//! M6 never reached the case.
//!
//! **One send buffer, and a write into a pending packet is refused.** The
//! block holds a single 64-byte IN buffer. After a flush — `wr_done` or the
//! 64th byte, either way — the send buffer is "unavailable for firmware to
//! write into" until the host has read all of it, and only then does
//! `SERIAL_IN_EMPTY_INT` fire to say another 64 bytes fit (ESP32-C3 TRM v1.3
//! §30.3.2 "CDC-ACM Firmware Interface Functional Description", p. 767 — the same IP
//! block; its register chapter, the esp32c6 0.23.2 PAC and the esp32s3 0.35.2
//! PAC describe `SERIAL_IN_EP_DATA_FREE` in the same words, and ESP-IDF's
//! `usb_serial_jtag_ll_write_txfifo` gates every byte on that bit). So the
//! model commits one packet and holds `serial_in_ep_data_free` at 0 until the
//! host takes it — that part is **documented**. What silicon does with a byte
//! written anyway (dropped here, onto the `tried` stream) is not stated
//! anywhere and stays **modeled**. The seven-bit `in_ep1_st` address fields
//! are not evidence of a second buffer: every endpoint's status register
//! (`in_ep0_st`…`in_ep3_st`, the 64-byte control endpoint included) has the
//! same 7-bit `wr_addr`/`rd_addr` pair, and counting 0…64 takes seven bits.
//! The 2026-09-13 defect is two writers breaking this contract — esp-hal's
//! `write_async` never reads the free bit — not the model; its register-level
//! tests are under "one send buffer, two writers" below.
//!
//! **What happens to a refused byte is not known, and neither is the loss's
//! size.** The model drops every byte written while the buffer is not free,
//! one byte at a time, and keeps every byte written after it frees. So a
//! write that begins inside the pending window and outlasts it loses its
//! *head* and delivers its tail: a partial loss. Whether silicon drops the
//! bytes, drops the whole write, overwrites the pending packet, or something
//! else is not written anywhere (TRM §30.3.2 says only that the buffer is
//! "unavailable for firmware to write into"). The one silicon observation, the
//! C6's packed-frame loss (below), lost **~5 bytes** of a frame made only of
//! whole 64-byte packets. That fits "refused bytes are dropped, and the window
//! ended partway through a write". It does not prove it: the loss could not be
//! located inside the frame, so a whole-write loss of some other short write
//! is not excluded.
//!
//! # The free lag (a hypothesis switch, off by default)
//!
//! In the model a drain raises `serial_in_empty` and returns
//! `serial_in_ep_data_free` **at the same cycle**. With one writer on the
//! endpoint, which is the shipped C6 after boot, that makes a loss impossible.
//! esp-hal's write future wakes only on a drain, so every packet it writes
//! finds the buffer free. Yet the real C6 lost a few bytes inside ~0.3 % of
//! its packed frames, silently, until PR #795's IN-endpoint gate
//! (`fw-esp32-common/src/serial/in_endpoint.rs`) went in, and none after
//! (`docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md`).
//!
//! [`UsbSerialJtag::set_in_free_lag_ns`] (`--usb-in-free-lag <ns>`) opens a
//! gap between the two. The drain raises `serial_in_empty` as always, but the
//! buffer stays unwritable for the lag, and no second edge marks the lag's
//! end. esp-hal's `write_async` writes a frame's next packet the moment its
//! future wakes, with no free check. Its first few bytes land inside the lag
//! and are refused, and the rest of the packet is delivered. The frame arrives
//! a few bytes short, and nothing on the device notices. The gate reads
//! `serial_in_ep_data_free` before every packet, a few hundred cycles later on
//! its own path, and so writes only once the buffer is free.
//! `lp-cli/tests/emu_usb_free_lag.rs` is the pair: the ungated fixture image
//! tears packed frames by a few bytes, and the shipped image, at the same lag,
//! delivers every frame.
//!
//! It is a **hypothesis**, graded nothing:
//!
//! - no document gives silicon such a gap, and nobody has measured one;
//! - it is the one path found that produces the symptom's *shape* with a
//!   single writer: a short frame, a few bytes, nothing logged;
//! - ESP-IDF's own driver does not trust the edge either. Its ISR re-checks
//!   that the FIFO is writable after `SERIAL_IN_EMPTY` and ignores the
//!   interrupt if it is not (`esp-idf` v5.4,
//!   `components/esp_driver_usb_serial_jtag/src/usb_serial_jtag.c`,
//!   `usb_serial_jtag_isr_handler_default`; read as behaviour, Apache-2.0).
//!   It blames a second writer, but the check covers a lag just the same;
//! - it does **not** explain why JSON lost nothing on the same board. The
//!   ungated emulated image loses bytes at the lag whichever encoding it
//!   writes, because the wake-to-write path is the same for both.
//!
//! So the lag stays at 0 everywhere except the test that switches it on, and
//! a transcript, not this paragraph, is what could ever promote it.
//!
//! Two things distinguish the draining state from esp-emu's model (spike
//! report §4: `INT_RAW = 0xA` with `INT_CLR` ignored, `EP1_CONF = 0x2` for
//! ever): `sof` **clears** on `int_clr` and returns on the next frame, and a
//! packet committed with `wr_done` **reaches the host stream** — the
//! `usb-sj` stream is what a host received, never what the guest tried.
//!
//! # What the guest then does (the sources, not this model)
//!
//! esp-println (every `[INIT] …` line, panics through esp-backtrace):
//! `fifo_full()` is `ep1_conf & 0b010 == 0`; a full FIFO is committed with
//! `wr_done` (`0b001`) and `wait_for_flush()` spins **50,000** iterations on
//! `fifo_full()`, then latches `TIMED_OUT` and every later print returns at
//! once. `println!` ends in a flush, so a line shorter than 64 B is
//! committed at its newline. With a draining host the spin ends after the
//! latency (≈ 3,000 iterations under `t1`); with the port closed or no
//! cable it latches, once, and the firmware's console falls silent.
//!
//! esp-hal (io_task's link): `write_async` pushes ≤ 64 bytes, writes
//! `wr_done`, and awaits a future whose `new` **sets `int_ena.serial_in_empty`**
//! and whose `poll` is Ready when that enable bit reads **clear** — the ISR
//! clears it. So on the async path the enable bit doubles as the future's
//! completion flag, and a future dropped mid-wait (the 250 ms chunk timeout,
//! the 1 ms `select` around `read`) leaves its enable bit **set** until the
//! interrupt fires and the ISR clears it. `read_async` = `drain_rx_fifo`
//! (pop `ep1` while `avail`) or a future on `serial_out_recv_pkt`, the same
//! way. This model never clears an enable bit; only the guest does.
//!
//! `UsbConnectionMonitor::poll` (every ~2 ms): `sof = int_raw.sof`;
//! `int_clr.sof = 1`; three misses ≈ 6 ms → not enumerated, and the
//! not-draining latch is reset so a re-attach starts optimistic.
//!
//! # Constants that are not the PAC's or a driver's
//!
//! - [`SOF_PERIOD_US`] = 1,000 — grade **documented**: the USB full-speed
//!   frame, the firmware's own module doc ("USB full-speed hosts send SOF
//!   every 1ms"); never measured here.
//! - [`IN_DRAIN_LATENCY_US`] = 100 — grade **modeled**: "sub-millisecond,
//!   well under every timeout the firmware uses (100 ms probe, 250 ms
//!   chunk, 2 ms bridge stall); not measured". [`OUT_LAND_LATENCY_US`] is
//!   the same number for the same reason.
//! - one OUT packet resident at a time — grade **modeled**, from the PAC's
//!   single `out_ep1_st` buffer with one `wr_addr`/`rd_addr` pair: the
//!   hardware NAKs the host until software has read the packet, so the next
//!   one lands only after the pop (plus the latency).
//! - a bus reset drops a committed IN packet — grade **modeled**: a USB bus
//!   reset returns the device to its default state; nothing here observed
//!   it.
//!
//! # Per-register grades (`--strict-grade`)
//!
//! ⚠️ **The grades are the chip's, not this file's** ([`Config::grades`]),
//! and they do not travel between parts: a transcript recorded on a C6 is a
//! measurement of a C6. A chip whose silicon nobody has read supplies an
//! empty table and every register is [`RegGrade::Modeled`]. What follows is
//! the **C6's** table and the evidence behind it, kept here because it is
//! also the worked example of what a promotion costs.
//!
//! Revised by M6 P4 once the transcripts existed. A grade moves only with a
//! transcript, and the four under `lp-emu/transcripts/esp32c6/` are what
//! moved these: `boot-idle` (a host attached and draining, replayed against
//! silicon's capture of the same image bytes), `usb-negative-control` (the
//! port held closed from boot and opened at eight seconds),
//! `usb-detach-reattach` and `usb-host-absent`.
//!
//! | grade | registers | why |
//! |---|---|---|
//! | `measured` | `ep1`, `ep1_conf`, `int_raw`, `int_st`, `int_ena`, `int_clr` | the transitions those transcripts prove: SOF present while attached and absent when the cable is out; `serial_in_ep_data_free` returning only once a host has drained the packet (measured through esp-println's polled path and one writer at a time; a write *into* the pending packet is the "one send buffer" paragraph's — documented refusal, modeled fate); `serial_in_empty` completing esp-hal's write future; `serial_out_recv_pkt` on host bytes (`lp-cli`'s hello, `emu_usb_hello`); and the whole path exercised byte for byte by both drivers |
//! | `documented` | `fram_num` (the SOF period is the USB full-speed frame), `conf0` (the PAC bit map; never written by the shipped image on the C6) | a document states the behaviour; nothing measured it |
//! | `modeled` | everything else — listed under "Modeled registers" below | our reading of the PAC and the drivers |
//!
//! **The grade is per register, and three of these registers are only
//! measured bit by bit.** What the transcripts prove of `int_raw` / `int_st`
//! / `int_ena` / `int_clr` is bits 1–3 (`sof`, `serial_out_recv_pkt`,
//! `serial_in_empty`) and of `ep1_conf` bits 0–2 (`wr_done`,
//! `serial_in_ep_data_free`, `serial_out_ep_data_avail`). The rest of those
//! registers — the four bus-error bits, `in_token_rec_in_ep1`,
//! `usb_bus_reset`, the two zero-payload bits, `rts_chg`/`dtr_chg`, the two
//! line-coding bits — is still our reading of the PAC, and the firmware
//! never enables or reads any of them. Grading them with the register they
//! live in is coarser than the evidence; a per-bit table would be honest to
//! the bit and is the obvious refinement if anything ever depends on it.
//! Recorded here rather than smoothed over.
//!
//! # Modeled registers (the strict gate's list)
//!
//! `test`, `jfifo_st`, `in_ep0_st`, `in_ep1_st`, `in_ep2_st`, `in_ep3_st`,
//! `out_ep0_st`, `out_ep1_st`, `out_ep2_st`, `misc_conf`, `mem_conf`,
//! `chip_rst`, `set_line_code_w0`, `set_line_code_w1`, `get_line_code_w0`,
//! `get_line_code_w1`, `config_update`, `ser_afifo_config`, `bus_reset_st`,
//! `date`. Reset values are the PAC's and reads answer them; the behaviour
//! behind them is not modelled, for one reason each time: **neither esp-hal
//! 1.1.1 nor esp-println 0.17 touches them on the C6**. A run under
//! `--strict-grade documented` therefore crosses none of them, and one that
//! did would stop with the register's name — which is the point of the flag.
//! (`out_ep1_st` is the exception that proves it: the OUT path writes it and
//! the drivers read `wr_addr`, but only through `drain_rx_fifo`'s `avail`
//! test, and no transcript here sends enough host bytes to exercise its
//! wrap. Modeled, and said so.)
//!
//! # Not modelled, on purpose
//!
//! **The PCR reset of the block** (esp-hal's `new_inner` pulses PCR
//! `usb_device_conf.rst_en` on the first enable) is not seen here: the
//! block does not watch PCR. Whether that pulse re-enumerates the device on
//! silicon is decidable from the sitting-1 attached-host transcript
//! (discovery §5 (b): one port open showing both the `[INIT]` lines and the
//! hello says it does not); if it does, P4 adds the seam. The JTAG channel,
//! the line-coding registers, the bus-error interrupt bits and the
//! zero-payload bits are accept-and-remember: the firmware never reads
//! them, and a strict-grade run stops on them by design.

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_emu_core::sched::EventId;

use crate::regfile::{lane_of, merge_lane};
use crate::regnames::RegNames;
use crate::{
    BusCx, MachineRequest, Peripheral, RegFile, RegGrade, RegGrades, ResetSource, Strap, StreamId,
    Width, event_id, event_local,
};

pub const EP1: u32 = 0x00;
pub const EP1_CONF: u32 = 0x04;
pub const INT_RAW: u32 = 0x08;
pub const INT_ST: u32 = 0x0c;
pub const INT_ENA: u32 = 0x10;
pub const INT_CLR: u32 = 0x14;
pub const CONF0: u32 = 0x18;
pub const FRAM_NUM: u32 = 0x24;
pub const IN_EP1_ST: u32 = 0x2c;
pub const OUT_EP1_ST: u32 = 0x3c;

// PAC `RESET_VALUE`s (discovery §1). Every other register resets to 0.
const IN_EPN_ST_RESET: u32 = 0x01;

const EP1_CONF_WR_DONE: u32 = 1 << 0;
const EP1_CONF_IN_FREE: u32 = 1 << 1;
const EP1_CONF_OUT_AVAIL: u32 = 1 << 2;

pub const INT_SOF: u32 = 1 << 1;
pub const INT_SERIAL_OUT_RECV_PKT: u32 = 1 << 2;
pub const INT_SERIAL_IN_EMPTY: u32 = 1 << 3;
pub const INT_IN_TOKEN_REC_IN_EP1: u32 = 1 << 8;
pub const INT_USB_BUS_RESET: u32 = 1 << 9;
pub const INT_RTS_CHG: u32 = 1 << 12;
pub const INT_DTR_CHG: u32 = 1 << 13;

/// Every interrupt bit both chips' `int_raw` declares: bits 0–11. A chip
/// with the CDC line-coding and modem-line bits passes a wider
/// [`Config::int_mask`]; a chip without them passes this one, and the bits it
/// does not have can then be neither raised, enabled nor cleared.
pub const INT_MASK_CORE: u32 = 0x0fff;
/// The C6's mask: [`INT_MASK_CORE`] plus `rts_chg`, `dtr_chg` and the two
/// line-coding bits (12–15).
pub const INT_MASK_WITH_CDC: u32 = 0xffff;

/// `chip_rst`: bit 0 "chip reset is detected from usb serial channel, write
/// 1 to clear"; bit 1 the same from the JTAG channel; bit 2 "disable chip
/// reset from usb serial channel".
const CHIP_RST_SERIAL: u32 = 1 << 0;
const CHIP_RST_JTAG: u32 = 1 << 1;
const CHIP_RST_DISABLE: u32 = 1 << 2;

/// The two registers a chip either has or does not: `chip_rst` and
/// `bus_reset_st`.
///
/// The C6 has both and the view gives them behaviour — a bus reset releases
/// `bus_reset_st`, and `chip_rst` records a serial-channel reset and can
/// **refuse** one through bit 2. The S3's silicon has neither
/// (`esp32s3-0.35.2/src/usb_device.rs:22` reserves `0x4c`…`0x7c`), so a chip
/// that withholds this capability models the absence: nothing is written at
/// those offsets, and a reset over the serial channel is unconditional
/// because there is no bit that could disable it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostReset {
    /// `chip_rst`'s offset in this chip's block.
    pub chip_rst: u32,
    /// `bus_reset_st`'s offset.
    pub bus_reset_st: u32,
    /// `bus_reset_st`'s PAC reset value — the value a bus reset restores.
    pub bus_reset_st_reset: u32,
}

/// The chip's parameters for this block.
///
/// Everything in here is a **chip** number and none of it may become a
/// `const` in this file: the base, the aperture, the interrupt source, the
/// generated register-name table, the CPU clock the modelled latencies are
/// expressed in, the interrupt bits the part declares, the grades its
/// evidence supports, and whether it has the host-reset pair at all.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// The block's base address in the chip's MMIO space.
    pub base: u32,
    /// Its aperture, as the chip registers it.
    pub len: u32,
    /// The interrupt **source** number this block drives.
    pub source: u16,
    /// The chip's generated register-name table for `usb_device`. Its reset
    /// values seed the accept-and-remember file, so the two registers whose
    /// resets differ between parts need nothing from this file.
    pub regs: &'static RegNames,
    /// CPU cycles per emulated microsecond — the chip's clock. Every
    /// latency below is stated in microseconds and multiplied by this.
    pub cycles_per_us: u64,
    /// Every `int_raw` bit this part declares. Bits outside it can be
    /// neither raised by the model, enabled by the guest, nor cleared:
    /// [`INT_MASK_CORE`] for a part without the CDC and modem-line bits,
    /// [`INT_MASK_WITH_CDC`] for one with them.
    pub int_mask: u32,
    /// How often a **live** host source (a socket) is re-polled, in cycles.
    /// Wall clock decides when a socket's bytes appear, so the model asks on
    /// a grid in guest time; the chip owns the number because it is a cycle
    /// count.
    pub live_poll_cycles: u64,
    /// `(offset, grade)` for every register this chip's own evidence lifts
    /// above [`RegGrade::Modeled`]. Empty when nobody has read that chip's
    /// silicon — which is the honest answer, not a gap.
    pub grades: &'static [(u32, RegGrade)],
    /// `chip_rst` / `bus_reset_st`, when the part has them.
    pub host_reset: Option<HostReset>,
}

/// The chip's per-register grade table, as [`Config::grades`] declares it.
///
/// Anything unlisted is [`RegGrade::Modeled`], which for this block is a
/// considered answer rather than a default: the chip's README lists those
/// registers by name, with the reason its firmware never reaches them.
pub fn grades(cfg: &Config) -> RegGrades {
    let mut table = RegGrades::new();
    for (off, grade) in cfg.grades {
        table = table.with_grade(*off, *grade);
    }
    table
}

/// Every register this chip's block grades `Modeled`, in offset order — the
/// list a README and `--strict-grade` both mean by "the modeled registers".
pub fn modeled_registers(cfg: &Config) -> Vec<&'static str> {
    let table = grades(cfg);
    (0..cfg.len)
        .step_by(4)
        .filter(|off| table.grade(*off) == RegGrade::Modeled)
        .filter_map(|off| cfg.regs.name(off))
        .collect()
}

/// A little-endian reader over a `save_state` blob.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn u32(&mut self) -> Option<u32> {
        let (head, rest) = self.0.split_first_chunk::<4>()?;
        self.0 = rest;
        Some(u32::from_le_bytes(*head))
    }

    fn u64(&mut self) -> Option<u64> {
        let (head, rest) = self.0.split_first_chunk::<8>()?;
        self.0 = rest;
        Some(u64::from_le_bytes(*head))
    }
}

/// `in_ep1_st` / `out_ep1_st` field shapes: bits 0:1 the endpoint state,
/// bits 2:8 `wr_addr`, bits 9:15 `rd_addr`; `out_ep1_st` bits 16:22
/// `rec_data_cnt`. "When SERIAL_OUT_RECV_PKT_INT is detected, there are
/// `OUT_EP1_WR_ADDR-2` bytes data in OUT EP1."
const EP_ST_WR_ADDR_SHIFT: u32 = 2;
const EP_ST_RD_ADDR_SHIFT: u32 = 9;
const OUT_EP_REC_CNT_SHIFT: u32 = 16;
const OUT_EP_WR_ADDR_BIAS: u32 = 2;

/// The IN endpoint FIFO: 64 bytes (the PAC's `ep1` doc: "up to 64 bytes").
pub const IN_FIFO_DEPTH: usize = 64;
/// One OUT packet: a full-speed bulk packet is at most 64 bytes.
pub const OUT_PACKET_MAX: usize = 64;

/// The SOF period: the USB full-speed frame, 1 ms. Grade **documented**
/// (the USB specification's frame; the firmware's module doc cites it).
///
/// ⚠️ The three latencies here are **microseconds**, and the cycle counts
/// they become are the chip's ([`Config::cycles_per_us`]): the same
/// millisecond is a different number of cycles at 160 MHz and at 240 MHz,
/// which is the whole reason they are not `const` cycle counts any more.
pub const SOF_PERIOD_US: u64 = 1_000;

/// How long after `wr_done` a draining host has taken the IN packet. Grade
/// **modeled**: sub-millisecond, well under every timeout the firmware uses
/// (the 100 ms probe, the 250 ms chunk, the bridge's 2 ms stall); not
/// measured.
pub const IN_DRAIN_LATENCY_US: u64 = 100;

/// How long after the host writes a byte (or the previous OUT packet is
/// read out) the next OUT packet lands. Grade **modeled**, the same number
/// for the same reason as [`IN_DRAIN_LATENCY_US`].
pub const OUT_LAND_LATENCY_US: u64 = IN_DRAIN_LATENCY_US;

const EV_SOF: u16 = 0;
const EV_IN_DELIVER: u16 = 1;
const EV_OUT_POLL: u16 = 2;
const EV_OUT_LAND: u16 = 3;
const EV_IN_FREE: u16 = 4;

/// The host's side of the cable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostState {
    /// No cable: no SOF, nothing ever drains, nothing ever arrives.
    #[default]
    Absent,
    /// A cable and an enumerated host. `draining` is whether an
    /// application has the port open and is reading it.
    Attached { draining: bool },
}

impl HostState {
    pub const fn attached(self) -> bool {
        matches!(self, HostState::Attached { .. })
    }

    pub const fn draining(self) -> bool {
        matches!(self, HostState::Attached { draining: true })
    }

    /// `absent` | `attached` (draining) | `attached-idle` (port closed), as
    /// the CLI's `--usb-host` spells them.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "absent" => Some(HostState::Absent),
            "attached" => Some(HostState::Attached { draining: true }),
            "attached-idle" => Some(HostState::Attached { draining: false }),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            HostState::Absent => "absent",
            HostState::Attached { draining: true } => "attached",
            HostState::Attached { draining: false } => "attached-idle",
        }
    }
}

impl core::fmt::Display for HostState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// USB-Serial-JTAG with a host model.
#[derive(Debug)]
pub struct UsbSerialJtag {
    /// The chip's parameters. Not in the save-state blob: it is how the
    /// machine was **built**, not what a run's future depends on.
    cfg: &'static Config,
    /// [`SOF_PERIOD_US`] in this chip's cycles.
    sof_period_cycles: u64,
    /// [`IN_DRAIN_LATENCY_US`] in this chip's cycles.
    in_drain_latency_cycles: u64,
    /// [`OUT_LAND_LATENCY_US`] in this chip's cycles.
    out_land_latency_cycles: u64,
    regs: RegFile,
    grades: RegGrades,
    index: usize,
    /// The `usb-sj` stream: what a host **received** (sink) and what it
    /// **sent** (source).
    delivered: Option<StreamId>,
    /// The observation stream: bytes the guest handed over that no host
    /// took — pushed while absent, dropped past a committed FIFO, or
    /// dropped by a bus reset.
    tried: Option<StreamId>,
    host: HostState,

    // The IN endpoint (device → host).
    in_fifo: Vec<u8>,
    /// A `wr_done` committed the FIFO as one packet; `free` reads 0 until a
    /// host takes it.
    committed: bool,
    committed_at: Option<u64>,
    /// The cycle a scheduled delivery lands, while one is in flight.
    in_deliver_due: Option<u64>,
    /// Bytes written past the 64-byte fill or into a committed FIFO,
    /// dropped.
    dropped: u64,
    /// Committed packets a bus reset dropped.
    dropped_packets: u64,
    in_delivered: u64,
    /// The **free lag**, in this chip's cycles: how long after a drain's
    /// `serial_in_empty` the send buffer stays unwritable. 0 — the default,
    /// and every gate's setting — is the drain and the free bit at the same
    /// cycle. See [`set_in_free_lag_ns`](Self::set_in_free_lag_ns).
    in_free_lag_cycles: u64,
    /// While the free lag runs: the cycle `serial_in_ep_data_free` returns.
    in_free_due: Option<u64>,
    /// Bytes refused because they were written inside the free lag (a
    /// subset of [`dropped`](Self::dropped)).
    dropped_in_lag: u64,

    // Interrupts and frames.
    int_raw: u32,
    /// `fram_num`: 11 bits, the last SOF's frame index.
    fram_num: u16,
    /// The cycle the next SOF lands; chained from here, never from the
    /// dispatch cycle (a slice ends at or after an event).
    sof_due: u64,

    // The OUT endpoint (host → device).
    /// Host bytes taken from the source and not yet landed: the host side's
    /// buffer while the device NAKs.
    out_staging: VecDeque<u8>,
    /// The one resident packet, as the guest reads it out.
    out_pkt: VecDeque<u8>,
    out_wr_addr: u8,
    out_rd_addr: u8,
    out_rec_cnt: u8,
    out_land_due: Option<u64>,
    out_poll_due: u64,
    /// `ep1` reads with no packet resident (answered 0).
    out_underruns: u64,
    out_landed: u64,

    // The control lines, as the host last set them.
    last_dtr: Option<bool>,
    last_rts: Option<bool>,
    dtr_high_seen: bool,
}

impl UsbSerialJtag {
    /// `cfg` is the chip's parameters; `delivered` is the `usb-sj` stream
    /// (what a host receives / sends); `tried` the observation stream;
    /// `host` the state at power-on.
    pub fn new(
        cfg: &'static Config,
        delivered: Option<StreamId>,
        tried: Option<StreamId>,
        host: HostState,
    ) -> Self {
        Self {
            cfg,
            sof_period_cycles: SOF_PERIOD_US * cfg.cycles_per_us,
            in_drain_latency_cycles: IN_DRAIN_LATENCY_US * cfg.cycles_per_us,
            out_land_latency_cycles: OUT_LAND_LATENCY_US * cfg.cycles_per_us,
            regs: RegFile::new("USB_DEVICE", cfg.len).with_names(*cfg.regs),
            grades: grades(cfg),
            index: 0,
            delivered,
            tried,
            host,
            in_fifo: Vec::with_capacity(IN_FIFO_DEPTH),
            committed: false,
            committed_at: None,
            in_deliver_due: None,
            dropped: 0,
            dropped_packets: 0,
            in_delivered: 0,
            in_free_lag_cycles: 0,
            in_free_due: None,
            dropped_in_lag: 0,
            int_raw: INT_SERIAL_IN_EMPTY,
            fram_num: 0,
            sof_due: 0,
            out_staging: VecDeque::new(),
            out_pkt: VecDeque::with_capacity(OUT_PACKET_MAX),
            out_wr_addr: 0,
            out_rd_addr: 0,
            out_rec_cnt: 0,
            out_land_due: None,
            out_poll_due: 0,
            out_underruns: 0,
            out_landed: 0,
            last_dtr: None,
            last_rts: None,
            dtr_high_seen: false,
        }
    }

    /// P6's shape: no host, one observation stream.
    pub fn absent(cfg: &'static Config, tried: Option<StreamId>) -> Self {
        Self::new(cfg, None, tried, HostState::Absent)
    }

    /// The chip's parameters, as the machine and the CLI read them back.
    pub fn config(&self) -> &'static Config {
        self.cfg
    }

    // ---- what a test or the machine reads back ---------------------------

    pub fn host(&self) -> HostState {
        self.host
    }

    /// Bytes the guest handed to the IN endpoint and no host has taken yet
    /// (at most 64).
    pub fn in_fifo(&self) -> &[u8] {
        &self.in_fifo
    }

    /// A `wr_done` committed the FIFO and no host has taken it.
    pub fn committed(&self) -> bool {
        self.committed
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn dropped_packets(&self) -> u64 {
        self.dropped_packets
    }

    /// Bytes delivered to the host stream so far.
    pub fn in_delivered(&self) -> u64 {
        self.in_delivered
    }

    /// Bytes refused because they were written inside the free lag.
    pub fn dropped_in_lag(&self) -> u64 {
        self.dropped_in_lag
    }

    /// Hold `serial_in_ep_data_free` at 0 for `ns` emulated nanoseconds
    /// after each drain's `serial_in_empty` — **a hypothesis switch, off by
    /// default** (module docs, "The free lag"). The raw bit is raised at the
    /// drain as always, so esp-hal's write future wakes there; a byte written
    /// before the lag ends is refused like any byte written into a pending
    /// packet. No second `serial_in_empty` marks the end of the lag.
    pub fn set_in_free_lag_ns(&mut self, ns: u64) {
        self.in_free_lag_cycles = ns.saturating_mul(self.cfg.cycles_per_us) / 1_000;
    }

    /// The free lag in this chip's cycles (0 = off).
    pub fn in_free_lag_cycles(&self) -> u64 {
        self.in_free_lag_cycles
    }

    pub fn fram_num(&self) -> u16 {
        self.fram_num
    }

    /// Host bytes taken from the source and not yet read by the guest:
    /// resident plus staged.
    pub fn out_pending(&self) -> usize {
        self.out_pkt.len() + self.out_staging.len()
    }

    pub fn out_underruns(&self) -> u64 {
        self.out_underruns
    }

    /// Are frames arriving? True exactly while a host is attached — the
    /// `sof=on|off` the control channel's `state` reply reports.
    pub fn sof_running(&self) -> bool {
        self.host.attached()
    }

    // ---- the host's transitions (P3's control channel calls these) --------

    /// A cable is plugged in and the host enumerates the device: a bus
    /// reset, then SOF every millisecond. The port is closed until
    /// [`open`](Self::open). A committed IN packet is dropped by the bus
    /// reset (modeled; see the module docs).
    pub fn attach(&mut self, cx: &mut BusCx<'_>) {
        if self.host.attached() {
            log::warn!("USB_DEVICE: attach while already attached, ignored");
            return;
        }
        self.host = HostState::Attached { draining: false };
        self.bus_reset(cx);
        self.start_sof(cx.now, cx);
        let line = format!(
            "cyc={} pc=0x{:08x} USB_DEVICE host attached: bus reset, SOF every {SOF_PERIOD_US} \
             us, port closed",
            cx.now, cx.pc
        );
        cx.trace.note(&line);
        self.update_lines(cx);
    }

    /// The cable is pulled: SOF stops, a delivery in flight is lost, a
    /// committed packet stays committed, staged host bytes are dropped (they
    /// were in the host's buffer). The resident OUT packet stays: it is in
    /// the device's own buffer.
    pub fn detach(&mut self, cx: &mut BusCx<'_>) {
        if !self.host.attached() {
            log::warn!("USB_DEVICE: detach while absent, ignored");
            return;
        }
        self.host = HostState::Absent;
        cx.sched.cancel(event_id(self.index, EV_SOF));
        if self.in_deliver_due.take().is_some() {
            cx.sched.cancel(event_id(self.index, EV_IN_DELIVER));
        }
        if self.out_land_due.take().is_some() {
            cx.sched.cancel(event_id(self.index, EV_OUT_LAND));
        }
        let staged = self.out_staging.len();
        self.out_staging.clear();
        let line = format!(
            "cyc={} pc=0x{:08x} USB_DEVICE host detached: SOF stops{}{}",
            cx.now,
            cx.pc,
            if self.committed {
                format!(
                    "; {} committed bytes stay in the IN endpoint",
                    self.in_fifo.len()
                )
            } else {
                String::new()
            },
            if staged > 0 {
                format!("; {staged} staged host bytes dropped")
            } else {
                String::new()
            }
        );
        cx.trace.note(&line);
        self.update_lines(cx);
    }

    /// An application opens the port and reads it: a committed IN packet is
    /// delivered after the latency, and host bytes start landing.
    pub fn open(&mut self, cx: &mut BusCx<'_>) {
        match self.host {
            HostState::Attached { draining: false } => {}
            HostState::Attached { draining: true } => {
                log::warn!("USB_DEVICE: open while already draining, ignored");
                return;
            }
            HostState::Absent => {
                log::warn!("USB_DEVICE: open with no host attached, ignored (attach first)");
                return;
            }
        }
        self.host = HostState::Attached { draining: true };
        let line = format!(
            "cyc={} pc=0x{:08x} USB_DEVICE port opened: the host drains{}",
            cx.now,
            cx.pc,
            if self.committed {
                format!(
                    "; {} committed bytes delivered in {IN_DRAIN_LATENCY_US} us",
                    self.in_fifo.len()
                )
            } else {
                String::new()
            }
        );
        cx.trace.note(&line);
        if self.committed && self.in_deliver_due.is_none() {
            self.schedule_delivery(cx.now, cx);
        }
        self.try_land(cx.now, cx);
    }

    /// The application closes the port: nothing drains from here on. A
    /// delivery already in flight completes (the host controller had it).
    pub fn close(&mut self, cx: &mut BusCx<'_>) {
        if !self.host.draining() {
            log::warn!("USB_DEVICE: close while not draining, ignored");
            return;
        }
        self.host = HostState::Attached { draining: false };
        let line = format!(
            "cyc={} pc=0x{:08x} USB_DEVICE port closed: the host stops draining",
            cx.now, cx.pc
        );
        cx.trace.note(&line);
    }

    /// The host's DTR/RTS lines, decoded exactly as the scripted fake device
    /// does (`fake_device_core::set_signals`): any DTR-high marks the
    /// download dance; an RTS falling edge completes a dance — download
    /// mode if DTR went high, otherwise a plain reset. Either line changing
    /// raises its `*_chg` raw bit.
    pub fn set_signals(&mut self, dtr: Option<bool>, rts: Option<bool>, cx: &mut BusCx<'_>) {
        if let Some(dtr) = dtr {
            if self.last_dtr != Some(dtr) {
                self.raise(INT_DTR_CHG);
            }
            self.last_dtr = Some(dtr);
            if dtr {
                self.dtr_high_seen = true;
            }
        }
        if let Some(rts) = rts {
            let falling = self.last_rts == Some(true) && !rts;
            if self.last_rts != Some(rts) {
                self.raise(INT_RTS_CHG);
            }
            self.last_rts = Some(rts);
            if falling {
                if self.dtr_high_seen {
                    self.dtr_high_seen = false;
                    self.download_mode(cx);
                } else {
                    self.reset(cx);
                }
                // Whether the request was made or `chip_rst.disable`
                // suppressed it, the dance is over; the trace note says which.
            }
        }
        self.update_lines(cx);
    }

    /// The serial channel's chip reset into the app (the plain dance).
    /// `false` when `chip_rst.disable` (bit 2) suppressed it — which a chip
    /// with no [`HostReset`] can never do.
    pub fn reset(&mut self, cx: &mut BusCx<'_>) -> bool {
        self.chip_reset(Strap::App, cx)
    }

    /// The serial channel's chip reset into the ROM download console (the
    /// dance with DTR high). `false` when `chip_rst.disable` suppressed it.
    pub fn download_mode(&mut self, cx: &mut BusCx<'_>) -> bool {
        self.chip_reset(Strap::Download, cx)
    }

    /// `chip_rst` bit 2: the guest has disabled the chip reset the serial
    /// channel can ask for. A dance is then recorded and not performed, and
    /// the control channel answers `err` rather than pretending.
    ///
    /// ⚠️ **Always `false` on a chip with no [`HostReset`]**: the register
    /// does not exist there, so the guest has no say and a reset over the
    /// serial channel is unconditional.
    pub fn chip_reset_disabled(&self) -> bool {
        match self.cfg.host_reset {
            Some(hr) => self.regs.stored(hr.chip_rst) & CHIP_RST_DISABLE != 0,
            None => false,
        }
    }

    /// Host bytes on the OUT path without a byte stream behind them: the
    /// control channel's `usb-write`, and what a test uses instead of a
    /// socket. They stage exactly as a source's bytes do — a closed port
    /// holds them until [`open`](Self::open).
    pub fn host_write(&mut self, bytes: &[u8], cx: &mut BusCx<'_>) {
        if bytes.is_empty() {
            return;
        }
        self.out_staging.extend(bytes.iter().copied());
        self.try_land(cx.now, cx);
    }

    fn chip_reset(&mut self, strap: Strap, cx: &mut BusCx<'_>) -> bool {
        // A chip with no `chip_rst` records nothing and refuses nothing: the
        // register is not there, so the reset is unconditional.
        let disabled = match self.cfg.host_reset {
            Some(hr) => {
                let stored = self.regs.stored(hr.chip_rst);
                self.regs.poke(hr.chip_rst, stored | CHIP_RST_SERIAL);
                stored & CHIP_RST_DISABLE != 0
            }
            None => false,
        };
        let line = format!(
            "cyc={} pc=0x{:08x} USB_DEVICE chip reset from the serial channel, strap = {strap}{}",
            cx.now,
            cx.pc,
            if disabled {
                " — chip_rst.disable is set, recorded and not performed"
            } else if self.cfg.host_reset.is_none() {
                " — this chip has no chip_rst, so the guest has no say"
            } else {
                ""
            }
        );
        cx.trace.note(&line);
        if disabled {
            return false;
        }
        let at = cx.now;
        cx.request(MachineRequest::Reset {
            source: "USB_DEVICE chip_rst (serial)",
            at,
            strap,
            cause: ResetSource::ChipReset,
        });
        true
    }

    // ---- the pieces ------------------------------------------------------

    fn in_free(&self) -> bool {
        !self.committed && self.in_free_due.is_none() && self.in_fifo.len() < IN_FIFO_DEPTH
    }

    /// The free lag ends: the send buffer is writable again. Nothing is
    /// raised — the drain already raised `serial_in_empty`.
    fn end_free_lag(&mut self, cx: &mut BusCx<'_>) {
        if self.in_free_due.take().is_none() {
            return;
        }
        let line = format!(
            "cyc={} pc=0x{:08x} USB_DEVICE free lag over: serial_in_ep_data_free = 1",
            cx.now, cx.pc
        );
        cx.trace.note(&line);
    }

    fn out_avail(&self) -> bool {
        !self.out_pkt.is_empty()
    }

    /// Raise raw interrupt bits, holding out any this part does not declare
    /// ([`Config::int_mask`]). A `dtr_chg` on a chip with no modem lines is
    /// a host event with nowhere to land, and this is where it lands nowhere.
    fn raise(&mut self, bits: u32) {
        self.int_raw |= bits & self.cfg.int_mask;
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.int_raw & self.regs.stored(INT_ENA);
        cx.irq.set_level(self.cfg.source, st != 0);
    }

    fn observe(&mut self, bytes: &[u8], cx: &mut BusCx<'_>) {
        if let Some(id) = self.tried {
            cx.host.stream(id).write(bytes);
        }
    }

    /// A USB bus reset: the endpoints return to their default state, which
    /// empties a committed IN packet (modeled). `bus_reset_st` = released.
    fn bus_reset(&mut self, cx: &mut BusCx<'_>) {
        self.raise(INT_USB_BUS_RESET);
        if let Some(hr) = self.cfg.host_reset {
            self.regs.poke(hr.bus_reset_st, hr.bus_reset_st_reset);
        }
        self.fram_num = 0;
        if self.in_free_due.take().is_some() {
            cx.sched.cancel(event_id(self.index, EV_IN_FREE));
        }
        if self.committed {
            let n = self.in_fifo.len();
            self.dropped_packets += 1;
            let bytes = core::mem::take(&mut self.in_fifo);
            self.observe(&bytes, cx);
            self.committed = false;
            self.committed_at = None;
            if self.in_deliver_due.take().is_some() {
                cx.sched.cancel(event_id(self.index, EV_IN_DELIVER));
            }
            self.raise(INT_SERIAL_IN_EMPTY);
            let line = format!(
                "cyc={} pc=0x{:08x} USB_DEVICE bus reset dropped the committed IN packet ({n} \
                 bytes never reached a host; on the observation stream)",
                cx.now, cx.pc
            );
            cx.trace.note(&line);
        }
    }

    fn start_sof(&mut self, from: u64, cx: &mut BusCx<'_>) {
        self.sof_due = from.saturating_add(self.sof_period_cycles);
        cx.sched
            .schedule_at(self.sof_due, event_id(self.index, EV_SOF));
    }

    fn schedule_delivery(&mut self, from: u64, cx: &mut BusCx<'_>) {
        let due = from.saturating_add(self.in_drain_latency_cycles);
        self.in_deliver_due = Some(due);
        cx.sched
            .schedule_at(due, event_id(self.index, EV_IN_DELIVER));
    }

    /// The host took the committed packet: bytes to the host stream, the
    /// FIFO empty, `free` back, `serial_in_empty` and `in_token_rec_in_ep1`
    /// raised.
    fn deliver_in(&mut self, cx: &mut BusCx<'_>) {
        self.in_deliver_due = None;
        if !self.committed {
            return;
        }
        let bytes = core::mem::take(&mut self.in_fifo);
        self.in_delivered += bytes.len() as u64;
        if let Some(id) = self.delivered {
            cx.host.stream(id).write(&bytes);
        }
        self.committed = false;
        self.committed_at = None;
        self.raise(INT_SERIAL_IN_EMPTY | INT_IN_TOKEN_REC_IN_EP1);
        let free = if self.in_free_lag_cycles > 0 {
            let due = cx.now.saturating_add(self.in_free_lag_cycles);
            self.in_free_due = Some(due);
            cx.sched.schedule_at(due, event_id(self.index, EV_IN_FREE));
            format!(
                "serial_in_ep_data_free = 0 for the {}-cycle free lag",
                self.in_free_lag_cycles
            )
        } else {
            "serial_in_ep_data_free = 1".to_string()
        };
        let line = format!(
            "cyc={} pc=0x{:08x} USB_DEVICE IN packet of {} bytes delivered to the host \
             ({free}, serial_in_empty raised)",
            cx.now,
            cx.pc,
            bytes.len()
        );
        cx.trace.note(&line);
        self.update_lines(cx);
    }

    /// Ask the host source for everything it has at `at`, stage it, land a
    /// packet if one can, and schedule the next poll.
    fn poll_source(&mut self, at: u64, cx: &mut BusCx<'_>) {
        let Some(id) = self.delivered else {
            return;
        };
        let ev = event_id(self.index, EV_OUT_POLL);
        let (taken, next_ready, live) = {
            let stream = cx.host.stream(id);
            let mut taken = 0usize;
            while let Some(b) = stream.next_byte(at) {
                self.out_staging.push_back(b);
                taken += 1;
            }
            (taken, stream.next_ready(), stream.is_live())
        };
        if taken > 0 {
            self.try_land(at, cx);
        }
        match next_ready {
            Some(ready) => {
                self.out_poll_due = ready.max(at + 1);
                cx.sched.schedule_at(self.out_poll_due, ev);
            }
            None if live => {
                // A socket: wall clock decides when bytes appear, so poll
                // from the machine's actual time, not the chain's.
                self.out_poll_due = cx.now.max(at).saturating_add(self.cfg.live_poll_cycles);
                cx.sched.schedule_at(self.out_poll_due, ev);
            }
            None => {}
        }
    }

    /// Land the next OUT packet after the latency, if the host is draining,
    /// nothing is resident, nothing is already scheduled, and there is
    /// something to land.
    fn try_land(&mut self, from: u64, cx: &mut BusCx<'_>) {
        if !self.host.draining()
            || self.out_avail()
            || self.out_land_due.is_some()
            || self.out_staging.is_empty()
        {
            return;
        }
        let due = from.saturating_add(self.out_land_latency_cycles);
        self.out_land_due = Some(due);
        cx.sched.schedule_at(due, event_id(self.index, EV_OUT_LAND));
    }

    fn land_out(&mut self, cx: &mut BusCx<'_>) {
        self.out_land_due = None;
        if !self.host.draining() || self.out_avail() {
            return;
        }
        let n = self.out_staging.len().min(OUT_PACKET_MAX);
        self.out_pkt.extend(self.out_staging.drain(..n));
        self.out_landed += n as u64;
        self.out_wr_addr = (n as u32 + OUT_EP_WR_ADDR_BIAS) as u8;
        self.out_rd_addr = 0;
        self.out_rec_cnt = n as u8;
        self.raise(INT_SERIAL_OUT_RECV_PKT);
        let line = format!(
            "cyc={} pc=0x{:08x} USB_DEVICE OUT packet of {n} bytes landed (serial_out_ep_data_avail \
             = 1, serial_out_recv_pkt raised{})",
            cx.now,
            cx.pc,
            if self.out_staging.is_empty() {
                String::new()
            } else {
                format!("; {} more bytes wait behind it", self.out_staging.len())
            }
        );
        cx.trace.note(&line);
        self.update_lines(cx);
    }

    fn pop_out(&mut self, cx: &mut BusCx<'_>) -> u8 {
        match self.out_pkt.pop_front() {
            Some(b) => {
                self.out_rd_addr = self.out_rd_addr.wrapping_add(1);
                if self.out_pkt.is_empty() {
                    self.try_land(cx.now, cx);
                }
                b
            }
            None => {
                // esp-hal only reads while `avail`; a guest that reads past
                // the packet gets 0 (modeled: the PAC does not say).
                if self.out_underruns == 0 {
                    let line = format!(
                        "cyc={} pc=0x{:08x} USB_DEVICE ep1 read with no OUT packet resident: \
                         answered 0 (modeled)",
                        cx.now, cx.pc
                    );
                    cx.trace.note(&line);
                }
                self.out_underruns += 1;
                0
            }
        }
    }

    // ---- registers --------------------------------------------------------

    fn read_word(&self, off: u32) -> u32 {
        match off {
            // `ep1` reads pop the OUT FIFO: a side effect, handled in `read`.
            EP1 => 0,
            EP1_CONF => {
                let mut v = 0;
                if self.in_free() {
                    v |= EP1_CONF_IN_FREE;
                }
                if self.out_avail() {
                    v |= EP1_CONF_OUT_AVAIL;
                }
                v
            }
            INT_RAW => self.int_raw,
            INT_ST => self.int_raw & self.regs.stored(INT_ENA),
            INT_CLR => 0,
            FRAM_NUM => u32::from(self.fram_num),
            IN_EP1_ST => IN_EPN_ST_RESET | ((self.in_fifo.len() as u32) << EP_ST_WR_ADDR_SHIFT),
            OUT_EP1_ST => {
                (u32::from(self.out_wr_addr) << EP_ST_WR_ADDR_SHIFT)
                    | (u32::from(self.out_rd_addr) << EP_ST_RD_ADDR_SHIFT)
                    | (u32::from(self.out_rec_cnt) << OUT_EP_REC_CNT_SHIFT)
            }
            other => self.regs.effective(other),
        }
    }

    fn push_in(&mut self, byte: u8, cx: &mut BusCx<'_>) {
        if self.in_free() {
            self.in_fifo.push(byte);
            if !self.host.attached() {
                // What the guest tried to print with nobody there — an
                // observation, not guest output that reached anyone.
                self.observe(&[byte], cx);
            }
            // **The block commits a full FIFO by itself.** esp-hal says so on
            // the only API that writes this register — `write_byte_nb`:
            // "Requires manual flushing (automatically flushed every 64
            // bytes)" (`esp-hal-1.1.1/src/usb_serial_jtag.rs:191-192`) — and
            // a driver written against that doc will otherwise spin for ever
            // on a full endpoint nothing will ever send.
            //
            // Found by M5 P3: the `rmt-chase` harness's `Esp32UsbSerialIo`
            // writes byte by byte and flushes only at the end of a write, so
            // any log line longer than 64 bytes stalled here until its 250 ms
            // drain timeout and then dropped its tail — losing the payload's
            // first record. Nothing had hit it before because the shipped
            // image's `ChunkedWriter` chunks below 64 and writes `wr_done`
            // itself, which is the one path M6 measured.
            if self.in_fifo.len() >= IN_FIFO_DEPTH {
                self.wr_done(cx);
            }
            return;
        }
        let in_lag = !self.committed && self.in_free_due.is_some();
        if self.dropped == 0 || (in_lag && self.dropped_in_lag == 0) {
            let line = format!(
                "cyc={} pc=0x{:08x} USB_DEVICE ep1 write with the IN FIFO {} (host {}): byte \
                 0x{byte:02x} dropped",
                cx.now,
                cx.pc,
                if self.committed {
                    "committed"
                } else if in_lag {
                    "inside the free lag"
                } else {
                    "full"
                },
                self.host
            );
            cx.trace.note(&line);
        }
        if in_lag {
            self.dropped_in_lag += 1;
        }
        self.dropped += 1;
        self.observe(&[byte], cx);
    }

    fn wr_done(&mut self, cx: &mut BusCx<'_>) {
        if self.in_fifo.is_empty() {
            // Committing nothing: the endpoint is empty at once.
            self.raise(INT_SERIAL_IN_EMPTY);
        } else if !self.committed {
            self.committed = true;
            self.committed_at = Some(cx.now);
            let n = self.in_fifo.len();
            let line = match self.host {
                HostState::Attached { draining: true } => {
                    self.schedule_delivery(cx.now, cx);
                    format!(
                        "cyc={} pc=0x{:08x} USB_DEVICE wr_done: {n} bytes committed to the IN \
                         endpoint; the host takes them in {IN_DRAIN_LATENCY_US} us",
                        cx.now, cx.pc
                    )
                }
                HostState::Attached { draining: false } => format!(
                    "cyc={} pc=0x{:08x} USB_DEVICE wr_done: {n} bytes committed to the IN \
                     endpoint; the host is attached but not draining (port closed): \
                     serial_in_ep_data_free stays 0 until it opens",
                    cx.now, cx.pc
                ),
                HostState::Absent => format!(
                    "cyc={} pc=0x{:08x} USB_DEVICE wr_done: {n} bytes committed to the IN \
                     endpoint; no host will drain them (serial_in_ep_data_free stays 0)",
                    cx.now, cx.pc
                ),
            };
            cx.trace.note(&line);
        }
        self.update_lines(cx);
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            EP1 => self.push_in((value & 0xff) as u8, cx),
            EP1_CONF => {
                if value & EP1_CONF_WR_DONE != 0 {
                    self.wr_done(cx);
                }
            }
            INT_ENA => {
                self.regs.poke(INT_ENA, value & self.cfg.int_mask);
                self.update_lines(cx);
            }
            INT_CLR => {
                self.int_raw &= !(value & self.cfg.int_mask);
                self.update_lines(cx);
            }
            INT_RAW | INT_ST | FRAM_NUM | IN_EP1_ST | OUT_EP1_ST => {}
            // `chip_rst`, where the part has it: bits 0 and 1 are
            // write-one-to-clear, bit 2 is stored. On a part without it this
            // offset is reserved and falls through to accept-and-remember
            // like any other word the chip's table does not name.
            other if self.cfg.host_reset.is_some_and(|hr| hr.chip_rst == other) => {
                let old = self.regs.stored(other);
                let cleared = old & !(value & (CHIP_RST_SERIAL | CHIP_RST_JTAG));
                let new = (cleared & !CHIP_RST_DISABLE) | (value & CHIP_RST_DISABLE);
                self.regs.poke(other, new);
            }
            other => self.regs.poke(other, value),
        }
    }
}

impl Peripheral for UsbSerialJtag {
    fn name(&self) -> &'static str {
        "USB_DEVICE"
    }

    fn attached(&mut self, index: usize) {
        self.index = index;
    }

    fn started(&mut self, cx: &mut BusCx<'_>) {
        if self.host.attached() {
            // The cable was in at power-on: the host's bus reset and its
            // frames are already there when the guest first looks.
            self.bus_reset(cx);
            self.start_sof(cx.now, cx);
        }
        self.poll_source(cx.now, cx);
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        let word = off & !3;
        if word == EP1 {
            // A pop is a side effect: only lane 0 carries the byte.
            if off & 3 == 0 {
                let byte = self.pop_out(cx);
                return lane_of(u32::from(byte), off, width);
            }
            return 0;
        }
        lane_of(self.read_word(word), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        if word == EP1 {
            if off & 3 == 0 {
                self.write_word(EP1, value & 0xff, cx);
            }
            return;
        }
        let merged = merge_lane(self.read_word(word), off, width, value);
        self.write_word(word, merged, cx);
    }

    fn on_event(&mut self, id: EventId, cx: &mut BusCx<'_>) {
        match event_local(id) {
            EV_SOF => {
                if !self.host.attached() {
                    return;
                }
                self.raise(INT_SOF);
                self.fram_num = (self.fram_num + 1) & 0x7ff;
                self.sof_due = self.sof_due.saturating_add(self.sof_period_cycles);
                cx.sched
                    .schedule_at(self.sof_due, event_id(self.index, EV_SOF));
                self.update_lines(cx);
            }
            EV_IN_DELIVER => {
                if self.in_deliver_due.is_some() {
                    self.deliver_in(cx);
                }
            }
            EV_OUT_POLL => {
                let due = self.out_poll_due;
                self.poll_source(due, cx);
            }
            EV_OUT_LAND => {
                if self.out_land_due.is_some() {
                    self.land_out(cx);
                }
            }
            EV_IN_FREE => self.end_free_lag(cx),
            _ => {}
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        self.cfg.regs.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        Some(self.grades.grade(off))
    }

    /// The one block the machine drives from outside the guest: M6 P3's
    /// control channel calls [`attach`](Self::attach) and its siblings
    /// through [`crate::SocBus::with_peripheral`].
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.cfg.len as usize + 3 * IN_FIFO_DEPTH + 128);
        let host = match self.host {
            HostState::Absent => 0u32,
            HostState::Attached { draining: false } => 1,
            HostState::Attached { draining: true } => 2,
        };
        let opt = |v: Option<u64>| v.unwrap_or(u64::MAX);
        let signals = self.last_dtr.map_or(0, |d| 0b01 | (u32::from(d) << 1))
            | (self.last_rts.map_or(0, |r| 0b01 | (u32::from(r) << 1)) << 2)
            | (u32::from(self.dtr_high_seen) << 4);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&host.to_le_bytes());
        out.extend_from_slice(&self.int_raw.to_le_bytes());
        out.extend_from_slice(&u32::from(self.committed).to_le_bytes());
        out.extend_from_slice(&self.dropped.to_le_bytes());
        out.extend_from_slice(&self.dropped_packets.to_le_bytes());
        out.extend_from_slice(&self.in_delivered.to_le_bytes());
        out.extend_from_slice(&opt(self.committed_at).to_le_bytes());
        out.extend_from_slice(&opt(self.in_deliver_due).to_le_bytes());
        out.extend_from_slice(&u32::from(self.fram_num).to_le_bytes());
        out.extend_from_slice(&self.sof_due.to_le_bytes());
        out.extend_from_slice(&opt(self.out_land_due).to_le_bytes());
        out.extend_from_slice(&self.out_poll_due.to_le_bytes());
        out.extend_from_slice(
            &(u32::from(self.out_wr_addr)
                | (u32::from(self.out_rd_addr) << 8)
                | (u32::from(self.out_rec_cnt) << 16))
                .to_le_bytes(),
        );
        out.extend_from_slice(&self.out_underruns.to_le_bytes());
        out.extend_from_slice(&self.out_landed.to_le_bytes());
        out.extend_from_slice(&signals.to_le_bytes());
        out.extend_from_slice(&self.in_free_lag_cycles.to_le_bytes());
        out.extend_from_slice(&opt(self.in_free_due).to_le_bytes());
        out.extend_from_slice(&self.dropped_in_lag.to_le_bytes());
        out.extend_from_slice(&(self.in_fifo.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.in_fifo);
        out.extend_from_slice(&(self.out_pkt.len() as u32).to_le_bytes());
        out.extend(self.out_pkt.iter());
        out.extend_from_slice(&(self.out_staging.len() as u32).to_le_bytes());
        out.extend(self.out_staging.iter());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let (Some(index), Some(host), Some(int_raw), Some(committed), Some(dropped)) =
            (r.u64(), r.u32(), r.u32(), r.u32(), r.u64())
        else {
            log::warn!("USB_DEVICE: load_state blob too short, ignored");
            return;
        };
        let (Some(dropped_packets), Some(in_delivered), Some(committed_at), Some(deliver_due)) =
            (r.u64(), r.u64(), r.u64(), r.u64())
        else {
            log::warn!("USB_DEVICE: load_state blob too short, ignored");
            return;
        };
        let (Some(fram_num), Some(sof_due), Some(out_land_due), Some(out_poll_due)) =
            (r.u32(), r.u64(), r.u64(), r.u64())
        else {
            log::warn!("USB_DEVICE: load_state blob too short, ignored");
            return;
        };
        let (Some(addrs), Some(out_underruns), Some(out_landed), Some(signals)) =
            (r.u32(), r.u64(), r.u64(), r.u32())
        else {
            log::warn!("USB_DEVICE: load_state blob too short, ignored");
            return;
        };
        let (Some(in_free_lag), Some(in_free_due), Some(dropped_in_lag)) = (r.u64(), r.u64(), r.u64())
        else {
            log::warn!("USB_DEVICE: load_state blob too short, ignored");
            return;
        };
        let take = |r: &mut Reader<'_>| -> Option<Vec<u8>> {
            let len = r.u32()? as usize;
            if r.0.len() < len {
                return None;
            }
            let (head, rest) = r.0.split_at(len);
            r.0 = rest;
            Some(head.to_vec())
        };
        let (Some(in_fifo), Some(out_pkt), Some(out_staging)) =
            (take(&mut r), take(&mut r), take(&mut r))
        else {
            log::warn!("USB_DEVICE: load_state blob too short, ignored");
            return;
        };
        let opt = |v: u64| (v != u64::MAX).then_some(v);
        let line = |bits: u32| (bits & 1 != 0).then_some(bits & 2 != 0);
        self.index = index as usize;
        self.host = match host {
            1 => HostState::Attached { draining: false },
            2 => HostState::Attached { draining: true },
            _ => HostState::Absent,
        };
        self.int_raw = int_raw;
        self.committed = committed != 0;
        self.dropped = dropped;
        self.dropped_packets = dropped_packets;
        self.in_delivered = in_delivered;
        self.committed_at = opt(committed_at);
        self.in_deliver_due = opt(deliver_due);
        self.fram_num = fram_num as u16;
        self.sof_due = sof_due;
        self.out_land_due = opt(out_land_due);
        self.out_poll_due = out_poll_due;
        self.out_wr_addr = addrs as u8;
        self.out_rd_addr = (addrs >> 8) as u8;
        self.out_rec_cnt = (addrs >> 16) as u8;
        self.out_underruns = out_underruns;
        self.out_landed = out_landed;
        self.last_dtr = line(signals);
        self.last_rts = line(signals >> 2);
        self.dtr_high_seen = signals & (1 << 4) != 0;
        self.in_free_lag_cycles = in_free_lag;
        self.in_free_due = opt(in_free_due);
        self.dropped_in_lag = dropped_in_lag;
        self.in_fifo = in_fifo;
        self.out_pkt = out_pkt.into();
        self.out_staging = out_staging.into();
        self.regs.load_state(r.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::MemorySink;
    use crate::regnames::Access;
    use crate::{ByteLog, Sandbox, ScriptedSource, SocBus};
    use lp_emu_core::Bus;

    /// `jfifo_st`, the one register in this block that no driver on either
    /// chip touches — which is what makes it the strict-grade test's
    /// example. Only tests name it, so it lives here.
    const JFIFO_ST: u32 = 0x20;
    const CHIP_RST: u32 = 0x4c;
    const BUS_RESET_ST: u32 = 0x68;

    /// **The shared layout, as a fixture** — the twenty registers the module
    /// docs verify are identical on both parts, with the reset values both
    /// PACs agree on, plus the eight the C6 has and the S3 does not so the
    /// capability can be tested from both sides.
    ///
    /// ⚠️ It is a fixture and not a chip: [`CFG`]'s base, aperture, source
    /// and clock below are **invented numbers**, chosen so that nothing here
    /// could be mistaken for a part. The chips' own generated tables live in
    /// the chip crates, where a register layout belongs, and each chip's own
    /// tests check that its table is the one this file was moved against.
    static FIXTURE_REGS: RegNames = RegNames {
        block: "usb_device",
        entries: &[
            (0x000, "ep1"),
            (0x004, "ep1_conf"),
            (0x008, "int_raw"),
            (0x00c, "int_st"),
            (0x010, "int_ena"),
            (0x014, "int_clr"),
            (0x018, "conf0"),
            (0x01c, "test"),
            (0x020, "jfifo_st"),
            (0x024, "fram_num"),
            (0x028, "in_ep0_st"),
            (0x02c, "in_ep1_st"),
            (0x030, "in_ep2_st"),
            (0x034, "in_ep3_st"),
            (0x038, "out_ep0_st"),
            (0x03c, "out_ep1_st"),
            (0x040, "out_ep2_st"),
            (0x044, "misc_conf"),
            (0x048, "mem_conf"),
            (0x04c, "chip_rst"),
            (0x050, "set_line_code_w0"),
            (0x054, "set_line_code_w1"),
            (0x058, "get_line_code_w0"),
            (0x05c, "get_line_code_w1"),
            (0x060, "config_update"),
            (0x064, "ser_afifo_config"),
            (0x068, "bus_reset_st"),
            (0x080, "date"),
        ],
        resets: &[
            (0x004, 0x0000_0002),
            (0x008, 0x0000_0008),
            (0x018, 0x0000_4200),
            (0x020, 0x0000_0044),
            (0x028, 0x0000_0001),
            (0x02c, 0x0000_0001),
            (0x030, 0x0000_0001),
            (0x034, 0x0000_0001),
            (0x048, 0x0000_0002),
            (0x068, 0x0000_0001),
        ],
        access: &[
            (0x00c, Access::ReadOnly),
            (0x014, Access::WriteOnly),
            (0x024, Access::ReadOnly),
            (0x028, Access::ReadOnly),
            (0x02c, Access::ReadOnly),
            (0x030, Access::ReadOnly),
            (0x034, Access::ReadOnly),
            (0x038, Access::ReadOnly),
            (0x03c, Access::ReadOnly),
            (0x040, Access::ReadOnly),
            (0x068, Access::ReadOnly),
        ],
    };

    /// The same table with everything from `chip_rst` on removed: a part
    /// whose `0x4c`…`0x7c` is reserved.
    static FIXTURE_REGS_NO_HOST_RESET: RegNames = RegNames {
        block: "usb_device",
        entries: &[
            (0x000, "ep1"),
            (0x004, "ep1_conf"),
            (0x008, "int_raw"),
            (0x00c, "int_st"),
            (0x010, "int_ena"),
            (0x014, "int_clr"),
            (0x018, "conf0"),
            (0x01c, "test"),
            (0x020, "jfifo_st"),
            (0x024, "fram_num"),
            (0x028, "in_ep0_st"),
            (0x02c, "in_ep1_st"),
            (0x030, "in_ep2_st"),
            (0x034, "in_ep3_st"),
            (0x038, "out_ep0_st"),
            (0x03c, "out_ep1_st"),
            (0x040, "out_ep2_st"),
            (0x044, "misc_conf"),
            (0x048, "mem_conf"),
            (0x080, "date"),
        ],
        resets: &[
            (0x004, 0x0000_0002),
            (0x008, 0x0000_0008),
            (0x018, 0x0000_4200),
            (0x020, 0x0000_0044),
            (0x028, 0x0000_0001),
            (0x02c, 0x0000_0001),
            (0x030, 0x0000_0001),
            (0x034, 0x0000_0001),
            (0x048, 0x0000_0002),
        ],
        access: &[
            (0x00c, Access::ReadOnly),
            (0x014, Access::WriteOnly),
            (0x024, Access::ReadOnly),
            (0x028, Access::ReadOnly),
            (0x02c, Access::ReadOnly),
            (0x030, Access::ReadOnly),
            (0x034, Access::ReadOnly),
            (0x038, Access::ReadOnly),
            (0x03c, Access::ReadOnly),
            (0x040, Access::ReadOnly),
        ],
    };

    /// The promotions the four committed C6 transcripts bought, used here to
    /// exercise the grade plumbing. A chip states its own.
    const FIXTURE_GRADES: &[(u32, RegGrade)] = &[
        (EP1, RegGrade::Measured),
        (EP1_CONF, RegGrade::Measured),
        (INT_RAW, RegGrade::Measured),
        (INT_ST, RegGrade::Measured),
        (INT_ENA, RegGrade::Measured),
        (INT_CLR, RegGrade::Measured),
        (FRAM_NUM, RegGrade::Documented),
        (CONF0, RegGrade::Documented),
    ];

    /// Invented numbers: no part has this base, this source or this clock.
    const CYCLES_PER_US: u64 = 100;
    const BASE: u32 = 0x1000_0000;

    /// A part **with** the host-reset pair.
    static CFG: Config = Config {
        base: BASE,
        len: 0x100,
        source: 7,
        regs: &FIXTURE_REGS,
        cycles_per_us: CYCLES_PER_US,
        int_mask: INT_MASK_WITH_CDC,
        live_poll_cycles: 1_000 * CYCLES_PER_US,
        grades: FIXTURE_GRADES,
        host_reset: Some(HostReset {
            chip_rst: CHIP_RST,
            bus_reset_st: BUS_RESET_ST,
            bus_reset_st_reset: 0x01,
        }),
    };

    /// A part **without** it: no `chip_rst`, no `bus_reset_st`, no CDC or
    /// modem-line interrupt bits. The S3's shape.
    static CFG_NO_HOST_RESET: Config = Config {
        base: BASE,
        len: 0x100,
        source: 7,
        regs: &FIXTURE_REGS_NO_HOST_RESET,
        cycles_per_us: CYCLES_PER_US,
        int_mask: INT_MASK_CORE,
        live_poll_cycles: 1_000 * CYCLES_PER_US,
        grades: &[],
        host_reset: None,
    };

    const SOURCE: u16 = CFG.source;

    /// A sandbox with the `usb-sj` stream (delivered + a scripted source)
    /// and the observation stream, and a USB block on them in `host`.
    struct Rig {
        sb: Sandbox,
        u: UsbSerialJtag,
        delivered: ByteLog,
        tried: ByteLog,
    }

    fn rig_with(host: HostState, script: ScriptedSource) -> Rig {
        rig_cfg(&CFG, host, script)
    }

    fn rig_cfg(cfg: &'static Config, host: HostState, script: ScriptedSource) -> Rig {
        let mut sb = Sandbox::new();
        let delivered = ByteLog::new();
        let id = sb.host.add(
            "usb-sj",
            Box::new(MemorySink(delivered.clone())),
            Box::new(script),
        );
        let (tried_id, tried) = sb.host.add_memory("usb-sj-tried");
        let mut u = UsbSerialJtag::new(cfg, Some(id), Some(tried_id), host);
        u.attached(7);
        u.started(&mut sb.cx());
        Rig {
            sb,
            u,
            delivered,
            tried,
        }
    }

    /// P6's rig: no host; the log handed back is the observation one.
    fn rig() -> (Sandbox, UsbSerialJtag, ByteLog) {
        let r = rig_with(HostState::Absent, ScriptedSource::new());
        (r.sb, r.u, r.tried)
    }

    const MS: u64 = 1_000 * CYCLES_PER_US;
    const IN_DRAIN_LATENCY_CYCLES: u64 = IN_DRAIN_LATENCY_US * CYCLES_PER_US;
    const OUT_LAND_LATENCY_CYCLES: u64 = OUT_LAND_LATENCY_US * CYCLES_PER_US;

    /// esp-println's `write_bytes_in_cs`, byte for byte, with its 50,000
    /// spin counted. Returns whether `TIMED_OUT` latched.
    fn esp_println_write(
        sb: &mut Sandbox,
        u: &mut UsbSerialJtag,
        bytes: &[u8],
        timed_out: &mut bool,
    ) {
        let fifo_full = |sb: &mut Sandbox, u: &mut UsbSerialJtag| sb.read(u, EP1_CONF) & 0b010 == 0;
        if fifo_full(sb, u) && *timed_out {
            return;
        }
        for &b in bytes {
            if fifo_full(sb, u) {
                sb.write(u, EP1_CONF, 0b001);
                let mut timeout = 50_000u32;
                while fifo_full(sb, u) {
                    if timeout == 0 {
                        *timed_out = true;
                        return;
                    }
                    timeout -= 1;
                }
            }
            sb.write(u, EP1, u32::from(b));
        }
        *timed_out = false;
    }

    /// The same printer with guest time passing: each `fifo_full()` poll
    /// costs [`POLL_CYCLES`] (a five-instruction loop under `t1`) and the
    /// due events run. Returns the longest spin any wait took.
    const POLL_CYCLES: u64 = 5;

    fn esp_println_write_timed(
        sb: &mut Sandbox,
        u: &mut UsbSerialJtag,
        bytes: &[u8],
        timed_out: &mut bool,
    ) -> u32 {
        let mut longest = 0u32;
        let fifo_full = |sb: &mut Sandbox, u: &mut UsbSerialJtag| {
            let now = sb.now + POLL_CYCLES;
            sb.run_to(u, now);
            sb.read(u, EP1_CONF) & 0b010 == 0
        };
        if fifo_full(sb, u) && *timed_out {
            return 0;
        }
        for &b in bytes {
            if fifo_full(sb, u) {
                sb.write(u, EP1_CONF, 0b001);
                let mut timeout = 50_000u32;
                while fifo_full(sb, u) {
                    if timeout == 0 {
                        *timed_out = true;
                        return longest;
                    }
                    timeout -= 1;
                }
                longest = longest.max(50_000 - timeout);
            }
            sb.write(u, EP1, u32::from(b));
        }
        // `println!` ends in `Printer::flush` = `fifo_flush()`.
        sb.write(u, EP1_CONF, 0b001);
        *timed_out = false;
        longest
    }

    // ---- P6's five, unchanged, under `HostState::Absent` ------------------

    #[test]
    fn the_reset_registers_say_free_empty_and_no_sof() {
        let (mut sb, mut u, _) = rig();
        assert_eq!(sb.read(&mut u, EP1_CONF), 0x02, "the PAC reset");
        assert_eq!(
            sb.read(&mut u, INT_RAW),
            0x08,
            "serial_in_empty, the PAC reset"
        );
        assert_eq!(sb.read(&mut u, CONF0), 0x4200);
        assert_eq!(sb.read(&mut u, IN_EP1_ST), 0x01);
        assert_eq!(sb.read(&mut u, BUS_RESET_ST), 0x01);
        assert_eq!(sb.read(&mut u, EP1), 0, "no OUT data");
        assert_eq!(u.reg_name(0x04), Some("ep1_conf"));
        assert_eq!(sb.read(&mut u, FRAM_NUM), 0);
        assert_eq!(sb.read(&mut u, OUT_EP1_ST), 0);
        assert_eq!(sb.sched.live(), 0, "no host: nothing scheduled");

        // And every other register the PAC gives a non-zero reset reads it,
        // seeded by `with_names` rather than listed here — the four this
        // block computes from the link's own state are the exceptions, and
        // the lines above are what checks those.
        let computed = [EP1_CONF, INT_RAW, IN_EP1_ST, OUT_EP1_ST];
        for (off, want) in FIXTURE_REGS.resets {
            if computed.contains(off) {
                continue;
            }
            assert_eq!(
                sb.read(&mut u, *off),
                *want,
                "USB_DEVICE+{off:#05x} {}",
                FIXTURE_REGS.name(*off).unwrap_or("?")
            );
        }
    }

    #[test]
    fn esp_println_fills_64_bytes_spins_once_latches_timed_out_and_falls_silent() {
        let (mut sb, mut u, log) = rig();
        let mut timed_out = false;
        let line = b"[INIT] a line of esp-println output that is longer than sixty-four bytes\n";
        esp_println_write(&mut sb, &mut u, line, &mut timed_out);
        assert!(timed_out, "the 50,000-iteration wait ended in TIMED_OUT");
        assert!(u.committed());
        assert_eq!(u.in_fifo().len(), 64);
        assert_eq!(log.bytes(), &line[..64], "the sink holds what was tried");
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b010, 0, "free stays 0");
        // Every later print returns at once: nothing more reaches the FIFO.
        esp_println_write(&mut sb, &mut u, b"[INIT] more\n", &mut timed_out);
        assert!(timed_out);
        assert_eq!(log.len(), 64);
        assert_eq!(u.dropped(), 0, "esp-println never writes past a full FIFO");
        // A guest that did would be dropped and noted.
        sb.write(&mut u, EP1, 0x41);
        assert_eq!(u.dropped(), 1);
    }

    #[test]
    fn sof_never_arrives_and_the_monitor_clears_nothing() {
        let (mut sb, mut u, _) = rig();
        for i in 0..10u64 {
            sb.run_to(&mut u, i * 2 * MS);
            assert_eq!(sb.read(&mut u, INT_RAW) & INT_SOF, 0);
            sb.write(&mut u, INT_CLR, INT_SOF);
        }
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b100, 0, "no OUT data avail");
        assert_eq!(u.fram_num(), 0);
    }

    #[test]
    fn serial_in_empty_resolves_one_write_then_never_again_and_source_48_follows() {
        let (mut sb, mut u, _) = rig();
        // esp-hal `new`: disable both interrupts (int_clr + int_ena).
        sb.write(
            &mut u,
            INT_CLR,
            INT_SERIAL_IN_EMPTY | INT_SERIAL_OUT_RECV_PKT,
        );
        sb.write(&mut u, INT_ENA, 0);
        assert_eq!(sb.read(&mut u, INT_RAW), 0);
        // The probe write: byte, wr_done, listen for serial_in_empty.
        sb.write(&mut u, EP1, u32::from(b'\n'));
        sb.write(&mut u, EP1_CONF, EP1_CONF_WR_DONE);
        sb.write(&mut u, INT_ENA, INT_SERIAL_IN_EMPTY);
        assert!(
            !sb.irq.level(SOURCE),
            "committed and never drained: not empty, no interrupt"
        );
        sb.run_to(&mut u, 300 * MS);
        assert!(!sb.irq.level(SOURCE), "and not after 300 ms");
        // A committed-but-empty flush *is* empty at once.
        let (mut sb2, mut u2, _) = rig();
        sb2.write(&mut u2, INT_CLR, INT_SERIAL_IN_EMPTY);
        sb2.write(&mut u2, EP1_CONF, EP1_CONF_WR_DONE);
        sb2.write(&mut u2, INT_ENA, INT_SERIAL_IN_EMPTY);
        assert!(sb2.irq.level(SOURCE));
        assert_eq!(sb2.read(&mut u2, INT_ST), INT_SERIAL_IN_EMPTY);
        // The async ISR: drop the enable, clear the raw.
        sb2.write(&mut u2, INT_ENA, 0);
        sb2.write(&mut u2, INT_CLR, INT_SERIAL_IN_EMPTY);
        assert!(!sb2.irq.level(SOURCE));
        // The connected path never arms RX; if a guest did, nothing fires.
        sb2.write(&mut u2, INT_ENA, INT_SERIAL_OUT_RECV_PKT);
        assert!(!sb2.irq.level(SOURCE));
    }

    #[test]
    fn the_state_round_trips() {
        let (mut sb, mut u, _) = rig();
        sb.write(&mut u, EP1, 0x41);
        sb.write(&mut u, EP1_CONF, EP1_CONF_WR_DONE);
        sb.write(&mut u, INT_ENA, INT_SERIAL_IN_EMPTY);
        let blob = u.save_state();
        let mut other = UsbSerialJtag::absent(&CFG, None);
        other.load_state(&blob);
        assert!(other.committed());
        assert_eq!(other.in_fifo(), b"A");
        assert_eq!(other.regs.stored(INT_ENA), INT_SERIAL_IN_EMPTY);
        assert_eq!(other.int_raw, u.int_raw);
        assert_eq!(other.host(), HostState::Absent);
    }

    // ---- the attached host --------------------------------------------------

    /// 875 bytes of `[INIT]` lines, as the shipped image prints them: a mix
    /// of lengths, some past 64 bytes.
    fn init_lines() -> Vec<u8> {
        let mut out = Vec::new();
        let lines = [
            "[INIT] Initializing board...\n",
            "[INIT] Board initialized: seeed/xiao-esp32-c6 rev 0.2, two WS281x outputs declared\n",
            "[INIT] Recovery region checked: cause=power-on prior_boot_complete=false\n",
            "[INIT] Runtime started\n",
            "[INIT] fw-esp32 starting (esp32c6, server, radio, memory_fs)\n",
            "[INIT] Spawning I/O task on the USB-Serial-JTAG link with a 100 ms settle\n",
            "[INIT] I/O task spawned\n",
            "[INIT] Logger installed: log lines go to the outgoing queue from here on\n",
            "[INIT] ESP-NOW radio bring-up: calibrating, then channel 11\n",
            "[INIT] RMT: 2 WS281x channels for 2 declared outputs, 48-word blocks each\n",
            "[INIT] Server loop entered; first frame pending\n",
            "[INIT] Stack painted: 71960 bytes watched for the heartbeat's high-water line\n",
            "[INIT] done\n",
        ];
        for l in lines {
            out.extend_from_slice(l.as_bytes());
        }
        // Pad to exactly 875 bytes with one more line.
        while out.len() < 874 {
            out.push(b'.');
        }
        out.push(b'\n');
        assert_eq!(out.len(), 875);
        out
    }

    #[test]
    fn esp_println_with_a_draining_host_crosses_the_fifo_in_packets_and_never_times_out() {
        let Rig {
            mut sb,
            mut u,
            delivered,
            tried,
        } = rig_with(
            HostState::Attached { draining: true },
            ScriptedSource::new(),
        );
        let input = init_lines();
        let mut timed_out = false;
        let mut longest = 0;
        for line in input.split_inclusive(|&b| b == b'\n') {
            longest = longest.max(esp_println_write_timed(
                &mut sb,
                &mut u,
                line,
                &mut timed_out,
            ));
            assert!(!timed_out, "TIMED_OUT latched on {line:?}");
        }
        // The last flush is still in flight.
        let end = sb.now + IN_DRAIN_LATENCY_CYCLES;
        sb.run_to(&mut u, end);
        assert_eq!(
            delivered.bytes(),
            input,
            "the delivered log equals the input"
        );
        assert!(tried.is_empty(), "nothing was merely tried");
        assert_eq!(u.in_delivered(), 875);
        assert_eq!(u.dropped(), 0);
        let latency_polls = (IN_DRAIN_LATENCY_CYCLES / POLL_CYCLES) as u32;
        assert!(
            longest > 0 && longest <= latency_polls + 1,
            "the longest spin was {longest} polls; the latency is {latency_polls}"
        );
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b010, 0b010, "free again");
    }

    #[test]
    fn esp_hal_write_async_completes_through_serial_in_empty_and_the_isr() {
        let Rig {
            mut sb,
            mut u,
            delivered,
            ..
        } = rig_with(
            HostState::Attached { draining: true },
            ScriptedSource::new(),
        );
        // `UsbSerialJtag::new`: both interrupts off.
        sb.write(
            &mut u,
            INT_CLR,
            INT_SERIAL_IN_EMPTY | INT_SERIAL_OUT_RECV_PKT,
        );
        sb.write(&mut u, INT_ENA, 0);
        // `write_async`: push a 64-byte chunk, wr_done, the future arms bit 3.
        let chunk: Vec<u8> = (0..64u8).map(|i| b'a' + i % 26).collect();
        for &b in &chunk {
            sb.write(&mut u, EP1, u32::from(b));
        }
        sb.write(&mut u, EP1_CONF, EP1_CONF_WR_DONE);
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b010, 0, "committed: not free");
        assert_eq!(sb.read(&mut u, IN_EP1_ST), 0x01 | (64 << 2));
        let armed = sb.read(&mut u, INT_ENA) | INT_SERIAL_IN_EMPTY;
        sb.write(&mut u, INT_ENA, armed);
        assert!(!sb.irq.level(SOURCE));
        let committed_at = sb.now;
        sb.run_to(&mut u, committed_at + IN_DRAIN_LATENCY_CYCLES - 1);
        assert!(!sb.irq.level(SOURCE), "not before the latency");
        assert!(delivered.is_empty());
        sb.run_to(&mut u, committed_at + IN_DRAIN_LATENCY_CYCLES);
        assert!(sb.irq.level(SOURCE), "the block's source is high");
        assert_eq!(sb.read(&mut u, INT_ST), INT_SERIAL_IN_EMPTY);
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_IN_TOKEN_REC_IN_EP1,
            INT_IN_TOKEN_REC_IN_EP1,
            "the host polled us"
        );
        assert_eq!(delivered.bytes(), chunk);
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b010, 0b010, "free");
        assert_eq!(sb.read(&mut u, IN_EP1_ST), 0x01, "addresses reset");
        // The ISR: clear the enable, clear the raw, wake.
        let ena = sb.read(&mut u, INT_ENA) & !INT_SERIAL_IN_EMPTY;
        sb.write(&mut u, INT_ENA, ena);
        sb.write(
            &mut u,
            INT_CLR,
            INT_SERIAL_IN_EMPTY | INT_SERIAL_OUT_RECV_PKT,
        );
        assert!(!sb.irq.level(SOURCE), "line low");
        assert_eq!(
            sb.read(&mut u, INT_ENA) & INT_SERIAL_IN_EMPTY,
            0,
            "the future's Ready"
        );
    }

    #[test]
    fn attached_idle_holds_the_packet_until_the_port_opens() {
        let Rig {
            mut sb,
            mut u,
            delivered,
            tried,
        } = rig_with(
            HostState::Attached { draining: false },
            ScriptedSource::new(),
        );
        sb.write(&mut u, INT_CLR, INT_SERIAL_IN_EMPTY);
        for &b in b"[INIT] Initializing board...\n" {
            sb.write(&mut u, EP1, u32::from(b));
        }
        sb.write(&mut u, EP1_CONF, EP1_CONF_WR_DONE);
        sb.write(&mut u, INT_ENA, INT_SERIAL_IN_EMPTY);
        sb.run_to(&mut u, 300 * MS);
        assert_eq!(
            sb.read(&mut u, EP1_CONF) & 0b010,
            0,
            "free stays 0 for 300 ms"
        );
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_SERIAL_IN_EMPTY, 0);
        assert!(!sb.irq.level(SOURCE));
        assert!(delivered.is_empty() && tried.is_empty());
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_SOF,
            INT_SOF,
            "the cable is in: frames arrive"
        );

        u.open(&mut sb.cx());
        assert_eq!(u.host(), HostState::Attached { draining: true });
        let opened = sb.now;
        sb.run_to(&mut u, opened + IN_DRAIN_LATENCY_CYCLES);
        assert_eq!(delivered.text(), "[INIT] Initializing board...\n");
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b010, 0b010, "free = 1");
        assert_eq!(
            sb.read(&mut u, INT_ST),
            INT_SERIAL_IN_EMPTY,
            "bit 3 raised, the dropped future's enable still set"
        );
        assert!(sb.irq.level(SOURCE));
    }

    #[test]
    fn the_250ms_write_timeout_shape_as_the_firmware_sees_it() {
        let Rig {
            mut sb,
            mut u,
            delivered,
            tried,
        } = rig_with(
            HostState::Attached { draining: false },
            ScriptedSource::new(),
        );
        sb.write(&mut u, INT_CLR, INT_SERIAL_IN_EMPTY);
        sb.write(&mut u, INT_ENA, 0);
        let chunk = |sb: &mut Sandbox, u: &mut UsbSerialJtag, fill: u8| {
            // `write_async`: 64 bytes, wr_done, arm bit 3.
            for _ in 0..64 {
                sb.write(u, EP1, u32::from(fill));
            }
            sb.write(u, EP1_CONF, EP1_CONF_WR_DONE);
            let ena = sb.read(u, INT_ENA) | INT_SERIAL_IN_EMPTY;
            sb.write(u, INT_ENA, ena);
        };
        // The hello's first chunk: committed, 250 ms pass, the future is
        // dropped with its enable bit set.
        chunk(&mut sb, &mut u, b'1');
        sb.run_to(&mut u, 250 * MS);
        assert!(!sb.irq.level(SOURCE), "first timeout");
        assert_eq!(sb.read(&mut u, INT_ENA), INT_SERIAL_IN_EMPTY);
        // The retry, 250 ms later: the FIFO is still committed, so the
        // pushes are dropped, the wr_done is a no-op, and the wait times out
        // again — the second timeout latches "not draining".
        chunk(&mut sb, &mut u, b'2');
        assert_eq!(u.dropped(), 64);
        assert_eq!(tried.bytes(), vec![b'2'; 64]);
        sb.run_to(&mut u, 500 * MS);
        assert!(!sb.irq.level(SOURCE), "second timeout");
        assert!(delivered.is_empty());
        // A reader opens the port: the first packet drains, the interrupt
        // fires on the still-armed enable, the ISR clears it.
        u.open(&mut sb.cx());
        sb.run_to(&mut u, 500 * MS + IN_DRAIN_LATENCY_CYCLES);
        assert_eq!(delivered.bytes(), vec![b'1'; 64]);
        assert!(sb.irq.level(SOURCE));
        sb.write(&mut u, INT_ENA, 0);
        sb.write(&mut u, INT_CLR, INT_SERIAL_IN_EMPTY);
        assert!(!sb.irq.level(SOURCE));
        // The next commit drains (the probe, then the resumed protocol).
        chunk(&mut sb, &mut u, b'3');
        let at = sb.now;
        sb.run_to(&mut u, at + IN_DRAIN_LATENCY_CYCLES);
        assert!(sb.irq.level(SOURCE));
        let mut expect = vec![b'1'; 64];
        expect.extend(vec![b'3'; 64]);
        assert_eq!(delivered.bytes(), expect);
        assert_eq!(u.dropped(), 64, "nothing more dropped");
    }

    // ---- one send buffer, two writers (the 2026-09-13 defect) ---------------
    //
    // `docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md`.
    // The block has ONE send buffer, unwritable from a flush until the host
    // has read it all (ESP32-C3 TRM v1.3 §30.3.2, p. 767 — the same IP as the
    // C6's and the S3's; see the module docs).
    // These pin the mechanism at the registers, independent of any image's
    // timing: what a write into a pending packet does, what wakes esp-hal's
    // write future early, and the firmware-side gate that makes both moot.

    /// esp-hal 1.1.1 `write_async`, one chunk: push with no free check,
    /// `wr_done`, then `UsbSerialJtagWriteFuture::new` sets the enable
    /// **without clearing the raw** and the future waits for the ISR. Returns
    /// the cycles from the commit to the ISR — the future's wake.
    fn esp_hal_write_chunk(sb: &mut Sandbox, u: &mut UsbSerialJtag, chunk: &[u8]) -> u64 {
        for &b in chunk {
            sb.write(u, EP1, u32::from(b));
        }
        sb.write(u, EP1_CONF, EP1_CONF_WR_DONE);
        let ena = sb.read(u, INT_ENA) | INT_SERIAL_IN_EMPTY;
        sb.write(u, INT_ENA, ena);
        let committed_at = sb.now;
        wait_for_isr(sb, u);
        sb.now - committed_at
    }

    /// Poll until the block's source is high, then run esp-hal's
    /// `async_interrupt_handler`: drop the enable, clear both raws.
    fn wait_for_isr(sb: &mut Sandbox, u: &mut UsbSerialJtag) {
        let deadline = sb.now + 10 * IN_DRAIN_LATENCY_CYCLES;
        while !sb.irq.level(SOURCE) {
            assert!(sb.now < deadline, "the write future never woke");
            let next = sb.now + POLL_CYCLES;
            sb.run_to(u, next);
        }
        let ena = sb.read(u, INT_ENA) & !INT_SERIAL_IN_EMPTY;
        sb.write(u, INT_ENA, ena);
        sb.write(u, INT_CLR, INT_SERIAL_IN_EMPTY | INT_SERIAL_OUT_RECV_PKT);
    }

    /// The firmware's gate (`fw-esp32-common`
    /// `serial::in_endpoint::InEndpoint::ready`, used by the C6 and S3), register
    /// for register: wait until the buffer is free — clear, recheck, else esp-hal's
    /// `flush_tx_async` — then clear the now-stale `serial_in_empty`.
    fn in_endpoint_ready(sb: &mut Sandbox, u: &mut UsbSerialJtag) {
        let free = |sb: &mut Sandbox, u: &mut UsbSerialJtag| sb.read(u, EP1_CONF) & 0b010 != 0;
        if !free(sb, u) {
            sb.write(u, INT_CLR, INT_SERIAL_IN_EMPTY);
            if !free(sb, u) {
                let ena = sb.read(u, INT_ENA) | INT_SERIAL_IN_EMPTY;
                sb.write(u, INT_ENA, ena);
                wait_for_isr(sb, u);
            }
        }
        sb.write(u, INT_CLR, INT_SERIAL_IN_EMPTY);
    }

    /// A draining host, esp-hal's `UsbSerialJtag::new` done, and one
    /// esp-println line printed and flushed. Returns the rig and the line.
    fn two_writers_rig() -> (Rig, &'static [u8]) {
        let mut r = rig_with(
            HostState::Attached { draining: true },
            ScriptedSource::new(),
        );
        sb_new_driver(&mut r);
        let line: &'static [u8] = b"[INIT] Server loop entered; first frame pending\n";
        let mut timed_out = false;
        esp_println_write_timed(&mut r.sb, &mut r.u, line, &mut timed_out);
        assert!(!timed_out);
        (r, line)
    }

    fn sb_new_driver(r: &mut Rig) {
        r.sb.write(
            &mut r.u,
            INT_CLR,
            INT_SERIAL_IN_EMPTY | INT_SERIAL_OUT_RECV_PKT,
        );
        r.sb.write(&mut r.u, INT_ENA, 0);
    }

    fn frame(n: usize) -> Vec<u8> {
        (0..n).map(|i| b'A' + (i % 26) as u8).collect()
    }

    #[test]
    fn a_write_into_a_pending_packet_is_refused_and_lands_on_the_tried_stream() {
        // The stop-all reply's shape: esp-println's last packet is committed
        // and the io_task's first chunk follows inside the drain.
        let (mut r, line) = two_writers_rig();
        assert!(r.u.committed(), "esp-println's packet is pending");
        let reply = b"M!{\"id\":1,\"msg\":\"stopAllProjects\"}\n";
        esp_hal_write_chunk(&mut r.sb, &mut r.u, reply);
        let end = r.sb.now + IN_DRAIN_LATENCY_CYCLES;
        r.sb.run_to(&mut r.u, end);
        assert_eq!(r.u.dropped(), reply.len() as u64, "every byte refused");
        assert_eq!(r.tried.bytes(), reply, "and observed, not delivered");
        assert_eq!(r.delivered.bytes(), line, "the host got the line only");
    }

    #[test]
    fn a_stale_serial_in_empty_wakes_esp_hals_write_future_at_once_and_the_next_chunk_is_refused() {
        // The boot hello's shape: esp-println's packet drains, its raw
        // `serial_in_empty` stays set (nobody listens, nobody clears), and
        // esp-hal's future arms onto it.
        let (mut r, line) = two_writers_rig();
        let end = r.sb.now + IN_DRAIN_LATENCY_CYCLES;
        r.sb.run_to(&mut r.u, end);
        assert!(!r.u.committed());
        assert_eq!(
            r.sb.read(&mut r.u, INT_RAW) & INT_SERIAL_IN_EMPTY,
            INT_SERIAL_IN_EMPTY,
            "the stale raw"
        );
        let hello = frame(128);
        let woke = esp_hal_write_chunk(&mut r.sb, &mut r.u, &hello[..64]);
        assert!(
            woke < IN_DRAIN_LATENCY_CYCLES,
            "the future woke {woke} cycles after the commit, before any drain"
        );
        assert!(r.u.committed(), "chunk one is still pending");
        esp_hal_write_chunk(&mut r.sb, &mut r.u, &hello[64..]);
        let end = r.sb.now + 2 * IN_DRAIN_LATENCY_CYCLES;
        r.sb.run_to(&mut r.u, end);
        assert_eq!(r.u.dropped(), 64);
        assert_eq!(r.tried.bytes(), &hello[64..], "chunk two, refused whole");
        let mut expect = line.to_vec();
        expect.extend_from_slice(&hello[..64]);
        assert_eq!(r.delivered.bytes(), expect);
    }

    #[test]
    fn the_in_endpoint_gate_delivers_every_byte_on_both_shapes() {
        // The same two shapes through the firmware's gate: nothing refused,
        // nothing merely tried, every chunk woken by its own drain.
        for drained_first in [false, true] {
            let (mut r, line) = two_writers_rig();
            if drained_first {
                let end = r.sb.now + IN_DRAIN_LATENCY_CYCLES;
                r.sb.run_to(&mut r.u, end);
            }
            let msg = frame(300);
            for chunk in msg.chunks(64) {
                in_endpoint_ready(&mut r.sb, &mut r.u);
                let woke = esp_hal_write_chunk(&mut r.sb, &mut r.u, chunk);
                assert!(
                    (IN_DRAIN_LATENCY_CYCLES..IN_DRAIN_LATENCY_CYCLES + POLL_CYCLES)
                        .contains(&woke),
                    "woken {woke} cycles after the commit: by this chunk's own drain \
                     (drained_first={drained_first})"
                );
            }
            assert_eq!(r.u.dropped(), 0, "drained_first={drained_first}");
            assert!(r.tried.is_empty(), "drained_first={drained_first}");
            let mut expect = line.to_vec();
            expect.extend_from_slice(&msg);
            assert_eq!(r.delivered.bytes(), expect, "drained_first={drained_first}");
        }
    }

    #[test]
    fn read_async_with_a_dropped_future_sees_the_packet_when_it_lands() {
        // Five bytes at 2 ms, then 70 more at 3 ms (a packet and a bit).
        let script = ScriptedSource::new()
            .at(2 * MS, b"M!ab\n")
            .at(3 * MS, vec![b'x'; 70]);
        let Rig { mut sb, mut u, .. } = rig_with(HostState::Attached { draining: true }, script);
        sb.write(
            &mut u,
            INT_CLR,
            INT_SERIAL_IN_EMPTY | INT_SERIAL_OUT_RECV_PKT,
        );
        sb.write(&mut u, INT_ENA, 0);
        // `read_serial`: drain (nothing), then the future arms bit 2 …
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b100, 0);
        sb.write(&mut u, INT_ENA, INT_SERIAL_OUT_RECV_PKT);
        // … and the 1 ms `select` drops it: the enable stays set.
        sb.run_to(&mut u, MS);
        assert!(!sb.irq.level(SOURCE));
        assert_eq!(sb.read(&mut u, INT_ENA), INT_SERIAL_OUT_RECV_PKT);
        // The packet lands after the latency.
        sb.run_to(&mut u, 2 * MS + OUT_LAND_LATENCY_CYCLES - 1);
        assert_eq!(
            sb.read(&mut u, EP1_CONF) & 0b100,
            0,
            "not before the latency"
        );
        sb.run_to(&mut u, 2 * MS + OUT_LAND_LATENCY_CYCLES);
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b100, 0b100, "avail = 1");
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_SERIAL_OUT_RECV_PKT,
            INT_SERIAL_OUT_RECV_PKT
        );
        assert_eq!(sb.read(&mut u, INT_ST), INT_SERIAL_OUT_RECV_PKT);
        assert!(sb.irq.level(SOURCE), "line high");
        assert_eq!(
            sb.read(&mut u, OUT_EP1_ST),
            (7 << EP_ST_WR_ADDR_SHIFT) | (5 << OUT_EP_REC_CNT_SHIFT),
            "wr_addr = n + 2, rec_data_cnt = n"
        );
        // The ISR clears the enable and the raw bit.
        sb.write(&mut u, INT_ENA, 0);
        sb.write(&mut u, INT_CLR, INT_SERIAL_OUT_RECV_PKT);
        assert!(!sb.irq.level(SOURCE));
        // `drain_rx_fifo`: pop while `avail`.
        let mut got = Vec::new();
        while sb.read(&mut u, EP1_CONF) & 0b100 != 0 {
            got.push(sb.read(&mut u, EP1) as u8);
        }
        assert_eq!(got, b"M!ab\n");
        assert_eq!(
            sb.read(&mut u, OUT_EP1_ST) >> EP_ST_RD_ADDR_SHIFT & 0x7f,
            5,
            "rd_addr advanced"
        );
        // The second chunk arrived at 3 ms while the first was resident: it
        // waited, and lands only now, after the pop plus the latency.
        sb.run_to(&mut u, 3 * MS + OUT_LAND_LATENCY_CYCLES);
        assert_eq!(u.out_pending(), 70);
        let popped_at = sb.now;
        sb.run_to(&mut u, popped_at + OUT_LAND_LATENCY_CYCLES);
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b100, 0b100);
        assert_eq!(
            sb.read(&mut u, OUT_EP1_ST) & 0x7f << EP_ST_WR_ADDR_SHIFT,
            66 << 2
        );
        let mut n = 0;
        while sb.read(&mut u, EP1_CONF) & 0b100 != 0 {
            assert_eq!(sb.read(&mut u, EP1), u32::from(b'x'));
            n += 1;
        }
        assert_eq!(n, 64, "one 64-byte packet");
        assert_eq!(u.out_pending(), 6, "the rest waits");
        let at = sb.now;
        sb.run_to(&mut u, at + OUT_LAND_LATENCY_CYCLES);
        assert_eq!(
            sb.read(&mut u, OUT_EP1_ST) >> OUT_EP_REC_CNT_SHIFT & 0x7f,
            6
        );
        // Reading past the packet answers 0 and is counted.
        for _ in 0..6 {
            sb.read(&mut u, EP1);
        }
        assert_eq!(sb.read(&mut u, EP1), 0);
        assert_eq!(u.out_underruns(), 1);
    }

    #[test]
    fn sof_arrives_every_millisecond_while_attached_and_the_monitor_counts_misses() {
        let Rig { mut sb, mut u, .. } = rig_with(
            HostState::Attached { draining: false },
            ScriptedSource::new(),
        );
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_USB_BUS_RESET,
            INT_USB_BUS_RESET,
            "the bus reset of enumeration"
        );
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_SOF, 0, "no frame yet");
        sb.run_to(&mut u, MS - 1);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_SOF, 0);
        sb.run_to(&mut u, MS);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_SOF, INT_SOF);
        assert_eq!(sb.read(&mut u, FRAM_NUM), 1);
        sb.write(&mut u, INT_CLR, INT_SOF);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_SOF, 0, "cleared");
        sb.run_to(&mut u, 2 * MS);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_SOF, INT_SOF, "set again");
        assert_eq!(sb.read(&mut u, FRAM_NUM), 2);
        // `fram_num` is 11 bits.
        sb.run_to(&mut u, 2050 * MS);
        assert_eq!(sb.read(&mut u, FRAM_NUM), 2050 & 0x7ff);

        // `UsbConnectionMonitor::poll` every 2 ms, with its 3-miss rule.
        let mut no_sof = 0u8;
        let mut poll = |sb: &mut Sandbox, u: &mut UsbSerialJtag| -> bool {
            let seen = sb.read(u, INT_RAW) & INT_SOF != 0;
            sb.write(u, INT_CLR, INT_SOF);
            if seen {
                no_sof = 0;
            } else {
                no_sof = no_sof.saturating_add(1);
            }
            no_sof < 3
        };
        for i in 0..5u64 {
            sb.run_to(&mut u, (2051 + 2 * i) * MS);
            assert!(poll(&mut sb, &mut u), "enumerated while attached");
        }
        u.detach(&mut sb.cx());
        assert_eq!(u.host(), HostState::Absent);
        let base = sb.now;
        // The last frame was consumed by the poll above; the next frame
        // never comes. Two misses are still "enumerated", the third is not.
        let mut verdicts = Vec::new();
        for i in 1..=4u64 {
            sb.run_to(&mut u, base + 2 * i * MS);
            verdicts.push(poll(&mut sb, &mut u));
        }
        assert_eq!(verdicts, [true, true, false, false], "3 misses ≈ 6 ms");
        assert_eq!(sb.sched.live(), 0, "no more frames scheduled");
        // Re-attach: bus reset, frames resume, the counter restarts.
        sb.write(&mut u, INT_CLR, INT_USB_BUS_RESET);
        u.attach(&mut sb.cx());
        assert_eq!(sb.read(&mut u, FRAM_NUM), 0);
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_USB_BUS_RESET,
            INT_USB_BUS_RESET
        );
        let at = sb.now;
        sb.run_to(&mut u, at + MS);
        assert_eq!(sb.read(&mut u, INT_RAW) & INT_SOF, INT_SOF);
        assert_eq!(sb.read(&mut u, FRAM_NUM), 1);
    }

    #[test]
    fn a_bus_reset_drops_a_committed_packet_and_a_detach_keeps_it() {
        let Rig {
            mut sb,
            mut u,
            delivered,
            tried,
        } = rig_with(
            HostState::Attached { draining: false },
            ScriptedSource::new(),
        );
        sb.write(&mut u, INT_CLR, INT_SERIAL_IN_EMPTY);
        for &b in b"held\n" {
            sb.write(&mut u, EP1, u32::from(b));
        }
        sb.write(&mut u, EP1_CONF, EP1_CONF_WR_DONE);
        u.detach(&mut sb.cx());
        assert!(u.committed(), "a detach keeps the packet");
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b010, 0);
        u.attach(&mut sb.cx());
        assert!(!u.committed(), "the bus reset drops it");
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b010, 0b010);
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_SERIAL_IN_EMPTY,
            INT_SERIAL_IN_EMPTY
        );
        assert_eq!(tried.text(), "held\n", "on the observation stream");
        assert!(delivered.is_empty());
        assert_eq!(u.dropped_packets(), 1);
        // A close while draining stops the drain; an open resumes it.
        u.open(&mut sb.cx());
        u.close(&mut sb.cx());
        assert_eq!(u.host(), HostState::Attached { draining: false });
        for &b in b"later\n" {
            sb.write(&mut u, EP1, u32::from(b));
        }
        sb.write(&mut u, EP1_CONF, EP1_CONF_WR_DONE);
        let at = sb.now;
        sb.run_to(&mut u, at + 10 * MS);
        assert!(delivered.is_empty(), "closed: nothing drains");
        u.open(&mut sb.cx());
        let at = sb.now;
        sb.run_to(&mut u, at + IN_DRAIN_LATENCY_CYCLES);
        assert_eq!(delivered.text(), "later\n");
    }

    #[test]
    fn the_dtr_rts_dances_decode_like_the_fake_device() {
        let Rig { mut sb, mut u, .. } = rig_with(
            HostState::Attached { draining: true },
            ScriptedSource::new(),
        );
        sb.write(&mut u, INT_CLR, CFG.int_mask);
        // `hardware.rs:228-236`, the USB-Serial-JTAG hard reset:
        // D0; sleep; R1; D0; R1; sleep; R0.
        let seq: &[(Option<bool>, Option<bool>)] = &[
            (Some(false), None),
            (None, Some(true)),
            (Some(false), None),
            (None, Some(true)),
            (None, Some(false)),
        ];
        for &(dtr, rts) in seq {
            u.set_signals(dtr, rts, &mut sb.cx());
        }
        assert_eq!(
            sb.request,
            Some(MachineRequest::Reset {
                source: "USB_DEVICE chip_rst (serial)",
                at: sb.now,
                strap: Strap::App,
                cause: ResetSource::ChipReset,
            })
        );
        assert_eq!(sb.read(&mut u, CHIP_RST) & CHIP_RST_SERIAL, CHIP_RST_SERIAL);
        assert_eq!(
            sb.read(&mut u, INT_RAW) & (INT_RTS_CHG | INT_DTR_CHG),
            INT_RTS_CHG | INT_DTR_CHG
        );
        // w1c: the guest clears the reset-detected bit.
        sb.write(&mut u, CHIP_RST, CHIP_RST_SERIAL);
        assert_eq!(sb.read(&mut u, CHIP_RST), 0);

        // The download dance: R0 D0 W100 D1 R0 W100 R1 D0 R1 W100 R0 D0.
        sb.request = None;
        let seq: &[(Option<bool>, Option<bool>)] = &[
            (None, Some(false)),
            (Some(false), None),
            (Some(true), None),
            (None, Some(false)),
            (None, Some(true)),
            (Some(false), None),
            (None, Some(true)),
            (None, Some(false)),
            (Some(false), None),
        ];
        for &(dtr, rts) in seq {
            u.set_signals(dtr, rts, &mut sb.cx());
        }
        assert!(matches!(
            sb.request,
            Some(MachineRequest::Reset {
                strap: Strap::Download,
                ..
            })
        ));

        // `chip_rst` bit 2 set: the reset is recorded, not requested.
        sb.request = None;
        sb.write(&mut u, CHIP_RST, CHIP_RST_DISABLE);
        u.reset(&mut sb.cx());
        assert_eq!(sb.request, None);
        assert_eq!(
            sb.read(&mut u, CHIP_RST),
            CHIP_RST_DISABLE | CHIP_RST_SERIAL,
            "bit 0 still recorded"
        );
    }

    #[test]
    fn the_whole_state_round_trips_with_a_delivery_and_an_out_queue_in_flight() {
        let script = ScriptedSource::new().at(MS, vec![b'q'; 100]);
        let Rig { mut sb, mut u, .. } = rig_with(HostState::Attached { draining: true }, script);
        sb.run_to(&mut u, MS + OUT_LAND_LATENCY_CYCLES);
        assert_eq!(sb.read(&mut u, EP1), u32::from(b'q'));
        for &b in b"in flight" {
            sb.write(&mut u, EP1, u32::from(b));
        }
        sb.write(&mut u, EP1_CONF, EP1_CONF_WR_DONE);
        u.set_signals(Some(true), Some(true), &mut sb.cx());
        let blob = u.save_state();
        let mut other = UsbSerialJtag::absent(&CFG, None);
        other.load_state(&blob);
        assert_eq!(other.host(), HostState::Attached { draining: true });
        assert_eq!(other.index, 7);
        assert!(other.committed());
        assert_eq!(other.in_fifo(), b"in flight");
        assert_eq!(other.in_deliver_due, u.in_deliver_due);
        assert!(other.in_deliver_due.is_some());
        assert_eq!(other.out_pkt.len(), 63);
        assert_eq!(other.out_staging.len(), 36);
        assert_eq!(other.out_rd_addr, 1);
        assert_eq!(other.out_wr_addr, 66);
        assert_eq!(other.fram_num, u.fram_num);
        assert_eq!(other.sof_due, u.sof_due);
        assert_eq!(other.last_dtr, Some(true));
        assert_eq!(other.last_rts, Some(true));
        assert!(other.dtr_high_seen);
        assert_eq!(other.int_raw, u.int_raw);
        assert_eq!(other.save_state(), blob);
    }

    /// The promoted table, and the flag that reads it. `fram_num` passes
    /// under `measured` because nothing below `measured` is; `jfifo_st` — a
    /// register no driver on this chip touches — fails, which is what the
    /// list in the file header is a list of.
    #[test]
    fn strict_grade_answers_the_measured_path_and_refuses_a_modeled_register() {
        let mut bus = SocBus::new();
        bus.add_mmio_window(BASE, 0x100);
        let mut u = UsbSerialJtag::absent(&CFG, None);
        // The fixture's promotions, the C6's table's shape.
        for off in [EP1, EP1_CONF, INT_RAW, INT_ST, INT_ENA, INT_CLR] {
            assert_eq!(u.reg_grade(off), Some(RegGrade::Measured), "{off:#05x}");
        }
        assert_eq!(u.reg_grade(FRAM_NUM), Some(RegGrade::Documented));
        assert_eq!(u.reg_grade(CONF0), Some(RegGrade::Documented));
        assert_eq!(u.reg_grade(JFIFO_ST), Some(RegGrade::Modeled));
        u.attached(0);
        bus.add_peripheral(BASE, 0x100, Box::new(u));

        // `documented`: the whole data path answers, and so does fram_num.
        bus.set_strict_grade(Some(RegGrade::Documented));
        assert!(bus.read_word(BASE + EP1).is_ok());
        assert!(bus.read_word(BASE + FRAM_NUM).is_ok());
        assert!(
            bus.read_word(BASE + JFIFO_ST).is_err(),
            "a register no driver on this chip touches is not documented"
        );
        let v = bus.first_strict_violation().unwrap();
        assert_eq!(v.grade, Some(RegGrade::Modeled));
        assert_eq!(v.address, BASE + JFIFO_ST);
        assert!(v.in_mmio_window);

        // `measured`: the data path still answers, fram_num no longer does.
        let mut bus = SocBus::new();
        bus.add_mmio_window(BASE, 0x100);
        let mut u = UsbSerialJtag::absent(&CFG, None);
        u.attached(0);
        bus.add_peripheral(BASE, 0x100, Box::new(u));
        bus.set_strict_grade(Some(RegGrade::Measured));
        assert!(bus.read_word(BASE + EP1_CONF).is_ok());
        assert!(
            bus.read_word(BASE + FRAM_NUM).is_err(),
            "the SOF period is documented, never measured here"
        );
        assert_eq!(
            bus.first_strict_violation().unwrap().grade,
            Some(RegGrade::Documented)
        );
    }

    /// The list the README publishes is generated from the table, so the two
    /// cannot drift: a register promoted here leaves the list by itself.
    #[test]
    fn the_modeled_register_list_is_the_table_read_back() {
        let modeled = modeled_registers(&CFG);
        for measured in ["ep1", "ep1_conf", "int_raw", "int_st", "int_ena", "int_clr"] {
            assert!(!modeled.contains(&measured), "{measured}");
        }
        for documented in ["fram_num", "conf0"] {
            assert!(!modeled.contains(&documented), "{documented}");
        }
        for m in [
            "test",
            "jfifo_st",
            "in_ep0_st",
            "out_ep1_st",
            "misc_conf",
            "mem_conf",
            "chip_rst",
            "set_line_code_w0",
            "get_line_code_w1",
            "config_update",
            "ser_afifo_config",
            "bus_reset_st",
            "date",
        ] {
            assert!(modeled.contains(&m), "`{m}` is missing from the list");
        }
    }

    #[test]
    fn host_states_spell_themselves_the_way_the_cli_does() {
        assert_eq!(HostState::parse("absent"), Some(HostState::Absent));
        assert_eq!(
            HostState::parse("attached"),
            Some(HostState::Attached { draining: true })
        );
        assert_eq!(
            HostState::parse("attached-idle"),
            Some(HostState::Attached { draining: false })
        );
        assert_eq!(HostState::parse("draining"), None);
        assert_eq!(
            HostState::Attached { draining: false }.to_string(),
            "attached-idle"
        );
        assert!(HostState::Attached { draining: false }.attached());
        assert!(!HostState::Attached { draining: false }.draining());
    }

    /// **The capability, from the other side** (Xtensa M6 P05): a part whose
    /// `0x4c`…`0x7c` is reserved has no `chip_rst` to refuse a reset with and
    /// no `bus_reset_st` to release, and its `int_raw` stops at bit 11.
    ///
    /// Every one of these is a thing the C6 view *does* and this part must
    /// not, which is why they are asserted rather than assumed.
    #[test]
    fn a_part_without_the_host_reset_pair_cannot_refuse_a_reset_and_has_no_modem_bits() {
        let Rig { mut sb, mut u, .. } = rig_cfg(
            &CFG_NO_HOST_RESET,
            HostState::Attached { draining: true },
            ScriptedSource::new(),
        );
        // The bus reset of enumeration raised bit 9 and wrote nothing at
        // `bus_reset_st`: that word is reserved here and reads as the
        // accept-and-remember zero.
        assert_eq!(
            sb.read(&mut u, INT_RAW) & INT_USB_BUS_RESET,
            INT_USB_BUS_RESET
        );
        assert_eq!(sb.read(&mut u, BUS_RESET_ST), 0, "reserved on this part");
        assert_eq!(sb.read(&mut u, CHIP_RST), 0);

        // The host asserts DTR and RTS. A host really does — the verbs stay
        // on the control channel — but there is no register for them to
        // reach, so `int_raw` does not move.
        sb.write(&mut u, INT_CLR, CFG_NO_HOST_RESET.int_mask);
        u.set_signals(Some(true), Some(true), &mut sb.cx());
        assert_eq!(
            sb.read(&mut u, INT_RAW) & (INT_RTS_CHG | INT_DTR_CHG),
            0,
            "bits 12 and 13 are not declared on this part"
        );
        // Nor can the guest enable one.
        sb.write(
            &mut u,
            INT_ENA,
            INT_RTS_CHG | INT_DTR_CHG | INT_SERIAL_IN_EMPTY,
        );
        assert_eq!(sb.read(&mut u, INT_ENA), INT_SERIAL_IN_EMPTY);

        // The guest tries to set `chip_rst` bit 2. There is no such bit:
        // the write is accepted-and-remembered at a reserved word and the
        // reset it would have disabled happens anyway.
        sb.write(&mut u, CHIP_RST, 0b100);
        assert!(!u.chip_reset_disabled(), "no chip_rst, so no say");
        sb.request = None;
        assert!(u.reset(&mut sb.cx()), "unconditional on this part");
        assert_eq!(
            sb.request,
            Some(MachineRequest::Reset {
                source: "USB_DEVICE chip_rst (serial)",
                at: sb.now,
                strap: Strap::App,
                cause: ResetSource::ChipReset,
            })
        );
        // And the download dance the same way.
        sb.request = None;
        assert!(u.download_mode(&mut sb.cx()));
        assert!(matches!(
            sb.request,
            Some(MachineRequest::Reset {
                strap: Strap::Download,
                ..
            })
        ));

        // No silicon has been read for this part, so nothing is above
        // `Modeled` — including the six the C6's transcripts bought.
        for off in [
            EP1, EP1_CONF, INT_RAW, INT_ST, INT_ENA, INT_CLR, FRAM_NUM, CONF0,
        ] {
            assert_eq!(u.reg_grade(off), Some(RegGrade::Modeled), "{off:#05x}");
        }
        // And the register list it publishes is its own table's, so the C6's
        // eight extra registers are absent from it.
        let modeled = modeled_registers(&CFG_NO_HOST_RESET);
        assert!(modeled.contains(&"ep1"), "nothing is promoted here");
        for absent in [
            "chip_rst",
            "bus_reset_st",
            "set_line_code_w0",
            "config_update",
        ] {
            assert!(!modeled.contains(&absent), "`{absent}` is a C6 register");
        }
    }

    /// The data path itself is the same file, so it is the same behaviour on
    /// a part with no host-reset pair: the `[INIT]` chain crosses the FIFO
    /// and reaches the host.
    #[test]
    fn the_console_path_is_unchanged_on_a_part_without_the_pair() {
        let Rig {
            mut sb,
            mut u,
            delivered,
            tried,
        } = rig_cfg(
            &CFG_NO_HOST_RESET,
            HostState::Attached { draining: true },
            ScriptedSource::new(),
        );
        let input = init_lines();
        let mut timed_out = false;
        for line in input.split_inclusive(|&b| b == b'\n') {
            esp_println_write_timed(&mut sb, &mut u, line, &mut timed_out);
            assert!(!timed_out, "TIMED_OUT latched on {line:?}");
        }
        let end = sb.now + IN_DRAIN_LATENCY_CYCLES;
        sb.run_to(&mut u, end);
        assert_eq!(delivered.bytes(), input);
        assert!(tried.is_empty());
        assert_eq!(u.dropped(), 0);
    }
}
