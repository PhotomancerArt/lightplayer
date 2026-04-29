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
use crate::readonly_in_scan::in_aggregate_param_read_only;
use lps_shared::{LpsType, TextureBindingSpec};

/// See [`readonly_in_scan::local_for_in_aggregate_value_param_optional`].
pub(crate) use crate::readonly_in_scan::local_for_in_aggregate_value_param_optional;

/// Deferred VMContext addressing for uniform struct fields of array type (`uniform { T arr[n]; }`).
#[derive(Clone, Debug)]
pub(crate) enum UniformVmctxDeferred {
    /// Array member at `base_offset` (bytes from VMContext); elements use `stride` and `element` layout.
    ArrayField {
        base_offset: u32,
        element: lps_shared::LpsType,
        stride: u32,
        len: u32,
    },
    /// Dynamic element: `addr_vreg` points at `VMCTX + (base_offset + index * stride)` for the array field.
    ElementAddr {
        addr_vreg: VReg,
        element: lps_shared::LpsType,
    },
}

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
                if matches!(size, ArraySize::Pending(_) | ArraySize::Dynamic) {
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
    /// When false, no VMContext bytes are reserved; loads must not use [`Self::byte_offset`].
    /// Parse-synthesized `__lp_samp_*` Naga sampler globals use this (see `parse.rs`).
    pub vmctx_backed: bool,
}

/// Map from Naga GlobalVariable handle to its lowering info.
pub(crate) type GlobalVarMap = BTreeMap<Handle<GlobalVariable>, GlobalVarInfo>;

/// Naga stores `uniform Block { } name` instance as a function [`LocalVariable`] initialized from
/// the corresponding [`GlobalVariable`]; map proxy locals to that global for VMContext loads.
fn scan_uniform_instance_local_to_global(
    module: &Module,
    func: &Function,
) -> BTreeMap<Handle<LocalVariable>, Handle<GlobalVariable>> {
    let mut m = BTreeMap::new();
    fn walk_block(
        block: &naga::Block,
        module: &Module,
        func: &Function,
        m: &mut BTreeMap<Handle<LocalVariable>, Handle<GlobalVariable>>,
    ) {
        for stmt in block.iter() {
            match stmt {
                Statement::Store { pointer, value } => {
                    if let Expression::LocalVariable(lv) = &func.expressions[*pointer] {
                        let mut v = *value;
                        while let Expression::Load { pointer: p } = &func.expressions[v] {
                            v = *p;
                        }
                        if let Expression::GlobalVariable(gv) = &func.expressions[v] {
                            if module.global_variables[*gv].space == AddressSpace::Uniform {
                                m.insert(*lv, *gv);
                            }
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
    for (lv, var) in func.local_variables.iter() {
        if let Some(mut h) = var.init {
            while let Expression::Load { pointer } = &func.expressions[h] {
                h = *pointer;
            }
            if let Expression::GlobalVariable(gv) = &func.expressions[h] {
                if module.global_variables[*gv].space == AddressSpace::Uniform {
                    m.entry(lv).or_insert(*gv);
                }
            }
        }
    }
    m
}

pub(crate) type VRegVec = SmallVec<[VReg; 4]>;

/// Where aggregate (array) element data lives: stack slot or `inout`/`out` parameter buffer
/// ([`LowerCtx::arg_vregs`]\[0\] = pointer).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AggregateSlot {
    Local(SlotId),
    /// Pointer-to-array formal (`out` / `inout`); base address is [`LowerCtx::arg_vregs`]\[0\].
    Param(u32),
    /// By-value `in` aggregate that the M5 scan proved read-only: no stack copy; base is the same
    /// pointer as [`LowerCtx::arg_vregs`]\[`arg_i`]\[0\] (like [`AggregateSlot::Param`] for addressing).
    ParamReadOnly(u32),
    /// Array-typed private global: base address is `VMContext +` [`GlobalVarInfo::byte_offset`].
    Global(Handle<GlobalVariable>),
}

/// Stack [`SlotId`] and layout metadata for one aggregate-typed value (M1: arrays only).
#[derive(Clone, Debug)]
pub(crate) struct AggregateInfo {
    pub slot: AggregateSlot,
    pub layout: crate::naga_util::AggregateLayout,
    /// Naga value type of this aggregate (unwrapped array / future struct) for LpsType and layout.
    pub naga_ty: Handle<Type>,
}

impl AggregateInfo {
    /// Array-only — panics on Struct (phase 03 introduces struct accessors).
    pub(crate) fn dimensions(&self) -> &[u32] {
        match &self.layout.kind {
            crate::naga_util::AggregateKind::Array { dimensions, .. } => dimensions,
            crate::naga_util::AggregateKind::Struct { .. } => {
                unreachable!("AggregateInfo::dimensions on struct")
            }
        }
    }

    pub(crate) fn leaf_element_ty(&self) -> Handle<Type> {
        match &self.layout.kind {
            crate::naga_util::AggregateKind::Array {
                leaf_element_ty, ..
            } => *leaf_element_ty,
            crate::naga_util::AggregateKind::Struct { .. } => {
                unreachable!("AggregateInfo::leaf_element_ty on struct")
            }
        }
    }

    pub(crate) fn leaf_stride(&self) -> u32 {
        match &self.layout.kind {
            crate::naga_util::AggregateKind::Array { leaf_stride, .. } => *leaf_stride,
            crate::naga_util::AggregateKind::Struct { .. } => {
                unreachable!("AggregateInfo::leaf_stride on struct")
            }
        }
    }

    pub(crate) fn element_count(&self) -> u32 {
        match &self.layout.kind {
            crate::naga_util::AggregateKind::Array { element_count, .. } => *element_count,
            crate::naga_util::AggregateKind::Struct { .. } => {
                unreachable!("AggregateInfo::element_count on struct")
            }
        }
    }

    pub(crate) fn total_size(&self) -> u32 {
        self.layout.total_size
    }

    pub(crate) fn align(&self) -> u32 {
        self.layout.align
    }
}

/// Debug-only: [`AggregateSlot::ParamReadOnly`] must never be written — proven by
/// [`readonly_in_scan`](crate::readonly_in_scan). If this trips, the scan and lowering disagree.
#[inline]
pub(crate) fn debug_assert_not_param_readonly_aggregate_store(
    info: &AggregateInfo,
    site: &'static str,
) {
    debug_assert!(
        !matches!(info.slot, AggregateSlot::ParamReadOnly(_)),
        "store to read-only `in` aggregate (scan miss or lowering bug): {site}"
    );
}

/// sret return buffer: hidden pointer at [`IrFunction::sret_arg`], `memcpy` the aggregate here, then
/// return no values in LPIR.
#[derive(Clone, Debug)]
pub(crate) struct SretCtx {
    pub addr: VReg,
    pub size: u32,
}

/// By-value `in` array or struct: metadata for prologue [`LpirOp::Memcpy`] (after all
/// [`FunctionBuilder::add_param`] calls).
struct PendingInAggregateValueArg {
    arg_i: u32,
    lv: Handle<LocalVariable>,
    naga_ty: Handle<Type>,
    layout: crate::naga_util::AggregateLayout,
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
    /// Map from Naga GlobalVariable handle to VMContext / lowering info.
    pub(crate) global_map: GlobalVarMap,
    /// [`Expression::index`] → deferred uniform array field / indexed element (see [`UniformVmctxDeferred`]).
    pub(crate) uniform_vmctx_deferred: BTreeMap<usize, UniformVmctxDeferred>,
    /// `uniform Block { } instance` locals → backing [`GlobalVariable`] (uniform).
    pub(crate) uniform_instance_locals: BTreeMap<Handle<LocalVariable>, Handle<GlobalVariable>>,
    /// Compile-time [`TextureBindingSpec`] keyed by sampler uniform name ([`crate::LowerOptions`]).
    pub(crate) texture_specs: &'a BTreeMap<String, TextureBindingSpec>,
    /// Mirrors [`crate::LowerOptions::texel_fetch_bounds`] for `texelFetch` lowering.
    pub(crate) texel_fetch_bounds: lpir::TexelFetchBoundsMode,
    /// Uniform block metadata for canonical paths and std430 offsets (same as [`LpsModuleSig::uniforms_type`]).
    pub(crate) uniforms_type: Option<&'a LpsType>,
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
        texture_specs: &'a BTreeMap<String, TextureBindingSpec>,
        texel_fetch_bounds: lpir::TexelFetchBoundsMode,
        uniforms_type: Option<&'a LpsType>,
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
        let mut pending_in_aggregate_specs: Vec<PendingInAggregateValueArg> = Vec::new();
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
                TypeInner::Array { .. } | TypeInner::Struct { .. } => {
                    // By-value `in` aggregate: one pointer param; memcpy from prologue after all params.
                    let layout =
                        crate::naga_util::aggregate_layout(module, arg.ty)?.ok_or_else(|| {
                            LowerError::Internal(String::from(
                                "by-value `in` param: expected aggregate layout",
                            ))
                        })?;
                    let _ = array_ty_pointer_arg_ir_type(module, arg.ty)?;
                    let param_ptr = fb.add_param(IrType::Pointer);
                    if let Some(lv) =
                        local_for_in_aggregate_value_param_optional(func, module, i as u32)?
                    {
                        pending_in_aggregate_specs.push(PendingInAggregateValueArg {
                            arg_i: i as u32,
                            lv,
                            naga_ty: arg.ty,
                            layout: layout.clone(),
                        });
                    }
                    arg_vregs.insert(i as u32, smallvec::smallvec![param_ptr]);
                }
                _ => {
                    let tys = naga_type_to_ir_types(module, inner)?;
                    let mut vregs = VRegVec::new();
                    for ty in &tys {
                        let v = fb.add_param(*ty);
                        vregs.push(v);
                    }
                    arg_vregs.insert(i as u32, vregs);
                }
            }
        }

        let in_aggregate_read_only = in_aggregate_param_read_only(module, func)?;

        let mut pending_in_aggregate_value_param: BTreeMap<Handle<LocalVariable>, AggregateInfo> =
            BTreeMap::new();
        for spec in pending_in_aggregate_specs {
            let use_readonly = in_aggregate_read_only
                .get(&spec.arg_i)
                .copied()
                .unwrap_or(false);
            if use_readonly {
                pending_in_aggregate_value_param.insert(
                    spec.lv,
                    AggregateInfo {
                        slot: AggregateSlot::ParamReadOnly(spec.arg_i),
                        layout: spec.layout,
                        naga_ty: spec.naga_ty,
                    },
                );
            } else {
                let param_ptr = arg_vregs
                    .get(&spec.arg_i)
                    .and_then(|v| v.first())
                    .copied()
                    .ok_or_else(|| {
                        LowerError::Internal(String::from(
                            "in array value param: missing pointer vreg after param pass",
                        ))
                    })?;
                let local_slot = fb.alloc_slot(spec.layout.total_size);
                let local_addr = fb.alloc_vreg(IrType::Pointer);
                fb.push(LpirOp::SlotAddr {
                    dst: local_addr,
                    slot: local_slot,
                });
                fb.push(LpirOp::Memcpy {
                    dst_addr: local_addr,
                    src_addr: param_ptr,
                    size: spec.layout.total_size,
                });
                pending_in_aggregate_value_param.insert(
                    spec.lv,
                    AggregateInfo {
                        slot: AggregateSlot::Local(local_slot),
                        layout: spec.layout,
                        naga_ty: spec.naga_ty,
                    },
                );
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
        let mut aggregate_map: BTreeMap<Handle<LocalVariable>, AggregateInfo> = BTreeMap::new();
        for (lv_handle, var) in func.local_variables.iter() {
            if param_aliases.contains_key(&lv_handle) {
                continue;
            }
            if let Some(info) = pending_in_aggregate_value_param.remove(&lv_handle) {
                aggregate_map.insert(lv_handle, info);
                continue;
            }
            match &module.types[var.ty].inner {
                TypeInner::Array { .. } => {
                    if array_type_has_inferred_dimension(module, var.ty) {
                        let (dimensions, leaf_ty, leaf_stride) =
                            crate::lower_array_multidim::flatten_local_array_shape(
                                module, func, &var,
                            )?;
                        let element_count = dimensions
                            .iter()
                            .try_fold(1u32, |acc, &d| acc.checked_mul(d))
                            .ok_or_else(|| {
                                LowerError::Internal(String::from("array element count overflow"))
                            })?;
                        let total = element_count.checked_mul(leaf_stride).ok_or_else(|| {
                            LowerError::Internal(String::from("array slot size overflow"))
                        })?;
                        let std430_total = total;
                        let align = crate::lower_aggregate_layout::aggregate_size_and_align(
                            module, leaf_ty,
                        )?
                        .1;
                        let layout = crate::naga_util::AggregateLayout {
                            kind: crate::naga_util::AggregateKind::Array {
                                dimensions,
                                leaf_element_ty: leaf_ty,
                                leaf_stride,
                                element_count,
                            },
                            total_size: std430_total,
                            align,
                        };
                        let slot = fb.alloc_slot(std430_total);
                        aggregate_map.insert(
                            lv_handle,
                            AggregateInfo {
                                slot: AggregateSlot::Local(slot),
                                layout,
                                naga_ty: var.ty,
                            },
                        );
                    } else {
                        let layout = crate::naga_util::aggregate_layout(module, var.ty)?
                            .ok_or_else(|| {
                                LowerError::Internal(String::from(
                                    "local fixed array: expected aggregate layout",
                                ))
                            })?;
                        let slot = fb.alloc_slot(layout.total_size);
                        aggregate_map.insert(
                            lv_handle,
                            AggregateInfo {
                                slot: AggregateSlot::Local(slot),
                                layout,
                                naga_ty: var.ty,
                            },
                        );
                    }
                }
                TypeInner::Struct { .. } => {
                    let layout =
                        crate::naga_util::aggregate_layout(module, var.ty)?.ok_or_else(|| {
                            LowerError::Internal(String::from("struct local: expected layout"))
                        })?;
                    let slot = fb.alloc_slot(layout.total_size);
                    aggregate_map.insert(
                        lv_handle,
                        AggregateInfo {
                            slot: AggregateSlot::Local(slot),
                            layout,
                            naga_ty: var.ty,
                        },
                    );
                }
                _ => {
                    let inner = &module.types[var.ty].inner;
                    let tys = naga_type_to_ir_types(module, inner)?;
                    let mut vregs = VRegVec::new();
                    for ty in &tys {
                        vregs.push(fb.alloc_vreg(*ty));
                    }
                    local_map.insert(lv_handle, vregs);
                }
            }
        }
        if !pending_in_aggregate_value_param.is_empty() {
            return Err(LowerError::Internal(String::from(
                "in aggregate value param: pending locals did not match any local variable",
            )));
        }

        for (lv_handle, var) in func.local_variables.iter() {
            if param_aliases.contains_key(&lv_handle) {
                continue;
            }
            if let Some(info) = aggregate_map.get(&lv_handle) {
                if var.init.is_none() {
                    if matches!(info.slot, AggregateSlot::Local(_)) {
                        match &info.layout.kind {
                            crate::naga_util::AggregateKind::Array { .. } => {
                                crate::lower_array::zero_fill_array_slot(&mut fb, module, info)?;
                            }
                            crate::naga_util::AggregateKind::Struct { .. } => {
                                crate::lower_struct::zero_fill_struct_slot(&mut fb, module, info)?;
                            }
                        }
                    }
                }
            }
        }

        let expr_cache = vec![None; func.expressions.len()];
        let array_length_literal_fixes =
            crate::lower_array::scan_naga_multidim_array_length_literals(func, &aggregate_map);
        let uniform_instance_locals = scan_uniform_instance_local_to_global(module, func);

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
            uniform_vmctx_deferred: BTreeMap::new(),
            uniform_instance_locals,
            texture_specs,
            texel_fetch_bounds,
            uniforms_type,
        };

        for (lv_handle, var) in func.local_variables.iter() {
            if ctx.param_aliases.contains_key(&lv_handle) {
                continue;
            }
            let Some(init_h) = var.init else {
                continue;
            };
            if let Some(info) = ctx.aggregate_map.get(&lv_handle).cloned() {
                if matches!(
                    &info.layout.kind,
                    crate::naga_util::AggregateKind::Array { .. }
                ) {
                    crate::lower_array::lower_array_initializer(&mut ctx, &info, init_h)?;
                } else {
                    if matches!(info.slot, AggregateSlot::ParamReadOnly(_))
                        && matches!(&func.expressions[init_h], Expression::ZeroValue(_))
                    {
                        continue;
                    }
                    debug_assert_not_param_readonly_aggregate_store(
                        &info,
                        "LowerCtx::new: struct local with initializer",
                    );
                    let lps_ty =
                        crate::lower_aggregate_layout::naga_to_lps_type(ctx.module, info.naga_ty)?;
                    let base =
                        crate::lower_array::aggregate_storage_base_vreg(&mut ctx, &info.slot)?;
                    crate::lower_aggregate_write::store_lps_value_into_slot(
                        &mut ctx,
                        base,
                        0,
                        info.naga_ty,
                        &lps_ty,
                        init_h,
                        Some(&info.layout),
                    )?;
                }
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
    ///
    /// - [`ArraySubscriptRoot::Local`]: from [`Self::aggregate_map`], including by-value `in`
    ///   aggregates ([`AggregateSlot::ParamReadOnly`]) and stack locals. Not the same as
    ///   [`Self::pointer_args`] (those are `inout` / `out` / pointer formals only).
    /// - [`ArraySubscriptRoot::Param`]: **only** pointer array/struct formals in [`Self::pointer_args`],
    ///   not by-value `in` (those are never a `Param` root here; Naga uses a `LocalVariable` proxy).
    pub(crate) fn aggregate_info_for_subscript_root(
        &self,
        root: crate::lower_array_multidim::ArraySubscriptRoot,
    ) -> Result<Option<AggregateInfo>, LowerError> {
        use crate::lower_array_multidim::ArraySubscriptRoot;
        match root {
            ArraySubscriptRoot::Local(lv) => Ok(self.aggregate_map.get(&lv).cloned()),
            ArraySubscriptRoot::Param(arg_i) => {
                // `pointer_args` only: by-value `in` aggregates are `Local` + `ParamReadOnly` in `aggregate_map`.
                let Some(&pointee) = self.pointer_args.get(&arg_i) else {
                    return Ok(None);
                };
                if let Some(layout) = crate::naga_util::aggregate_layout(self.module, pointee)? {
                    return Ok(Some(AggregateInfo {
                        slot: AggregateSlot::Param(arg_i),
                        layout,
                        naga_ty: pointee,
                    }));
                }
                Ok(None)
            }
            ArraySubscriptRoot::CallResult(expr) => {
                Ok(self.call_result_aggregates.get(&expr).cloned())
            }
            ArraySubscriptRoot::Global(gv) => {
                let mut naga_ty = self.module.global_variables[gv].ty;
                if let TypeInner::Pointer { base, .. } = &self.module.types[naga_ty].inner {
                    naga_ty = *base;
                }
                Ok(
                    crate::naga_util::aggregate_layout(self.module, naga_ty)?.map(|layout| {
                        AggregateInfo {
                            slot: AggregateSlot::Global(gv),
                            layout,
                            naga_ty,
                        }
                    }),
                )
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
                        let is_struct_val = arg_ty.is_some_and(|h| {
                            matches!(&module.types[h].inner, TypeInner::Struct { .. })
                        });
                        // `in T[]` / `in` struct: stack slot in `aggregate_map`; do not alias as flat vregs.
                        if !is_ptr && !is_array_val && !is_struct_val {
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
