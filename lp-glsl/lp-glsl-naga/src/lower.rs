//! Naga module → LPIR [`lpir::IrModule`] lowering entry point.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{CalleeRef, IrFunction, IrModule, IrType, ModuleBuilder, Op, VReg};
use naga::{Function, Handle, Module};

use crate::NagaModule;
use crate::lower_ctx::LowerCtx;
use crate::lower_error::LowerError;

/// Lower a parsed [`NagaModule`] to LPIR. Function bodies are stubbed (constant return only)
/// until later phases wire in `lower_stmt` / `lower_expr`.
pub fn lower(naga_module: &NagaModule) -> Result<IrModule, LowerError> {
    let import_count = 0u32;
    let mut func_map: BTreeMap<Handle<Function>, CalleeRef> = BTreeMap::new();
    for (i, (handle, _)) in naga_module.functions.iter().enumerate() {
        func_map.insert(*handle, CalleeRef(import_count.saturating_add(i as u32)));
    }
    let import_map = BTreeMap::new();

    let mut mb = ModuleBuilder::new();
    for (handle, info) in &naga_module.functions {
        let func = &naga_module.module.functions[*handle];
        let ir = lower_stub_function(
            &naga_module.module,
            func,
            info.name.as_str(),
            &func_map,
            &import_map,
        )?;
        mb.add_function(ir);
    }
    Ok(mb.finish())
}

fn lower_stub_function(
    module: &Module,
    func: &Function,
    name: &str,
    func_map: &BTreeMap<Handle<Function>, CalleeRef>,
    import_map: &BTreeMap<String, CalleeRef>,
) -> Result<IrFunction, LowerError> {
    let mut ctx = LowerCtx::new(module, func, name, func_map, import_map)?;
    stub_return(&mut ctx)?;
    Ok(ctx.finish())
}

fn stub_return(ctx: &mut LowerCtx<'_>) -> Result<(), LowerError> {
    let mut out: Vec<VReg> = Vec::new();
    for ty in &ctx.return_types {
        let v = ctx.fb.alloc_vreg(*ty);
        match ty {
            IrType::F32 => ctx.fb.push(Op::FconstF32 { dst: v, value: 0.0 }),
            IrType::I32 => ctx.fb.push(Op::IconstI32 { dst: v, value: 0 }),
        }
        out.push(v);
    }
    ctx.fb.push_return(&out);
    Ok(())
}
