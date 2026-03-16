//! RValue abstraction for unified handling of computed values
//!
//! This module provides a unified interface for handling all computed values
//! (scalars, vectors, matrices, etc.) in a single place, following Clang's
//! proven architecture for separating LValue (locations) from RValue (values).

use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::Value;

use alloc::{vec, vec::Vec};

/// Represents an RValue (right-hand value) - a computed value
///
/// This enum abstracts over all possible computed values, allowing
/// unified handling of scalars, vectors, matrices, etc.
/// The type is stored alongside the values for type checking and conversion.
#[derive(Debug, Clone)]
pub struct RValue {
    /// The computed values
    values: Vec<Value>,
    /// The type of this RValue
    ty: GlslType,
}

impl RValue {
    /// Create an RValue from a single scalar value
    pub fn from_scalar(val: Value, ty: GlslType) -> Self {
        RValue {
            values: vec![val],
            ty,
        }
    }

    /// Create an RValue from multiple values (aggregate)
    pub fn from_aggregate(vals: Vec<Value>, ty: GlslType) -> Self {
        RValue { values: vals, ty }
    }

    /// Get the type of this RValue
    pub fn ty(&self) -> &GlslType {
        &self.ty
    }

    /// Extract as scalar value, if this is a scalar
    pub fn as_scalar(&self) -> Option<Value> {
        if self.values.len() == 1 {
            Some(self.values[0])
        } else {
            None
        }
    }

    /// Extract as aggregate (vector/matrix), if this is an aggregate
    pub fn as_aggregate(&self) -> &[Value] {
        &self.values
    }

    /// Convert into a vector of values
    pub fn into_values(self) -> Vec<Value> {
        self.values
    }

    /// Get a reference to the values
    pub fn values(&self) -> &[Value] {
        &self.values
    }

    /// Get the number of component values
    pub fn component_count(&self) -> usize {
        self.values.len()
    }
}
