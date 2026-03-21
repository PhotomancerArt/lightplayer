//! Lower `naga::Module` to a WASM binary via `wasm-encoder`.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;

use lp_glsl_builtin_ids::BuiltinId;
use lp_glsl_naga::{FloatMode, NagaModule};
use naga::{
    BinaryOperator, Block, Bytes, Expression, Function, Handle, Literal, MathFunction, Module,
    ScalarKind, Statement, TypeInner, UnaryOperator,
};
use wasm_encoder::{
    BlockType, CodeSection, EntityType, ExportKind, ExportSection, Function as WasmFunction,
    FunctionSection, Ieee32, ImportSection, Instruction, MemoryType, Module as WasmModule,
    TypeSection, ValType,
};

use crate::locals::LocalAlloc;
use crate::options::WasmOptions;
use crate::types::{
    glsl_type_to_wasm_components, type_handle_component_count, type_handle_element_scalar_kind,
};

const Q16_16_SCALE: f32 = 65536.0;

/// Base offset for LPFX `out`-pointer scratch in imported linear memory (below typical stacks).
const LPFX_SCRATCH_BASE: u32 = 65536;

/// WASM structured-control nesting depth (each `block` / `loop` / `if` adds one).
struct EmitCtx {
    depth: u32,
    loop_stack: Vec<LoopFrame>,
    /// Naga function handle → WASM function index (same order as [`emit_module`] exports).
    func_indices: BTreeMap<Handle<Function>, u32>,
    /// Q32 LPFX builtin → WASM function import index (only when [`emit_module`] emitted imports).
    lpfx_import_indices: BTreeMap<BuiltinId, u32>,
    scratch_cursor: Cell<u32>,
}

impl Default for EmitCtx {
    fn default() -> Self {
        Self {
            depth: 0,
            loop_stack: Vec::new(),
            func_indices: BTreeMap::new(),
            lpfx_import_indices: BTreeMap::new(),
            scratch_cursor: Cell::new(LPFX_SCRATCH_BASE),
        }
    }
}

/// Labels for `break` / `continue` relative to [`EmitCtx::depth`].
struct LoopFrame {
    /// Depth after the loop’s outer `block` (first of the three constructs).
    break_target_depth: u32,
    /// Depth after the inner `block` wrapping the loop body (third construct).
    body_entry_depth: u32,
}

impl EmitCtx {
    fn br_depth_for_break(&self) -> Option<u32> {
        let frame = self.loop_stack.last()?;
        self.depth.checked_sub(frame.break_target_depth)
    }

    fn br_depth_for_continue(&self) -> Option<u32> {
        let frame = self.loop_stack.last()?;
        let rel = if self.depth < frame.body_entry_depth {
            0
        } else {
            self.depth - frame.body_entry_depth
        };
        Some(rel)
    }
}

pub fn emit_module(naga_module: &NagaModule, options: &WasmOptions) -> Result<Vec<u8>, String> {
    let module = &naga_module.module;

    let lpfx_ids = crate::lpfx::collect_lpfx_builtin_ids(module, &naga_module.functions);
    if lpfx_ids.is_empty() {
        return emit_module_inner(naga_module, options, None, BTreeMap::new(), 0);
    }

    let bids: Vec<BuiltinId> = lpfx_ids.iter().copied().collect();
    let num_lpfx = bids.len() as u32;
    let mut lpfx_import_indices = BTreeMap::new();
    for (i, bid) in bids.iter().enumerate() {
        lpfx_import_indices.insert(*bid, i as u32);
    }

    emit_module_inner(
        naga_module,
        options,
        Some(bids.as_slice()),
        lpfx_import_indices,
        num_lpfx,
    )
}

fn emit_module_inner(
    naga_module: &NagaModule,
    options: &WasmOptions,
    lpfx_bids: Option<&[BuiltinId]>,
    lpfx_import_indices: BTreeMap<BuiltinId, u32>,
    import_func_count: u32,
) -> Result<Vec<u8>, String> {
    let module = &naga_module.module;
    let mode = options.float_mode;

    let mut types_sec = TypeSection::new();
    let mut import_sec = ImportSection::new();
    let mut func_sec = FunctionSection::new();
    let mut export_sec = ExportSection::new();
    let mut code_sec = CodeSection::new();

    if let Some(bids) = lpfx_bids {
        import_sec.import(
            "env",
            "memory",
            MemoryType {
                minimum: 2,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            },
        );
        for (i, bid) in bids.iter().enumerate() {
            let (params, results) =
                crate::lpfx::q32_lpfx_wasm_signature(*bid).ok_or_else(|| {
                    format!("WASM codegen: LPFX import missing signature for {bid:?}")
                })?;
            types_sec.ty().function(params, results);
            import_sec.import("builtins", bid.name(), EntityType::Function(i as u32));
        }
    }

    let type_base = import_func_count as usize;
    let func_indices: BTreeMap<Handle<Function>, u32> = naga_module
        .functions
        .iter()
        .enumerate()
        .map(|(i, (h, _))| (*h, import_func_count + i as u32))
        .collect();

    for (func_i, (func_handle, fi)) in naga_module.functions.iter().enumerate() {
        let func = &module.functions[*func_handle];

        let name = func.name.as_deref().unwrap_or("_unnamed");
        let params: Vec<ValType> = fi
            .params
            .iter()
            .flat_map(|(_, ty)| glsl_type_to_wasm_components(ty, mode))
            .collect();
        let results: Vec<ValType> = glsl_type_to_wasm_components(&fi.return_type, mode);

        let type_idx = (type_base + func_i) as u32;
        types_sec.ty().function(params.clone(), results.clone());
        func_sec.function(type_idx);
        export_sec.export(name, ExportKind::Func, import_func_count + func_i as u32);

        let alloc = LocalAlloc::new(module, func, mode);
        let locals = alloc.wasm_local_groups();
        let mut wasm_fn = WasmFunction::new(locals);

        emit_local_inits(module, func, &mut wasm_fn, mode, &alloc)?;

        let mut ctx = EmitCtx {
            func_indices: func_indices.clone(),
            lpfx_import_indices: lpfx_import_indices.clone(),
            scratch_cursor: Cell::new(LPFX_SCRATCH_BASE),
            ..Default::default()
        };
        emit_block(
            module,
            func,
            &func.body,
            &mut wasm_fn,
            mode,
            &alloc,
            &mut ctx,
        )?;

        wasm_fn.instruction(&Instruction::End);
        code_sec.function(&wasm_fn);
    }

    let mut out = WasmModule::new();
    out.section(&types_sec);
    if !import_sec.is_empty() {
        out.section(&import_sec);
    }
    out.section(&func_sec);
    out.section(&export_sec);
    out.section(&code_sec);
    Ok(out.finish())
}

/// Naga keeps `LocalVariable::init` instead of always lowering to `Store`; emit those before the body.
fn emit_local_inits(
    module: &Module,
    func: &Function,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    for (handle, lv) in func.local_variables.iter() {
        if alloc.is_parameter_alias(handle) {
            continue;
        }
        let Some(init_h) = lv.init else {
            continue;
        };
        let dim = alloc.local_variable_slots(module, func, handle);
        let actual = crate::emit_vec::expr_component_count(module, func, init_h)?;
        emit_expr(module, func, init_h, wasm_fn, mode, alloc)?;
        if dim == 1 && actual > 1 {
            let base = alloc.alloc_temp_n(actual)?;
            for i in (0..actual).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(base + i));
            }
            wasm_fn.instruction(&Instruction::LocalGet(base));
        } else if dim > 1 && dim != actual {
            return Err(format!(
                "WASM codegen: local init width {actual} vs local slots {dim}"
            ));
        }
        let idx = alloc
            .resolve_local_variable(handle)
            .ok_or_else(|| String::from("WASM codegen: init for unresolved local"))?;
        for off in (0..dim).rev() {
            wasm_fn.instruction(&Instruction::LocalSet(idx + off));
        }
    }
    Ok(())
}

fn emit_block(
    module: &Module,
    func: &Function,
    block: &Block,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
    ctx: &mut EmitCtx,
) -> Result<(), String> {
    for stmt in block.iter() {
        emit_stmt(module, func, stmt, wasm_fn, mode, alloc, ctx)?;
    }
    Ok(())
}

fn emit_stmt(
    module: &Module,
    func: &Function,
    stmt: &Statement,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
    ctx: &mut EmitCtx,
) -> Result<(), String> {
    match stmt {
        Statement::Emit(range) => {
            for h in range.clone() {
                if !expr_may_have_side_effects(module, func, h) {
                    continue;
                }
                let slots = crate::emit_vec::expr_component_count(module, func, h)?;
                emit_expr(module, func, h, wasm_fn, mode, alloc)?;
                for _ in 0..slots {
                    wasm_fn.instruction(&Instruction::Drop);
                }
            }
            Ok(())
        }
        Statement::Block(inner) => emit_block(module, func, inner, wasm_fn, mode, alloc, ctx),
        Statement::If {
            condition,
            accept,
            reject,
        } => {
            emit_expr(module, func, *condition, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::If(BlockType::Empty));
            ctx.depth += 1;
            emit_block(module, func, accept, wasm_fn, mode, alloc, ctx)?;
            wasm_fn.instruction(&Instruction::Else);
            emit_block(module, func, reject, wasm_fn, mode, alloc, ctx)?;
            wasm_fn.instruction(&Instruction::End);
            ctx.depth -= 1;
            Ok(())
        }
        Statement::Loop {
            body,
            continuing,
            break_if,
        } => {
            let break_target_depth = ctx.depth + 1;
            let body_entry_depth = ctx.depth + 3;

            wasm_fn.instruction(&Instruction::Block(BlockType::Empty));
            ctx.depth += 1;
            wasm_fn.instruction(&Instruction::Loop(BlockType::Empty));
            ctx.depth += 1;
            wasm_fn.instruction(&Instruction::Block(BlockType::Empty));
            ctx.depth += 1;

            ctx.loop_stack.push(LoopFrame {
                break_target_depth,
                body_entry_depth,
            });

            let (head, tail) = split_do_while_trailing_guard(body, continuing, break_if);
            emit_stmt_slice(module, func, head, wasm_fn, mode, alloc, ctx)?;
            wasm_fn.instruction(&Instruction::End);
            ctx.depth -= 1;
            if let Some(tail_stmts) = tail {
                emit_stmt_slice(module, func, tail_stmts, wasm_fn, mode, alloc, ctx)?;
            }

            emit_block(module, func, continuing, wasm_fn, mode, alloc, ctx)?;
            if let Some(h) = break_if {
                emit_expr(module, func, *h, wasm_fn, mode, alloc)?;
                wasm_fn.instruction(&Instruction::BrIf(1));
            }
            wasm_fn.instruction(&Instruction::Br(0));
            wasm_fn.instruction(&Instruction::End);
            ctx.depth -= 1;
            wasm_fn.instruction(&Instruction::End);
            ctx.depth -= 1;

            ctx.loop_stack.pop();

            Ok(())
        }
        Statement::Break => {
            let d = ctx
                .br_depth_for_break()
                .ok_or_else(|| String::from("WASM codegen: break outside of any loop"))?;
            wasm_fn.instruction(&Instruction::Br(d));
            Ok(())
        }
        Statement::Continue => {
            let d = ctx
                .br_depth_for_continue()
                .ok_or_else(|| String::from("WASM codegen: continue outside of any loop"))?;
            wasm_fn.instruction(&Instruction::Br(d));
            Ok(())
        }
        Statement::Return { value } => {
            match value {
                Some(h) => {
                    let ret_slots = func
                        .result
                        .as_ref()
                        .map(|r| type_handle_component_count(module, r.ty))
                        .unwrap_or(0);
                    let actual = crate::emit_vec::expr_component_count(module, func, *h)?;
                    emit_expr(module, func, *h, wasm_fn, mode, alloc)?;
                    if ret_slots == 1 && actual > 1 {
                        let base = alloc.alloc_temp_n(actual)?;
                        for i in (0..actual).rev() {
                            wasm_fn.instruction(&Instruction::LocalSet(base + i));
                        }
                        wasm_fn.instruction(&Instruction::LocalGet(base));
                    } else if ret_slots > 1 && ret_slots != actual {
                        return Err(format!(
                            "WASM codegen: return value width {actual} vs expected {ret_slots}"
                        ));
                    }
                }
                None => {}
            }
            wasm_fn.instruction(&Instruction::Return);
            Ok(())
        }
        Statement::Store { pointer, value } => {
            let lv = store_pointer_local(func, *pointer)?;
            let idx = alloc
                .resolve_local_variable(lv)
                .ok_or_else(|| String::from("WASM codegen: store to unresolved local"))?;
            let dim = alloc.local_variable_slots(module, func, lv);
            let actual = crate::emit_vec::expr_component_count(module, func, *value)?;
            emit_expr(module, func, *value, wasm_fn, mode, alloc)?;
            if dim == 1 && actual > 1 {
                let base = alloc.alloc_temp_n(actual)?;
                for i in (0..actual).rev() {
                    wasm_fn.instruction(&Instruction::LocalSet(base + i));
                }
                wasm_fn.instruction(&Instruction::LocalGet(base));
            } else if dim > 1 && dim != actual {
                return Err(format!(
                    "WASM codegen: store value width {actual} vs local slots {dim}"
                ));
            }
            for off in (0..dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(idx + off));
            }
            Ok(())
        }
        Statement::Call {
            function,
            arguments,
            result,
        } => {
            let callee = &module.functions[*function];
            if arguments.len() != callee.arguments.len() {
                return Err(format!(
                    "WASM codegen: call argument count {} vs callee {}",
                    arguments.len(),
                    callee.arguments.len()
                ));
            }
            if matches!(mode, FloatMode::Q32) {
                if let Some(bid) = crate::lpfx::resolve_lpfx_q32_builtin(module, *function) {
                    let import_idx =
                        ctx.lpfx_import_indices.get(&bid).copied().ok_or_else(|| {
                            format!("WASM codegen: LPFX builtin {bid:?} missing from import map")
                        })?;
                    return crate::lpfx::emit_lpfx_import_call(
                        module,
                        func,
                        *function,
                        arguments,
                        *result,
                        wasm_fn,
                        mode,
                        alloc,
                        import_idx,
                        bid,
                        &ctx.scratch_cursor,
                    );
                }
            }
            for (&arg_h, arg_decl) in arguments.iter().zip(callee.arguments.iter()) {
                let expected = type_handle_component_count(module, arg_decl.ty);
                let actual = crate::emit_vec::expr_component_count(module, func, arg_h)?;
                emit_expr(module, func, arg_h, wasm_fn, mode, alloc)?;
                if expected == 1 && actual > 1 {
                    let base = alloc.alloc_temp_n(actual)?;
                    for i in (0..actual).rev() {
                        wasm_fn.instruction(&Instruction::LocalSet(base + i));
                    }
                    wasm_fn.instruction(&Instruction::LocalGet(base));
                } else if expected > 1 && expected != actual {
                    return Err(format!(
                        "WASM codegen: call arg width {actual} vs param slots {expected}"
                    ));
                }
            }
            let wasm_fidx = ctx
                .func_indices
                .get(function)
                .copied()
                .ok_or_else(|| format!("WASM codegen: unresolved call target {function:?}"))?;
            wasm_fn.instruction(&Instruction::Call(wasm_fidx));
            if let Some(res_h) = result {
                let base = alloc.call_result_wasm_base(*res_h).ok_or_else(|| {
                    format!("WASM codegen: missing CallResult locals for {res_h:?}")
                })?;
                let ret_ty = callee
                    .result
                    .as_ref()
                    .ok_or_else(|| String::from("WASM codegen: call with result to void callee"))?
                    .ty;
                let dim = type_handle_component_count(module, ret_ty);
                for off in (0..dim).rev() {
                    wasm_fn.instruction(&Instruction::LocalSet(base + off));
                }
            }
            Ok(())
        }
        _ => Err(format!("WASM codegen: unsupported statement {stmt:?}")),
    }
}

/// Naga `Statement::Emit` may list pure expressions for evaluation visibility only.
/// Emitting them here and dropping stack slots breaks parents that compose the same handles
/// (e.g. chained `&&`); skip when there is no observable side effect.
fn expr_may_have_side_effects(module: &Module, func: &Function, expr: Handle<Expression>) -> bool {
    match &func.expressions[expr] {
        Expression::ImageSample { .. }
        | Expression::ImageLoad { .. }
        | Expression::ImageQuery { .. }
        | Expression::Derivative { .. }
        | Expression::CooperativeLoad { .. }
        | Expression::CooperativeMultiplyAdd { .. }
        | Expression::RayQueryVertexPositions { .. }
        | Expression::RayQueryGetIntersection { .. } => true,
        Expression::Access { base, index } => {
            expr_may_have_side_effects(module, func, *base)
                || expr_may_have_side_effects(module, func, *index)
        }
        Expression::AccessIndex { base, .. }
        | Expression::Splat { value: base, .. }
        | Expression::Swizzle { vector: base, .. }
        | Expression::Unary { expr: base, .. }
        | Expression::Relational { argument: base, .. }
        | Expression::As { expr: base, .. } => expr_may_have_side_effects(module, func, *base),
        Expression::Binary { left, right, .. } => {
            expr_may_have_side_effects(module, func, *left)
                || expr_may_have_side_effects(module, func, *right)
        }
        Expression::Select {
            condition,
            accept,
            reject,
        } => {
            expr_may_have_side_effects(module, func, *condition)
                || expr_may_have_side_effects(module, func, *accept)
                || expr_may_have_side_effects(module, func, *reject)
        }
        Expression::Math {
            arg,
            arg1,
            arg2,
            arg3,
            ..
        } => {
            expr_may_have_side_effects(module, func, *arg)
                || arg1.map_or(false, |h| expr_may_have_side_effects(module, func, h))
                || arg2.map_or(false, |h| expr_may_have_side_effects(module, func, h))
                || arg3.map_or(false, |h| expr_may_have_side_effects(module, func, h))
        }
        Expression::Compose { components, .. } => components
            .iter()
            .any(|&h| expr_may_have_side_effects(module, func, h)),
        Expression::Load { pointer } => expr_may_have_side_effects(module, func, *pointer),
        Expression::ArrayLength(p) => expr_may_have_side_effects(module, func, *p),
        Expression::Literal(_)
        | Expression::Constant(_)
        | Expression::Override(_)
        | Expression::ZeroValue(_)
        | Expression::FunctionArgument(_)
        | Expression::LocalVariable(_)
        | Expression::GlobalVariable(_)
        | Expression::CallResult(_)
        | Expression::AtomicResult { .. }
        | Expression::WorkGroupUniformLoadResult { .. }
        | Expression::RayQueryProceedResult
        | Expression::SubgroupBallotResult
        | Expression::SubgroupOperationResult { .. } => false,
    }
}

fn store_pointer_local(
    func: &Function,
    pointer: Handle<Expression>,
) -> Result<Handle<naga::LocalVariable>, String> {
    match &func.expressions[pointer] {
        Expression::LocalVariable(lv) => Ok(*lv),
        _ => Err(String::from(
            "WASM codegen: store pointer must be a local variable for Phase I",
        )),
    }
}

pub(crate) fn emit_expr(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    if let Expression::AccessIndex { base, .. } = &func.expressions[expr] {
        if crate::emit_vec::expr_component_count(module, func, *base)? > 1 {
            return crate::emit_vec::emit_vector_expr(module, func, expr, wasm_fn, mode, alloc);
        }
    }
    let dim = crate::emit_vec::expr_component_count(module, func, expr)?;
    if dim > 1 {
        return crate::emit_vec::emit_vector_expr(module, func, expr, wasm_fn, mode, alloc);
    }
    match &func.expressions[expr] {
        Expression::Literal(lit) => emit_literal(lit, wasm_fn, mode),
        Expression::Constant(h) => {
            let init = module.constants[*h].init;
            emit_global_expression(module, init, wasm_fn, mode, alloc)
        }
        Expression::FunctionArgument(i) => {
            let base = alloc
                .function_argument_wasm_base(*i)
                .ok_or_else(|| String::from("WASM codegen: bad function argument index"))?;
            wasm_fn.instruction(&Instruction::LocalGet(base));
            Ok(())
        }
        Expression::CallResult(_) => {
            let base = alloc
                .call_result_wasm_base(expr)
                .ok_or_else(|| String::from("WASM codegen: CallResult local missing"))?;
            wasm_fn.instruction(&Instruction::LocalGet(base));
            Ok(())
        }
        Expression::LocalVariable(_) => Err(String::from(
            "WASM codegen: LocalVariable must be used through Load",
        )),
        Expression::Load { pointer } => {
            let ptr = &func.expressions[*pointer];
            match ptr {
                Expression::LocalVariable(lv) => {
                    let idx = alloc
                        .resolve_local_variable(*lv)
                        .ok_or_else(|| String::from("WASM codegen: unresolved local variable"))?;
                    wasm_fn.instruction(&Instruction::LocalGet(idx));
                    Ok(())
                }
                _ => Err(String::from("WASM codegen: load from non-local pointer")),
            }
        }
        Expression::Binary {
            op: BinaryOperator::LogicalAnd,
            left,
            right,
        } => {
            emit_expr(module, func, *left, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::LocalSet(alloc.bool_binary_stash));
            emit_expr(module, func, *right, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::LocalGet(alloc.bool_binary_stash));
            wasm_fn.instruction(&Instruction::I32And);
            Ok(())
        }
        Expression::Binary {
            op: BinaryOperator::LogicalOr,
            left,
            right,
        } => {
            emit_expr(module, func, *left, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::LocalSet(alloc.bool_binary_stash));
            emit_expr(module, func, *right, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::LocalGet(alloc.bool_binary_stash));
            wasm_fn.instruction(&Instruction::I32Or);
            Ok(())
        }
        Expression::Binary { op, left, right } => {
            let k = expr_scalar_kind(module, func, *left)?;
            emit_expr(module, func, *left, wasm_fn, mode, alloc)?;
            emit_expr(module, func, *right, wasm_fn, mode, alloc)?;
            emit_binary(*op, k, mode, wasm_fn, alloc)?;
            Ok(())
        }
        Expression::Unary { op, expr: inner } => {
            let k = expr_scalar_kind(module, func, *inner)?;
            emit_expr(module, func, *inner, wasm_fn, mode, alloc)?;
            emit_unary(*op, k, mode, wasm_fn)?;
            Ok(())
        }
        Expression::Select {
            condition,
            accept,
            reject,
        } => {
            emit_expr(module, func, *accept, wasm_fn, mode, alloc)?;
            emit_expr(module, func, *reject, wasm_fn, mode, alloc)?;
            emit_expr(module, func, *condition, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::Select);
            Ok(())
        }
        Expression::As {
            expr: inner,
            kind,
            convert,
        } => {
            let inner_dim = crate::emit_vec::expr_component_count(module, func, *inner)?;
            if inner_dim > 1 {
                // GLSL scalar cast from `vecN`/`bvecN` uses the first component (`.x`).
                let src_k = crate::emit_vec::vector_element_kind(module, func, *inner)?;
                emit_expr(module, func, *inner, wasm_fn, mode, alloc)?;
                let base = alloc.alloc_temp_n(inner_dim)?;
                for i in (0..inner_dim).rev() {
                    wasm_fn.instruction(&Instruction::LocalSet(base + i));
                }
                wasm_fn.instruction(&Instruction::LocalGet(base));
                emit_cast(*kind, *convert, src_k, mode, wasm_fn, alloc)?;
                return Ok(());
            }
            let src_k = expr_scalar_kind(module, func, *inner)?;
            emit_expr(module, func, *inner, wasm_fn, mode, alloc)?;
            emit_cast(*kind, *convert, src_k, mode, wasm_fn, alloc)?;
            Ok(())
        }
        Expression::ZeroValue(ty_h) => {
            let inner = &module.types[*ty_h].inner;
            emit_zero_value(inner, wasm_fn, mode)
        }
        Expression::Math {
            fun: MathFunction::Mix,
            arg,
            arg1: Some(y),
            arg2: Some(t),
            ..
        } => {
            let k = expr_scalar_kind(module, func, *arg)?;
            if k != ScalarKind::Float {
                return Err(String::from("WASM codegen: mix expects float"));
            }
            emit_scalar_mix(module, func, *arg, *y, *t, wasm_fn, mode, alloc)
        }
        Expression::Math {
            fun: MathFunction::SmoothStep,
            arg: edge0,
            arg1: Some(edge1),
            arg2: Some(x),
            ..
        } => {
            let k = expr_scalar_kind(module, func, *edge0)?;
            if k != ScalarKind::Float {
                return Err(String::from("WASM codegen: smoothstep expects float"));
            }
            emit_scalar_smoothstep(module, func, *edge0, *edge1, *x, wasm_fn, mode, alloc)
        }
        Expression::Math {
            fun: MathFunction::Step,
            arg: edge,
            arg1: Some(x),
            ..
        } => {
            let k = expr_scalar_kind(module, func, *edge)?;
            if k != ScalarKind::Float {
                return Err(String::from("WASM codegen: step expects float"));
            }
            emit_scalar_step(module, func, *edge, *x, wasm_fn, mode, alloc)
        }
        Expression::Math {
            fun: MathFunction::Round,
            arg,
            ..
        } => emit_scalar_round(module, func, *arg, wasm_fn, mode, alloc),
        Expression::Math {
            fun: MathFunction::Abs,
            arg,
            ..
        } => emit_scalar_abs(module, func, *arg, wasm_fn, mode, alloc),
        Expression::Math { fun, arg, arg1, .. } => match (fun, arg1.as_ref()) {
            (MathFunction::Min | MathFunction::Max, Some(rhs)) => {
                let is_max = matches!(fun, MathFunction::Max);
                let k = expr_scalar_kind(module, func, *arg)?;
                emit_expr(module, func, *arg, wasm_fn, mode, alloc)?;
                emit_expr(module, func, *rhs, wasm_fn, mode, alloc)?;
                emit_scalar_min_max(is_max, k, mode, wasm_fn, alloc)?;
                Ok(())
            }
            _ => Err(format!(
                "WASM codegen: unsupported expression {:?}",
                func.expressions[expr]
            )),
        },
        _ => Err(format!(
            "WASM codegen: unsupported expression {:?}",
            func.expressions[expr]
        )),
    }
}

fn emit_zero_value(
    inner: &TypeInner,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
) -> Result<(), String> {
    emit_zero_value_inner(inner, wasm_fn, mode)
}

pub(crate) fn emit_zero_value_inner(
    inner: &TypeInner,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
) -> Result<(), String> {
    match *inner {
        TypeInner::Scalar(s) => match s.kind {
            ScalarKind::Float if s.width == 4 => match mode {
                FloatMode::Float => {
                    wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(0.0f32)));
                }
                FloatMode::Q32 => {
                    wasm_fn.instruction(&Instruction::I32Const(0));
                }
            },
            ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool if s.width == 4 => {
                wasm_fn.instruction(&Instruction::I32Const(0));
            }
            _ => {
                return Err(format!("WASM codegen: ZeroValue unsupported scalar {s:?}"));
            }
        },
        _ => {
            return Err(String::from(
                "WASM codegen: ZeroValue inner must be scalar here",
            ));
        }
    }
    Ok(())
}

pub(crate) fn emit_global_expression(
    module: &Module,
    expr: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    match &module.global_expressions[expr] {
        Expression::Literal(lit) => emit_literal(lit, wasm_fn, mode),
        Expression::Compose { components, .. } => {
            for &c in components {
                emit_global_expression(module, c, wasm_fn, mode, alloc)?;
            }
            Ok(())
        }
        Expression::Splat { size, value } => {
            let dim = *size as u32;
            emit_global_expression(module, *value, wasm_fn, mode, alloc)?;
            if dim <= 1 {
                return Ok(());
            }
            let s = alloc.splat_scratch;
            wasm_fn.instruction(&Instruction::LocalSet(s));
            for _ in 0..dim {
                wasm_fn.instruction(&Instruction::LocalGet(s));
            }
            Ok(())
        }
        _ => Err(format!(
            "WASM codegen: unsupported global expression {:?}",
            module.global_expressions[expr]
        )),
    }
}

/// Fixed16x16 values are clamped to approximately ±32768 (see filetests).
fn clamp_f32_to_q16_16_range(v: f32) -> f32 {
    const LO: f32 = -32768.0;
    const HI: f32 = 32767.9999847412109375;
    v.clamp(LO, HI)
}

pub(crate) fn emit_literal(
    lit: &Literal,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
) -> Result<(), String> {
    match *lit {
        Literal::F32(v) => match mode {
            FloatMode::Float => {
                wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(v)));
                Ok(())
            }
            FloatMode::Q32 => {
                let v = clamp_f32_to_q16_16_range(v);
                let q = (f64::from(v) * f64::from(Q16_16_SCALE)) as i32;
                wasm_fn.instruction(&Instruction::I32Const(q));
                Ok(())
            }
        },
        Literal::I32(v) => {
            wasm_fn.instruction(&Instruction::I32Const(v));
            Ok(())
        }
        Literal::U32(v) => {
            wasm_fn.instruction(&Instruction::I32Const(v as i32));
            Ok(())
        }
        Literal::Bool(b) => {
            wasm_fn.instruction(&Instruction::I32Const(b as i32));
            Ok(())
        }
        _ => Err(format!("WASM codegen: unsupported literal {lit:?}")),
    }
}

pub(crate) fn expr_scalar_kind(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
) -> Result<ScalarKind, String> {
    match &func.expressions[expr] {
        Expression::Constant(h) => type_handle_element_scalar_kind(module, module.constants[*h].ty)
            .map_err(|e| String::from(e)),
        Expression::Literal(l) => match l {
            Literal::F32(_) | Literal::F64(_) | Literal::F16(_) | Literal::AbstractFloat(_) => {
                Ok(ScalarKind::Float)
            }
            Literal::I32(_) | Literal::I64(_) | Literal::AbstractInt(_) => Ok(ScalarKind::Sint),
            Literal::U32(_) | Literal::U64(_) => Ok(ScalarKind::Uint),
            Literal::Bool(_) => Ok(ScalarKind::Bool),
        },
        Expression::FunctionArgument(i) => {
            let arg = func
                .arguments
                .get(*i as usize)
                .ok_or_else(|| String::from("bad argument index"))?;
            type_handle_element_scalar_kind(module, arg.ty).map_err(|e| String::from(e))
        }
        Expression::LocalVariable(lv) => {
            let lv_ty = func.local_variables[*lv].ty;
            type_handle_element_scalar_kind(module, lv_ty).map_err(|e| String::from(e))
        }
        Expression::Load { pointer } => {
            let ptr = &func.expressions[*pointer];
            match ptr {
                Expression::LocalVariable(lv) => {
                    let lv_ty = func.local_variables[*lv].ty;
                    type_handle_element_scalar_kind(module, lv_ty).map_err(|e| String::from(e))
                }
                _ => expr_scalar_kind(module, func, *pointer),
            }
        }
        Expression::Binary { op, left, .. } => match op {
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
            | BinaryOperator::LogicalAnd
            | BinaryOperator::LogicalOr => Ok(ScalarKind::Bool),
            _ => expr_scalar_kind(module, func, *left),
        },
        Expression::Unary { expr: inner, .. } => expr_scalar_kind(module, func, *inner),
        Expression::Select { accept, .. } => expr_scalar_kind(module, func, *accept),
        Expression::As { kind, .. } => Ok(*kind),
        Expression::AccessIndex { base, .. } => {
            crate::emit_vec::vector_element_kind(module, func, *base)
        }
        Expression::CallResult(fh) => {
            let ret = module.functions[*fh]
                .result
                .as_ref()
                .ok_or_else(|| String::from("CallResult for void function"))?;
            type_handle_element_scalar_kind(module, ret.ty).map_err(|e| String::from(e))
        }
        Expression::Math {
            fun:
                MathFunction::Min
                | MathFunction::Max
                | MathFunction::Mix
                | MathFunction::SmoothStep
                | MathFunction::Step
                | MathFunction::Round
                | MathFunction::Abs,
            arg,
            ..
        } => crate::emit_vec::vector_element_kind(module, func, *arg),
        Expression::ZeroValue(ty_h) => match &module.types[*ty_h].inner {
            TypeInner::Scalar(s) => Ok(s.kind),
            _ => Err(String::from("zerovalue kind")),
        },
        _ => Err(format!(
            "cannot infer scalar kind for {:?}",
            func.expressions[expr]
        )),
    }
}

pub(crate) fn emit_binary(
    op: BinaryOperator,
    kind: ScalarKind,
    mode: FloatMode,
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    match (op, kind, mode) {
        (BinaryOperator::Add, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Add);
        }
        (BinaryOperator::Add, ScalarKind::Float, FloatMode::Q32) => {
            emit_q32_add_sat(wasm_fn, alloc)?;
        }
        (BinaryOperator::Subtract, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Sub);
        }
        (BinaryOperator::Subtract, ScalarKind::Float, FloatMode::Q32) => {
            emit_q32_sub_sat(wasm_fn, alloc)?;
        }
        (BinaryOperator::Multiply, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Mul);
        }
        (BinaryOperator::Multiply, ScalarKind::Float, FloatMode::Q32) => {
            emit_q32_mul(wasm_fn, alloc)?;
        }
        (BinaryOperator::Divide, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Div);
        }
        (BinaryOperator::Divide, ScalarKind::Float, FloatMode::Q32) => {
            emit_q32_div(wasm_fn, alloc)?;
        }
        (BinaryOperator::Modulo, ScalarKind::Float, FloatMode::Float) => {
            emit_f32_mod(wasm_fn, alloc)?;
        }
        (BinaryOperator::Modulo, ScalarKind::Float, FloatMode::Q32) => {
            return Err(String::from(
                "WASM codegen: float modulo in Q32 not implemented",
            ));
        }

        (BinaryOperator::Add, ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Add);
        }
        (BinaryOperator::Subtract, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Sub);
        }
        (BinaryOperator::Multiply, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Mul);
        }
        (BinaryOperator::Divide, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32DivS);
        }
        (BinaryOperator::Divide, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32DivU);
        }
        (BinaryOperator::Modulo, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32RemS);
        }
        (BinaryOperator::Modulo, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32RemU);
        }

        (BinaryOperator::Equal, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Eq);
        }
        (BinaryOperator::Equal, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32Eq);
        }
        (BinaryOperator::NotEqual, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Ne);
        }
        (BinaryOperator::NotEqual, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32Ne);
        }
        (BinaryOperator::Less, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Lt);
        }
        (BinaryOperator::Less, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32LtS);
        }
        (BinaryOperator::LessEqual, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Le);
        }
        (BinaryOperator::LessEqual, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32LeS);
        }
        (BinaryOperator::Greater, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Gt);
        }
        (BinaryOperator::Greater, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32GtS);
        }
        (BinaryOperator::GreaterEqual, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Ge);
        }
        (BinaryOperator::GreaterEqual, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32GeS);
        }

        (BinaryOperator::Equal, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32Eq);
        }
        (BinaryOperator::NotEqual, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32Ne);
        }
        (BinaryOperator::Less, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32LtS);
        }
        (BinaryOperator::LessEqual, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32LeS);
        }
        (BinaryOperator::Greater, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32GtS);
        }
        (BinaryOperator::GreaterEqual, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32GeS);
        }

        (BinaryOperator::Equal, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Eq);
        }
        (BinaryOperator::NotEqual, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Ne);
        }
        (BinaryOperator::Less, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32LtU);
        }
        (BinaryOperator::LessEqual, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32LeU);
        }
        (BinaryOperator::Greater, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32GtU);
        }
        (BinaryOperator::GreaterEqual, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32GeU);
        }

        (BinaryOperator::Equal, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Eq);
        }
        (BinaryOperator::NotEqual, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Ne);
        }

        (BinaryOperator::LogicalAnd, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32And);
        }
        (BinaryOperator::LogicalOr, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Or);
        }

        (BinaryOperator::And, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32And);
        }
        (BinaryOperator::InclusiveOr, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Or);
        }
        (BinaryOperator::ExclusiveOr, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Xor);
        }

        (BinaryOperator::ShiftLeft, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Shl);
        }
        (BinaryOperator::ShiftRight, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32ShrS);
        }
        (BinaryOperator::ShiftRight, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32ShrU);
        }

        _ => {
            return Err(format!(
                "WASM codegen: unsupported binary {op:?} for {kind:?} / {mode:?}"
            ));
        }
    }
    Ok(())
}

pub(crate) fn emit_scalar_min_max(
    is_max: bool,
    kind: ScalarKind,
    mode: FloatMode,
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    match (kind, mode) {
        (ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(if is_max {
                &Instruction::F32Max
            } else {
                &Instruction::F32Min
            });
            Ok(())
        }
        (ScalarKind::Float, FloatMode::Q32) => {
            emit_i32_min_max_select(is_max, false, wasm_fn, alloc)
        }
        (ScalarKind::Sint | ScalarKind::Bool, FloatMode::Q32) => {
            emit_i32_min_max_select(is_max, false, wasm_fn, alloc)
        }
        (ScalarKind::Uint, FloatMode::Q32) => emit_i32_min_max_select(is_max, true, wasm_fn, alloc),
        (ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool, FloatMode::Float) => Err(
            String::from("WASM codegen: integer min/max in Float mode not supported"),
        ),
        _ => Err(format!(
            "WASM codegen: min/max unsupported for {kind:?} / {mode:?}"
        )),
    }
}

fn emit_i32_min_max_select(
    is_max: bool,
    unsigned: bool,
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let (s0, s1) = alloc
        .q32_scratch
        .ok_or_else(|| String::from("Q32 scratch missing for i32 min/max"))?;
    let cond_local = alloc.splat_scratch;
    wasm_fn.instruction(&Instruction::LocalSet(s1));
    wasm_fn.instruction(&Instruction::LocalSet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    match (is_max, unsigned) {
        (false, false) => wasm_fn.instruction(&Instruction::I32LtS),
        (false, true) => wasm_fn.instruction(&Instruction::I32LtU),
        (true, false) => wasm_fn.instruction(&Instruction::I32GtS),
        (true, true) => wasm_fn.instruction(&Instruction::I32GtU),
    };
    wasm_fn.instruction(&Instruction::LocalSet(cond_local));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::LocalGet(cond_local));
    wasm_fn.instruction(&Instruction::Select);
    Ok(())
}

pub(crate) fn emit_unary(
    op: UnaryOperator,
    kind: ScalarKind,
    mode: FloatMode,
    wasm_fn: &mut WasmFunction,
) -> Result<(), String> {
    match (op, kind, mode) {
        (UnaryOperator::Negate, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Neg);
        }
        (UnaryOperator::Negate, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32Const(0));
            wasm_fn.instruction(&Instruction::I32Sub);
        }
        (UnaryOperator::Negate, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32Const(0));
            wasm_fn.instruction(&Instruction::I32Sub);
        }
        (UnaryOperator::LogicalNot, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Eqz);
        }
        (UnaryOperator::LogicalNot, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Eqz);
        }
        (UnaryOperator::BitwiseNot, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Const(-1));
            wasm_fn.instruction(&Instruction::I32Xor);
        }
        _ => {
            return Err(format!(
                "WASM codegen: unsupported unary {op:?} for {kind:?}"
            ));
        }
    }
    Ok(())
}

/// Fixed-point Q16.16 `round` (half-way away from zero), matching `__lp_q32_round`.
fn emit_q32_round_inner(wasm_fn: &mut WasmFunction, s0: u32, s1: u32) -> Result<(), String> {
    wasm_fn.instruction(&Instruction::LocalSet(s0));

    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I32Eqz);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(0));
    wasm_fn.instruction(&Instruction::Else);

    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I32Const(0));
    wasm_fn.instruction(&Instruction::I32GtS);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I32Const(0x8000));
    wasm_fn.instruction(&Instruction::I32Add);
    wasm_fn.instruction(&Instruction::I32Const(16));
    wasm_fn.instruction(&Instruction::I32ShrS);
    wasm_fn.instruction(&Instruction::I32Const(16));
    wasm_fn.instruction(&Instruction::I32Shl);
    wasm_fn.instruction(&Instruction::Else);

    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I32Const(0x8000));
    wasm_fn.instruction(&Instruction::I32Add);
    wasm_fn.instruction(&Instruction::I32Const(16));
    wasm_fn.instruction(&Instruction::I32ShrS);
    wasm_fn.instruction(&Instruction::I32Const(16));
    wasm_fn.instruction(&Instruction::I32Shl);
    wasm_fn.instruction(&Instruction::LocalSet(s1));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I32Const(0xFFFF));
    wasm_fn.instruction(&Instruction::I32And);
    wasm_fn.instruction(&Instruction::I32Const(0x8000));
    wasm_fn.instruction(&Instruction::I32Eq);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::I32Const(0x1_0000));
    wasm_fn.instruction(&Instruction::I32Sub);
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::End);

    wasm_fn.instruction(&Instruction::End);

    wasm_fn.instruction(&Instruction::End);
    Ok(())
}

fn emit_q32_round(wasm_fn: &mut WasmFunction, alloc: &LocalAlloc) -> Result<(), String> {
    let (s0, s1) = alloc
        .round_i32_scratch()
        .ok_or_else(|| String::from("WASM codegen: missing i32 scratch for round"))?;
    emit_q32_round_inner(wasm_fn, s0, s1)
}

pub(crate) fn emit_round_top_of_stack(
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    match mode {
        FloatMode::Q32 => emit_q32_round(wasm_fn, alloc),
        FloatMode::Float => emit_float_round_via_q32(wasm_fn, alloc),
    }
}

fn emit_float_round_via_q32(wasm_fn: &mut WasmFunction, alloc: &LocalAlloc) -> Result<(), String> {
    const LO: f32 = -32768.0;
    const HI: f32 = 32767.9999847412109375;
    let (f0, _) = alloc
        .float_scratch
        .ok_or_else(|| String::from("WASM codegen: missing float scratch for round"))?;
    wasm_fn.instruction(&Instruction::LocalSet(f0));
    wasm_fn.instruction(&Instruction::LocalGet(f0));
    wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(LO)));
    wasm_fn.instruction(&Instruction::F32Max);
    wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(HI)));
    wasm_fn.instruction(&Instruction::F32Min);
    wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(Q16_16_SCALE)));
    wasm_fn.instruction(&Instruction::F32Mul);
    wasm_fn.instruction(&Instruction::I32TruncSatF32S);
    emit_q32_round(wasm_fn, alloc)?;
    wasm_fn.instruction(&Instruction::F32ConvertI32S);
    wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(Q16_16_SCALE)));
    wasm_fn.instruction(&Instruction::F32Div);
    Ok(())
}

fn emit_scalar_abs(
    module: &Module,
    func: &Function,
    arg: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let k = expr_scalar_kind(module, func, arg)?;
    emit_expr(module, func, arg, wasm_fn, mode, alloc)?;
    match k {
        ScalarKind::Float => emit_abs_top_of_stack(wasm_fn, mode, alloc),
        ScalarKind::Sint | ScalarKind::Uint => emit_i32_abs_top(wasm_fn, alloc),
        _ => Err(format!(
            "WASM codegen: abs unsupported for scalar {k:?} in {mode:?}"
        )),
    }
}

fn emit_i32_abs_top(wasm_fn: &mut WasmFunction, alloc: &LocalAlloc) -> Result<(), String> {
    let t = alloc.alloc_temp_n(1)?;
    wasm_fn.instruction(&Instruction::LocalTee(t));
    wasm_fn.instruction(&Instruction::I32Const(0));
    wasm_fn.instruction(&Instruction::LocalGet(t));
    wasm_fn.instruction(&Instruction::I32LtS);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(0));
    wasm_fn.instruction(&Instruction::LocalGet(t));
    wasm_fn.instruction(&Instruction::I32Sub);
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::LocalGet(t));
    wasm_fn.instruction(&Instruction::End);
    Ok(())
}

pub(crate) fn emit_abs_top_of_stack(
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Abs);
        }
        FloatMode::Q32 => {
            emit_i32_abs_top(wasm_fn, alloc)?;
        }
    }
    Ok(())
}

fn emit_scalar_round(
    module: &Module,
    func: &Function,
    arg: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let k = expr_scalar_kind(module, func, arg)?;
    if k != ScalarKind::Float {
        return Err(String::from("WASM codegen: round expects float"));
    }
    emit_expr(module, func, arg, wasm_fn, mode, alloc)?;
    emit_round_top_of_stack(wasm_fn, mode, alloc)
}

/// Map i32 bool (0 / non-zero) to Q16.16 `0.0` / `1.0`.
fn emit_i32_bool_to_q32_float(
    wasm_fn: &mut WasmFunction,
    _alloc: &LocalAlloc,
) -> Result<(), String> {
    wasm_fn.instruction(&Instruction::I32Eqz);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(0));
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::I32Const(65_536));
    wasm_fn.instruction(&Instruction::End);
    Ok(())
}

pub(crate) fn emit_cast(
    dst_kind: ScalarKind,
    convert: Option<Bytes>,
    src_kind: ScalarKind,
    mode: FloatMode,
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    if convert.is_none() {
        match (src_kind, dst_kind, mode) {
            (ScalarKind::Bool, ScalarKind::Float, FloatMode::Float) => {
                wasm_fn.instruction(&Instruction::F32ConvertI32S);
                return Ok(());
            }
            (ScalarKind::Bool, ScalarKind::Float, FloatMode::Q32) => {
                return emit_i32_bool_to_q32_float(wasm_fn, alloc);
            }
            _ => {}
        }
        if matches!(
            (src_kind, dst_kind),
            (ScalarKind::Float, ScalarKind::Float)
                | (ScalarKind::Sint, ScalarKind::Sint)
                | (ScalarKind::Uint, ScalarKind::Uint)
                | (ScalarKind::Bool, ScalarKind::Bool)
        ) {
            return Ok(());
        }
    }
    match (src_kind, dst_kind, mode) {
        (ScalarKind::Float, ScalarKind::Float, _)
        | (ScalarKind::Sint, ScalarKind::Sint, _)
        | (ScalarKind::Uint, ScalarKind::Uint, _)
        | (ScalarKind::Bool, ScalarKind::Bool, _) => {}
        (ScalarKind::Float, ScalarKind::Sint, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::I32TruncF32S);
        }
        (ScalarKind::Float, ScalarKind::Uint, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::I32TruncF32U);
        }
        (ScalarKind::Sint, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32ConvertI32S);
        }
        (ScalarKind::Uint, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32ConvertI32U);
        }

        (ScalarKind::Float, ScalarKind::Sint, FloatMode::Q32) => {
            emit_q32_fixed_trunc_toward_zero_to_i32(wasm_fn);
            let scratch = alloc
                .q32_scratch
                .map(|(a, _)| a)
                .ok_or_else(|| String::from("Q32 scratch missing"))?;
            emit_clamp_i32_to_q16_int_range(wasm_fn, scratch);
        }
        (ScalarKind::Float, ScalarKind::Uint, FloatMode::Q32) => {
            emit_q32_fixed_trunc_toward_zero_to_i32(wasm_fn);
            // Match naga/WGSL: negative floats convert to 0u (not signed-trunc then bitcast).
            let scratch = alloc
                .q32_scratch
                .map(|(a, _)| a)
                .ok_or_else(|| String::from("Q32 scratch missing"))?;
            wasm_fn.instruction(&Instruction::LocalSet(scratch));
            wasm_fn.instruction(&Instruction::I32Const(0));
            wasm_fn.instruction(&Instruction::LocalGet(scratch));
            wasm_fn.instruction(&Instruction::LocalGet(scratch));
            wasm_fn.instruction(&Instruction::I32Const(0));
            wasm_fn.instruction(&Instruction::I32LtS);
            wasm_fn.instruction(&Instruction::Select);
        }
        (ScalarKind::Sint, ScalarKind::Float, FloatMode::Q32) => {
            emit_i32_clamp_then_q32_scale_sint(wasm_fn, alloc)?;
        }
        (ScalarKind::Uint, ScalarKind::Float, FloatMode::Q32) => {
            emit_u32_clamp_then_q32_scale_uint(wasm_fn, alloc)?;
        }

        (ScalarKind::Sint, ScalarKind::Uint, _) | (ScalarKind::Uint, ScalarKind::Sint, _) => {}

        (ScalarKind::Bool, ScalarKind::Sint | ScalarKind::Uint, _) => {}

        (ScalarKind::Bool, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32ConvertI32S);
        }
        (ScalarKind::Bool, ScalarKind::Float, FloatMode::Q32) => {
            emit_i32_bool_to_q32_float(wasm_fn, alloc)?;
        }

        (ScalarKind::Sint | ScalarKind::Uint, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Const(0));
            wasm_fn.instruction(&Instruction::I32Ne);
        }

        (ScalarKind::Float, ScalarKind::Bool, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(0.0f32)));
            wasm_fn.instruction(&Instruction::F32Ne);
        }
        (ScalarKind::Float, ScalarKind::Bool, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32Const(0));
            wasm_fn.instruction(&Instruction::I32Ne);
        }

        _ => {
            return Err(format!(
                "WASM codegen: unsupported cast {src_kind:?} -> {dst_kind:?} ({mode:?})"
            ));
        }
    }
    Ok(())
}

/// `if (!cond) { break; }` that the GLSL front lowers as the trailing check on `do {} while`.
fn stmt_is_trailing_loop_guard(stmt: &Statement) -> bool {
    let Statement::If { accept, reject, .. } = stmt else {
        return false;
    };
    reject.is_empty() && accept.len() == 1 && matches!(accept.first(), Some(Statement::Break))
}

/// Split Naga `Loop::body` so `continue` can target the end of the user `do {}` block without
/// skipping the trailing `if (!cond) break` (see `naga::front::glsl` do-while lowering).
fn split_do_while_trailing_guard<'a>(
    body: &'a Block,
    continuing: &'a Block,
    break_if: &'a Option<Handle<Expression>>,
) -> (&'a [Statement], Option<&'a [Statement]>) {
    let stmts: &'a [Statement] = body;
    if !continuing.is_empty() || break_if.is_some() {
        return (stmts, None);
    }
    if stmts.len() < 2 {
        return (stmts, None);
    }
    let last = match stmts.last() {
        Some(s) => s,
        None => return (stmts, None),
    };
    if !stmt_is_trailing_loop_guard(last) {
        return (stmts, None);
    }
    let n = stmts.len() - 1;
    (&stmts[..n], Some(&stmts[n..]))
}

fn emit_stmt_slice(
    module: &Module,
    func: &Function,
    stmts: &[Statement],
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
    ctx: &mut EmitCtx,
) -> Result<(), String> {
    for stmt in stmts {
        emit_stmt(module, func, stmt, wasm_fn, mode, alloc, ctx)?;
    }
    Ok(())
}

/// Truncate fixed-point Q32.16 (`i32`) toward zero to the integer unit (divide by 65536).
fn emit_q32_fixed_trunc_toward_zero_to_i32(wasm_fn: &mut WasmFunction) {
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64Const(65_536));
    wasm_fn.instruction(&Instruction::I64DivS);
    wasm_fn.instruction(&Instruction::I32WrapI64);
}

/// Clamp signed integer to `[-32768, 32767]` (representable Q16.16 **integer** units).
fn emit_clamp_i32_to_q16_int_range(wasm_fn: &mut WasmFunction, scratch: u32) {
    wasm_fn.instruction(&Instruction::LocalSet(scratch));
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::I32Const(32_767));
    wasm_fn.instruction(&Instruction::I32GtS);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(32_767));
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::End);
    wasm_fn.instruction(&Instruction::LocalSet(scratch));
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::I32Const(-32_768));
    wasm_fn.instruction(&Instruction::I32LtS);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(-32_768));
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::End);
}

/// Clamp signed i32 to Q16.16 **integer** range then scale to fixed (`× 65536`).
fn emit_i32_clamp_then_q32_scale_sint(
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let scratch = alloc
        .q32_scratch
        .map(|(a, _)| a)
        .ok_or_else(|| String::from("Q32 scratch missing"))?;
    emit_clamp_i32_to_q16_int_range(wasm_fn, scratch);
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64Const(16));
    wasm_fn.instruction(&Instruction::I64Shl);
    wasm_fn.instruction(&Instruction::I32WrapI64);
    Ok(())
}

/// Clamp unsigned i32 to `0..=32767` then scale to fixed.
fn emit_u32_clamp_then_q32_scale_uint(
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let scratch = alloc
        .q32_scratch
        .map(|(a, _)| a)
        .ok_or_else(|| String::from("Q32 scratch missing"))?;
    wasm_fn.instruction(&Instruction::LocalSet(scratch));
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::I32Const(32_767));
    wasm_fn.instruction(&Instruction::I32GtU);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(32_767));
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::End);
    wasm_fn.instruction(&Instruction::LocalSet(scratch));
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::I64ExtendI32U);
    wasm_fn.instruction(&Instruction::I64Const(16));
    wasm_fn.instruction(&Instruction::I64Shl);
    wasm_fn.instruction(&Instruction::I32WrapI64);
    Ok(())
}

/// Match `__lp_q32_add` / `__lp_q32_sub`: widen to i64, op, saturate to fixed-point range.
pub(crate) fn emit_q32_add_sat(
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let (s0, s1) = alloc
        .q32_scratch
        .ok_or_else(|| String::from("Q32 scratch missing"))?;
    let acc = alloc
        .q32_i64_sat
        .ok_or_else(|| String::from("Q32 i64 sat local missing"))?;
    wasm_fn.instruction(&Instruction::LocalSet(s1));
    wasm_fn.instruction(&Instruction::LocalSet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64Add);
    wasm_fn.instruction(&Instruction::LocalSet(acc));
    emit_q32_sat_wide_i64_local(wasm_fn, acc)
}

pub(crate) fn emit_q32_sub_sat(
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let (s0, s1) = alloc
        .q32_scratch
        .ok_or_else(|| String::from("Q32 scratch missing"))?;
    let acc = alloc
        .q32_i64_sat
        .ok_or_else(|| String::from("Q32 i64 sat local missing"))?;
    wasm_fn.instruction(&Instruction::LocalSet(s1));
    wasm_fn.instruction(&Instruction::LocalSet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64Sub);
    wasm_fn.instruction(&Instruction::LocalSet(acc));
    emit_q32_sat_wide_i64_local(wasm_fn, acc)
}

/// Saturate i64 in local `acc` to Q32 fixed range (same bounds as `__lp_q32_mul`), leave i32 on stack.
fn emit_q32_sat_wide_i64_local(wasm_fn: &mut WasmFunction, acc: u32) -> Result<(), String> {
    const MAX_FIXED: i64 = 0x7FFF_FFFF;
    wasm_fn.instruction(&Instruction::LocalGet(acc));
    wasm_fn.instruction(&Instruction::I64Const(MAX_FIXED));
    wasm_fn.instruction(&Instruction::I64GtS);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(0x7FFF_FFFF));
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::LocalGet(acc));
    wasm_fn.instruction(&Instruction::I64Const(i32::MIN as i64));
    wasm_fn.instruction(&Instruction::I64LtS);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(i32::MIN));
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::LocalGet(acc));
    wasm_fn.instruction(&Instruction::I32WrapI64);
    wasm_fn.instruction(&Instruction::End);
    wasm_fn.instruction(&Instruction::End);
    Ok(())
}

pub(crate) fn emit_q32_mul(wasm_fn: &mut WasmFunction, alloc: &LocalAlloc) -> Result<(), String> {
    let (s0, s1) = alloc
        .q32_scratch
        .ok_or_else(|| String::from("Q32 scratch missing"))?;
    let acc = alloc
        .q32_i64_sat
        .ok_or_else(|| String::from("Q32 i64 sat local missing"))?;
    wasm_fn.instruction(&Instruction::LocalSet(s1));
    wasm_fn.instruction(&Instruction::LocalSet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64Mul);
    wasm_fn.instruction(&Instruction::I64Const(16));
    wasm_fn.instruction(&Instruction::I64ShrS);
    wasm_fn.instruction(&Instruction::LocalSet(acc));
    emit_q32_sat_wide_i64_local(wasm_fn, acc)
}

fn emit_q32_div(wasm_fn: &mut WasmFunction, alloc: &LocalAlloc) -> Result<(), String> {
    let (s0, s1) = alloc
        .q32_scratch
        .ok_or_else(|| String::from("Q32 scratch missing"))?;
    wasm_fn.instruction(&Instruction::LocalSet(s1));
    wasm_fn.instruction(&Instruction::LocalSet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64Const(16));
    wasm_fn.instruction(&Instruction::I64Shl);
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64DivS);
    wasm_fn.instruction(&Instruction::I32WrapI64);
    Ok(())
}

/// `a - trunc(a/b)*b` using two f32 scratch locals.
fn emit_scalar_mix(
    module: &Module,
    func: &Function,
    x: Handle<Expression>,
    y: Handle<Expression>,
    t: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    emit_expr(module, func, x, wasm_fn, mode, alloc)?;
    emit_expr(module, func, y, wasm_fn, mode, alloc)?;
    emit_expr(module, func, t, wasm_fn, mode, alloc)?;
    let base = alloc.alloc_temp_n(3)?;
    wasm_fn.instruction(&Instruction::LocalSet(base + 2));
    wasm_fn.instruction(&Instruction::LocalSet(base + 1));
    wasm_fn.instruction(&Instruction::LocalSet(base));
    wasm_fn.instruction(&Instruction::LocalGet(base + 1));
    wasm_fn.instruction(&Instruction::LocalGet(base));
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Sub);
            wasm_fn.instruction(&Instruction::LocalGet(base + 2));
            wasm_fn.instruction(&Instruction::F32Mul);
            wasm_fn.instruction(&Instruction::LocalGet(base));
            wasm_fn.instruction(&Instruction::F32Add);
        }
        FloatMode::Q32 => {
            emit_q32_sub_sat(wasm_fn, alloc)?;
            wasm_fn.instruction(&Instruction::LocalGet(base + 2));
            emit_q32_mul(wasm_fn, alloc)?;
            wasm_fn.instruction(&Instruction::LocalGet(base));
            emit_q32_add_sat(wasm_fn, alloc)?;
        }
    }
    Ok(())
}

fn emit_scalar_smoothstep(
    module: &Module,
    func: &Function,
    edge0: Handle<Expression>,
    edge1: Handle<Expression>,
    x: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    emit_expr(module, func, edge0, wasm_fn, mode, alloc)?;
    emit_expr(module, func, edge1, wasm_fn, mode, alloc)?;
    emit_expr(module, func, x, wasm_fn, mode, alloc)?;
    let work = alloc.alloc_temp_n(7)?;
    wasm_fn.instruction(&Instruction::LocalSet(work + 2));
    wasm_fn.instruction(&Instruction::LocalSet(work + 1));
    wasm_fn.instruction(&Instruction::LocalSet(work));
    emit_smoothstep_e0_e1_x_slots(wasm_fn, mode, alloc, work)
}

fn emit_scalar_step(
    module: &Module,
    func: &Function,
    edge: Handle<Expression>,
    x: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    emit_expr(module, func, x, wasm_fn, mode, alloc)?;
    emit_expr(module, func, edge, wasm_fn, mode, alloc)?;
    emit_step_x_edge_stack(wasm_fn, mode, alloc)
}

/// `work..work+2` hold e0,e1,x. Uses `work+3..=work+6` as temporaries.
pub(crate) fn emit_smoothstep_e0_e1_x_slots(
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
    work: u32,
) -> Result<(), String> {
    let e0 = work;
    let e1 = work + 1;
    let xv = work + 2;
    let range_s = work + 3;
    let t_s = work + 4;
    let tt_s = work + 5;
    let aux_s = work + 6;

    wasm_fn.instruction(&Instruction::LocalGet(e1));
    wasm_fn.instruction(&Instruction::LocalGet(e0));
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Sub);
        }
        FloatMode::Q32 => {
            emit_q32_sub_sat(wasm_fn, alloc)?;
        }
    }
    wasm_fn.instruction(&Instruction::LocalSet(range_s));

    wasm_fn.instruction(&Instruction::LocalGet(xv));
    wasm_fn.instruction(&Instruction::LocalGet(e0));
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Sub);
        }
        FloatMode::Q32 => {
            emit_q32_sub_sat(wasm_fn, alloc)?;
        }
    }
    wasm_fn.instruction(&Instruction::LocalGet(range_s));
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Div);
        }
        FloatMode::Q32 => {
            emit_q32_div(wasm_fn, alloc)?;
        }
    }

    // clamp to [0, 1]
    match mode {
        FloatMode::Float => {
            emit_literal(&Literal::F32(0.0), wasm_fn, mode)?;
            wasm_fn.instruction(&Instruction::F32Max);
            emit_literal(&Literal::F32(1.0), wasm_fn, mode)?;
            wasm_fn.instruction(&Instruction::F32Min);
        }
        FloatMode::Q32 => {
            emit_literal(&Literal::F32(0.0), wasm_fn, mode)?;
            emit_scalar_min_max(true, ScalarKind::Float, mode, wasm_fn, alloc)?;
            emit_literal(&Literal::F32(1.0), wasm_fn, mode)?;
            emit_scalar_min_max(false, ScalarKind::Float, mode, wasm_fn, alloc)?;
        }
    }
    wasm_fn.instruction(&Instruction::LocalSet(t_s));

    wasm_fn.instruction(&Instruction::LocalGet(t_s));
    wasm_fn.instruction(&Instruction::LocalGet(t_s));
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Mul);
        }
        FloatMode::Q32 => {
            emit_q32_mul(wasm_fn, alloc)?;
        }
    }
    wasm_fn.instruction(&Instruction::LocalSet(tt_s));

    emit_literal(&Literal::F32(2.0), wasm_fn, mode)?;
    wasm_fn.instruction(&Instruction::LocalGet(t_s));
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Mul);
        }
        FloatMode::Q32 => {
            emit_q32_mul(wasm_fn, alloc)?;
        }
    }
    wasm_fn.instruction(&Instruction::LocalSet(aux_s));

    emit_literal(&Literal::F32(3.0), wasm_fn, mode)?;
    wasm_fn.instruction(&Instruction::LocalGet(aux_s));
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Sub);
        }
        FloatMode::Q32 => {
            emit_q32_sub_sat(wasm_fn, alloc)?;
        }
    }
    wasm_fn.instruction(&Instruction::LocalSet(aux_s));

    wasm_fn.instruction(&Instruction::LocalGet(tt_s));
    wasm_fn.instruction(&Instruction::LocalGet(aux_s));
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Mul);
        }
        FloatMode::Q32 => {
            emit_q32_mul(wasm_fn, alloc)?;
        }
    }
    Ok(())
}

/// Stack: `x`, `edge` (edge on top). Pushes `1.0` or `0.0` in the active float mode.
pub(crate) fn emit_step_x_edge_stack(
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let cond = alloc.splat_scratch;
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Ge);
        }
        FloatMode::Q32 => {
            wasm_fn.instruction(&Instruction::I32GeS);
        }
    }
    wasm_fn.instruction(&Instruction::LocalSet(cond));
    match mode {
        FloatMode::Float => {
            wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(1.0f32)));
            wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(0.0f32)));
        }
        FloatMode::Q32 => {
            wasm_fn.instruction(&Instruction::I32Const(65_536));
            wasm_fn.instruction(&Instruction::I32Const(0));
        }
    }
    wasm_fn.instruction(&Instruction::LocalGet(cond));
    wasm_fn.instruction(&Instruction::Select);
    Ok(())
}

fn emit_f32_mod(wasm_fn: &mut WasmFunction, alloc: &LocalAlloc) -> Result<(), String> {
    let (s0, s1) = alloc
        .float_scratch
        .ok_or_else(|| String::from("float scratch missing"))?;
    wasm_fn.instruction(&Instruction::LocalSet(s1));
    wasm_fn.instruction(&Instruction::LocalSet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::F32Div);
    wasm_fn.instruction(&Instruction::F32Trunc);
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::F32Mul);
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::F32Sub);
    Ok(())
}
