// Immediate-range data derived from espressif/llvm-project
//   llvm/lib/Target/Xtensa/XtensaOperands.td
//   llvm/lib/Target/Xtensa/XtensaInstrInfo.td
//   llvm/lib/Target/Xtensa/MCTargetDesc/XtensaAsmBackend.cpp (fixup ranges)
//   commit f6ee8246025cea8986ce90f5fe3660efcd66cb5f
// Apache License v2.0 WITH LLVM-exception; see
//   licenses/LLVM-Apache-2.0-with-LLVM-exception.txt
//
// Every range below was additionally probed at its boundaries against
// xtensa-esp32s3-elf-as (gas accepts min/max, rejects one step beyond) and,
// where lp-xt-inst encodes the opcode, asserted to encode/decode round-trip
// (tests/imm_legality.rs). No GPL source (binutils, QEMU) was copied — see
// docs/adr/2026-07-28-license-provenance-discipline.md.
//
//! Per-opcode immediate legality for the Xtensa (ESP32-S3 / LX7) integer subset.
//!
//! This is the data table `lpvm-native/src/imm.rs` needs in order to become
//! ISA-parameterized. RV32 has one uniform story (imm12 everywhere, `andi`/
//! `ori`/`xori` exist); Xtensa's immediate legality is **per-opcode**:
//!
//! - `addi` takes -128..=127 while `movi` takes -2048..=2047 and `addmi` takes
//!   multiples of 256 up to +/-32K;
//! - load/store offsets are unsigned and scaled by the access width;
//! - **there are no `andi`/`ori`/`xori` instructions at all** — bitwise ops
//!   with an immediate MUST materialize the constant (see [`ImmRule::NoImmForm`],
//!   an explicit entry rather than an omission);
//! - branch reach differs per branch family (+-128 B vs +-2 KB vs +-128 KB);
//! - `l32r` is backward-only with a one-extended, word-scaled field.
//!
//! Shape: a single [`ImmOp`] key per immediate operand class, a const
//! [`spec`] lookup returning the legal [`ImmRule`] plus the documented
//! [`Fallback`] lowering, and [`is_legal`] as the one predicate the emitter
//! (and later `lpvm-native`) gates on. Out-of-range immediates must be a hard
//! error or a documented fallback — never silent truncation
//! ([`lp_xt_inst::encode`] masks fields and does *not* validate).
//!
//! ## LX6 (classic ESP32) note — verified, no longer asserted
//!
//! Every rule in this table is identical on LX6 and LX7: the core-ISA
//! encodings, the Code Density option, and the MUL32/DIV32 options carry the
//! same immediate fields on both. No entry differs on classic ESP32.
//!
//! Evidence (P6, 2026-07-28): every boundary in this table was probed with
//! BOTH assemblers (`xtensa-esp32-elf-as` for LX6 vs `xtensa-esp32s3-elf-as`
//! for LX7, `--no-transform`) — 171 cases, identical accept/reject verdicts
//! and byte-identical encodings, including the absence of `andi`/`ori`/
//! `xori`, `l32r`'s one-extended backward reach (field 0x7FFF = −131076),
//! the joint `extui` constraint, `entry`'s frame field, and every branch/
//! `j`/`call0-12` reach limit. The probe is a live test
//! (`tests/imm_gas_lx6.rs`, skips loudly without the toolchain). The subset
//! the emitter exercises also ran divergence-free on classic silicon via the
//! P5 N-run corpus (FINDINGS.md, "LX6 conformance").

/// How an immediate operand is constrained.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImmRule {
    /// Contiguous range `min..=max`; the value must also be a multiple of
    /// `step` (all `min`/`max` in this table are themselves multiples of
    /// `step`, so `imm % step == 0` is the whole scaling rule).
    Range { min: i32, max: i32, step: i32 },
    /// The value must be a member of a lookup set (`b4const` / `b4constu`).
    Set(&'static [i32]),
    /// The opcode family has **no immediate form at all** (Xtensa has no
    /// `andi`/`ori`/`xori`). Every immediate is illegal; the fallback is the
    /// only lowering.
    NoImmForm,
}

/// The base a PC-relative immediate is measured from (`target = base + imm`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PcRel {
    /// Not PC-relative.
    None,
    /// `base = PC + 4` — all conditional branches, `beqz.n`/`bnez.n`, and `j`
    /// (regardless of the instruction's own 2- or 3-byte length).
    NextPc,
    /// `base = (PC & !3) + 4` — `call0/4/8/12` (target must be 4-aligned).
    AlignedNextPc,
    /// `base = (PC + 3) & !3` — `l32r` (word-aligned literal, backward only).
    AlignedPcPlus3,
}

/// The documented lowering when an immediate is outside its legal rule.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fallback {
    /// Materialize the constant into a scratch register (`movi`, else pooled
    /// `l32r`) and use the register-register form of the operation.
    ConstThenReg,
    /// Split the add: `addmi` covers the multiple-of-256 part, `addi` the
    /// signed low byte; beyond +-32K fall through to [`Fallback::ConstThenReg`]
    /// with `add`.
    AddmiSplit,
    /// Emit the constant into the literal pool and load it with a backward
    /// `l32r`.
    LiteralPool,
    /// Compute `base + offset` into a scratch register ([`Fallback::AddmiSplit`]
    /// arithmetic) and use the load/store with offset 0.
    AddressScratch,
    /// Use the wide (24-bit) form of the same operation, which has a strictly
    /// larger immediate rule.
    WideForm,
    /// Branch relaxation: invert the condition and branch over an
    /// unconditional `j` (extends any conditional branch to `j` reach).
    InvertOverJ,
    /// Pool the absolute target address, `l32r` it into a scratch register,
    /// and use the indirect form (`jx` / `callx0/4/8/12`).
    IndirectViaL32r,
    /// A different opcode covers the out-of-range case (documented on the
    /// [`ImmOp`] variant: `mov` for `slli` sa=0, `extui` for `srli` sa>=16,
    /// `slli`+`srai` for `sext` bit>22).
    OtherOpcode,
    /// No lowering exists: an out-of-range value here is an emitter-invariant
    /// violation and must be a hard error (never silent truncation).
    HardError,
}

/// The full legality record for one immediate operand class.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ImmSpec {
    /// Legal values.
    pub rule: ImmRule,
    /// Whether the machine field is signed (sign-/one-extended or a signed
    /// lookup) as opposed to zero-extended. For [`ImmRule::NoImmForm`] this is
    /// vacuous (`false`).
    pub signed: bool,
    /// PC-relative base, when the immediate is a displacement.
    pub pc_rel: PcRel,
    /// The documented lowering for illegal values.
    pub fallback: Fallback,
}

/// Every immediate operand class the emitter emits or plausibly will.
///
/// PC-relative variants are keyed by *byte displacement from their
/// [`PcRel`] base* — not by the raw encoded field — so `is_legal` answers the
/// question the emitter actually asks ("can I reach this target?").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImmOp {
    /// `addi at, as, imm8` — signed 8-bit. RV32 users note: 16x smaller than imm12.
    Addi,
    /// `addi.n ad, as, imm` (16-bit density form) — the set -1..=15 *excluding 0*
    /// (field value 0 encodes -1).
    AddiN,
    /// `addmi at, as, imm8<<8` — the scaled larger-range add: multiples of 256
    /// in -32768..=32512. Pairs with `addi` to synthesize any 16-bit-ish add.
    Addmi,
    /// `movi at, imm12` — signed 12-bit constant materialization.
    Movi,
    /// `movi.n as, imm7` (density) — asymmetric -32..=95 (7-bit field, values
    /// 96..127 decode as -32..-1).
    MoviN,
    /// `and` with immediate: **does not exist on Xtensa** ([`ImmRule::NoImmForm`]).
    /// (`extui` covers the special case `mask == (1<<k)-1, k<=16` as an
    /// optimization, but the general lowering is pool + `and`.)
    AndImm,
    /// `or` with immediate: **does not exist on Xtensa** ([`ImmRule::NoImmForm`]).
    OrImm,
    /// `xor` with immediate: **does not exist on Xtensa** ([`ImmRule::NoImmForm`]).
    XorImm,
    /// `l8ui at, as, off` — unsigned byte offset 0..=255, unscaled.
    L8ui,
    /// `l16ui at, as, off` — unsigned offset 0..=510, multiple of 2.
    L16ui,
    /// `l16si at, as, off` — unsigned offset 0..=510, multiple of 2.
    L16si,
    /// `l32i at, as, off` — unsigned offset 0..=1020, multiple of 4.
    L32i,
    /// `s8i at, as, off` — unsigned byte offset 0..=255, unscaled.
    S8i,
    /// `s16i at, as, off` — unsigned offset 0..=510, multiple of 2.
    S16i,
    /// `s32i at, as, off` — unsigned offset 0..=1020, multiple of 4.
    S32i,
    /// `l32i.n at, as, off` (density) — offset 0..=60, multiple of 4.
    L32iN,
    /// `s32i.n at, as, off` (density) — offset 0..=60, multiple of 4.
    S32iN,
    /// `l32r at, target` — byte displacement from `(PC + 3) & !3`. The 16-bit
    /// field is **one-extended** and word-scaled: displacement -262144..=-4,
    /// multiple of 4, **backward only** (a forward literal is unencodable).
    /// NOTE: `emit.rs` currently asserts the sign-extended-i16 subset
    /// (-131072..=-4); field values 0x0000..0x7fff legally reach the farther
    /// -262144..=-131076 half (gas-confirmed).
    L32rDisp,
    /// `entry as, frame` — unsigned, multiple of 8, 0..=32760 (imm12 field
    /// scaled by 8). This is the large-frame limit: frames > 32760 bytes have
    /// no `entry` encoding and need the `movsp` idiom (P6 item 1) — until
    /// then, a hard error.
    EntryFrame,
    /// `slli ar, as, sa` — shift amount 1..=31 (the 5-bit field stores
    /// `32 - sa`). sa=0 is a plain `mov` (LLVM rejects it; use the fallback).
    /// gas additionally accepts sa=32 (field 0, result always 0) — this table
    /// follows LLVM and treats 32 as illegal (lower to `movi 0`).
    SlliSa,
    /// `srli ar, at, sa` — **only 0..=15** (4-bit field). sa in 16..=31 must
    /// lower to `extui ar, at, sa, 32-sa`.
    SrliSa,
    /// `srai ar, at, sa` — 0..=31.
    SraiSa,
    /// `ssai sa` — 0..=31 (sets SAR for `src`).
    SsaiSa,
    /// `extui ar, at, shift, width` — shift 0..=31. Joint constraint with the
    /// width: `shift + width <= 32` (see [`extui_legal`]).
    ExtuiShift,
    /// `extui` field width 1..=16 (op2 stores `width - 1`). Joint constraint:
    /// `shift + width <= 32` (see [`extui_legal`]).
    ExtuiWidth,
    /// `sext ar, as, bit` — sign-extend from bit position 7..=22. Positions
    /// outside lower to the `slli`+`srai` pair.
    SextBit,
    /// `bbci`/`bbsi` bit index 0..=31 — structural (chosen by the compiler,
    /// never data-dependent), so out of range is a hard error.
    BbiBit,
    /// All RRI8 conditional branches (`beq`/`bne`/`blt[u]`/`bge[u]`/`bany`/
    /// `ball`/`bnone`/`bnall`/`bbc`/`bbs`/`bbci`/`bbsi`/`beqi`/`bnei`/`blti`/
    /// `bgei`/`bltui`/`bgeui`): displacement -128..=127 from `PC + 4`.
    Branch8Disp,
    /// BRI12 compare-to-zero branches (`beqz`/`bnez`/`bltz`/`bgez`):
    /// displacement -2048..=2047 from `PC + 4` — 16x the reach of the
    /// two-register compares.
    Branch12Disp,
    /// `beqz.n`/`bnez.n` (density): displacement 0..=63 from `PC + 4`,
    /// **forward only** (unsigned 6-bit field).
    Branch6NDisp,
    /// The comparison value of `beqi`/`bnei`/`blti`/`bgei` — the signed
    /// `b4const` set [`lp_xt_inst::B4CONST`], not a range.
    BranchB4Const,
    /// The comparison value of `bltui`/`bgeui` — the unsigned `b4constu` set
    /// [`lp_xt_inst::B4CONSTU`] (note: contains 32768 and 65536, but not 0/1).
    BranchB4Constu,
    /// `j target` — displacement -131072..=131071 from `PC + 4` (signed
    /// 18-bit byte offset).
    JDisp,
    /// `call0/4/8/12 target` — displacement -524288..=524284 from
    /// `(PC & !3) + 4`, multiple of 4 (signed 18-bit *word* offset; target is
    /// always 4-aligned).
    CallDisp,
    /// `lsi`/`ssi ft, as, off` — the **float** load/store offset: unsigned
    /// 0..=1020, multiple of 4 (ISA RM, *Load Single Immediate*, p. 399 — the
    /// `imm8` field holds `off / 4`).
    ///
    /// Numerically identical to [`ImmOp::L32i`]/[`ImmOp::S32i`], and a separate
    /// entry anyway, for two reasons. It is the offset of a *different*
    /// instruction on a different register file, so the two ranges are free to
    /// diverge without this table lying; and, more urgently, it is the entry
    /// that gates a known silent-corruption hazard.
    ///
    /// **The hazard:** `lp_xt_inst`'s encoder computes the field as
    /// `(offset / 4) & 0xff` with no range check. A float spill slot at offset
    /// 1024 therefore encodes as field 0 — it stores to `[base + 0]`, a
    /// perfectly valid address holding some other live value, with no
    /// diagnostic anywhere. Nothing downstream can distinguish that from a
    /// correct spill. Every float spill offset must be gated through
    /// [`is_legal`] before it reaches the encoder; past the limit, the emitter
    /// takes [`Fallback::AddressScratch`] (compute `base + offset` into `a8`
    /// and use offset 0) rather than truncating.
    FpLsiOffset,
}

impl ImmOp {
    /// Every entry, for exhaustive iteration in tests and docs.
    pub const ALL: &'static [ImmOp] = &[
        ImmOp::Addi,
        ImmOp::AddiN,
        ImmOp::Addmi,
        ImmOp::Movi,
        ImmOp::MoviN,
        ImmOp::AndImm,
        ImmOp::OrImm,
        ImmOp::XorImm,
        ImmOp::L8ui,
        ImmOp::L16ui,
        ImmOp::L16si,
        ImmOp::L32i,
        ImmOp::S8i,
        ImmOp::S16i,
        ImmOp::S32i,
        ImmOp::L32iN,
        ImmOp::S32iN,
        ImmOp::L32rDisp,
        ImmOp::EntryFrame,
        ImmOp::SlliSa,
        ImmOp::SrliSa,
        ImmOp::SraiSa,
        ImmOp::SsaiSa,
        ImmOp::ExtuiShift,
        ImmOp::ExtuiWidth,
        ImmOp::SextBit,
        ImmOp::BbiBit,
        ImmOp::Branch8Disp,
        ImmOp::Branch12Disp,
        ImmOp::Branch6NDisp,
        ImmOp::BranchB4Const,
        ImmOp::BranchB4Constu,
        ImmOp::JDisp,
        ImmOp::CallDisp,
        ImmOp::FpLsiOffset,
    ];
}

/// A [`Range`](ImmRule::Range) with step 1 (unscaled).
const fn range(min: i32, max: i32) -> ImmRule {
    ImmRule::Range { min, max, step: 1 }
}

/// A scaled [`Range`](ImmRule::Range).
const fn scaled(min: i32, max: i32, step: i32) -> ImmRule {
    ImmRule::Range { min, max, step }
}

/// The full legality record for `op`. This match IS the table.
pub const fn spec(op: ImmOp) -> ImmSpec {
    // Shorthand: a non-PC-relative spec.
    const fn plain(rule: ImmRule, signed: bool, fallback: Fallback) -> ImmSpec {
        ImmSpec {
            rule,
            signed,
            pc_rel: PcRel::None,
            fallback,
        }
    }
    // Shorthand: a PC-relative displacement spec (always signed unless noted).
    const fn pcrel(rule: ImmRule, signed: bool, pc_rel: PcRel, fallback: Fallback) -> ImmSpec {
        ImmSpec {
            rule,
            signed,
            pc_rel,
            fallback,
        }
    }
    match op {
        // -- constant materialization / add --
        ImmOp::Addi => plain(range(-128, 127), true, Fallback::AddmiSplit),
        ImmOp::AddiN => plain(range(-1, 15), true, Fallback::WideForm), // 0 excluded: see is_legal
        ImmOp::Addmi => plain(scaled(-32768, 32512, 256), true, Fallback::ConstThenReg),
        ImmOp::Movi => plain(range(-2048, 2047), true, Fallback::LiteralPool),
        ImmOp::MoviN => plain(range(-32, 95), true, Fallback::WideForm),
        // -- THE key Xtensa fact: no bitwise-immediate forms exist --
        ImmOp::AndImm => plain(ImmRule::NoImmForm, false, Fallback::ConstThenReg),
        ImmOp::OrImm => plain(ImmRule::NoImmForm, false, Fallback::ConstThenReg),
        ImmOp::XorImm => plain(ImmRule::NoImmForm, false, Fallback::ConstThenReg),
        // -- load/store offsets (unsigned, scaled by access width) --
        ImmOp::L8ui | ImmOp::S8i => plain(range(0, 255), false, Fallback::AddressScratch),
        ImmOp::L16ui | ImmOp::L16si | ImmOp::S16i => {
            plain(scaled(0, 510, 2), false, Fallback::AddressScratch)
        }
        ImmOp::L32i | ImmOp::S32i => plain(scaled(0, 1020, 4), false, Fallback::AddressScratch),
        ImmOp::L32iN | ImmOp::S32iN => plain(scaled(0, 60, 4), false, Fallback::WideForm),
        // -- float load/store offset (`lsi`/`ssi`); see the variant's doc for
        //    why the encoder's missing range check makes this load-bearing --
        ImmOp::FpLsiOffset => plain(scaled(0, 1020, 4), false, Fallback::AddressScratch),
        // -- literal pool --
        ImmOp::L32rDisp => pcrel(
            scaled(-262144, -4, 4),
            true, // one-extended: every field value is negative
            PcRel::AlignedPcPlus3,
            Fallback::HardError, // pool-before-code layout must guarantee reach
        ),
        // -- frame --
        ImmOp::EntryFrame => plain(scaled(0, 32760, 8), false, Fallback::HardError),
        // -- shift / extract fields --
        ImmOp::SlliSa => plain(range(1, 31), false, Fallback::OtherOpcode), // 0 => mov
        ImmOp::SrliSa => plain(range(0, 15), false, Fallback::OtherOpcode), // 16..=31 => extui
        ImmOp::SraiSa => plain(range(0, 31), false, Fallback::HardError),
        ImmOp::SsaiSa => plain(range(0, 31), false, Fallback::HardError),
        ImmOp::ExtuiShift => plain(range(0, 31), false, Fallback::HardError),
        ImmOp::ExtuiWidth => plain(range(1, 16), false, Fallback::ConstThenReg), // wider mask => pool + and
        ImmOp::SextBit => plain(range(7, 22), false, Fallback::OtherOpcode),     // slli+srai pair
        ImmOp::BbiBit => plain(range(0, 31), false, Fallback::HardError),
        // -- branch displacements (from PC + 4) --
        ImmOp::Branch8Disp => pcrel(range(-128, 127), true, PcRel::NextPc, Fallback::InvertOverJ),
        ImmOp::Branch12Disp => pcrel(
            range(-2048, 2047),
            true,
            PcRel::NextPc,
            Fallback::InvertOverJ,
        ),
        ImmOp::Branch6NDisp => pcrel(range(0, 63), false, PcRel::NextPc, Fallback::WideForm),
        // -- branch comparison constants (sets, not ranges) --
        ImmOp::BranchB4Const => plain(
            ImmRule::Set(&lp_xt_inst::B4CONST),
            true,
            Fallback::ConstThenReg,
        ),
        ImmOp::BranchB4Constu => plain(
            ImmRule::Set(&lp_xt_inst::B4CONSTU),
            false,
            Fallback::ConstThenReg,
        ),
        // -- jumps / calls --
        ImmOp::JDisp => pcrel(
            range(-131072, 131071),
            true,
            PcRel::NextPc,
            Fallback::IndirectViaL32r,
        ),
        ImmOp::CallDisp => pcrel(
            scaled(-524288, 524284, 4),
            true,
            PcRel::AlignedNextPc,
            Fallback::IndirectViaL32r,
        ),
    }
}

/// Is `imm` a legal immediate for `op`?
///
/// For PC-relative entries `imm` is the byte displacement from the entry's
/// [`PcRel`] base. For [`ImmRule::NoImmForm`] entries this is `false` for
/// every value — the immediate form does not exist.
pub const fn is_legal(op: ImmOp, imm: i32) -> bool {
    // addi.n's field has no encoding for 0 (field value 0 means -1).
    if matches!(op, ImmOp::AddiN) && imm == 0 {
        return false;
    }
    match spec(op).rule {
        ImmRule::Range { min, max, step } => imm >= min && imm <= max && imm % step == 0,
        ImmRule::Set(set) => {
            let mut i = 0;
            while i < set.len() {
                if set[i] == imm {
                    return true;
                }
                i += 1;
            }
            false
        }
        ImmRule::NoImmForm => false,
    }
}

/// The documented fallback lowering for `op` (see [`Fallback`]).
pub const fn fallback(op: ImmOp) -> Fallback {
    spec(op).fallback
}

/// The joint `extui` constraint: both fields in range **and**
/// `shift + width <= 32` (the extracted field may not read past bit 31;
/// gas rejects e.g. `extui a2, a3, 17, 16` with "operands sum to greater
/// than 32").
pub const fn extui_legal(shift: i32, width: i32) -> bool {
    is_legal(ImmOp::ExtuiShift, shift) && is_legal(ImmOp::ExtuiWidth, width) && shift + width <= 32
}
