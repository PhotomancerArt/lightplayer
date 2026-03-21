//! Map Naga `LocalVariable` handles to WASM `local` indices.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_glsl_naga::FloatMode;
use naga::{Block, Expression, Function, Handle, LocalVariable, Module, Statement, Type};
use wasm_encoder::ValType;

use crate::types::{scalar_naga_inner_to_valtype, type_handle_component_count};

/// General-purpose temp locals for vector splat/swizzle/binary shuffling.
const SCRATCH_TEMP_COUNT: u32 = 16;

/// WASM local index allocator for a single function.
///
/// WASM locals `0..param_count-1` are the function parameters (each vector uses consecutive
/// slots). Naga models `in` parameters as `LocalVariable` + `Store` from `FunctionArgument`;
/// [`LocalAlloc::resolve_local_variable`] maps those to the WASM base index for that argument.
pub struct LocalAlloc {
    /// WASM local index of the first scalar of each function argument.
    arg_wasm_bases: Vec<u32>,
    param_aliases: BTreeMap<Handle<LocalVariable>, u32>,
    local_map: BTreeMap<Handle<LocalVariable>, u32>,
    /// `ValType` for each extra WASM local (indices align with `local_map` assignment order).
    extra_local_valtypes: Vec<ValType>,
    /// Two i32 scratch slots for Q32 × / % sequences (only `Some` in Q32 mode).
    pub q32_scratch: Option<(u32, u32)>,
    /// One i64 temp for Q32 fixed-point widening + saturation (only `Some` in Q32 mode).
    pub q32_i64_sat: Option<u32>,
    /// Two f32 scratch slots for float `%` lowering (only `Some` in Float mode).
    pub float_scratch: Option<(u32, u32)>,
    /// Two i32 scratch slots for `round()` lowering in Float mode (fixed-point detour).
    float_round_i32_scratch: Option<(u32, u32)>,
    /// Base index of [`SCRATCH_TEMP_COUNT`] consecutive temps for vector lowering.
    scratch_temp_base: u32,
    /// Short-lived slot for `Splat` / scalar broadcast (does not overlap [`alloc_temp_n`] pool).
    pub splat_scratch: u32,
    /// Single i32 slot for lowering scalar `&&` / `||` so the left bool survives [`emit_expr`] on the
    /// right operand (which may reuse [`scratch_temp_base`] / [`splat_scratch`]).
    ///
    /// Nested `&&` / `||` reuse this slot between operands; GLSL parses these left-associative, which
    /// matches that pattern. A right-nested tree would need additional stash locals.
    pub(crate) bool_binary_stash: u32,
    /// WASM base local for each `Expression::CallResult` (multi-slot for vector returns).
    call_result_bases: BTreeMap<Handle<Expression>, u32>,
}

impl LocalAlloc {
    pub fn new(module: &Module, func: &Function, mode: FloatMode) -> Self {
        let mut arg_wasm_bases: Vec<u32> = Vec::with_capacity(func.arguments.len());
        let mut next_param: u32 = 0;
        for arg in &func.arguments {
            arg_wasm_bases.push(next_param);
            next_param += type_handle_component_count(module, arg.ty);
        }
        let param_count = next_param;

        let param_aliases = build_param_aliases(func, &arg_wasm_bases);

        let mut local_map = BTreeMap::new();
        let mut extra_local_valtypes = Vec::new();
        let mut next = param_count;
        for (handle, lv) in func.local_variables.iter() {
            if param_aliases.contains_key(&handle) {
                continue;
            }
            let inner = &module.types[lv.ty].inner;
            let slots = type_handle_component_count(module, lv.ty);
            let vt = scalar_naga_inner_to_valtype(inner, mode);
            local_map.insert(handle, next);
            for _ in 0..slots {
                extra_local_valtypes.push(vt);
            }
            next += slots;
        }

        let mut q32_scratch = None;
        let mut q32_i64_sat = None;
        let mut float_scratch = None;
        let mut float_round_i32_scratch = None;
        if matches!(mode, FloatMode::Q32) {
            let a = next;
            let b = next + 1;
            extra_local_valtypes.push(ValType::I32);
            extra_local_valtypes.push(ValType::I32);
            q32_scratch = Some((a, b));
            next += 2;
            let acc = next;
            extra_local_valtypes.push(ValType::I64);
            q32_i64_sat = Some(acc);
            next += 1;
        } else {
            let a = next;
            let b = next + 1;
            extra_local_valtypes.push(ValType::F32);
            extra_local_valtypes.push(ValType::F32);
            float_scratch = Some((a, b));
            next += 2;
            let ra = next;
            let rb = next + 1;
            extra_local_valtypes.push(ValType::I32);
            extra_local_valtypes.push(ValType::I32);
            float_round_i32_scratch = Some((ra, rb));
            next += 2;
        }

        let scratch_vt = match mode {
            FloatMode::Q32 => ValType::I32,
            FloatMode::Float => ValType::F32,
        };
        let scratch_temp_base = next;
        for _ in 0..SCRATCH_TEMP_COUNT {
            extra_local_valtypes.push(scratch_vt);
        }
        next += SCRATCH_TEMP_COUNT;
        let splat_scratch = next;
        extra_local_valtypes.push(scratch_vt);
        next += 1;
        let bool_binary_stash = next;
        extra_local_valtypes.push(ValType::I32);
        next += 1;

        let mut call_results: Vec<(Handle<Expression>, Handle<Type>)> = Vec::new();
        collect_call_results_block(&func.body, module, &mut call_results);
        let mut call_result_bases = BTreeMap::new();
        for (expr_h, ty_h) in call_results {
            let slots = type_handle_component_count(module, ty_h);
            let inner = &module.types[ty_h].inner;
            let comp_vt = scalar_naga_inner_to_valtype(inner, mode);
            call_result_bases.insert(expr_h, next);
            for _ in 0..slots {
                extra_local_valtypes.push(comp_vt);
            }
            next += slots;
        }

        Self {
            arg_wasm_bases,
            param_aliases,
            local_map,
            extra_local_valtypes,
            q32_scratch,
            q32_i64_sat,
            float_scratch,
            float_round_i32_scratch,
            scratch_temp_base,
            splat_scratch,
            bool_binary_stash,
            call_result_bases,
        }
    }

    pub fn function_argument_wasm_base(&self, arg_idx: u32) -> Option<u32> {
        self.arg_wasm_bases.get(arg_idx as usize).copied()
    }

    /// i32 scratch used by Q32 `round()` / float `round()` (fixed-point) lowering.
    pub(crate) fn round_i32_scratch(&self) -> Option<(u32, u32)> {
        self.q32_scratch.or(self.float_round_i32_scratch)
    }

    /// WASM additional locals in index order (merged runs of the same [`ValType`] only when adjacent).
    pub fn wasm_local_groups(&self) -> Vec<(u32, ValType)> {
        let mut out: Vec<(u32, ValType)> = Vec::new();
        for &vt in &self.extra_local_valtypes {
            if let Some((n, prev)) = out.last_mut() {
                if *prev == vt {
                    *n += 1;
                    continue;
                }
            }
            out.push((1, vt));
        }
        out
    }

    pub fn resolve_local_variable(&self, lv: Handle<LocalVariable>) -> Option<u32> {
        self.param_aliases
            .get(&lv)
            .or_else(|| self.local_map.get(&lv))
            .copied()
    }

    pub fn is_parameter_alias(&self, lv: Handle<LocalVariable>) -> bool {
        self.param_aliases.contains_key(&lv)
    }

    /// Scalar slots for this local (1 for scalars, N for `vecN`).
    pub fn local_variable_slots(
        &self,
        module: &Module,
        func: &Function,
        lv: Handle<LocalVariable>,
    ) -> u32 {
        type_handle_component_count(module, func.local_variables[lv].ty)
    }

    /// `n` consecutive scratch locals; all vector lowering shares one pool (no overlapping ops).
    pub fn alloc_temp_n(&self, n: u32) -> Result<u32, String> {
        if n > SCRATCH_TEMP_COUNT {
            return Err(format!(
                "WASM codegen: scratch temp request {n} exceeds pool {SCRATCH_TEMP_COUNT}"
            ));
        }
        Ok(self.scratch_temp_base)
    }

    /// First WASM local index of the shared scratch pool ([`alloc_temp_n`] always starts here).
    pub(crate) fn scratch_pool_base(&self) -> u32 {
        self.scratch_temp_base
    }

    /// Number of consecutive scratch locals ([`alloc_temp_n`] must not assume more than this).
    pub(crate) fn scratch_pool_len() -> u32 {
        SCRATCH_TEMP_COUNT
    }

    pub fn call_result_wasm_base(&self, res: Handle<Expression>) -> Option<u32> {
        self.call_result_bases.get(&res).copied()
    }
}

fn collect_call_results_block(
    block: &Block,
    module: &Module,
    out: &mut Vec<(Handle<Expression>, Handle<Type>)>,
) {
    for stmt in block.iter() {
        match stmt {
            Statement::Call {
                function,
                result: Some(res_expr),
                ..
            } => {
                let callee = &module.functions[*function];
                let Some(fr) = callee.result.as_ref() else {
                    continue;
                };
                out.push((*res_expr, fr.ty));
            }
            Statement::Block(inner) => collect_call_results_block(inner, module, out),
            Statement::If { accept, reject, .. } => {
                collect_call_results_block(accept, module, out);
                collect_call_results_block(reject, module, out);
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                collect_call_results_block(body, module, out);
                collect_call_results_block(continuing, module, out);
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    collect_call_results_block(&case.body, module, out);
                }
            }
            _ => {}
        }
    }
}

fn build_param_aliases(
    func: &Function,
    arg_wasm_bases: &[u32],
) -> BTreeMap<Handle<LocalVariable>, u32> {
    let mut m = BTreeMap::new();
    walk_block(&func.body, func, arg_wasm_bases, &mut m);
    m
}

fn walk_block(
    block: &naga::Block,
    func: &Function,
    arg_wasm_bases: &[u32],
    m: &mut BTreeMap<Handle<LocalVariable>, u32>,
) {
    for stmt in block.iter() {
        match stmt {
            Statement::Store { pointer, value } => {
                if let (Expression::LocalVariable(lv), Expression::FunctionArgument(idx)) =
                    (&func.expressions[*pointer], &func.expressions[*value])
                {
                    let base = arg_wasm_bases.get(*idx as usize).copied().unwrap_or(*idx);
                    m.insert(*lv, base);
                }
            }
            Statement::Block(inner) => walk_block(inner, func, arg_wasm_bases, m),
            Statement::If { accept, reject, .. } => {
                walk_block(accept, func, arg_wasm_bases, m);
                walk_block(reject, func, arg_wasm_bases, m);
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                walk_block(body, func, arg_wasm_bases, m);
                walk_block(continuing, func, arg_wasm_bases, m);
            }
            Statement::Switch { cases, .. } => {
                for c in cases {
                    walk_block(&c.body, func, arg_wasm_bases, m);
                }
            }
            _ => {}
        }
    }
}
