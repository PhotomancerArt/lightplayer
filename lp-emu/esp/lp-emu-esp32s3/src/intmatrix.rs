//! The S3's interrupt matrix: `INTERRUPT_CORE0` and `INTERRUPT_CORE1`'s
//! per-core source maps, as one state — **the mask form, and nothing else**.
//!
//! **The classic's file (`lp-emu-esp32v3/src/intmatrix.rs`) with the S3's
//! constants, never the C6's** (`m6/notes.md` §3.0 row 4, §4): the C6's
//! matrix is built around a PLIC and an `INTPRI`, neither of which exists
//! on this chip. The copy is here rather than a parameterisation of the
//! classic's because the classic must not move by a byte; M8's extraction
//! would take the routing table, the status words and the disabled-value
//! reasoning, and leave the source count and the register views behind.
//!
//! # The rule (X43, M4 P3b, PR #704)
//!
//! > ⚠️ the S3 bus must answer the **mask** form and **never** implement
//! > `cpu_interrupt`.
//!
//! On Xtensa the interrupt **enable** mask is `INTENABLE` — a CPU register
//! the bus cannot see — and the priority resolution is `PS.INTLEVEL`
//! against the chip's fixed per-interrupt level table, which is the
//! *hart's* ([`crate::machine::CORE_INTERRUPTS`]). So this matrix answers
//! [`CpuIntMatrix::asserted`] — *which CPU interrupts are asserted* — and
//! leaves [`CpuIntMatrix::cpu_interrupt`] at its `None` default. The hart
//! resolves.
//!
//! X43's failure mode is why the rule is written down: `XtHart` re-samples
//! the bus after every MMIO store (`resample_external`), so a matrix that
//! answered `None` from `cpu_interrupt` **zeroed the asserted mask on every
//! store**, and the software interrupt a task-switch had just raised was
//! dropped — the executor then never woke and nothing said so.
//! `tests/clock.rs` runs that exact sequence on this chip, at a real
//! quantum, because 1-cycle stepping masks the class.
//!
//! # The registers, and where each fact comes from
//!
//! - **`INTERRUPT_CORE0` and `INTERRUPT_CORE1` are one 4 KB block** at
//!   `0x600C_2000`: core 0's registers at `+0x000`, core 1's at `+0x800`
//!   (`esp32s3-0.35.2/src/lib.rs:785,794`; `regs::INTERRUPT_CORE1`'s first
//!   entry is `core_1_intr_map0` at `+0x800`). This machine registers them
//!   as **two views over one matrix**, each `0x800` long
//!   ([`InterruptCoreView`]), so a trace line names the core.
//! - **`core_N_intr_map[s]` `+0x000..+0x18c` = 99 entries**, stride 4. The
//!   PAC's `Interrupt` enum numbers 94 named sources over `0..=98` with
//!   gaps (`regs::INTERRUPT_SOURCES`), so 99 is the *number range* and not
//!   the variant count.
//! - **The map register holds a CPU interrupt number, and there is no
//!   hardware "disabled" value.** esp-hal's `map_raw`
//!   (`interrupt/mod.rs:356-370`) writes the raw `cpu_interrupt` word into
//!   `INTERRUPT_CORE0.core_0_intr_map(n)` / `INTERRUPT_CORE1.core_1_intr_map(n)`
//!   for the S3 (the `DPORT` alias is the classic's arm of the same
//!   `cfg_if`, `:52-57`); `disable` (`:352-354`) writes
//!   `DISABLED_CPU_INTERRUPT`, which on Xtensa is **16**
//!   (`interrupt/xtensa.rs:278`, a file that is not per-chip). Sixteen is a
//!   real CPU interrupt on this chip too — one esp-hal never enables in
//!   `INTENABLE` — so this matrix routes it like any other and lets the
//!   hart's enable mask do the disabling. Modelling 16 as "not routed"
//!   would put an esp-hal convention inside the silicon. A value **≥ 32**
//!   has nowhere to go: remembered, not routed.
//! - **Which core a source is mapped on is the calling core's choice.**
//! - **`core_N_intr_status[0..4]` `+0x18c..+0x19c` are read-only** (the
//!   generated table's `Access::ReadOnly`) and are the raw **source**
//!   levels, thirty-two per word, four words for 99 sources. Both cores
//!   see the same levels; the maps are what differ. Derived live from
//!   [`IrqLines`] on every read.
//! - **The four software interrupts** are `SYSTEM.cpu_intr_from_cpu[0..4]`
//!   ([`crate::periph::system`]), driving sources **`FROM_CPU_INTR0..3` =
//!   79..82** (`regs::source`). The map registers decide which CPU
//!   interrupt each one raises; this file hardcodes no number.
//! - `clock_gate` `+0x19c` (reset 1) and `date` `+0x7fc` are
//!   accept-and-remember in each view.

use core::any::Any;

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, CpuIntMatrix, IrqLines, Peripheral, RegFile, Width};

use crate::regs::{self, source};

/// Peripheral interrupt **sources** the S3 declares, per core: the number
/// range of the PAC's `Interrupt` enum, `0..=98`, and the length of each
/// `core_N_intr_map` array (`regs::INTERRUPT_CORE0`: `core_0_intr_map0` at
/// `+0x000` … `core_0_intr_map98` at `+0x188`).
pub const SOURCES: usize = 99;

/// CPU interrupts 0..32: `INTENABLE` bits, and the width of the mask
/// [`CpuIntMatrix::asserted`] answers with.
pub const CPU_INTERRUPTS: usize = 32;

/// The two cores' maps.
pub const CORES: usize = 2;

/// `FROM_CPU_INTR0`, the first of the four software interrupt **sources**
/// (`regs::source`: 79, 80, 81, 82).
pub const FROM_CPU_INTR0: u16 = source::FROM_CPU_INTR0;

/// The value esp-hal writes into a map register to mean "disabled"
/// (`interrupt/xtensa.rs:278`).
///
/// **Not modelled as a special case** — see the module docs. Declared so
/// the tests can say what they are exercising and so a reader of a trace
/// knows what a `16` in a map register came from.
pub const ESP_HAL_DISABLED_CPU_INTERRUPT: u32 = 16;

/// The S3's per-core interrupt matrix. See the module docs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Esp32S3IntMatrix {
    /// `core_N_intr_map[s]`, as the guest wrote it. The whole word is kept
    /// so a driver's read-back is exact (esp-hal's `mapped_to_raw` reads it
    /// and feeds it to `CpuInterrupt::from_u32`); only values below
    /// [`CPU_INTERRUPTS`] route.
    map: [[u32; SOURCES]; CORES],
}

impl Default for Esp32S3IntMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl Esp32S3IntMatrix {
    /// The matrix as reset leaves it.
    ///
    /// Every map register's PAC reset is **0** (`regs::INTERRUPT_CORE0`'s
    /// `resets` table lists only `clock_gate`), so every source starts
    /// routed to CPU interrupt 0. That is what the part does, and it is
    /// harmless because `INTENABLE` also resets to 0. `esp_hal::init`'s
    /// `setup_interrupts` writes 16 over all of them.
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

    /// Route `source` on `core` to CPU interrupt `cpu_interrupt`.
    pub fn set_map(&mut self, core: usize, source: usize, cpu_interrupt: u32) {
        let Some(slot) = self.map.get_mut(core).and_then(|m| m.get_mut(source)) else {
            log::warn!(
                "Esp32S3IntMatrix: core {core} source {source} is outside \
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
    /// sources and ninety-nine map entries, and this runs on every MMIO
    /// store.
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
    /// (32 sources per word, four words). Read-only and live.
    ///
    /// The same for both cores — the levels are chip-wide (plan PD6) and it
    /// is the maps that are per-core — so `core` is not a parameter.
    /// Sources past [`SOURCES`] do not exist, and the top word says so.
    pub fn status_word(&self, irq: &IrqLines, k: u32) -> u32 {
        let raw = irq.raw();
        let bit = 32 * u64::from(k);
        let mut v = (raw[(bit / 64) as usize] >> (bit % 64)) as u32;
        if k == 3 {
            v &= (1u32 << (SOURCES - 96)) - 1;
        }
        v
    }

    /// The interrupt **source** the `n`th software interrupt drives, or
    /// `None` past the fourth.
    pub fn software_source(n: u16) -> Option<u16> {
        (n < 4).then(|| FROM_CPU_INTR0 + n)
    }
}

impl CpuIntMatrix for Esp32S3IntMatrix {
    /// Pure and cheap: the routing applied to the source levels, and nothing
    /// else. No enable mask, no priority — those are `INTENABLE` and
    /// `PS.INTLEVEL`, CPU registers this trait cannot see and must not
    /// pretend to.
    #[inline]
    fn asserted(&self, hart: usize, irq: &IrqLines) -> u32 {
        self.asserted_on(hart, irq)
    }

    // `cpu_interrupt` keeps the trait's `None` default. See the module docs:
    // the enable mask is a CPU register, so this matrix cannot resolve and
    // the hart does. **Do not add an override here** — X43.

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
                "Esp32S3IntMatrix::load_state: {} bytes, expected {expected}; ignored",
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

// ---------------------------------------------------------------------------
// The register views
// ---------------------------------------------------------------------------

/// One core's half of the block: `0x800` bytes.
pub const VIEW_LEN: u32 = 0x800;
/// `core_N_intr_map0`.
pub const INTR_MAP: u32 = 0x000;
/// One past `core_N_intr_map98`, which is `core_N_intr_status0`.
pub const INTR_STATUS: u32 = INTR_MAP + SOURCES as u32 * 4;
/// How many status words: 99 sources over 32-bit words.
pub const STATUS_WORDS: u32 = SOURCES.div_ceil(32) as u32;
/// One past `core_N_intr_status3`, which is `clock_gate`.
pub const CLOCK_GATE: u32 = INTR_STATUS + STATUS_WORDS * 4;
/// `date`.
pub const DATE: u32 = 0x7fc;

/// `INTERRUPT_CORE0` / `INTERRUPT_CORE1` — a register view over the bus's
/// [`Esp32S3IntMatrix`] for one core, with a [`RegFile`] behind the two
/// registers that are not the matrix's.
///
/// The matrix is the *bus's* (reached through [`BusCx::matrix`]) and the
/// snapshot carries it; this view holds no routing state of its own, so
/// two views over one matrix cannot disagree.
pub struct InterruptCoreView {
    core: usize,
    file: RegFile,
}

impl InterruptCoreView {
    fn new(core: usize) -> Self {
        let (name, table, table_base) = match core {
            0 => ("INTERRUPT_CORE0", &regs::INTERRUPT_CORE0, 0),
            _ => ("INTERRUPT_CORE1", &regs::INTERRUPT_CORE1, VIEW_LEN),
        };
        // The generated tables are offsets from the block's one base, so
        // core 1's are `+0x800` off this view's; the resets are seeded by
        // hand for the same reason, and the two registers the file answers
        // are graded as the PAC calls them (read-write, pretended about
        // by nobody: documented).
        let mut file = RegFile::new(name, VIEW_LEN);
        for (off, value) in table.resets {
            if *off >= table_base && *off < table_base + VIEW_LEN {
                file = file.with_reset(*off - table_base, *value);
            }
        }
        let mut file = file.with_pac_grades();
        for off in (INTR_MAP..CLOCK_GATE).step_by(4) {
            file = file.with_grade(off, RegGrade::Modeled);
        }
        file = file
            .with_grade(CLOCK_GATE, RegGrade::Documented)
            .with_grade(DATE, RegGrade::Documented);
        Self { core, file }
    }

    /// Core 0's half, `+0x000`.
    pub fn core0() -> Self {
        Self::new(0)
    }

    /// Core 1's half, `+0x800`.
    pub fn core1() -> Self {
        Self::new(1)
    }

    fn read_word(&self, off: u32, cx: &BusCx<'_>) -> u32 {
        if (INTR_MAP..INTR_STATUS).contains(&off) {
            let source = ((off - INTR_MAP) / 4) as usize;
            return matrix_ref(cx).map(self.core, source).unwrap_or(0);
        }
        if (INTR_STATUS..CLOCK_GATE).contains(&off) {
            let k = (off - INTR_STATUS) / 4;
            return matrix_ref(cx).status_word(cx.irq, k);
        }
        self.file.effective(off)
    }
}

fn matrix_ref<'a>(cx: &'a BusCx<'_>) -> &'a Esp32S3IntMatrix {
    cx.matrix
        .as_any()
        .downcast_ref::<Esp32S3IntMatrix>()
        .expect("the INTERRUPT_CORE views need Esp32S3IntMatrix on the bus")
}

fn matrix_mut<'a>(cx: &'a mut BusCx<'_>) -> &'a mut Esp32S3IntMatrix {
    cx.matrix
        .as_any_mut()
        .downcast_mut::<Esp32S3IntMatrix>()
        .expect("the INTERRUPT_CORE views need Esp32S3IntMatrix on the bus")
}

impl Peripheral for InterruptCoreView {
    fn name(&self) -> &'static str {
        if self.core == 0 {
            "INTERRUPT_CORE0"
        } else {
            "INTERRUPT_CORE1"
        }
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        lane_of(self.read_word(off & !3, cx), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        let old = self.read_word(word, cx);
        let next = merge_lane(old, off, width, value);
        if (INTR_MAP..INTR_STATUS).contains(&word) {
            let source = ((word - INTR_MAP) / 4) as usize;
            matrix_mut(cx).set_map(self.core, source, next);
            return;
        }
        if (INTR_STATUS..CLOCK_GATE).contains(&word) {
            // Read-only in the PAC's own access table, and derived here.
            return;
        }
        self.file.write(off, width, value, cx);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        if self.core == 0 {
            regs::INTERRUPT_CORE0.name(off)
        } else {
            regs::INTERRUPT_CORE1.name(off + VIEW_LEN)
        }
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.file.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        // The routing is the matrix's and rides in the snapshot as its own
        // field; only the two plain registers are this view's.
        self.file.save_state()
    }

    fn load_state(&mut self, bytes: &[u8]) {
        self.file.load_state(bytes);
    }
}

impl std::fmt::Debug for InterruptCoreView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InterruptCoreView")
            .field("core", &self.core)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    fn sandbox() -> Sandbox {
        Sandbox::new().with_matrix(Box::new(Esp32S3IntMatrix::new()))
    }

    #[test]
    fn the_source_count_is_the_pacs_ninety_nine() {
        assert_eq!(SOURCES, 99);
        assert_eq!(INTR_STATUS, 0x18c);
        assert_eq!(STATUS_WORDS, 4);
        assert_eq!(CLOCK_GATE, 0x19c);
        assert_eq!(regs::INTERRUPT_CORE0.name(0x188), Some("core_0_intr_map98"));
        assert_eq!(
            regs::INTERRUPT_CORE0.name(INTR_STATUS),
            Some("core_0_intr_status0")
        );
        assert_eq!(
            regs::INTERRUPT_CORE0.name(0x198),
            Some("core_0_intr_status3")
        );
        assert_eq!(regs::INTERRUPT_CORE0.name(CLOCK_GATE), Some("clock_gate"));
        assert_eq!(regs::INTERRUPT_CORE1.name(0x800), Some("core_1_intr_map0"));
        assert_eq!(
            regs::INTERRUPT_CORE1.name(0x800 + 0x188),
            Some("core_1_intr_map98")
        );
        // Every named source fits in the map.
        assert!(
            regs::INTERRUPT_SOURCES
                .iter()
                .all(|(n, _)| usize::from(*n) < SOURCES)
        );
    }

    #[test]
    fn a_mapped_source_asserts_its_cpu_interrupt_on_that_core_only() {
        let mut m = Esp32S3IntMatrix::new();
        let mut irq = IrqLines::new();
        // `RMT` (40) to CPU interrupt 23 on core 0; core 1 disabled the
        // esp-hal way.
        m.set_map(0, 40, 23);
        m.set_map(1, 40, ESP_HAL_DISABLED_CPU_INTERRUPT);
        assert_eq!(m.asserted(0, &irq), 0, "nothing is high yet");
        irq.set_level(40, true);
        assert_eq!(m.asserted(0, &irq), 1 << 23);
        assert_eq!(
            m.asserted(1, &irq),
            1 << 16,
            "esp-hal's 16 is a real line the hart simply never enables, not a hardware disable"
        );
        assert_eq!(
            m.cpu_interrupt(0, &irq),
            None,
            "the mask form, and nothing else (X43)"
        );
        irq.set_level(40, false);
        assert_eq!(m.asserted(0, &irq), 0);
    }

    #[test]
    fn a_map_value_past_the_cpu_interrupt_space_routes_nowhere_but_reads_back() {
        let mut m = Esp32S3IntMatrix::new();
        let mut irq = IrqLines::new();
        m.set_map(0, 96, 0xffff_ffff);
        irq.set_level(96, true);
        assert_eq!(m.asserted(0, &irq), 0);
        assert_eq!(
            m.map(0, 96),
            Some(0xffff_ffff),
            "the guest reads what it wrote"
        );
        assert_eq!(m.map(0, 99), None);
    }

    #[test]
    fn the_four_software_interrupts_are_sources_79_to_82() {
        assert_eq!(Esp32S3IntMatrix::software_source(0), Some(79));
        assert_eq!(Esp32S3IntMatrix::software_source(3), Some(82));
        assert_eq!(Esp32S3IntMatrix::software_source(4), None);
    }

    #[test]
    fn the_status_words_are_the_raw_source_levels_over_four_words() {
        let m = Esp32S3IntMatrix::new();
        let mut irq = IrqLines::new();
        irq.set_level(0, true);
        irq.set_level(31, true);
        irq.set_level(79, true);
        irq.set_level(96, true);
        irq.set_level(98, true);
        assert_eq!(m.status_word(&irq, 0), (1 << 0) | (1 << 31));
        assert_eq!(m.status_word(&irq, 1), 0);
        assert_eq!(m.status_word(&irq, 2), 1 << (79 - 64));
        assert_eq!(m.status_word(&irq, 3), (1 << 0) | (1 << 2));
        // Source 99 does not exist, so nothing above bit 2 can be set.
        irq.set_level(99, true);
        assert_eq!(m.status_word(&irq, 3) & !0b111, 0);
    }

    /// The acceptance test for the views: a store into core 0's map routes
    /// a source, raising it shows in the asserted mask, and core 1's view
    /// is the other half of the same state.
    #[test]
    fn the_two_views_write_one_matrix() {
        let mut sb = sandbox();
        let mut v0 = InterruptCoreView::core0();
        let mut v1 = InterruptCoreView::core1();
        assert_eq!(v0.reg_name(0), Some("core_0_intr_map0"));
        assert_eq!(v1.reg_name(0), Some("core_1_intr_map0"));
        assert_eq!(v1.reg_name(CLOCK_GATE), Some("clock_gate"));
        assert_eq!(sb.read(&mut v0, CLOCK_GATE), 1, "the PAC's reset");
        assert_eq!(
            sb.read(&mut v1, CLOCK_GATE),
            1,
            "seeded on the +0x800 half too"
        );

        // `setup_interrupts`: 16 over every entry, both cores.
        for s in 0..SOURCES as u32 {
            sb.write(&mut v0, INTR_MAP + 4 * s, ESP_HAL_DISABLED_CPU_INTERRUPT);
            sb.write(&mut v1, INTR_MAP + 4 * s, ESP_HAL_DISABLED_CPU_INTERRUPT);
        }
        // Bind `FROM_CPU_INTR0` (79) to CPU interrupt 19 on core 0 only.
        sb.write(&mut v0, INTR_MAP + 4 * 79, 19);
        assert_eq!(sb.read(&mut v0, INTR_MAP + 4 * 79), 19);
        assert_eq!(sb.read(&mut v1, INTR_MAP + 4 * 79), 16);
        sb.irq.set_level(79, true);
        assert_eq!(sb.matrix.asserted(0, &sb.irq), 1 << 19);
        assert_eq!(sb.matrix.asserted(1, &sb.irq), 1 << 16);
        // The status words are the same source levels from either view.
        assert_eq!(sb.read(&mut v0, INTR_STATUS + 8), 1 << (79 - 64));
        assert_eq!(sb.read(&mut v1, INTR_STATUS + 8), 1 << (79 - 64));
        sb.write(&mut v0, INTR_STATUS + 8, 0);
        assert_eq!(
            sb.read(&mut v0, INTR_STATUS + 8),
            1 << (79 - 64),
            "read-only"
        );
        assert_eq!(v0.reg_grade(INTR_MAP), Some(RegGrade::Modeled));
        assert_eq!(v0.reg_grade(CLOCK_GATE), Some(RegGrade::Documented));
    }

    #[test]
    fn the_matrix_round_trips_through_a_snapshot() {
        let mut m = Esp32S3IntMatrix::new();
        m.set_map(0, 40, 23);
        m.set_map(1, 98, 5);
        let bytes = m.save_state();
        let mut back = Esp32S3IntMatrix::new();
        back.load_state(&bytes);
        assert_eq!(back, m);
    }
}
