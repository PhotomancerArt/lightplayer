//! `USB_DEVICE` (USB-Serial-JTAG) at `0x6000_F000` — the honest model of
//! **no host attached**. The full model, with attach/detach/draining
//! transitions and a control channel, is M6; this is its first state.
//!
//! Register facts are the esp32c6 PAC 0.23.2 `usb_device` block (offsets in
//! `regs::USB_DEVICE`; `ep1_conf` resets `0x02`, `int_raw` resets `0x08`,
//! `conf0` `0x4200`, `in_ep1_st` `0x01`, `bus_reset_st` `0x01`) and the two
//! drivers that touch it (discovery §6): esp-println's raw-MMIO printer
//! (`esp-println-0.17.0/src/lib.rs:235-337`) and esp-hal's
//! `usb_serial_jtag.rs`.
//!
//! # What "no host" looks like at the registers
//!
//! - `ep1_conf.serial_in_ep_data_free` (bit 1) is 1 while the 64-byte IN
//!   FIFO has room **and** nothing has been committed with `wr_done` (bit 0).
//!   After a `wr_done` with bytes in the FIFO it reads 0 and stays 0: the
//!   PAC says the bit returns "until data in UART Tx FIFO is read by USB
//!   Host", and there is no host.
//! - `ep1_conf.serial_out_ep_data_avail` (bit 2) is 0 always: no host, no
//!   OUT packets.
//! - `int_raw.sof` (bit 1) is **never** set — the observable the firmware's
//!   `UsbConnectionMonitor` polls, and the inverse of esp-emu's SOF-forever
//!   (spike report §4). `serial_out_recv_pkt` (bit 2) never.
//!   `serial_in_empty` (bit 3) is set at reset (the PAC's `0x08`: the IN
//!   endpoint is empty), cleared by `int_clr`, and set again only when the
//!   IN endpoint becomes empty — which after the first committed write it
//!   never does.
//! - `int_ena` / `int_clr` (w1c) / `int_st = raw & ena`; source 48 follows.
//!
//! # What the guest then does (the sources, not this model)
//!
//! esp-println: 64 bytes of `[INIT] …` fill the FIFO, `fifo_full()` becomes
//! true, `fifo_flush()` commits them, `wait_for_flush()` spins 50,000
//! iterations on bit 1, latches `TIMED_OUT`, and every later print returns
//! at once without writing. The bytes the guest *did* hand over are written
//! to the `usb-sj` host sink as an **emulator observation** — they never
//! reached a host and the log says so — and the spin is the trace's one
//! `SPIN` line on the shipped image (`USB_DEVICE+0x004 ep1_conf`).
//!
//! `UsbConnectionMonitor::poll` sees no SOF three times → `is_enumerated()`
//! false → `is_connected()` false → the connected path that arms
//! `int_ena.serial_out_recv_pkt` is never taken. `io_task`'s probe write
//! (`b"\n"` every 2 s, 100 ms timeout): the first resolves on the reset
//! `serial_in_empty`, every later one times out, and the monitor logs "host
//! not draining" into the void. So the gate is register and static state,
//! not a log line: `TIMED_OUT == 1` and `int_ena & (1 << 2) == 0`.

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, StreamId, Width};

use super::systimer::Reader;
use crate::regs::{self, source};

const EP1: u32 = 0x00;
const EP1_CONF: u32 = 0x04;
const INT_RAW: u32 = 0x08;
const INT_ST: u32 = 0x0c;
const INT_ENA: u32 = 0x10;
const INT_CLR: u32 = 0x14;
const CONF0: u32 = 0x18;
const IN_EP1_ST: u32 = 0x2c;
const BUS_RESET_ST: u32 = 0x68;

const CONF0_RESET: u32 = 0x4200;
const IN_EP1_ST_RESET: u32 = 0x01;
const BUS_RESET_ST_RESET: u32 = 0x01;

const EP1_CONF_WR_DONE: u32 = 1 << 0;
const EP1_CONF_IN_FREE: u32 = 1 << 1;

pub const INT_SOF: u32 = 1 << 1;
pub const INT_SERIAL_OUT_RECV_PKT: u32 = 1 << 2;
pub const INT_SERIAL_IN_EMPTY: u32 = 1 << 3;
const INT_MASK: u32 = 0xffff;

/// The IN endpoint FIFO: 64 bytes (the PAC's `ep1` doc: "up to 64 bytes").
pub const IN_FIFO_DEPTH: usize = 64;

/// USB-Serial-JTAG with no host attached.
#[derive(Debug)]
pub struct UsbSerialJtag {
    regs: RegFile,
    sink: Option<StreamId>,
    in_fifo: Vec<u8>,
    /// A `wr_done` committed bytes that no host will ever read.
    sealed: bool,
    int_raw: u32,
    /// Bytes written past the 64-byte fill, dropped.
    dropped: u64,
    sealed_at: Option<u64>,
}

impl UsbSerialJtag {
    pub fn new(sink: Option<StreamId>) -> Self {
        Self {
            regs: RegFile::new("USB_DEVICE", 0x100)
                .with_names(regs::USB_DEVICE)
                .with_reset(CONF0, CONF0_RESET)
                .with_reset(IN_EP1_ST, IN_EP1_ST_RESET)
                .with_reset(BUS_RESET_ST, BUS_RESET_ST_RESET),
            sink,
            in_fifo: Vec::with_capacity(IN_FIFO_DEPTH),
            sealed: false,
            int_raw: INT_SERIAL_IN_EMPTY,
            dropped: 0,
            sealed_at: None,
        }
    }

    fn in_free(&self) -> bool {
        !self.sealed && self.in_fifo.len() < IN_FIFO_DEPTH
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.int_raw & self.regs.stored(INT_ENA);
        cx.irq.set_level(source::USB_DEVICE, st != 0);
    }

    fn read_word(&self, off: u32) -> u32 {
        match off {
            // No OUT data, ever.
            EP1 => 0,
            EP1_CONF => {
                if self.in_free() {
                    EP1_CONF_IN_FREE
                } else {
                    0
                }
            }
            INT_RAW => self.int_raw,
            INT_ST => self.int_raw & self.regs.stored(INT_ENA),
            INT_CLR => 0,
            other => self.regs.effective(other),
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            EP1 => {
                let byte = (value & 0xff) as u8;
                if self.in_free() {
                    self.in_fifo.push(byte);
                    // What the guest tried to print — an observation, not
                    // guest output that reached anyone.
                    if let Some(id) = self.sink {
                        cx.host.stream(id).write_byte(byte);
                    }
                } else {
                    if self.dropped == 0 {
                        let line = format!(
                            "cyc={} pc=0x{:08x} USB_DEVICE ep1 write with the IN FIFO {} \
                             (host absent): byte 0x{byte:02x} dropped",
                            cx.now,
                            cx.pc,
                            if self.sealed { "committed" } else { "full" }
                        );
                        cx.trace.note(&line);
                    }
                    self.dropped += 1;
                }
            }
            EP1_CONF => {
                if value & EP1_CONF_WR_DONE != 0 {
                    if self.in_fifo.is_empty() {
                        // Committing nothing: the endpoint is empty at once.
                        self.int_raw |= INT_SERIAL_IN_EMPTY;
                    } else if !self.sealed {
                        self.sealed = true;
                        self.sealed_at = Some(cx.now);
                        let line = format!(
                            "cyc={} pc=0x{:08x} USB_DEVICE wr_done: {} bytes committed to the IN \
                             endpoint; no host will drain them (serial_in_ep_data_free stays 0)",
                            cx.now,
                            cx.pc,
                            self.in_fifo.len()
                        );
                        cx.trace.note(&line);
                    }
                    self.update_lines(cx);
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
            INT_RAW | INT_ST => {}
            other => self.regs.poke(other, value),
        }
    }

    /// Bytes the guest handed to the IN endpoint (at most 64).
    pub fn in_fifo(&self) -> &[u8] {
        &self.in_fifo
    }

    pub fn sealed(&self) -> bool {
        self.sealed
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

impl Peripheral for UsbSerialJtag {
    fn name(&self) -> &'static str {
        "USB_DEVICE"
    }

    fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
        lane_of(self.read_word(off & !3), off, width)
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

    fn on_event(&mut self, _id: EventId, _cx: &mut BusCx<'_>) {}

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::USB_DEVICE.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x100 + IN_FIFO_DEPTH + 32);
        out.extend_from_slice(&self.int_raw.to_le_bytes());
        out.extend_from_slice(&u32::from(self.sealed).to_le_bytes());
        out.extend_from_slice(&self.dropped.to_le_bytes());
        out.extend_from_slice(&self.sealed_at.unwrap_or(u64::MAX).to_le_bytes());
        out.extend_from_slice(&(self.in_fifo.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.in_fifo);
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let (Some(int_raw), Some(sealed), Some(dropped), Some(sealed_at), Some(len)) =
            (r.u32(), r.u32(), r.u64(), r.u64(), r.u32())
        else {
            log::warn!("USB_DEVICE: load_state blob too short, ignored");
            return;
        };
        let len = len as usize;
        if r.0.len() < len {
            return;
        }
        let (fifo, rest) = r.0.split_at(len);
        self.int_raw = int_raw;
        self.sealed = sealed != 0;
        self.dropped = dropped;
        self.sealed_at = (sealed_at != u64::MAX).then_some(sealed_at);
        self.in_fifo = fifo.to_vec();
        self.regs.load_state(rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::{ByteLog, Sandbox};

    fn rig() -> (Sandbox, UsbSerialJtag, ByteLog) {
        let mut sb = Sandbox::new();
        let (id, log) = sb.host.add_memory("usb-sj");
        (sb, UsbSerialJtag::new(Some(id)), log)
    }

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
    }

    #[test]
    fn esp_println_fills_64_bytes_spins_once_latches_timed_out_and_falls_silent() {
        let (mut sb, mut u, log) = rig();
        let mut timed_out = false;
        let line = b"[INIT] a line of esp-println output that is longer than sixty-four bytes\n";
        esp_println_write(&mut sb, &mut u, line, &mut timed_out);
        assert!(timed_out, "the 50,000-iteration wait ended in TIMED_OUT");
        assert!(u.sealed());
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
        for _ in 0..10 {
            assert_eq!(sb.read(&mut u, INT_RAW) & INT_SOF, 0);
            sb.write(&mut u, INT_CLR, INT_SOF);
        }
        assert_eq!(sb.read(&mut u, EP1_CONF) & 0b100, 0, "no OUT data avail");
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
        let mut other = UsbSerialJtag::new(None);
        other.load_state(&blob);
        assert!(other.sealed());
        assert_eq!(other.in_fifo(), b"A");
        assert_eq!(other.regs.stored(INT_ENA), INT_SERIAL_IN_EMPTY);
        assert_eq!(other.int_raw, u.int_raw);
    }
}
