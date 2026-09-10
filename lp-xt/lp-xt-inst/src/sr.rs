// Encoding data for this module is **assembler-derived**: the RSR/WSR/XSR/RUR/
// WUR field layouts and every register number below were read out of
// `xtensa-esp32-elf-as` / `xtensa-esp32s3-elf-as` + `-objdump` output for
// one-instruction `.S` files (see `lp-xt/fixtures/fp/README.md`). Tool *output*
// is fact; no binutils, GCC, or QEMU source was read or adapted. See
//   docs/adr/2026-07-29-license-provenance-discipline.md
//
//! Special- and user-register access over the **whole LX6/LX7 firmware SR/UR
//! space**.
//!
//! Every register number and every legal `rsr`/`wsr`/`xsr`/`rur`/`wur`
//! combination below was read back from both the LX6 (`xtensa-esp32-elf-as`)
//! and the LX7 (`xtensa-esp32s3-elf-as`) assembler; the two agree on every
//! register in this table except the four marked **LX6 only**, which the LX7
//! assembler rejects outright.
//!
//! Three deliberate properties:
//!
//! - **`from_num` is partial.** A register number outside this table is
//!   [`crate::DecodeError::Unsupported`], never a numeric fallback variant. An
//!   SR the machine has no behaviour for must not decode into something that
//!   looks executable — that is the silent-wrong-answer failure this table
//!   exists to prevent.
//! - **The access asymmetries are modelled, not smoothed over.** SR 226 reads
//!   as `INTERRUPT` and writes as `INTSET`; SR 227 (`INTCLEAR`) is write-only;
//!   SR 235 (`PRID`) is read-only. The assembler refuses the missing forms and
//!   so does [`SpecialReg::allows`].
//! - **User registers are a different opcode space.** `THREADPTR`, `FCR`,
//!   `FSR` and the LX6 double-precision registers live under `rur`/`wur`, not
//!   `rsr`/`wsr` — `esp-rtos`'s task switch is the code that proves it
//!   (`rur.threadptr` / `wur.threadptr`).
//!
//! This crate decodes, encodes and disassembles these accesses. It holds no
//! register state and no behaviour: what a `wsr.ps` *does* is the machine-mode
//! hart's problem, not this crate's.

/// A special register reachable by `RSR`/`WSR`/`XSR`.
///
/// Numbering is assembler-derived (see this module's provenance header).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpecialReg {
    /// `LBEG` — loop begin address, SR 0.
    Lbeg,
    /// `LEND` — loop end address, SR 1.
    Lend,
    /// `LCOUNT` — remaining loop iterations, SR 2.
    Lcount,
    /// `SAR` — shift amount register, SR 3.
    Sar,
    /// `BR` — the 16-bit Boolean register file, SR 4.
    Br,
    /// `LITBASE` — literal base for the extended-L32R option, SR 5.
    Litbase,
    /// `SCOMPARE1` — the compare value `s32c1i` tests against, SR 12.
    Scompare1,
    /// `ACCLO` — MAC16 accumulator, low 32 bits, SR 16.
    Acclo,
    /// `ACCHI` — MAC16 accumulator, high 8 bits, SR 17.
    Acchi,
    /// `M0` — MAC16 operand register 0, SR 32.
    M0,
    /// `M1` — MAC16 operand register 1, SR 33.
    M1,
    /// `M2` — MAC16 operand register 2, SR 34.
    M2,
    /// `M3` — MAC16 operand register 3, SR 35.
    M3,
    /// `WINDOWBASE` — the current register window, SR 72.
    WindowBase,
    /// `WINDOWSTART` — the live-window bitmap, SR 73.
    WindowStart,
    /// `IBREAKENABLE` — instruction-breakpoint enable bits, SR 96.
    IbreakEnable,
    /// `MEMCTL` — cache/memory control, SR 97. Present on **both** LX6 and LX7.
    Memctl,
    /// `ATOMCTL` — `s32c1i` bus behaviour per memory region, SR 99.
    Atomctl,
    /// `DDR` — debug data register, SR 104.
    Ddr,
    /// `IBREAKA0` — instruction breakpoint address 0, SR 128.
    Ibreaka0,
    /// `IBREAKA1` — instruction breakpoint address 1, SR 129.
    Ibreaka1,
    /// `DBREAKA0` — data breakpoint address 0, SR 144.
    Dbreaka0,
    /// `DBREAKA1` — data breakpoint address 1, SR 145.
    Dbreaka1,
    /// `DBREAKC0` — data breakpoint control 0, SR 160.
    Dbreakc0,
    /// `DBREAKC1` — data breakpoint control 1, SR 161.
    Dbreakc1,
    /// `EPC1` — exception PC for level 1, SR 177.
    Epc1,
    /// `EPC2` — exception PC for level 2, SR 178.
    Epc2,
    /// `EPC3` — exception PC for level 3, SR 179.
    Epc3,
    /// `EPC4` — exception PC for level 4, SR 180.
    Epc4,
    /// `EPC5` — exception PC for level 5, SR 181.
    Epc5,
    /// `EPC6` — exception PC for level 6 (debug), SR 182.
    Epc6,
    /// `EPC7` — exception PC for level 7 (NMI), SR 183.
    Epc7,
    /// `DEPC` — double-exception PC, SR 192.
    Depc,
    /// `EPS2` — saved PS for level 2, SR 194.
    Eps2,
    /// `EPS3` — saved PS for level 3, SR 195.
    Eps3,
    /// `EPS4` — saved PS for level 4, SR 196.
    Eps4,
    /// `EPS5` — saved PS for level 5, SR 197.
    Eps5,
    /// `EPS6` — saved PS for level 6, SR 198.
    Eps6,
    /// `EPS7` — saved PS for level 7, SR 199.
    Eps7,
    /// `EXCSAVE1` — scratch for the level-1 handler, SR 209.
    Excsave1,
    /// `EXCSAVE2` — scratch for the level-2 handler, SR 210.
    Excsave2,
    /// `EXCSAVE3` — scratch for the level-3 handler, SR 211.
    Excsave3,
    /// `EXCSAVE4` — scratch for the level-4 handler, SR 212.
    Excsave4,
    /// `EXCSAVE5` — scratch for the level-5 handler, SR 213.
    Excsave5,
    /// `EXCSAVE6` — scratch for the level-6 handler, SR 214.
    Excsave6,
    /// `EXCSAVE7` — scratch for the level-7 handler, SR 215.
    Excsave7,
    /// `CPENABLE` — the per-coprocessor enable mask, SR 224. Bit 0 is the FPU.
    Cpenable,
    /// SR 226 — reads as `INTERRUPT` (pending interrupts), writes as `INTSET`.
    /// `xsr` is **not** a legal access; see [`SpecialReg::allows`].
    Interrupt,
    /// `INTCLEAR` — SR 227, **write-only**: writing a 1 clears that interrupt.
    Intclear,
    /// `INTENABLE` — per-interrupt enable mask, SR 228. A CPU register, not a
    /// peripheral one: the interrupt matrix cannot see it.
    Intenable,
    /// `PS` — the processor state register, SR 230.
    Ps,
    /// `VECBASE` — the relocatable vector table base, SR 231.
    Vecbase,
    /// `EXCCAUSE` — the level-1 exception cause, SR 232.
    Exccause,
    /// `DEBUGCAUSE` — why the debug exception fired, SR 233.
    Debugcause,
    /// `CCOUNT` — the free-running cycle counter, SR 234.
    Ccount,
    /// `PRID` — the per-hart processor id, SR 235. **Read-only.**
    Prid,
    /// `ICOUNT` — the instruction counter, SR 236.
    Icount,
    /// `ICOUNTLEVEL` — the interrupt level `ICOUNT` counts at, SR 237.
    Icountlevel,
    /// `EXCVADDR` — the faulting virtual address, SR 238.
    Excvaddr,
    /// `CCOMPARE0` — timer 0 compare value, SR 240.
    Ccompare0,
    /// `CCOMPARE1` — timer 1 compare value, SR 241.
    Ccompare1,
    /// `CCOMPARE2` — timer 2 compare value, SR 242.
    Ccompare2,
    /// `MISC0` — scratch register 0, SR 244.
    Misc0,
    /// `MISC1` — scratch register 1, SR 245.
    Misc1,
    /// `MISC2` — scratch register 2, SR 246.
    Misc2,
    /// `MISC3` — scratch register 3, SR 247.
    Misc3,
}

impl SpecialReg {
    /// Every modelled special register, in ascending [`SpecialReg::num`] order.
    ///
    /// `tests/roundtrip.rs` checks this list against [`SpecialReg::from_num`]
    /// in both directions, so a variant added to the enum but not to this list
    /// (or vice versa) fails the build's tests rather than going unnoticed.
    pub const ALL: [SpecialReg; 66] = [
        SpecialReg::Lbeg,
        SpecialReg::Lend,
        SpecialReg::Lcount,
        SpecialReg::Sar,
        SpecialReg::Br,
        SpecialReg::Litbase,
        SpecialReg::Scompare1,
        SpecialReg::Acclo,
        SpecialReg::Acchi,
        SpecialReg::M0,
        SpecialReg::M1,
        SpecialReg::M2,
        SpecialReg::M3,
        SpecialReg::WindowBase,
        SpecialReg::WindowStart,
        SpecialReg::IbreakEnable,
        SpecialReg::Memctl,
        SpecialReg::Atomctl,
        SpecialReg::Ddr,
        SpecialReg::Ibreaka0,
        SpecialReg::Ibreaka1,
        SpecialReg::Dbreaka0,
        SpecialReg::Dbreaka1,
        SpecialReg::Dbreakc0,
        SpecialReg::Dbreakc1,
        SpecialReg::Epc1,
        SpecialReg::Epc2,
        SpecialReg::Epc3,
        SpecialReg::Epc4,
        SpecialReg::Epc5,
        SpecialReg::Epc6,
        SpecialReg::Epc7,
        SpecialReg::Depc,
        SpecialReg::Eps2,
        SpecialReg::Eps3,
        SpecialReg::Eps4,
        SpecialReg::Eps5,
        SpecialReg::Eps6,
        SpecialReg::Eps7,
        SpecialReg::Excsave1,
        SpecialReg::Excsave2,
        SpecialReg::Excsave3,
        SpecialReg::Excsave4,
        SpecialReg::Excsave5,
        SpecialReg::Excsave6,
        SpecialReg::Excsave7,
        SpecialReg::Cpenable,
        SpecialReg::Interrupt,
        SpecialReg::Intclear,
        SpecialReg::Intenable,
        SpecialReg::Ps,
        SpecialReg::Vecbase,
        SpecialReg::Exccause,
        SpecialReg::Debugcause,
        SpecialReg::Ccount,
        SpecialReg::Prid,
        SpecialReg::Icount,
        SpecialReg::Icountlevel,
        SpecialReg::Excvaddr,
        SpecialReg::Ccompare0,
        SpecialReg::Ccompare1,
        SpecialReg::Ccompare2,
        SpecialReg::Misc0,
        SpecialReg::Misc1,
        SpecialReg::Misc2,
        SpecialReg::Misc3,
    ];

    /// The architectural special-register number.
    #[inline]
    pub const fn num(self) -> u8 {
        match self {
            SpecialReg::Lbeg => 0,
            SpecialReg::Lend => 1,
            SpecialReg::Lcount => 2,
            SpecialReg::Sar => 3,
            SpecialReg::Br => 4,
            SpecialReg::Litbase => 5,
            SpecialReg::Scompare1 => 12,
            SpecialReg::Acclo => 16,
            SpecialReg::Acchi => 17,
            SpecialReg::M0 => 32,
            SpecialReg::M1 => 33,
            SpecialReg::M2 => 34,
            SpecialReg::M3 => 35,
            SpecialReg::WindowBase => 72,
            SpecialReg::WindowStart => 73,
            SpecialReg::IbreakEnable => 96,
            SpecialReg::Memctl => 97,
            SpecialReg::Atomctl => 99,
            SpecialReg::Ddr => 104,
            SpecialReg::Ibreaka0 => 128,
            SpecialReg::Ibreaka1 => 129,
            SpecialReg::Dbreaka0 => 144,
            SpecialReg::Dbreaka1 => 145,
            SpecialReg::Dbreakc0 => 160,
            SpecialReg::Dbreakc1 => 161,
            SpecialReg::Epc1 => 177,
            SpecialReg::Epc2 => 178,
            SpecialReg::Epc3 => 179,
            SpecialReg::Epc4 => 180,
            SpecialReg::Epc5 => 181,
            SpecialReg::Epc6 => 182,
            SpecialReg::Epc7 => 183,
            SpecialReg::Depc => 192,
            SpecialReg::Eps2 => 194,
            SpecialReg::Eps3 => 195,
            SpecialReg::Eps4 => 196,
            SpecialReg::Eps5 => 197,
            SpecialReg::Eps6 => 198,
            SpecialReg::Eps7 => 199,
            SpecialReg::Excsave1 => 209,
            SpecialReg::Excsave2 => 210,
            SpecialReg::Excsave3 => 211,
            SpecialReg::Excsave4 => 212,
            SpecialReg::Excsave5 => 213,
            SpecialReg::Excsave6 => 214,
            SpecialReg::Excsave7 => 215,
            SpecialReg::Cpenable => 224,
            SpecialReg::Interrupt => 226,
            SpecialReg::Intclear => 227,
            SpecialReg::Intenable => 228,
            SpecialReg::Ps => 230,
            SpecialReg::Vecbase => 231,
            SpecialReg::Exccause => 232,
            SpecialReg::Debugcause => 233,
            SpecialReg::Ccount => 234,
            SpecialReg::Prid => 235,
            SpecialReg::Icount => 236,
            SpecialReg::Icountlevel => 237,
            SpecialReg::Excvaddr => 238,
            SpecialReg::Ccompare0 => 240,
            SpecialReg::Ccompare1 => 241,
            SpecialReg::Ccompare2 => 242,
            SpecialReg::Misc0 => 244,
            SpecialReg::Misc1 => 245,
            SpecialReg::Misc2 => 246,
            SpecialReg::Misc3 => 247,
        }
    }

    /// The register for an architectural number, or `None` if outside the
    /// modelled set.
    ///
    /// Deliberately **partial**: see this module's doc.
    #[inline]
    pub const fn from_num(n: u8) -> Option<SpecialReg> {
        match n {
            0 => Some(SpecialReg::Lbeg),
            1 => Some(SpecialReg::Lend),
            2 => Some(SpecialReg::Lcount),
            3 => Some(SpecialReg::Sar),
            4 => Some(SpecialReg::Br),
            5 => Some(SpecialReg::Litbase),
            12 => Some(SpecialReg::Scompare1),
            16 => Some(SpecialReg::Acclo),
            17 => Some(SpecialReg::Acchi),
            32 => Some(SpecialReg::M0),
            33 => Some(SpecialReg::M1),
            34 => Some(SpecialReg::M2),
            35 => Some(SpecialReg::M3),
            72 => Some(SpecialReg::WindowBase),
            73 => Some(SpecialReg::WindowStart),
            96 => Some(SpecialReg::IbreakEnable),
            97 => Some(SpecialReg::Memctl),
            99 => Some(SpecialReg::Atomctl),
            104 => Some(SpecialReg::Ddr),
            128 => Some(SpecialReg::Ibreaka0),
            129 => Some(SpecialReg::Ibreaka1),
            144 => Some(SpecialReg::Dbreaka0),
            145 => Some(SpecialReg::Dbreaka1),
            160 => Some(SpecialReg::Dbreakc0),
            161 => Some(SpecialReg::Dbreakc1),
            177 => Some(SpecialReg::Epc1),
            178 => Some(SpecialReg::Epc2),
            179 => Some(SpecialReg::Epc3),
            180 => Some(SpecialReg::Epc4),
            181 => Some(SpecialReg::Epc5),
            182 => Some(SpecialReg::Epc6),
            183 => Some(SpecialReg::Epc7),
            192 => Some(SpecialReg::Depc),
            194 => Some(SpecialReg::Eps2),
            195 => Some(SpecialReg::Eps3),
            196 => Some(SpecialReg::Eps4),
            197 => Some(SpecialReg::Eps5),
            198 => Some(SpecialReg::Eps6),
            199 => Some(SpecialReg::Eps7),
            209 => Some(SpecialReg::Excsave1),
            210 => Some(SpecialReg::Excsave2),
            211 => Some(SpecialReg::Excsave3),
            212 => Some(SpecialReg::Excsave4),
            213 => Some(SpecialReg::Excsave5),
            214 => Some(SpecialReg::Excsave6),
            215 => Some(SpecialReg::Excsave7),
            224 => Some(SpecialReg::Cpenable),
            226 => Some(SpecialReg::Interrupt),
            227 => Some(SpecialReg::Intclear),
            228 => Some(SpecialReg::Intenable),
            230 => Some(SpecialReg::Ps),
            231 => Some(SpecialReg::Vecbase),
            232 => Some(SpecialReg::Exccause),
            233 => Some(SpecialReg::Debugcause),
            234 => Some(SpecialReg::Ccount),
            235 => Some(SpecialReg::Prid),
            236 => Some(SpecialReg::Icount),
            237 => Some(SpecialReg::Icountlevel),
            238 => Some(SpecialReg::Excvaddr),
            240 => Some(SpecialReg::Ccompare0),
            241 => Some(SpecialReg::Ccompare1),
            242 => Some(SpecialReg::Ccompare2),
            244 => Some(SpecialReg::Misc0),
            245 => Some(SpecialReg::Misc1),
            246 => Some(SpecialReg::Misc2),
            247 => Some(SpecialReg::Misc3),
            _ => None,
        }
    }

    /// Whether `op` is a legal access to this register.
    ///
    /// Three registers are asymmetric, and the assembler refuses the missing
    /// forms — so this crate does too, rather than pretending the space is
    /// uniform:
    ///
    /// | Register | `rsr` | `wsr` | `xsr` |
    /// |---|---|---|---|
    /// | SR 226 | `rsr.interrupt` | `wsr.intset` | — |
    /// | SR 227 | — | `wsr.intclear` | — |
    /// | SR 235 | `rsr.prid` | — | — |
    #[inline]
    pub const fn allows(self, op: SrOp) -> bool {
        match self {
            SpecialReg::Interrupt => matches!(op, SrOp::Rsr | SrOp::Wsr),
            SpecialReg::Intclear => matches!(op, SrOp::Wsr),
            SpecialReg::Prid => matches!(op, SrOp::Rsr),
            _ => true,
        }
    }

    /// The objdump mnemonic suffix under `op` (`rsr.<name>`).
    ///
    /// SR 226 is the one register whose *name* depends on the direction:
    /// `rsr.interrupt` reads pending interrupts, `wsr.intset` raises them.
    #[inline]
    pub const fn name_for(self, op: SrOp) -> &'static str {
        match (self, op) {
            (SpecialReg::Interrupt, SrOp::Wsr) => "intset",
            _ => self.name(),
        }
    }

    /// The register's canonical (read-side) objdump mnemonic suffix.
    ///
    /// Prefer [`SpecialReg::name_for`] when rendering a specific access.
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            SpecialReg::Lbeg => "lbeg",
            SpecialReg::Lend => "lend",
            SpecialReg::Lcount => "lcount",
            SpecialReg::Sar => "sar",
            SpecialReg::Br => "br",
            SpecialReg::Litbase => "litbase",
            SpecialReg::Scompare1 => "scompare1",
            SpecialReg::Acclo => "acclo",
            SpecialReg::Acchi => "acchi",
            SpecialReg::M0 => "m0",
            SpecialReg::M1 => "m1",
            SpecialReg::M2 => "m2",
            SpecialReg::M3 => "m3",
            SpecialReg::WindowBase => "windowbase",
            SpecialReg::WindowStart => "windowstart",
            SpecialReg::IbreakEnable => "ibreakenable",
            SpecialReg::Memctl => "memctl",
            SpecialReg::Atomctl => "atomctl",
            SpecialReg::Ddr => "ddr",
            SpecialReg::Ibreaka0 => "ibreaka0",
            SpecialReg::Ibreaka1 => "ibreaka1",
            SpecialReg::Dbreaka0 => "dbreaka0",
            SpecialReg::Dbreaka1 => "dbreaka1",
            SpecialReg::Dbreakc0 => "dbreakc0",
            SpecialReg::Dbreakc1 => "dbreakc1",
            SpecialReg::Epc1 => "epc1",
            SpecialReg::Epc2 => "epc2",
            SpecialReg::Epc3 => "epc3",
            SpecialReg::Epc4 => "epc4",
            SpecialReg::Epc5 => "epc5",
            SpecialReg::Epc6 => "epc6",
            SpecialReg::Epc7 => "epc7",
            SpecialReg::Depc => "depc",
            SpecialReg::Eps2 => "eps2",
            SpecialReg::Eps3 => "eps3",
            SpecialReg::Eps4 => "eps4",
            SpecialReg::Eps5 => "eps5",
            SpecialReg::Eps6 => "eps6",
            SpecialReg::Eps7 => "eps7",
            SpecialReg::Excsave1 => "excsave1",
            SpecialReg::Excsave2 => "excsave2",
            SpecialReg::Excsave3 => "excsave3",
            SpecialReg::Excsave4 => "excsave4",
            SpecialReg::Excsave5 => "excsave5",
            SpecialReg::Excsave6 => "excsave6",
            SpecialReg::Excsave7 => "excsave7",
            SpecialReg::Cpenable => "cpenable",
            SpecialReg::Interrupt => "interrupt",
            SpecialReg::Intclear => "intclear",
            SpecialReg::Intenable => "intenable",
            SpecialReg::Ps => "ps",
            SpecialReg::Vecbase => "vecbase",
            SpecialReg::Exccause => "exccause",
            SpecialReg::Debugcause => "debugcause",
            SpecialReg::Ccount => "ccount",
            SpecialReg::Prid => "prid",
            SpecialReg::Icount => "icount",
            SpecialReg::Icountlevel => "icountlevel",
            SpecialReg::Excvaddr => "excvaddr",
            SpecialReg::Ccompare0 => "ccompare0",
            SpecialReg::Ccompare1 => "ccompare1",
            SpecialReg::Ccompare2 => "ccompare2",
            SpecialReg::Misc0 => "misc0",
            SpecialReg::Misc1 => "misc1",
            SpecialReg::Misc2 => "misc2",
            SpecialReg::Misc3 => "misc3",
        }
    }
}

/// A user register reachable by `RUR`/`WUR`.
///
/// A different opcode space from [`SpecialReg`], not a subset of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UserReg {
    /// `EXPSTATE` — ESP32 GPIO-expansion state, UR 230. **LX6 only** (the LX7
    /// assembler rejects the mnemonic).
    Expstate,
    /// `THREADPTR` — the thread pointer, UR 231. `esp-rtos`'s task switch moves
    /// it with `rur.threadptr` / `wur.threadptr`.
    Threadptr,
    /// `FCR` — FP control register (rounding mode), UR 232.
    Fcr,
    /// `FSR` — FP status register (sticky exception flags), UR 233.
    Fsr,
    /// `F64R_LO` — double-precision accelerator result, low word, UR 234.
    /// **LX6 only.**
    F64rLo,
    /// `F64R_HI` — double-precision accelerator result, high word, UR 235.
    /// **LX6 only.**
    F64rHi,
    /// `F64S` — double-precision accelerator status, UR 236. **LX6 only.**
    F64s,
}

impl UserReg {
    /// Every modelled user register, in ascending [`UserReg::num`] order.
    pub const ALL: [UserReg; 7] = [
        UserReg::Expstate,
        UserReg::Threadptr,
        UserReg::Fcr,
        UserReg::Fsr,
        UserReg::F64rLo,
        UserReg::F64rHi,
        UserReg::F64s,
    ];

    /// The architectural user-register number.
    #[inline]
    pub const fn num(self) -> u8 {
        match self {
            UserReg::Expstate => 230,
            UserReg::Threadptr => 231,
            UserReg::Fcr => 232,
            UserReg::Fsr => 233,
            UserReg::F64rLo => 234,
            UserReg::F64rHi => 235,
            UserReg::F64s => 236,
        }
    }

    /// The register for an architectural number, or `None` if outside the
    /// modelled set.
    #[inline]
    pub const fn from_num(n: u8) -> Option<UserReg> {
        match n {
            230 => Some(UserReg::Expstate),
            231 => Some(UserReg::Threadptr),
            232 => Some(UserReg::Fcr),
            233 => Some(UserReg::Fsr),
            234 => Some(UserReg::F64rLo),
            235 => Some(UserReg::F64rHi),
            236 => Some(UserReg::F64s),
            _ => None,
        }
    }

    /// The objdump mnemonic suffix (`rur.<name>`).
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            UserReg::Expstate => "expstate",
            UserReg::Threadptr => "threadptr",
            UserReg::Fcr => "fcr",
            UserReg::Fsr => "fsr",
            UserReg::F64rLo => "f64r_lo",
            UserReg::F64rHi => "f64r_hi",
            UserReg::F64s => "f64s",
        }
    }
}

/// Which special-register access a [`crate::Inst::Sr`] performs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SrOp {
    /// `rsr.<sr> at` — read; `op1 = 3`, `op2 = 0`.
    Rsr,
    /// `wsr.<sr> at` — write; `op1 = 3`, `op2 = 1`.
    Wsr,
    /// `xsr.<sr> at` — atomic exchange; `op1 = 1`, `op2 = 6`.
    Xsr,
}

impl SrOp {
    /// The objdump mnemonic prefix.
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            SrOp::Rsr => "rsr",
            SrOp::Wsr => "wsr",
            SrOp::Xsr => "xsr",
        }
    }
}

/// Which user-register access a [`crate::Inst::Ur`] performs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UrOp {
    /// `rur.<ur> at` — read; `op1 = 3`, `op2 = 0xE`.
    Rur,
    /// `wur.<ur> at` — write; `op1 = 3`, `op2 = 0xF`.
    Wur,
}

impl UrOp {
    /// The objdump mnemonic prefix.
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            UrOp::Rur => "rur",
            UrOp::Wur => "wur",
        }
    }
}
