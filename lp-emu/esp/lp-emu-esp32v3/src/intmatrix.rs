//! The classic's interrupt matrix: DPORT's **two** per-core source maps, as
//! one state.
//!
//! # The seam (plan DD22, the C6's precedent)
//!
//! [`Esp32V3IntMatrix`] lives on the bus as its [`CpuIntMatrix`] and is the
//! single source of truth for the routing. The register block that
//! configures it — `DPORT` — is an ordinary peripheral on the decode table,
//! and it holds none of this state: [`crate::periph::dport::DportView`]
//! reaches the matrix through [`BusCx::matrix`] and reads back from it. One
//! state, nothing to keep in step.
//!
//! # What this matrix can and cannot answer, and why that is an ISA fact
//!
//! On Xtensa the interrupt **enable** mask is `INTENABLE` — a CPU register
//! the bus cannot see — and the priority resolution is `PS.INTLEVEL` against
//! the chip's fixed per-interrupt level table (which lives on the *hart*,
//! [`crate::machine::CORE_INTERRUPTS`]). So this matrix answers
//! [`CpuIntMatrix::asserted`] — *which CPU interrupts are asserted* — and
//! leaves [`CpuIntMatrix::cpu_interrupt`] at M2 P2's `None` default. The hart
//! resolves. That split is the whole reason `asserted` exists (M2 P2,
//! plan D2).
//!
//! # The registers, and where each fact comes from
//!
//! - **`core_0_intr_map[s]` @`+0x104`, `core_1_intr_map[s]` @`+0x218`, sixty-
//!   nine sources each.** Sixty-nine, not seventy: `esp32-0.40.2/src/dport.rs`
//!   declares `core_0_intr_map: [CORE_0_INTR_MAP; 69]` and
//!   `core_1_intr_map: [CORE_1_INTR_MAP; 69]`, and the generated name table
//!   ends at `core_1_intr_map68` @`+0x328` with `ahblite_mpu_table_uart` at
//!   `+0x32c`. The phase file's "70" was arithmetic on the wrong end of a
//!   half-open range.
//! - **The map register holds a CPU interrupt number, and there is no
//!   hardware "disabled" value.** esp-hal's `map_raw`
//!   (`third_party/esp-hal/src/interrupt/mod.rs:356-370`) writes the raw
//!   `cpu_interrupt` word into the calling core's map; `disable`
//!   (`:352-354`) writes `DISABLED_CPU_INTERRUPT`, which on Xtensa is
//!   **16** (`interrupt/xtensa.rs:278`). Sixteen is a real CPU interrupt on
//!   this chip — it is simply one esp-hal never enables in `INTENABLE` — so
//!   this matrix routes it like any other and lets the hart's enable mask do
//!   the disabling. Modelling 16 as "not routed" would put an
//!   esp-hal convention inside the silicon, and would be wrong for any image
//!   that used the line. A value **≥ 32** has nowhere to go: the CPU
//!   interrupt space is 32 wide, so those are remembered and not routed.
//! - **Which core a source is mapped on is the calling core's choice.**
//!   esp-hal maps into `Cpu::current()`'s matrix, which is the mechanism
//!   behind `ISR_ON_APP_CORE` (ADR `2026-08-04-rmt-isr-on-app-core`).
//! - **`core_0_intr_status[0..3]` @`+0xEC`, `core_1_intr_status[0..3]`
//!   @`+0xF8` are read-only** (the generated table's `Access::ReadOnly`
//!   entries) and are the raw **source** levels, thirty-two per word — the
//!   PAC's field is `pro_intr_status_0`, bits 0:31, not a CPU-interrupt
//!   mask. Both cores see the same source levels; the maps are what differ.
//!   Derived live from [`IrqLines`] on every read, exactly as the C6's
//!   `core_0_intr_status` is.
//! - **The four software interrupts** are `cpu_intr_from_cpu[0..4]`
//!   @`+0xDC..+0xEC`, one bit each (`cpu_intr`, bit 0). They drive interrupt
//!   **sources** `FROM_CPU_INTR0..3` = **24..27**
//!   (`esp32-0.40.2/src/lib.rs:259-266`), so the map registers decide which
//!   CPU interrupt each one raises rather than this file hardcoding a
//!   number. The firmware's `swi1` is the frame doorbell (M4) and `swi2`
//!   drives the io task's `InterruptExecutor` at Priority2, so swi2 is on
//!   M3's critical path and has to actually fire.
//!
//! ⚠️ The classic's fixed per-interrupt **level/type** table (which CPU
//! interrupt is level 1 / 3 / 5, which are edge and which level) is the
//! *hart's*: M1 P3 took it as `CoreConfig::interrupts`, and
//! [`crate::machine::CORE_INTERRUPTS`] is the single copy. A second copy here
//! is the kind of duplication that fails only under load.

use core::any::Any;

use lp_emu_esp_common::{CpuIntMatrix, IrqLines};

/// Peripheral interrupt **sources** the classic declares, per core.
///
/// `esp32-0.40.2/src/dport.rs:57-58`: `core_0_intr_map: [CORE_0_INTR_MAP; 69]`
/// and the same for core 1. The PAC's `Interrupt` enum stops at
/// `CACHE_IA` = 68.
pub const SOURCES: usize = 69;

/// CPU interrupts 0..32: `INTENABLE` bits, and the width of the mask
/// [`CpuIntMatrix::asserted`] answers with.
pub const CPU_INTERRUPTS: usize = 32;

/// The two cores' maps.
pub const CORES: usize = 2;

/// `FROM_CPU_INTR0`, the first of the four software interrupt **sources**
/// (`esp32-0.40.2/src/lib.rs:259-266`: 24, 25, 26, 27).
pub const FROM_CPU_INTR0: u16 = 24;

/// How many software interrupts the classic has: `cpu_intr_from_cpu[0..4]`.
pub const SOFTWARE_INTERRUPTS: u16 = 4;

/// The value esp-hal writes into a map register to mean "disabled"
/// (`third_party/esp-hal/src/interrupt/xtensa.rs:278`).
///
/// **Not modelled as a special case** — see the module docs. Declared so the
/// tests can say what they are exercising and so a reader of a trace knows
/// what a `16` in a map register came from.
pub const ESP_HAL_DISABLED_CPU_INTERRUPT: u32 = 16;

/// The classic's per-core interrupt matrix. See the module docs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Esp32V3IntMatrix {
    /// `core_N_intr_map[s]`, as the guest wrote it. The whole word is kept
    /// so a driver's read-back is exact (esp-hal's `mapped_to_raw` reads it
    /// and feeds it to `CpuInterrupt::from_u32`); only values below
    /// [`CPU_INTERRUPTS`] route.
    map: [[u32; SOURCES]; CORES],
}

impl Default for Esp32V3IntMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl Esp32V3IntMatrix {
    /// The matrix as reset leaves it.
    ///
    /// Every map register's PAC reset is **0** (`regs::DPORT`'s `resets`
    /// table lists no offset in `0x104..0x32c`), so every source starts
    /// routed to CPU interrupt 0. That is not a convenience: it is what the
    /// part does, and it is harmless because `INTENABLE` also resets to 0.
    /// `esp_hal::init`'s `setup_interrupts` is what writes 16 over all of
    /// them, and it is the first MMIO the direct load performs (P3's stop
    /// A1, cycle 29).
    pub fn new() -> Self {
        Self {
            map: [[0; SOURCES]; CORES],
        }
    }

    /// `core_N_intr_map[source]` as the guest wrote it, or `None` past the
    /// last core or source.
    pub fn map(&self, core: usize, source: usize) -> Option<u32> {
        self.map.get(core)?.get(source).copied()
    }

    /// Route `source` on `core` to CPU interrupt `cpu_interrupt`. What a
    /// `core_N_intr_map[source]` store does, and what a test uses instead of
    /// spelling the offset.
    pub fn set_map(&mut self, core: usize, source: usize, cpu_interrupt: u32) {
        let Some(slot) = self.map.get_mut(core).and_then(|m| m.get_mut(source)) else {
            log::warn!(
                "Esp32V3IntMatrix: core {core} source {source} is outside \
                 {CORES}×{SOURCES}, dropped"
            );
            return;
        };
        *slot = cpu_interrupt;
    }

    /// Bit `n` set: some source routed to CPU interrupt `n` on `core` is
    /// high.
    ///
    /// Walks the levels rather than the map: a boot has a handful of live
    /// sources and sixty-nine map entries, and this runs on every MMIO store.
    pub fn asserted_on(&self, core: usize, irq: &IrqLines) -> u32 {
        let Some(map) = self.map.get(core) else {
            return 0;
        };
        let mut out = 0u32;
        for (w, word) in irq.raw().iter().enumerate() {
            let mut bits = *word;
            while bits != 0 {
                let bit = bits.trailing_zeros();
                bits &= bits - 1;
                let source = w * 64 + bit as usize;
                if let Some(&n) = map.get(source)
                    && n < CPU_INTERRUPTS as u32
                {
                    out |= 1 << n;
                }
            }
        }
        out
    }

    /// `core_N_intr_status[k]`: the raw **source** levels, word `k`
    /// (32 sources per word, three words). Read-only and live.
    ///
    /// The same for both cores — the levels are chip-wide (plan PD6) and it
    /// is the maps that are per-core — so `core` is not a parameter. Sources
    /// past [`SOURCES`] do not exist, and the top word says so.
    pub fn status_word(&self, irq: &IrqLines, k: u32) -> u32 {
        let raw = irq.raw();
        let bit = 32 * u64::from(k);
        let mut v = (raw[(bit / 64) as usize] >> (bit % 64)) as u32;
        if k == 2 {
            v &= (1u32 << (SOURCES - 64)) - 1;
        }
        v
    }

    /// The interrupt **source** the `n`th software interrupt drives, or
    /// `None` past the fourth.
    pub fn software_source(n: u16) -> Option<u16> {
        (n < SOFTWARE_INTERRUPTS).then(|| FROM_CPU_INTR0 + n)
    }
}

impl CpuIntMatrix for Esp32V3IntMatrix {
    /// Pure and cheap: the routing applied to the source levels, and nothing
    /// else. No enable mask, no priority — those are `INTENABLE` and
    /// `PS.INTLEVEL`, CPU registers this trait cannot see and must not
    /// pretend to.
    #[inline]
    fn asserted(&self, hart: usize, irq: &IrqLines) -> u32 {
        self.asserted_on(hart, irq)
    }

    // `cpu_interrupt` keeps M2 P2's `None` default. See the module docs: the
    // enable mask is a CPU register, so this matrix cannot resolve and the
    // hart does.

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(CORES * SOURCES * 4);
        for core in &self.map {
            for word in core {
                out.extend_from_slice(&word.to_le_bytes());
            }
        }
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let expected = CORES * SOURCES * 4;
        if bytes.len() != expected {
            log::warn!(
                "Esp32V3IntMatrix::load_state: {} bytes, expected {expected}; ignored",
                bytes.len()
            );
            return;
        }
        for (core, chunk) in self.map.iter_mut().zip(bytes.chunks_exact(SOURCES * 4)) {
            for (slot, word) in core.iter_mut().zip(chunk.chunks_exact(4)) {
                *slot = u32::from_le_bytes(word.try_into().expect("4 bytes"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_source_count_is_the_pacs_sixty_nine() {
        // `esp32-0.40.2/src/dport.rs:57-58`, and the generated table's own
        // count: `core_1_intr_map0` is at +0x218 and +0x104 + 69*4 = +0x218.
        assert_eq!(SOURCES, 69);
        assert_eq!(0x104 + SOURCES as u32 * 4, 0x218);
        assert_eq!(crate::regs::DPORT.name(0x218), Some("core_1_intr_map0"));
        assert_eq!(
            crate::regs::DPORT.name(0x218 + (SOURCES as u32 - 1) * 4),
            Some("core_1_intr_map68")
        );
        assert_eq!(
            crate::regs::DPORT.name(0x218 + SOURCES as u32 * 4),
            Some("ahblite_mpu_table_uart"),
            "the map ends here; a seventieth entry would land on another register"
        );
    }

    #[test]
    fn a_mapped_source_asserts_its_cpu_interrupt_on_that_core_only() {
        let mut m = Esp32V3IntMatrix::new();
        let mut irq = IrqLines::new();
        // Source 9 (`UART0` on this part) to CPU interrupt 23 on the PRO
        // core; the APP core leaves it wherever reset left it.
        m.set_map(0, 9, 23);
        m.set_map(1, 9, ESP_HAL_DISABLED_CPU_INTERRUPT);

        assert_eq!(m.asserted(0, &irq), 0, "nothing is high yet");
        irq.set_level(9, true);
        assert_eq!(m.asserted(0, &irq), 1 << 23);
        assert_eq!(
            m.asserted(1, &irq),
            1 << 16,
            "esp-hal's 16 is a real line the hart simply never enables, not a \
             hardware disable"
        );
        irq.set_level(9, false);
        assert_eq!(m.asserted(0, &irq), 0);
    }

    #[test]
    fn two_sources_on_one_cpu_interrupt_are_one_bit() {
        let mut m = Esp32V3IntMatrix::new();
        let mut irq = IrqLines::new();
        m.set_map(0, 9, 23);
        m.set_map(0, 10, 23);
        irq.set_level(9, true);
        assert_eq!(m.asserted(0, &irq), 1 << 23);
        irq.set_level(10, true);
        assert_eq!(m.asserted(0, &irq), 1 << 23);
        irq.set_level(9, false);
        assert_eq!(m.asserted(0, &irq), 1 << 23, "source 10 still holds it");
        irq.set_level(10, false);
        assert_eq!(m.asserted(0, &irq), 0);
    }

    #[test]
    fn a_map_value_past_the_cpu_interrupt_space_routes_nowhere_but_reads_back() {
        let mut m = Esp32V3IntMatrix::new();
        let mut irq = IrqLines::new();
        m.set_map(0, 9, 0xffff_ffff);
        irq.set_level(9, true);
        assert_eq!(m.asserted(0, &irq), 0);
        assert_eq!(m.map(0, 9), Some(0xffff_ffff), "the guest reads what it wrote");
    }

    #[test]
    fn the_four_software_interrupts_are_sources_24_to_27() {
        assert_eq!(Esp32V3IntMatrix::software_source(0), Some(24));
        assert_eq!(Esp32V3IntMatrix::software_source(1), Some(25));
        assert_eq!(Esp32V3IntMatrix::software_source(2), Some(26));
        assert_eq!(Esp32V3IntMatrix::software_source(3), Some(27));
        assert_eq!(Esp32V3IntMatrix::software_source(4), None);
    }

    #[test]
    fn the_status_words_are_the_raw_source_levels() {
        let m = Esp32V3IntMatrix::new();
        let mut irq = IrqLines::new();
        irq.set_level(0, true);
        irq.set_level(31, true);
        irq.set_level(32, true);
        irq.set_level(68, true);
        assert_eq!(m.status_word(&irq, 0), (1 << 0) | (1 << 31));
        assert_eq!(m.status_word(&irq, 1), 1);
        assert_eq!(m.status_word(&irq, 2), 1 << (68 - 64));
        // Source 69 does not exist, so nothing above bit 4 can be set.
        assert_eq!(m.status_word(&irq, 2) & !((1 << 5) - 1), 0);
    }

    #[test]
    fn the_matrix_round_trips_through_a_snapshot() {
        let mut m = Esp32V3IntMatrix::new();
        m.set_map(0, 9, 23);
        m.set_map(1, 68, 5);
        let bytes = m.save_state();
        let mut back = Esp32V3IntMatrix::new();
        back.load_state(&bytes);
        assert_eq!(back, m);
    }
}
