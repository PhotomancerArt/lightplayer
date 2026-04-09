//! RV32 emission: machine code, relocations, ELF object (`object` crate).

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use object::write::{Object, Relocation, StandardSection, Symbol, SymbolId, SymbolSection};
use object::{
    Architecture, BinaryFormat, Endianness, FileFlags, SymbolFlags, SymbolKind, SymbolScope, elf,
};

use super::abi::{self, A0, ARG_REGS, AbiInfo, FrameLayout, PhysReg, RET_REGS, S0, SP, SRET_PTR};
use super::inst::{
    encode_addi, encode_auipc, encode_jalr, encode_lw, encode_mul, encode_ret, encode_sub,
    encode_sw, iconst32_sequence,
};
use crate::error::NativeError;
use crate::regalloc::{Allocation, GreedyAlloc};
use crate::vinst::VInst;
use lpir::VReg;
use lps_shared;

/// Byte offset in `.text` where a relocation applies (at the `auipc` of an auipc+jalr pair).
#[derive(Clone, Debug)]
pub struct NativeReloc {
    pub offset: usize,
    pub symbol: String,
}

/// Machine code for one function plus relocations and optional debug line map.
#[derive(Debug)]
pub struct EmittedFunction {
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
    /// When [`EmitContext`] was built with `debug_info`, maps each instruction's byte offset to an LPIR op index.
    pub debug_lines: Vec<(u32, Option<u32>)>,
}

#[derive(Debug)]
pub struct EmitContext {
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
    pub debug_lines: Vec<(u32, Option<u32>)>,
    frame: FrameLayout,
    debug_info: bool,
    current_src_op: Option<u32>,
}

impl EmitContext {
    /// Create a new emit context with the given frame layout.
    pub fn with_frame(frame: FrameLayout, debug_info: bool) -> Self {
        Self {
            code: Vec::new(),
            relocs: Vec::new(),
            debug_lines: Vec::new(),
            frame,
            debug_info,
            current_src_op: None,
        }
    }

    /// Create a new emit context for a leaf function.
    pub fn new(is_leaf: bool, debug_info: bool) -> Self {
        let frame = if is_leaf {
            abi::leaf_frame()
        } else {
            abi::nonleaf_frame(0)
        };
        Self::with_frame(frame, debug_info)
    }

    fn push_u32(&mut self, w: u32) {
        let offset = self.code.len() as u32;
        self.code.extend_from_slice(&w.to_le_bytes());
        if self.debug_info && self.current_src_op.is_some() {
            self.debug_lines.push((offset, self.current_src_op));
        }
    }

    /// Emit function prologue: adjust sp, save ra and s0 if needed.
    /// For sret functions, also saves the sret pointer (in a0) to s1.
    pub fn emit_prologue(&mut self, abi_info: &AbiInfo) {
        let sp = u32::from(SP);
        let frame_size = self.frame.size as i32;

        // Adjust stack pointer
        self.push_u32(encode_addi(sp, sp, -frame_size));

        // Save s0 (frame pointer) if we have one
        if self.frame.saved_s0 {
            let s0_off = self.frame.size as i32 + self.frame.s0_save_offset;
            self.push_u32(encode_sw(u32::from(S0), sp, s0_off));
        }

        // Save ra if non-leaf
        if self.frame.saved_ra {
            let ra_off = self.frame.size as i32 + self.frame.ra_save_offset;
            self.push_u32(encode_sw(u32::from(abi::RA), sp, ra_off));
        }

        // Set up frame pointer: s0 = sp + frame_size
        if self.frame.saved_s0 {
            let sp = u32::from(SP);
            let s0 = u32::from(S0);
            self.push_u32(encode_addi(s0, sp, frame_size));
        }

        // For sret functions: save sret pointer (a0) to callee-saved s1
        // This preserves it across the function body which may clobber a0
        if abi_info.is_sret() {
            let a0 = u32::from(A0);
            let s1 = u32::from(SRET_PTR);
            self.push_u32(encode_addi(s1, a0, 0)); // mv s1, a0
        }
    }

    /// Emit function epilogue: restore ra/s0, adjust sp, return.
    ///
    /// For sret functions, the return values have already been stored to the
    /// buffer pointed to by a0. Normal return just executes ret.
    pub fn emit_epilogue(&mut self, _abi_info: &AbiInfo) {
        let sp = u32::from(SP);
        let frame_size = self.frame.size as i32;

        // Restore ra if saved
        if self.frame.saved_ra {
            let ra_off = self.frame.size as i32 + self.frame.ra_save_offset;
            self.push_u32(encode_lw(u32::from(abi::RA), sp, ra_off));
        }

        // Restore s0 if saved
        if self.frame.saved_s0 {
            let s0_off = self.frame.size as i32 + self.frame.s0_save_offset;
            self.push_u32(encode_lw(u32::from(S0), sp, s0_off));
        }

        // Adjust stack pointer back
        self.push_u32(encode_addi(sp, sp, frame_size));
        self.push_u32(encode_ret());
    }

    /// Get the physical register for a vreg.
    /// Returns Err if the vreg is not assigned (shouldn't happen after successful regalloc).
    fn phys(alloc: &Allocation, v: VReg) -> Result<PhysReg, NativeError> {
        let i = v.0 as usize;
        alloc
            .vreg_to_phys
            .get(i)
            .copied()
            .flatten()
            .ok_or_else(|| NativeError::UnassignedVReg(v.0))
    }

    /// Temporary registers for spill handling.
    const TEMP0: PhysReg = 5; // t0
    const TEMP1: PhysReg = 6; // t1

    /// Emit a load from a spill slot into a temporary register.
    /// Returns the temporary register.
    fn load_spill(&mut self, slot_index: u32, temp: PhysReg) -> PhysReg {
        let offset = self.frame.spill_to_offset(slot_index);
        self.push_u32(encode_lw(u32::from(temp), u32::from(S0), offset));
        temp
    }

    /// Emit a store from a temporary register to a spill slot.
    fn store_spill(&mut self, slot_index: u32, temp: PhysReg) {
        let offset = self.frame.spill_to_offset(slot_index);
        self.push_u32(encode_sw(u32::from(temp), u32::from(S0), offset));
    }

    /// Get or load a vreg for use (source operand).
    /// If the vreg is spilled, loads it into the specified temp register.
    /// Otherwise returns the assigned physical register.
    fn use_vreg(
        &mut self,
        alloc: &Allocation,
        v: VReg,
        temp: PhysReg,
    ) -> Result<PhysReg, NativeError> {
        if let Some(slot_index) = alloc.spill_slot(v) {
            // VReg is spilled - load from stack into temp register
            Ok(self.load_spill(slot_index, temp))
        } else {
            // VReg has a physical register
            Self::phys(alloc, v)
        }
    }

    /// Get or reserve a vreg for definition (destination operand).
    /// If the vreg is spilled, returns the temp register (caller must store after use).
    /// Otherwise returns the assigned physical register.
    fn def_vreg(
        &mut self,
        alloc: &Allocation,
        v: VReg,
        temp: PhysReg,
    ) -> Result<PhysReg, NativeError> {
        if alloc.is_spilled(v) {
            // VReg is spilled - use temp as temporary, caller must store
            Ok(temp)
        } else {
            // VReg has a physical register
            Self::phys(alloc, v)
        }
    }

    /// Store a spilled vreg after it was written to a temporary register.
    /// Call this after `def_vreg` when the vreg was spilled.
    fn store_def_vreg(&mut self, alloc: &Allocation, v: VReg, temp: PhysReg) {
        if let Some(slot_index) = alloc.spill_slot(v) {
            // VReg was spilled - store temp to stack
            self.store_spill(slot_index, temp);
        }
    }

    pub fn emit_vinst(
        &mut self,
        inst: &VInst,
        alloc: &Allocation,
        abi_info: &AbiInfo,
    ) -> Result<(), NativeError> {
        self.current_src_op = inst.src_op();
        match inst {
            VInst::Add32 {
                dst, src1, src2, ..
            } => {
                // Use TEMP0 for src1, TEMP1 for src2 if spilled
                let rs1 = self.use_vreg(alloc, *src1, Self::TEMP0)? as u32;
                let rs2 = self.use_vreg(alloc, *src2, Self::TEMP1)? as u32;
                // Result can go to TEMP0 if dst is spilled
                let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
                self.push_u32(crate::isa::rv32::inst::encode_add(rd, rs1, rs2));
                self.store_def_vreg(alloc, *dst, Self::TEMP0);
            }
            VInst::Sub32 {
                dst, src1, src2, ..
            } => {
                let rs1 = self.use_vreg(alloc, *src1, Self::TEMP0)? as u32;
                let rs2 = self.use_vreg(alloc, *src2, Self::TEMP1)? as u32;
                let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_sub(rd, rs1, rs2));
                self.store_def_vreg(alloc, *dst, Self::TEMP0);
            }
            VInst::Mul32 {
                dst, src1, src2, ..
            } => {
                let rs1 = self.use_vreg(alloc, *src1, Self::TEMP0)? as u32;
                let rs2 = self.use_vreg(alloc, *src2, Self::TEMP1)? as u32;
                let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_mul(rd, rs1, rs2));
                self.store_def_vreg(alloc, *dst, Self::TEMP0);
            }
            VInst::Mov32 { dst, src, .. } => {
                let rs = self.use_vreg(alloc, *src, Self::TEMP0)? as u32;
                let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
                if rd != rs {
                    self.push_u32(encode_addi(rd, rs, 0));
                }
                self.store_def_vreg(alloc, *dst, Self::TEMP0);
            }
            VInst::Load32 {
                dst, base, offset, ..
            } => {
                // base must not use TEMP0 if dst will use TEMP0
                // For simplicity: load base first (into TEMP1), then use TEMP0 for result
                let rs1 = self.use_vreg(alloc, *base, Self::TEMP1)? as u32;
                let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_lw(rd, rs1, *offset));
                self.store_def_vreg(alloc, *dst, Self::TEMP0);
            }
            VInst::Store32 {
                src, base, offset, ..
            } => {
                let rs2 = self.use_vreg(alloc, *src, Self::TEMP0)? as u32;
                let rs1 = self.use_vreg(alloc, *base, Self::TEMP1)? as u32;
                self.push_u32(encode_sw(rs2, rs1, *offset));
            }
            VInst::IConst32 { dst, val, .. } => {
                let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
                for w in iconst32_sequence(rd, *val) {
                    self.push_u32(w);
                }
                self.store_def_vreg(alloc, *dst, Self::TEMP0);
            }
            VInst::Call {
                target, args, rets, ..
            } => {
                if args.len() > ARG_REGS.len() {
                    return Err(NativeError::TooManyArgs(args.len()));
                }
                for (i, a) in args.iter().enumerate() {
                    let from = self.use_vreg(alloc, *a, Self::TEMP0)? as u32;
                    let to = ARG_REGS[i] as u32;
                    if from != to {
                        self.push_u32(encode_addi(to, from, 0));
                    }
                }
                let auipc_off = self.code.len();
                let ra = u32::from(abi::RA);
                self.push_u32(encode_auipc(ra, 0));
                self.push_u32(encode_jalr(ra, ra, 0));
                self.relocs.push(NativeReloc {
                    offset: auipc_off,
                    symbol: target.name.clone(),
                });
                for (i, r) in rets.iter().enumerate() {
                    if i >= RET_REGS.len() {
                        return Err(NativeError::TooManyReturns(i + 1));
                    }
                    let dst = self.def_vreg(alloc, *r, Self::TEMP0)? as u32;
                    let src = RET_REGS[i] as u32;
                    if dst != src {
                        self.push_u32(encode_addi(dst, src, 0));
                    }
                    self.store_def_vreg(alloc, *r, Self::TEMP0);
                }
            }
            VInst::Ret { vals, .. } => {
                if abi_info.is_sret() {
                    // Sret: store values to buffer pointed to by s1
                    // s1 was loaded with the sret buffer address in the prologue
                    // (since a0 may be clobbered during function execution)
                    let base_reg = SRET_PTR as u32; // s1
                    for (i, v) in vals.iter().enumerate() {
                        let src = self.use_vreg(alloc, *v, Self::TEMP0)? as u32;
                        let offset = (i * 4) as i32;
                        // Store each scalar to s1 + offset
                        self.push_u32(encode_sw(src, base_reg, offset));
                    }
                    // Return value buffer address is already in a0 per ABI
                } else {
                    // Direct return: move values to a0-a3
                    for (i, v) in vals.iter().enumerate() {
                        let src = self.use_vreg(alloc, *v, Self::TEMP0)? as u32;
                        let dst = RET_REGS[i] as u32;
                        if src != dst {
                            self.push_u32(encode_addi(dst, src, 0));
                        }
                    }
                }
            }
            VInst::Label(..) => {}
        }
        self.current_src_op = None;
        Ok(())
    }
}

/// Emit one function to RV32 bytes (and relocations). Used by ELF writer and debug assembly.
///
/// # Arguments
/// * `func` - The LPIR function to emit
/// * `fn_sig` - Surface signature (ABI classification, must match `func` parameter layout)
/// * `float_mode` - Floating point mode (Q32 or SoftFloat)
/// * `debug_info` - Whether to include debug line information
pub fn emit_function_bytes(
    func: &lpir::IrFunction,
    fn_sig: &lps_shared::LpsFnSig,
    float_mode: lpir::FloatMode,
    debug_info: bool,
) -> Result<EmittedFunction, NativeError> {
    let abi_info = AbiInfo::from_lps_sig(fn_sig);
    let vinsts = crate::lower::lower_ops(func, float_mode)?;
    let slots = func.total_param_slots() as usize;
    let func_abi = super::abi2::func_abi_rv32(fn_sig, slots);
    let alloc = GreedyAlloc::new().allocate_with_func_abi(func, &vinsts, &func_abi)?;
    let is_leaf = !vinsts.iter().any(|v| v.is_call());

    // Create frame with spill count from allocation
    let frame = if is_leaf && alloc.spill_count() == 0 {
        abi::leaf_frame()
    } else {
        abi::nonleaf_frame(alloc.spill_count())
    };

    let mut ctx = EmitContext::with_frame(frame, debug_info);
    ctx.emit_prologue(&abi_info);
    for v in &vinsts {
        ctx.emit_vinst(v, &alloc, &abi_info)?;
    }
    ctx.emit_epilogue(&abi_info);
    Ok(EmittedFunction {
        code: ctx.code,
        relocs: ctx.relocs,
        debug_lines: ctx.debug_lines,
    })
}

/// Append all local functions from `ir` into one RV32 ELF relocatable object.
///
/// # Arguments
/// * `ir` - The LPIR module to emit
/// * `sig` - Module signatures containing function metadata (for ABI classification)
/// * `float_mode` - Floating point mode (Q32 or SoftFloat)
pub fn emit_module_elf(
    ir: &lpir::IrModule,
    sig: &lps_shared::LpsModuleSig,
    float_mode: lpir::FloatMode,
) -> Result<Vec<u8>, NativeError> {
    if ir.functions.is_empty() {
        return Err(NativeError::EmptyModule);
    }

    // Build a map from function name to LpsFnSig for ABI classification
    let sig_map: BTreeMap<&str, &lps_shared::LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut obj = Object::new(BinaryFormat::Elf, Architecture::Riscv32, Endianness::Little);
    obj.flags = FileFlags::Elf {
        os_abi: elf::ELFOSABI_NONE,
        abi_version: 0,
        e_flags: elf::EF_RISCV_FLOAT_ABI_SOFT,
    };

    let text = obj.section_id(StandardSection::Text);
    let mut undefined_syms: BTreeMap<String, SymbolId> = BTreeMap::new();

    for func in &ir.functions {
        // Get the signature for this function, or use a default (void -> void) if not found
        let default_sig = lps_shared::LpsFnSig {
            name: func.name.clone(),
            return_type: lps_shared::LpsType::Void,
            parameters: alloc::vec::Vec::new(),
        };
        let fn_sig = sig_map
            .get(func.name.as_str())
            .copied()
            .unwrap_or(&default_sig);
        let emitted = emit_function_bytes(func, fn_sig, float_mode, false)?;
        let ctx = emitted;

        let func_off = obj.append_section_data(text, &ctx.code, 4);
        let scope = if func.is_entry {
            SymbolScope::Linkage
        } else {
            SymbolScope::Compilation
        };
        obj.add_symbol(Symbol {
            name: func.name.as_bytes().to_vec(),
            value: func_off,
            size: ctx.code.len() as u64,
            kind: SymbolKind::Text,
            scope,
            weak: false,
            section: SymbolSection::Section(text),
            flags: SymbolFlags::None,
        });

        for r in &ctx.relocs {
            let sym_id = if let Some(id) = undefined_syms.get(&r.symbol) {
                *id
            } else {
                let id = obj.add_symbol(Symbol {
                    name: r.symbol.as_bytes().to_vec(),
                    value: 0,
                    size: 0,
                    kind: SymbolKind::Text,
                    scope: SymbolScope::Linkage,
                    weak: false,
                    section: SymbolSection::Undefined,
                    flags: SymbolFlags::None,
                });
                undefined_syms.insert(r.symbol.clone(), id);
                id
            };
            obj.add_relocation(
                text,
                Relocation {
                    offset: func_off + r.offset as u64,
                    symbol: sym_id,
                    addend: 0,
                    flags: object::RelocationFlags::Elf {
                        r_type: elf::R_RISCV_CALL_PLT,
                    },
                },
            )
            .map_err(|e| NativeError::ObjectWrite(e.to_string()))?;
        }
    }

    obj.write()
        .map_err(|e| NativeError::ObjectWrite(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regalloc::{GreedyAlloc, RegAlloc};
    use alloc::vec;

    fn alloc_fn(f: &lpir::IrFunction, v: &[VInst]) -> crate::regalloc::Allocation {
        // Tests use direct return (not sret), so arg_reg_offset = 0
        RegAlloc::allocate(&GreedyAlloc, f, v, 0).expect("alloc")
    }
    use lpir::{IrFunction, Op};
    use lps_shared::{FnParam, LpsFnSig, LpsType, ParamQualifier};

    /// [`LpsFnSig`] consistent with [`leaf_add`] (vmctx + two scalar params, scalar return).
    fn leaf_lps_sig() -> LpsFnSig {
        LpsFnSig {
            name: String::from("leaf_add"),
            return_type: LpsType::Int,
            parameters: vec![
                FnParam {
                    name: String::from("a"),
                    ty: LpsType::Int,
                    qualifier: ParamQualifier::In,
                },
                FnParam {
                    name: String::from("b"),
                    ty: LpsType::Int,
                    qualifier: ParamQualifier::In,
                },
            ],
        }
    }

    fn leaf_abi_info() -> super::AbiInfo {
        super::AbiInfo::from_lps_sig(&leaf_lps_sig())
    }

    /// Matches [`reloc_recorded_on_call`] IR (two float params, float return).
    fn call_test_lps_sig() -> LpsFnSig {
        LpsFnSig {
            name: String::from("c"),
            return_type: LpsType::Float,
            parameters: vec![
                FnParam {
                    name: String::from("a"),
                    ty: LpsType::Float,
                    qualifier: ParamQualifier::In,
                },
                FnParam {
                    name: String::from("b"),
                    ty: LpsType::Float,
                    qualifier: ParamQualifier::In,
                },
            ],
        }
    }

    fn call_test_abi_info() -> super::AbiInfo {
        super::AbiInfo::from_lps_sig(&call_test_lps_sig())
    }

    fn leaf_add() -> IrFunction {
        IrFunction {
            name: String::from("leaf_add"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 2,
            return_types: vec![lpir::IrType::I32],
            vreg_types: vec![
                lpir::IrType::I32,
                lpir::IrType::I32,
                lpir::IrType::I32,
                lpir::IrType::I32,
            ],
            slots: vec![],
            body: vec![
                Op::Iadd {
                    dst: VReg(3),
                    lhs: VReg(1),
                    rhs: VReg(2),
                },
                Op::Return {
                    values: lpir::types::VRegRange { start: 0, count: 1 },
                },
            ],
            vreg_pool: vec![VReg(3)],
        }
    }

    #[test]
    fn emit_leaf_prologue_epilogue_size() {
        let f = leaf_add();
        let v = crate::lower::lower_ops(&f, lpir::FloatMode::Q32).expect("lower");
        let a = alloc_fn(&f, &v);
        let mut ctx = EmitContext::new(true, false);
        let abi = leaf_abi_info();
        ctx.emit_prologue(&abi);
        for i in &v {
            ctx.emit_vinst(i, &a, &abi).expect("emit");
        }
        ctx.emit_epilogue(&abi);
        assert!(ctx.code.len() >= 12);
        assert!(ctx.relocs.is_empty());
    }

    #[test]
    fn debug_lines_populated_when_enabled() {
        let f = leaf_add();
        let e = emit_function_bytes(&f, &leaf_lps_sig(), lpir::FloatMode::Q32, true).expect("emit");
        assert!(
            !e.debug_lines.is_empty(),
            "expected per-instruction debug lines"
        );
    }

    #[test]
    fn reloc_recorded_on_call() {
        let f = IrFunction {
            name: String::from("c"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 2,
            return_types: vec![],
            vreg_types: vec![lpir::IrType::I32; 4],
            slots: vec![],
            body: vec![
                Op::Fadd {
                    dst: VReg(3),
                    lhs: VReg(1),
                    rhs: VReg(2),
                },
                Op::Return {
                    values: lpir::types::VRegRange { start: 0, count: 1 },
                },
            ],
            vreg_pool: vec![VReg(3)],
        };
        let v = crate::lower::lower_ops(&f, lpir::FloatMode::Q32).expect("lower");
        let a = alloc_fn(&f, &v);
        let mut ctx = EmitContext::new(false, false);
        let abi = call_test_abi_info();
        ctx.emit_prologue(&abi);
        for i in &v {
            ctx.emit_vinst(i, &a, &abi).expect("emit");
        }
        ctx.emit_epilogue(&abi);
        assert_eq!(ctx.relocs.len(), 1);
        assert!(ctx.relocs[0].symbol.contains("fadd"));
    }
}
