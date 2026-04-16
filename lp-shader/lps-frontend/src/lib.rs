//! Naga-based GLSL parsing for the lps stack. Wraps `naga::front::glsl` and exposes
//! [`NagaModule`] with per-function metadata for WASM codegen and test dispatch.

#![no_std]

extern crate alloc;

pub use naga;

pub mod lower;
mod lower_access;
mod lower_array;
mod lower_array_multidim;
mod lower_binary;
mod lower_cast;
mod lower_ctx;
mod lower_error;
mod lower_expr;
mod lower_lpfn;
mod lower_math;
mod lower_math_geom;
mod lower_math_helpers;
mod lower_matrix;
mod lower_stmt;
mod lower_unary;
mod naga_types;
mod naga_util;
mod parse;
pub mod std_math_handler;

pub use lower::lower;
pub use lower_error::LowerError;

pub use lps_shared::{FnParam, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};

/// Back-compat alias; prefer [`ParamQualifier`].
pub type GlslParamQualifier = ParamQualifier;
/// Back-compat alias for a single formal parameter; prefer [`FnParam`].
pub type LpsSig = FnParam;

pub use naga_types::{CompileError, FunctionInfo, NagaModule, naga_module_from_parsed};
pub use parse::{compile, prepared_glsl_for_compile, user_snippet_first_physical_line};

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::std_math_handler::StdMathHandler;
    use lpir::{ImportHandler, InterpError, Value};

    #[test]
    fn parse_float_add() {
        let src = "float add(float a, float b) { return a + b; }";
        let result = compile(src).unwrap();
        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.functions[0].1.name, "add");
        assert_eq!(result.functions[0].1.return_type, LpsType::Float);
        assert_eq!(result.functions[0].1.params.len(), 2);
        assert_eq!(
            result.functions[0].1.params[0].qualifier,
            ParamQualifier::In
        );
    }

    #[test]
    fn parse_int_function() {
        let src = "int negate(int x) { return -x; }";
        let result = compile(src).unwrap();
        assert_eq!(result.functions[0].1.return_type, LpsType::Int);
    }

    #[test]
    fn parse_void_function() {
        let src = "void do_nothing() { }";
        let result = compile(src).unwrap();
        assert_eq!(result.functions[0].1.return_type, LpsType::Void);
    }

    #[test]
    fn parse_multiple_functions() {
        let src = r#"
            float foo() { return 1.0; }
            float bar() { return 2.0; }
        "#;
        let result = compile(src).unwrap();
        assert_eq!(result.functions.len(), 2);
        let names: Vec<_> = result
            .functions
            .iter()
            .map(|(_, i)| i.name.as_str())
            .collect();
        assert!(names.contains(&"foo") && names.contains(&"bar"));
    }

    #[test]
    fn lower_error_in_function_display_names_function() {
        let e = LowerError::InFunction {
            name: String::from("my_fn"),
            inner: Box::new(LowerError::Internal(String::from("detail"))),
        };
        let s = alloc::format!("{e}");
        assert!(s.contains("my_fn"), "{s}");
        assert!(s.contains("detail"), "{s}");
    }

    #[test]
    fn prepared_source_user_line_alignment() {
        let first = super::user_snippet_first_physical_line();
        let prep = super::prepared_glsl_for_compile("//marker\n");
        let phys = prep
            .lines()
            .enumerate()
            .find(|(_, l)| l.contains("//marker"))
            .map(|(i, _)| i + 1)
            .expect("marker line");
        assert_eq!(phys, first);
    }

    #[test]
    fn float_main_does_not_get_duplicate_void_main_suffix() {
        let src = "float main() { return 1.0; }\n";
        let prep = super::prepared_glsl_for_compile(src);
        assert!(!prep.contains("void main()"));
    }

    #[test]
    fn naga_module_accessible() {
        let src = "float f() { return 1.0; }";
        let result = compile(src).unwrap();
        assert!(result.module.functions.len() >= 1);
    }

    #[test]
    fn lower_produces_ir_functions() {
        let src = "float add(float a, float b) { return a + b; }";
        let naga = compile(src).unwrap();
        let (ir, _) = super::lower(&naga).expect("lower");
        assert_eq!(ir.functions.len(), 1);
        let add = ir.functions.values().find(|f| f.name == "add").expect("add fn");
        assert_eq!(add.param_count, 2);
    }

    #[test]
    fn parse_local_const_array_size() {
        // Test that naga can parse local const variables used as array sizes
        let src = r#"
float test() {
    const int SIZE = 4;
    float arr[SIZE];
    return arr[0];
}
"#;
        // This test documents current naga behavior
        match compile(src) {
            Ok(_) => {
                // If this passes, local const array sizes work
            }
            Err(e) => {
                // Currently naga doesn't support local const as array sizes
                // This is a known limitation
                let err_str = alloc::format!("{e}");
                assert!(
                    err_str.contains("Unknown variable") || err_str.contains("SIZE"),
                    "Expected error about unknown variable SIZE, got: {err_str}"
                );
            }
        }
    }

    #[test]
    fn lowered_module_validates() {
        let src = "float add(float a, float b) { return a + b; }";
        let naga = compile(src).unwrap();
        let (ir, _) = super::lower(&naga).expect("lower");
        lpir::validate_module(&ir).expect("validate lowered IR");
    }

    #[test]
    fn lower_get_fuel_validates() {
        // Test that __lp_get_fuel lowering produces valid IR with VMContext import
        let src = "int f() { return int(__lp_get_fuel()); }";
        let naga = compile(src).unwrap();
        let (ir, _) = super::lower(&naga).expect("lower");
        // Should have 1 user function and 1 import (@vm::__lp_get_fuel)
        assert_eq!(ir.functions.len(), 1);
        assert!(
            ir.imports.iter().any(|i| i.func_name == "__lp_get_fuel"),
            "missing __lp_get_fuel import"
        );
        assert!(
            ir.imports
                .iter()
                .any(|i| i.func_name == "__lp_get_fuel" && i.needs_vmctx),
            "__lp_get_fuel should need_vmctx"
        );
        lpir::validate_module(&ir).expect("validate");
    }

    #[test]
    fn lower_void_implicit_return_validates() {
        let src = "void f() { }";
        let naga = compile(src).unwrap();
        let (ir, _) = super::lower(&naga).expect("lower");
        lpir::validate_module(&ir).expect("validate");
    }

    #[test]
    fn lower_inout_float_modify() {
        let src = r#"
void modify_inout(inout float value) {
    value = value + 10.0;
}
float test_main() {
    float x = 5.0;
    modify_inout(x);
    return x;
}
"#;
        let naga = compile(src).unwrap();
        let _ = super::lower(&naga).expect("lower inout");
    }

    #[test]
    fn lower_sin_validates_with_imports() {
        let src = "float f(float x) { return sin(x); }";
        let naga = compile(src).unwrap();
        let (ir, _) = super::lower(&naga).expect("lower");
        lpir::validate_module(&ir).expect("validate");
        assert!(!ir.imports.is_empty());
    }

    #[test]
    fn interp_sin_std_math() {
        use lpir::{Value, interpret};
        let src = "float f(float x) { return sin(x); }";
        let naga = compile(src).unwrap();
        let (ir, _) = super::lower(&naga).expect("lower");
        let mut h: StdMathHandler = Default::default();
        let out = interpret(&ir, "f", &[Value::F32(0.0)], &mut h).expect("interp");
        assert!(out[0].as_f32().unwrap().abs() < 1e-5);
    }

    #[test]
    fn interp_nested_user_call() {
        use lpir::{Value, interpret};
        let src = "float g(float x) { return x + 1.0; } float f(float x) { return g(x); }";
        let naga = compile(src).unwrap();
        let (ir, _) = super::lower(&naga).expect("lower");
        let mut h: StdMathHandler = Default::default();
        let out = interpret(&ir, "f", &[Value::F32(2.0)], &mut h).expect("interp");
        assert!((out[0].as_f32().unwrap() - 3.0).abs() < 1e-4);
    }

    #[test]
    fn lower_lpfn_saturate_validates_and_interps() {
        use lpir::interpret;
        let src = "float f(float x) { return lpfn_saturate(x); }";
        let naga = compile(src).unwrap();
        let (ir, _) = super::lower(&naga).expect("lower");
        lpir::validate_module(&ir).expect("validate");
        let mut imp = TestImports::default();
        let out = interpret(&ir, "f", &[Value::F32(1.5)], &mut imp).expect("interp");
        assert!((out[0].as_f32().unwrap() - 1.0).abs() < 1e-5);
    }

    #[derive(Default)]
    struct TestImports {
        math: StdMathHandler,
    }

    impl ImportHandler for TestImports {
        fn call(
            &mut self,
            module_name: &str,
            func_name: &str,
            args: &[Value],
        ) -> Result<Vec<Value>, InterpError> {
            if module_name == "lpfn" && func_name.starts_with("lpfn_saturate_") {
                let x = args[0]
                    .as_f32()
                    .ok_or_else(|| InterpError::Import(String::from("expected f32")))?;
                return Ok(vec![Value::F32(x.max(0.0).min(1.0))]);
            }
            self.math.call(module_name, func_name, args)
        }
    }

    // ========================
    // Globals & Uniforms Tests
    // ========================

    #[test]
    fn lower_global_read_emits_load_from_vmctx() {
        // Global variable (private address space) - simple read
        let src = "float my_global; float test() { return my_global; }";
        let naga = compile(src).unwrap();
        let (ir, sig) = super::lower(&naga).expect("lower");

        // Verify globals_type is set
        assert!(sig.globals_type.is_some(), "globals_type should be set");
        assert!(sig.uniforms_type.is_none(), "uniforms_type should be None");

        // Verify the test function contains Load ops with VMCTX base
        let test_func = ir
            .functions
            .values()
            .find(|f| f.name == "test")
            .expect("test function");
        let has_load_from_vmctx = test_func.body.iter().any(|op| {
            matches!(
                op,
                lpir::LpirOp::Load {
                    base: lpir::VReg(0),
                    ..
                }
            )
        });
        assert!(
            has_load_from_vmctx,
            "test function should contain Load from VMCTX"
        );

        lpir::validate_module(&ir).expect("validate");
    }

    #[test]
    fn lower_global_with_vec3_emits_multiple_loads() {
        // Global vec3 should emit 3 Load ops
        let src = "vec3 my_vec; vec3 test() { return my_vec; }";
        let naga = compile(src).unwrap();
        let (ir, sig) = super::lower(&naga).expect("lower");

        // Verify globals_type is set and contains a Vec3
        assert!(sig.globals_type.is_some(), "globals_type should be set");
        if let Some(LpsType::Struct { members, .. }) = &sig.globals_type {
            assert_eq!(members.len(), 1);
            assert_eq!(members[0].ty, LpsType::Vec3);
        } else {
            panic!("globals_type should be a Struct");
        }

        // Verify the test function contains multiple Load ops
        let test_func = ir
            .functions
            .values()
            .find(|f| f.name == "test")
            .expect("test function");
        let load_count = test_func
            .body
            .iter()
            .filter(|op| {
                matches!(
                    op,
                    lpir::LpirOp::Load {
                        base: lpir::VReg(0),
                        ..
                    }
                )
            })
            .count();
        assert_eq!(load_count, 3, "vec3 read should emit 3 Load ops from VMCTX");

        lpir::validate_module(&ir).expect("validate");
    }

    #[test]
    fn lower_global_with_initializer_synthesizes_shader_init() {
        let src = "float my_global = 42.0; float test() { return my_global; }";
        let naga = compile(src).unwrap();
        let (ir, sig) = super::lower(&naga).expect("lower");

        // Verify globals_type is set
        assert!(sig.globals_type.is_some(), "globals_type should be set");

        // Verify __shader_init function exists
        let init_func = ir.functions.values().find(|f| f.name == "__shader_init");
        assert!(
            init_func.is_some(),
            "__shader_init function should be synthesized"
        );

        // Verify __shader_init contains Store ops
        let init_func = init_func.unwrap();
        let has_store_to_vmctx = init_func.body.iter().any(|op| {
            matches!(
                op,
                lpir::LpirOp::Store {
                    base: lpir::VReg(0),
                    ..
                }
            )
        });
        assert!(
            has_store_to_vmctx,
            "__shader_init should contain Store to VMCTX"
        );

        lpir::validate_module(&ir).expect("validate");
    }

    #[test]
    fn lower_global_write_emits_store_to_vmctx() {
        let src = "float my_global; void test() { my_global = 3.14; }";
        let naga = compile(src).unwrap();
        let (ir, _sig) = super::lower(&naga).expect("lower");

        // Verify the test function contains Store ops with VMCTX base
        let test_func = ir
            .functions
            .values()
            .find(|f| f.name == "test")
            .expect("test function");
        let has_store_to_vmctx = test_func.body.iter().any(|op| {
            matches!(
                op,
                lpir::LpirOp::Store {
                    base: lpir::VReg(0),
                    ..
                }
            )
        });
        assert!(
            has_store_to_vmctx,
            "test function should contain Store to VMCTX"
        );

        lpir::validate_module(&ir).expect("validate");
    }

    #[test]
    fn lower_global_vec3_write_emits_multiple_stores() {
        let src = "vec3 my_vec; void test() { my_vec = vec3(1.0, 2.0, 3.0); }";
        let naga = compile(src).unwrap();
        let (ir, _sig) = super::lower(&naga).expect("lower");

        // Verify the test function contains 3 Store ops (one per component)
        let test_func = ir
            .functions
            .values()
            .find(|f| f.name == "test")
            .expect("test function");
        let store_count = test_func
            .body
            .iter()
            .filter(|op| {
                matches!(
                    op,
                    lpir::LpirOp::Store {
                        base: lpir::VReg(0),
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            store_count, 3,
            "vec3 write should emit 3 Store ops to VMCTX"
        );

        lpir::validate_module(&ir).expect("validate");
    }

    #[test]
    fn lower_multiple_globals_sets_correct_metadata() {
        let src = "float g1; int g2; float test() { return g1 + float(g2); }";
        let naga = compile(src).unwrap();
        let (ir, sig) = super::lower(&naga).expect("lower");

        // Verify globals_type has 2 members
        assert!(sig.globals_type.is_some(), "globals_type should be set");
        if let Some(LpsType::Struct { members, .. }) = &sig.globals_type {
            assert_eq!(members.len(), 2);
            assert_eq!(members[0].ty, LpsType::Float);
            assert_eq!(members[1].ty, LpsType::Int);
        } else {
            panic!("globals_type should be a Struct");
        }

        lpir::validate_module(&ir).expect("validate");
    }

    // NOTE: Uniform block support is limited because Naga represents uniform blocks as structs,
    // which require additional type mapping work. The infrastructure for uniforms is in place
    // (uniforms_type field in LpsModuleSig), but struct-to-struct type conversion is a TODO.
    // See the spec for M1 - uniform blocks can be added as follow-up work.
}

pub use lpir::FloatMode;
