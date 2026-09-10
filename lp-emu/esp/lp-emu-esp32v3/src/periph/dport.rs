//! `DPORT` — the classic's system block, as a **view**.
//!
//! P3 answered this block with an accept-and-remember [`RegFile`] seeded from
//! the PAC, which was exactly the right probe and is exactly wrong as a
//! model: three of its register groups are not storage at all.
//!
//! | group | where the state really lives |
//! |---|---|
//! | `core_0_intr_map` / `core_1_intr_map` / `core_*_intr_status` | [`crate::intmatrix::Esp32V3IntMatrix`], through [`BusCx::matrix`] |
//! | `cpu_intr_from_cpu[0..4]` | [`BusCx::irq`] — they *are* interrupt sources |
//! | `appcpu_ctrl_{a..d}`, `pro/app_cache_ctrl{,1}` | [`AppCoreControl`] and [`ClassicCache`], which the machine also reads |
//!
//! Everything else keeps the accept block's behaviour, which P3 measured as
//! sufficient and which this file must not quietly change: `perip_clk_en`
//! (`+0xC0`), `perip_rst_en` (`+0xC4`), `peri_clk_en` (`+0x1C`),
//! `peri_rst_en` (`+0x20`) and `cpu_per_conf` (`+0x3C`) are
//! remember-what-was-written, with the PAC's reset values, because
//! `disable_peripherals` read-modify-writes them eleven times and the clock
//! tree reads `cpu_per_conf` back (report §3.4).
//!
//! # The two register groups whose *semantics* are the finding
//!
//! **The software interrupts are sources, not CPU interrupts.**
//! `cpu_intr_from_cpu[n]` at `+0xDC + 4n` has one bit (`cpu_intr`, bit 0). A
//! written 1 raises interrupt **source** `FROM_CPU_INTR{n}` = `24 + n`
//! (`esp32-0.40.2/src/lib.rs:259-266`) and a written 0 lowers it; the map
//! registers then decide which CPU interrupt that source reaches, exactly as
//! for a peripheral's line. Hardcoding a CPU-interrupt number here would work
//! for this firmware and break for the next one.
//!
//! **`appcpu_ctrl_c.appcpu_runstall` is one half of the stall key.** The
//! other halves are `RTC_CNTL.options0.sw_stall_appcpu_c0` and
//! `RTC_CNTL.sw_cpu_stall.sw_stall_appcpu_c1` (P5's block). This file owns
//! DPORT's half and nothing else: the machine composes them in
//! [`crate::machine::Machine::core_stalled`], so P4 and P5 never read each
//! other's files.
//!
//! # What M3 does when the guest tries to start core 1
//!
//! Nothing, loudly. When the guest clears `appcpu_runstall` **and** releases
//! `appcpu_resetting`, [`AppCoreControl`] records the cycle and the boot
//! address it was given and the machine reports it once at `info`. Core 1
//! stays stalled (Q5), and the firmware takes its documented fallback —
//! `[INIT] APP core unavailable; RMT ISR on PRO core (single-core semantics)`
//! (`lp-fw/fw-esp32v3/src/main.rs:838-845`). M4 is what makes the attempt
//! succeed.
//!
//! # `immu_page_mode` / `dmmu_page_mode` are **not** the flash MMU
//!
//! The phase file expected these to be the flash MMU's page size, with a stop
//! if the guest chose anything but 64 KiB. They are not: the PAC's fields are
//! `internal_sram_immu_ena` (bit 0) and `immu_page_mode` (bits 2:1), and they
//! configure the sixteen-entry **internal-SRAM** MMU whose tables are
//! `DPORT.immu_table0` / `dmmu_table0` at `+0x504`/`+0x544`. The flash MMU's
//! page size lives in `*_cache_ctrl1` bits 10:9 and is read there
//! ([`crate::cache`]). So these two are accept-and-remember, and a stop
//! against them would fire on something unrelated to what it claimed. Named
//! here as a correction to the phase file rather than obeyed.

use std::sync::{Arc, Mutex};

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::cache::{CacheHandle, ClassicCache};
use crate::intmatrix::{Esp32V3IntMatrix, SOURCES};
use crate::regs;

pub use super::accept::DPORT_LEN;

// ---- the offsets this view gives behaviour to -----------------------------
// Every one is `regs::DPORT`'s own, and `tests` asserts the names.

/// `appcpu_ctrl_a.appcpu_resetting`, bit 0. PAC reset **1**: core 1 is held
/// in reset out of the box.
pub const APPCPU_CTRL_A: u32 = 0x02c;
/// `appcpu_ctrl_b.appcpu_clkgate_en`, bit 0.
pub const APPCPU_CTRL_B: u32 = 0x030;
/// `appcpu_ctrl_c.appcpu_runstall`, bit 0 — DPORT's half of the stall key.
pub const APPCPU_CTRL_C: u32 = 0x034;
/// `appcpu_ctrl_d.appcpu_boot_addr`, the whole word.
pub const APPCPU_CTRL_D: u32 = 0x038;

/// `pro_cache_ctrl`.
pub const PRO_CACHE_CTRL: u32 = 0x040;
/// `pro_cache_ctrl1`.
pub const PRO_CACHE_CTRL1: u32 = 0x044;
/// `app_cache_ctrl`.
pub const APP_CACHE_CTRL: u32 = 0x058;
/// `app_cache_ctrl1`.
pub const APP_CACHE_CTRL1: u32 = 0x05c;

/// `cpu_intr_from_cpu0`; the four are one word apart.
pub const CPU_INTR_FROM_CPU0: u32 = 0x0dc;
/// One past `cpu_intr_from_cpu3`.
pub const CPU_INTR_FROM_CPU_END: u32 = CPU_INTR_FROM_CPU0 + 4 * 4;

/// `core_0_intr_status0`; three words per core, read-only.
pub const CORE_0_INTR_STATUS: u32 = 0x0ec;
/// `core_1_intr_status0`.
pub const CORE_1_INTR_STATUS: u32 = 0x0f8;
/// One past `core_1_intr_status2`.
pub const CORE_INTR_STATUS_END: u32 = CORE_1_INTR_STATUS + 3 * 4;

/// `core_0_intr_map0`.
pub const CORE_0_INTR_MAP: u32 = 0x104;
/// `core_1_intr_map0` — which is also one past `core_0_intr_map68`.
pub const CORE_1_INTR_MAP: u32 = CORE_0_INTR_MAP + SOURCES as u32 * 4;
/// One past `core_1_intr_map68`.
pub const CORE_INTR_MAP_END: u32 = CORE_1_INTR_MAP + SOURCES as u32 * 4;

/// The `appcpu_ctrl_*` state, shared with the machine.
///
/// Held behind a handle rather than read out of the register file because
/// the *machine* is what decides whether core 1 runs, and it cannot reach
/// inside a peripheral mid-slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppCoreControl {
    /// `appcpu_ctrl_a.appcpu_resetting`. PAC reset 1.
    pub resetting: bool,
    /// `appcpu_ctrl_b.appcpu_clkgate_en`. PAC reset 0 — the core's clock is
    /// gated off, which esp-hal's `is_running` checks **first**.
    pub clkgate_en: bool,
    /// `appcpu_ctrl_c.appcpu_runstall`. PAC reset 0 — the runstall alone
    /// does not hold core 1 out of reset; `appcpu_resetting` and the gated
    /// clock do, and so does RTC_CNTL's key.
    pub runstall: bool,
    /// `appcpu_ctrl_d.appcpu_boot_addr`, remembered for M4.
    pub boot_addr: u32,
    /// The first cycle at which the guest had both released the reset and
    /// cleared the runstall, with the boot address it had written by then.
    pub start_attempt: Option<(Cycles, u32)>,
    /// Whether the machine has already reported that attempt.
    pub reported: bool,
}

/// The handle the DPORT view and the machine share.
pub type AppCoreHandle = Arc<Mutex<AppCoreControl>>;

impl Default for AppCoreControl {
    fn default() -> Self {
        Self::new()
    }
}

impl AppCoreControl {
    /// The PAC's reset state: `appcpu_ctrl_a = 1` (`regs::DPORT`'s `resets`
    /// lists `(0x02c, 0x00000001)`), everything else 0.
    pub const fn new() -> Self {
        Self {
            resetting: true,
            clkgate_en: false,
            runstall: false,
            boot_addr: 0,
            start_attempt: None,
            reported: false,
        }
    }

    pub fn handle() -> AppCoreHandle {
        Arc::new(Mutex::new(Self::new()))
    }

    /// DPORT's half of "is core 1 held?".
    ///
    /// Three bits, and esp-hal's `is_running` reads the first two **before**
    /// it looks at RTC_CNTL's key at all
    /// (`third_party/esp-hal/src/soc/esp32/cpu_control.rs:41-55`):
    ///
    /// ```text
    /// DPORT_APPCPU_CLKGATE_EN in APPCPU_CTRL_B bit 0 -> needs to be 1 to even be enabled
    /// DPORT_APPCPU_RUNSTALL   in APPCPU_CTRL_C bit 0 -> needs to be 0 to not stall
    /// ```
    ///
    /// `appcpu_resetting` (`APPCPU_CTRL_A` bit 0) is the third and is the
    /// PAC's reset value — the part comes out of reset holding core 1 — and
    /// `start_app_core` clears it as part of the same sequence.
    ///
    /// RTC_CNTL's two halves are P5's, and
    /// [`crate::machine::Machine::core_stalled`] ORs all of it together.
    pub const fn holds_core1(&self) -> bool {
        self.resetting || self.runstall || !self.clkgate_en
    }

    /// Note a write that may have released the core, at `now`.
    fn note(&mut self, now: Cycles) {
        if !self.holds_core1() && self.start_attempt.is_none() {
            self.start_attempt = Some((now, self.boot_addr));
        }
    }
}

/// `DPORT` at `0x3FF0_0000` — a view over the matrix, the cache and the
/// app-core controls, with a [`RegFile`] behind everything else.
pub struct DportView {
    file: RegFile,
    cache: CacheHandle,
    appcpu: AppCoreHandle,
}

impl DportView {
    /// The block, sharing `cache` and `appcpu` with the machine.
    ///
    /// The register file is P3's, unchanged: `regs::DPORT`'s names and all
    /// forty-nine of the PAC's non-zero reset values, with the PAC's
    /// read-only access table. The registers this view intercepts read from
    /// their real home instead, but the file still holds their reset values
    /// so a reader of `stored()` sees the part's own numbers.
    pub fn new(cache: CacheHandle, appcpu: AppCoreHandle) -> Self {
        Self {
            file: super::accept::dport(),
            cache,
            appcpu,
        }
    }

    /// Which core a cache-control offset belongs to, if any.
    const fn cache_core(off: u32) -> Option<(usize, bool)> {
        match off {
            PRO_CACHE_CTRL => Some((0, false)),
            PRO_CACHE_CTRL1 => Some((0, true)),
            APP_CACHE_CTRL => Some((1, false)),
            APP_CACHE_CTRL1 => Some((1, true)),
            _ => None,
        }
    }

    /// The word this view answers `off` with, ignoring the byte lane.
    fn read_word(&self, off: u32, cx: &BusCx<'_>) -> u32 {
        if let Some((core, ctrl1)) = Self::cache_core(off) {
            let c = self.cache.lock().expect("cache poisoned");
            return if ctrl1 { c.ctrl1(core) } else { c.ctrl(core) };
        }
        if (CPU_INTR_FROM_CPU0..CPU_INTR_FROM_CPU_END).contains(&off) {
            let n = ((off - CPU_INTR_FROM_CPU0) / 4) as u16;
            let source = Esp32V3IntMatrix::software_source(n).expect("n < 4");
            return u32::from(cx.irq.level(source));
        }
        if (CORE_0_INTR_STATUS..CORE_INTR_STATUS_END).contains(&off) {
            let k = ((off - CORE_0_INTR_STATUS) / 4) % 3;
            return matrix_ref(cx).status_word(cx.irq, k);
        }
        if (CORE_0_INTR_MAP..CORE_INTR_MAP_END).contains(&off) {
            let (core, source) = map_index(off);
            return matrix_ref(cx).map(core, source).unwrap_or(0);
        }
        match off {
            APPCPU_CTRL_A => u32::from(self.appcpu.lock().expect("appcpu poisoned").resetting),
            APPCPU_CTRL_B => u32::from(self.appcpu.lock().expect("appcpu poisoned").clkgate_en),
            APPCPU_CTRL_C => u32::from(self.appcpu.lock().expect("appcpu poisoned").runstall),
            APPCPU_CTRL_D => self.appcpu.lock().expect("appcpu poisoned").boot_addr,
            _ => self.file.effective(off),
        }
    }
}

/// `core_N_intr_map` offset → `(core, source)`.
fn map_index(off: u32) -> (usize, usize) {
    let word = (off - CORE_0_INTR_MAP) / 4;
    ((word as usize) / SOURCES, (word as usize) % SOURCES)
}

fn matrix_ref<'a>(cx: &'a BusCx<'_>) -> &'a Esp32V3IntMatrix {
    cx.matrix
        .as_any()
        .downcast_ref::<Esp32V3IntMatrix>()
        .expect("the DPORT view needs Esp32V3IntMatrix on the bus")
}

fn matrix_mut<'a>(cx: &'a mut BusCx<'_>) -> &'a mut Esp32V3IntMatrix {
    cx.matrix
        .as_any_mut()
        .downcast_mut::<Esp32V3IntMatrix>()
        .expect("the DPORT view needs Esp32V3IntMatrix on the bus")
}

impl Peripheral for DportView {
    fn name(&self) -> &'static str {
        "DPORT"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        lane_of(self.read_word(off & !3, cx), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        let old = self.read_word(word, cx);
        let next = merge_lane(old, off, width, value);

        if let Some((core, ctrl1)) = Self::cache_core(word) {
            let was_off = {
                let mut c = self.cache.lock().expect("cache poisoned");
                let was_off = !c.enabled(core);
                if ctrl1 {
                    c.write_ctrl1(core, next);
                } else {
                    c.write_ctrl(core, next, cx.now, cx.pc);
                }
                was_off
            };
            // A change to what the flash window *means*, or to whether it may
            // be read at all, has to reach the machine before the guest runs
            // another instruction: the cache-off watch is armed and disarmed
            // between slices, and the ROM's `Cache_Read_Enable` is followed
            // by a fetch through the window a handful of instructions later.
            let now_off = !self.cache.lock().expect("cache poisoned").enabled(core);
            if was_off != now_off {
                cx.yield_to_machine();
            }
            return;
        }

        if (CPU_INTR_FROM_CPU0..CPU_INTR_FROM_CPU_END).contains(&word) {
            let n = ((word - CPU_INTR_FROM_CPU0) / 4) as u16;
            let source = Esp32V3IntMatrix::software_source(n).expect("n < 4");
            cx.irq.set_level(source, next & 1 != 0);
            return;
        }

        if (CORE_0_INTR_STATUS..CORE_INTR_STATUS_END).contains(&word) {
            // Read-only in the PAC's own access table, and derived here.
            return;
        }

        if (CORE_0_INTR_MAP..CORE_INTR_MAP_END).contains(&word) {
            let (core, source) = map_index(word);
            matrix_mut(cx).set_map(core, source, next);
            return;
        }

        match word {
            APPCPU_CTRL_A | APPCPU_CTRL_B | APPCPU_CTRL_C | APPCPU_CTRL_D => {
                let mut a = self.appcpu.lock().expect("appcpu poisoned");
                match word {
                    APPCPU_CTRL_A => a.resetting = next & 1 != 0,
                    APPCPU_CTRL_B => a.clkgate_en = next & 1 != 0,
                    APPCPU_CTRL_C => a.runstall = next & 1 != 0,
                    _ => a.boot_addr = next,
                }
                a.note(cx.now);
                if a.start_attempt.is_some() && !a.reported {
                    // The machine reports it at the slice boundary, where it
                    // can also decide what to do about it.
                    cx.yield_to_machine();
                }
            }
            _ => self.file.write(off, width, value, cx),
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::DPORT.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.file.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        // The register file, then the two states this view is the guest's
        // door onto. The matrix is the *bus's* and the snapshot carries it
        // there (`crate::snapshot`), exactly as the C6's does.
        let mut out = self.file.save_state();
        let appcpu = *self.appcpu.lock().expect("appcpu poisoned");
        out.push(u8::from(appcpu.resetting));
        out.push(u8::from(appcpu.clkgate_en));
        out.push(u8::from(appcpu.runstall));
        out.push(u8::from(appcpu.reported));
        out.extend_from_slice(&appcpu.boot_addr.to_le_bytes());
        match appcpu.start_attempt {
            Some((at, entry)) => {
                out.push(1);
                out.extend_from_slice(&at.to_le_bytes());
                out.extend_from_slice(&entry.to_le_bytes());
            }
            None => out.extend_from_slice(&[0; 13]),
        }
        out.extend_from_slice(&self.cache.lock().expect("cache poisoned").save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let file_len = self.file.save_state().len();
        const APPCPU_LEN: usize = 4 + 4 + 13;
        if bytes.len() < file_len + APPCPU_LEN {
            log::warn!(
                "DportView::load_state: {} bytes is too short; ignored",
                bytes.len()
            );
            return;
        }
        self.file.load_state(&bytes[..file_len]);
        let a = &bytes[file_len..file_len + APPCPU_LEN];
        {
            let mut appcpu = self.appcpu.lock().expect("appcpu poisoned");
            appcpu.resetting = a[0] != 0;
            appcpu.clkgate_en = a[1] != 0;
            appcpu.runstall = a[2] != 0;
            appcpu.reported = a[3] != 0;
            appcpu.boot_addr = u32::from_le_bytes(a[4..8].try_into().expect("4 bytes"));
            appcpu.start_attempt = (a[8] != 0).then(|| {
                (
                    u64::from_le_bytes(a[9..17].try_into().expect("8 bytes")),
                    u32::from_le_bytes(a[17..21].try_into().expect("4 bytes")),
                )
            });
        }
        self.cache
            .lock()
            .expect("cache poisoned")
            .load_state(&bytes[file_len + APPCPU_LEN..]);
    }
}

/// `DPORT`'s aperture. See [`super::accept::DPORT_LEN`].
pub const LEN: u32 = DPORT_LEN;

/// A fresh view with its own handles — what a test uses when it does not
/// need to look at the shared state from outside.
pub fn dport() -> DportView {
    DportView::new(ClassicCache::handle(), AppCoreControl::handle())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CACHE_ENABLE, CACHE_FLUSH_DONE, CACHE_FLUSH_ENA};
    use lp_emu_esp_common::Sandbox;

    fn sandbox() -> Sandbox {
        Sandbox::new().with_matrix(Box::new(Esp32V3IntMatrix::new()))
    }

    #[test]
    fn the_offsets_are_the_generated_tables() {
        assert_eq!(regs::DPORT.name(APPCPU_CTRL_A), Some("appcpu_ctrl_a"));
        assert_eq!(regs::DPORT.name(APPCPU_CTRL_B), Some("appcpu_ctrl_b"));
        assert_eq!(regs::DPORT.name(APPCPU_CTRL_C), Some("appcpu_ctrl_c"));
        assert_eq!(regs::DPORT.name(APPCPU_CTRL_D), Some("appcpu_ctrl_d"));
        assert_eq!(regs::DPORT.name(PRO_CACHE_CTRL), Some("pro_cache_ctrl"));
        assert_eq!(regs::DPORT.name(PRO_CACHE_CTRL1), Some("pro_cache_ctrl1"));
        assert_eq!(regs::DPORT.name(APP_CACHE_CTRL), Some("app_cache_ctrl"));
        assert_eq!(regs::DPORT.name(APP_CACHE_CTRL1), Some("app_cache_ctrl1"));
        assert_eq!(
            regs::DPORT.name(CPU_INTR_FROM_CPU0),
            Some("cpu_intr_from_cpu0")
        );
        assert_eq!(
            regs::DPORT.name(CPU_INTR_FROM_CPU_END - 4),
            Some("cpu_intr_from_cpu3")
        );
        assert_eq!(
            regs::DPORT.name(CORE_0_INTR_STATUS),
            Some("core_0_intr_status0")
        );
        assert_eq!(
            regs::DPORT.name(CORE_1_INTR_STATUS),
            Some("core_1_intr_status0")
        );
        assert_eq!(regs::DPORT.name(CORE_0_INTR_MAP), Some("core_0_intr_map0"));
        assert_eq!(regs::DPORT.name(CORE_1_INTR_MAP), Some("core_1_intr_map0"));
        assert_eq!(
            regs::DPORT.name(CORE_INTR_MAP_END),
            Some("ahblite_mpu_table_uart"),
            "the map ends where the next register begins"
        );
    }

    /// **The acceptance test for the matrix**: a store into
    /// `core_0_intr_map[9]` routes source 9, and raising it shows up in the
    /// mask the machine hands the hart.
    #[test]
    fn a_source_raised_through_core_0_intr_map_reaches_the_asserted_mask() {
        let mut sb = sandbox();
        let mut d = dport();

        // esp-hal's `setup_interrupts` writes 16 over every entry first.
        for source in 0..SOURCES as u32 {
            sb.write(&mut d, CORE_0_INTR_MAP + source * 4, 16);
        }
        // Then `enable(Interrupt::UART0, Priority::Priority1)` maps source 9
        // onto a level-1 CPU interrupt; 23 is one of the classic's level-3
        // lines and any number below 32 exercises the same path.
        sb.write(&mut d, CORE_0_INTR_MAP + 9 * 4, 23);
        assert_eq!(sb.read(&mut d, CORE_0_INTR_MAP + 9 * 4), 23);

        let asserted = |sb: &Sandbox| sb.matrix.asserted(0, &sb.irq);
        assert_eq!(asserted(&sb), 0);
        sb.irq.set_level(9, true);
        assert_eq!(asserted(&sb), 1 << 23);
        // The status word says so from the guest's side, and it is read-only.
        assert_eq!(sb.read(&mut d, CORE_0_INTR_STATUS), 1 << 9);
        sb.write(&mut d, CORE_0_INTR_STATUS, 0);
        assert_eq!(sb.read(&mut d, CORE_0_INTR_STATUS), 1 << 9);
    }

    /// swi2 is on M3's critical path: the io task's `InterruptExecutor` runs
    /// at Priority2 behind it.
    #[test]
    fn a_software_interrupt_is_a_source_the_map_routes() {
        let mut sb = sandbox();
        let mut d = dport();
        // Route FROM_CPU_INTR2 (source 26) to CPU interrupt 19, a level-2
        // line on this chip.
        sb.write(&mut d, CORE_0_INTR_MAP + 26 * 4, 19);
        assert_eq!(sb.matrix.asserted(0, &sb.irq), 0);

        sb.write(&mut d, CPU_INTR_FROM_CPU0 + 2 * 4, 1);
        assert!(sb.irq.level(26));
        assert_eq!(sb.matrix.asserted(0, &sb.irq), 1 << 19);
        assert_eq!(sb.read(&mut d, CPU_INTR_FROM_CPU0 + 2 * 4), 1);

        sb.write(&mut d, CPU_INTR_FROM_CPU0 + 2 * 4, 0);
        assert!(!sb.irq.level(26));
        assert_eq!(sb.matrix.asserted(0, &sb.irq), 0);
    }

    #[test]
    fn the_cache_control_words_are_the_caches_own_state() {
        let cache = ClassicCache::handle();
        let mut d = DportView::new(cache.clone(), AppCoreControl::handle());
        let mut sb = sandbox();
        sb.now = 1_284_610;
        sb.pc = 0x4008_1c04;

        // `Cache_Read_Enable`: read-modify-write bit 3.
        let ctrl = sb.read(&mut d, PRO_CACHE_CTRL);
        sb.write(&mut d, PRO_CACHE_CTRL, ctrl | CACHE_ENABLE);
        assert!(cache.lock().unwrap().enabled(0));
        assert!(sb.yield_now, "a change to the cache reaches the machine");

        // `Cache_Read_Disable`, and the cycle and pc are kept for D4.
        let ctrl = sb.read(&mut d, PRO_CACHE_CTRL);
        sb.write(&mut d, PRO_CACHE_CTRL, ctrl & !CACHE_ENABLE);
        assert!(!cache.lock().unwrap().enabled(0));
        assert_eq!(
            cache.lock().unwrap().disabled(0),
            (1_284_610, Some(0x4008_1c04))
        );

        // `Cache_Flush`'s handshake, from the guest's side.
        let ctrl = sb.read(&mut d, PRO_CACHE_CTRL);
        sb.write(&mut d, PRO_CACHE_CTRL, ctrl | CACHE_FLUSH_ENA);
        assert_ne!(sb.read(&mut d, PRO_CACHE_CTRL) & CACHE_FLUSH_DONE, 0);

        // The APP core's words are its own.
        assert!(!cache.lock().unwrap().enabled(1));
        sb.write(&mut d, APP_CACHE_CTRL, CACHE_ENABLE);
        assert!(cache.lock().unwrap().enabled(1));

        // `ctrl1` is remembered, and the page mode is read out of it.
        sb.write(&mut d, PRO_CACHE_CTRL1, 0x0000_0aff);
        assert_eq!(sb.read(&mut d, PRO_CACHE_CTRL1), 0x0000_0aff);
        assert_eq!(cache.lock().unwrap().page_mode(0), 1);
    }

    #[test]
    fn releasing_core_one_is_recorded_and_refused() {
        let appcpu = AppCoreControl::handle();
        let mut d = DportView::new(ClassicCache::handle(), appcpu.clone());
        let mut sb = sandbox();

        assert_eq!(
            sb.read(&mut d, APPCPU_CTRL_A),
            1,
            "the PAC holds it in reset"
        );
        assert!(appcpu.lock().unwrap().holds_core1());

        sb.now = 4_000_000;
        sb.write(&mut d, APPCPU_CTRL_D, 0x4008_0844);
        sb.write(&mut d, APPCPU_CTRL_B, 1);
        sb.write(&mut d, APPCPU_CTRL_C, 1);
        assert!(appcpu.lock().unwrap().holds_core1());
        assert!(appcpu.lock().unwrap().start_attempt.is_none());

        // All three released: DPORT no longer holds it, and the attempt is
        // recorded with the cycle and the entry point it was given. The
        // clock gate is already on — esp-hal's `is_running` checks it first
        // and `start_app_core` sets it first.
        sb.write(&mut d, APPCPU_CTRL_C, 0);
        assert!(
            appcpu.lock().unwrap().holds_core1(),
            "the reset is still asserted"
        );
        sb.now = 4_000_100;
        sb.write(&mut d, APPCPU_CTRL_A, 0);
        assert!(!appcpu.lock().unwrap().holds_core1());
        // Gating the clock back off holds it again, which is the bit esp-hal
        // reads before anything else.
        {
            let mut a = appcpu.lock().unwrap();
            a.clkgate_en = false;
            assert!(a.holds_core1());
            a.clkgate_en = true;
        }
        assert_eq!(
            appcpu.lock().unwrap().start_attempt,
            Some((4_000_100, 0x4008_0844))
        );
        assert!(sb.yield_now, "the machine gets to report it");

        // The register file still reads back what was written.
        assert_eq!(sb.read(&mut d, APPCPU_CTRL_D), 0x4008_0844);
        assert_eq!(sb.read(&mut d, APPCPU_CTRL_B), 1);
    }

    /// P3 measured these five as remember-what-was-written, and the view must
    /// not have changed them (report §3.4).
    #[test]
    fn the_clock_gates_and_the_cpu_clock_select_are_still_accept_and_remember() {
        let mut sb = sandbox();
        let mut d = dport();
        // `perip_clk_en`'s PAC reset, which `disable_peripherals` reads,
        // masks and writes back eleven times.
        assert_eq!(sb.read(&mut d, 0x0c0), 0xf9c1_e06f);
        sb.write(&mut d, 0x0c0, 0xf900_600f);
        assert_eq!(sb.read(&mut d, 0x0c0), 0xf900_600f);

        for off in [0x01c, 0x020, 0x0c4] {
            sb.write(&mut d, off, 0x0100_0000);
            assert_eq!(sb.read(&mut d, off), 0x0100_0000);
            sb.write(&mut d, off, 0);
            assert_eq!(sb.read(&mut d, off), 0);
        }

        // `cpu_per_conf` 0 → 2 is the XTAL → PLL switch at cycle 150,048.
        sb.write(&mut d, 0x03c, 0);
        assert_eq!(sb.read(&mut d, 0x03c), 0);
        sb.write(&mut d, 0x03c, 2);
        assert_eq!(sb.read(&mut d, 0x03c), 2);

        // The internal-SRAM MMU's page modes: remembered, not obeyed. See
        // the module docs.
        sb.write(&mut d, 0x080, 0b101);
        assert_eq!(sb.read(&mut d, 0x080), 0b101);
        sb.write(&mut d, 0x084, 0b011);
        assert_eq!(sb.read(&mut d, 0x084), 0b011);
    }

    #[test]
    fn a_byte_lane_write_reaches_the_right_register() {
        let mut sb = sandbox();
        let mut d = dport();
        // The map registers hold a small number, so the low lane is the
        // interesting one; `merge_lane` must keep the rest of the word.
        sb.write(&mut d, CORE_0_INTR_MAP + 9 * 4, 0x0000_0017);
        let mut cx = sb.cx();
        d.write(CORE_0_INTR_MAP + 9 * 4, Width::Byte, 0x05, &mut cx);
        drop(cx);
        assert_eq!(sb.read(&mut d, CORE_0_INTR_MAP + 9 * 4), 0x0000_0005);
    }

    #[test]
    fn the_view_round_trips_through_a_snapshot() {
        let cache = ClassicCache::handle();
        let appcpu = AppCoreControl::handle();
        let mut d = DportView::new(cache.clone(), appcpu.clone());
        let mut sb = sandbox();
        sb.now = 99;
        sb.pc = 0x4000_9a97;
        sb.write(&mut d, 0x0c0, 0xf900_600f);
        sb.write(&mut d, PRO_CACHE_CTRL, CACHE_ENABLE);
        sb.write(&mut d, APPCPU_CTRL_D, 0x4008_0844);
        cache.lock().unwrap().mmu.set_entry(0, 77, 0x31);

        let bytes = d.save_state();
        let mut back = DportView::new(ClassicCache::handle(), AppCoreControl::handle());
        back.load_state(&bytes);
        assert_eq!(sb.read(&mut back, 0x0c0), 0xf900_600f);
        assert_eq!(sb.read(&mut back, PRO_CACHE_CTRL), CACHE_ENABLE);
        assert_eq!(sb.read(&mut back, APPCPU_CTRL_D), 0x4008_0844);
        assert_eq!(back.cache.lock().unwrap().mmu.entry(0, 77), 0x31);
    }
}
