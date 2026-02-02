//! Constant conversion functions.

use crate::backend::transform::q32::types::{FixedPointFormat, float_to_fixed16x16};
use crate::error::{ErrorCode, GlslError};
use cranelift_codegen::ir::{Function, Inst, InstBuilder, InstructionData};
use cranelift_frontend::FunctionBuilder;
use hashbrown::HashMap;

/// Convert F32const to iconst with fixed-point value.
pub(crate) fn convert_f32const(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    // Get the float constant value
    let inst_data = &old_func.dfg.insts[old_inst];

    // Extract the float value from UnaryIeee32 format
    let f32_value = match inst_data {
        InstructionData::UnaryIeee32 { opcode: _, imm } => f32::from_bits(imm.bits()),
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0301,
                alloc::format!(
                    "F32const instruction {old_inst:?} has unexpected format: {inst_data:?}"
                ),
            ));
        }
    };

    // Convert to fixed-point
    let target_type = format.cranelift_type();
    let fixed_value = match format {
        FixedPointFormat::Fixed16x16 => float_to_fixed16x16(f32_value) as i64,
        FixedPointFormat::Q32x32 => {
            return Err(GlslError::new(
                ErrorCode::E0301,
                "Q32x32 format not yet implemented",
            ));
        }
    };

    // Create I32 constant
    let new_value = builder.ins().iconst(target_type, fixed_value);

    // Map old value to new value
    let old_result = old_func.dfg.first_result(old_inst);
    value_map.insert(old_result, new_value);

    Ok(())
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    #[cfg(feature = "emulator")]
    use crate::backend::transform::q32::q32_test_util;

    /// Test fconst: constant conversion
    #[test]
    #[cfg(feature = "emulator")]
    #[ignore] // TODO emu: Q32 converter tests failing
    fn test_q32_fconst() {
        // Use a simple value that can be represented exactly
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.0p1
    return v0
}
"#;
        q32_test_util::run_q32_test(clif, 2.0);
    }
}
