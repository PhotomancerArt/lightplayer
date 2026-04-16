//! Stack-based builders for [`crate::lpir_module::IrFunction`] and [`crate::lpir_module::LpirModule`].

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::lpir_module::{ImportDecl, IrFunction, LpirModule, SlotDecl, VMCTX_VREG};
use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, FuncId, ImportId, IrType, SlotId, VReg, VRegRange};

/// Build a single function's IR (flat op stream + pools).
pub struct FunctionBuilder {
    name: String,
    is_entry: bool,
    return_types: Vec<IrType>,
    param_count: u16,
    vreg_types: Vec<IrType>,
    slots: Vec<SlotDecl>,
    body: Vec<LpirOp>,
    vreg_pool: Vec<VReg>,
    next_vreg: u32,
    next_slot: u32,
    block_stack: Vec<BlockEntry>,
}

enum BlockEntry {
    If {
        start_idx: usize,
    },
    Else {
        if_start_idx: usize,
    },
    Loop {
        start_idx: usize,
        continuing_set: bool,
    },
    Switch {
        start_idx: usize,
        /// Index of last `CaseStart` / `DefaultStart` needing `end_offset` patch, if any.
        pending_case: Option<usize>,
    },
}

impl FunctionBuilder {
    pub fn new(name: &str, return_types: &[IrType]) -> Self {
        Self {
            name: String::from(name),
            is_entry: false,
            return_types: return_types.to_vec(),
            param_count: 0,
            vreg_types: alloc::vec![IrType::Pointer], // VMContext (pointer width at codegen)
            slots: Vec::new(),
            body: Vec::new(),
            vreg_pool: Vec::new(),
            next_vreg: 1,
            next_slot: 0,
            block_stack: Vec::new(),
        }
    }

    pub fn set_entry(&mut self) {
        self.is_entry = true;
    }

    pub fn add_param(&mut self, ty: IrType) -> VReg {
        let v = VReg(self.next_vreg);
        self.next_vreg += 1;
        self.param_count += 1;
        self.vreg_types.push(ty);
        v
    }

    pub fn alloc_vreg(&mut self, ty: IrType) -> VReg {
        let v = VReg(self.next_vreg);
        self.next_vreg += 1;
        self.vreg_types.push(ty);
        v
    }

    pub fn alloc_slot(&mut self, size: u32) -> SlotId {
        let id = SlotId(self.next_slot);
        self.next_slot += 1;
        self.slots.push(SlotDecl { size });
        id
    }

    pub fn push(&mut self, op: LpirOp) {
        self.body.push(op);
    }

    pub fn push_if(&mut self, cond: VReg) {
        let idx = self.body.len();
        self.body.push(LpirOp::IfStart {
            cond,
            else_offset: 0,
            end_offset: 0,
        });
        self.block_stack.push(BlockEntry::If { start_idx: idx });
    }

    pub fn push_else(&mut self) {
        let entry = self
            .block_stack
            .pop()
            .expect("push_else without matching push_if");
        match entry {
            BlockEntry::If { start_idx } => {
                let else_idx = self.body.len();
                if let LpirOp::IfStart {
                    else_offset,
                    end_offset: _,
                    ..
                } = &mut self.body[start_idx]
                {
                    *else_offset = else_idx as u32;
                }
                self.body.push(LpirOp::Else);
                self.block_stack.push(BlockEntry::Else {
                    if_start_idx: start_idx,
                });
            }
            _ => panic!("push_else: expected If on stack"),
        }
    }

    pub fn end_if(&mut self) {
        let end_idx = self.body.len();
        self.body.push(LpirOp::End);
        let after = (end_idx + 1) as u32;
        let entry = self.block_stack.pop().expect("end_if without open block");
        match entry {
            BlockEntry::If { start_idx } => {
                if let LpirOp::IfStart {
                    else_offset,
                    end_offset,
                    ..
                } = &mut self.body[start_idx]
                {
                    *else_offset = end_idx as u32;
                    *end_offset = after;
                }
            }
            BlockEntry::Else { if_start_idx } => {
                if let LpirOp::IfStart {
                    end_offset,
                    else_offset: _,
                    ..
                } = &mut self.body[if_start_idx]
                {
                    *end_offset = after;
                }
            }
            _ => panic!("end_if: expected If or Else"),
        }
    }

    pub fn push_loop(&mut self) {
        let idx = self.body.len();
        self.body.push(LpirOp::LoopStart {
            continuing_offset: 0,
            end_offset: 0,
        });
        self.block_stack.push(BlockEntry::Loop {
            start_idx: idx,
            continuing_set: false,
        });
    }

    pub fn push_continuing(&mut self) {
        let cur = self.body.len() as u32;
        let top = self
            .block_stack
            .last_mut()
            .expect("push_continuing outside block");
        let BlockEntry::Loop {
            start_idx,
            continuing_set,
        } = top
        else {
            panic!("push_continuing: expected Loop on stack");
        };
        assert!(!*continuing_set, "push_continuing called twice");
        *continuing_set = true;
        if let LpirOp::LoopStart {
            continuing_offset, ..
        } = &mut self.body[*start_idx]
        {
            *continuing_offset = cur;
        }
    }

    pub fn end_loop(&mut self) {
        let end_idx = self.body.len();
        self.body.push(LpirOp::End);
        let after = (end_idx + 1) as u32;
        let entry = self.block_stack.pop().expect("end_loop without push_loop");
        match entry {
            BlockEntry::Loop { start_idx, .. } => {
                if let LpirOp::LoopStart {
                    continuing_offset,
                    end_offset,
                } = &mut self.body[start_idx]
                {
                    if *continuing_offset == 0 {
                        *continuing_offset = (start_idx + 1) as u32;
                    }
                    *end_offset = after;
                }
            }
            _ => panic!("end_loop: expected Loop"),
        }
    }

    pub fn push_switch(&mut self, selector: VReg) {
        let idx = self.body.len();
        self.body.push(LpirOp::SwitchStart {
            selector,
            end_offset: 0,
        });
        self.block_stack.push(BlockEntry::Switch {
            start_idx: idx,
            pending_case: None,
        });
    }

    fn patch_switch_pending_to_here(&mut self) {
        let cur = self.body.len() as u32;
        let top = self
            .block_stack
            .last_mut()
            .expect("patch_switch_pending: no switch on stack");
        let BlockEntry::Switch { pending_case, .. } = top else {
            panic!("patch_switch_pending: expected Switch");
        };
        if let Some(pc) = pending_case.take() {
            match &mut self.body[pc] {
                LpirOp::CaseStart { end_offset, .. } | LpirOp::DefaultStart { end_offset } => {
                    *end_offset = cur;
                }
                _ => {}
            }
        }
    }

    pub fn push_case(&mut self, value: i32) {
        self.patch_switch_pending_to_here();
        let case_idx = self.body.len();
        self.body.push(LpirOp::CaseStart {
            value,
            end_offset: 0,
        });
        let top = self
            .block_stack
            .last_mut()
            .expect("push_case outside switch");
        let BlockEntry::Switch { pending_case, .. } = top else {
            panic!("push_case: expected Switch on stack");
        };
        *pending_case = Some(case_idx);
    }

    pub fn push_default(&mut self) {
        self.patch_switch_pending_to_here();
        let case_idx = self.body.len();
        self.body.push(LpirOp::DefaultStart { end_offset: 0 });
        let top = self
            .block_stack
            .last_mut()
            .expect("push_default outside switch");
        let BlockEntry::Switch { pending_case, .. } = top else {
            panic!("push_default: expected Switch on stack");
        };
        *pending_case = Some(case_idx);
    }

    /// Close a `switch` arm (`case` / `default` body). The following `}` closes the whole `switch`.
    pub fn end_switch_arm(&mut self) {
        self.body.push(LpirOp::End);
    }

    pub fn end_switch(&mut self) {
        let end_idx = self.body.len();
        self.patch_switch_pending_to_here();
        self.body.push(LpirOp::End);
        let after = (end_idx + 1) as u32;
        let entry = self
            .block_stack
            .pop()
            .expect("end_switch without push_switch");
        match entry {
            BlockEntry::Switch { start_idx, .. } => {
                if let LpirOp::SwitchStart { end_offset, .. } = &mut self.body[start_idx] {
                    *end_offset = after;
                }
            }
            _ => panic!("end_switch: expected Switch"),
        }
    }

    /// Handle a line that is only `}` in the text format (dispatches to [`Self::end_if`], etc.).
    pub fn close_brace_for_text(&mut self, peek_next: Option<&str>) -> Result<(), &'static str> {
        match self.block_stack.last() {
            Some(BlockEntry::If { .. }) | Some(BlockEntry::Else { .. }) => {
                self.end_if();
                Ok(())
            }
            Some(BlockEntry::Loop { .. }) => {
                self.end_loop();
                Ok(())
            }
            Some(BlockEntry::Switch { .. }) => {
                let arm_only = matches!(
                    peek_next,
                    Some(s) if s == "}" || s.starts_with("case ") || s.starts_with("default")
                );
                if arm_only {
                    self.end_switch_arm();
                } else {
                    self.end_switch();
                }
                Ok(())
            }
            None => Err("unexpected `}`"),
        }
    }

    /// Record a defining occurrence of `v` from the text (`vN:ty` or plain `vN` on reassignment).
    pub fn record_vreg_def(
        &mut self,
        v: VReg,
        explicit_ty: Option<IrType>,
    ) -> Result<(), &'static str> {
        let i = v.0 as usize;
        match explicit_ty {
            Some(t) => {
                if i > self.vreg_types.len() {
                    return Err("sparse vreg index (missing lower vregs)");
                }
                if i == self.vreg_types.len() {
                    self.vreg_types.push(t);
                    self.next_vreg = self.next_vreg.max(v.0.saturating_add(1));
                } else if self.vreg_types[i] != t {
                    return Err("vreg type mismatch on redefinition");
                }
                Ok(())
            }
            None => {
                if i >= self.vreg_types.len() {
                    Err("first use of vreg requires :type")
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn push_call(&mut self, callee: CalleeRef, args: &[VReg], results: &[VReg]) {
        let args_start = self.vreg_pool.len() as u32;
        self.vreg_pool.extend_from_slice(args);
        let results_start = self.vreg_pool.len() as u32;
        self.vreg_pool.extend_from_slice(results);
        self.body.push(LpirOp::Call {
            callee,
            args: VRegRange {
                start: args_start,
                count: args.len() as u16,
            },
            results: VRegRange {
                start: results_start,
                count: results.len() as u16,
            },
        });
    }

    pub fn push_return(&mut self, values: &[VReg]) {
        let start = self.vreg_pool.len() as u32;
        self.vreg_pool.extend_from_slice(values);
        self.body.push(LpirOp::Return {
            values: VRegRange {
                start,
                count: values.len() as u16,
            },
        });
    }

    pub fn finish(mut self) -> IrFunction {
        assert!(
            self.block_stack.is_empty(),
            "FunctionBuilder::finish with unclosed blocks"
        );
        IrFunction {
            name: core::mem::take(&mut self.name),
            is_entry: self.is_entry,
            vmctx_vreg: VMCTX_VREG,
            param_count: self.param_count,
            return_types: self.return_types,
            vreg_types: self.vreg_types,
            slots: self.slots,
            body: self.body,
            vreg_pool: self.vreg_pool,
        }
    }
}

/// Build an [`LpirModule`].
pub struct ModuleBuilder {
    imports: Vec<ImportDecl>,
    functions: BTreeMap<FuncId, IrFunction>,
    next_func_id: u16,
}

impl Default for ModuleBuilder {
    fn default() -> Self {
        Self {
            imports: Vec::new(),
            functions: BTreeMap::new(),
            next_func_id: 0,
        }
    }
}

impl ModuleBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Id that the next [`Self::add_function`] will assign (for parser self-reference).
    pub fn next_local_func_id(&self) -> FuncId {
        FuncId(self.next_func_id)
    }

    pub fn import_count(&self) -> u32 {
        self.imports.len() as u32
    }

    pub fn imports(&self) -> &[ImportDecl] {
        &self.imports
    }

    pub fn function_count(&self) -> u32 {
        self.functions.len() as u32
    }

    pub fn add_import(&mut self, decl: ImportDecl) -> CalleeRef {
        self.imports.push(decl);
        let idx = (self.imports.len() - 1) as u16;
        CalleeRef::Import(ImportId(idx))
    }

    pub fn add_function(&mut self, func: IrFunction) -> CalleeRef {
        let id = FuncId(self.next_func_id);
        self.next_func_id = self.next_func_id.saturating_add(1);
        self.functions.insert(id, func);
        CalleeRef::Local(id)
    }

    pub fn finish(self) -> LpirModule {
        LpirModule {
            imports: self.imports,
            functions: self.functions,
        }
    }
}
