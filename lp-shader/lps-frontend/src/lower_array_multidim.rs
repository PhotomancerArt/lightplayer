//! Multi-dimensional array helpers: flat row-major index and `AccessIndex` chains.

use alloc::format;
use alloc::string::String;

use alloc::vec::Vec;

use naga::{
    ArraySize, Expression, Function, GlobalVariable, Handle, LocalVariable, Module, Type, TypeInner,
};
use smallvec::SmallVec;

use crate::lower_aggregate_layout::array_element_stride;
use crate::lower_error::LowerError;

/// Row-major flat index with per-axis clamping (matches v1 clamp semantics).
pub(crate) fn flat_index_const_clamped(
    dimensions: &[u32],
    indices: &[u32],
) -> Result<u32, LowerError> {
    if dimensions.is_empty() || dimensions.len() != indices.len() {
        return Err(LowerError::Internal(format!(
            "flat_index_const_clamped: dimensions {} vs indices {}",
            dimensions.len(),
            indices.len()
        )));
    }
    let mut flat = 0u32;
    for (d, &i) in dimensions.iter().zip(indices.iter()) {
        if *d == 0 {
            return Err(LowerError::Internal(String::from(
                "flat_index_const_clamped: zero dimension",
            )));
        }
        let clamped = i.min(*d - 1);
        flat = flat
            .checked_mul(*d)
            .and_then(|x| x.checked_add(clamped))
            .ok_or_else(|| LowerError::Internal(String::from("flat index overflow")))?;
    }
    Ok(flat)
}

/// `Access(Access(..LocalVariable), …)` for dynamic subscripts (outer index last in vector).
pub(crate) fn peel_access_chain(
    func: &Function,
    mut expr: Handle<Expression>,
) -> Option<(Handle<LocalVariable>, Vec<Handle<Expression>>)> {
    let mut indices = Vec::new();
    loop {
        match &func.expressions[expr] {
            Expression::Access { base, index } => {
                indices.push(*index);
                expr = *base;
            }
            Expression::LocalVariable(lv) => {
                indices.reverse();
                return Some((*lv, indices));
            }
            _ => return None,
        }
    }
}

/// One indexing step: constant (`[]` literal) or dynamic (`[i]`).
#[derive(Clone, Debug)]
pub(crate) enum SubscriptOperand {
    Const(u32),
    Dynamic(Handle<Expression>),
}

/// Root of a peeled `Access` / `AccessIndex` array chain: stack local, `out` / `inout` param, or call result.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ArraySubscriptRoot {
    Local(Handle<LocalVariable>),
    /// Function argument index; must be in [`crate::lower_ctx::LowerCtx::pointer_args`] with array pointee.
    Param(u32),
    /// [`Expression::CallResult`] from a callee with aggregate return (slot in [`LowerCtx::call_result_aggregates`](crate::lower_ctx::LowerCtx::call_result_aggregates)).
    CallResult(Handle<Expression>),
    /// Private/uniform `[` `]` roots on globals (VMContext); writable actuals reject uniforms elsewhere.
    Global(Handle<GlobalVariable>),
}

/// Mixed `Access` / `AccessIndex` chain ending at [`LocalVariable`] or array pointer [`Expression::FunctionArgument`] (outer index first in vector).
///
/// NOTE: Unlike `peel_access_chain` which needs to reverse because Access chains
/// are stored inner-to-outer, AccessIndex chains appear to be stored outer-to-inner
/// in Naga's representation, so we do NOT reverse here.
pub(crate) fn peel_array_subscript_chain(
    func: &Function,
    mut expr: Handle<Expression>,
) -> Option<(ArraySubscriptRoot, Vec<SubscriptOperand>)> {
    let mut ops = Vec::new();
    loop {
        match &func.expressions[expr] {
            Expression::AccessIndex { base, index } => {
                ops.push(SubscriptOperand::Const(*index));
                expr = *base;
            }
            Expression::Access { base, index } => {
                ops.push(SubscriptOperand::Dynamic(*index));
                expr = *base;
            }
            Expression::LocalVariable(lv) => {
                // Walking from the full r-value toward `LocalVariable`, each step pushes this
                // bracket's index. For `arr[row][col]` that yields `[col, row]` — opposite of GLSL
                // left-to-right order, which must align with `dimensions` (outermost first).
                ops.reverse();
                return Some((ArraySubscriptRoot::Local(*lv), ops));
            }
            Expression::GlobalVariable(gv) => {
                ops.reverse();
                return Some((ArraySubscriptRoot::Global(*gv), ops));
            }
            Expression::FunctionArgument(arg_i) => {
                ops.reverse();
                return Some((ArraySubscriptRoot::Param(*arg_i), ops));
            }
            Expression::CallResult(_) => {
                ops.reverse();
                return Some((ArraySubscriptRoot::CallResult(expr), ops));
            }
            _ => return None,
        }
    }
}

/// `AccessIndex`-only peel (legacy call sites that only use literals).
pub(crate) fn peel_access_index_chain(
    func: &Function,
    expr: Handle<Expression>,
) -> Option<(Handle<LocalVariable>, SmallVec<[u32; 4]>)> {
    let (root, ops) = peel_array_subscript_chain(func, expr)?;
    let ArraySubscriptRoot::Local(lv) = root else {
        return None;
    };
    let mut indices = SmallVec::<[u32; 4]>::new();
    for op in ops {
        match op {
            SubscriptOperand::Const(c) => indices.push(c),
            SubscriptOperand::Dynamic(_) => return None,
        }
    }
    Some((lv, indices))
}

/// Walk nested `TypeInner::Array`, outermost dimension first; returns leaf type and byte stride
/// per leaf (std430 element stride from [`crate::lower_aggregate_layout::array_element_stride`]).
///
/// The leaf may be any sized element type Naga places after the array nest (including
/// [`TypeInner::Struct`]); stride is computed from the full `LpsType` layout.
pub(crate) fn flatten_local_array_shape(
    module: &Module,
    func: &Function,
    var: &LocalVariable,
) -> Result<(SmallVec<[u32; 4]>, Handle<Type>, u32), LowerError> {
    let mut dimensions = SmallVec::<[u32; 4]>::new();
    let mut cur_ty = var.ty;

    let leaf_ty = loop {
        match &module.types[cur_ty].inner {
            TypeInner::Array { base, size, .. } => {
                let n = match size {
                    ArraySize::Constant(nz) => nz.get(),
                    ArraySize::Pending(_) | ArraySize::Dynamic => {
                        if !dimensions.is_empty() {
                            return Err(LowerError::UnsupportedType(String::from(
                                "only outermost array may use inferred size (`[]`)",
                            )));
                        }
                        let Some(init_h) = var.init else {
                            return Err(LowerError::UnsupportedType(String::from(
                                "unsized local array requires an initializer",
                            )));
                        };
                        match &func.expressions[init_h] {
                            Expression::Compose { components, .. } => {
                                u32::try_from(components.len()).map_err(|_| {
                                    LowerError::Internal(String::from(
                                        "inferred array length overflows u32",
                                    ))
                                })?
                            }
                            _ => {
                                return Err(LowerError::UnsupportedType(String::from(
                                    "array size must be constant or inferable from `{ ... }` init",
                                )));
                            }
                        }
                    }
                };
                dimensions.push(n);
                match &module.types[*base].inner {
                    TypeInner::Array { .. } => {
                        cur_ty = *base;
                    }
                    _ => {
                        break *base;
                    }
                }
            }
            _ => {
                return Err(LowerError::Internal(String::from(
                    "flatten_local_array_shape: local is not array-typed",
                )));
            }
        }
    };

    let leaf_stride = array_element_stride(module, leaf_ty)?;

    // Naga nests `T[a][b]` so the type walk collects sizes inner-to-outer (`[b, a]`).
    // GLSL / stack layout use outermost (`a`) first in `dimensions` and in `arr[i][j]`.
    dimensions.reverse();

    Ok((dimensions, leaf_ty, leaf_stride))
}

/// Like [`flatten_local_array_shape`], but from an array [`naga::Type`] handle only (no initializer).
/// Used for `in T[N]` function parameters, which are value types in Naga (not pointers).
///
/// Struct and other aggregate leaves are supported the same way as scalars and vectors.
pub(crate) fn flatten_array_type_shape(
    module: &Module,
    mut cur_ty: Handle<Type>,
) -> Result<(SmallVec<[u32; 4]>, Handle<Type>, u32), LowerError> {
    let mut dimensions = SmallVec::<[u32; 4]>::new();

    let leaf_ty = loop {
        match &module.types[cur_ty].inner {
            TypeInner::Array { base, size, .. } => {
                let n = match size {
                    ArraySize::Constant(nz) => nz.get(),
                    ArraySize::Pending(_) | ArraySize::Dynamic => {
                        return Err(LowerError::UnsupportedType(String::from(
                            "flatten_array_type_shape: only constant-sized arrays supported",
                        )));
                    }
                };
                dimensions.push(n);
                match &module.types[*base].inner {
                    TypeInner::Array { .. } => {
                        cur_ty = *base;
                    }
                    _ => {
                        break *base;
                    }
                }
            }
            _ => {
                return Err(LowerError::Internal(String::from(
                    "flatten_array_type_shape: not an array type",
                )));
            }
        }
    };

    let leaf_stride = array_element_stride(module, leaf_ty)?;

    dimensions.reverse();

    Ok((dimensions, leaf_ty, leaf_stride))
}
