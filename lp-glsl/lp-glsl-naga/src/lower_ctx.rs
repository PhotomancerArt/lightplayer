//! Per-function lowering context (builder, expression cache, local maps).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{CalleeRef, FunctionBuilder, IrModule, IrType, VReg};
use naga::{Expression, Function, Handle, LocalVariable, Module, ScalarKind, Statement, TypeInner};

use crate::lower_error::LowerError;
use crate::lower_expr;

// `ir_module`, `func_map`, `import_map` are for call/math lowering; `return_types` may be used for checks.
#[allow(
    dead_code,
    reason = "reserved for call lowering and cross-function checks"
)]
pub(crate) struct LowerCtx<'a> {
    pub fb: FunctionBuilder,
    pub module: &'a Module,
    pub func: &'a Function,
    pub ir_module: Option<&'a IrModule>,
    pub expr_cache: Vec<Option<VReg>>,
    pub local_map: BTreeMap<Handle<LocalVariable>, VReg>,
    pub param_aliases: BTreeMap<Handle<LocalVariable>, VReg>,
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

        for arg in func.arguments.iter() {
            let inner = &module.types[arg.ty].inner;
            let ty = naga_type_to_ir_type(inner)?;
            fb.add_param(ty);
        }

        let arg_idx_to_vreg: BTreeMap<u32, VReg> = (0..func.arguments.len() as u32)
            .map(|i| (i, VReg(i)))
            .collect();

        let param_idx = scan_param_argument_indices(func);
        let mut param_aliases: BTreeMap<Handle<LocalVariable>, VReg> = BTreeMap::new();
        for (lv, arg_i) in &param_idx {
            if let Some(v) = arg_idx_to_vreg.get(arg_i) {
                param_aliases.insert(*lv, *v);
            }
        }

        let mut local_map: BTreeMap<Handle<LocalVariable>, VReg> = BTreeMap::new();
        for (lv_handle, var) in func.local_variables.iter() {
            if param_aliases.contains_key(&lv_handle) {
                continue;
            }
            let inner = &module.types[var.ty].inner;
            let ty = naga_type_to_ir_type(inner)?;
            let v = fb.alloc_vreg(ty);
            local_map.insert(lv_handle, v);
        }

        let expr_cache = vec![None; func.expressions.len()];

        Ok(Self {
            fb,
            module,
            func,
            ir_module: None,
            expr_cache,
            local_map,
            param_aliases,
            func_map: func_map.clone(),
            import_map: import_map.clone(),
            lpfx_map: lpfx_map.clone(),
            return_types,
        })
    }

    pub(crate) fn finish(self) -> lpir::IrFunction {
        self.fb.finish()
    }

    pub(crate) fn resolve_local(&self, lv: Handle<LocalVariable>) -> Result<VReg, LowerError> {
        if let Some(v) = self.param_aliases.get(&lv) {
            return Ok(*v);
        }
        self.local_map
            .get(&lv)
            .copied()
            .ok_or_else(|| LowerError::Internal(format!("unknown local variable {lv:?}")))
    }

    pub(crate) fn ensure_expr(
        &mut self,
        expr: Handle<naga::Expression>,
    ) -> Result<VReg, LowerError> {
        let i = expr.index();
        if let Some(v) = self.expr_cache.get(i).and_then(|c| *c) {
            return Ok(v);
        }
        let v = lower_expr::lower_expr(self, expr)?;
        if let Some(slot) = self.expr_cache.get_mut(i) {
            *slot = Some(v);
        }
        Ok(v)
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

pub(crate) fn naga_type_to_ir_type(inner: &TypeInner) -> Result<IrType, LowerError> {
    match *inner {
        TypeInner::Scalar(scalar) => naga_scalar_to_ir_type(scalar.kind),
        _ => Err(LowerError::UnsupportedType(format!(
            "only scalar locals/parameters supported, got {inner:?}"
        ))),
    }
}

fn func_return_ir_types(module: &Module, func: &Function) -> Result<Vec<IrType>, LowerError> {
    let Some(res) = &func.result else {
        return Ok(Vec::new());
    };
    match &module.types[res.ty].inner {
        TypeInner::Scalar(scalar) => Ok(vec![naga_scalar_to_ir_type(scalar.kind)?]),
        _ => Err(LowerError::UnsupportedType(String::from(
            "only scalar return types supported",
        ))),
    }
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
