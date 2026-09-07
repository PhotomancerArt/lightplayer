//! `WIFI_MAC` — the radio window `0x600A_0000..0x600A_9800` as **one**
//! accept-and-remember block, driven by the spin detector (plan D5).
//!
//! # What is here
//!
//! The esp-radio blob touches ≈1,088 register sites in this window (spike
//! report §8: 169 offsets, 762 sites directly, plus the spill-over the
//! inventory mis-attributed to MODEM_SYSCON and LP_APM0). None of it is
//! documented: the PAC names only the `IEEE802154` block at `0x600A_3000`
//! and calls the rest nothing at all. So there is **no generated name
//! table** for this window — the coarse names below are ours, by address
//! range, and the provenance is this paragraph: the WiFi MAC/BB/PWR layout
//! is Espressif's closed blob, and `+0xNNNN` is the most honest name a
//! register here can have.
//!
//! # How it is driven
//!
//! Accept-and-remember, with a read-override list that starts **empty** and
//! grows one entry per `SPIN` line the trace shows on the default image:
//! every bit the blob polls is a status bit its hardware would have set,
//! and the override is the value that lets the poll exit, each with the
//! evidence beside it ([`OVERRIDES`]). Every distinct offset the blob
//! touches leaves one `WIFI_MAC TOUCH` note in the trace the first time, so
//! a run's log is the list of what the blob reached; the DMA base the blob
//! programs for its RX path is reported as the `WIFI RX config` line — the
//! vision's first artefact of the virtual-air work.
//!
//! Interrupt sources 0–3 (`WIFI_MAC`, `WIFI_MAC_NMI`, `WIFI_PWR`, `WIFI_BB`)
//! are never raised: nothing here receives.

use std::collections::BTreeSet;

use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

/// The window (`memmap::periph::MODEM_WINDOW`, up to `MODEM_SYSCON`).
pub const WINDOW_LEN: u32 = 0x9800;

/// Coarse names by range: `(start, end, name)`. Ours, not the PAC's — see
/// the module docs.
pub const COARSE_NAMES: &[(u32, u32, &str)] = &[
    // The WiFi MAC proper: esp-radio's `mac_txrx_init`, interrupt clear and
    // the RX DMA configuration land here.
    (0x0000, 0x3000, "mac"),
    // The PAC's `IEEE802154` block (`esp32c6-0.23.2/src/lib.rs`,
    // `IEEE802154: 0x600a_3000`) — named by the PAC, but esp-radio's WiFi
    // path does not run the 802.15.4 driver.
    (0x3000, 0x4000, "ieee802154"),
    // Baseband / RF calibration / power: `rfcal_*`, `hal_init_imrsp_power`.
    (0x4000, 0x9800, "bb"),
];

/// Coarse names for the second block, `WIFI_PWR`
/// (`memmap::periph::WIFI_PWR`, the gap between MODEM_SYSCON and
/// MODEM_LPCON). Offsets are from `0x600A_9900`; the ROM's `tsf_hal_*`
/// (TBTT and SoC wake-up) reads `0x600A_D050` = `+0x3750` first.
pub const PWR_COARSE_NAMES: &[(u32, u32, &str)] = &[(0x0000, 0x5700, "pwr")];

/// One override the blob demanded: `(offset, mask, value, why)`.
///
/// Each entry is a status bit a `SPIN` line showed the blob polling on the
/// default image; the value is the one that lets the poll exit. **Starts
/// empty and grows by evidence** — never add one without the `SPIN` line
/// that asked for it.
pub const OVERRIDES: &[(u32, u32, u32, &str)] = &[
    // `SPIN WIFI_MAC+0x418 mac = 0x00000003 x10000` from `txdc_cal_new`
    // (`0x4221dd98` in the memfs reference image, 30.2 ms into the boot).
    // The blob read-modify-writes `+0x418` to set bit 1, clear bit 0, set
    // bit 0 (a start strobe), then loops `lw a4,0x418(s5); slli a3,a4,9;
    // bgez a3` — until **bit 22** reads 1, the calibration's done flag —
    // and afterwards tests bit 29 (`slli a3,a4,2; bltz`) as an error flag.
    // Blob spins here; esp-emu evidently satisfies it; value chosen so the
    // poll exits (bit 22 set, bit 29 left 0 so the no-error path is taken).
    (0x0418, 1 << 22, 1 << 22, "txdc_cal_new done flag"),
    // `SPIN WIFI_MAC+0x814 mac = 0x00003008 x10000` from
    // `ram_pwdet_tone_start` (`0x4221b06c`, 30.4 ms). The blob strobes
    // `+0x810` bit 0 and loops `lw a5,0x814(a3); srli a5,a5,14; andi a5,a5,7;
    // bne a5,a4(=7)` — until the 3-bit state field at bits 14:16 reads 7.
    // Blob spins here; esp-emu evidently satisfies it; value chosen so the
    // poll exits (the field reads 7).
    (0x0814, 0x7 << 14, 0x7 << 14, "ram_pwdet_tone_start state field == 7"),
    // `SPIN WIFI_MAC+0x0cc mac = 0x25824e50 x10000` from
    // `ram_set_chan_freq_sw_start` (`0x4221a72a`, 30.3 ms): after the ROM's
    // `freq_chan_en_sw` and a 10 µs delay it loops `lw a5,0xcc(a4); andi
    // a5,a5,256; beqz a5` — until **bit 8** reads 1, the channel/frequency
    // lock flag (`freq_reg_init` wrote the register with bit 8 clear). Blob
    // spins here; esp-emu evidently satisfies it; value chosen so the poll
    // exits.
    (0x00cc, 1 << 8, 1 << 8, "ram_set_chan_freq_sw_start lock flag"),
];

/// The `WIFI_PWR` block's override list; same rule, same shape.
pub const PWR_OVERRIDES: &[(u32, u32, u32, &str)] = &[];

/// Coarse names for the third block, `I2C_MST_MEM`
/// (`memmap::periph::I2C_MST_MEM`, the analog I2C master's burst command
/// memory at `I2C_ANA_MST + 0x400`). Not in the PAC (`i2c_ana_mst` ends at
/// `date`, `+0x34`). P6's first G6-2 finding: the memfs boot-idle image ran
/// strict to 27.8 ms and stopped on
/// `W4 0x600afc00 = 0x00060267 from phy_i2c_master_cmd_mem_init+0xc`. That
/// the words are the analog master's burst commands is an inference from
/// the writer's name, the PAC's `burst_conf`/`burst_status` pair and the
/// address (`+0x400` from the master's own block); nothing here executes
/// them.
pub const I2C_MST_MEM_COARSE_NAMES: &[(u32, u32, &str)] = &[(0x0000, 0x0400, "cmd_mem")];

/// `WIFI_PWR + 0x3700` (`0x600A_D000`): a free-running **microsecond
/// counter**, the one register in either block that is live rather than
/// remembered. Evidence: the blob's `wait_i2c_sdm_stable`
/// (`.rwtext.wifi`, `0x4080_C820` in the reference image) latches it, then
/// loops until the I2C SDM reads back `0x5B` *or* the register has advanced
/// by `0x270F` = 9,999 — a 10 ms timeout on a 1 MHz clock. A remembered 0
/// never advances and the boot never leaves that loop (8.77 M reads in
/// 5.5 s of the first P6 run). *Modeled*: `cycles / 160`, from reset;
/// nothing measured which clock the chip feeds it.
pub const PWR_MICROS_COUNTER: u32 = 0x3700;

/// The register the blob programs with its RX DMA descriptor base, reported
/// as the `WIFI RX config` trace line. `None` until the trace shows which
/// offset receives a DRAM address (esp-emu's trace called it
/// `dma_base=0x15DBC`, without saying where it read it).
pub const RX_DMA_BASE_OFFSET: Option<u32> = None;

/// The radio window (and, as a second instance, the `WIFI_PWR` gap).
#[derive(Debug)]
pub struct WifiStub {
    name: &'static str,
    names: &'static [(u32, u32, &'static str)],
    regs: RegFile,
    touched: BTreeSet<u32>,
}

impl WifiStub {
    /// `WIFI_MAC`: `0x600A_0000..0x600A_9800`.
    pub fn new() -> Self {
        let mut regs = RegFile::new("WIFI_MAC", WINDOW_LEN);
        for &(off, mask, value, _) in OVERRIDES {
            regs = regs.with_read_override(off, mask, value);
        }
        Self {
            name: "WIFI_MAC",
            names: COARSE_NAMES,
            regs,
            touched: BTreeSet::new(),
        }
    }

    /// `WIFI_PWR`: `0x600A_9900..0x600A_F000`, the same accept-and-remember
    /// with its own (so far empty) override list — every entry must carry
    /// the `SPIN` line that asked for it, as for [`OVERRIDES`].
    pub fn pwr() -> Self {
        let mut regs = RegFile::new("WIFI_PWR", crate::memmap::periph::WIFI_PWR_LEN);
        for &(off, mask, value, _) in PWR_OVERRIDES {
            regs = regs.with_read_override(off, mask, value);
        }
        Self {
            name: "WIFI_PWR",
            names: PWR_COARSE_NAMES,
            regs,
            touched: BTreeSet::new(),
        }
    }

    /// `I2C_MST_MEM`: `0x600A_FC00..0x600B_0000`, the PHY's I2C burst command
    /// memory as a plain accept-and-remember block with the touch log — the
    /// PHY writes its command words here at init and the analog master
    /// executes them from it; nothing reads them back from the CPU side, so
    /// remembering is the whole model. No override list: no `SPIN` has ever
    /// landed here.
    pub fn i2c_mst_mem() -> Self {
        Self {
            name: "I2C_MST_MEM",
            names: I2C_MST_MEM_COARSE_NAMES,
            regs: RegFile::new("I2C_MST_MEM", crate::memmap::periph::I2C_MST_MEM_LEN),
            touched: BTreeSet::new(),
        }
    }

    /// Distinct offsets the guest has touched so far.
    pub fn touched(&self) -> &BTreeSet<u32> {
        &self.touched
    }

    fn note_touch(&mut self, off: u32, access: &str, cx: &mut BusCx<'_>) {
        let word = off & !3;
        if self.touched.insert(word) && cx.trace.is_enabled() {
            let name = self.reg_name(word).unwrap_or("?");
            let line = format!(
                "cyc={} pc=0x{:08x} {} TOUCH +0x{word:04x} {name} ({access}; {} distinct so far)",
                cx.now,
                cx.pc,
                self.name,
                self.touched.len()
            );
            cx.trace.note(&line);
        }
    }
}

impl Default for WifiStub {
    fn default() -> Self {
        Self::new()
    }
}

impl Peripheral for WifiStub {
    fn name(&self) -> &'static str {
        self.name
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        self.note_touch(off, "R", cx);
        let word = off & !3;
        if self.name == "WIFI_PWR" && word == PWR_MICROS_COUNTER {
            let micros = (cx.now / crate::memmap::CYCLES_PER_US) as u32;
            return lane_of(micros, off, width);
        }
        lane_of(self.regs.effective(off), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        self.note_touch(off, "W", cx);
        let word = off & !3;
        let merged = merge_lane(self.regs.stored(word), off, width, value);
        self.regs.poke(word, merged);
        if self.name == "WIFI_MAC" && Some(word) == RX_DMA_BASE_OFFSET && cx.trace.is_enabled() {
            let line = format!(
                "cyc={} pc=0x{:08x} WIFI RX config: dma_base=0x{merged:08x}",
                cx.now, cx.pc
            );
            cx.trace.note(&line);
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        self.names
            .iter()
            .find(|(start, end, _)| off >= *start && off < *end)
            .map(|(_, _, name)| *name)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(WINDOW_LEN as usize + 4 * self.touched.len() + 4);
        out.extend_from_slice(&(self.touched.len() as u32).to_le_bytes());
        for off in &self.touched {
            out.extend_from_slice(&off.to_le_bytes());
        }
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let Some((head, mut rest)) = bytes.split_first_chunk::<4>() else {
            return;
        };
        let n = u32::from_le_bytes(*head) as usize;
        self.touched.clear();
        for _ in 0..n {
            let Some((off, r)) = rest.split_first_chunk::<4>() else {
                return;
            };
            self.touched.insert(u32::from_le_bytes(*off));
            rest = r;
        }
        self.regs.load_state(rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;
    use lp_emu_esp_common::trace::SharedBuffer;

    #[test]
    fn the_window_remembers_writes_names_coarsely_and_logs_each_offset_once() {
        let buf = SharedBuffer::new();
        let mut sb = Sandbox::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut w = WifiStub::new();
        assert_eq!(w.reg_name(0x0010), Some("mac"));
        assert_eq!(w.reg_name(0x3004), Some("ieee802154"));
        assert_eq!(w.reg_name(0x8000), Some("bb"));
        assert_eq!(w.reg_name(0x9800), None);

        sb.write(&mut w, 0x1234, 0xdead_beef);
        assert_eq!(sb.read(&mut w, 0x1234), 0xdead_beef);
        sb.read(&mut w, 0x1234);
        sb.read(&mut w, 0x97fc);
        assert_eq!(w.touched().len(), 2);
        let touches: Vec<String> = buf
            .lines()
            .into_iter()
            .filter(|l| l.contains("WIFI_MAC TOUCH"))
            .collect();
        assert_eq!(touches.len(), 2, "{touches:?}");
        assert!(touches[0].contains("WIFI_MAC TOUCH +0x1234 mac (W; 1 distinct so far)"));
        assert!(touches[1].contains("+0x97fc bb (R; 2 distinct so far)"));

        let mut c = WifiStub::i2c_mst_mem();
        assert_eq!(c.name(), "I2C_MST_MEM");
        assert_eq!(c.reg_name(0x0000), Some("cmd_mem"));
        assert_eq!(c.reg_name(0x0400), None);
        sb.write(&mut c, 0x0000, 0x0006_0267);
        assert_eq!(sb.read(&mut c, 0x0000), 0x0006_0267, "a command word is remembered");

        let mut p = WifiStub::pwr();
        assert_eq!(p.name(), "WIFI_PWR");
        assert_eq!(p.reg_name(0x3750), Some("pwr"));
        assert_eq!(p.reg_name(0x5700), None);
        sb.write(&mut p, 0x3750, 7);
        assert_eq!(sb.read(&mut p, 0x3750), 7);
        // The microsecond counter advances with guest time; `wait_i2c_sdm_stable`
        // gives up after 9,999 of them.
        sb.now = 0;
        let t0 = sb.read(&mut p, PWR_MICROS_COUNTER);
        sb.now = 10_000 * crate::memmap::CYCLES_PER_US;
        assert_eq!(sb.read(&mut p, PWR_MICROS_COUNTER) - t0, 10_000);
        sb.write(&mut p, PWR_MICROS_COUNTER, 0);
        assert_eq!(
            sb.read(&mut p, PWR_MICROS_COUNTER),
            10_000,
            "a write does not stop it"
        );

        let blob = w.save_state();
        let mut other = WifiStub::new();
        other.load_state(&blob);
        assert_eq!(other.touched(), w.touched());
        assert_eq!(other.regs.stored(0x1234), 0xdead_beef);
    }

    #[test]
    fn the_override_list_is_applied_and_every_entry_has_a_reason() {
        for &(off, mask, _, why) in OVERRIDES {
            assert!(off < WINDOW_LEN && mask != 0 && !why.is_empty());
        }
        let mut sb = Sandbox::new();
        let mut w = WifiStub::new();
        for &(off, mask, value, _) in OVERRIDES {
            sb.write(&mut w, off, !value);
            assert_eq!(sb.read(&mut w, off) & mask, value & mask, "+0x{off:04x}");
        }
    }
}
