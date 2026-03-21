//! Map Naga `LocalVariable` handles to WASM `local` indices.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use lp_glsl_naga::FloatMode;
use naga::{Expression, Function, Handle, LocalVariable, Module, Statement};
use wasm_encoder::ValType;

use crate::types::scalar_naga_inner_to_valtype;

/// WASM local index allocator for a single function.
///
/// WASM locals `0..param_count-1` are the function parameters. Naga models `in` parameters as
/// `LocalVariable` + `Store` from `FunctionArgument`; [`LocalAlloc::resolve_local_variable`] maps
/// those to argument indices.
pub struct LocalAlloc {
    param_aliases: BTreeMap<Handle<LocalVariable>, u32>,
    local_map: BTreeMap<Handle<LocalVariable>, u32>,
    /// `ValType` for each extra WASM local (indices align with `local_map` assignment order).
    extra_local_valtypes: Vec<ValType>,
    /// Two i32 scratch slots for Q32 × / % sequences (only `Some` in Q32 mode).
    pub q32_scratch: Option<(u32, u32)>,
    /// Two f32 scratch slots for float `%` lowering (only `Some` in Float mode).
    pub float_scratch: Option<(u32, u32)>,
}

impl LocalAlloc {
    pub fn new(module: &Module, func: &Function, mode: FloatMode) -> Self {
        let param_count = func.arguments.len() as u32;
        let param_aliases = build_param_aliases(func);

        let mut local_map = BTreeMap::new();
        let mut extra_local_valtypes = Vec::new();
        let mut next = param_count;
        for (handle, lv) in func.local_variables.iter() {
            if param_aliases.contains_key(&handle) {
                continue;
            }
            let inner = &module.types[lv.ty].inner;
            let vt = scalar_naga_inner_to_valtype(inner, mode);
            local_map.insert(handle, next);
            extra_local_valtypes.push(vt);
            next += 1;
        }

        let mut q32_scratch = None;
        let mut float_scratch = None;
        if matches!(mode, FloatMode::Q32) {
            let a = next;
            let b = next + 1;
            extra_local_valtypes.push(ValType::I32);
            extra_local_valtypes.push(ValType::I32);
            q32_scratch = Some((a, b));
        } else {
            let a = next;
            let b = next + 1;
            extra_local_valtypes.push(ValType::F32);
            extra_local_valtypes.push(ValType::F32);
            float_scratch = Some((a, b));
        }

        Self {
            param_aliases,
            local_map,
            extra_local_valtypes,
            q32_scratch,
            float_scratch,
        }
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
}

fn build_param_aliases(func: &Function) -> BTreeMap<Handle<LocalVariable>, u32> {
    let mut m = BTreeMap::new();
    walk_block(&func.body, func, &mut m);
    m
}

fn walk_block(block: &naga::Block, func: &Function, m: &mut BTreeMap<Handle<LocalVariable>, u32>) {
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
            Statement::Loop {
                body, continuing, ..
            } => {
                walk_block(body, func, m);
                walk_block(continuing, func, m);
            }
            Statement::Switch { cases, .. } => {
                for c in cases {
                    walk_block(&c.body, func, m);
                }
            }
            _ => {}
        }
    }
}
