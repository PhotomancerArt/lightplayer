//! LValue type definitions

use crate::semantic::types::Type as GlslType;
use alloc::vec::Vec;
use cranelift_codegen::ir::Value;
use cranelift_frontend::Variable;

/// Describes how to access data from a pointer
#[derive(Debug, Clone)]
pub enum PointerAccessPattern {
    /// Direct access: entire variable/vector/matrix
    /// Examples: `arr` (array variable), `v` (out/inout vec3 param)
    Direct { component_count: usize },
    /// Component access: `v.x`, `arr[i].xy`
    /// Examples: `v.x` (out/inout param component), `arr[0].x` (array element component)
    Component {
        indices: Vec<usize>,
        result_ty: GlslType,
    },
    /// Array element access: `arr[i]`
    /// Examples: `arr[0]`, `arr[idx]`
    ArrayElement {
        index: Option<usize>,     // Compile-time constant
        index_val: Option<Value>, // Runtime index
        element_ty: GlslType,
        element_size_bytes: usize,
        component_indices: Option<Vec<usize>>, // For arr[i].x
    },
}

/// Represents a modifiable location (LValue) in GLSL
///
/// This enum abstracts over all possible modifiable locations, allowing
/// unified handling of variables, vector components, matrix elements, etc.
#[derive(Debug, Clone)]
pub enum LValue {
    /// Simple variable: `x`
    Variable { vars: Vec<Variable>, ty: GlslType },
    /// Vector component access: `v.x` or `v.xy`
    Component {
        base_vars: Vec<Variable>,
        base_ty: GlslType,
        indices: Vec<usize>, // Component indices
        result_ty: GlslType,
    },
    /// Matrix element: `m[0][1]` (single scalar)
    MatrixElement {
        base_vars: Vec<Variable>,
        base_ty: GlslType,
        row: usize,
        col: usize,
    },
    /// Matrix column: `m[0]` (vector)
    MatrixColumn {
        base_vars: Vec<Variable>,
        base_ty: GlslType,
        col: usize,
        result_ty: GlslType,
    },
    /// Vector element: `v[0]` (single scalar)
    VectorElement {
        base_vars: Vec<Variable>,
        base_ty: GlslType,
        index: usize, // Component index (0=x, 1=y, 2=z, 3=w)
    },
    /// Array element: `arr[i]` (single element, can be scalar or vector)
    ArrayElement {
        array_ptr: cranelift_codegen::ir::Value,
        base_ty: GlslType,
        index: Option<usize>, // Compile-time constant index
        index_val: Option<cranelift_codegen::ir::Value>, // Runtime index value
        element_ty: GlslType,
        element_size_bytes: usize,
        component_indices: Option<Vec<usize>>, // For component access like arr[i].x
    },
    /// Pointer-based storage: arrays, out/inout params, future structs
    PointerBased {
        ptr: Value,
        base_ty: GlslType,
        access_pattern: PointerAccessPattern,
    },
}

impl LValue {
    /// Get the type of this LValue
    pub fn ty(&self) -> GlslType {
        match self {
            LValue::Variable { ty, .. } => ty.clone(),
            LValue::Component { result_ty, .. } => result_ty.clone(),
            LValue::MatrixElement { .. } => {
                // Matrix element is always float scalar
                GlslType::Float
            }
            LValue::MatrixColumn { result_ty, .. } => result_ty.clone(),
            LValue::VectorElement { base_ty, .. } => {
                // Vector element type is the base type of the vector
                base_ty.vector_base_type().unwrap()
            }
            LValue::ArrayElement {
                element_ty,
                component_indices,
                ..
            } => {
                // If component_indices is set, return component type, otherwise element type
                if let Some(indices) = component_indices {
                    if indices.len() == 1 {
                        element_ty.vector_base_type().unwrap_or(element_ty.clone())
                    } else {
                        element_ty
                            .vector_base_type()
                            .and_then(|base| GlslType::vector_type(&base, indices.len()))
                            .unwrap_or(element_ty.clone())
                    }
                } else {
                    element_ty.clone()
                }
            }
            LValue::PointerBased {
                base_ty,
                access_pattern,
                ..
            } => {
                match access_pattern {
                    PointerAccessPattern::Direct { .. } => base_ty.clone(),
                    PointerAccessPattern::Component { result_ty, .. } => result_ty.clone(),
                    PointerAccessPattern::ArrayElement {
                        element_ty,
                        component_indices,
                        ..
                    } => {
                        // Similar to ArrayElement variant logic
                        if let Some(indices) = component_indices {
                            if indices.len() == 1 {
                                element_ty.vector_base_type().unwrap_or(element_ty.clone())
                            } else {
                                element_ty
                                    .vector_base_type()
                                    .and_then(|base| GlslType::vector_type(&base, indices.len()))
                                    .unwrap_or(element_ty.clone())
                            }
                        } else {
                            element_ty.clone()
                        }
                    }
                }
            }
        }
    }
}
