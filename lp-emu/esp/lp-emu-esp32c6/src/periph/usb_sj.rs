//! `USB_DEVICE` (USB-Serial-JTAG) at `0x6000_F000` — the honest model of
//! the host's side: **absent**, **attached with the port closed**, and
//! **attached with an application draining it**, with the transitions
//! between them. P6 built the absent state; this file (M6 P2) grows it. The
//! control channel that drives the transitions from outside is M6 P3; the
//! builder's `UsbHost` option sets the initial state until then.
//!
//! Register facts are the esp32c6 PAC 0.23.2 `usb_device` block (offsets in
//! `regs::USB_DEVICE`; reset values per register below) and the two drivers
//! that touch it (M6 discovery §2–§3): esp-println's raw-MMIO printer
//! (`esp-println-0.17.0/src/lib.rs:235-337`) and esp-hal 1.1.1's
//! `usb_serial_jtag.rs` (blocking `:173-231`, async + ISR `:806-961`). The
//! firmware's own reading of a host is `UsbConnectionMonitor`
//! (`fw-esp32c6/src/board/esp32c6/usb_connection.rs`): SOF every 1 ms means
//! a cable, two 250 ms write timeouts in a row mean nobody is draining.
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
//! Revised by M6 P4 once the transcripts existed. A grade moves only with a
//! transcript, and the four under `lp-emu/transcripts/esp32c6/` are what
//! moved these: `boot-idle` (a host attached and draining, replayed against
//! silicon's capture of the same image bytes), `usb-negative-control` (the
//! port held closed from boot and opened at eight seconds),
//! `usb-detach-reattach` and `usb-host-absent`.
//!
//! | grade | registers | why |
//! |---|---|---|
//! | `measured` | `ep1`, `ep1_conf`, `int_raw`, `int_st`, `int_ena`, `int_clr` | the transitions those transcripts prove: SOF present while attached and absent when the cable is out; `serial_in_ep_data_free` returning only once a host has drained the packet; `serial_in_empty` completing esp-hal's write future; `serial_out_recv_pkt` on host bytes (`lp-cli`'s hello, `emu_usb_hello`); and the whole path exercised byte for byte by both drivers |
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

use std::collections::VecDeque;

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{
    BusCx, MachineRequest, Peripheral, RegFile, RegGrade, RegGrades, Strap, StreamId, Width,
    event_id, event_local,
};

use super::systimer::Reader;
use super::uart::LIVE_POLL_CYCLES;
use crate::memmap;
use crate::regs::{self, source};

const EP1: u32 = 0x00;
const EP1_CONF: u32 = 0x04;
const INT_RAW: u32 = 0x08;
const INT_ST: u32 = 0x0c;
const INT_ENA: u32 = 0x10;
const INT_CLR: u32 = 0x14;
const CONF0: u32 = 0x18;
const TEST: u32 = 0x1c;
const JFIFO_ST: u32 = 0x20;
const FRAM_NUM: u32 = 0x24;
const IN_EP0_ST: u32 = 0x28;
const IN_EP1_ST: u32 = 0x2c;
const IN_EP2_ST: u32 = 0x30;
const IN_EP3_ST: u32 = 0x34;
const OUT_EP1_ST: u32 = 0x3c;
const MEM_CONF: u32 = 0x48;
const CHIP_RST: u32 = 0x4c;
const SER_AFIFO_CONFIG: u32 = 0x64;
const BUS_RESET_ST: u32 = 0x68;

// PAC `RESET_VALUE`s (discovery §1). Every other register resets to 0.
const CONF0_RESET: u32 = 0x4200;
const TEST_RESET: u32 = 0x30;
const JFIFO_ST_RESET: u32 = 0x44;
const IN_EPN_ST_RESET: u32 = 0x01;
const MEM_CONF_RESET: u32 = 0x02;
const SER_AFIFO_CONFIG_RESET: u32 = 0x10;
const BUS_RESET_ST_RESET: u32 = 0x01;

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
const INT_MASK: u32 = 0xffff;

/// `chip_rst`: bit 0 "chip reset is detected from usb serial channel, write
/// 1 to clear"; bit 1 the same from the JTAG channel; bit 2 "disable chip
/// reset from usb serial channel".
const CHIP_RST_SERIAL: u32 = 1 << 0;
const CHIP_RST_JTAG: u32 = 1 << 1;
const CHIP_RST_DISABLE: u32 = 1 << 2;

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
pub const SOF_PERIOD_US: u64 = 1_000;
/// Cycles between SOFs.
pub const SOF_PERIOD_CYCLES: u64 = SOF_PERIOD_US * memmap::CYCLES_PER_US;

/// How long after `wr_done` a draining host has taken the IN packet. Grade
/// **modeled**: sub-millisecond, well under every timeout the firmware uses
/// (the 100 ms probe, the 250 ms chunk, the bridge's 2 ms stall); not
/// measured.
pub const IN_DRAIN_LATENCY_US: u64 = 100;
pub const IN_DRAIN_LATENCY_CYCLES: u64 = IN_DRAIN_LATENCY_US * memmap::CYCLES_PER_US;

/// How long after the host writes a byte (or the previous OUT packet is
/// read out) the next OUT packet lands. Grade **modeled**, the same number
/// for the same reason as [`IN_DRAIN_LATENCY_US`].
pub const OUT_LAND_LATENCY_US: u64 = IN_DRAIN_LATENCY_US;
pub const OUT_LAND_LATENCY_CYCLES: u64 = OUT_LAND_LATENCY_US * memmap::CYCLES_PER_US;

const EV_SOF: u16 = 0;
const EV_IN_DELIVER: u16 = 1;
const EV_OUT_POLL: u16 = 2;
const EV_OUT_LAND: u16 = 3;

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

impl std::fmt::Display for HostState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// USB-Serial-JTAG with a host model.
#[derive(Debug)]
pub struct UsbSerialJtag {
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
    /// `delivered` is the `usb-sj` stream (what a host receives / sends);
    /// `tried` the observation stream; `host` the state at power-on.
    pub fn new(delivered: Option<StreamId>, tried: Option<StreamId>, host: HostState) -> Self {
        Self {
            regs: RegFile::new("USB_DEVICE", 0x100)
                .with_names(regs::USB_DEVICE)
                .with_reset(CONF0, CONF0_RESET)
                .with_reset(TEST, TEST_RESET)
                .with_reset(JFIFO_ST, JFIFO_ST_RESET)
                .with_reset(IN_EP0_ST, IN_EPN_ST_RESET)
                .with_reset(IN_EP1_ST, IN_EPN_ST_RESET)
                .with_reset(IN_EP2_ST, IN_EPN_ST_RESET)
                .with_reset(IN_EP3_ST, IN_EPN_ST_RESET)
                .with_reset(MEM_CONF, MEM_CONF_RESET)
                .with_reset(SER_AFIFO_CONFIG, SER_AFIFO_CONFIG_RESET)
                .with_reset(BUS_RESET_ST, BUS_RESET_ST_RESET),
            grades: Self::grades(),
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
    pub fn absent(tried: Option<StreamId>) -> Self {
        Self::new(None, tried, HostState::Absent)
    }

    /// The per-register grade table (the file header's).
    ///
    /// Anything unlisted is `Modeled`, which for this block is a considered
    /// answer rather than a default: the header lists those registers by
    /// name, with the reason the firmware never reaches them.
    pub fn grades() -> RegGrades {
        RegGrades::new()
            // The data path and the three interrupt bits the M6 transcripts
            // exercise end to end. See the header for the bit-level caveat.
            .with_grade(EP1, RegGrade::Measured)
            .with_grade(EP1_CONF, RegGrade::Measured)
            .with_grade(INT_RAW, RegGrade::Measured)
            .with_grade(INT_ST, RegGrade::Measured)
            .with_grade(INT_ENA, RegGrade::Measured)
            .with_grade(INT_CLR, RegGrade::Measured)
            .with_grade(FRAM_NUM, RegGrade::Documented)
            .with_grade(CONF0, RegGrade::Documented)
    }

    /// Every register this block grades `Modeled`, in offset order — the
    /// list the README and `--strict-grade` both mean by "the modeled
    /// registers".
    pub fn modeled_registers() -> Vec<&'static str> {
        let grades = Self::grades();
        (0..0x100u32)
            .step_by(4)
            .filter(|off| grades.grade(*off) == RegGrade::Modeled)
            .filter_map(|off| regs::USB_DEVICE.name(off))
            .collect()
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
                self.int_raw |= INT_DTR_CHG;
            }
            self.last_dtr = Some(dtr);
            if dtr {
                self.dtr_high_seen = true;
            }
        }
        if let Some(rts) = rts {
            let falling = self.last_rts == Some(true) && !rts;
            if self.last_rts != Some(rts) {
                self.int_raw |= INT_RTS_CHG;
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
    /// `false` when `chip_rst.disable` (bit 2) suppressed it.
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
    pub fn chip_reset_disabled(&self) -> bool {
        self.regs.stored(CHIP_RST) & CHIP_RST_DISABLE != 0
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
        let stored = self.regs.stored(CHIP_RST);
        self.regs.poke(CHIP_RST, stored | CHIP_RST_SERIAL);
        let disabled = stored & CHIP_RST_DISABLE != 0;
        let line = format!(
            "cyc={} pc=0x{:08x} USB_DEVICE chip reset from the serial channel, strap = {strap}{}",
            cx.now,
            cx.pc,
            if disabled {
                " — chip_rst.disable is set, recorded and not performed"
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
        });
        true
    }

    // ---- the pieces ------------------------------------------------------

    fn in_free(&self) -> bool {
        !self.committed && self.in_fifo.len() < IN_FIFO_DEPTH
    }

    fn out_avail(&self) -> bool {
        !self.out_pkt.is_empty()
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.int_raw & self.regs.stored(INT_ENA);
        cx.irq.set_level(source::USB_DEVICE, st != 0);
    }

    fn observe(&mut self, bytes: &[u8], cx: &mut BusCx<'_>) {
        if let Some(id) = self.tried {
            cx.host.stream(id).write(bytes);
        }
    }

    /// A USB bus reset: the endpoints return to their default state, which
    /// empties a committed IN packet (modeled). `bus_reset_st` = released.
    fn bus_reset(&mut self, cx: &mut BusCx<'_>) {
        self.int_raw |= INT_USB_BUS_RESET;
        self.regs.poke(BUS_RESET_ST, BUS_RESET_ST_RESET);
        self.fram_num = 0;
        if self.committed {
            let n = self.in_fifo.len();
            self.dropped_packets += 1;
            let bytes = std::mem::take(&mut self.in_fifo);
            self.observe(&bytes, cx);
            self.committed = false;
            self.committed_at = None;
            if self.in_deliver_due.take().is_some() {
                cx.sched.cancel(event_id(self.index, EV_IN_DELIVER));
            }
            self.int_raw |= INT_SERIAL_IN_EMPTY;
            let line = format!(
                "cyc={} pc=0x{:08x} USB_DEVICE bus reset dropped the committed IN packet ({n} \
                 bytes never reached a host; on the observation stream)",
                cx.now, cx.pc
            );
            cx.trace.note(&line);
        }
    }

    fn start_sof(&mut self, from: u64, cx: &mut BusCx<'_>) {
        self.sof_due = from.saturating_add(SOF_PERIOD_CYCLES);
        cx.sched
            .schedule_at(self.sof_due, event_id(self.index, EV_SOF));
    }

    fn schedule_delivery(&mut self, from: u64, cx: &mut BusCx<'_>) {
        let due = from.saturating_add(IN_DRAIN_LATENCY_CYCLES);
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
        let bytes = std::mem::take(&mut self.in_fifo);
        self.in_delivered += bytes.len() as u64;
        if let Some(id) = self.delivered {
            cx.host.stream(id).write(&bytes);
        }
        self.committed = false;
        self.committed_at = None;
        self.int_raw |= INT_SERIAL_IN_EMPTY | INT_IN_TOKEN_REC_IN_EP1;
        let line = format!(
            "cyc={} pc=0x{:08x} USB_DEVICE IN packet of {} bytes delivered to the host \
             (serial_in_ep_data_free = 1, serial_in_empty raised)",
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
                self.out_poll_due = cx.now.max(at).saturating_add(LIVE_POLL_CYCLES);
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
        let due = from.saturating_add(OUT_LAND_LATENCY_CYCLES);
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
        self.int_raw |= INT_SERIAL_OUT_RECV_PKT;
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
            return;
        }
        if self.dropped == 0 {
            let line = format!(
                "cyc={} pc=0x{:08x} USB_DEVICE ep1 write with the IN FIFO {} (host {}): byte \
                 0x{byte:02x} dropped",
                cx.now,
                cx.pc,
                if self.committed { "committed" } else { "full" },
                self.host
            );
            cx.trace.note(&line);
        }
        self.dropped += 1;
        self.observe(&[byte], cx);
    }

    fn wr_done(&mut self, cx: &mut BusCx<'_>) {
        if self.in_fifo.is_empty() {
            // Committing nothing: the endpoint is empty at once.
            self.int_raw |= INT_SERIAL_IN_EMPTY;
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
                self.regs.poke(INT_ENA, value & INT_MASK);
                self.update_lines(cx);
            }
            INT_CLR => {
                self.int_raw &= !(value & INT_MASK);
                self.update_lines(cx);
            }
            CHIP_RST => {
                // Bits 0 and 1 are write-one-to-clear; bit 2 is stored.
                let old = self.regs.stored(CHIP_RST);
                let cleared = old & !(value & (CHIP_RST_SERIAL | CHIP_RST_JTAG));
                let new = (cleared & !CHIP_RST_DISABLE) | (value & CHIP_RST_DISABLE);
                self.regs.poke(CHIP_RST, new);
            }
            INT_RAW | INT_ST | FRAM_NUM | IN_EP1_ST | OUT_EP1_ST => {}
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
                self.int_raw |= INT_SOF;
                self.fram_num = (self.fram_num + 1) & 0x7ff;
                self.sof_due = self.sof_due.saturating_add(SOF_PERIOD_CYCLES);
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
            _ => {}
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::USB_DEVICE.name(off)
    }

    /// The endpoint status words a driver spins on (M4).
    ///
    /// `esp-println` over USB-Serial-JTAG waits for `ep1_conf.in_ep_data_free`
    /// before every byte, which is the same shape as UART0's TX-FIFO poll.
    /// All of these are derived in [`read_word`](Self::read_word) from the
    /// two FIFOs, `int_raw`, `fram_num` and stored registers, and every one
    /// of those moves only on a write or on one of this block's scheduled
    /// events (`EV_SOF` for `fram_num`, `EV_IN_DELIVER` and `EV_OUT_LAND`
    /// for the FIFOs). `EP1` is excluded: reading it pops the OUT FIFO.
    fn pure_read(&self, off: u32) -> bool {
        matches!(
            off & !3,
            EP1_CONF | INT_RAW | INT_ST | FRAM_NUM | IN_EP1_ST | OUT_EP1_ST
        )
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        Some(self.grades.grade(off))
    }

    /// The one block the machine drives from outside the guest: M6 P3's
    /// control channel calls [`attach`](Self::attach) and its siblings
    /// through [`lp_emu_esp_common::SocBus::with_peripheral`].
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x100 + 3 * IN_FIFO_DEPTH + 128);
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
        self.in_fifo = in_fifo;
        self.out_pkt = out_pkt.into();
        self.out_staging = out_staging.into();
        self.regs.load_state(r.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_core::Bus;
    use lp_emu_esp_common::host::MemorySink;
    use lp_emu_esp_common::{ByteLog, Sandbox, ScriptedSource, SocBus};

    /// A sandbox with the `usb-sj` stream (delivered + a scripted source)
    /// and the observation stream, and a USB block on them in `host`.
    struct Rig {
        sb: Sandbox,
        u: UsbSerialJtag,
        delivered: ByteLog,
        tried: ByteLog,
    }

    fn rig_with(host: HostState, script: ScriptedSource) -> Rig {
        let mut sb = Sandbox::new();
        let delivered = ByteLog::new();
        let id = sb.host.add(
            "usb-sj",
            Box::new(MemorySink(delivered.clone())),
            Box::new(script),
        );
        let (tried_id, tried) = sb.host.add_memory("usb-sj-tried");
        let mut u = UsbSerialJtag::new(Some(id), Some(tried_id), host);
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

    const MS: u64 = 1_000 * memmap::CYCLES_PER_US;

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
        // The PAC resets P6 left at 0.
        assert_eq!(sb.read(&mut u, JFIFO_ST), 0x44);
        assert_eq!(sb.read(&mut u, MEM_CONF), 0x02);
        assert_eq!(sb.read(&mut u, SER_AFIFO_CONFIG), 0x10);
        assert_eq!(sb.read(&mut u, TEST), 0x30);
        assert_eq!(sb.read(&mut u, IN_EP0_ST), 0x01);
        assert_eq!(sb.read(&mut u, IN_EP2_ST), 0x01);
        assert_eq!(sb.read(&mut u, IN_EP3_ST), 0x01);
        assert_eq!(sb.read(&mut u, FRAM_NUM), 0);
        assert_eq!(sb.read(&mut u, OUT_EP1_ST), 0);
        assert_eq!(sb.sched.live(), 0, "no host: nothing scheduled");
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
            !sb.irq.level(source::USB_DEVICE),
            "committed and never drained: not empty, no interrupt"
        );
        sb.run_to(&mut u, 300 * MS);
        assert!(!sb.irq.level(source::USB_DEVICE), "and not after 300 ms");
        // A committed-but-empty flush *is* empty at once.
        let (mut sb2, mut u2, _) = rig();
        sb2.write(&mut u2, INT_CLR, INT_SERIAL_IN_EMPTY);
        sb2.write(&mut u2, EP1_CONF, EP1_CONF_WR_DONE);
        sb2.write(&mut u2, INT_ENA, INT_SERIAL_IN_EMPTY);
        assert!(sb2.irq.level(source::USB_DEVICE));
        assert_eq!(sb2.read(&mut u2, INT_ST), INT_SERIAL_IN_EMPTY);
        // The async ISR: drop the enable, clear the raw.
        sb2.write(&mut u2, INT_ENA, 0);
        sb2.write(&mut u2, INT_CLR, INT_SERIAL_IN_EMPTY);
        assert!(!sb2.irq.level(source::USB_DEVICE));
        // The connected path never arms RX; if a guest did, nothing fires.
        sb2.write(&mut u2, INT_ENA, INT_SERIAL_OUT_RECV_PKT);
        assert!(!sb2.irq.level(source::USB_DEVICE));
    }

    #[test]
    fn the_state_round_trips() {
        let (mut sb, mut u, _) = rig();
        sb.write(&mut u, EP1, 0x41);
        sb.write(&mut u, EP1_CONF, EP1_CONF_WR_DONE);
        sb.write(&mut u, INT_ENA, INT_SERIAL_IN_EMPTY);
        let blob = u.save_state();
        let mut other = UsbSerialJtag::absent(None);
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
        assert!(!sb.irq.level(source::USB_DEVICE));
        let committed_at = sb.now;
        sb.run_to(&mut u, committed_at + IN_DRAIN_LATENCY_CYCLES - 1);
        assert!(!sb.irq.level(source::USB_DEVICE), "not before the latency");
        assert!(delivered.is_empty());
        sb.run_to(&mut u, committed_at + IN_DRAIN_LATENCY_CYCLES);
        assert!(sb.irq.level(source::USB_DEVICE), "source 48 high");
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
        assert!(!sb.irq.level(source::USB_DEVICE), "line low");
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
        assert!(!sb.irq.level(source::USB_DEVICE));
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
        assert!(sb.irq.level(source::USB_DEVICE));
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
        assert!(!sb.irq.level(source::USB_DEVICE), "first timeout");
        assert_eq!(sb.read(&mut u, INT_ENA), INT_SERIAL_IN_EMPTY);
        // The retry, 250 ms later: the FIFO is still committed, so the
        // pushes are dropped, the wr_done is a no-op, and the wait times out
        // again — the second timeout latches "not draining".
        chunk(&mut sb, &mut u, b'2');
        assert_eq!(u.dropped(), 64);
        assert_eq!(tried.bytes(), vec![b'2'; 64]);
        sb.run_to(&mut u, 500 * MS);
        assert!(!sb.irq.level(source::USB_DEVICE), "second timeout");
        assert!(delivered.is_empty());
        // A reader opens the port: the first packet drains, the interrupt
        // fires on the still-armed enable, the ISR clears it.
        u.open(&mut sb.cx());
        sb.run_to(&mut u, 500 * MS + IN_DRAIN_LATENCY_CYCLES);
        assert_eq!(delivered.bytes(), vec![b'1'; 64]);
        assert!(sb.irq.level(source::USB_DEVICE));
        sb.write(&mut u, INT_ENA, 0);
        sb.write(&mut u, INT_CLR, INT_SERIAL_IN_EMPTY);
        assert!(!sb.irq.level(source::USB_DEVICE));
        // The next commit drains (the probe, then the resumed protocol).
        chunk(&mut sb, &mut u, b'3');
        let at = sb.now;
        sb.run_to(&mut u, at + IN_DRAIN_LATENCY_CYCLES);
        assert!(sb.irq.level(source::USB_DEVICE));
        let mut expect = vec![b'1'; 64];
        expect.extend(vec![b'3'; 64]);
        assert_eq!(delivered.bytes(), expect);
        assert_eq!(u.dropped(), 64, "nothing more dropped");
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
        assert!(!sb.irq.level(source::USB_DEVICE));
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
        assert!(sb.irq.level(source::USB_DEVICE), "line high");
        assert_eq!(
            sb.read(&mut u, OUT_EP1_ST),
            (7 << EP_ST_WR_ADDR_SHIFT) | (5 << OUT_EP_REC_CNT_SHIFT),
            "wr_addr = n + 2, rec_data_cnt = n"
        );
        // The ISR clears the enable and the raw bit.
        sb.write(&mut u, INT_ENA, 0);
        sb.write(&mut u, INT_CLR, INT_SERIAL_OUT_RECV_PKT);
        assert!(!sb.irq.level(source::USB_DEVICE));
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
        sb.write(&mut u, INT_CLR, INT_MASK);
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
        let mut other = UsbSerialJtag::absent(None);
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
        bus.add_mmio_window(memmap::periph::USB_DEVICE, 0x100);
        let mut u = UsbSerialJtag::absent(None);
        // M6 P4's promotions, each backed by a committed transcript.
        for off in [EP1, EP1_CONF, INT_RAW, INT_ST, INT_ENA, INT_CLR] {
            assert_eq!(u.reg_grade(off), Some(RegGrade::Measured), "{off:#05x}");
        }
        assert_eq!(u.reg_grade(FRAM_NUM), Some(RegGrade::Documented));
        assert_eq!(u.reg_grade(CONF0), Some(RegGrade::Documented));
        assert_eq!(u.reg_grade(JFIFO_ST), Some(RegGrade::Modeled));
        u.attached(0);
        bus.add_peripheral(memmap::periph::USB_DEVICE, 0x100, Box::new(u));

        // `documented`: the whole data path answers, and so does fram_num.
        bus.set_strict_grade(Some(RegGrade::Documented));
        assert!(bus.read_word(memmap::periph::USB_DEVICE + EP1).is_ok());
        assert!(bus.read_word(memmap::periph::USB_DEVICE + FRAM_NUM).is_ok());
        assert!(
            bus.read_word(memmap::periph::USB_DEVICE + JFIFO_ST)
                .is_err(),
            "a register no driver on this chip touches is not documented"
        );
        let v = bus.first_strict_violation().unwrap();
        assert_eq!(v.grade, Some(RegGrade::Modeled));
        assert_eq!(v.address, memmap::periph::USB_DEVICE + JFIFO_ST);
        assert!(v.in_mmio_window);

        // `measured`: the data path still answers, fram_num no longer does.
        let mut bus = SocBus::new();
        bus.add_mmio_window(memmap::periph::USB_DEVICE, 0x100);
        let mut u = UsbSerialJtag::absent(None);
        u.attached(0);
        bus.add_peripheral(memmap::periph::USB_DEVICE, 0x100, Box::new(u));
        bus.set_strict_grade(Some(RegGrade::Measured));
        assert!(bus.read_word(memmap::periph::USB_DEVICE + EP1_CONF).is_ok());
        assert!(
            bus.read_word(memmap::periph::USB_DEVICE + FRAM_NUM)
                .is_err(),
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
        let modeled = UsbSerialJtag::modeled_registers();
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
}
