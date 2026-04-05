//! Naga module → LPIR [`lpir::IrModule`] lowering entry point.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;

use lpir::{CalleeRef, ImportDecl, IrFunction, IrModule, IrType, ModuleBuilder};
use lpvm::{GlslFunctionMeta, GlslModuleMeta};
use naga::{Function, Handle, Module};

use crate::NagaModule;
use crate::lower_ctx::LowerCtx;
use crate::lower_error::LowerError;
use crate::lower_lpfx;

/// Lower a parsed [`NagaModule`] to LPIR (scalarized vectors and matrices).
///
/// Registers `@glsl::*`, `@lpir::*`, and `@lpfx::*` imports as needed, then emits one [`lpir::IrFunction`] per
/// entry in [`NagaModule::functions`]. Fails with [`LowerError`] on unsupported Naga IR outside the
/// scalar subset.
pub fn lower(naga_module: &NagaModule) -> Result<(IrModule, GlslModuleMeta), LowerError> {
    let mut mb = ModuleBuilder::new();
    let import_map = register_math_imports(&mut mb);
    let lpfx_map = lower_lpfx::register_lpfx_imports(&mut mb, naga_module)?;
    let import_count = mb.import_count();

    let mut func_map: BTreeMap<Handle<Function>, CalleeRef> = BTreeMap::new();
    for (i, (handle, _)) in naga_module.functions.iter().enumerate() {
        func_map.insert(*handle, CalleeRef(import_count.saturating_add(i as u32)));
    }

    let mut glsl_meta = GlslModuleMeta::default();
    for (handle, info) in &naga_module.functions {
        let func = &naga_module.module.functions[*handle];
        let ir = lower_function(
            &naga_module.module,
            func,
            info.name.as_str(),
            &func_map,
            &import_map,
            &lpfx_map,
        )
        .map_err(|e| LowerError::InFunction {
            name: info.name.clone(),
            inner: Box::new(e),
        })?;
        glsl_meta.functions.push(GlslFunctionMeta {
            name: info.name.clone(),
            params: info.params.clone(),
            return_type: info.return_type.clone(),
        });
        mb.add_function(ir);
    }
    Ok((mb.finish(), glsl_meta))
}

fn register_math_imports(mb: &mut ModuleBuilder) -> BTreeMap<String, CalleeRef> {
    let mut m = BTreeMap::new();
    let mut reg =
        |module: &str, name: &str, params: &[IrType], rets: &[IrType], needs_vmctx: bool| {
            let r = mb.add_import(ImportDecl {
                module_name: String::from(module),
                func_name: String::from(name),
                param_types: params.to_vec(),
                return_types: rets.to_vec(),
                lpfx_glsl_params: None,
                needs_vmctx,
            });
            m.insert(format!("{module}::{name}"), r);
        };
    let f1 = &[IrType::F32];
    let r1 = &[IrType::F32];
    let u1 = &[IrType::I32];
    reg("lpir", "sqrt", f1, r1, false);
    reg("glsl", "sin", f1, r1, false);
    reg("glsl", "cos", f1, r1, false);
    reg("glsl", "tan", f1, r1, false);
    reg("glsl", "asin", f1, r1, false);
    reg("glsl", "acos", f1, r1, false);
    reg("glsl", "atan", f1, r1, false);
    reg("glsl", "atan2", &[IrType::F32, IrType::F32], r1, false);
    reg("glsl", "sinh", f1, r1, false);
    reg("glsl", "cosh", f1, r1, false);
    reg("glsl", "tanh", f1, r1, false);
    reg("glsl", "asinh", f1, r1, false);
    reg("glsl", "acosh", f1, r1, false);
    reg("glsl", "atanh", f1, r1, false);
    reg("glsl", "exp", f1, r1, false);
    reg("glsl", "exp2", f1, r1, false);
    reg("glsl", "log", f1, r1, false);
    reg("glsl", "log2", f1, r1, false);
    reg("glsl", "pow", &[IrType::F32, IrType::F32], r1, false);
    reg("glsl", "ldexp", &[IrType::F32, IrType::I32], r1, false);
    reg("glsl", "round", f1, r1, false);
    reg("vm", "__lp_get_fuel", &[], u1, true);
    m
}

fn lower_function(
    module: &Module,
    func: &Function,
    name: &str,
    func_map: &BTreeMap<Handle<Function>, CalleeRef>,
    import_map: &BTreeMap<String, CalleeRef>,
    lpfx_map: &BTreeMap<Handle<Function>, CalleeRef>,
) -> Result<IrFunction, LowerError> {
    let mut ctx = LowerCtx::new(module, func, name, func_map, import_map, lpfx_map)?;
    crate::lower_stmt::lower_block(&mut ctx, &func.body)?;
    if func.result.is_none() && crate::lower_stmt::void_block_missing_return(&func.body) {
        ctx.fb.push_return(&[]);
    }
    Ok(ctx.finish())
}
