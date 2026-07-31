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

impl IsaTarget {
    /// Pool-init order for the register allocator's LRU, for one register class.
    ///
    /// Every register hook on this type is a **per-class** query, because the
    /// allocator runs one independent pool per [`RegClass`]: an integer vreg can
    /// never satisfy a float constraint, and the same hardware encoding names
    /// two different registers in the two classes.
    ///
    /// Neither backend has a float pool yet — hardware-FPU codegen is the
    /// Xtensa f32 milestone and RV32F the rv32 one — so [`RegClass::Float`]
    /// answers with an empty pool everywhere. An empty pool is not a silent
    /// fallback: a float vreg reaching the allocator on such a target fails
    /// with [`AllocError::OutOfRegisters`] rather than landing in a GPR.
    ///
    /// Note that a Q32 shader has no float vregs at all — a Q16.16 `float` is
    /// an integer and lowers to integer instructions — so this is the honest
    /// answer for the fixed-point path, not a placeholder for it.
    pub fn allocatable_pool_order(self, class: RegClass) -> &'static [u8] {
        match class {
            RegClass::Int => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => crate::isa::rv32::gpr::ALLOC_POOL,
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => crate::isa::xt::gpr::ALLOC_POOL,
            },
            RegClass::Float => &[],
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
            RegClass::Float => false,
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
            // No backend names float registers yet; the ISA-specific float
            // tables arrive with their emitters. Rendering must not panic, so
            // this stays a legible placeholder rather than an `unreachable!`.
            RegClass::Float => "f?",
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
    /// Empty for [`RegClass::Float`] while the float pools are empty.
    pub fn caller_saved_pool_hw(self, class: RegClass) -> &'static [u8] {
        match class {
            RegClass::Int => match self {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => crate::isa::rv32::gpr::CALLER_SAVED_POOL,
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => crate::isa::xt::gpr::CALLER_SAVED_POOL,
            },
            RegClass::Float => &[],
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
    /// Integer-class today. A float move cycle needs a float scratch, and that
    /// register is named by whichever backend first has float argument
    /// registers to shuffle; this hook grows a `class` parameter then rather
    /// than answering with a GPR that cannot hold the value.
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
    /// i32-returning one. `None` for [`RegClass::Float`] until a backend has
    /// one.
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
    /// a hard-float ABI stages a float argument in the float file. Answering
    /// `None` for [`RegClass::Float`] is what makes an unimplemented float
    /// argument fall into the stack-pass path's error rather than into a GPR;
    /// the slot arithmetic below (the sret/vmctx shuffles) is class-independent
    /// and is reused as-is when the float banks land.
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
