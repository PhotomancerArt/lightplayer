//! NumericStrategy abstraction for pluggable float representation.
//!
//! The GLSL semantic analysis always works with float semantics. This module
//! controls how those semantics map to CLIF IR instructions. FloatStrategy
//! emits standard float instructions; Q32Strategy (Plan B) will emit fixed-point.

use cranelift_codegen::ir::{InstBuilder, Signature, Type, Value, condcodes::FloatCC, types};
use cranelift_frontend::FunctionBuilder;

/// Strategy for emitting numeric (float) operations.
///
/// Uses enum dispatch to avoid generic parameter propagation. Each method
/// on NumericMode dispatches via match to the concrete strategy.
pub enum NumericMode {
    Float(FloatStrategy),
}

impl NumericMode {
    /// The CLIF type used for GLSL `float` values.
    pub fn scalar_type(&self) -> Type {
        match self {
            NumericMode::Float(s) => s.scalar_type(),
        }
    }

    /// Emit a constant value.
    pub fn emit_const(&self, val: f32, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_const(val, builder),
        }
    }

    /// Emit add.
    pub fn emit_add(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_add(a, b, builder),
        }
    }

    /// Emit subtract.
    pub fn emit_sub(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_sub(a, b, builder),
        }
    }

    /// Emit multiply.
    pub fn emit_mul(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_mul(a, b, builder),
        }
    }

    /// Emit divide.
    pub fn emit_div(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_div(a, b, builder),
        }
    }

    /// Emit negate.
    pub fn emit_neg(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_neg(a, builder),
        }
    }

    /// Emit absolute value.
    pub fn emit_abs(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_abs(a, builder),
        }
    }

    /// Emit comparison. cc uses FloatCC semantics; Q32Strategy will translate to IntCC.
    pub fn emit_cmp(
        &self,
        cc: FloatCC,
        a: Value,
        b: Value,
        builder: &mut FunctionBuilder,
    ) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_cmp(cc, a, b, builder),
        }
    }

    /// Emit min.
    pub fn emit_min(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_min(a, b, builder),
        }
    }

    /// Emit max.
    pub fn emit_max(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_max(a, b, builder),
        }
    }

    /// Emit floor.
    pub fn emit_floor(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_floor(a, builder),
        }
    }

    /// Emit ceil.
    pub fn emit_ceil(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_ceil(a, builder),
        }
    }

    /// Emit sqrt.
    pub fn emit_sqrt(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_sqrt(a, builder),
        }
    }

    /// Convert signed integer to scalar type.
    pub fn emit_from_sint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_from_sint(a, builder),
        }
    }

    /// Convert scalar type to signed integer.
    pub fn emit_to_sint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_to_sint(a, builder),
        }
    }

    /// Convert unsigned integer to scalar type.
    pub fn emit_from_uint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_from_uint(a, builder),
        }
    }

    /// Convert scalar type to unsigned integer.
    pub fn emit_to_uint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_to_uint(a, builder),
        }
    }

    /// Transform a float-semantic Signature to the target representation.
    pub fn map_signature(&self, sig: &Signature) -> Signature {
        match self {
            NumericMode::Float(s) => s.map_signature(sig),
        }
    }
}

/// Float strategy: emits standard CLIF float instructions.
pub struct FloatStrategy;

impl FloatStrategy {
    pub fn scalar_type(&self) -> Type {
        types::F32
    }

    pub fn emit_const(&self, val: f32, builder: &mut FunctionBuilder) -> Value {
        builder.ins().f32const(val)
    }

    pub fn emit_add(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fadd(a, b)
    }

    pub fn emit_sub(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fsub(a, b)
    }

    pub fn emit_mul(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fmul(a, b)
    }

    pub fn emit_div(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fdiv(a, b)
    }

    pub fn emit_neg(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fneg(a)
    }

    pub fn emit_abs(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fabs(a)
    }

    pub fn emit_cmp(
        &self,
        cc: FloatCC,
        a: Value,
        b: Value,
        builder: &mut FunctionBuilder,
    ) -> Value {
        builder.ins().fcmp(cc, a, b)
    }

    pub fn emit_min(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fmin(a, b)
    }

    pub fn emit_max(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fmax(a, b)
    }

    pub fn emit_floor(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().floor(a)
    }

    pub fn emit_ceil(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().ceil(a)
    }

    pub fn emit_sqrt(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().sqrt(a)
    }

    pub fn emit_from_sint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fcvt_from_sint(types::F32, a)
    }

    pub fn emit_to_sint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fcvt_to_sint(types::I32, a)
    }

    pub fn emit_from_uint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fcvt_from_uint(types::F32, a)
    }

    pub fn emit_to_uint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().fcvt_to_uint(types::I32, a)
    }

    pub fn map_signature(&self, sig: &Signature) -> Signature {
        sig.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_strategy_scalar_type_is_f32() {
        let s = FloatStrategy;
        assert_eq!(s.scalar_type(), types::F32);
    }
}
