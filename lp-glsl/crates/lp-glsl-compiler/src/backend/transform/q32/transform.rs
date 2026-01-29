//! Q32 transform implementation for backend2

use crate::backend::transform::pipeline::{Transform, TransformContext};
use crate::backend::transform::q32::instructions::convert_all_instructions;
use crate::backend::transform::q32::signature::convert_signature;
use crate::backend::transform::q32::types::FixedPointFormat;
use crate::backend::transform::shared::transform_function_body;
use crate::error::GlslError;
use cranelift_codegen::ir::{Function, Signature};
use cranelift_module::Module;

/// Q32 transform - converts F32 to fixed-point representation
pub struct Q32Transform {
    format: FixedPointFormat,
}

impl Q32Transform {
    /// Create a new Q32 transform with the specified format
    pub fn new(format: FixedPointFormat) -> Self {
        Self { format }
    }

    /// Create a Q32 transform with default format (Fixed16x16)
    pub fn default() -> Self {
        Self::new(FixedPointFormat::Fixed16x16)
    }
}

impl Transform for Q32Transform {
    fn transform_signature(&self, sig: &Signature) -> Signature {
        convert_signature(sig, self.format)
    }

    fn transform_function<M: Module>(
        &self,
        old_func: &Function,
        ctx: &mut TransformContext<'_, M>,
    ) -> Result<Function, GlslError> {
        // 1. Convert signature (happens before transform_function_body)
        let new_sig = convert_signature(&old_func.signature, self.format);
        let format = self.format;

        // 2. Get pointer type from module ISA (needed for builtin signatures)
        let pointer_type = ctx.module.module_internal().isa().pointer_type();

        // 3. Capture func_id_map and old_func_id_map from context
        let func_id_map = ctx.func_id_map.clone();
        let old_func_id_map = ctx.old_func_id_map.clone();
        // We need module access for declare_func_in_func, but we can't capture mutable references
        // So we'll pass the module through a different mechanism - store it in a way that can be accessed
        // For now, we'll handle colocated functions differently in the converter

        // 4. Create call conversion state for FuncRef/SigRef mapping
        use crate::backend::transform::q32::instructions::CallConversionState;
        use core::cell::RefCell;
        let call_state = RefCell::new(CallConversionState::new());

        // 5. Use shared transform_function_body with instruction converter
        transform_function_body(
            old_func,
            new_sig,
            // Instruction transformation callback
            move |old_func, old_inst, builder, value_map, stack_slot_map, block_map| {
                convert_all_instructions(
                    old_func,
                    old_inst,
                    builder,
                    value_map,
                    format,
                    block_map,
                    stack_slot_map,
                    &mut *call_state.borrow_mut(),
                    &func_id_map,
                    &old_func_id_map,
                    pointer_type,
                )
            },
            // Type mapping callback for block parameters
        )
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::*;
    use cranelift_codegen::ir::{AbiParam, Signature, types};
    use cranelift_codegen::isa::CallConv;

    /// Test signature conversion: F32 params → I32 params
    #[test]
    fn test_q32_signature_conversion() {
        let transform = Q32Transform::default();

        // Test F32 param conversion
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(types::F32));
        sig.returns.push(AbiParam::new(types::F32));

        let transformed = transform.transform_signature(&sig);

        assert_eq!(transformed.params.len(), 1);
        assert_eq!(transformed.params[0].value_type, types::I32);
        assert_eq!(transformed.returns.len(), 1);
        assert_eq!(transformed.returns[0].value_type, types::I32);
    }

    /// Test signature conversion: mixed F32 and I32 params
    #[test]
    fn test_q32_signature_mixed() {
        let transform = Q32Transform::default();

        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(types::F32));
        sig.params.push(AbiParam::new(types::I32));
        sig.returns.push(AbiParam::new(types::F32));

        let transformed = transform.transform_signature(&sig);

        assert_eq!(transformed.params.len(), 2);
        assert_eq!(transformed.params[0].value_type, types::I32); // F32 → I32
        assert_eq!(transformed.params[1].value_type, types::I32); // I32 unchanged
        assert_eq!(transformed.returns.len(), 1);
        assert_eq!(transformed.returns[0].value_type, types::I32); // F32 → I32
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_call_clif() {
        // Test with multiple functions - parse each separately
        // Q32 transform should preserve function calls correctly for integer-only code
        use crate::backend::transform::shared::transform_test_util;
        transform_test_util::assert_nop_q32_transform(
            "Q32 transform should preserve function calls for integer-only code",
            r#"
function %test_int_add_positive_positive() -> i32 system_v {
block0:
    v0 = iconst.i32 5
    v1 = iconst.i32 3
    v2 = iadd v0, v1  ; v0 = 5, v1 = 3
    return v2

block1:
    v3 = iconst.i32 0
    return v3  ; v3 = 0
}

function %main() -> i32 system_v {
    sig0 = () -> i32 system_v
    fn0 = colocated %test_int_add_positive_positive sig0

block0:
    v0 = call fn0()
    return v0

block1:
    v1 = iconst.i32 0
    return v1  ; v1 = 0
}
"#,
        );
    }

    #[test]
    #[cfg(feature = "std")]
    #[cfg(feature = "emulator")]
    fn test_do_while() {
        // Test do-while loop with continue - should return 10
        // Iterations: i=0 (sum=0), i=1 (sum=1), i=2 (sum=3, continue), i=3 (sum=6, continue), i=4 (sum=10, continue), i=5 (exit)
        use crate::backend::transform::shared::transform_test_util::run_int32_test;
        run_int32_test(
            r#"
int test_continue_do_while_loop_after_first() {
    int sum = 0;
    int i = 0;
    do {
        sum = sum + i;
        i = i + 1;
        if (i >= 2) {
            continue;
        }
    } while (i < 5);

    return sum;
}

int main() {
    return test_continue_do_while_loop_after_first();
}
"#,
            10, // Expected result: 0+1+2+3+4 = 10
        );
    }
}
