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
//! # Interrupts
//!
//! Interrupt sources 0–3 (`WIFI_MAC`, `WIFI_MAC_NMI`, `WIFI_PWR`, `WIFI_BB`)
//! were never raised until M4 P2. M4 P1 established **which** of them the
//! blob claims and **which registers its ISR reads and clears** — see
//! [`RADIO_INT_SOURCES`], [`MAC_INT_EVENT_OFFSET`] and
//! [`PWR_INT_EVENT_OFFSET`] — but raising one never produced a TX
//! completion, so **no TX completion is originated here** and
//! `docs/debt/emu-c6-radio-tx-never-completes.md` still carries that
//! question.
//!
//! P2 raises **source 0 on a delivery into the RX ring**
//! ([`WifiStub::raise_rx_interrupt`]), and honours the guest's
//! write-one-to-clear at [`MAC_INT_CLEAR_OFFSET`] so the line drops when the
//! ISR has drained it. What that raise is worth is in the README's
//! "what the guest actually checked" table: the raise is **ours**, the source
//! and the clear are the guest's own.

use std::collections::BTreeSet;

use lp_emu_esp_common::RegGrade;
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
    // Baseband / RF calibration / power: `rfcal_*`, `hal_init_imrsp_power` —
    // and, P6 found, the MAC's `hal_mac_*` registers too: the RX DMA base
    // at `+0x4084`, the ready flag at `+0x4ddc`. One coarse name for both,
    // because nothing documents where one ends and the other begins.
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
    (
        0x0814,
        0x7 << 14,
        0x7 << 14,
        "ram_pwdet_tone_start state field == 7",
    ),
    // `SPIN WIFI_MAC+0x0cc mac = 0x25824e50 x10000` from
    // `ram_set_chan_freq_sw_start` (`0x4221a72a`, 30.3 ms): after the ROM's
    // `freq_chan_en_sw` and a 10 µs delay it loops `lw a5,0xcc(a4); andi
    // a5,a5,256; beqz a5` — until **bit 8** reads 1, the channel/frequency
    // lock flag (`freq_reg_init` wrote the register with bit 8 clear). Blob
    // spins here; esp-emu evidently satisfies it; value chosen so the poll
    // exits.
    (
        0x00cc,
        1 << 8,
        1 << 8,
        "ram_set_chan_freq_sw_start lock flag",
    ),
    // `SPIN WIFI_MAC+0x4a0 mac = 0x00000000 x10000` from the ROM's
    // `rom_iq_est_enable` (`0x40005dba`, 30.3 ms): it sets `+0x470` bit 26,
    // programs `+0x474` (bit 20, a length field, then bits 0 and 1 as
    // strobes) and loops `lw a5,0x4a0(a4); slli a3,a5,15; bgez a3` — until
    // **bit 16** reads 1, the IQ-estimate done flag. Blob (via the ROM)
    // spins here; esp-emu evidently satisfies it; value chosen so the poll
    // exits.
    (0x04a0, 1 << 16, 1 << 16, "rom_iq_est_enable done flag"),
    // `SPIN WIFI_MAC+0x4ddc bb = 0x00000002 x10000` from `hal_init`
    // (`0x4222b364`, 39.4 ms) — the MAC proper, after the PHY calibration:
    // it sets bit 1 of `+0x4ddc` (an enable strobe) and loops `lw a4,0(a4);
    // andi a4,a4,1; beqz a4` — until **bit 0** reads 1, ready — before
    // `mac_txrx_init`. Blob spins here; esp-emu evidently satisfies it;
    // value chosen so the poll exits.
    (0x4ddc, 1 << 0, 1 << 0, "hal_init ready flag"),
    // `SPIN WIFI_MAC+0x4080 bb = 0x88000001 x10000` from
    // `hal_mac_rx_is_dscr_reload+0x4` (`0x42079682`) — reached the first
    // time this machine ever delivered a frame into the RX ring (M4 P2, and
    // never before, because nothing here had ever received). The RX path
    // sets bit 0 through `hal_mac_rx_set_dscr_reload+0xe` and then loops
    // `lw` on the same word until it reads back **0**: the shape of a
    // self-clearing strobe. Bits 31 and 27 of this register are the RX
    // enables `ic_enable_rx` and
    // `hal_mac_set_rxbuf_reload_use_hw_beacon_enable` had already set, and
    // they are remembered as written. Value chosen so the poll exits — bit 0
    // always reads 0, so a reload the blob asks for is instantly done.
    (0x4080, 1 << 0, 0, "hal_mac_rx_is_dscr_reload strobe self-clears"),
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

/// The **TX slot's register group**, `WIFI_MAC + 0x4d68`.
///
/// Not a guess: `--break-at mac_tx_set_plcp0` on the `test_espnow` image
/// stops with `a4=0x600a4d68`, so the blob is handed this address as the
/// slot's base and writes PLCP0 at base + 4. The whole group the first
/// `send_channel` touches is `+0x4d60`, `+0x4d64`, `+0x4d68`, `+0x4d6c`;
/// M4 P0's `m4/discovery-air.md` has the ledger, the writer of each and the
/// confidence on each reading.
pub const TX_SLOT_BASE_OFFSET: u32 = 0x4d68;

/// `WIFI_MAC + 0x4d6c` — the TX slot's **PLCP0** word, the one register in
/// the window that carries a DRAM pointer on the TX path.
///
/// Written twice per frame on the observed path, and the two writes are the
/// handoff:
///
/// ```text
/// cyc=165826331 pc=0x408051b4 W4 WIFI_MAC+0x4d6c = 0x0061de88  mac_tx_set_plcp0+0x6a
/// cyc=165826944 pc=0x40806040 W4 WIFI_MAC+0x4d6c = 0xc061de88  hal_mac_txq_enable+0xe
/// ```
///
/// `mac_tx_set_plcp0` programs the pointer; `hal_mac_txq_enable` re-writes
/// the same word with bits 31 and 30 set, and nothing in the window is
/// touched afterwards. So the second write is where the frame is final —
/// [`TX_GO_MASK`].
pub const TX_PLCP0_OFFSET: u32 = 0x4d6c;

/// The bits `hal_mac_txq_enable` adds to [`TX_PLCP0_OFFSET`] and nothing
/// clears. *Modeled as a start strobe*: they are set on the last write of
/// the sequence and the frame's bytes are complete under them. What they
/// mean to the silicon is not known — see the discovery document's
/// "what stays unknown".
pub const TX_GO_MASK: u32 = 0b11 << 30;

/// How much of a PLCP0 word is the descriptor's address.
///
/// The evidence is arithmetic, not documentation: PLCP0 read `0x0061de88`,
/// and `--break-at lmacTxFrame` on the same run stopped with
/// `a0=0x4081de4c`. `0x4080_0000 | (0x0061de88 & 0xf_ffff)` is `0x4081de88`
/// = `a0 + 0x3c`, and the twelve bytes there are `{0xc0110060, 0x4081defc,
/// 0x00000000}` — a word, a DRAM pointer and a null link. The remaining
/// bits (`0x006` in `0x0061de88`) are **not** part of the address and this
/// machine does not claim to know what they are.
pub const TX_DESC_PTR_MASK: u32 = 0x000f_ffff;

/// The DRAM base the masked pointer is completed with (HP-SRAM's data view).
pub const TX_DESC_PTR_BASE: u32 = 0x4080_0000;

/// One frame the blob handed the MAC: the PLCP0 write that armed it.
///
/// Deliberately *only* what the register said. Everything else the radio TX
/// log prints — the descriptor, the buffer, the length — the machine reads
/// out of guest RAM afterwards, because a peripheral cannot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxHandoff {
    /// Guest cycle of the arming write.
    pub at: u64,
    /// The PC that performed it (`hal_mac_txq_enable+0xe` on the observed
    /// path).
    pub pc: u32,
    /// The whole PLCP0 word, strobe bits included.
    pub plcp0: u32,
}

impl TxHandoff {
    /// The descriptor address this word points at. See [`TX_DESC_PTR_MASK`].
    pub fn descriptor(&self) -> u32 {
        TX_DESC_PTR_BASE | (self.plcp0 & TX_DESC_PTR_MASK)
    }
}

/// The interrupt **source** the blob's radio path claims, and the event and
/// clear registers its ISR reads and writes.
///
/// All four are **observed**, on the `test_espnow` image, and none of them is
/// acted on by this block today: M4 P1 recorded them and P2 owns whatever
/// uses them. They are written down here rather than in a planning file
/// because the next person to open this module is the one who needs them.
///
/// **Who claims the source.** esp-radio's
/// `os_adapter_chip_specific::set_isr` routes sources 0 (`WIFI_MAC`) and 2
/// (`WIFI_PWR`) to CPU interrupt 16, 5.6 ms into the boot:
///
/// ```text
/// cyc=5619236 pc=0x4202c9d8 W4 INTERRUPT_CORE0+0x000 core_0_intr_map0 = 0x00000010
/// cyc=5619237 pc=0x4202c9da W4 INTERRUPT_CORE0+0x008 core_0_intr_map2 = 0x00000010
/// ```
///
/// CPU interrupt 16 was already enabled and prioritised by esp-hal's
/// `_setup_interrupts` at boot (`W4 PLIC_MX+0x050 mxint16_pri = 0x00000001`,
/// `W4 PLIC_MX+0x000 mxint_enable = 0x00010000`), and the esp-rtos tick
/// (source 51) rides the same CPU interrupt — so the line is live and a
/// raised source 0 *is* taken. Sources 1 (`WIFI_MAC_NMI`) and 3 (`WIFI_BB`)
/// are left at the `_setup_interrupts` default of 31 and are never claimed.
pub const RADIO_INT_SOURCES: [u16; 2] =
    [crate::regs::source::WIFI_MAC, crate::regs::source::WIFI_PWR];

/// `WIFI_MAC + 0x4c48` — the MAC's interrupt **event** word, and
/// `+0x4c4c` its **write-one-to-clear** twin.
///
/// Observed by raising source 0 after a TX go strobe: the blob's ISR enters,
/// reads the event word through `hal_mac_interrupt_get_event` (the symbol
/// table names `0x4080d22e`, and the read is at `+0x4`), and writes the
/// value it just read back into `+0x4c4c`:
///
/// ```text
/// cyc=165843231 pc=0x4080d232 R4 WIFI_MAC+0x4c48 bb = 0x00001000
/// cyc=165843260 pc=0x4080d23c W4 WIFI_MAC+0x4c4c bb = 0x00001000
/// ```
///
/// `mac_txrx_init` writes `+0x4c4c = 0xffffffff` at 34.9 ms, which is what a
/// clear register is written with at init, and is the second reading that
/// says `+0x4c4c` is the clear rather than a second status word.
///
/// The ISR is a **loop**: it re-reads `+0x4c48` after clearing and exits when
/// it reads zero. A block that raises the line without honouring the clear
/// re-enters the ISR forever (measured: every 464 cycles).
pub const MAC_INT_EVENT_OFFSET: u32 = 0x4c48;
pub const MAC_INT_CLEAR_OFFSET: u32 = 0x4c4c;

/// `WIFI_PWR + 0x37b0` / `+0x37b4` — the same pair on the PWR block, read
/// and written by the *same* ISR entry through `hal_pwr_interrupt_get_event`
/// (`0x4080d2d6`), with `+0x37ac` read beside it:
///
/// ```text
/// cyc=165843243 pc=0x4080d2d6 R4 WIFI_PWR+0x37b0 pwr = 0x00000000
/// cyc=165843250 pc=0x40008e88 R4 WIFI_PWR+0x37ac pwr = 0x00000000
/// cyc=165843274 pc=0x4080d2e0 W4 WIFI_PWR+0x37b4 pwr = 0x00000000
/// ```
///
/// So one ISR entry drains two event words, and the loop's exit condition
/// covers both.
pub const PWR_INT_EVENT_OFFSET: u32 = 0x37b0;
pub const PWR_INT_CLEAR_OFFSET: u32 = 0x37b4;

/// The register the blob programs with its RX DMA descriptor base, reported
/// as the `WIFI RX config` trace line. Found in P6's G6-5 run: the only
/// write of a DRAM address into the window is
/// `cyc=6269893 pc=0x4222acdc W4 WIFI_MAC+0x4084 = 0x4081557c`, from the
/// MAC init right before `mac_txrx_init` — a pointer into the app's `.bss`,
/// the RX descriptor ring. (esp-emu's trace called it `dma_base=0x15DBC`
/// without saying where it read it; that is the same kind of value, an
/// offset from `0x4080_0000`, on the flash-backed image's layout.)
pub const RX_DMA_BASE_OFFSET: Option<u32> = Some(0x4084);

/// The event bit [`WifiStub::raise_rx_interrupt`] puts in
/// [`MAC_INT_EVENT_OFFSET`] before it raises source 0: **bit 14**.
///
/// # Observed, not chosen — and it disagrees with M4 P1
///
/// P1 tried all 32 bits of this word, one at a time, plus all-ones, and
/// reported an identical instruction count for every candidate: "the event
/// word's value does not reach a dispatch at all". **That holds only while
/// the RX ring is empty.** P1 had nothing to receive, so the blob's RX path
/// looked at a ring with no filled descriptor and left by the same route
/// whatever it had been told.
///
/// M4 P2 ran the same sweep with a frame already written into a descriptor,
/// against the *RX* oracle, and the candidates separate at once — eight
/// distinct instruction counts across 35 of them. **Bit 14 is the only one
/// that reaches the RX path**: the guest goes on to read the ring's base
/// (`+0x4084`), [`RX_LAST_DSCR_OFFSET`] and its neighbours through
/// `wdev_record_rx_linked_list` and `hal_mac_rx_get_last_dscr`, and to work
/// the descriptor-reload strobe at `+0x4080`. No other bit produces a single
/// access to any of those registers.
///
/// Corroboration, from the guest's own boot: `hal_init` writes an interrupt
/// enable mask of `0x19a879e0` to `WIFI_MAC+0x4c40` (`cyc=5618718`), and bit
/// 14 is one of the fourteen bits set in it.
///
/// So this constant rests on **evidence**, which is more than P1 could
/// manage — and the register is still `modeled`, because what the silicon
/// puts in that word has never been watched.
pub const RX_INT_EVENT_BITS: u32 = 1 << 14;

/// `WIFI_MAC + 0x408c` — the descriptor the hardware last filled, read by
/// `hal_mac_rx_get_last_dscr+0x4` (`0x42079664`) and again by
/// `wdev_record_rx_linked_list+0x44` on every RX interrupt.
///
/// Observed the first time a frame was delivered into the ring: with bit 14
/// raised, the guest reads `+0x4084` (the base it programmed itself), then
/// this register, then `+0x4088`, `+0x4094` and `+0x4090` beside it.
/// Answering 0 — "the hardware has filled nothing" — sends it into the
/// descriptor-reload path and it never looks at the ring; answering the
/// address of the descriptor just filled is what carries the frame up to
/// the app.
///
/// *Modeled.* That this register names the **last filled descriptor** is an
/// inference from its reader's symbol name plus what the guest does with
/// each answer; nothing on silicon has been watched writing it.
pub const RX_LAST_DSCR_OFFSET: u32 = 0x408c;

/// `WIFI_MAC + 0x4088` — the descriptor the hardware will fill **next**,
/// read by `hal_mac_rx_read_rxdscrnext+0x4` (`0x4080ab18`) and again by
/// `wdev_record_rx_linked_list+0x4c`.
///
/// **This is M4 P0's U6**, answered: "whether `RX_DMA_BASE_OFFSET` is the
/// only pointer register on the RX side, or whether a second register
/// carries a read/write cursor into the ring". It is not the only one, and
/// this is the cursor. P0 could not see it because nothing had ever
/// received; it is read only on the RX path.
///
/// Answering 0 is not merely useless, it is **visibly wrong**: the blob
/// takes the value as a pointer and dereferences it, and the emulator's own
/// strict-bus counter catches the result —
/// `R4 UNMAPPED+0x1143c [unmapped]` from `0x40809fbc`. A cursor that names
/// the descriptor after the one just filled is what stops that.
///
/// *Modeled*, for the same reason as [`RX_LAST_DSCR_OFFSET`]: the reading
/// is its reader's name plus the guest's behaviour on each answer.
pub const RX_DSCR_NEXT_OFFSET: u32 = 0x4088;

/// The registers the air writes or answers, each with its grade and the
/// evidence for it: `(offset, grade, what decided it)`.
///
/// Every one is **`modeled`**. `Measured` would mean a committed silicon
/// transcript agrees with the model, and none of these has one — the whole
/// window's meanings are inferences from a writer's symbol name and an
/// observed value. The table is here so that a reader can see *which*
/// registers the air is now part of, and on what evidence, the same way
/// [`OVERRIDES`] shows which polls the boot demanded.
///
/// [`Peripheral::reg_grade`] grades the **whole window** `modeled` rather
/// than only these four; see its docs for the blast radius that publishing a
/// table has.
pub const AIR_GRADES: &[(u32, RegGrade, &str)] = &[
    (
        0x4084,
        RegGrade::Modeled,
        "RX_DMA_BASE_OFFSET. The blob writes its RX ring's base here \
         (`W4 WIFI_MAC+0x4084 = 0x4081557c` from the MAC init, and \
         `0x40811434` on the test_espnow image); a delivery reads it back \
         and walks the chain. Modeled: the ring's shape is read out of \
         guest RAM (M4 P0 §2), the register's meaning is inferred from its \
         writer's name",
    ),
    (
        0x4c48,
        RegGrade::Modeled,
        "MAC_INT_EVENT_OFFSET. The ISR reads it \
         (`hal_mac_interrupt_get_event`) on every raise of source 0; a \
         delivery ORs RX_INT_EVENT_BITS into it before raising. Modeled, \
         and the *value* is undetermined: no bit pattern changes what the \
         guest does (M4 P1 A.2, and P2's RX sweep)",
    ),
    (
        0x4c4c,
        RegGrade::Modeled,
        "MAC_INT_CLEAR_OFFSET. Write-one-to-clear: the ISR writes back the \
         bits it read, and `mac_txrx_init` writes 0xffffffff at init. This \
         block clears those bits from +0x4c48 and drops the source when the \
         word reaches zero. Modeled: the w1c behaviour is read off the \
         guest's own writes, not a document",
    ),
    (
        TX_PLCP0_OFFSET,
        RegGrade::Modeled,
        "The TX slot's PLCP0. Answered as an ordinary remembered register; \
         the strobed write is what hands a frame to the air (M4 P0 §1-§2). \
         Modeled: the pointer reading is arithmetic against a break-at, and \
         the strobe bits' meaning is not known",
    ),
];

/// One received frame's `rx_ctrl` header, in **our own words**.
///
/// # Provenance
///
/// Per `docs/adr/2026-07-29-license-provenance-discipline.md`. The field
/// names, widths and bit offsets below are **derived from the public bit
/// layout of `wifi_pkt_rx_ctrl_t`** (an alias of `esp_wifi_rxctrl_t`) in the
/// **`esp-wifi-sys-esp32c6` crate, version 0.2.0, Apache-2.0**, whose
/// bindings are generated from esp-idf's `esp_wifi_types.h`. Nothing is
/// **copied**: the crate is not vendored, no source from it is in this
/// repository, and the code below is written from the layout, not from the
/// bindings' text. Nothing here was disassembled out of a blob binary.
///
/// # What is *chosen* here, and it is a lot
///
/// M4 P0 §4 is explicit that the RX ring on this machine was **posted but
/// never filled**, so nothing has ever seen the header the silicon writes.
/// Two things are therefore assumptions this module is making, not facts it
/// inherited:
///
/// 1. **That the header sits at `buf[0]`,** with the 802.11 frame
///    immediately after it. That is the shape the promiscuous-mode buffer
///    has (`rx_ctrl` then payload) and it is the obvious reading; it is not
///    an observation.
/// 2. **That it is [`RX_CTRL_LEN`] bytes long**, which is what the layout
///    above sums to.
///
/// Every field is either **derived from the frame** (`sig_len`, `is_group`),
/// **taken from the receiving machine's own clock** (`timestamp`), or a
/// **stated constant** ([`RX_RSSI_DBM`], [`RX_NOISE_FLOOR_DBM`],
/// [`RX_CTRL_RATE`], [`RX_CTRL_CHANNEL`]) — never a value pretending to be a
/// measurement. The air models no PHY, so there is no RSSI to compute and no
/// rate to recover.
pub mod rx_ctrl {
    /// The header's length in bytes: the layout's own size.
    ///
    /// The struct is `#[repr(C, packed)]`, so this is the sum of its parts
    /// with no padding: a four-byte bit field, `he_siga1`, a one-byte bit
    /// field, `he_siga2`, and an 81-byte bit field.
    pub const LEN: usize = 4 + 4 + 1 + 2 + 81;

    /// `rssi`, byte 0, signed 8-bit. **A constant, not a measurement.** The
    /// air models no PHY and no range (RD10), so there is nothing to derive
    /// one from; −40 dBm is "a peer on the same bench", and every frame this
    /// machine delivers reports it.
    pub const RSSI_DBM: i8 = -40;

    /// `noise_floor`, byte 20, signed 8-bit. A constant, for the same
    /// reason as [`RSSI_DBM`].
    pub const NOISE_FLOOR_DBM: i8 = -96;

    /// `rate`, byte 1 bits 0..5. **Fixed at 0** — 802.11b 1 Mbit/s with a
    /// long preamble, the basic rate the air's latency constant was chosen
    /// from (`lockstep::DEFAULT_LATENCY_US`).
    ///
    /// RD10 says `rate` "comes from the frame"; it cannot. An 802.11 frame
    /// does not carry the rate it was sent at — the PLCP header does, and
    /// the PLCP registers the blob wrote are M4 P0's U5, undetermined. So
    /// this is a **stated fixed value** consistent with the latency, and it
    /// is named as one here and in the README rather than dressed up as a
    /// derivation.
    pub const RATE: u8 = 0;

    /// `channel`, byte 21 bits 0..4. **Fixed.** The air has no channel: one
    /// medium, everybody on it hears everybody (RD10). 11 is the channel the
    /// `test_espnow` image asks its driver for
    /// (`espnow_radio_driver::DEFAULT_ESPNOW_CHANNEL`), so a frame that
    /// claimed anything else would be claiming a channel model this
    /// emulator does not have.
    pub const CHANNEL: u8 = 11;

    /// A frame, headed. `bytes` is the 802.11 frame as it travelled;
    /// `micros` is the receiving machine's own microsecond clock.
    ///
    /// The returned buffer is the header followed by the frame followed by
    /// **four zero bytes where the FCS would be**. The air carries no FCS
    /// (the sender's buffer did not hold one either — M4 P0 §2) and none is
    /// computed here; the four bytes exist so that a reader who trusts
    /// `sig_len` reads zeros rather than whatever the heap left behind.
    pub fn frame_with_header(bytes: &[u8], micros: u32) -> Vec<u8> {
        let mut out = vec![0u8; LEN];
        // `rssi`: byte 0, signed.
        out[0] = RSSI_DBM as u8;
        // `rate`: byte 1, bits 0..5.
        out[1] = RATE & 0x1f;
        // `rxmatch0`: bit 28, which is bit 4 of byte 3. **The one field in
        // this header the guest demands.** Sweeping every value of every
        // byte against `--break-at ppRxPkt` (M4 P2) says exactly this: the
        // frame reaches `ppRxPkt` for all 128 values of byte 3 that have bit
        // 4 set and for none of the 128 that do not, and no other byte of
        // the header changes whether it gets there. `rxmatch0` reads as
        // "receive filter 0 matched", and this air delivers every frame to
        // every participant, so filter 0 matched.
        out[3] |= 1 << 4;
        // `is_group`: bit 7 of byte 11. **Derived from the frame**: 802.11
        // calls addr1 a group address when the low bit of its first byte is
        // set, and ESP-NOW's broadcast frames are exactly that. addr1
        // starts at frame byte 4, after the frame control and duration.
        let is_group = bytes.get(4).is_some_and(|b| b & 1 == 1);
        if is_group {
            out[11] |= 1 << 7;
        }
        // `timestamp`: bytes 12..16, byte-aligned in the layout. **The
        // receiving machine's own microsecond counter** — the same one the
        // TX path takes its timestamps from (`WIFI_PWR+0x3700`, M4 P0 §1),
        // so a guest that compares them is comparing one clock.
        out[12..16].copy_from_slice(&micros.to_le_bytes());
        // `noise_floor`: byte 20, signed.
        out[20] = NOISE_FLOOR_DBM as u8;
        // `channel` in bits 0..4 of byte 21, `second` (the secondary
        // channel, fixed at 0 = none) in bits 4..8.
        out[21] = CHANNEL & 0x0f;
        // `sig_len`: 14 bits at byte 84. **Derived from the frame**: the
        // length including the four-byte FCS, which is the same convention
        // the TX side's length word used (60 for a 56-byte frame, M4 P0 §2).
        let sig_len = (bytes.len() as u32 + 4) & 0x3fff;
        out[84] = sig_len as u8;
        out[85] = (sig_len >> 8) as u8;
        out.extend_from_slice(bytes);
        out.extend_from_slice(&[0, 0, 0, 0]);
        out
    }
}

/// The radio window (and, as a second instance, the `WIFI_PWR` gap).
#[derive(Debug)]
pub struct WifiStub {
    name: &'static str,
    names: &'static [(u32, u32, &'static str)],
    regs: RegFile,
    touched: BTreeSet<u32>,
    /// Frames armed since the machine last drained them. Only `WIFI_MAC`
    /// ever fills it; the machine reads the bytes and empties it at the
    /// slice boundary that follows.
    tx_handoffs: Vec<TxHandoff>,
    /// Whether anything is listening ([`Self::arm_tx_log`] or
    /// [`Self::arm_air`]).
    ///
    /// The recording ends the slice so the machine can read guest RAM before
    /// the buffer is reused, and ending a slice early is a real difference —
    /// it moves where a pending interrupt is taken. So a machine with no
    /// `--tx-log` and no air does not arm it, and its runs are byte-for-byte
    /// the runs it had before this block could record anything.
    tx_capture_armed: bool,
    /// What [`Self::raise_rx_interrupt`] puts in the event word. Defaults to
    /// [`RX_INT_EVENT_BITS`]; settable so the sweep that failed to
    /// distinguish one value from another can be re-run without a rebuild.
    rx_int_event_bits: u32,
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
            tx_handoffs: Vec::new(),
            tx_capture_armed: false,
            rx_int_event_bits: RX_INT_EVENT_BITS,
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
            tx_handoffs: Vec::new(),
            tx_capture_armed: false,
            rx_int_event_bits: RX_INT_EVENT_BITS,
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
            tx_handoffs: Vec::new(),
            tx_capture_armed: false,
            rx_int_event_bits: RX_INT_EVENT_BITS,
        }
    }

    /// Distinct offsets the guest has touched so far.
    pub fn touched(&self) -> &BTreeSet<u32> {
        &self.touched
    }

    /// Start recording TX handoffs. The machine calls this when a
    /// `--tx-log` sink is set, and only then — see `tx_capture_armed`.
    pub fn arm_tx_log(&mut self) {
        self.tx_capture_armed = true;
    }

    /// The same recording, armed because this machine is in an
    /// [`lp_emu_esp_common::air::Air`] rather than because a log is being
    /// written. One flag, two reasons, and the same off switch: a machine
    /// with neither is the machine that came before either existed.
    pub fn arm_air(&mut self) {
        self.tx_capture_armed = true;
    }

    /// Take the frames armed since the last call, for the machine's radio TX
    /// log. Empty on every block but `WIFI_MAC`.
    pub fn take_tx_handoffs(&mut self) -> Vec<TxHandoff> {
        std::mem::take(&mut self.tx_handoffs)
    }

    /// The RX descriptor ring's base, as the **guest's own blob** programmed
    /// it into [`RX_DMA_BASE_OFFSET`], or `None` before it has.
    ///
    /// Read out of the register file rather than off the bus on purpose: a
    /// bus read from the machine would leave a `TOUCH` note and enter the
    /// touched set, and a delivery must not change what a run's trace says
    /// the *guest* reached.
    pub fn rx_dma_base(&self) -> Option<u32> {
        let off = RX_DMA_BASE_OFFSET?;
        if self.name != "WIFI_MAC" {
            return None;
        }
        match self.regs.stored(off) {
            0 => None,
            base => Some(base),
        }
    }

    /// Put [`RX_INT_EVENT_BITS`] (or whatever [`Self::set_rx_event_bits`]
    /// last set) into the MAC's event word.
    ///
    /// The **caller raises the line** — a peripheral only reaches
    /// `IrqLines` inside an access, and a delivery happens at a slice
    /// boundary, where the machine holds them. So this sets the word the
    /// ISR will read and the machine sets the level; the two belong
    /// together and [`Esp32C6Machine::offer_air_frame`] is the only caller.
    ///
    /// [`Esp32C6Machine::offer_air_frame`]: crate::machine::Esp32C6Machine::offer_air_frame
    pub fn raise_rx_interrupt(&mut self, descriptor: u32, next: u32) {
        if self.name != "WIFI_MAC" {
            return;
        }
        self.regs.poke(RX_LAST_DSCR_OFFSET, descriptor);
        self.regs.poke(RX_DSCR_NEXT_OFFSET, next);
        let pending = self.regs.stored(MAC_INT_EVENT_OFFSET) | self.rx_int_event_bits;
        self.regs.poke(MAC_INT_EVENT_OFFSET, pending);
    }

    /// Whether anything is left in the MAC's event word — the level the
    /// machine should be holding on source 0.
    pub fn radio_interrupt_pending(&self) -> bool {
        self.name == "WIFI_MAC" && self.regs.stored(MAC_INT_EVENT_OFFSET) != 0
    }

    /// The value [`Self::raise_rx_interrupt`] writes. See
    /// [`RX_INT_EVENT_BITS`] for why this is settable: the sweep that could
    /// not distinguish one value from another is worth re-running, and a
    /// rebuild per candidate is not.
    pub fn set_rx_event_bits(&mut self, bits: u32) {
        self.rx_int_event_bits = bits;
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
        // The ISR's write-one-to-clear. M4 P1 observed the guest writing back
        // exactly the bits it had just read from `+0x4c48`, and observed that
        // it **loops** — re-reading the event word after each clear and
        // exiting when it reads zero. So a raise this block does not retract
        // re-enters the ISR every 464 cycles forever. This is where it is
        // retracted, and the level drops with the last bit.
        if self.name == "WIFI_MAC" && word == MAC_INT_CLEAR_OFFSET {
            let left = self.regs.stored(MAC_INT_EVENT_OFFSET) & !merged;
            self.regs.poke(MAC_INT_EVENT_OFFSET, left);
            if left == 0 {
                cx.irq.set_level(crate::regs::source::WIFI_MAC, false);
            }
        }
        if self.name == "WIFI_MAC" && Some(word) == RX_DMA_BASE_OFFSET && cx.trace.is_enabled() {
            let line = format!(
                "cyc={} pc=0x{:08x} WIFI RX config: dma_base=0x{merged:08x}",
                cx.now, cx.pc
            );
            cx.trace.note(&line);
        }
        // The TX handoff. Recorded, never answered: the block still does not
        // raise an interrupt and the guest is not told anything it would not
        // have been told without the log.
        if self.tx_capture_armed
            && self.name == "WIFI_MAC"
            && word == TX_PLCP0_OFFSET
            && merged & TX_GO_MASK == TX_GO_MASK
        {
            self.tx_handoffs.push(TxHandoff {
                at: cx.now,
                pc: cx.pc,
                plcp0: merged,
            });
            // The machine reads guest RAM for the log, and a peripheral
            // cannot; end the slice so it does that before the guest can
            // reuse the buffer.
            cx.yield_to_machine();
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    /// `WIFI_MAC` publishes a table; `WIFI_PWR` and `I2C_MST_MEM` do not.
    ///
    /// # Why one block and not all three
    ///
    /// Publishing a table is a statement that *somebody graded this block*,
    /// and `--strict-grade` then stops at it instead of passing over it. M4
    /// P2 gave `WIFI_MAC` registers the air writes and answers, so it is
    /// graded; nobody has graded `WIFI_PWR` or the PHY's command memory, and
    /// `nobody graded this` is not the same statement as `this is modelled`
    /// (see [`Peripheral::reg_grade`]'s own docs).
    ///
    /// # Why every entry is `modeled`, including the ones with a trace line
    ///
    /// `Measured` means a committed silicon transcript agrees with the
    /// model. Nothing here has one, and the register meanings themselves are
    /// inferences from a writer's name and an observed value — the module
    /// docs say so. The four offsets the air touches carry their evidence in
    /// [`AIR_GRADES`]; the rest of the window is `modeled` as well, which is
    /// the honest answer for a block whose layout is a closed blob's.
    ///
    /// **The blast radius, stated:** before this, `WIFI_MAC` published no
    /// table and `--strict-grade` passed over it. It no longer does, so a
    /// `--strict-grade documented` run over the default scope now stops on
    /// the first `WIFI_MAC` read — which is what the flag is for. The two
    /// tests that name a scope (`tests/usb_attached.rs` g4_4,
    /// `tests/rom_download_console.rs`) are unaffected because they name
    /// one, and `the_boot_reads_registers_we_only_modelled_and_this_is_which`
    /// still stops where it stopped: its first violation is
    /// `I2C_ANA_MST+0x004` at cycle 246,696, and the blob does not reach
    /// this window until radio init at ~30 ms.
    fn reg_grade(&self, _off: u32) -> Option<RegGrade> {
        (self.name == "WIFI_MAC").then_some(RegGrade::Modeled)
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
        assert_eq!(
            sb.read(&mut c, 0x0000),
            0x0006_0267,
            "a command word is remembered"
        );

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

    /// The radio TX log's trigger: `mac_tx_set_plcp0` programming the
    /// pointer is **not** a frame, and `hal_mac_txq_enable` re-writing the
    /// same word with the strobe **is**. The values are the ones the
    /// `test_espnow` image produced (M4 P0's ledger).
    #[test]
    fn only_the_strobed_plcp0_write_is_a_tx_handoff() {
        let mut sb = Sandbox::new();
        let mut w = WifiStub::new();

        // Nothing is listening yet: the strobe records nothing at all, which
        // is what keeps every run without `--tx-log` the run it always was.
        sb.write(&mut w, TX_PLCP0_OFFSET, 0xc061_de88);
        assert!(
            w.take_tx_handoffs().is_empty(),
            "an unarmed block records nothing"
        );
        w.arm_tx_log();

        // `mac_tx_set_plcp0+0x6a`: the pointer, no strobe. Not a frame yet.
        sb.now = 165_826_331;
        sb.write(&mut w, TX_PLCP0_OFFSET, 0x0061_de88);
        assert!(
            w.take_tx_handoffs().is_empty(),
            "the pointer alone is not a handoff"
        );

        // `hal_mac_txq_enable+0xe`: the same word, bits 31 and 30 set.
        sb.now = 165_826_944;
        sb.write(&mut w, TX_PLCP0_OFFSET, 0xc061_de88);
        let armed = w.take_tx_handoffs();
        assert_eq!(armed.len(), 1, "{armed:?}");
        assert_eq!(armed[0].plcp0, 0xc061_de88);
        assert_eq!(armed[0].at, 165_826_944);
        assert_eq!(
            armed[0].descriptor(),
            0x4081_de88,
            "the low 20 bits, completed with the DRAM base"
        );
        assert!(w.take_tx_handoffs().is_empty(), "drained once, not twice");

        // Every other offset in the slot group is silent, however it is
        // written — only PLCP0 carries a pointer.
        for off in [TX_SLOT_BASE_OFFSET, 0x4d60, 0x4d64, 0x5488, 0x54bc] {
            sb.write(&mut w, off, 0xffff_ffff);
        }
        assert!(w.take_tx_handoffs().is_empty(), "only +0x4d6c arms");

        // And the block only arms on `WIFI_MAC`: the same offset in the PWR
        // window is an ordinary register.
        let mut p = WifiStub::pwr();
        p.arm_tx_log();
        sb.write(&mut p, TX_PLCP0_OFFSET, 0xc061_de88);
        assert!(p.take_tx_handoffs().is_empty(), "WIFI_PWR has no TX slot");
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
