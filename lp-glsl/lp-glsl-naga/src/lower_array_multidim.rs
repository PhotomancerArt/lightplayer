//! Multi-dimensional array helpers: flat row-major index and `AccessIndex` chains.

use alloc::format;
use alloc::string::String;

use alloc::vec::Vec;

use naga::{ArraySize, Expression, Function, Handle, LocalVariable, Module, Type, TypeInner};
use smallvec::SmallVec;

use crate::lower_error::LowerError;
use crate::naga_util::naga_type_to_ir_types;

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

/// Mixed `Access` / `AccessIndex` chain ending at [`LocalVariable`] (outer index first in vector).
///
/// NOTE: Unlike `peel_access_chain` which needs to reverse because Access chains
/// are stored inner-to-outer, AccessIndex chains appear to be stored outer-to-inner
/// in Naga's representation, so we do NOT reverse here.
pub(crate) fn peel_array_subscript_chain(
    func: &Function,
    mut expr: Handle<Expression>,
) -> Option<(Handle<LocalVariable>, Vec<SubscriptOperand>)> {
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
                // AccessIndex chains: collected outer-to-inner, already in correct order.
                // Return outer index first for correct flat index calculation.
                return Some((*lv, ops));
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
    let (lv, ops) = peel_array_subscript_chain(func, expr)?;
    let mut indices = SmallVec::<[u32; 4]>::new();
    for op in ops {
        match op {
            SubscriptOperand::Const(c) => indices.push(c),
            SubscriptOperand::Dynamic(_) => return None,
        }
    }
    Some((lv, indices))
}

/// Walk nested `TypeInner::Array`, outermost dimension first; returns leaf type and byte stride per leaf.
///
/// NOTE: The leaf stride is rounded up to 4 bytes to ensure 4-byte alignment for RV32 loads/stores.
/// This means bool arrays use 4 bytes per element instead of 1, but maintains alignment.
pub(crate) fn flatten_local_array_shape(
    module: &Module,
    func: &Function,
    var: &LocalVariable,
) -> Result<(SmallVec<[u32; 4]>, Handle<Type>, u32), LowerError> {
    let mut dimensions = SmallVec::<[u32; 4]>::new();
    let mut cur_ty = var.ty;

    let (leaf_ty, leaf_stride) = loop {
        match &module.types[cur_ty].inner {
            TypeInner::Array { base, size, stride } => {
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
                let stride_v = *stride;
                match &module.types[*base].inner {
                    TypeInner::Array { .. } => {
                        cur_ty = *base;
                    }
                    _ => {
                        // Round up stride to 4 bytes for alignment (RV32 requires 4-byte aligned loads)
                        let aligned_stride = stride_v.max(4);
                        break (*base, aligned_stride);
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

    // Naga's array stride can be smaller than our stack layout: we place each scalar component at
    // `byte_off + j * 4` (see `store_array_element_const`). Ensure consecutive elements do not overlap
    // (e.g. `bvec4[2]` must stride by 16 bytes, not 4).
    let leaf_inner = &module.types[leaf_ty].inner;
    let ir_components = u32::try_from(naga_type_to_ir_types(leaf_inner)?.len()).map_err(|_| {
        LowerError::Internal(String::from(
            "flatten_local_array_shape: IR component count overflows u32",
        ))
    })?;
    let min_layout_stride = ir_components.saturating_mul(4);
    let leaf_stride = leaf_stride.max(min_layout_stride);

    Ok((dimensions, leaf_ty, leaf_stride))
}
