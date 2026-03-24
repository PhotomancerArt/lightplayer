//! Per-function lowering context (builder, expression cache, local maps).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{CalleeRef, FunctionBuilder, IrModule, IrType, Op, VReg};
use naga::{
    Expression, Function, Handle, LocalVariable, Module, ScalarKind, Statement, TypeInner,
    VectorSize,
};
use smallvec::SmallVec;

use crate::lower_error::LowerError;
use crate::lower_expr;

pub(crate) type VRegVec = SmallVec<[VReg; 4]>;
pub(crate) type IrTypeVec = SmallVec<[IrType; 4]>;

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
    pub param_aliases: BTreeMap<Handle<LocalVariable>, VRegVec>,
    /// VRegs per function argument index (flattened scalars).
    pub(crate) arg_vregs: BTreeMap<u32, VRegVec>,
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
        for (i, arg) in func.arguments.iter().enumerate() {
            let inner = &module.types[arg.ty].inner;
            let tys = naga_type_to_ir_types(inner)?;
            let mut vregs = VRegVec::new();
            for ty in &tys {
                let v = fb.add_param(*ty);
                vregs.push(v);
            }
            arg_vregs.insert(i as u32, vregs);
        }

        let param_idx = scan_param_argument_indices(func);
        let mut param_aliases: BTreeMap<Handle<LocalVariable>, VRegVec> = BTreeMap::new();
        for (lv, arg_i) in &param_idx {
            if let Some(vs) = arg_vregs.get(arg_i) {
                param_aliases.insert(*lv, vs.clone());
            }
        }

        let mut local_map: BTreeMap<Handle<LocalVariable>, VRegVec> = BTreeMap::new();
        for (lv_handle, var) in func.local_variables.iter() {
            if param_aliases.contains_key(&lv_handle) {
                continue;
            }
            let inner = &module.types[var.ty].inner;
            let tys = naga_type_to_ir_types(inner)?;
            let mut vregs = VRegVec::new();
            for ty in &tys {
                vregs.push(fb.alloc_vreg(*ty));
            }
            local_map.insert(lv_handle, vregs);
        }

        let expr_cache = vec![None; func.expressions.len()];

        let mut ctx = Self {
            fb,
            module,
            func,
            ir_module: None,
            expr_cache,
            local_map,
            param_aliases,
            arg_vregs,
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
        if let Some(v) = self.param_aliases.get(&lv) {
            return Ok(v.clone());
        }
        self.local_map
            .get(&lv)
            .cloned()
            .ok_or_else(|| LowerError::Internal(format!("unknown local variable {lv:?}")))
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

pub(crate) fn vector_size_usize(size: VectorSize) -> usize {
    match size {
        VectorSize::Bi => 2,
        VectorSize::Tri => 3,
        VectorSize::Quad => 4,
    }
}

pub(crate) fn naga_scalar_to_ir_type(kind: ScalarKind) -> Result<IrType, LowerError> {
    match kind {
        ScalarKind::Float => Ok(IrType::F32),
        ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => Ok(IrType::I32),
        ScalarKind::AbstractInt | ScalarKind::AbstractFloat => Err(LowerError::UnsupportedType(
            String::from("abstract numeric type"),
        )),
    }
}

pub(crate) fn naga_type_to_ir_types(inner: &TypeInner) -> Result<IrTypeVec, LowerError> {
    match *inner {
        TypeInner::Scalar(scalar) => {
            let t = naga_scalar_to_ir_type(scalar.kind)?;
            Ok(smallvec::smallvec![t])
        }
        TypeInner::Vector { size, scalar, .. } => {
            let t = naga_scalar_to_ir_type(scalar.kind)?;
            let n = vector_size_usize(size);
            Ok(SmallVec::from_elem(t, n))
        }
        TypeInner::Matrix {
            columns,
            rows,
            scalar,
            ..
        } => {
            let t = naga_scalar_to_ir_type(scalar.kind)?;
            let n = vector_size_usize(columns) * vector_size_usize(rows);
            Ok(SmallVec::from_elem(t, n))
        }
        _ => Err(LowerError::UnsupportedType(format!(
            "unsupported type for LPIR: {inner:?}"
        ))),
    }
}

/// Single scalar IR type; use [`naga_type_to_ir_types`] for vectors and matrices.
#[allow(
    dead_code,
    reason = "convenience for scalar-only call sites and future passes"
)]
pub(crate) fn naga_type_to_ir_type(inner: &TypeInner) -> Result<IrType, LowerError> {
    let tys = naga_type_to_ir_types(inner)?;
    if tys.len() != 1 {
        return Err(LowerError::UnsupportedType(String::from(
            "expected a single scalar IR type",
        )));
    }
    Ok(tys[0])
}

pub(crate) fn naga_type_width(inner: &TypeInner) -> usize {
    match *inner {
        TypeInner::Scalar(_) => 1,
        TypeInner::Vector { size, .. } => vector_size_usize(size),
        TypeInner::Matrix { columns, rows, .. } => {
            vector_size_usize(columns) * vector_size_usize(rows)
        }
        _ => 1,
    }
}

pub(crate) fn func_return_ir_types(
    module: &Module,
    func: &Function,
) -> Result<Vec<IrType>, LowerError> {
    let Some(res) = &func.result else {
        return Ok(Vec::new());
    };
    let inner = &module.types[res.ty].inner;
    let tys = naga_type_to_ir_types(inner)?;
    Ok(tys.to_vec())
}

fn scan_param_argument_indices(func: &Function) -> BTreeMap<Handle<LocalVariable>, u32> {
    let mut m = BTreeMap::new();
    fn walk_block(
        block: &naga::Block,
        func: &Function,
        m: &mut BTreeMap<Handle<LocalVariable>, u32>,
    ) {
        for stmt in block.iter() {
            match stmt {
                Statement::Store { pointer, value } => {
                    if let (Expression::LocalVariable(lv), Expression::FunctionArgument(idx)) =
                        (&func.expressions[*pointer], &func.expressions[*value])
                    {
                        m.insert(*lv, *idx);
                    }
                }
                Statement::Block(inner) => walk_block(inner, func, m),
                Statement::If { accept, reject, .. } => {
                    walk_block(accept, func, m);
                    walk_block(reject, func, m);
                }
                Statement::Switch { cases, .. } => {
                    for case in cases.iter() {
                        walk_block(&case.body, func, m);
                    }
                }
                Statement::Loop {
                    body, continuing, ..
                } => {
                    walk_block(body, func, m);
                    walk_block(continuing, func, m);
                }
                _ => {}
            }
        }
    }
    walk_block(&func.body, func, &mut m);
    m
}
