pub mod rv32;

use alloc::string::String;

use lpir::IrFunction;
use object::Architecture;

use crate::abi::FrameLayout;
use crate::regalloc::{AllocError, AllocOutput};
use crate::vinst::{AluImmOp, ModuleSymbols, VInst, VReg};

/// Options for annotated disassembly text.
///
/// Re-exported here so callers outside `isa::` never name an ISA-specific
/// module; the type itself lives with the RV32 disassembler for now.
pub use rv32::debug::disasm::DisasmOptions;

/// Backend emitter output: machine code, call relocations, and debug lines.
///
/// The shape is ISA-neutral even though RV32 owns the struct today; a second
/// backend reuses it rather than introducing a parallel type.
pub(crate) type IsaEmitOutput = rv32::emit::Rv32EmitOutput;

/// The target ISA + sub-architecture for a compiled module.
///
/// Variant names describe the **target hardware**, not the codegen output.
/// `Rv32imac` is the ESP32-C6 target (`riscv32imac-unknown-none-elf`); the
/// emitter currently produces only base RV32IM instructions. The A and C
/// extensions appear in the target name because the firmware runtime uses
/// them, not because we emit them.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum IsaTarget {
    Rv32imac,
}

impl IsaTarget {
    /// Pool-init order for the register allocator's LRU.
    pub fn allocatable_pool_order(self) -> &'static [u8] {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::ALLOC_POOL,
        }
    }

    /// True if `p` is in the allocatable register pool.
    pub fn is_in_allocatable_pool(self, p: u8) -> bool {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::pool_contains(p),
        }
    }

    /// Human-readable name for `p` (debug rendering only).
    pub fn reg_name(self, p: u8) -> &'static str {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::reg_name(p),
        }
    }

    /// True if a return value with `scalar_count` scalars uses the
    /// sret-via-buffer convention rather than direct registers.
    pub fn sret_uses_buffer_for(self, scalar_count: u32) -> bool {
        match self {
            IsaTarget::Rv32imac => {
                (scalar_count as usize) > crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD
            }
        }
    }

    /// Minimum stack frame alignment in bytes.
    pub fn stack_alignment(self) -> u32 {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::abi::STACK_ALIGNMENT,
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
            IsaTarget::Rv32imac => crate::isa::rv32::abi::FRAME_TOP_RESERVED_BYTES,
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
            IsaTarget::Rv32imac => {
                let _ = op;
                crate::isa::rv32::abi::fits_imm12(val)
            }
        }
    }

    /// `object` crate Architecture for ELF emission.
    pub fn elf_architecture(self) -> Architecture {
        match self {
            IsaTarget::Rv32imac => Architecture::Riscv32,
        }
    }

    /// e_flags value for ELF header.
    pub fn elf_e_flags(self) -> u32 {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::link::EF_RISCV_FLOAT_ABI_SOFT,
        }
    }

    /// Caller-saved GPR indices within the allocatable pool (clobbered across calls).
    pub fn caller_saved_pool_hw(self) -> &'static [u8] {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::CALLER_SAVED_POOL,
        }
    }

    /// Hardware index for the `idx`-th scalar return register for direct (non-sret) returns.
    pub fn direct_ret_reg_hw(self, idx: usize) -> Option<u8> {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::RET_REGS.get(idx).copied(),
        }
    }

    /// Count of direct return registers in the hardware ABI (e.g. 2 for RV32 a0–a1).
    pub fn direct_ret_reg_count(self) -> usize {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::RET_REGS.len(),
        }
    }

    /// Hardware index for the `idx`-th incoming call argument register.
    pub fn call_arg_reg_hw(self, idx: usize) -> Option<u8> {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::ARG_REGS.get(idx).copied(),
        }
    }

    /// Number of argument registers in the hardware calling convention.
    pub fn call_arg_reg_count(self) -> usize {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::ARG_REGS.len(),
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
            IsaTarget::Rv32imac => {
                if callee_uses_sret && !caller_passes_sret_ptr {
                    crate::isa::rv32::abi::ARG_REGS.len() - 1
                } else {
                    crate::isa::rv32::abi::ARG_REGS.len()
                }
            }
        }
    }

    /// Target hardware GPR for the `arg_index`-th LPIR [`VInst::Call`] operand
    /// (RV32 `a0`–`a7`), or `None` when the operand is stack-passed.
    pub fn lpir_call_arg_target_hw(
        self,
        callee_uses_sret: bool,
        caller_passes_sret_ptr: bool,
        caller_sret_vm_abi_swap: bool,
        arg_index: usize,
    ) -> Option<u8> {
        match self {
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
        }
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

    /// ELF / JIT relocation type for a direct call to a named symbol.
    pub fn call_reloc_type(self) -> u32 {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::link::R_RISCV_CALL_PLT,
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
            IsaTarget::Rv32imac => crate::isa::rv32::emit::emit_function(
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
            IsaTarget::Rv32imac => lp_riscv_inst::format_instruction(word),
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
            IsaTarget::Rv32imac => {
                let word = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
                Some((lp_riscv_inst::format_instruction(word), 4))
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
            IsaTarget::Rv32imac => {
                let table = crate::isa::rv32::debug::LineTable::from_debug_lines(debug_lines);
                crate::isa::rv32::debug::disasm::disassemble_function(code, &table, func, opts)
            }
        }
    }
}
