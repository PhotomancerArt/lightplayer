//! Naga module → LPIR [`lpir::IrModule`] lowering entry point.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;

use lpir::{CalleeRef, ImportDecl, IrFunction, IrModule, IrType, ModuleBuilder};
use naga::{Function, Handle, Module};

use crate::NagaModule;
use crate::lower_ctx::LowerCtx;
use crate::lower_error::LowerError;
use crate::lower_lpfx;

/// Lower a parsed [`NagaModule`] to LPIR (scalarized vectors and matrices).
///
/// Registers `std.math` and `@lpfx::*` imports as needed, then emits one [`lpir::IrFunction`] per
/// entry in [`NagaModule::functions`]. Fails with [`LowerError`] on unsupported Naga IR outside the
/// scalar subset.
pub fn lower(naga_module: &NagaModule) -> Result<IrModule, LowerError> {
    let mut mb = ModuleBuilder::new();
    let import_map = register_std_math_imports(&mut mb);
    let lpfx_map = lower_lpfx::register_lpfx_imports(&mut mb, naga_module)?;
    let import_count = mb.import_count();

    let mut func_map: BTreeMap<Handle<Function>, CalleeRef> = BTreeMap::new();
    for (i, (handle, _)) in naga_module.functions.iter().enumerate() {
        func_map.insert(*handle, CalleeRef(import_count.saturating_add(i as u32)));
    }

    for (handle, info) in &naga_module.functions {
        let func = &naga_module.module.functions[*handle];
        let ir = lower_function(
            &naga_module.module,
            func,
            info.name.as_str(),
            &func_map,
            &import_map,
            &lpfx_map,
        )?;
        mb.add_function(ir);
    }
    Ok(mb.finish())
}

fn register_std_math_imports(mb: &mut ModuleBuilder) -> BTreeMap<String, CalleeRef> {
    let mut m = BTreeMap::new();
    let mut reg = |name: &str, params: &[IrType], rets: &[IrType]| {
        let r = mb.add_import(ImportDecl {
            module_name: String::from("std.math"),
            func_name: String::from(name),
            param_types: params.to_vec(),
            return_types: rets.to_vec(),
            lpfx_glsl_params: None,
        });
        m.insert(format!("std.math::{name}"), r);
    };
    let f1 = &[IrType::F32];
    let r1 = &[IrType::F32];
    reg("sin", f1, r1);
    reg("cos", f1, r1);
    reg("tan", f1, r1);
    reg("asin", f1, r1);
    reg("acos", f1, r1);
    reg("atan", f1, r1);
    reg("atan2", &[IrType::F32, IrType::F32], r1);
    reg("sinh", f1, r1);
    reg("cosh", f1, r1);
    reg("tanh", f1, r1);
    reg("asinh", f1, r1);
    reg("acosh", f1, r1);
    reg("atanh", f1, r1);
    reg("exp", f1, r1);
    reg("exp2", f1, r1);
    reg("log", f1, r1);
    reg("log2", f1, r1);
    reg("pow", &[IrType::F32, IrType::F32], r1);
    reg("ldexp", &[IrType::F32, IrType::I32], r1);
    reg("sqrt", f1, r1);
    reg("round", f1, r1);
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
