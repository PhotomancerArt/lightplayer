//! The radio windows — every MMIO window the esp-radio blobs touch on this
//! chip that the PAC does not name — as **accept-and-remember** blocks, the
//! C6's method (`lp-emu-esp32c6/src/periph/wifi_stub.rs`), on this chip's
//! own addresses (S3 radio roadmap M1).
//!
//! # Where the windows come from
//!
//! None of it is documented. `esp32s3-0.35.2/src/lib.rs` places no block at
//! `0x6001_1000`, `0x6001_C000`, `0x6003_1000`, `0x6003_3000`,
//! `0x6003_5000` or `0x6004_2000`, and stops `FE`, `NRX`, `BB` and
//! `I2C_ANA_MST` at the registers it names. So **every name here is ours,
//! by address range**, and the provenance of each window is the census that
//! found it: the `l32r` literals of the M1 radio probe image
//! (`fw-esp32s3 --features radio_boot_probe`, esp-radio 0.18.0 and
//! `esp-wifi-sys-esp32s3` 0.2.0's blobs) that land in no modelled block,
//! grouped by page, each page named after the blob routines that load it.
//! The Wi‑Fi MAC/BB/PWR, Bluetooth and PHY layouts are Espressif's closed
//! blobs, and `+0xNNNN` is the most honest name a register here can have.
//!
//! # How they are driven
//!
//! Accept-and-remember, with a read-override list per window that starts
//! **empty** and grows one entry per `SPIN` line the trace shows on the
//! probe image (each with its evidence beside it), and one `TOUCH` trace
//! note per distinct offset — so a run's log is the list of what the blobs
//! reached. A write stores; a read answers what was stored, or 0. Nothing
//! here raises an interrupt: the blobs' sources are mapped through
//! [`crate::intmatrix`] like any other, and raising RX/TX completions is
//! the emulated Wi‑Fi milestone's (M5).
//!
//! ⚠️ **No window here is graded**: [`Peripheral::reg_grade`] answers
//! `None`, the C6's `WIFI_PWR`/`COEX` posture — "nobody graded this" is not
//! "this is modelled".

use std::collections::BTreeSet;

use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

/// One radio window: its block name, its base, its length, its coarse
/// names (offsets from the base) and its read overrides.
#[derive(Clone, Copy, Debug)]
pub struct RadioWindow {
    pub name: &'static str,
    pub base: u32,
    pub len: u32,
    /// `(start, end, name)`, offsets from `base`. Ours, not the PAC's.
    pub names: &'static [(u32, u32, &'static str)],
    /// `(offset, mask, value, why)` — each a status bit a `SPIN` line showed
    /// a blob polling, and the value that lets the poll exit. **Never add
    /// one without the `SPIN` line that asked for it.**
    pub overrides: &'static [(u32, u32, u32, &'static str)],
}

/// The rest of the `FE` page above the PAC's `0x100`-byte block: the PHY's
/// "pbus" (`ram_set_pbus_reg`, `set_pbus_mem`, `save_pbus_reg`,
/// `ram_wifi_tx_dig_gain_reg`, `start_tx_tone_step`).
pub const PBUS: RadioWindow = RadioWindow {
    name: "PBUS",
    base: 0x6000_6100,
    len: 0x0f00,
    names: &[(0x000, 0xf00, "pbus")],
    overrides: &[
        // No `SPIN` line, and the reason is the loop's shape: the blob's
        // `ram_iq_est_enable` (`0x422982a0..0x422982b5` in the probe image,
        // ~23 ms) alternates two reads — `RF_CTRL+0x08c` bits 12:18 (a
        // sample count it compares with 69) and `0x6000_6174` — so the
        // single-register spin detector never fires; the run's deadline
        // found it, parked at `ram_iq_est_enable+0x9a` for 3 s emulated. The
        // exit is `l32i a3, 0x60006174; bnone a3, 0x10000` — until **bit
        // 16** reads 1, the IQ estimate's done flag, after the routine set
        // `+0x44` bit 20, a length field and bits 0 and 1 as strobes. The
        // C6's ROM `rom_iq_est_enable` exits on bit 16 of its own done
        // register the same way. Blob loops here; value chosen so it exits.
        (0x074, 1 << 16, 1 << 16, "ram_iq_est_enable done flag"),
    ],
};

/// The analog I2C master's page above the three registers the PAC names
/// (`ana_conf0..ana_config2`, which end at `+0x4c`): the PHY's frequency
/// and PLL registers (`ram_phy_dis_hw_set_freq` read-modify-writes `+0xc4`
/// bit 25 — the first stop of the M1 probe after `salt`/`saltu` —
/// `freq_i2c_write_set`, `set_chan_freq_sw_start`, `pll_cap_mem_update`,
/// `ram_i2c_master_reset`). Up to the PAC's `I2S0` at `0x6000_F000`.
pub const I2C_MST_HI: RadioWindow = RadioWindow {
    name: "I2C_MST_HI",
    base: 0x6000_E04C,
    len: 0x1000 - 0x4c,
    names: &[(0x000, 0xfb4, "i2c_mst_hi")],
    overrides: &[
        // `SPIN I2C_MST_HI+0x004 i2c_mst_hi = 0x000000c8 x10000` from the
        // mask ROM's `rom_pkdet_vol_start` (`0x4003_6a44`, 15 ms into the
        // probe's boot, under `esp_radio::wifi::new`'s PHY init). It sets
        // bits 23 and 19 of `0x6000_E05C` (start strobes), then loops
        // `l32i a8, 0x6000e050; extui a8, a8, 24, 3; bnei a8, 7` — until
        // the 3-bit field at bits 24:26 reads 7, and only then clears bit 1.
        // The C6's `ram_pwdet_tone_start` waits on the same shape (a 3-bit
        // state field == 7). Blob spins here; value chosen so the poll
        // exits.
        (
            0x004,
            0x7 << 24,
            0x7 << 24,
            "rom_pkdet_vol_start state field == 7",
        ),
        // `SPIN I2C_MST_HI+0x000 i2c_mst_hi = 0x00113cf3 x10000` from the
        // blob's `txdc_cal_v70` (`0x42299b83` in the probe image, 22.8 ms).
        // Each step writes `0x6000_E04C` with `… | 0x113cf1`, then
        // `… | 0x113cf3` (bit 1, a start strobe), and loops `l32i; bnone
        // a12, 0x0100_0000` — until **bit 24** reads 1, the step's done
        // flag. It then reads bits 31 and 30 as the comparator's two
        // directions (`bltz`, `& 0x4000_0000`) and moves its DC offset up or
        // down; both paths converge, so they are left as written (0). Blob
        // spins here; value chosen so the poll exits. The C6's
        // `txdc_cal_new` waits on its own done bit the same way.
        (0x000, 1 << 24, 1 << 24, "txdc_cal_v70 step done flag"),
    ],
};

/// Bluetooth's baseband and controller registers (`bt_bb_gain_set`,
/// `bt_bb_dpo_set`, `bt_bb_target_set`, `r_bt_bb_isr_hack`,
/// `r_rwble_isr_hack`), up to the PAC's `I2C0` at `0x6001_3000`.
pub const BT: RadioWindow = RadioWindow {
    name: "BT",
    base: 0x6001_1000,
    len: 0x2000,
    names: &[(0x0000, 0x0800, "bt_bb"), (0x0800, 0x2000, "bt_rw")],
    overrides: &[],
};

/// The RF control page below the PAC's `NRX` (`rx_agc_reg_opt`,
/// `set_rx_gain_table`, `set_rx_sense`, `rx_11b_opt`, `bt_bb_restart_set`).
pub const RF_CTRL: RadioWindow = RadioWindow {
    name: "RF_CTRL",
    base: 0x6001_C000,
    len: 0x0c00,
    names: &[(0x000, 0xc00, "rf_ctrl")],
    overrides: &[],
};

/// `NRX`'s page above the PAC's `0x100`-byte block (`+0xd04..+0xd0c` of
/// the `0x6001_C000` page in the census).
pub const NRX_HI: RadioWindow = RadioWindow {
    name: "NRX_HI",
    base: 0x6001_CD00,
    len: 0x0300,
    names: &[(0x000, 0x300, "nrx_hi")],
    overrides: &[],
};

/// `BB`'s page above the PAC's `0x100`-byte block, up to the PAC's next
/// block at `0x6001_E000`. No literal in the census lands here; it is mapped
/// because the baseband is one window on the blob's side and a computed
/// access into it would otherwise stop a strict run.
pub const BB_HI: RadioWindow = RadioWindow {
    name: "BB_HI",
    base: 0x6001_D100,
    len: 0x0f00,
    names: &[(0x000, 0xf00, "bb_hi")],
    overrides: &[],
};

/// The BLE link layer's exchange memory and core registers
/// (`r_emi_em_base_init`, `r_lld_core_init`, `r_rf_rw_v9_le_init`,
/// `rwip_aes_encrypt_sync`, `r_lld_scan_start_eco`), up to the Wi‑Fi MAC.
pub const BLE_RW: RadioWindow = RadioWindow {
    name: "BLE_RW",
    base: 0x6003_1000,
    len: 0x2000,
    names: &[(0x0000, 0x2000, "ble_rw")],
    overrides: &[
        // The mask ROM's `r_rwip_time_get` (`0x4002_bd70`, found parked
        // there at a 3 s deadline, under `BleConnector::new`): it
        // read-modify-writes `0x6003_101C` to set **bit 31** (a "sample the
        // native clock" request), then loops `l32i; bltz` — while bit 31
        // still reads 1 — and afterwards reads the clock from the same word
        // (`& 0x0fff_ffff`) and the fine count from `+0x020`. The shape of
        // a self-clearing strobe, as the C6's `hal_mac_rx_is_dscr_reload`
        // is. Value chosen so the poll exits: bit 31 always reads 0, so a
        // sample the ROM asks for is instantly taken.
        (
            0x01c,
            1 << 31,
            0,
            "r_rwip_time_get sample strobe self-clears",
        ),
        // The blob's `r_rwip_driver_init` (`0x422732fd` in the probe image,
        // parked there at a 3 s deadline): it read-modify-writes
        // `0x6003_1000` to set **bit 31** and loops `l32i; bltz` while it
        // still reads 1 — the link layer's soft reset, which hardware
        // clears when the reset is done. `r_lld_core_init` read-modify-
        // writes the same word's low bits and keeps bit 31 clear. A
        // self-clearing strobe; value chosen so the poll exits.
        (
            0x000,
            1 << 31,
            0,
            "r_rwip_driver_init soft reset self-clears",
        ),
        // Not a spin: an **assertion**. The blob's `r_lld_core_init`
        // (`0x42269a5f` in the probe image) reads `0x6003_1004` once and
        // compares it with `0x0900_1B00` (`beq a4, a11`); on a mismatch it
        // calls the controller's assert hook with line `0x144`, which the
        // ROM's `r_rwip_assert_hack` routes to esp-radio's
        // `assert_wrapper` — a panic. Found by `--break-at
        // r_rwip_assert_hack` plus a `BLE_RW` trace: the last access before
        // the break is this read answering 0. The word is the link-layer
        // IP's version register — read-only, read once, compared with a
        // constant — so the whole word answers the constant the blob
        // checks for. Modeled: no silicon read of it exists yet; the M1
        // transcript's boot passing the same assertion on FS3 is the check.
        (
            0x004,
            0xffff_ffff,
            0x0900_1b00,
            "r_lld_core_init IP version == 0x0900_1B00",
        ),
    ],
};

/// The Wi‑Fi MAC (`hal_init`, `mac_txrx_init`, `mac_rxbuf_init`,
/// `hal_crypto_*`, `mac_tx_set_plcp1`), up to the PAC's `RNG` data word at
/// `0x6003_4F6C` — which sits inside this window on silicon and is its own
/// block here ([`super::rng`]).
pub const WIFI_MAC: RadioWindow = RadioWindow {
    name: "WIFI_MAC",
    base: 0x6003_3000,
    len: 0x1f6c,
    names: &[(0x0000, 0x1000, "mac"), (0x1000, 0x1f6c, "mac_hi")],
    overrides: &[
        // The blob's `hal_init` (`0x422a2dcc` in the probe image, found
        // parked there at a 3 s deadline), the MAC proper after the PHY
        // calibration: it sets bit 1 of `0x6003_3D14` (an enable strobe) and
        // loops `l32i; bbci a3, 0` — until **bit 0** reads 1, ready — before
        // `mac_txrx_init`. The C6's `hal_init` does the same at its
        // `WIFI_MAC+0x4ddc`. Blob spins here; value chosen so the poll exits.
        (0x0d14, 1 << 0, 1 << 0, "hal_init ready flag"),
    ],
};

/// Above the PAC's `RNG` aperture: the MAC's TSF and power timers
/// (`hal_mac_tsf_reset`, `tsf_hal_set_timer_*`, `hal_get_tsf_time`,
/// `hal_set_sta_tbtt`) — the C6's `WIFI_PWR` routines, here at
/// `0x6003_5000` — to the end of that page.
///
/// ⚠️ The census's `0x6003_5000..0x6003_5034` (and anything else below
/// `0x6003_508C`) lands **inside [`super::rng`]'s aperture**: the PAC puts
/// `RNG` at the odd base `0x6003_4F6C` so that its one register, `data`,
/// sits at `+0x110`, and the block's register file remembers every other
/// offset in it, so those accesses are answered there, untouched by this
/// module and with no `TOUCH` note.
pub const WIFI_PWR: RadioWindow = RadioWindow {
    name: "WIFI_PWR",
    base: 0x6003_508C,
    len: 0x6003_6000 - 0x6003_508C,
    names: &[(0x000, 0xf74, "pwr")],
    overrides: &[],
};

/// The Bluetooth controller's wake/sleep registers (`btdm_wakeup_request`,
/// `r_rwip_driver_init`, `rw_schedule`, `r_rf_sleep_hack`).
pub const BTDM_WAKE: RadioWindow = RadioWindow {
    name: "BTDM_WAKE",
    base: 0x6004_2000,
    len: 0x1000,
    names: &[(0x000, 0x1000, "btdm_wake")],
    overrides: &[],
};

/// Every radio window, in registration order (appended to the boot set —
/// see [`super`]'s "Registration order is a contract").
pub const WINDOWS: &[RadioWindow] = &[
    PBUS, I2C_MST_HI, BT, RF_CTRL, NRX_HI, BB_HI, BLE_RW, WIFI_MAC, WIFI_PWR, BTDM_WAKE,
];

/// One radio window on the bus.
#[derive(Debug)]
pub struct RadioStub {
    window: RadioWindow,
    regs: RegFile,
    touched: BTreeSet<u32>,
}

impl RadioStub {
    pub fn new(window: RadioWindow) -> Self {
        let mut regs = RegFile::new(window.name, window.len);
        for &(off, mask, value, _) in window.overrides {
            regs = regs.with_read_override(off, mask, value);
        }
        Self {
            window,
            regs,
            touched: BTreeSet::new(),
        }
    }

    /// The window this block answers.
    pub fn window(&self) -> RadioWindow {
        self.window
    }

    /// Distinct word offsets the guest has touched so far.
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
                self.window.name,
                self.touched.len()
            );
            cx.trace.note(&line);
        }
    }
}

impl Peripheral for RadioStub {
    fn name(&self) -> &'static str {
        self.window.name
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        self.note_touch(off, "R", cx);
        lane_of(self.regs.effective(off & !3), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        self.note_touch(off, "W", cx);
        let word = off & !3;
        let merged = merge_lane(self.regs.stored(word), off, width, value);
        self.regs.poke(word, merged);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        self.window
            .names
            .iter()
            .find(|(start, end, _)| off >= *start && off < *end)
            .map(|(_, _, name)| *name)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.window.len as usize + 4 * self.touched.len() + 4);
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
    use crate::memmap::periph as base;

    #[test]
    fn the_windows_do_not_overlap_each_other_or_a_named_block() {
        let named: &[(u32, u32)] = &[
            (base::FE, crate::periph::accept::RF_LEN),
            (
                base::I2C_ANA_MST,
                crate::periph::i2c_ana_mst::I2C_ANA_MST_LEN,
            ),
            (base::NRX, crate::periph::accept::RF_LEN),
            (base::BB, crate::periph::accept::RF_LEN),
            (base::RNG, crate::periph::rng::LEN),
            (base::USB_DEVICE, crate::periph::usb_sj::USB_DEVICE_LEN),
        ];
        for (i, w) in WINDOWS.iter().enumerate() {
            for v in &WINDOWS[i + 1..] {
                assert!(
                    w.base + w.len <= v.base || v.base + v.len <= w.base,
                    "{} overlaps {}",
                    w.name,
                    v.name
                );
            }
            for &(b, n) in named {
                assert!(
                    w.base + w.len <= b || b + n <= w.base,
                    "{} overlaps the block at {b:#010x}",
                    w.name
                );
            }
        }
    }

    #[test]
    fn the_windows_meet_the_named_blocks_they_extend() {
        assert_eq!(PBUS.base, base::FE + crate::periph::accept::RF_LEN);
        assert_eq!(
            I2C_MST_HI.base,
            base::I2C_ANA_MST + crate::periph::i2c_ana_mst::I2C_ANA_MST_LEN
        );
        assert_eq!(NRX_HI.base, base::NRX + crate::periph::accept::RF_LEN);
        assert_eq!(BB_HI.base, base::BB + crate::periph::accept::RF_LEN);
        assert_eq!(WIFI_MAC.base + WIFI_MAC.len, base::RNG);
        assert_eq!(WIFI_PWR.base, base::RNG + crate::periph::rng::LEN);
    }

    #[test]
    fn a_window_remembers_what_it_is_written_and_touches_once() {
        let mut stub = RadioStub::new(WIFI_MAC);
        let mut sb = lp_emu_esp_common::Sandbox::new();
        sb.write(&mut stub, 0x84, 0xdead_beef);
        assert_eq!(sb.read(&mut stub, 0x84), 0xdead_beef);
        assert_eq!(sb.read(&mut stub, 0x88), 0);
        assert_eq!(stub.touched().len(), 2);
        let saved = stub.save_state();
        let mut other = RadioStub::new(WIFI_MAC);
        other.load_state(&saved);
        assert_eq!(sb.read(&mut other, 0x84), 0xdead_beef);
    }
}
