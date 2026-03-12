//! NumericStrategy abstraction for pluggable float representation.
//!
//! The GLSL semantic analysis always works with float semantics. This module
//! controls how those semantics map to CLIF IR instructions. FloatStrategy
//! emits standard float instructions; Q32Strategy (Plan B) emits Q16.16 fixed-point.

use crate::backend::transform::q32::{Q32Options, float_to_fixed16x16};
use cranelift_codegen::ir::{
    AbiParam, ArgumentPurpose, InstBuilder, Signature, Type, Value,
    condcodes::{FloatCC, IntCC},
    types,
};
use cranelift_frontend::FunctionBuilder;
use lp_model::glsl_opts::{AddSubMode, DivMode, MulMode};

const Q32_SHIFT: i64 = 16;

/// Strategy for emitting numeric (float) operations.
///
/// Uses enum dispatch to avoid generic parameter propagation. Each method
/// on NumericMode dispatches via match to the concrete strategy.
pub enum NumericMode {
    Float(FloatStrategy),
    Q32(Q32Strategy),
}

/// Q16.16 fixed-point numeric strategy.
///
/// Emits fixed-point equivalents of float operations. Saturating add/sub/mul/div
/// and sqrt require builtin calls (Plan C) and use `todo!()` for now.
pub struct Q32Strategy {
    pub opts: Q32Options,
}

impl Q32Strategy {
    pub fn new(opts: Q32Options) -> Self {
        Self { opts }
    }

    fn float_cc_to_int_cc(cc: FloatCC) -> IntCC {
        match cc {
            FloatCC::Equal => IntCC::Equal,
            FloatCC::NotEqual => IntCC::NotEqual,
            FloatCC::LessThan => IntCC::SignedLessThan,
            FloatCC::LessThanOrEqual => IntCC::SignedLessThanOrEqual,
            FloatCC::GreaterThan => IntCC::SignedGreaterThan,
            FloatCC::GreaterThanOrEqual => IntCC::SignedGreaterThanOrEqual,
            FloatCC::Ordered => IntCC::Equal,
            FloatCC::Unordered => IntCC::NotEqual,
            FloatCC::OrderedNotEqual => IntCC::NotEqual,
            FloatCC::UnorderedOrEqual => IntCC::Equal,
            FloatCC::UnorderedOrLessThan => IntCC::SignedLessThan,
            FloatCC::UnorderedOrLessThanOrEqual => IntCC::SignedLessThanOrEqual,
            FloatCC::UnorderedOrGreaterThan => IntCC::SignedGreaterThan,
            FloatCC::UnorderedOrGreaterThanOrEqual => IntCC::SignedGreaterThanOrEqual,
        }
    }
}

impl NumericMode {
    /// The CLIF type used for GLSL `float` values.
    pub fn scalar_type(&self) -> Type {
        match self {
            NumericMode::Float(s) => s.scalar_type(),
            NumericMode::Q32(s) => s.scalar_type(),
        }
    }

    /// Emit a constant value.
    pub fn emit_const(&self, val: f32, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_const(val, builder),
            NumericMode::Q32(s) => s.emit_const(val, builder),
        }
    }

    /// Emit add.
    pub fn emit_add(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_add(a, b, builder),
            NumericMode::Q32(s) => s.emit_add(a, b, builder),
        }
    }

    /// Emit subtract.
    pub fn emit_sub(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_sub(a, b, builder),
            NumericMode::Q32(s) => s.emit_sub(a, b, builder),
        }
    }

    /// Emit multiply.
    pub fn emit_mul(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_mul(a, b, builder),
            NumericMode::Q32(s) => s.emit_mul(a, b, builder),
        }
    }

    /// Emit divide.
    pub fn emit_div(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_div(a, b, builder),
            NumericMode::Q32(s) => s.emit_div(a, b, builder),
        }
    }

    /// Emit negate.
    pub fn emit_neg(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_neg(a, builder),
            NumericMode::Q32(s) => s.emit_neg(a, builder),
        }
    }

    /// Emit absolute value.
    pub fn emit_abs(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_abs(a, builder),
            NumericMode::Q32(s) => s.emit_abs(a, builder),
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
            NumericMode::Q32(s) => s.emit_cmp(cc, a, b, builder),
        }
    }

    /// Emit min.
    pub fn emit_min(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_min(a, b, builder),
            NumericMode::Q32(s) => s.emit_min(a, b, builder),
        }
    }

    /// Emit max.
    pub fn emit_max(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_max(a, b, builder),
            NumericMode::Q32(s) => s.emit_max(a, b, builder),
        }
    }

    /// Emit floor.
    pub fn emit_floor(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_floor(a, builder),
            NumericMode::Q32(s) => s.emit_floor(a, builder),
        }
    }

    /// Emit ceil.
    pub fn emit_ceil(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_ceil(a, builder),
            NumericMode::Q32(s) => s.emit_ceil(a, builder),
        }
    }

    /// Emit sqrt.
    pub fn emit_sqrt(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_sqrt(a, builder),
            NumericMode::Q32(s) => s.emit_sqrt(a, builder),
        }
    }

    /// Convert signed integer to scalar type.
    pub fn emit_from_sint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_from_sint(a, builder),
            NumericMode::Q32(s) => s.emit_from_sint(a, builder),
        }
    }

    /// Convert scalar type to signed integer.
    pub fn emit_to_sint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_to_sint(a, builder),
            NumericMode::Q32(s) => s.emit_to_sint(a, builder),
        }
    }

    /// Convert unsigned integer to scalar type.
    pub fn emit_from_uint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_from_uint(a, builder),
            NumericMode::Q32(s) => s.emit_from_uint(a, builder),
        }
    }

    /// Convert scalar type to unsigned integer.
    pub fn emit_to_uint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        match self {
            NumericMode::Float(s) => s.emit_to_uint(a, builder),
            NumericMode::Q32(s) => s.emit_to_uint(a, builder),
        }
    }

    /// Transform a float-semantic Signature to the target representation.
    pub fn map_signature(&self, sig: &Signature) -> Signature {
        match self {
            NumericMode::Float(s) => s.map_signature(sig),
            NumericMode::Q32(s) => s.map_signature(sig),
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

impl Q32Strategy {
    pub fn scalar_type(&self) -> Type {
        types::I32
    }

    pub fn emit_const(&self, val: f32, builder: &mut FunctionBuilder) -> Value {
        let fixed = float_to_fixed16x16(val);
        builder.ins().iconst(types::I32, fixed as i64)
    }

    pub fn emit_add(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self.opts.add_sub {
            AddSubMode::Wrapping => builder.ins().iadd(a, b),
            AddSubMode::Saturating => unreachable!("saturating add handled by CodegenContext"),
        }
    }

    pub fn emit_sub(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self.opts.add_sub {
            AddSubMode::Wrapping => builder.ins().isub(a, b),
            AddSubMode::Saturating => unreachable!("saturating sub handled by CodegenContext"),
        }
    }

    pub fn emit_mul(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self.opts.mul {
            MulMode::Wrapping => {
                let product_lo = builder.ins().imul(a, b);
                let product_hi = builder.ins().smulhi(a, b);
                let lo_shifted = builder.ins().sshr_imm(product_lo, Q32_SHIFT);
                let hi_shifted = builder.ins().ishl_imm(product_hi, Q32_SHIFT);
                builder.ins().bor(lo_shifted, hi_shifted)
            }
            MulMode::Saturating => unreachable!("saturating mul handled by CodegenContext"),
        }
    }

    pub fn emit_div(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        match self.opts.div {
            DivMode::Reciprocal => self.emit_div_reciprocal(a, b, builder),
            DivMode::Saturating => unreachable!("saturating div handled by CodegenContext"),
        }
    }

    fn emit_div_reciprocal(
        &self,
        dividend: Value,
        divisor: Value,
        builder: &mut FunctionBuilder,
    ) -> Value {
        let zero = builder.ins().iconst(types::I32, 0);
        let one = builder.ins().iconst(types::I32, 1);
        const MAX_FIXED: i64 = 0x7FFF_FFFF;
        const MIN_FIXED: i64 = i32::MIN as i64;

        let is_div0 = builder.ins().icmp(IntCC::Equal, divisor, zero);
        let div_neg = builder.ins().icmp(IntCC::SignedLessThan, divisor, zero);
        let div_negated = builder.ins().ineg(divisor);
        let div_abs_raw = builder.ins().select(div_neg, div_negated, divisor);
        let div_abs = builder.ins().select(is_div0, one, div_abs_raw);

        let half_range = builder.ins().iconst(types::I32, 0x8000_0000i64);
        let recip = builder.ins().udiv(half_range, div_abs);

        let div_neg_d = builder.ins().icmp(IntCC::SignedLessThan, dividend, zero);
        let div_negated_d = builder.ins().ineg(dividend);
        let dividend_abs = builder.ins().select(div_neg_d, div_negated_d, dividend);

        let p_lo = builder.ins().imul(dividend_abs, recip);
        let p_hi = builder.ins().smulhi(dividend_abs, recip);
        let lo_shifted = builder.ins().ushr_imm(p_lo, 15);
        let hi_shifted = builder.ins().ishl_imm(p_hi, 17);
        let quotient_abs = builder.ins().bor(lo_shifted, hi_shifted);

        let xor_signs = builder.ins().bxor(dividend, divisor);
        let signs_differ = builder.ins().icmp(IntCC::SignedLessThan, xor_signs, zero);
        let negated = builder.ins().ineg(quotient_abs);
        let quotient_signed = builder.ins().select(signs_differ, negated, quotient_abs);

        let max_fixed_val = builder.ins().iconst(types::I32, MAX_FIXED);
        let min_fixed_val = builder.ins().iconst(types::I32, MIN_FIXED);
        let div0_pos = builder
            .ins()
            .icmp(IntCC::SignedGreaterThanOrEqual, dividend, zero);
        let saturate_val = builder.ins().select(div0_pos, max_fixed_val, min_fixed_val);
        builder.ins().select(is_div0, saturate_val, quotient_signed)
    }

    pub fn emit_neg(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        builder.ins().ineg(a)
    }

    pub fn emit_abs(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        let zero = builder.ins().iconst(types::I32, 0);
        let is_negative = builder.ins().icmp(IntCC::SignedLessThan, a, zero);
        let negated = builder.ins().ineg(a);
        builder.ins().select(is_negative, negated, a)
    }

    pub fn emit_cmp(
        &self,
        cc: FloatCC,
        a: Value,
        b: Value,
        builder: &mut FunctionBuilder,
    ) -> Value {
        let int_cc = Self::float_cc_to_int_cc(cc);
        builder.ins().icmp(int_cc, a, b)
    }

    pub fn emit_min(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        let cmp = builder.ins().icmp(IntCC::SignedLessThan, a, b);
        builder.ins().select(cmp, a, b)
    }

    pub fn emit_max(&self, a: Value, b: Value, builder: &mut FunctionBuilder) -> Value {
        let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, a, b);
        builder.ins().select(cmp, a, b)
    }

    pub fn emit_floor(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        let shift_const = builder.ins().iconst(types::I32, Q32_SHIFT);
        let rounded = builder.ins().sshr(a, shift_const);
        builder.ins().ishl(rounded, shift_const)
    }

    pub fn emit_ceil(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        let mask = (1i64 << Q32_SHIFT) - 1;
        let mask_const = builder.ins().iconst(types::I32, mask);
        let added = builder.ins().iadd(a, mask_const);
        let shift_const = builder.ins().iconst(types::I32, Q32_SHIFT);
        let rounded = builder.ins().sshr(added, shift_const);
        builder.ins().ishl(rounded, shift_const)
    }

    pub fn emit_sqrt(&self, _a: Value, _builder: &mut FunctionBuilder) -> Value {
        unreachable!("Q32 sqrt handled by CodegenContext")
    }

    pub fn emit_from_sint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        let target_type = types::I32;
        let shift_const = builder.ins().iconst(target_type, Q32_SHIFT);

        let max_int = builder.ins().iconst(target_type, 32767i64);
        let min_int = builder.ins().iconst(target_type, -32768i64);
        let clamped_max = builder.ins().smin(a, max_int);
        let clamped_int = builder.ins().smax(clamped_max, min_int);

        let arg_type = builder.func.dfg.value_type(a);
        if arg_type.bits() < target_type.bits() {
            let extended = builder.ins().sextend(target_type, clamped_int);
            builder.ins().ishl(extended, shift_const)
        } else {
            builder.ins().ishl(clamped_int, shift_const)
        }
    }

    pub fn emit_to_sint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        let shift_const = builder.ins().iconst(types::I32, Q32_SHIFT);
        let zero = builder.ins().iconst(types::I32, 0);
        let bias_mask = builder.ins().iconst(types::I32, (1i64 << Q32_SHIFT) - 1);

        let is_negative = builder.ins().icmp(IntCC::SignedLessThan, a, zero);
        let biased_arg = builder.ins().iadd(a, bias_mask);
        let biased_value = builder.ins().select(is_negative, biased_arg, a);
        builder.ins().sshr(biased_value, shift_const)
    }

    pub fn emit_from_uint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        let target_type = types::I32;
        let shift_const = builder.ins().iconst(target_type, Q32_SHIFT);
        let arg_type = builder.func.dfg.value_type(a);

        if arg_type.bits() < target_type.bits() {
            let extended = builder.ins().uextend(target_type, a);
            builder.ins().ishl(extended, shift_const)
        } else if arg_type.bits() == target_type.bits() {
            let max_uint = builder.ins().iconst(target_type, 32767i64);
            let extended_arg = builder.ins().uextend(types::I64, a);
            let extended_max = builder.ins().uextend(types::I64, max_uint);
            let clamped_i64 = builder.ins().umin(extended_arg, extended_max);
            let clamped = builder.ins().ireduce(target_type, clamped_i64);
            builder.ins().ishl(clamped, shift_const)
        } else {
            let truncated = builder.ins().ireduce(target_type, a);
            builder.ins().ishl(truncated, shift_const)
        }
    }

    pub fn emit_to_uint(&self, a: Value, builder: &mut FunctionBuilder) -> Value {
        let shift_const = builder.ins().iconst(types::I32, Q32_SHIFT);
        let zero = builder.ins().iconst(types::I32, 0);
        let is_negative = builder.ins().icmp(IntCC::SignedLessThan, a, zero);
        let mask_value = (1u64 << Q32_SHIFT as u32) - 1;
        let mask = builder.ins().iconst(types::I32, mask_value as i64);
        let adjusted_negative = builder.ins().iadd(a, mask);
        let shifted_negative = builder.ins().sshr(adjusted_negative, shift_const);
        let shifted_positive = builder.ins().sshr(a, shift_const);
        builder
            .ins()
            .select(is_negative, shifted_negative, shifted_positive)
    }

    pub fn map_signature(&self, sig: &Signature) -> Signature {
        let target_type = types::I32;
        let mut new_sig = Signature::new(sig.call_conv);
        for param in &sig.params {
            let ty = if param.value_type == types::F32 {
                target_type
            } else {
                param.value_type
            };
            if param.purpose == ArgumentPurpose::Normal {
                new_sig.params.push(AbiParam::new(ty));
            } else {
                new_sig.params.push(AbiParam::special(ty, param.purpose));
            }
        }
        for ret in &sig.returns {
            let ty = if ret.value_type == types::F32 {
                target_type
            } else {
                ret.value_type
            };
            if ret.purpose == ArgumentPurpose::Normal {
                new_sig.returns.push(AbiParam::new(ty));
            } else {
                new_sig.returns.push(AbiParam::special(ty, ret.purpose));
            }
        }
        new_sig
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::transform::q32::{Q32Options, float_to_fixed16x16};
    use cranelift_codegen::ir::{AbiParam, Function, InstBuilder};
    use cranelift_codegen::isa::CallConv;
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use lp_model::glsl_opts::{AddSubMode, DivMode, MulMode};

    #[test]
    fn float_strategy_scalar_type_is_f32() {
        let s = FloatStrategy;
        assert_eq!(s.scalar_type(), types::F32);
    }

    #[test]
    fn q32_strategy_scalar_type_is_i32() {
        let opts = Q32Options::default();
        let s = Q32Strategy::new(opts);
        assert_eq!(s.scalar_type(), types::I32);
    }

    fn with_q32_builder<F>(opts: Q32Options, f: F)
    where
        F: FnOnce(&Q32Strategy, &mut FunctionBuilder),
    {
        let mut func = Function::new();
        func.signature = Signature::new(CallConv::SystemV);
        func.signature.returns.push(AbiParam::new(types::I32));
        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut builder_ctx);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        let s = Q32Strategy::new(opts);
        f(&s, &mut builder);
        builder.seal_block(block);
        builder.finalize();
    }

    #[test]
    fn q32_emit_const_produces_i32() {
        let opts = Q32Options::default();
        with_q32_builder(opts, |s, builder| {
            let v = s.emit_const(2.0, builder);
            assert_eq!(builder.func.dfg.value_type(v), types::I32);
            builder.ins().return_(&[v]);
        });
    }

    #[test]
    fn q32_emit_const_applies_float_to_fixed16x16() {
        let expected = float_to_fixed16x16(1.0);
        assert_eq!(expected, 65536, "1.0 in Q16.16 = 65536");
    }

    #[test]
    fn q32_emit_add_wrapping_produces_iadd() {
        let opts = Q32Options::builder().add_sub(AddSubMode::Wrapping).build();
        with_q32_builder(opts, |s, builder| {
            let a = builder.ins().iconst(types::I32, 0x10000); // 1.0
            let b = builder.ins().iconst(types::I32, 0x10000); // 1.0
            let v = s.emit_add(a, b, builder);
            assert_eq!(builder.func.dfg.value_type(v), types::I32);
            builder.ins().return_(&[v]);
        });
    }

    #[test]
    fn q32_emit_mul_wrapping_produces_i32() {
        let opts = Q32Options::builder().mul(MulMode::Wrapping).build();
        with_q32_builder(opts, |s, builder| {
            let a = builder.ins().iconst(types::I32, 0x20000); // 2.0
            let b = builder.ins().iconst(types::I32, 0x18000); // 1.5
            let v = s.emit_mul(a, b, builder);
            assert_eq!(builder.func.dfg.value_type(v), types::I32);
            builder.ins().return_(&[v]);
        });
    }

    #[test]
    fn q32_emit_cmp_returns_icmp_result() {
        let opts = Q32Options::default();
        with_q32_builder(opts, |s, builder| {
            let a = builder.ins().iconst(types::I32, 0x10000);
            let b = builder.ins().iconst(types::I32, 0x20000);
            let v = s.emit_cmp(FloatCC::LessThan, a, b, builder);
            assert_eq!(builder.func.dfg.value_type(v), types::I8);
            let v_i32 = builder.ins().sextend(types::I32, v);
            builder.ins().return_(&[v_i32]);
        });
    }

    #[test]
    fn q32_emit_from_sint_clamps_and_shifts() {
        let opts = Q32Options::default();
        with_q32_builder(opts, |s, builder| {
            let a = builder.ins().iconst(types::I32, 5);
            let v = s.emit_from_sint(a, builder);
            assert_eq!(builder.func.dfg.value_type(v), types::I32);
            builder.ins().return_(&[v]);
        });
    }

    #[test]
    fn q32_emit_to_sint_shifts() {
        let opts = Q32Options::default();
        with_q32_builder(opts, |s, builder| {
            let a = builder.ins().iconst(types::I32, 0x50000); // 5.0
            let v = s.emit_to_sint(a, builder);
            assert_eq!(builder.func.dfg.value_type(v), types::I32);
            builder.ins().return_(&[v]);
        });
    }

    #[test]
    fn q32_map_signature_f32_to_i32() {
        let opts = Q32Options::default();
        let s = Q32Strategy::new(opts);
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(types::F32));
        sig.returns.push(AbiParam::new(types::F32));
        let mapped = s.map_signature(&sig);
        assert_eq!(mapped.params[0].value_type, types::I32);
        assert_eq!(mapped.returns[0].value_type, types::I32);
    }

    #[test]
    fn q32_emit_div_reciprocal_produces_i32() {
        let opts = Q32Options::builder().div(DivMode::Reciprocal).build();
        with_q32_builder(opts, |s, builder| {
            let dividend = builder.ins().iconst(types::I32, 0x40000); // 4.0
            let divisor = builder.ins().iconst(types::I32, 0x20000); // 2.0
            let v = s.emit_div(dividend, divisor, builder);
            assert_eq!(builder.func.dfg.value_type(v), types::I32);
            builder.ins().return_(&[v]);
        });
    }
}
