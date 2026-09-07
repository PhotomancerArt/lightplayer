//! The C6's interrupt matrix: `INTERRUPT_CORE0` + `PLIC_MX`, as one state.
//!
//! # The seam (plan DD22)
//!
//! [`Esp32C6IntMatrix`] lives on the bus as its [`CpuIntMatrix`] and is the
//! **single source of truth** for the routing configuration. The two register
//! blocks that configure it — `INTERRUPT_CORE0` at `0x6001_0000` and
//! `PLIC_MX` at `0x2000_1000` — are ordinary peripherals on the decode table,
//! but they hold no state of their own: [`InterruptCore0View`] and
//! [`PlicMxView`] are register **views** that write into the matrix through
//! [`BusCx::matrix`] and read back from it. One state, nothing to keep in
//! step. (P4 documented the alternative — rebuild the matrix from the blocks'
//! registers on every ask — and it is not taken because the ask happens on
//! every MMIO store, and a rebuild from 77 map registers on every store is
//! the wrong side of the trade-off once the matrix is real.)
//!
//! # The semantics, all verified in `m3/discovery-esp-hal-csr-irq.md` §2
//!
//! - `core_0_intr_map[s]` holds the CPU interrupt number source `s` is routed
//!   to; 31 means disabled (`interrupts.disabled_interrupt`). Written by
//!   `_setup_interrupts` for all 77 sources and by `map_raw`.
//! - A CPU interrupt `n` is **asserted** when any source mapped to it is
//!   high, **eligible** when `MXINT_ENABLE[n]` is set and
//!   `MXINT_PRI[n] >= MXINT_THRESH` (`plic.rs:68-69`, the comment esp-hal
//!   leaves on `change_current_runlevel`).
//! - Among eligible interrupts the highest `MXINT_PRI` wins. **Ties go to
//!   the higher number — modeled, not verified against silicon**; esp-hal
//!   never relies on tie order (its vectored interrupts get distinct
//!   priorities).
//! - `MXINT_CLEAR` is read-modify-written by esp-hal (`plic.rs:34-37`), so it
//!   is a write-1 pulse that **reads back 0**; storing it would hold every
//!   interrupt clear forever. `MXINT_ENABLE` and `MXINT_TYPE` are also RMW'd
//!   and must read back what was written.
//! - `MXINT_THRESH` is 8 bits and resets to **1**, not 0: esp-hal's
//!   `RunLevel::ThreadMode` is threshold 1.
//! - `MXINT_TYPE` bit set = edge. Everything esp-hal configures is Level
//!   (`riscv.rs:491`, `enable_direct` at `:360`). An edge-typed interrupt is
//!   **modelled as level** here: the latch a real edge would set has no state
//!   to live in, because [`CpuIntMatrix::cpu_interrupt`] is a pure function
//!   of the levels (that purity is what lets the bus ask it on every store).
//!   A write that sets an edge bit is logged so a future image that does it
//!   is visible.
//! - `core_0_intr_status[0..3]` is **live**: re-derived from the source levels
//!   on every read. `handle_interrupts` reads it once per entry
//!   (`riscv.rs:565`) and re-reads `core_0_intr_map` per set bit.
//! - `EMIP_STATUS` reads `asserted & enable` (modelled; esp-hal never reads
//!   it). `MXINT_CLAIM` is accepted and read back.
//!
//! Software interrupts are not here: `INTPRI.cpu_intr_from_cpu[n]` drives
//! source `22 + n` as a level, and that is [`crate::periph::intpri`].

use core::any::Any;

use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, CpuIntMatrix, IrqLines, Peripheral, Width};

use crate::regs;

/// The number of peripheral interrupt **sources** the C6 declares
/// (`esp32c6-0.23.2/src/interrupt.rs`, 0..=76).
pub const SOURCE_COUNT: u16 = 77;

/// The CPU interrupt number that means "disabled" on the C6
/// (`esp-metadata-generated`: `interrupts.disabled_interrupt`).
pub const DISABLED_CPU_INTERRUPT: u8 = 31;

/// CPU interrupts 0..32: `mtvec` vectored slots, `mie` bits, `MXINT_*` bits.
pub const CPU_INTERRUPT_COUNT: usize = 32;

/// `MXINT_THRESH` after reset. esp-hal's `RunLevel::ThreadMode` is
/// threshold 1 (`plic.rs:48-55`); 0 would let a priority-0 interrupt through.
pub const MXINT_THRESH_RESET: u8 = 1;

// INTERRUPT_CORE0 offsets (`regs::INTERRUPT_CORE0`).
const CORE0_MAP_END: u32 = 4 * SOURCE_COUNT as u32; // 0x134
const CORE0_STATUS0: u32 = 0x134;
const CORE0_STATUS2: u32 = 0x13c;
const CORE0_CLOCK_GATE: u32 = 0x140;
const CORE0_REG_DATE: u32 = 0x7fc;

// PLIC_MX offsets (`regs::PLIC_MX`).
const PLIC_ENABLE: u32 = 0x00;
const PLIC_TYPE: u32 = 0x04;
const PLIC_CLEAR: u32 = 0x08;
const PLIC_EMIP: u32 = 0x0c;
const PLIC_PRI0: u32 = 0x10;
const PLIC_PRI31: u32 = 0x8c;
const PLIC_THRESH: u32 = 0x90;
const PLIC_CLAIM: u32 = 0x94;

/// The C6's interrupt matrix. See the module docs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Esp32C6IntMatrix {
    /// `core_0_intr_map[s]`: the CPU interrupt source `s` routes to.
    map: [u8; SOURCE_COUNT as usize],
    core0_clock_gate: u32,
    core0_reg_date: u32,
    /// `MXINT_ENABLE`, bit per CPU interrupt.
    enable: u32,
    /// `MXINT_TYPE`, bit per CPU interrupt (1 = edge).
    kind: u32,
    /// `MXINT_PRI[n]`, four bits each.
    pri: [u8; CPU_INTERRUPT_COUNT],
    /// `MXINT_THRESH`, eight bits.
    thresh: u8,
    claim: u32,
}

impl Default for Esp32C6IntMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl Esp32C6IntMatrix {
    pub fn new() -> Self {
        Self {
            map: [0; SOURCE_COUNT as usize],
            core0_clock_gate: 0,
            core0_reg_date: 0,
            enable: 0,
            kind: 0,
            pri: [0; CPU_INTERRUPT_COUNT],
            thresh: MXINT_THRESH_RESET,
            claim: 0,
        }
    }

    // ---- the configuration, as the firmware sees it ----------------------

    /// `core_0_intr_map[source]`, or `None` past the last source.
    pub fn map(&self, source: u16) -> Option<u8> {
        self.map.get(usize::from(source)).copied()
    }

    pub fn enable(&self) -> u32 {
        self.enable
    }

    pub fn kind(&self) -> u32 {
        self.kind
    }

    pub fn priority(&self, cpu_interrupt: u8) -> u8 {
        self.pri[usize::from(cpu_interrupt & 31)]
    }

    pub fn threshold(&self) -> u8 {
        self.thresh
    }

    // ---- the derivations ------------------------------------------------

    /// Bit `n` set: some source routed to CPU interrupt `n` is high.
    pub fn asserted(&self, irq: &IrqLines) -> u32 {
        let mut out = 0u32;
        for (w, word) in irq.raw().iter().enumerate() {
            let mut bits = *word;
            while bits != 0 {
                let bit = bits.trailing_zeros();
                bits &= bits - 1;
                let source = w * 64 + bit as usize;
                if let Some(&n) = self.map.get(source)
                    && n < DISABLED_CPU_INTERRUPT
                {
                    out |= 1 << n;
                }
            }
        }
        out
    }

    /// `EMIP_STATUS`: asserted and enabled. Modelled — esp-hal never reads it.
    pub fn emip_status(&self, irq: &IrqLines) -> u32 {
        self.asserted(irq) & self.enable
    }

    /// `core_0_intr_status[k]`: the raw source levels, word `k`.
    fn status_word(&self, irq: &IrqLines, k: u32) -> u32 {
        let raw = irq.raw();
        let bit = 32 * k;
        let word = raw[(bit / 64) as usize] >> (bit % 64);
        let mut v = word as u32;
        // Sources past 76 do not exist; keep the word honest.
        if k == 2 {
            v &= (1u32 << (SOURCE_COUNT - 64)) - 1;
        }
        v
    }

    // ---- register views ---------------------------------------------------

    fn read_core0(&self, off: u32, irq: &IrqLines) -> u32 {
        match off {
            o if o < CORE0_MAP_END => u32::from(self.map[(o / 4) as usize]),
            CORE0_STATUS0..=CORE0_STATUS2 => self.status_word(irq, (off - CORE0_STATUS0) / 4),
            CORE0_CLOCK_GATE => self.core0_clock_gate,
            CORE0_REG_DATE => self.core0_reg_date,
            _ => 0,
        }
    }

    fn write_core0(&mut self, off: u32, value: u32) {
        match off {
            o if o < CORE0_MAP_END => {
                // The register is 32 bits wide in the PAC, but only a CPU
                // interrupt number fits; 31 and above all mean "disabled"
                // (`CpuInterrupt::from_u32` yields `None`, treated as not
                // enabled, discovery §4).
                self.map[(o / 4) as usize] = value.min(u32::from(DISABLED_CPU_INTERRUPT)) as u8;
            }
            CORE0_CLOCK_GATE => self.core0_clock_gate = value,
            CORE0_REG_DATE => self.core0_reg_date = value,
            // `core_0_intr_status` is read-only.
            _ => {}
        }
    }

    fn read_plic(&self, off: u32, irq: &IrqLines) -> u32 {
        match off {
            PLIC_ENABLE => self.enable,
            PLIC_TYPE => self.kind,
            // Write-1-pulse: reads 0, always. See the module docs.
            PLIC_CLEAR => 0,
            PLIC_EMIP => self.emip_status(irq),
            PLIC_PRI0..=PLIC_PRI31 => u32::from(self.pri[((off - PLIC_PRI0) / 4) as usize]),
            PLIC_THRESH => u32::from(self.thresh),
            PLIC_CLAIM => self.claim,
            _ => 0,
        }
    }

    fn write_plic(&mut self, off: u32, value: u32) {
        match off {
            PLIC_ENABLE => self.enable = value,
            PLIC_TYPE => {
                if value & !self.kind != 0 {
                    log::warn!(
                        "PLIC_MX: MXINT_TYPE set edge bits {:#010x}; edge-typed CPU interrupts \
                         are modelled as level (see intmatrix.rs)",
                        value & !self.kind
                    );
                }
                self.kind = value;
            }
            // Nothing to clear: level interrupts have no latch, and edge
            // ones are modelled as level. The write is consumed.
            PLIC_CLEAR => {}
            PLIC_PRI0..=PLIC_PRI31 => {
                self.pri[((off - PLIC_PRI0) / 4) as usize] = (value & 0xf) as u8;
            }
            PLIC_THRESH => self.thresh = (value & 0xff) as u8,
            PLIC_CLAIM => self.claim = value,
            _ => {}
        }
    }
}

impl CpuIntMatrix for Esp32C6IntMatrix {
    fn cpu_interrupt(&self, _hart: usize, irq: &IrqLines) -> Option<u8> {
        let mut eligible = self.asserted(irq) & self.enable;
        let mut best: Option<(u8, u8)> = None;
        while eligible != 0 {
            let n = eligible.trailing_zeros() as u8;
            eligible &= eligible - 1;
            let p = self.pri[usize::from(n)];
            if p < self.thresh {
                continue;
            }
            // `>=`: a tie goes to the higher number (ascending scan).
            if best.is_none_or(|(_, bp)| p >= bp) {
                best = Some((n, p));
            }
        }
        best.map(|(n, _)| n)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(160);
        out.extend_from_slice(&self.map);
        out.extend_from_slice(&self.core0_clock_gate.to_le_bytes());
        out.extend_from_slice(&self.core0_reg_date.to_le_bytes());
        out.extend_from_slice(&self.enable.to_le_bytes());
        out.extend_from_slice(&self.kind.to_le_bytes());
        out.extend_from_slice(&self.pri);
        out.push(self.thresh);
        out.extend_from_slice(&self.claim.to_le_bytes());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let expected = SOURCE_COUNT as usize + 16 + CPU_INTERRUPT_COUNT + 1 + 4;
        if bytes.len() != expected {
            log::warn!(
                "Esp32C6IntMatrix::load_state: {} bytes, expected {expected}; ignored",
                bytes.len()
            );
            return;
        }
        let (map, rest) = bytes.split_at(SOURCE_COUNT as usize);
        self.map.copy_from_slice(map);
        let word = |b: &[u8]| u32::from_le_bytes(b.try_into().expect("4 bytes"));
        self.core0_clock_gate = word(&rest[0..4]);
        self.core0_reg_date = word(&rest[4..8]);
        self.enable = word(&rest[8..12]);
        self.kind = word(&rest[12..16]);
        let rest = &rest[16..];
        self.pri.copy_from_slice(&rest[..CPU_INTERRUPT_COUNT]);
        self.thresh = rest[CPU_INTERRUPT_COUNT];
        self.claim = word(&rest[CPU_INTERRUPT_COUNT + 1..]);
    }
}

/// The C6 matrix behind a [`BusCx`], or a panic naming the build bug: the
/// views are only ever registered by a machine that installed the matrix.
fn matrix<'a>(cx: &'a mut BusCx<'_>) -> &'a mut Esp32C6IntMatrix {
    cx.matrix
        .as_any_mut()
        .downcast_mut::<Esp32C6IntMatrix>()
        .expect("INTERRUPT_CORE0/PLIC_MX views need Esp32C6IntMatrix on the bus")
}

/// `INTERRUPT_CORE0` at `0x6001_0000`: a view into the matrix. Stateless.
#[derive(Clone, Copy, Debug, Default)]
pub struct InterruptCore0View;

impl Peripheral for InterruptCore0View {
    fn name(&self) -> &'static str {
        "INTERRUPT_CORE0"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        let word = off & !3;
        // The status words come from the levels; the borrow of `irq` and of
        // the matrix are two fields of the same context.
        let m = cx
            .matrix
            .as_any()
            .downcast_ref::<Esp32C6IntMatrix>()
            .expect("INTERRUPT_CORE0 view needs Esp32C6IntMatrix on the bus");
        lane_of(m.read_core0(word, cx.irq), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        let old = {
            let m = cx
                .matrix
                .as_any()
                .downcast_ref::<Esp32C6IntMatrix>()
                .expect("INTERRUPT_CORE0 view needs Esp32C6IntMatrix on the bus");
            m.read_core0(word, cx.irq)
        };
        matrix(cx).write_core0(word, merge_lane(old, off, width, value));
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::INTERRUPT_CORE0.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        // The state is the matrix's, and the snapshot carries it there.
        Vec::new()
    }

    fn load_state(&mut self, _bytes: &[u8]) {}
}

/// `PLIC_MX` at `0x2000_1000`: a view into the matrix. Stateless.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlicMxView;

impl Peripheral for PlicMxView {
    fn name(&self) -> &'static str {
        "PLIC_MX"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        let word = off & !3;
        let m = cx
            .matrix
            .as_any()
            .downcast_ref::<Esp32C6IntMatrix>()
            .expect("PLIC_MX view needs Esp32C6IntMatrix on the bus");
        lane_of(m.read_plic(word, cx.irq), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        let old = {
            let m = cx
                .matrix
                .as_any()
                .downcast_ref::<Esp32C6IntMatrix>()
                .expect("PLIC_MX view needs Esp32C6IntMatrix on the bus");
            m.read_plic(word, cx.irq)
        };
        matrix(cx).write_plic(word, merge_lane(old, off, width, value));
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::PLIC_MX.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        Vec::new()
    }

    fn load_state(&mut self, _bytes: &[u8]) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regs::source;
    use lp_emu_esp_common::Sandbox;

    fn sandbox() -> Sandbox {
        Sandbox::new().with_matrix(Box::new(Esp32C6IntMatrix::new()))
    }

    fn matrix_of(sb: &Sandbox) -> &Esp32C6IntMatrix {
        sb.matrix.as_any().downcast_ref().unwrap()
    }

    /// What `_setup_interrupts` + `init_vectoring` + `enable_direct` do for
    /// the tick (priority 1 → CPU interrupt 16) and SWI0 (CPU interrupt 1).
    fn esp_hal_setup(sb: &mut Sandbox) {
        let mut core0 = InterruptCore0View;
        let mut plic = PlicMxView;
        for s in 0..SOURCE_COUNT {
            sb.write(&mut core0, 4 * u32::from(s), 31);
        }
        // Vectored: kind Level, priority n-15, enabled — for 16..=30.
        for n in 16u32..=30 {
            sb.write(&mut plic, PLIC_PRI0 + 4 * n, n - 15);
            let en = sb.read(&mut plic, PLIC_ENABLE);
            sb.write(&mut plic, PLIC_ENABLE, en | (1 << n));
        }
        // The tick: TG0_T0_LEVEL → CPU interrupt 16 (Priority1).
        sb.write(&mut core0, 4 * u32::from(source::TG0_T0_LEVEL), 16);
        // SWI0 via enable_direct: map 22 → 1, pri 1, level, enable.
        sb.write(&mut core0, 4 * u32::from(source::FROM_CPU_INTR0), 1);
        sb.write(&mut plic, PLIC_PRI0 + 4, 1);
        let en = sb.read(&mut plic, PLIC_ENABLE);
        sb.write(&mut plic, PLIC_ENABLE, en | 2);
    }

    #[test]
    fn nothing_is_asserted_until_a_source_is_high_and_mapped_and_enabled() {
        let mut sb = sandbox();
        assert_eq!(matrix_of(&sb).cpu_interrupt(0, &sb.irq), None);
        sb.irq.set_level(source::TG0_T0_LEVEL, true);
        assert_eq!(
            matrix_of(&sb).cpu_interrupt(0, &sb.irq),
            None,
            "mapped to 0 but not enabled"
        );
        esp_hal_setup(&mut sb);
        assert_eq!(matrix_of(&sb).cpu_interrupt(0, &sb.irq), Some(16));
        sb.irq.set_level(source::TG0_T0_LEVEL, false);
        assert_eq!(matrix_of(&sb).cpu_interrupt(0, &sb.irq), None);
    }

    #[test]
    fn the_threshold_gates_by_priority_and_resets_to_one() {
        let mut sb = sandbox();
        let mut plic = PlicMxView;
        assert_eq!(sb.read(&mut plic, PLIC_THRESH), 1, "reset threshold is 1");
        esp_hal_setup(&mut sb);
        sb.irq.set_level(source::TG0_T0_LEVEL, true); // pri 1
        assert_eq!(matrix_of(&sb).cpu_interrupt(0, &sb.irq), Some(16));
        // handle_interrupts raises the threshold to level + 1 = 2.
        sb.write(&mut plic, PLIC_THRESH, 2);
        assert_eq!(matrix_of(&sb).cpu_interrupt(0, &sb.irq), None);
        sb.write(&mut plic, PLIC_THRESH, 1);
        assert_eq!(matrix_of(&sb).cpu_interrupt(0, &sb.irq), Some(16));
        // Eight bits wide.
        sb.write(&mut plic, PLIC_THRESH, 0x1ff);
        assert_eq!(sb.read(&mut plic, PLIC_THRESH), 0xff);
    }

    #[test]
    fn the_highest_priority_wins_and_a_tie_goes_to_the_higher_number() {
        let mut sb = sandbox();
        esp_hal_setup(&mut sb);
        // UART0 → CPU 18 (priority 3), tick → 16 (priority 1).
        let mut core0 = InterruptCore0View;
        sb.write(&mut core0, 4 * u32::from(source::UART0), 18);
        sb.irq.set_level(source::TG0_T0_LEVEL, true);
        sb.irq.set_level(source::UART0, true);
        assert_eq!(matrix_of(&sb).cpu_interrupt(0, &sb.irq), Some(18));
        // Same priority as the tick on CPU interrupt 1 (SWI0): 1 vs 16.
        sb.irq.set_level(source::UART0, false);
        sb.irq.set_level(source::FROM_CPU_INTR0, true);
        assert_eq!(
            matrix_of(&sb).cpu_interrupt(0, &sb.irq),
            Some(16),
            "modelled tie order: the higher number"
        );
    }

    #[test]
    fn mxint_clear_is_a_pulse_that_reads_zero_while_enable_and_type_read_back() {
        let mut sb = sandbox();
        let mut plic = PlicMxView;
        sb.write(&mut plic, PLIC_ENABLE, 0x0001_0002);
        sb.write(&mut plic, PLIC_TYPE, 0x0000_0002);
        // esp-hal's RMW: `modify(|r, w| bits(r.bits() | (1 << n)))`.
        let r = sb.read(&mut plic, PLIC_CLEAR);
        sb.write(&mut plic, PLIC_CLEAR, r | (1 << 16));
        assert_eq!(sb.read(&mut plic, PLIC_CLEAR), 0);
        assert_eq!(sb.read(&mut plic, PLIC_ENABLE), 0x0001_0002);
        assert_eq!(sb.read(&mut plic, PLIC_TYPE), 0x0000_0002);
        // Priorities are four bits.
        sb.write(&mut plic, PLIC_PRI0 + 4 * 16, 0x1f);
        assert_eq!(sb.read(&mut plic, PLIC_PRI0 + 4 * 16), 0xf);
    }

    #[test]
    fn the_status_words_are_the_live_source_levels_and_the_map_clamps_to_disabled() {
        let mut sb = sandbox();
        let mut core0 = InterruptCore0View;
        sb.irq.set_level(source::TG0_T0_LEVEL, true); // 51 → word 1 bit 19
        sb.irq.set_level(source::FROM_CPU_INTR0, true); // 22 → word 0 bit 22
        sb.irq.set_level(source::ECC, true); // 76 → word 2 bit 12
        assert_eq!(sb.read(&mut core0, CORE0_STATUS0), 1 << 22);
        assert_eq!(sb.read(&mut core0, CORE0_STATUS0 + 4), 1 << 19);
        assert_eq!(sb.read(&mut core0, CORE0_STATUS0 + 8), 1 << 12);
        sb.irq.set_level(source::TG0_T0_LEVEL, false);
        assert_eq!(
            sb.read(&mut core0, CORE0_STATUS0 + 4),
            0,
            "live, not latched"
        );

        sb.write(&mut core0, 4 * 43, 31);
        assert_eq!(sb.read(&mut core0, 4 * 43), 31);
        sb.write(&mut core0, 4 * 43, 0xffff);
        assert_eq!(
            sb.read(&mut core0, 4 * 43),
            31,
            "anything past 30 is disabled"
        );
        assert_eq!(matrix_of(&sb).map(43), Some(31));
        assert_eq!(matrix_of(&sb).map(77), None);
    }

    #[test]
    fn emip_status_is_asserted_and_enabled() {
        let mut sb = sandbox();
        esp_hal_setup(&mut sb);
        let mut plic = PlicMxView;
        sb.irq.set_level(source::TG0_T0_LEVEL, true);
        assert_eq!(sb.read(&mut plic, PLIC_EMIP), 1 << 16);
        sb.write(&mut plic, PLIC_ENABLE, 0);
        assert_eq!(sb.read(&mut plic, PLIC_EMIP), 0);
    }

    #[test]
    fn the_state_round_trips_through_a_snapshot() {
        let mut sb = sandbox();
        esp_hal_setup(&mut sb);
        let mut plic = PlicMxView;
        sb.write(&mut plic, PLIC_THRESH, 3);
        sb.write(&mut plic, PLIC_CLAIM, 9);
        let saved = sb.matrix.save_state();
        let mut other = Esp32C6IntMatrix::new();
        other.load_state(&saved);
        assert_eq!(&other, matrix_of(&sb));
        assert_eq!(other.threshold(), 3);
        assert_eq!(other.map(22), Some(1));
        assert_eq!(other.priority(1), 1);
        assert_eq!(other.enable() & 2, 2);
    }

    #[test]
    fn the_source_count_matches_the_generated_table() {
        assert_eq!(regs::INTERRUPT_SOURCES.len(), SOURCE_COUNT as usize);
        assert_eq!(regs::INTERRUPT_SOURCES.last().map(|(n, _)| *n), Some(76));
        assert_eq!(DISABLED_CPU_INTERRUPT, 31);
        assert_eq!(source::SYSTIMER_TARGET0, 57);
        assert_eq!(source::UART0, 43);
    }
}
