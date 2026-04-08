//! RV32 emission: machine code, relocations, ELF object (`object` crate).

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use object::write::{Object, Relocation, StandardSection, Symbol, SymbolId, SymbolSection};
use object::{
    Architecture, BinaryFormat, Endianness, FileFlags, SymbolFlags, SymbolKind, SymbolScope, elf,
};

use super::abi::{self, ARG_REGS, PhysReg, RET_REGS, SP};
use super::inst::{
    encode_addi, encode_auipc, encode_jalr, encode_lw, encode_mul, encode_ret, encode_sub,
    encode_sw, iconst32_sequence,
};
use crate::error::NativeError;
use crate::regalloc::{Allocation, GreedyAlloc, RegAlloc};
use crate::vinst::VInst;
use lpir::VReg;

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
    frame_size: i32,
    is_leaf: bool,
    debug_info: bool,
    current_src_op: Option<u32>,
}

impl EmitContext {
    pub fn new(is_leaf: bool, debug_info: bool) -> Self {
        Self {
            code: Vec::new(),
            relocs: Vec::new(),
            debug_lines: Vec::new(),
            frame_size: 16,
            is_leaf,
            debug_info,
            current_src_op: None,
        }
    }

    fn push_u32(&mut self, w: u32) {
        let offset = self.code.len() as u32;
        self.code.extend_from_slice(&w.to_le_bytes());
        if self.debug_info && self.current_src_op.is_some() {
            self.debug_lines.push((offset, self.current_src_op));
        }
    }

    pub fn emit_prologue(&mut self) {
        let sp = u32::from(SP);
        self.push_u32(encode_addi(sp, sp, -self.frame_size));
        if !self.is_leaf {
            let off = self.frame_size - 4;
            self.push_u32(encode_sw(u32::from(abi::RA), sp, off));
        }
    }

    pub fn emit_epilogue(&mut self) {
        let sp = u32::from(SP);
        if !self.is_leaf {
            let off = self.frame_size - 4;
            self.push_u32(encode_lw(u32::from(abi::RA), sp, off));
        }
        self.push_u32(encode_addi(sp, sp, self.frame_size));
        self.push_u32(encode_ret());
    }

    fn phys(alloc: &Allocation, v: VReg) -> Result<PhysReg, NativeError> {
        let i = v.0 as usize;
        alloc
            .vreg_to_phys
            .get(i)
            .copied()
            .flatten()
            .ok_or_else(|| NativeError::UnassignedVReg(v.0))
    }

    pub fn emit_vinst(&mut self, inst: &VInst, alloc: &Allocation) -> Result<(), NativeError> {
        self.current_src_op = inst.src_op();
        match inst {
            VInst::Add32 {
                dst, src1, src2, ..
            } => {
                let rd = Self::phys(alloc, *dst)? as u32;
                let rs1 = Self::phys(alloc, *src1)? as u32;
                let rs2 = Self::phys(alloc, *src2)? as u32;
                self.push_u32(crate::isa::rv32::inst::encode_add(rd, rs1, rs2));
            }
            VInst::Sub32 {
                dst, src1, src2, ..
            } => {
                let rd = Self::phys(alloc, *dst)? as u32;
                let rs1 = Self::phys(alloc, *src1)? as u32;
                let rs2 = Self::phys(alloc, *src2)? as u32;
                self.push_u32(encode_sub(rd, rs1, rs2));
            }
            VInst::Mul32 {
                dst, src1, src2, ..
            } => {
                let rd = Self::phys(alloc, *dst)? as u32;
                let rs1 = Self::phys(alloc, *src1)? as u32;
                let rs2 = Self::phys(alloc, *src2)? as u32;
                self.push_u32(encode_mul(rd, rs1, rs2));
            }
            VInst::Mov32 { dst, src, .. } => {
                let rd = Self::phys(alloc, *dst)? as u32;
                let rs = Self::phys(alloc, *src)? as u32;
                if rd != rs {
                    self.push_u32(encode_addi(rd, rs, 0));
                }
            }
            VInst::Load32 {
                dst, base, offset, ..
            } => {
                let rd = Self::phys(alloc, *dst)? as u32;
                let rs1 = Self::phys(alloc, *base)? as u32;
                self.push_u32(encode_lw(rd, rs1, *offset));
            }
            VInst::Store32 {
                src, base, offset, ..
            } => {
                let rs2 = Self::phys(alloc, *src)? as u32;
                let rs1 = Self::phys(alloc, *base)? as u32;
                self.push_u32(encode_sw(rs2, rs1, *offset));
            }
            VInst::IConst32 { dst, val, .. } => {
                let rd = Self::phys(alloc, *dst)? as u32;
                for w in iconst32_sequence(rd, *val) {
                    self.push_u32(w);
                }
            }
            VInst::Call {
                target, args, rets, ..
            } => {
                if args.len() > ARG_REGS.len() {
                    return Err(NativeError::TooManyArgs(args.len()));
                }
                for (i, a) in args.iter().enumerate() {
                    let from = Self::phys(alloc, *a)? as u32;
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
                    let dst = Self::phys(alloc, *r)? as u32;
                    let src = RET_REGS[i] as u32;
                    if dst != src {
                        self.push_u32(encode_addi(dst, src, 0));
                    }
                }
            }
            VInst::Ret { vals, .. } => {
                for (i, v) in vals.iter().enumerate() {
                    if i >= RET_REGS.len() {
                        return Err(NativeError::TooManyReturns(vals.len()));
                    }
                    let src = Self::phys(alloc, *v)? as u32;
                    let dst = RET_REGS[i] as u32;
                    if src != dst {
                        self.push_u32(encode_addi(dst, src, 0));
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
pub fn emit_function_bytes(
    func: &lpir::IrFunction,
    float_mode: lpir::FloatMode,
    debug_info: bool,
) -> Result<EmittedFunction, NativeError> {
    let vinsts = crate::lower::lower_ops(func, float_mode)?;
    let alloc = RegAlloc::allocate(&GreedyAlloc, func, &vinsts)?;
    let is_leaf = !vinsts.iter().any(|v| v.is_call());
    let mut ctx = EmitContext::new(is_leaf, debug_info);
    ctx.emit_prologue();
    for v in &vinsts {
        ctx.emit_vinst(v, &alloc)?;
    }
    ctx.emit_epilogue();
    Ok(EmittedFunction {
        code: ctx.code,
        relocs: ctx.relocs,
        debug_lines: ctx.debug_lines,
    })
}

/// Append all local functions from `ir` into one RV32 ELF relocatable object.
pub fn emit_module_elf(
    ir: &lpir::IrModule,
    float_mode: lpir::FloatMode,
) -> Result<Vec<u8>, NativeError> {
    if ir.functions.is_empty() {
        return Err(NativeError::EmptyModule);
    }

    let mut obj = Object::new(BinaryFormat::Elf, Architecture::Riscv32, Endianness::Little);
    obj.flags = FileFlags::Elf {
        os_abi: elf::ELFOSABI_NONE,
        abi_version: 0,
        e_flags: elf::EF_RISCV_FLOAT_ABI_SOFT,
    };

    let text = obj.section_id(StandardSection::Text);
    let mut undefined_syms: BTreeMap<String, SymbolId> = BTreeMap::new();

    for func in &ir.functions {
        let emitted = emit_function_bytes(func, float_mode, false)?;
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
        RegAlloc::allocate(&GreedyAlloc, f, v).expect("alloc")
    }
    use lpir::{IrFunction, Op};

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
        ctx.emit_prologue();
        for i in &v {
            ctx.emit_vinst(i, &a).expect("emit");
        }
        ctx.emit_epilogue();
        assert!(ctx.code.len() >= 12);
        assert!(ctx.relocs.is_empty());
    }

    #[test]
    fn debug_lines_populated_when_enabled() {
        let f = leaf_add();
        let e = emit_function_bytes(&f, lpir::FloatMode::Q32, true).expect("emit");
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
        ctx.emit_prologue();
        for i in &v {
            ctx.emit_vinst(i, &a).expect("emit");
        }
        ctx.emit_epilogue();
        assert_eq!(ctx.relocs.len(), 1);
        assert!(ctx.relocs[0].symbol.contains("fadd"));
    }
}
