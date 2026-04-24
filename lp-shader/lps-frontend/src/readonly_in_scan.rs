//! Intra-procedural read-only classification for by-value `in` aggregate parameters (M5).
//!
//! Used to elide the entry `Memcpy` from the pointer parameter into a stack slot when the
//! aggregate is never written and is not passed onward as a callee `inout`/`out` argument.

use alloc::collections::BTreeMap;
use alloc::string::String;

use naga::{
    AddressSpace, Block, Expression, Function, Handle, LocalVariable, Module, Statement, TypeInner,
};

use crate::lower_array_multidim::{
    peel_access_chain, peel_access_index_chain, peel_array_subscript_chain, ArraySubscriptRoot,
};
use crate::lower_error::LowerError;
use crate::lower_struct::peel_struct_access_index_chain_to_local;

/// For each argument index `i` where [`Function::arguments`]\[i\] is a by-value [`TypeInner::Array`]
/// or [`TypeInner::Struct`] (not [`TypeInner::Pointer`]), returns whether the parameter is
/// **read-only** in the sense of M5: safe to use [`crate::lower_ctx::AggregateSlot::ParamReadOnly`]
/// and elide the
/// prologue stack slot + `Memcpy`.
///
/// `true` read-only; `false` mutable (conservative when analysis is uncertain).
pub(crate) fn in_aggregate_param_read_only(
    module: &Module,
    func: &Function,
) -> Result<BTreeMap<u32, bool>, LowerError> {
    let mut out = BTreeMap::new();
    for (i, arg) in func.arguments.iter().enumerate() {
        let i = i as u32;
        match &module.types[arg.ty].inner {
            TypeInner::Array { .. } | TypeInner::Struct { .. } => {
                let ro = classify_one(module, func, i)?;
                out.insert(i, ro);
            }
            _ => {}
        }
    }
    Ok(out)
}

fn classify_one(module: &Module, func: &Function, arg_i: u32) -> Result<bool, LowerError> {
    let Some(param_lv) = local_for_in_aggregate_value_param_optional(func, module, arg_i)? else {
        return Ok(false);
    };
    if store_any_mutates_param(func, param_lv) {
        return Ok(false);
    }
    if call_any_passes_param_to_inout_out(module, func, arg_i, param_lv) {
        return Ok(false);
    }
    Ok(true)
}

fn store_any_mutates_param(func: &Function, param_lv: Handle<LocalVariable>) -> bool {
    let mut found = false;
    fn walk(
        block: &Block,
        func: &Function,
        param_lv: Handle<LocalVariable>,
        found: &mut bool,
    ) {
        for stmt in block.iter() {
            if *found {
                return;
            }
            match stmt {
                Statement::Store { pointer, .. } => {
                    if store_pointer_targets_param_aggregate(func, *pointer, param_lv) {
                        *found = true;
                        return;
                    }
                }
                Statement::Block(inner) => walk(inner, func, param_lv, found),
                Statement::If { accept, reject, .. } => {
                    walk(accept, func, param_lv, found);
                    walk(reject, func, param_lv, found);
                }
                Statement::Switch { cases, .. } => {
                    for case in cases.iter() {
                        walk(&case.body, func, param_lv, found);
                    }
                }
                Statement::Loop {
                    body, continuing, ..
                } => {
                    walk(body, func, param_lv, found);
                    walk(continuing, func, param_lv, found);
                }
                _ => {}
            }
        }
    }
    walk(&func.body, func, param_lv, &mut found);
    found
}

fn call_any_passes_param_to_inout_out(
    module: &Module,
    func: &Function,
    param_index: u32,
    param_lv: Handle<LocalVariable>,
) -> bool {
    let mut found = false;
    fn walk(
        block: &Block,
        module: &Module,
        func: &Function,
        param_index: u32,
        param_lv: Handle<LocalVariable>,
        found: &mut bool,
    ) {
        for stmt in block.iter() {
            if *found {
                return;
            }
            match stmt {
                Statement::Call {
                    function: callee,
                    arguments,
                    ..
                } => {
                    let f = &module.functions[*callee];
                    for (j, &arg_h) in arguments.iter().enumerate() {
                        let callee_arg = &f.arguments[j];
                        if !callee_formal_is_function_pointer(module, callee_arg) {
                            continue;
                        }
                        if call_arg_expr_aliases_in_param(func, arg_h, param_index, param_lv) {
                            *found = true;
                            return;
                        }
                    }
                }
                Statement::Block(inner) => walk(inner, module, func, param_index, param_lv, found),
                Statement::If { accept, reject, .. } => {
                    walk(accept, module, func, param_index, param_lv, found);
                    walk(reject, module, func, param_index, param_lv, found);
                }
                Statement::Switch { cases, .. } => {
                    for case in cases.iter() {
                        walk(&case.body, module, func, param_index, param_lv, found);
                    }
                }
                Statement::Loop {
                    body, continuing, ..
                } => {
                    walk(body, module, func, param_index, param_lv, found);
                    walk(continuing, module, func, param_index, param_lv, found);
                }
                _ => {}
            }
        }
    }
    walk(
        &func.body,
        module,
        func,
        param_index,
        param_lv,
        &mut found,
    );
    found
}

/// GLSL: only `inout` / `out` use [`Pointer`] in function address space; `in` aggregates are
/// passed by value.
fn callee_formal_is_function_pointer(
    module: &Module,
    arg: &naga::FunctionArgument,
) -> bool {
    matches!(
        &module.types[arg.ty].inner,
        TypeInner::Pointer {
            space: AddressSpace::Function,
            ..
        }
    )
}

fn call_arg_expr_aliases_in_param(
    func: &Function,
    mut expr: Handle<Expression>,
    param_index: u32,
    param_lv: Handle<LocalVariable>,
) -> bool {
    loop {
        match &func.expressions[expr] {
            Expression::As { expr: inner, .. } => expr = *inner,
            Expression::Load { pointer } => expr = *pointer,
            _ => break,
        }
    }
    match &func.expressions[expr] {
        Expression::FunctionArgument(i) => *i == param_index,
        Expression::LocalVariable(lv) => *lv == param_lv,
        _ => false,
    }
}

fn store_pointer_targets_param_aggregate(
    func: &Function,
    pointer: naga::Handle<Expression>,
    param_lv: Handle<LocalVariable>,
) -> bool {
    let mut p = pointer;
    loop {
        match &func.expressions[p] {
            Expression::As { expr: inner, .. } => p = *inner,
            Expression::Load { pointer: inner } => p = *inner,
            _ => break,
        }
    }
    match &func.expressions[p] {
        Expression::LocalVariable(lv) => *lv == param_lv,
        Expression::GlobalVariable(_) | Expression::FunctionArgument(_) => false,
        Expression::Access { .. } | Expression::AccessIndex { .. } => {
            if let Some((root, _)) = peel_array_subscript_chain(func, p) {
                if let ArraySubscriptRoot::Local(lv) = root {
                    if lv == param_lv {
                        return true;
                    }
                }
            }
            if let Some((lv, _)) = peel_struct_access_index_chain_to_local(func, p) {
                if lv == param_lv {
                    return true;
                }
            }
            if let Some((lv, _)) = peel_access_index_chain(func, p) {
                if lv == param_lv {
                    return true;
                }
            }
            if let Some((lv, _)) = peel_access_chain(func, p) {
                if lv == param_lv {
                    return true;
                }
            }
            // Unmodelled lvalue shape: be conservative and treat as a possible param write.
            true
        }
        _ => true,
    }
}

/// Naga GLSL: the [`LocalVariable`] for a by-value `in` array or struct param is matched by
/// parameter name and type to `func.arguments[i]`.
/// Shadow [`LocalVariable`] for a by-value aggregate parameter, if Naga emitted one (`const` params
/// may omit it).
pub(crate) fn local_for_in_aggregate_value_param_optional(
    func: &Function,
    module: &Module,
    param_index: u32,
) -> Result<Option<Handle<LocalVariable>>, LowerError> {
    let i = param_index as usize;
    let arg = func
        .arguments
        .get(i)
        .ok_or_else(|| LowerError::Internal(String::from("in aggregate: bad argument index")))?;
    if !matches!(
        &module.types[arg.ty].inner,
        TypeInner::Array { .. } | TypeInner::Struct { .. }
    ) {
        return Err(LowerError::Internal(String::from(
            "in aggregate: not an array or struct type",
        )));
    }
    if let Some(name) = &arg.name {
        for (lv, v) in func.local_variables.iter() {
            if v.ty == arg.ty && v.name.as_deref() == Some(name.as_str()) {
                return Ok(Some(lv));
            }
        }
    }
    let mut found = None;
    for (lv, v) in func.local_variables.iter() {
        if v.ty == arg.ty {
            if found.is_some() {
                return Err(LowerError::Internal(String::from(
                    "in aggregate: ambiguous local (use matching parameter name)",
                )));
            }
            found = Some(lv);
        }
    }
    Ok(found)
}
