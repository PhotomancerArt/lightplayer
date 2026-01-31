//! Identity transform implementation

use crate::backend::transform::pipeline::{Transform, TransformContext};
use crate::backend::transform::shared::{copy_instruction, transform_function_body};
use crate::error::GlslError;
use alloc::{format, vec::Vec};
use cranelift_codegen::ir::{Function, InstBuilder, Signature};

/// Identity transform - copies functions exactly without modification
pub struct IdentityTransform;

impl Transform for IdentityTransform {
    fn transform_signature(&self, sig: &Signature) -> Signature {
        sig.clone()
    }

    fn transform_function<M: cranelift_module::Module>(
        &self,
        old_func: &Function,
        ctx: &mut TransformContext<'_, M>,
    ) -> Result<Function, GlslError> {
        // Get transformed signature
        let new_sig = self.transform_signature(&old_func.signature);

        // Capture func_id_map and old_func_id_map for FuncId remapping
        let func_id_map = ctx.func_id_map.clone();
        let old_func_id_map = ctx.old_func_id_map.clone();

        transform_function_body(
            old_func,
            new_sig,
            // Instruction transformation: copy instructions exactly, but remap FuncIds
            move |old_func, old_inst, builder, value_map, stack_slot_map, block_map| {
                // Handle Call instructions specially to remap FuncIds
                use cranelift_codegen::ir::{ExtFuncData, InstructionData};
                let inst_data = &old_func.dfg.insts[old_inst];
                if let InstructionData::Call { func_ref, args, .. } = inst_data {
                    let old_args = args.as_slice(&old_func.dfg.value_lists);
                    let new_args: Vec<_> = old_args
                        .iter()
                        .map(|&v| {
                            value_map.get(&v).copied().ok_or_else(|| {
                                GlslError::new(
                                    crate::error::ErrorCode::E0301,
                                    format!("Value {v:?} not found in value_map"),
                                )
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;

                    let old_ext_func = &old_func.dfg.ext_funcs[*func_ref];
                    let old_sig_ref = old_ext_func.signature;
                    let old_sig = &old_func.dfg.signatures[old_sig_ref];
                    let new_sig_ref = builder.func.import_signature(old_sig.clone());

                    // Remap FuncId for User external names, and convert TestCase to User
                    use cranelift_codegen::ir::ExternalName;
                    let new_name = match &old_ext_func.name {
                        ExternalName::TestCase(testcase_name) => {
                            // Convert TestCase name to User name for ObjectModule compatibility
                            // ObjectModule doesn't support TestCase names in relocations (unimplemented!)
                            let func_name_str =
                                core::str::from_utf8(testcase_name.raw()).map_err(|e| {
                                    GlslError::new(
                                        crate::error::ErrorCode::E0301,
                                        format!("Invalid TestCase name encoding: {e}"),
                                    )
                                })?;
                            // Look up the new FuncId for this function name
                            let new_func_id = func_id_map.get(func_name_str).ok_or_else(|| {
                                GlslError::new(
                                    crate::error::ErrorCode::E0301,
                                    format!("Function '{func_name_str}' not found in func_id_map"),
                                )
                            })?;
                            // Create UserExternalName with the new FuncId
                            let new_user_name = cranelift_codegen::ir::UserExternalName {
                                namespace: 0,
                                index: new_func_id.as_u32(),
                            };
                            let new_user_ref =
                                builder.func.declare_imported_user_function(new_user_name);
                            ExternalName::User(new_user_ref)
                        }
                        ExternalName::User(old_user_ref) => {
                            let user_name = old_func
                                .params
                                .user_named_funcs()
                                .get(*old_user_ref)
                                .ok_or_else(|| {
                                    GlslError::new(
                                        crate::error::ErrorCode::E0301,
                                        format!(
                                            "UserExternalNameRef {old_user_ref} not found in function's user_named_funcs"
                                        ),
                                    )
                                })?;
                            // Map old FuncId -> function name -> new FuncId
                            let old_func_id = cranelift_module::FuncId::from_u32(user_name.index);
                            if let Some(func_name) = old_func_id_map.get(&old_func_id) {
                                if let Some(new_func_id) = func_id_map.get(func_name) {
                                    let new_user_name = cranelift_codegen::ir::UserExternalName {
                                        namespace: user_name.namespace,
                                        index: new_func_id.as_u32(),
                                    };
                                    let new_user_ref =
                                        builder.func.declare_imported_user_function(new_user_name);
                                    ExternalName::User(new_user_ref)
                                } else {
                                    // Fallback: use original if mapping not found
                                    let new_user_ref = builder
                                        .func
                                        .declare_imported_user_function(user_name.clone());
                                    ExternalName::User(new_user_ref)
                                }
                            } else {
                                // Fallback: use original if mapping not found
                                let new_user_ref = builder
                                    .func
                                    .declare_imported_user_function(user_name.clone());
                                ExternalName::User(new_user_ref)
                            }
                        }
                        _ => old_ext_func.name.clone(),
                    };

                    let new_ext_func = ExtFuncData {
                        name: new_name,
                        signature: new_sig_ref,
                        colocated: old_ext_func.colocated,
                    };
                    let new_func_ref = builder.func.import_function(new_ext_func);
                    let call_inst = builder.ins().call(new_func_ref, &new_args);
                    let old_results: Vec<_> = old_func.dfg.inst_results(old_inst).to_vec();
                    let new_results = builder.inst_results(call_inst);
                    for (old_result, new_result) in old_results.iter().zip(new_results.iter()) {
                        value_map.insert(*old_result, *new_result);
                    }
                    Ok(())
                } else {
                    // For non-call instructions, use copy_instruction
                    copy_instruction(
                        old_func,
                        old_inst,
                        builder,
                        value_map,
                        stack_slot_map,
                        block_map,
                        None,  // func_ref_map not used
                        |t| t, // Identity type mapping
                    )
                }
            },
            // Type mapping: identity (no conversion)
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::transform::shared::transform_test_util;

    #[test]
    #[cfg(feature = "std")]
    fn test_identity_transform_simple() {
        transform_test_util::assert_identity_transform(
            "Identity transform should produce identical CLIF",
            r#"
function %add(i32, i32) -> i32 system_v {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}
"#,
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_identity_transform_block_order() {
        transform_test_util::assert_identity_transform(
            "Identity transform should preserve block order",
            r#"
function %test(i32) -> i32 system_v {
block0(v0: i32):
    jump block1

block1:
    jump block2

block2:
    return v0
}
"#,
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_identity_transform_block_params() {
        transform_test_util::assert_identity_transform(
            "Identity transform should preserve block parameters",
            r#"
function %test(i32) -> i32 system_v {
block0(v0: i32):
    jump block1(v0)

block1(v1: i32):
    return v1
}
"#,
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_identity_transform_stack_slots() {
        transform_test_util::assert_identity_transform(
            "Identity transform should preserve stack slots",
            r#"
function %test(i32) -> i32 system_v {
ss0 = explicit_slot 4, align = 4
block0(v0: i32):
    return v0
}
"#,
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_identity_transform_multi_function() {
        // Test with multiple functions in a single module
        transform_test_util::assert_identity_transform(
            "Identity transform should preserve multiple functions",
            r#"
function %add(i32, i32) -> i32 system_v {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}

function %multiply(i32, i32) -> i32 system_v {
block0(v0: i32, v1: i32):
    v2 = imul v0, v1
    return v2
}
"#,
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_identity_transform_function_calls() {
        // Test that function calls are preserved correctly through module transformation
        transform_test_util::assert_identity_transform(
            "Identity transform should preserve function calls",
            r#"
function %helper(i32) -> i32 system_v {
block0(v0: i32):
    v1 = iconst.i32 1
    v2 = iadd v0, v1
    return v2
}

function %main(i32) -> i32 system_v {
    sig0 = (i32) -> i32 system_v
    fn0 = colocated %helper sig0

block0(v0: i32):
    v1 = call fn0(v0)
    return v1
}
"#,
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_complex_clif() {
        // Test with multiple functions - parse each separately
        transform_test_util::assert_identity_transform(
            "Identity transform should preserve add function",
            r#"
function %test_continue_do_while_loop_after_first() -> i32 system_v {
block0:
    v0 = iconst.i32 0
    v1 = iconst.i32 0
    jump block1(v0, v1)  ; v0 = 0, v1 = 0

block1(v2: i32, v3: i32):
    v4 = iadd v2, v3
    v5 = iconst.i32 1
    v6 = iadd v3, v5  ; v5 = 1
    v7 = iconst.i32 2
    v8 = icmp sge v6, v7  ; v7 = 2
    v9 = iconst.i8 1
    v10 = iconst.i8 0
    v11 = select v8, v9, v10  ; v9 = 1, v10 = 0
    brif v11, block4, block5(v6, v4)

block4:
    jump block2(v6, v4)

block6:
    v16 = iconst.i32 0
    v17 = iconst.i32 0
    jump block5(v17, v16)  ; v17 = 0, v16 = 0

block5(v12: i32, v13: i32):
    jump block2(v12, v13)

block2(v14: i32, v15: i32):
    v18 = iconst.i32 5
    v19 = icmp slt v14, v18  ; v18 = 5
    v20 = iconst.i8 1
    v21 = iconst.i8 0
    v22 = select v19, v20, v21  ; v20 = 1, v21 = 0
    brif v22, block1(v15, v14), block3

block3:
    return v15

block7:
    v23 = iconst.i32 0
    return v23  ; v23 = 0
}
"#,
        );
    }
}
