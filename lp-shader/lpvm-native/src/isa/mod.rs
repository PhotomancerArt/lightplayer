pub mod shared;

#[cfg(feature = "isa-rv32")]
pub mod rv32;
#[cfg(feature = "isa-xt")]
pub mod xt;

use alloc::string::String;

use lpir::IrFunction;
use object::Architecture;

use crate::abi::{FrameLayout, PReg, RegClass};
use crate::regalloc::{AllocError, AllocOutput};
use crate::vinst::{AluImmOp, ModuleSymbols, VInst, VReg};

/// Options for annotated disassembly text.
///
/// Re-exported here so callers outside `isa::` never name an ISA-specific
/// module.
pub use shared::DisasmOptions;

/// Backend emitter output: machine code, call relocations, and debug lines.
///
/// One shape for every backend — see [`shared`].
pub(crate) use shared::IsaEmitOutput;

/// The target ISA + sub-architecture for a compiled module.
///
/// Variant names describe the **target hardware**, not the codegen output.
/// `Rv32imac` is the ESP32-C6 target (`riscv32imac-unknown-none-elf`); the
/// emitter currently produces only base RV32IM instructions. The A and C
/// extensions appear in the target name because the firmware runtime uses
/// them, not because we emit them.
///
/// Because the name is the *hardware*, an rv32 part that **does** have the F
/// extension (ESP32-S31, ESP32-P4, any RV32IMAFC core) is a **new variant**,
/// not a flag on this one. See [`IsaTarget::f32_lowering`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum IsaTarget {
    #[cfg(feature = "isa-rv32")]
    Rv32imac,
    /// Xtensa windowed-ABI target (ESP32-S3 / LX7 and classic ESP32 / LX6 —
    /// the two are ISA-identical for the emitted integer subset; only the JIT
    /// buffer placement differs per chip, and that lives in `rt_jit`).
    #[cfg(feature = "isa-xt")]
    Xtensa,
}

/// How a target performs IEEE-754 binary32 arithmetic in
/// [`lpir::FloatMode::F32`].
///
/// This is the **float-capability seam**. It exists because "rv32" is not one
/// float story: the ESP32-C6 and RP2350's Hazard3 are RV32IMAC with no F
/// extension at all, while the ESP32-S31 and ESP32-P4 are RV32IMAFC with a
/// per-core single-precision FPU. Emitting `fadd.s` for a C6 does not produce a
/// wrong number — it takes an illegal-instruction trap on the first frame. So
/// the choice has to be a *named property of the target*, checked in one place,
/// rather than an assumption baked into shared lowering.
///
/// Note what makes this safe today: **no variant of [`IsaTarget`] answers
/// [`F32Lowering::HardwareFpu`]**, so no code path in this crate can emit an
/// FP-register instruction. The variant is not speculative padding — it is the
/// thing a new `Rv32imafc` (or the Xtensa FPU backend, roadmap M7) flips, and
/// having it here means that change is one arm of one match instead of a search
/// for every place float lowering assumed soft calls.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum F32Lowering {
    /// This target cannot execute f32 shaders at all; `FloatMode::F32` is a
    /// compile error naming the target.
    Unsupported,
    /// Float ops lower to calls into the platform soft-float library. Values
    /// live in **integer** registers ([`RegClass::Int`]) — the soft-float ABI
    /// passes and returns a `float` in `a0`-class registers — so this path
    /// needs no float register class, no float argument bank, and no new
    /// emitter instructions.
    SoftFloatCalls,
    /// Float ops lower to native FP instructions operating on the float
    /// register file ([`RegClass::Float`]).
    HardwareFpu,
}

impl IsaTarget {
    /// Pool-init order for the register allocator's LRU, for one register class.
    ///
    /// Every register hook on this type is a **per-class** query, because the
    /// allocator runs one independent pool per [`RegClass`]: an integer vreg can
    /// never satisfy a float constraint, and the same hardware encoding names
    /// two different registers in the two classes.
    ///
    /// Only one backend has a float pool: Xtensa, and only when `float-f32` is
    /// enabled. rv32's f32 path is soft float, which keeps every value in
    /// integer registers (see [`F32Lowering::SoftFloatCalls`]), so an empty
    /// float pool is the *correct* answer there rather than a missing feature.
    ///
    /// An empty pool is not a silent fallback: a float vreg reaching the
    /// allocator on such a target fails with [`AllocError::OutOfRegisters`]
    /// rather than landing in a GPR and being read back as an integer.
    ///
    /// Note that a Q32 shader has no float vregs at all — a Q16.16 `float` is
    /// an integer and lowers to integer instructions — so the empty pool is
    /// also the honest answer for the fixed-point path, not a placeholder.
    pub fn allocatable_pool_order(self, class: RegClass) -> &'static [u8] {
        match class {
            RegClass::Int => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => crate::isa::rv32::gpr::ALLOC_POOL,
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => crate::isa::xt::gpr::ALLOC_POOL,
            },
            RegClass::Float => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => &[],
                #[cfg(all(feature = "isa-xt", feature = "float-f32"))]
                IsaTarget::Xtensa => crate::isa::xt::fpr::ALLOC_POOL,
                #[cfg(all(feature = "isa-xt", not(feature = "float-f32")))]
                IsaTarget::Xtensa => &[],
            },
        }
    }

    /// True if `p` is in the allocatable register pool of its own class.
    pub fn is_in_allocatable_pool(self, p: PReg) -> bool {
        match p.class {
            RegClass::Int => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => crate::isa::rv32::gpr::pool_contains(p.hw),
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => crate::isa::xt::gpr::pool_contains(p.hw),
            },
            RegClass::Float => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => false,
                #[cfg(all(feature = "isa-xt", feature = "float-f32"))]
                IsaTarget::Xtensa => crate::isa::xt::fpr::pool_contains(p.hw),
                #[cfg(all(feature = "isa-xt", not(feature = "float-f32")))]
                IsaTarget::Xtensa => false,
            },
        }
    }

    /// Human-readable name for `p` (debug rendering only).
    pub fn reg_name(self, p: PReg) -> &'static str {
        match p.class {
            RegClass::Int => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => crate::isa::rv32::gpr::reg_name(p.hw),
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => crate::isa::xt::gpr::reg_name(p.hw),
            },
            RegClass::Float => match self {
                // Soft float never allocates a float register, so there is no
                // name to give. Rendering must not panic, so this stays a
                // legible placeholder rather than an `unreachable!`.
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => "f?",
                #[cfg(all(feature = "isa-xt", feature = "float-f32"))]
                IsaTarget::Xtensa => crate::isa::xt::fpr::reg_name(p.hw),
                #[cfg(all(feature = "isa-xt", not(feature = "float-f32")))]
                IsaTarget::Xtensa => "f?",
            },
        }
    }

    /// True if a return value with `scalar_count` scalars uses the
    /// sret-via-buffer convention rather than direct registers.
    pub fn sret_uses_buffer_for(self, scalar_count: u32) -> bool {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => {
                (scalar_count as usize) > crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD
            }
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => {
                (scalar_count as usize) > crate::isa::xt::abi::SRET_SCALAR_THRESHOLD
            }
        }
    }

    /// Minimum stack frame alignment in bytes.
    pub fn stack_alignment(self) -> u32 {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => crate::isa::rv32::abi::STACK_ALIGNMENT,
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => crate::isa::xt::abi::STACK_ALIGNMENT,
        }
    }

    /// Bytes reserved at the **top** of every stack frame (highest addresses,
    /// above the saved RA/FP/callee-save area) that the compiler must not use
    /// for spill slots or LPIR slots.
    ///
    /// RV32 reserves nothing — nothing but this function's own prologue writes
    /// inside its frame.
    ///
    /// Xtensa (ESP32-S3) is the reason this hook exists. With the windowed ABI
    /// the CPU's register-window **overflow handlers** store the caller's
    /// registers into the callee's frame without the callee asking: `16 * u`
    /// bytes at the frame top, where `u` is the CALL increment unit (CALL8 →
    /// 32 bytes, `FRAME_TOP_RESERVED_BYTES = 32`). Per BACKPORT.md this is
    /// "the one structural change needed" in the compiler core: get it wrong
    /// and **ancestor frames corrupt silently** — the handler overwrites
    /// whatever the callee put there, and the damage only surfaces after the
    /// return unwinds. Backed by 68 dual-run hardware cases in the experiment
    /// repo.
    pub fn frame_top_reserved_bytes(self) -> u32 {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => crate::isa::rv32::abi::FRAME_TOP_RESERVED_BYTES,
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => crate::isa::xt::abi::FRAME_TOP_RESERVED_BYTES,
        }
    }

    /// True if `val` is a legal immediate for the immediate-form ALU op `op`,
    /// i.e. the backend may emit `AluRRI` rather than materializing `val` into
    /// a register and emitting the three-register form.
    ///
    /// RV32's `OP-IMM` encoding gives every op the same signed 12-bit field, so
    /// the Rv32imac arm ignores `op` and answers with
    /// [`crate::isa::rv32::abi::fits_imm12`]. The opcode is still a parameter
    /// because immediate legality is **per-opcode** on Xtensa, and the future
    /// backport must be able to extend this method rather than replace it:
    ///
    /// - Xtensa has **no `ANDI`/`ORI`/`XORI`** — bitwise-immediate ops have no
    ///   immediate form at all (`NoImmForm` in the experiment repo's per-op
    ///   table, `xt-mini-emit/src/imm.rs`, 34 entries), so every bitwise
    ///   constant must be materialized.
    /// - Ranges differ per op and are mostly not symmetric (e.g. `ADDI`'s
    ///   `-128..=127` vs `ADDMI`'s scaled range vs the shift-amount fields).
    /// - The Xtensa encoder **silently truncates** an out-of-range immediate
    ///   into the encoding field, so an unchecked immediate becomes wrong code
    ///   with no diagnostic. Every immediate must be gated through here before
    ///   it reaches the encoder.
    /// - `extui` carries a **joint** constraint across two fields —
    ///   `shift + width <= 32` — which no single-value predicate can express.
    ///   That is the case a future multi-field variant of this method extends
    ///   to; taking `op` here keeps that an addition rather than a rewrite.
    ///
    /// A `false` answer never needs new code at the call sites: lowering already
    /// falls back to `IConst32` + `AluRRR`, and the peephole simply declines to
    /// fold.
    pub fn alu_imm_fits(self, op: AluImmOp, val: i32) -> bool {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => {
                let _ = op;
                crate::isa::rv32::abi::fits_imm12(val)
            }
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => {
                use crate::isa::xt::imm::{ImmOp, is_legal};
                match op {
                    AluImmOp::Addi => is_legal(ImmOp::Addi, val),
                    // The key Xtensa fact: no andi/ori/xori exist — every
                    // bitwise constant materializes (NoImmForm ⇒ false).
                    AluImmOp::Andi => is_legal(ImmOp::AndImm, val),
                    AluImmOp::Ori => is_legal(ImmOp::OrImm, val),
                    AluImmOp::Xori => is_legal(ImmOp::XorImm, val),
                    AluImmOp::Slli => is_legal(ImmOp::SlliSa, val),
                    AluImmOp::SrliU => is_legal(ImmOp::SrliSa, val),
                    AluImmOp::SraiS => is_legal(ImmOp::SraiSa, val),
                    // No compare-immediate forms on Xtensa either: lowering
                    // materializes and compares register-register.
                    AluImmOp::Slti | AluImmOp::SltiU => false,
                }
            }
        }
    }

    /// Whether this ISA's native integer divide/remainder **traps** when the
    /// divisor is zero.
    ///
    /// LPIR requires integer division and remainder to never trap and to
    /// follow RV32M semantics on the edge cases (`x / 0 == -1`, `x % 0 == x`) —
    /// see `docs/design/lpir/02-core-ops.md` and
    /// `docs/adr/2026-07-30-integer-division-never-traps.md`. RV32M defines
    /// those results in hardware, so `Rv32imac` emits the bare instruction and
    /// answers `false`.
    ///
    /// Xtensa's `QUOS`/`QUOU`/`REMS`/`REMU` raise `EXCCAUSE 6`
    /// (`IntegerDivideByZero`) instead, so lowering must guard them
    /// ([`crate::lower`]). Only the **zero divisor** needs guarding there:
    /// Xtensa's divide already yields `i32::MIN` for `i32::MIN / -1` and `0`
    /// for `i32::MIN % -1`, matching RV32M. Cranelift, by contrast, traps on
    /// both and guards both — the guard set is per-ISA, which is why this is a
    /// named property rather than an assumption baked into shared lowering.
    pub fn integer_div_traps_on_zero(self) -> bool {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => false,
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => true,
        }
    }

    /// How this target implements `float` when the shader is compiled in
    /// [`lpir::FloatMode::F32`] — the float-capability seam.
    ///
    /// `Rv32imac` names a part **without** the F extension (ESP32-C6, Hazard3),
    /// so it answers [`F32Lowering::SoftFloatCalls`]: float ops become direct
    /// calls to the platform soft-float symbols (`__addsf3`, `__mulsf3`, …).
    /// Those symbols are present in every rv32 image we build — on the C6 the
    /// linker resolves them to the chip's **ROM** `rvfplib` routines
    /// (`esp32c6.rom.rvfp.ld`), and in the host emulator's builtins image to
    /// Rust's `compiler_builtins`. See
    /// `docs/adr/2026-07-31-soft-float-via-compiler-builtins.md`.
    ///
    /// `Xtensa` names a part **with** the Floating-Point Coprocessor Option
    /// (ESP32-S3 / LX7 — 26/26 FP instructions confirmed present on desk
    /// silicon, M6-P1), so it answers [`F32Lowering::HardwareFpu`]: float ops
    /// lower to single FP instructions on the flat 16-entry FR file, with the
    /// operations that are *not* one instruction — divide, square root, the
    /// saturating conversions, the transcendentals — routed to the same M5
    /// builtins the soft-float path calls (M7 D4).
    ///
    /// Without `float-f32` it answers [`F32Lowering::Unsupported`], because the
    /// FP register tables and lowering are not linked. It is deliberately *not*
    /// [`F32Lowering::SoftFloatCalls`] in that build: the S3 has a real FPU, and
    /// quietly giving it the slow path would hide a misconfigured image behind
    /// working output.
    ///
    /// The asymmetry with `Rv32imac` is the whole point of this hook. "rv32"
    /// and "Xtensa" are not two dialects of one float story — one has no FPU at
    /// all and one has a full coprocessor, and the answer has to be a named
    /// property of the target rather than an assumption baked into lowering.
    pub fn f32_lowering(self) -> F32Lowering {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => F32Lowering::SoftFloatCalls,
            #[cfg(all(feature = "isa-xt", feature = "float-f32"))]
            IsaTarget::Xtensa => F32Lowering::HardwareFpu,
            #[cfg(all(feature = "isa-xt", not(feature = "float-f32")))]
            IsaTarget::Xtensa => F32Lowering::Unsupported,
        }
    }

    /// `object` crate Architecture for ELF emission.
    pub fn elf_architecture(self) -> Architecture {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => Architecture::Riscv32,
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => Architecture::Xtensa,
        }
    }

    /// e_flags value for ELF header.
    pub fn elf_e_flags(self) -> u32 {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => crate::isa::rv32::link::EF_RISCV_FLOAT_ABI_SOFT,
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => crate::isa::xt::link::EF_XTENSA_NONE,
        }
    }

    /// Caller-saved register indices within `class`'s allocatable pool
    /// (clobbered across calls).
    ///
    /// Per-class because a call clobbers each class's caller-saved bank
    /// independently: on RV32F `ft0..ft11` are clobbered by exactly the same
    /// call that clobbers `t0..t6`, and the allocator must evict from both.
    ///
    /// Xtensa's float answer is the **whole** float pool, which is not
    /// conservatism — M6-P4's static probe of `xtensa-esp32s3-elf-gcc` found
    /// that no FR survives a `call8`, and that toolchain compiles the
    /// `lps-builtins` f32 family this backend calls. Under-reporting here does
    /// not produce a crash: it leaves a live float in a register the callee
    /// overwrites, so the value is wrong only for inputs that happen to
    /// straddle a call.
    ///
    /// Empty for [`RegClass::Float`] on rv32, where soft float keeps every
    /// value in the integer bank and the integer answer already covers it.
    pub fn caller_saved_pool_hw(self, class: RegClass) -> &'static [u8] {
        match class {
            RegClass::Int => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => crate::isa::rv32::gpr::CALLER_SAVED_POOL,
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => crate::isa::xt::gpr::CALLER_SAVED_POOL,
            },
            RegClass::Float => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => &[],
                #[cfg(all(feature = "isa-xt", feature = "float-f32"))]
                IsaTarget::Xtensa => crate::isa::xt::fpr::CALLER_SAVED_POOL,
                #[cfg(all(feature = "isa-xt", not(feature = "float-f32")))]
                IsaTarget::Xtensa => &[],
            },
        }
    }

    /// A register the allocator may use to break a **cycle** when staging call
    /// arguments — never in the allocatable pool, so nothing live is destroyed.
    ///
    /// Needed because an argument's source register can be another argument's
    /// destination. On an ISA whose argument registers are disjoint from the
    /// allocatable pool (rv32: args `a0..a7` = 10..17, pool = 18..31) that never
    /// happens and this is dead weight. On Xtensa the caller-view staging bank
    /// `a10..a15` **is** the caller-saved half of the pool, so it happens
    /// constantly. See `regalloc::walk::sequence_arg_moves`.
    ///
    /// **Integer-class permanently, on both float targets — decided, not
    /// pending.** M4's version of this comment anticipated a `class` parameter
    /// for a float move cycle; M7 establishes that it is not needed and this
    /// hook does not grow one.
    ///
    /// A move cycle can only form among values staged in *argument* registers.
    /// Neither float ABI in this crate puts a float there: Xtensa passes float
    /// arguments in address registers as raw bit patterns (M7 D1) and rv32's
    /// soft float never leaves the integer file at all. So a float vreg never
    /// occupies an argument slot, never participates in an argument-move cycle,
    /// and never needs a float scratch to break one. Adding the parameter
    /// anyway would mean naming a float scratch register — and reserving one
    /// costs a register for the life of the backend (M7 D8 declines to reserve
    /// any).
    ///
    /// This changes only if a target arrives whose ABI passes floats in the
    /// float file — RV32F's `fa0..fa7`, say. That target adds the parameter
    /// *and* a scratch FR in the same change.
    pub fn move_cycle_scratch(self) -> PReg {
        PReg::int(match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::SCRATCH,
            // a9, not a8: `CALL8` writes the mangled return address into a8, so
            // keeping the swap temp clear of it leaves no overlap to reason about.
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => crate::isa::xt::gpr::SCRATCH2,
        })
    }

    /// The `idx`-th scalar return register of `class`, for direct (non-sret)
    /// returns.
    ///
    /// Per-class because the return bank is per-class in every float ABI that
    /// matters: RV32F returns a float in `fa0`, not `a0`, so an f32-returning
    /// call constrains its def to a different register file than an
    /// i32-returning one.
    ///
    /// **`None` for [`RegClass::Float`] on every target here, and that is
    /// settled rather than pending** (M7 D3). Neither float ABI in this crate
    /// returns a value in the float file: rv32's soft float returns a `float`
    /// in `a0` by construction, and Xtensa returns the raw IEEE bit pattern in
    /// an address register because the esp toolchain that compiles our float
    /// builtins does (M6-P4's measured probe). Lowering therefore emits an
    /// explicit [`crate::vinst::VInst::Wfr`] after a float-returning call, so
    /// the call's own def really is integer-class and this hook is asked the
    /// integer question.
    pub fn direct_ret_reg(self, class: RegClass, idx: usize) -> Option<PReg> {
        match class {
            RegClass::Int => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => crate::isa::rv32::gpr::RET_REGS.get(idx).copied(),
                // CALLER view: this hook names where a call's result lands
                // (regalloc/walk.rs allocates call-def vregs here). Under the
                // CALL8 rotation that is a10/a11, NOT the callee-view a2/a3 —
                // the classic two-views trap; see isa/xt/gpr.rs.
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => crate::isa::xt::gpr::CALL_RET_REGS.get(idx).copied(),
            }
            .map(PReg::int),
            RegClass::Float => None,
        }
    }

    /// Count of direct return registers of `class` in the hardware ABI
    /// (e.g. 2 for RV32 a0–a1).
    ///
    /// Zero for [`RegClass::Float`] for the reason spelled out on
    /// [`Self::direct_ret_reg`]: no float return bank exists on either target.
    pub fn direct_ret_reg_count(self, class: RegClass) -> usize {
        match class {
            RegClass::Int => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => crate::isa::rv32::gpr::RET_REGS.len(),
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => crate::isa::xt::gpr::CALL_RET_REGS.len(),
            },
            RegClass::Float => 0,
        }
    }

    /// The `idx`-th incoming call argument register of `class`.
    ///
    /// Per-class for the same reason as [`Self::direct_ret_reg`]: a hard-float
    /// ABI passes float arguments in the float file, and the two banks are
    /// indexed independently.
    ///
    /// **`None` for [`RegClass::Float`] by decision** (M7 D3) — the mirror of
    /// [`Self::direct_ret_reg`]'s note. Float parameters arrive in address
    /// registers and lowering emits a [`crate::vinst::VInst::Wfr`] at function
    /// entry to move each into the float file, so the parameter vreg this hook
    /// precolors is integer-class.
    pub fn call_arg_reg(self, class: RegClass, idx: usize) -> Option<PReg> {
        match class {
            RegClass::Int => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => crate::isa::rv32::gpr::ARG_REGS.get(idx).copied(),
                // CALLEE view (incoming parameters precolor here).
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => crate::isa::xt::gpr::ARG_REGS.get(idx).copied(),
            }
            .map(PReg::int),
            RegClass::Float => None,
        }
    }

    /// Number of argument registers of `class` in the hardware calling convention.
    ///
    /// Zero for [`RegClass::Float`]; see [`Self::call_arg_reg`].
    pub fn call_arg_reg_count(self, class: RegClass) -> usize {
        match class {
            RegClass::Int => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => crate::isa::rv32::gpr::ARG_REGS.len(),
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => crate::isa::xt::gpr::ARG_REGS.len(),
            },
            RegClass::Float => 0,
        }
    }

    /// First LPIR call-arg index that spills to the outgoing stack area.
    ///
    /// Legacy sret callees reserve `a0` for a struct-return pointer injected by the
    /// emitter, so only seven argument registers remain for explicit operands.
    /// M1 sret passes that pointer as the second LPIR arg, so all eight `a*` regs
    /// are available for the first eight operands.
    pub fn lpir_call_stack_args_start(
        self,
        callee_uses_sret: bool,
        caller_passes_sret_ptr: bool,
    ) -> usize {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => {
                if callee_uses_sret && !caller_passes_sret_ptr {
                    crate::isa::rv32::abi::ARG_REGS.len() - 1
                } else {
                    crate::isa::rv32::abi::ARG_REGS.len()
                }
            }
            // rv32's exact formula over 6 argument registers (BACKPORT.md:
            // the rotation is invisible to slot mapping).
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => {
                if callee_uses_sret && !caller_passes_sret_ptr {
                    crate::isa::xt::abi::ARG_REGS.len() - 1
                } else {
                    crate::isa::xt::abi::ARG_REGS.len()
                }
            }
        }
    }

    /// Target register for the `arg_index`-th LPIR [`VInst::Call`] operand
    /// (RV32 `a0`–`a7`), or `None` when the operand is stack-passed.
    ///
    /// `class` is the class of the *operand*, and it selects the register bank:
    /// a hard-float ABI stages a float argument in the float file.
    ///
    /// **`None` for [`RegClass::Float`] permanently on both targets** (M7 D3),
    /// not as a stub. Lowering emits a [`crate::vinst::VInst::Rfr`] before each
    /// float call argument, so a `Call`'s `args` slice is entirely
    /// integer-class by the time the allocator reads it and this early return
    /// is unreachable in well-formed output. It stays as a hard floor: a float
    /// vreg that somehow reached an argument slot must fail the stack-pass
    /// path loudly rather than be staged into a GPR and passed as an integer.
    /// The slot arithmetic below (the sret/vmctx shuffles) is class-independent
    /// and would be reused as-is if a float argument bank ever landed.
    pub fn lpir_call_arg_target(
        self,
        class: RegClass,
        callee_uses_sret: bool,
        caller_passes_sret_ptr: bool,
        caller_sret_vm_abi_swap: bool,
        arg_index: usize,
    ) -> Option<PReg> {
        if class == RegClass::Float {
            return None;
        }
        let hw = match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => {
                let slot = if !callee_uses_sret {
                    arg_index
                } else if !caller_passes_sret_ptr {
                    1usize.saturating_add(arg_index)
                } else if caller_sret_vm_abi_swap {
                    // Shader / `needs_vmctx` path: [vmctx, sret, …] → [a1, a0, a2, …].
                    match arg_index {
                        0 => 1,
                        1 => 0,
                        i => i,
                    }
                } else {
                    // `@texture::*` imports: [`ImportDecl::needs_vmctx`] is false — first operand is the
                    // callee sret destination; map linearly onto `a0`, `a1`, …
                    arg_index
                };
                crate::isa::rv32::abi::ARG_REGS.get(slot).map(|p| p.hw)
            }
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => {
                // Same slot computation as rv32; the target registers are the
                // CALLER-view staging bank a10..a15 (the callee's ENTRY
                // rotates them into its a2..a7). A parameter that arrived in
                // callee-view a2..a7 can never "pass through" to a call arg —
                // the rotation physically moves values — and the passthrough
                // check in regalloc/walk.rs correctly never matches.
                let slot = if !callee_uses_sret {
                    arg_index
                } else if !caller_passes_sret_ptr {
                    1usize.saturating_add(arg_index)
                } else if caller_sret_vm_abi_swap {
                    // Shader / `needs_vmctx` path: [vmctx, sret, …] →
                    // [a11, a10, a12, …] (the rotation image of rv32's a1/a0).
                    match arg_index {
                        0 => 1,
                        1 => 0,
                        i => i,
                    }
                } else {
                    arg_index
                };
                crate::isa::xt::gpr::OUT_ARG_REGS.get(slot).copied()
            }
        };
        hw.map(PReg::int)
    }

    /// The ISA of the CPU this crate is being compiled for.
    ///
    /// Only defined for architectures the on-device JIT supports — host builds
    /// always name their target explicitly. The `xtensa` arm lands with the
    /// ESP32-S3 backport.
    #[cfg(target_arch = "riscv32")]
    pub fn native() -> IsaTarget {
        IsaTarget::Rv32imac
    }

    /// See the sibling arm above — one `fn native` per JIT-capable CPU.
    #[cfg(target_arch = "xtensa")]
    pub fn native() -> IsaTarget {
        IsaTarget::Xtensa
    }

    /// ELF / JIT relocation type for a direct call to a named symbol.
    pub fn call_reloc_type(self) -> u32 {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => crate::isa::rv32::link::R_RISCV_CALL_PLT,
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => crate::isa::xt::link::R_XTENSA_32,
        }
    }

    /// Emit one function's machine code with the target's backend emitter.
    pub(crate) fn emit_function(
        self,
        vinsts: &[VInst],
        vreg_pool: &[VReg],
        output: &AllocOutput,
        frame: FrameLayout,
        symbols: &ModuleSymbols,
        is_sret: bool,
        collect_debug_lines: bool,
    ) -> Result<IsaEmitOutput, AllocError> {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => crate::isa::rv32::emit::emit_function(
                vinsts,
                vreg_pool,
                output,
                frame,
                symbols,
                is_sret,
                collect_debug_lines,
            ),
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => crate::isa::xt::emit::emit_function(
                vinsts,
                vreg_pool,
                output,
                frame,
                symbols,
                is_sret,
                collect_debug_lines,
            ),
        }
    }

    /// Render one instruction word as assembly text (debug output only).
    pub fn format_instruction(self, word: u32) -> String {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => lp_riscv_inst::format_instruction(word),
            // Xtensa instructions are 2 or 3 bytes; render the word's low
            // three little-endian bytes at pc 0 (callers with real buffers
            // use `format_instruction_at`).
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => {
                let bytes = word.to_le_bytes();
                lp_xt_inst::disasm::format_instruction(&bytes[..3], 0)
            }
        }
    }

    /// Disassemble the one instruction at the start of `bytes`, returning its
    /// text and its **encoded length**.
    ///
    /// Callers walking a code buffer must advance by the returned length rather
    /// than a fixed stride: RV32 instructions are a uniform 4 bytes, but Xtensa
    /// mixes 24-bit and 16-bit (narrow) encodings, so the length is only known
    /// after decoding. Returns `None` when `bytes` is too short to hold a
    /// complete instruction.
    pub fn format_instruction_at(self, bytes: &[u8]) -> Option<(String, usize)> {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => {
                let word = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
                Some((lp_riscv_inst::format_instruction(word), 4))
            }
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => {
                // Variable-width: the density rule on the first byte decides
                // 2 vs 3 bytes, known only after decoding.
                let len = match lp_xt_inst::decode(bytes) {
                    Ok((_, len)) => len,
                    Err(lp_xt_inst::DecodeError::Unsupported { len, .. }) => len,
                    Err(_) => return None,
                };
                if bytes.len() < len {
                    return None;
                }
                Some((
                    lp_xt_inst::disasm::format_instruction(&bytes[..len], 0),
                    len,
                ))
            }
        }
    }

    /// Annotated assembly listing for one compiled function (host debugging).
    ///
    /// `debug_lines` is the emitter's `(code_offset, optional_src_op)` table.
    pub fn disassemble_function(
        self,
        code: &[u8],
        debug_lines: &[(u32, Option<u32>)],
        func: &IrFunction,
        opts: DisasmOptions,
    ) -> String {
        match self {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => {
                let table = crate::isa::rv32::debug::LineTable::from_debug_lines(debug_lines);
                crate::isa::rv32::debug::disasm::disassemble_function(code, &table, func, opts)
            }
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => {
                // Plain variable-width listing; the annotated rv32-style
                // interleave arrives with the xt debug pass when a consumer
                // needs it (filetest asm snapshots use format_instruction_at).
                let _ = (func, opts);
                let mut out = String::new();
                let mut off = 0usize;
                while off < code.len() {
                    let Some((text, len)) = self.format_instruction_at(&code[off..]) else {
                        break;
                    };
                    let src = debug_lines
                        .iter()
                        .find(|(o, _)| *o as usize == off)
                        .and_then(|(_, s)| *s);
                    use core::fmt::Write as _;
                    match src {
                        Some(s) => {
                            let _ = writeln!(out, "{off:6x}:  {text}    ; op{s}");
                        }
                        None => {
                            let _ = writeln!(out, "{off:6x}:  {text}");
                        }
                    }
                    off += len;
                }
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_isa() -> alloc::vec::Vec<IsaTarget> {
        alloc::vec![
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac,
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa,
        ]
    }

    /// The float-capability seam's whole job: **exactly one** target may claim
    /// a hardware FPU, and only in a build that links the FP tables.
    ///
    /// M9 asserted that *nothing* answered `HardwareFpu`, because nothing could
    /// encode an FP instruction. M7 makes Xtensa the one that does — the S3 has
    /// the Floating-Point Coprocessor Option and M6-P1 confirmed all 26
    /// instructions present on silicon. The assertion is kept, not deleted,
    /// with the claim narrowed: any *other* target answering `HardwareFpu` is a
    /// part that would take an illegal-instruction trap on the first `fadd.s`.
    #[test]
    fn only_xtensa_with_float_f32_claims_a_hardware_fpu() {
        for isa in every_isa() {
            let claims_hardware = isa.f32_lowering() == F32Lowering::HardwareFpu;
            let may_claim_hardware = cfg!(feature = "isa-xt")
                && cfg!(feature = "float-f32")
                && alloc::format!("{isa:?}") == "Xtensa";
            assert_eq!(
                claims_hardware, may_claim_hardware,
                "{isa:?}: hardware-FPU claim does not match what this build can encode"
            );
        }
    }

    /// The gate, from the capability side: with `float-f32` off there is no FP
    /// emitter and no FR pool linked, so the S3 must report `Unsupported`
    /// rather than fall back to soft float. A silent fallback would compile,
    /// run, produce right answers, and be ~30x slower than the part is capable
    /// of, with nothing pointing at why.
    #[cfg(all(feature = "isa-xt", not(feature = "float-f32")))]
    #[test]
    fn xtensa_without_the_feature_is_unsupported_not_soft_float() {
        assert_eq!(IsaTarget::Xtensa.f32_lowering(), F32Lowering::Unsupported);
    }

    /// The C6 is the reference rv32 part and has no F extension; soft calls are
    /// the only correct answer for it.
    #[cfg(feature = "isa-rv32")]
    #[test]
    fn rv32imac_is_soft_float() {
        assert_eq!(
            IsaTarget::Rv32imac.f32_lowering(),
            F32Lowering::SoftFloatCalls
        );
    }

    /// Soft float keeps every value integer-class, so rv32's float pool stays
    /// empty. A float vreg reaching the allocator there fails loudly with
    /// `OutOfRegisters` rather than quietly landing an f32 bit pattern in a GPR
    /// that some later pass treats as an integer.
    #[cfg(feature = "isa-rv32")]
    #[test]
    fn rv32_has_no_float_register_pool() {
        let isa = IsaTarget::Rv32imac;
        assert!(isa.allocatable_pool_order(RegClass::Float).is_empty());
        assert!(isa.caller_saved_pool_hw(RegClass::Float).is_empty());
        assert!(!isa.is_in_allocatable_pool(PReg::float(0)));
    }

    /// The float pool is the *feature's* observable footprint in the register
    /// model: 15 of the 16 FRs when `float-f32` is on (`f15` is the emitter's
    /// scratch — see `isa::xt::fpr`), nothing at all when it is off. The off
    /// case is the gate's whole claim (M7 D9) — a leak here would mean a
    /// Fixed-only image carries the float allocator.
    #[cfg(feature = "isa-xt")]
    #[test]
    fn xtensa_float_pool_follows_the_feature() {
        let isa = IsaTarget::Xtensa;
        let pool = isa.allocatable_pool_order(RegClass::Float);
        if cfg!(feature = "float-f32") {
            assert_eq!(pool.len(), 15, "15 FRs allocatable, f15 is the scratch");
            // Every FR is call-clobbered — the measured esp-toolchain ABI.
            assert_eq!(isa.caller_saved_pool_hw(RegClass::Float), pool);
            assert!(isa.is_in_allocatable_pool(PReg::float(14)));
            assert!(
                !isa.is_in_allocatable_pool(PReg::float(15)),
                "the emitter scratch must never be handed to the allocator"
            );
            assert_eq!(isa.reg_name(PReg::float(3)), "f3");
        } else {
            assert!(pool.is_empty());
            assert!(isa.caller_saved_pool_hw(RegClass::Float).is_empty());
            assert!(!isa.is_in_allocatable_pool(PReg::float(0)));
        }
    }

    /// The same hardware index in the two classes must not be confused: `f3`
    /// and `a3` are different registers, and `reg_name` is what a spill trace
    /// or an alloc dump shows a human debugging a wrong pixel.
    #[cfg(all(feature = "isa-xt", feature = "float-f32"))]
    #[test]
    fn float_and_int_register_names_do_not_collide() {
        let isa = IsaTarget::Xtensa;
        for hw in 0..16u8 {
            assert_ne!(isa.reg_name(PReg::int(hw)), isa.reg_name(PReg::float(hw)));
        }
    }

    /// No target has a float **ABI** bank, and unlike the pool this is not
    /// waiting on a feature — it is M7 D3. Float values cross every call
    /// boundary in address registers on both float targets, so a float vreg
    /// never occupies an argument or return slot. If one of these ever answers
    /// non-zero, the `Rfr`/`Wfr` transfers lowering inserts have become
    /// redundant and the calling convention changed.
    #[test]
    fn no_target_has_a_float_abi_bank() {
        for isa in every_isa() {
            assert_eq!(isa.direct_ret_reg_count(RegClass::Float), 0);
            assert_eq!(isa.call_arg_reg_count(RegClass::Float), 0);
            assert_eq!(isa.direct_ret_reg(RegClass::Float, 0), None);
            assert_eq!(isa.call_arg_reg(RegClass::Float, 0), None);
            assert_eq!(
                isa.lpir_call_arg_target(RegClass::Float, false, false, false, 0),
                None
            );
            assert_eq!(isa.move_cycle_scratch().class, RegClass::Int);
        }
    }
}
