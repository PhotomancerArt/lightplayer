//! `USB_DEVICE` (USB-Serial-JTAG) at `0x6003_8000` — **the S3's parameters**
//! for the shared view in [`lp_emu_esp_common::ip::usb_sj`].
//!
//! The model is the C6's, moved rather than copied (ruling D1 (b) / DD64):
//! the host's three states — absent, attached with the port closed, attached
//! with an application draining it — the SOF cadence, the drain latencies,
//! the auto-commit at 64 bytes, the bus reset that drops a committed packet.
//! Read that module for what the block *does*; this file is only what the S3
//! *is*.
//!
//! # What the S3 does not have, and what follows
//!
//! ⚠️ `0x4c`…`0x7c` is **reserved** on this part
//! (`esp32s3-0.35.2/src/usb_device.rs:22`). Missing: `chip_rst 0x4c`,
//! `set_line_code_w0/w1`, `get_line_code_w0/w1`, `config_update`,
//! `ser_afifo_config`, `bus_reset_st 0x68`. And `int_raw` bits 12–15
//! (`rts_chg`, `dtr_chg`, `get_line_code`, `set_line_code`) are C6-only. So
//! this chip supplies **no** [`HostReset`](lp_emu_esp_common::ip::usb_sj::HostReset)
//! capability and the narrow [`INT_MASK_CORE`] mask. Three consequences,
//! each of them a thing a reader who knows the C6 will look for:
//!
//! 1. **The guest cannot refuse a reset over the serial channel.** There is
//!    no `chip_rst` bit 2 (`disable_usb_serial_chip_reset`), so the control
//!    channel's `reset` and `download-mode` are **unconditional** here and
//!    the C6's `err` reply naming the bit has no S3 counterpart.
//! 2. **The firmware cannot see DTR or RTS**, and does not try:
//!    `UsbConnectionMonitor::poll` reads `int_raw.sof` and clears it, and
//!    nothing else (`lp-fw/fw-esp32s3/src/board/esp32s3/usb_connection.rs:29-32`).
//!    The `dtr`/`rts`/`signals` verbs still exist on the control channel — a
//!    host really does assert those lines, and esptool's dances are made of
//!    them — they simply reach no guest-visible register.
//! 3. `test` resets to `0` here and to `0x30` on the C6, and `bus_reset_st`
//!    does not exist to be released. Both are the generated table's business
//!    ([`crate::regs::USB_DEVICE`]) and never the shared view's.
//!
//! # Grades: everything is `modeled`, and that is the honest answer
//!
//! ⚠️ **A transcript recorded on a C6 is a measurement of a C6.** The six
//! `measured` grades the C6's block carries were bought by four committed
//! transcripts under `lp-emu/transcripts/esp32c6/`, replayed against silicon
//! captures of that chip. No S3 silicon has been read (P09 owns that), so
//! [`GRADES`] is empty and every register of this block answers
//! [`RegGrade::Modeled`](lp_emu_esp_common::RegGrade::Modeled). A run under
//! `--strict-grade documented` therefore stops at the first register the
//! console touches — which is correct, and is what the flag is for.
//!
//! # The console is this block
//!
//! The shipped image's console is `esp-println` with the **`jtag-serial`**
//! feature (`lp-fw/fw-esp32s3/Cargo.toml:147`) — a raw-MMIO printer straight
//! into `ep1`. There is **no `spike_uart0_link` build on this chip** and the
//! firmware says so: *"The S3 has no `spike_uart0_link` build, so its link is
//! always the real USB one and SOF always gates writes"*
//! (`board/esp32s3/usb_connection.rs:9-10`). One image, one link.

use lp_emu_esp_common::StreamId;
pub use lp_emu_esp_common::ip::usb_sj::{
    Config, HostState, IN_DRAIN_LATENCY_US, IN_FIFO_DEPTH, INT_MASK_CORE, INT_SERIAL_IN_EMPTY,
    INT_SERIAL_OUT_RECV_PKT, INT_SOF, INT_USB_BUS_RESET, OUT_LAND_LATENCY_US, OUT_PACKET_MAX,
    SOF_PERIOD_US, UsbSerialJtag,
};
use lp_emu_esp_common::{RegGrade, ip::usb_sj as ip};

use crate::memmap;
use crate::regs::{self, source};

/// The block's aperture: `date` is at `+0x80` and the PAC's block is a 4 KiB
/// slot, but the view only ever answers the first page — the same 0x100 the
/// C6 registers, so a read past it is an unmapped stop rather than a zero.
pub const USB_DEVICE_LEN: u32 = 0x100;

/// How often a **live** host source (a socket) is re-polled, in cycles: one
/// emulated millisecond, both siblings' number. Wall clock decides when a
/// socket's bytes appear, so the model asks on a grid in guest time.
pub const LIVE_POLL_CYCLES: u64 = 1_000 * memmap::CYCLES_PER_US;

/// **Empty, on purpose.** See the module docs: no S3 silicon has been read,
/// so nothing here is above `Modeled`, and inheriting the C6's promotions
/// would be reporting one chip's measurement as another's.
pub const GRADES: &[(u32, RegGrade)] = &[];

/// The S3's `USB_DEVICE`.
pub static CONFIG: Config = Config {
    base: memmap::periph::USB_DEVICE,
    len: USB_DEVICE_LEN,
    source: source::USB_DEVICE,
    regs: &regs::USB_DEVICE,
    cycles_per_us: memmap::CYCLES_PER_US,
    int_mask: INT_MASK_CORE,
    live_poll_cycles: LIVE_POLL_CYCLES,
    grades: GRADES,
    // ⚠️ The whole point: this chip has no `chip_rst` and no
    // `bus_reset_st`, so the capability is withheld rather than faked.
    host_reset: None,
};

/// Cycles between SOFs on this chip.
pub const SOF_PERIOD_CYCLES: u64 = SOF_PERIOD_US * memmap::CYCLES_PER_US;
/// [`IN_DRAIN_LATENCY_US`] in this chip's cycles.
pub const IN_DRAIN_LATENCY_CYCLES: u64 = IN_DRAIN_LATENCY_US * memmap::CYCLES_PER_US;
/// [`OUT_LAND_LATENCY_US`] in this chip's cycles.
pub const OUT_LAND_LATENCY_CYCLES: u64 = OUT_LAND_LATENCY_US * memmap::CYCLES_PER_US;

/// The S3's block. `delivered` is the `usb-sj` stream (what a host receives
/// / sends); `tried` the observation stream; `host` the state at power-on.
pub fn new(delivered: Option<StreamId>, tried: Option<StreamId>, host: HostState) -> UsbSerialJtag {
    UsbSerialJtag::new(&CONFIG, delivered, tried, host)
}

/// No host, one observation stream.
pub fn absent(tried: Option<StreamId>) -> UsbSerialJtag {
    UsbSerialJtag::absent(&CONFIG, tried)
}

/// Every register this block grades `Modeled`, in offset order — which on
/// this chip is **every register it has**.
pub fn modeled_registers() -> Vec<&'static str> {
    ip::modeled_registers(&CONFIG)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::{Peripheral, Sandbox};

    /// The parameters, read back: a wrong base or a wrong source is the one
    /// class of mistake the shared view cannot catch for us.
    #[test]
    fn the_s3s_parameters_are_the_s3s() {
        assert_eq!(CONFIG.base, 0x6003_8000);
        assert_eq!(CONFIG.len, 0x100);
        assert_eq!(CONFIG.source, 96, "the PAC's USB_DEVICE source");
        assert_eq!(CONFIG.regs.block, "usb_device");
        assert_eq!(CONFIG.cycles_per_us, 240);
        assert_eq!(CONFIG.int_mask, 0x0fff, "bits 0-11 and no more");
        assert!(
            CONFIG.host_reset.is_none(),
            "0x4c..=0x7c is reserved on this part"
        );
        assert!(CONFIG.grades.is_empty(), "no S3 silicon has been read");
    }

    /// The reset registers, against **this chip's** table: the four the
    /// block computes from the link's own state are the exceptions, and the
    /// shared view's own tests are what check those.
    #[test]
    fn every_register_the_pac_gives_a_reset_answers_it_and_test_resets_to_zero() {
        let mut sb = Sandbox::new();
        let mut u = absent(None);
        u.attached(0);
        u.started(&mut sb.cx());
        let computed = [ip::EP1_CONF, ip::INT_RAW, ip::IN_EP1_ST, ip::OUT_EP1_ST];
        for (off, want) in regs::USB_DEVICE.resets {
            if computed.contains(off) {
                continue;
            }
            assert_eq!(
                sb.read(&mut u, *off),
                *want,
                "USB_DEVICE+{off:#05x} {}",
                regs::USB_DEVICE.name(*off).unwrap_or("?")
            );
        }
        assert_eq!(sb.read(&mut u, ip::EP1_CONF), 0x02, "the PAC reset");
        assert_eq!(sb.read(&mut u, ip::INT_RAW), 0x08, "serial_in_empty");
        assert_eq!(sb.read(&mut u, ip::CONF0), 0x4200);
        // ⚠️ `test` is 0x30 on the C6 and 0 here. The shared view never
        // seeds a reset value; the chip's generated table does.
        assert_eq!(sb.read(&mut u, 0x1c), 0, "`test`, the S3's PAC reset");
        // And the two the C6 has: reserved words, accepted and remembered.
        assert_eq!(sb.read(&mut u, 0x4c), 0, "no chip_rst here");
        assert_eq!(sb.read(&mut u, 0x68), 0, "no bus_reset_st here");
    }

    /// Nothing is above `Modeled`, so the list is the whole table — and the
    /// eight registers the C6 has are absent from it, because they are
    /// absent from the chip.
    #[test]
    fn every_register_is_modeled_and_the_c6s_extra_eight_are_not_here() {
        let modeled = modeled_registers();
        assert_eq!(modeled.len(), regs::USB_DEVICE.entries.len(), "{modeled:?}");
        for m in ["ep1", "ep1_conf", "int_raw", "int_st", "int_ena", "int_clr"] {
            assert!(
                modeled.contains(&m),
                "`{m}` is measured on the C6, not here"
            );
        }
        for absent in [
            "chip_rst",
            "set_line_code_w0",
            "config_update",
            "ser_afifo_config",
            "bus_reset_st",
        ] {
            assert!(!modeled.contains(&absent), "`{absent}` is a C6 register");
        }
    }

    /// The control channel's `reset` is **unconditional** on this chip: the
    /// bit that would have refused it does not exist.
    #[test]
    fn a_serial_channel_reset_is_unconditional_because_there_is_no_chip_rst() {
        let mut sb = Sandbox::new();
        let mut u = new(None, None, HostState::Attached { draining: true });
        u.attached(0);
        u.started(&mut sb.cx());
        // A guest that wrote the C6's disable bit changes nothing here.
        sb.write(&mut u, 0x4c, 0b100);
        assert!(!u.chip_reset_disabled());
        assert!(u.reset(&mut sb.cx()), "performed");
        assert!(matches!(
            sb.request,
            Some(lp_emu_esp_common::MachineRequest::Reset {
                strap: lp_emu_esp_common::Strap::App,
                ..
            })
        ));
    }

    /// The host's DTR/RTS reach no guest-visible register — and the verbs
    /// still exist, because a host really does assert those lines.
    #[test]
    fn dtr_and_rts_reach_no_register_on_this_chip() {
        let mut sb = Sandbox::new();
        let mut u = new(None, None, HostState::Attached { draining: true });
        u.attached(0);
        u.started(&mut sb.cx());
        sb.write(&mut u, ip::INT_CLR, CONFIG.int_mask);
        u.set_signals(Some(true), Some(false), &mut sb.cx());
        assert_eq!(
            sb.read(&mut u, ip::INT_RAW) & 0xf000,
            0,
            "bits 12-15 are not declared on this part"
        );
    }
}
