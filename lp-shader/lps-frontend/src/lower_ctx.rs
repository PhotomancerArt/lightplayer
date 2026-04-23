//! Per-function lowering context (builder, expression cache, local maps).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{CalleeRef, FunctionBuilder, IrType, LpirModule, LpirOp, SlotId, VReg};
use naga::{
    AddressSpace, Expression, Function, GlobalVariable, Handle, LocalVariable, Module, Statement,
    Type, TypeInner,
};
use smallvec::SmallVec;

use crate::lower_error::LowerError;
use crate::lower_expr;

pub(crate) use crate::naga_util::{
    array_ty_pointer_arg_ir_type, naga_scalar_to_ir_type, naga_type_to_ir_types, naga_type_width,
    vector_size_usize,
};

use naga::ArraySize;

/// True if `ty` is a (possibly nested) array type that still uses [`ArraySize::Pending`] or
/// [`ArraySize::Dynamic`] on any dimension (GLSL `T[]` with size inferred from the initializer).
/// [`crate::lower_aggregate_layout::aggregate_size_and_align`] cannot lower those Naga handles; byte
/// size must come from [`crate::lower_array_multidim::flatten_local_array_shape`] instead.
fn array_type_has_inferred_dimension(module: &Module, mut ty: Handle<Type>) -> bool {
    loop {
        match &module.types[ty].inner {
            TypeInner::Array { base, size, .. } => {
                if matches!(
                    size,
                    ArraySize::Pending(_) | ArraySize::Dynamic
                ) {
                    return true;
                }
                match &module.types[*base].inner {
                    TypeInner::Array { .. } => ty = *base,
                    _ => return false,
                }
            }
            _ => return false,
        }
    }
}

/// Information about a global variable (uniform or private global) for lowering.
#[derive(Clone, Debug)]
pub(crate) struct GlobalVarInfo {
    /// Byte offset from the start of the VMContext buffer (including header).
    pub byte_offset: u32,
    /// The LpsType of this global variable.
    pub ty: lps_shared::LpsType,
    /// Number of scalar components (for scalarization).
    pub component_count: u32,
    /// Whether this is a uniform (read-only) variable.
    pub is_uniform: bool,
}

/// Map from Naga GlobalVariable handle to its lowering info.
pub(crate) type GlobalVarMap = BTreeMap<Handle<GlobalVariable>, GlobalVarInfo>;

pub(crate) type VRegVec = SmallVec<[VReg; 4]>;

/// Where aggregate (array) element data lives: stack slot or `inout`/`out` parameter buffer
/// ([`LowerCtx::arg_vregs`]\[0\] = pointer).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AggregateSlot {
    Local(SlotId),
    /// Pointer-to-array formal (`out` / `inout`); base address is [`LowerCtx::arg_vregs`]\[0\].
    Param(u32),
}

/// Stack [`SlotId`] and layout metadata for one aggregate-typed value (M1: arrays only).
#[derive(Clone, Debug)]
pub(crate) struct AggregateInfo {
    pub slot: AggregateSlot,
    /// Outer dimension first, e.g. `[2, 3]` for `int[2][3]`.
    pub dimensions: SmallVec<[u32; 4]>,
    pub leaf_element_ty: Handle<Type>,
    pub leaf_stride: u32,
    /// Product of [`Self::dimensions`]; leaf slots in row-major order.
    pub element_count: u32,
    /// Total std430 size in bytes (from [`crate::lower_aggregate_layout::aggregate_size_and_align`]).
    pub total_size: u32,
}

/// sret return buffer: hidden pointer at [`IrFunction::sret_arg`], `memcpy` the aggregate here, then
/// return no values in LPIR.
#[derive(Clone, Debug)]
pub(crate) struct SretCtx {
    pub addr: VReg,
    pub size: u32,
}

/// By-value `in` array: metadata collected while allocating contiguous user param vregs; entry
/// [`LpirOp::Memcpy`] runs after all [`FunctionBuilder::add_param`] calls so scratch vregs never sit
/// between callee parameters (matches [`lpir::IrFunction::user_param_vreg`] layout).
struct PendingInArrayValueArg {
    arg_i: u32,
    lv: Handle<LocalVariable>,
    size: u32,
    dimensions: SmallVec<[u32; 4]>,
    leaf_ty: Handle<Type>,
    leaf_stride: u32,
    element_count: u32,
}

#[allow(
    dead_code,
    reason = "reserved for call lowering and cross-function checks"
)]
pub(crate) struct LowerCtx<'a> {
    pub fb: FunctionBuilder,
    pub module: &'a Module,
    pub func: &'a Function,
    pub ir_module: Option<&'a LpirModule>,
    pub expr_cache: Vec<Option<VRegVec>>,
    pub local_map: BTreeMap<Handle<LocalVariable>, VRegVec>,
    /// Stack (and `in` by-value) aggregates keyed by [`LocalVariable`].
    pub aggregate_map: BTreeMap<Handle<LocalVariable>, AggregateInfo>,
    /// Call results for functions returning an aggregate: [`Expression::CallResult`] → stack slot
    /// (same layout as `aggregate_map` entries, always [`AggregateSlot::Local`]).
    pub(crate) call_result_aggregates: BTreeMap<Handle<Expression>, AggregateInfo>,
    /// Naga emits `.length()` on multi-dim arrays as the outer type-tree size, not GLSL's leftmost `[]`.
    /// Pairs `Load(array)` + `Literal(U32)` (and chained `Literal(I32)` copies) get corrected values here.
    pub(crate) array_length_literal_fixes: BTreeMap<Handle<Expression>, i32>,
    pub param_aliases: BTreeMap<Handle<LocalVariable>, VRegVec>,
    /// VRegs per function argument: scalars/vectors (flattened), or one pointer for aggregates / `inout` bases.
    pub(crate) arg_vregs: BTreeMap<u32, VRegVec>,
    /// `in`/`out`/`inout` parameters: Naga type handle of the pointee (for Load/Store).
    pub(crate) pointer_args: BTreeMap<u32, Handle<Type>>,
    pub func_map: BTreeMap<Handle<Function>, CalleeRef>,
    pub import_map: BTreeMap<String, CalleeRef>,
    pub lpfn_map: BTreeMap<Handle<Function>, CalleeRef>,
    pub return_types: Vec<IrType>,
    /// Present when the shader function returns an aggregate by sret (LPIR void return, memcpy to `addr`).
    pub sret: Option<SretCtx>,
    /// Map from Naga GlobalVariable handle to (vmctx_byte_offset, component_count, is_uniform).
    pub(crate) global_map: GlobalVarMap,
}

impl<'a> LowerCtx<'a> {
    pub(crate) fn new(
        module: &'a Module,
        func: &'a Function,
        name: &str,
        func_map: &BTreeMap<Handle<Function>, CalleeRef>,
        import_map: &BTreeMap<String, CalleeRef>,
        lpfn_map: &BTreeMap<Handle<Function>, CalleeRef>,
        global_map: GlobalVarMap,
    ) -> Result<Self, LowerError> {
        let return_abi = crate::naga_util::func_return_ir_types_with_sret(
            module,
            func.result.as_ref().map(|r| r.ty),
        )?;
        let return_types = return_abi.returns.clone();
        let mut fb = FunctionBuilder::new(name, &return_types);
        let sret = if return_abi.sret.is_some() {
            let addr = fb.add_sret_param();
            Some(SretCtx {
                addr,
                size: return_abi.sret_size,
            })
        } else {
            None
        };

        let mut arg_vregs: BTreeMap<u32, VRegVec> = BTreeMap::new();
        let mut pointer_args: BTreeMap<u32, Handle<Type>> = BTreeMap::new();
        let mut pending_in_array_specs: Vec<PendingInArrayValueArg> = Vec::new();
        for (i, arg) in func.arguments.iter().enumerate() {
            let inner = &module.types[arg.ty].inner;
            match inner {
                TypeInner::Pointer {
                    base,
                    space: AddressSpace::Function,
                } => {
                    // Stack / `inout` / `out` array bases are addresses: use pointer-sized LPIR so host
                    // JIT (64-bit) matches Cranelift's pointer ABI (RV32/WASM still map Pointer → i32).
                    let addr = fb.add_param(IrType::Pointer);
                    arg_vregs.insert(i as u32, smallvec::smallvec![addr]);
                    pointer_args.insert(i as u32, *base);
                }
                TypeInner::Array { .. } => {
                    // By-value `in` aggregate: one pointer param; memcpy from prologue after all params.
                    let (size, _align) =
                        crate::lower_aggregate_layout::aggregate_size_and_align(module, arg.ty)?;
                    let _ = array_ty_pointer_arg_ir_type(module, arg.ty)?;
                    let param_ptr = fb.add_param(IrType::Pointer);
                    let lv = local_for_in_array_value_param(func, module, i as u32)?;
                    let (dimensions, leaf_ty, leaf_stride) =
                        crate::lower_array_multidim::flatten_array_type_shape(module, arg.ty)?;
                    let element_count = dimensions
                        .iter()
                        .try_fold(1u32, |acc, &d| acc.checked_mul(d))
                        .ok_or_else(|| {
                            LowerError::Internal(String::from(
                                "array `in` param: element count overflow",
                            ))
                        })?;
                    pending_in_array_specs.push(PendingInArrayValueArg {
                        arg_i: i as u32,
                        lv,
                        size,
                        dimensions,
                        leaf_ty,
                        leaf_stride,
                        element_count,
                    });
                    arg_vregs.insert(i as u32, smallvec::smallvec![param_ptr]);
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

        let mut pending_in_array_value_param: BTreeMap<Handle<LocalVariable>, AggregateInfo> =
            BTreeMap::new();
        for spec in pending_in_array_specs {
            let param_ptr = arg_vregs
                .get(&spec.arg_i)
                .and_then(|v| v.first())
                .copied()
                .ok_or_else(|| {
                    LowerError::Internal(String::from(
                        "in array value param: missing pointer vreg after param pass",
                    ))
                })?;
            let local_slot = fb.alloc_slot(spec.size);
            let local_addr = fb.alloc_vreg(IrType::Pointer);
            fb.push(LpirOp::SlotAddr {
                dst: local_addr,
                slot: local_slot,
            });
            fb.push(LpirOp::Memcpy {
                dst_addr: local_addr,
                src_addr: param_ptr,
                size: spec.size,
            });
            pending_in_array_value_param.insert(
                spec.lv,
                AggregateInfo {
                    slot: AggregateSlot::Local(local_slot),
                    dimensions: spec.dimensions,
                    leaf_element_ty: spec.leaf_ty,
                    leaf_stride: spec.leaf_stride,
                    element_count: spec.element_count,
                    total_size: spec.size,
                },
            );
        }

        let param_idx = scan_param_argument_indices(module, func);
        let mut param_aliases: BTreeMap<Handle<LocalVariable>, VRegVec> = BTreeMap::new();
        for (lv, arg_i) in &param_idx {
            if let Some(vs) = arg_vregs.get(arg_i) {
                param_aliases.insert(*lv, vs.clone());
            }
        }

        let mut local_map: BTreeMap<Handle<LocalVariable>, VRegVec> = BTreeMap::new();
        let mut aggregate_map: BTreeMap<Handle<LocalVariable>, AggregateInfo> = BTreeMap::new();
        for (lv_handle, var) in func.local_variables.iter() {
            if param_aliases.contains_key(&lv_handle) {
                continue;
            }
            if let Some(info) = pending_in_array_value_param.remove(&lv_handle) {
                aggregate_map.insert(lv_handle, info);
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
                    let std430_total = if array_type_has_inferred_dimension(module, var.ty) {
                        total
                    } else {
                        let (s, _) =
                            crate::lower_aggregate_layout::aggregate_size_and_align(module, var.ty)?;
                        debug_assert_eq!(
                            s, total,
                            "when Naga array type maps to LpsType, slot bytes must match std430"
                        );
                        s
                    };
                    let slot = fb.alloc_slot(std430_total);
                    aggregate_map.insert(
                        lv_handle,
                        AggregateInfo {
                            slot: AggregateSlot::Local(slot),
                            dimensions,
                            leaf_element_ty: leaf_ty,
                            leaf_stride,
                            element_count,
                            total_size: std430_total,
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
        if !pending_in_array_value_param.is_empty() {
            return Err(LowerError::Internal(String::from(
                "in array value param: pending locals did not match any local variable",
            )));
        }

        for (lv_handle, var) in func.local_variables.iter() {
            if param_aliases.contains_key(&lv_handle) {
                continue;
            }
            if let Some(info) = aggregate_map.get(&lv_handle) {
                if var.init.is_none() {
                    if matches!(info.slot, AggregateSlot::Local(_)) {
                        crate::lower_array::zero_fill_array_slot(&mut fb, module, info)?;
                    }
                }
            }
        }

        let expr_cache = vec![None; func.expressions.len()];
        let array_length_literal_fixes =
            crate::lower_array::scan_naga_multidim_array_length_literals(func, &aggregate_map);

        let mut ctx = Self {
            fb,
            module,
            func,
            ir_module: None,
            expr_cache,
            local_map,
            aggregate_map,
            call_result_aggregates: BTreeMap::new(),
            array_length_literal_fixes,
            param_aliases,
            arg_vregs,
            pointer_args,
            func_map: func_map.clone(),
            import_map: import_map.clone(),
            lpfn_map: lpfn_map.clone(),
            return_types,
            sret,
            global_map,
        };

        for (lv_handle, var) in func.local_variables.iter() {
            if ctx.param_aliases.contains_key(&lv_handle) {
                continue;
            }
            let Some(init_h) = var.init else {
                continue;
            };
            if let Some(info) = ctx.aggregate_map.get(&lv_handle).cloned() {
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
                ctx.fb.push(LpirOp::Copy { dst: *d, src: *s });
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
        if self.aggregate_map.contains_key(&lv) {
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
    pub(crate) fn resolve_aggregate(
        &self,
        lv: Handle<LocalVariable>,
    ) -> Result<&AggregateInfo, LowerError> {
        self.aggregate_map
            .get(&lv)
            .ok_or_else(|| LowerError::Internal(format!("local {lv:?} is not an aggregate array")))
    }

    /// [`AggregateInfo`] for a peeled subscript chain root (local, pointer param, or call result).
    pub(crate) fn aggregate_info_for_subscript_root(
        &self,
        root: crate::lower_array_multidim::ArraySubscriptRoot,
    ) -> Result<Option<AggregateInfo>, LowerError> {
        use crate::lower_array_multidim::ArraySubscriptRoot;
        match root {
            ArraySubscriptRoot::Local(lv) => Ok(self.aggregate_map.get(&lv).cloned()),
            ArraySubscriptRoot::Param(arg_i) => {
                let Some(&pointee) = self.pointer_args.get(&arg_i) else {
                    return Ok(None);
                };
                if !matches!(self.module.types[pointee].inner, TypeInner::Array { .. }) {
                    return Ok(None);
                }
                let (dimensions, leaf_ty, leaf_stride) =
                    crate::lower_array_multidim::flatten_array_type_shape(self.module, pointee)?;
                let element_count = dimensions
                    .iter()
                    .try_fold(1u32, |acc, &d| acc.checked_mul(d))
                    .ok_or_else(|| {
                        LowerError::Internal(String::from(
                            "aggregate_info_for_subscript_root: element count overflow",
                        ))
                    })?;
                let (total_size, _align) =
                    crate::lower_aggregate_layout::aggregate_size_and_align(self.module, pointee)?;
                Ok(Some(AggregateInfo {
                    slot: AggregateSlot::Param(arg_i),
                    dimensions,
                    leaf_element_ty: leaf_ty,
                    leaf_stride,
                    element_count,
                    total_size,
                }))
            }
            ArraySubscriptRoot::CallResult(expr) => {
                Ok(self.call_result_aggregates.get(&expr).cloned())
            }
        }
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

/// Naga GLSL: the [`LocalVariable`] for a by-value `in` array param (`float a[4]`) is matched by
/// parameter name and type to `func.arguments[i]`.
fn local_for_in_array_value_param(
    func: &Function,
    module: &Module,
    param_index: u32,
) -> Result<Handle<LocalVariable>, LowerError> {
    let i = param_index as usize;
    let arg = func
        .arguments
        .get(i)
        .ok_or_else(|| LowerError::Internal(String::from("in_array: bad argument index")))?;
    if !matches!(&module.types[arg.ty].inner, TypeInner::Array { .. }) {
        return Err(LowerError::Internal(String::from(
            "in_array: not an array type",
        )));
    }
    if let Some(name) = &arg.name {
        for (lv, v) in func.local_variables.iter() {
            if v.ty == arg.ty && v.name.as_deref() == Some(name.as_str()) {
                return Ok(lv);
            }
        }
    }
    let mut found = None;
    for (lv, v) in func.local_variables.iter() {
        if v.ty == arg.ty {
            if found.is_some() {
                return Err(LowerError::Internal(String::from(
                    "in_array: ambiguous local (use matching parameter name)",
                )));
            }
            found = Some(lv);
        }
    }
    found.ok_or_else(|| {
        LowerError::Internal(String::from(
            "in_array: no local variable for by-value array parameter",
        ))
    })
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
                        // `in T[]` uses a real stack array in `aggregate_map`; do not alias as flat vregs.
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
