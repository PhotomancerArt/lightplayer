//! Per-function lowering context (builder, expression cache, local maps).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{CalleeRef, FunctionBuilder, IrModule, IrType, Op, SlotId, VReg};
use naga::{
    AddressSpace, Expression, Function, Handle, LocalVariable, Module, Statement, Type, TypeInner,
};
use smallvec::SmallVec;

use crate::lower_error::LowerError;
use crate::lower_expr;

pub(crate) use crate::naga_util::{
    func_return_ir_types, naga_scalar_to_ir_type, naga_type_to_ir_types, naga_type_width,
    vector_size_usize,
};

pub(crate) type VRegVec = SmallVec<[VReg; 4]>;

/// Where array element data lives: owned stack slot vs. `inout`/`out` parameter buffer (`arg_vregs` base).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArraySlot {
    Local(SlotId),
    /// Reserved for future pointer-to-array formals (`out` / `inout`); lowering uses `ArraySlot::Local` for `in T[]` today.
    #[allow(dead_code)]
    Param(u32),
}

/// Stack [`SlotId`] metadata for one array-typed [`LocalVariable`].
#[derive(Clone, Debug)]
pub(crate) struct ArrayInfo {
    pub slot: ArraySlot,
    /// Outer dimension first, e.g. `[2, 3]` for `int[2][3]`.
    pub dimensions: SmallVec<[u32; 4]>,
    pub leaf_element_ty: Handle<Type>,
    pub leaf_stride: u32,
    /// Product of [`Self::dimensions`]; leaf slots in row-major order.
    pub element_count: u32,
}

#[allow(
    dead_code,
    reason = "reserved for call lowering and cross-function checks"
)]
pub(crate) struct LowerCtx<'a> {
    pub fb: FunctionBuilder,
    pub module: &'a Module,
    pub func: &'a Function,
    pub ir_module: Option<&'a IrModule>,
    pub expr_cache: Vec<Option<VRegVec>>,
    pub local_map: BTreeMap<Handle<LocalVariable>, VRegVec>,
    pub array_map: BTreeMap<Handle<LocalVariable>, ArrayInfo>,
    /// Naga emits `.length()` on multi-dim arrays as the outer type-tree size, not GLSL's leftmost `[]`.
    /// Pairs `Load(array)` + `Literal(U32)` (and chained `Literal(I32)` copies) get corrected values here.
    pub(crate) array_length_literal_fixes: BTreeMap<Handle<Expression>, i32>,
    pub param_aliases: BTreeMap<Handle<LocalVariable>, VRegVec>,
    /// VRegs per function argument index (flattened scalars).
    pub(crate) arg_vregs: BTreeMap<u32, VRegVec>,
    /// `in`/`out`/`inout` parameters: Naga type handle of the pointee (for Load/Store).
    pub(crate) pointer_args: BTreeMap<u32, Handle<Type>>,
    pub func_map: BTreeMap<Handle<Function>, CalleeRef>,
    pub import_map: BTreeMap<String, CalleeRef>,
    pub lpfx_map: BTreeMap<Handle<Function>, CalleeRef>,
    pub return_types: Vec<IrType>,
}

impl<'a> LowerCtx<'a> {
    pub(crate) fn new(
        module: &'a Module,
        func: &'a Function,
        name: &str,
        func_map: &BTreeMap<Handle<Function>, CalleeRef>,
        import_map: &BTreeMap<String, CalleeRef>,
        lpfx_map: &BTreeMap<Handle<Function>, CalleeRef>,
    ) -> Result<Self, LowerError> {
        let return_types = func_return_ir_types(module, func)?;
        let mut fb = FunctionBuilder::new(name, &return_types);

        let mut arg_vregs: BTreeMap<u32, VRegVec> = BTreeMap::new();
        let mut pointer_args: BTreeMap<u32, Handle<Type>> = BTreeMap::new();
        for (i, arg) in func.arguments.iter().enumerate() {
            let inner = &module.types[arg.ty].inner;
            match inner {
                TypeInner::Pointer {
                    base,
                    space: AddressSpace::Function,
                } => {
                    let addr = fb.add_param(IrType::I32);
                    arg_vregs.insert(i as u32, smallvec::smallvec![addr]);
                    pointer_args.insert(i as u32, *base);
                }
                TypeInner::Array { .. } => {
                    let tys = array_ty_flat_ir_types(module, arg.ty)?;
                    let mut vregs = VRegVec::new();
                    for ty in tys {
                        vregs.push(fb.add_param(ty));
                    }
                    arg_vregs.insert(i as u32, vregs);
                }
                _ => {
                    let tys = naga_type_to_ir_types(inner)?;
                    let mut vregs = VRegVec::new();
                    for ty in &tys {
                        let v = fb.add_param(*ty);
                        vregs.push(v);
                    }
                    arg_vregs.insert(i as u32, vregs);
                }
            }
        }

        let param_idx = scan_param_argument_indices(module, func);
        let mut param_aliases: BTreeMap<Handle<LocalVariable>, VRegVec> = BTreeMap::new();
        for (lv, arg_i) in &param_idx {
            if let Some(vs) = arg_vregs.get(arg_i) {
                param_aliases.insert(*lv, vs.clone());
            }
        }

        let mut local_map: BTreeMap<Handle<LocalVariable>, VRegVec> = BTreeMap::new();
        let mut array_map: BTreeMap<Handle<LocalVariable>, ArrayInfo> = BTreeMap::new();
        for (lv_handle, var) in func.local_variables.iter() {
            if param_aliases.contains_key(&lv_handle) {
                continue;
            }
            match &module.types[var.ty].inner {
                TypeInner::Array { .. } => {
                    let (dimensions, leaf_ty, leaf_stride) =
                        crate::lower_array_multidim::flatten_local_array_shape(module, func, &var)?;
                    let element_count = dimensions
                        .iter()
                        .try_fold(1u32, |acc, &d| acc.checked_mul(d))
                        .ok_or_else(|| {
                            LowerError::Internal(String::from("array element count overflow"))
                        })?;
                    let total = element_count.checked_mul(leaf_stride).ok_or_else(|| {
                        LowerError::Internal(String::from("array slot size overflow"))
                    })?;
                    let slot = fb.alloc_slot(total);
                    array_map.insert(
                        lv_handle,
                        ArrayInfo {
                            slot: ArraySlot::Local(slot),
                            dimensions,
                            leaf_element_ty: leaf_ty,
                            leaf_stride,
                            element_count,
                        },
                    );
                }
                _ => {
                    let inner = &module.types[var.ty].inner;
                    let tys = naga_type_to_ir_types(inner)?;
                    let mut vregs = VRegVec::new();
                    for ty in &tys {
                        vregs.push(fb.alloc_vreg(*ty));
                    }
                    local_map.insert(lv_handle, vregs);
                }
            }
        }

        for (lv_handle, var) in func.local_variables.iter() {
            if param_aliases.contains_key(&lv_handle) {
                continue;
            }
            if let Some(info) = array_map.get(&lv_handle) {
                if var.init.is_none() {
                    if matches!(info.slot, ArraySlot::Local(_)) {
                        crate::lower_array::zero_fill_array_slot(&mut fb, module, info)?;
                    }
                }
            }
        }

        let expr_cache = vec![None; func.expressions.len()];
        let array_length_literal_fixes =
            crate::lower_array::scan_naga_multidim_array_length_literals(func, &array_map);

        let mut ctx = Self {
            fb,
            module,
            func,
            ir_module: None,
            expr_cache,
            local_map,
            array_map,
            array_length_literal_fixes,
            param_aliases,
            arg_vregs,
            pointer_args,
            func_map: func_map.clone(),
            import_map: import_map.clone(),
            lpfx_map: lpfx_map.clone(),
            return_types,
        };

        for (lv_handle, var) in func.local_variables.iter() {
            if ctx.param_aliases.contains_key(&lv_handle) {
                continue;
            }
            let Some(init_h) = var.init else {
                continue;
            };
            if let Some(info) = ctx.array_map.get(&lv_handle).cloned() {
                crate::lower_array::lower_array_initializer(&mut ctx, &info, init_h)?;
                continue;
            }
            let dsts = ctx.local_map.get(&lv_handle).cloned().ok_or_else(|| {
                LowerError::Internal(format!("local init for missing vreg {lv_handle:?}"))
            })?;
            let srcs = lower_expr::lower_expr_vec(&mut ctx, init_h)?;
            if dsts.len() != srcs.len() {
                return Err(LowerError::Internal(format!(
                    "local init component mismatch: {} vs {}",
                    dsts.len(),
                    srcs.len()
                )));
            }
            for (d, s) in dsts.iter().zip(srcs.iter()) {
                ctx.fb.push(Op::Copy { dst: *d, src: *s });
            }
        }

        Ok(ctx)
    }

    pub(crate) fn finish(self) -> lpir::IrFunction {
        self.fb.finish()
    }

    pub(crate) fn arg_vregs_for(&self, idx: u32) -> Result<VRegVec, LowerError> {
        self.arg_vregs
            .get(&idx)
            .cloned()
            .ok_or_else(|| LowerError::Internal(format!("bad FunctionArgument index {idx}")))
    }

    pub(crate) fn resolve_local(&self, lv: Handle<LocalVariable>) -> Result<VRegVec, LowerError> {
        if self.array_map.contains_key(&lv) {
            return Err(LowerError::Internal(format!(
                "local {lv:?} is an array (use slot lowering)"
            )));
        }
        if let Some(v) = self.param_aliases.get(&lv) {
            return Ok(v.clone());
        }
        self.local_map
            .get(&lv)
            .cloned()
            .ok_or_else(|| LowerError::Internal(format!("unknown local variable {lv:?}")))
    }

    #[allow(dead_code, reason = "call lowering and tooling")]
    pub(crate) fn resolve_array(
        &self,
        lv: Handle<LocalVariable>,
    ) -> Result<&ArrayInfo, LowerError> {
        self.array_map
            .get(&lv)
            .ok_or_else(|| LowerError::Internal(format!("local {lv:?} is not an array")))
    }

    pub(crate) fn ensure_expr_vec(
        &mut self,
        expr: Handle<naga::Expression>,
    ) -> Result<VRegVec, LowerError> {
        lower_expr::lower_expr_vec(self, expr)
    }

    pub(crate) fn ensure_expr(
        &mut self,
        expr: Handle<naga::Expression>,
    ) -> Result<VReg, LowerError> {
        let vs = self.ensure_expr_vec(expr)?;
        if vs.len() != 1 {
            return Err(LowerError::Internal(format!(
                "expected scalar expression, got {} components",
                vs.len()
            )));
        }
        Ok(vs[0])
    }
}

/// One LPIR parameter per scalar component, row-major, for a value `in` array argument.
fn array_ty_flat_ir_types(
    module: &Module,
    array_ty: Handle<Type>,
) -> Result<Vec<IrType>, LowerError> {
    let (dimensions, leaf_ty, _) =
        crate::lower_array_multidim::flatten_array_type_shape(module, array_ty)?;
    let element_count = dimensions
        .iter()
        .try_fold(1u32, |acc, &d| acc.checked_mul(d))
        .ok_or_else(|| {
            LowerError::Internal(String::from("array_ty_flat_ir_types: count overflow"))
        })?;
    let leaf_inner = &module.types[leaf_ty].inner;
    let leaf_tys = naga_type_to_ir_types(leaf_inner)?;
    let mut out = Vec::new();
    for _ in 0..element_count {
        for ty in leaf_tys.iter() {
            out.push(*ty);
        }
    }
    Ok(out)
}

fn scan_param_argument_indices(
    module: &Module,
    func: &Function,
) -> BTreeMap<Handle<LocalVariable>, u32> {
    let mut m = BTreeMap::new();
    fn walk_block(
        block: &naga::Block,
        module: &Module,
        func: &Function,
        m: &mut BTreeMap<Handle<LocalVariable>, u32>,
    ) {
        for stmt in block.iter() {
            match stmt {
                Statement::Store { pointer, value } => {
                    if let (Expression::LocalVariable(lv), Expression::FunctionArgument(idx)) =
                        (&func.expressions[*pointer], &func.expressions[*value])
                    {
                        let arg_ty = func.arguments.get(*idx as usize).map(|a| a.ty);
                        let is_ptr = arg_ty.is_some_and(|h| {
                            matches!(
                                &module.types[h].inner,
                                TypeInner::Pointer {
                                    space: AddressSpace::Function,
                                    ..
                                }
                            )
                        });
                        let is_array_val = arg_ty.is_some_and(|h| {
                            matches!(&module.types[h].inner, TypeInner::Array { .. })
                        });
                        // `in T[]` uses a real stack array in `array_map`; do not alias as flat vregs.
                        if !is_ptr && !is_array_val {
                            m.insert(*lv, *idx);
                        }
                    }
                }
                Statement::Block(inner) => walk_block(inner, module, func, m),
                Statement::If { accept, reject, .. } => {
                    walk_block(accept, module, func, m);
                    walk_block(reject, module, func, m);
                }
                Statement::Switch { cases, .. } => {
                    for case in cases.iter() {
                        walk_block(&case.body, module, func, m);
                    }
                }
                Statement::Loop {
                    body, continuing, ..
                } => {
                    walk_block(body, module, func, m);
                    walk_block(continuing, module, func, m);
                }
                _ => {}
            }
        }
    }
    walk_block(&func.body, module, func, &mut m);
    m
}
