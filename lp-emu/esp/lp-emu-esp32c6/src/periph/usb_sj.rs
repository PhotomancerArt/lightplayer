//! `USB_DEVICE` (USB-Serial-JTAG) at `0x6000_F000` — **the C6's parameters**
//! for the shared view in [`lp_emu_esp_common::ip::usb_sj`].
//!
//! The model itself — the host's three states, the SOF cadence, the drain
//! latencies, the auto-commit at 64 bytes, the detach-drops-a-committed-packet
//! rule, and the six `measured` register grades the four committed C6
//! transcripts bought — moved to `lp-emu-esp-common` in Xtensa M6 P05 so the
//! S3's link is the same file rather than a copy (ruling D1 (b) / DD64). Read
//! that module for what the block *does*; this file is only what the C6
//! *is*. **No transcript moved, and no behaviour changed**:
//! `scripts/emu/oracle-sweep.sh` is what says so.
//!
//! The C6 has the two registers the S3's silicon lacks — `chip_rst 0x4c` and
//! `bus_reset_st 0x68` — so it supplies the [`HostReset`] capability, and it
//! has the CDC and modem-line interrupt bits, so its mask is
//! [`INT_MASK_WITH_CDC`].

use lp_emu_esp_common::StreamId;
pub use lp_emu_esp_common::ip::usb_sj::{
    Config, HostReset, HostState, IN_DRAIN_LATENCY_US, IN_FIFO_DEPTH, INT_DTR_CHG,
    INT_IN_TOKEN_REC_IN_EP1, INT_MASK_WITH_CDC, INT_RTS_CHG, INT_SERIAL_IN_EMPTY,
    INT_SERIAL_OUT_RECV_PKT, INT_SOF, INT_USB_BUS_RESET, OUT_LAND_LATENCY_US, OUT_PACKET_MAX,
    SOF_PERIOD_US, UsbSerialJtag,
};
use lp_emu_esp_common::{RegGrade, ip::usb_sj as ip};

use super::uart::LIVE_POLL_CYCLES;
use crate::memmap;
use crate::regs::{self, source};

/// `chip_rst` and `bus_reset_st`: the C6 has both, and the view gives both
/// behaviour. `bus_reset_st`'s PAC reset is `0x01` (discovery §1).
const HOST_RESET: HostReset = HostReset {
    chip_rst: 0x4c,
    bus_reset_st: 0x68,
    bus_reset_st_reset: 0x01,
};

/// M6 P4's promotions, **each backed by a committed transcript** under
/// `lp-emu/transcripts/esp32c6/`. Anything unlisted is
/// [`RegGrade::Modeled`]; the shared module's header lists those registers by
/// name with the reason the firmware never reaches them.
const GRADES: &[(u32, RegGrade)] = &[
    (ip::EP1, RegGrade::Measured),
    (ip::EP1_CONF, RegGrade::Measured),
    (ip::INT_RAW, RegGrade::Measured),
    (ip::INT_ST, RegGrade::Measured),
    (ip::INT_ENA, RegGrade::Measured),
    (ip::INT_CLR, RegGrade::Measured),
    (ip::FRAM_NUM, RegGrade::Documented),
    (ip::CONF0, RegGrade::Documented),
];

/// The C6's `USB_DEVICE`.
pub static CONFIG: Config = Config {
    base: memmap::periph::USB_DEVICE,
    len: 0x100,
    source: source::USB_DEVICE,
    regs: &regs::USB_DEVICE,
    cycles_per_us: memmap::CYCLES_PER_US,
    int_mask: INT_MASK_WITH_CDC,
    live_poll_cycles: LIVE_POLL_CYCLES,
    grades: GRADES,
    host_reset: Some(HOST_RESET),
};

/// Cycles between SOFs on this chip.
pub const SOF_PERIOD_CYCLES: u64 = SOF_PERIOD_US * memmap::CYCLES_PER_US;
/// [`IN_DRAIN_LATENCY_US`] in this chip's cycles.
pub const IN_DRAIN_LATENCY_CYCLES: u64 = IN_DRAIN_LATENCY_US * memmap::CYCLES_PER_US;
/// [`OUT_LAND_LATENCY_US`] in this chip's cycles.
pub const OUT_LAND_LATENCY_CYCLES: u64 = OUT_LAND_LATENCY_US * memmap::CYCLES_PER_US;

/// The C6's block. `delivered` is the `usb-sj` stream (what a host receives
/// / sends); `tried` the observation stream; `host` the state at power-on.
pub fn new(delivered: Option<StreamId>, tried: Option<StreamId>, host: HostState) -> UsbSerialJtag {
    UsbSerialJtag::new(&CONFIG, delivered, tried, host)
}

/// P6's shape: no host, one observation stream.
pub fn absent(tried: Option<StreamId>) -> UsbSerialJtag {
    UsbSerialJtag::absent(&CONFIG, tried)
}

/// Every register this block grades `Modeled`, in offset order — the list
/// the README and `--strict-grade` both mean by "the modeled registers".
pub fn modeled_registers() -> Vec<&'static str> {
    ip::modeled_registers(&CONFIG)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Peripheral;

    /// The parameters, read back: a wrong base or a wrong source is the one
    /// class of mistake the shared module cannot catch for us.
    #[test]
    fn the_c6s_parameters_are_the_c6s() {
        assert_eq!(CONFIG.base, 0x6000_F000);
        assert_eq!(CONFIG.len, 0x100);
        assert_eq!(CONFIG.source, 48, "the PAC's USB_DEVICE source");
        assert_eq!(CONFIG.regs.block, "usb_device");
        assert_eq!(CONFIG.cycles_per_us, memmap::CYCLES_PER_US);
        assert_eq!(CONFIG.int_mask, 0xffff, "the CDC and modem-line bits too");
        let hr = CONFIG.host_reset.expect("the C6 has chip_rst");
        assert_eq!(hr.chip_rst, 0x4c);
        assert_eq!(hr.bus_reset_st, 0x68);
        assert_eq!(hr.bus_reset_st_reset, 0x01);
        assert_eq!(regs::USB_DEVICE.name(hr.chip_rst), Some("chip_rst"));
        assert_eq!(regs::USB_DEVICE.name(hr.bus_reset_st), Some("bus_reset_st"));
        assert_eq!(regs::USB_DEVICE.reset(hr.bus_reset_st), Some(0x01));
    }

    /// The reset registers, against **this chip's** table rather than the
    /// shared module's fixture: the four the block computes from the link's
    /// own state are the exceptions, and the shared module's own tests are
    /// what check those.
    #[test]
    fn every_register_the_pac_gives_a_reset_answers_it() {
        let mut sb = lp_emu_esp_common::Sandbox::new();
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
        // `test` resets to 0x30 on this chip and to 0 on the S3 — a chip
        // number, and the reason the reset values are the table's.
        assert_eq!(sb.read(&mut u, 0x1c), 0x30);
        assert_eq!(sb.read(&mut u, 0x68), 0x01, "bus_reset_st, released");
    }

    /// The list the README publishes is generated from the table, so the two
    /// cannot drift: a register promoted here leaves the list by itself.
    #[test]
    fn the_modeled_register_list_is_the_table_read_back() {
        let modeled = modeled_registers();
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
}
