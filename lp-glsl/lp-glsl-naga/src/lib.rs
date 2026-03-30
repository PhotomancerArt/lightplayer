//! Naga-based GLSL parsing for the lp-glsl stack. Wraps `naga::front::glsl` and exposes
//! [`NagaModule`] with per-function metadata for WASM codegen and test dispatch.

#![no_std]

extern crate alloc;

// Dependency reserved for math/LPFX lowering (later phases).
use lp_glsl_builtin_ids as _;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
pub use naga;

use naga::{
    AddressSpace, Function, Handle, Module, ScalarKind, ShaderStage, TypeInner, VectorSize,
};

mod expr_scalar;
pub mod lower;
mod lower_access;
mod lower_ctx;
mod lower_error;
mod lower_expr;
mod lower_lpfx;
mod lower_math;
mod lower_matrix;
mod lower_stmt;
pub mod std_math_handler;

pub use lower::lower;
pub use lower_error::LowerError;

pub use lpir::{GlslFunctionMeta, GlslModuleMeta, GlslParamMeta, GlslParamQualifier, GlslType};

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
        assert_eq!(result.functions[0].1.return_type, GlslType::Float);
        assert_eq!(result.functions[0].1.params.len(), 2);
        assert_eq!(
            result.functions[0].1.params[0].qualifier,
            GlslParamQualifier::In
        );
    }

    #[test]
    fn parse_int_function() {
        let src = "int negate(int x) { return -x; }";
        let result = compile(src).unwrap();
        assert_eq!(result.functions[0].1.return_type, GlslType::Int);
    }

    #[test]
    fn parse_void_function() {
        let src = "void do_nothing() { }";
        let result = compile(src).unwrap();
        assert_eq!(result.functions[0].1.return_type, GlslType::Void);
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
        assert_eq!(ir.functions[0].name, "add");
        assert_eq!(ir.functions[0].param_count, 2);
    }

    #[test]
    fn lowered_module_validates() {
        let src = "float add(float a, float b) { return a + b; }";
        let naga = compile(src).unwrap();
        let (ir, _) = super::lower(&naga).expect("lower");
        lpir::validate_module(&ir).expect("validate lowered IR");
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
    fn lower_lpfx_saturate_validates_and_interps() {
        use lpir::interpret;
        let src = "float f(float x) { return lpfx_saturate(x); }";
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
            if module_name == "lpfx" && func_name.starts_with("lpfx_saturate_") {
                let x = args[0]
                    .as_f32()
                    .ok_or_else(|| InterpError::Import(String::from("expected f32")))?;
                return Ok(vec![Value::F32(x.max(0.0).min(1.0))]);
            }
            self.math.call(module_name, func_name, args)
        }
    }
}

pub use lpir::FloatMode;

#[derive(Clone, Debug)]
pub struct FunctionInfo {
    pub name: String,
    pub params: Vec<GlslParamMeta>,
    pub return_type: GlslType,
}

/// Parsed module plus one entry per named user function, in [`Module::functions`] iteration order.
pub struct NagaModule {
    pub module: Module,
    /// `(function_handle, metadata)` for each exported GLSL function.
    pub functions: Vec<(Handle<Function>, FunctionInfo)>,
}

#[derive(Debug)]
pub enum CompileError {
    Parse(String),
    UnsupportedType(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(msg) => write!(f, "GLSL parse error: {msg}"),
            Self::UnsupportedType(msg) => write!(f, "unsupported type: {msg}"),
        }
    }
}

impl core::error::Error for CompileError {}

/// LPFX preamble and `#line 1` sent to Naga before the user snippet (same layout as [`compile`]).
const LPFX_PREFIX: &str = concat!(
    "#version 450 core\n",
    include_str!("lpfx_prologue.glsl"),
    "\n#line 1\n",
);

fn prepend_lpfx_prototypes(source: &str) -> String {
    let mut s = String::from(LPFX_PREFIX);
    s.push_str(source);
    s
}

/// 1-based physical line where the user snippet's line 1 begins in sources from
/// [`prepared_glsl_for_compile`] (after `#line 1`, before any synthesized `void main()` suffix).
pub fn user_snippet_first_physical_line() -> usize {
    LPFX_PREFIX.lines().count() + 1
}

/// Full GLSL source passed to Naga: LPFX preamble, user snippet, then optional synthesized
/// `void main() {}` when the user did not define `void main`.
pub fn prepared_glsl_for_compile(user_snippet: &str) -> String {
    let source = prepend_lpfx_prototypes(user_snippet);
    ensure_vertex_entry_point(&source)
}

/// Wrap a parsed [`Module`] the same way as [`compile`] after parsing.
pub fn naga_module_from_parsed(module: Module) -> Result<NagaModule, CompileError> {
    let functions = extract_functions(&module)?;
    Ok(NagaModule { module, functions })
}

/// Parse GLSL and collect named function metadata.
pub fn compile(source: &str) -> Result<NagaModule, CompileError> {
    let source = prepared_glsl_for_compile(source);
    let module = parse_glsl(&source)?;
    naga_module_from_parsed(module)
}

/// Naga's GLSL frontend expects a shader entry point. Filetests and snippets only define helpers;
/// append an empty `main` when missing.
fn ensure_vertex_entry_point(source: &str) -> String {
    if glsl_source_declares_main(source) {
        return String::from(source);
    }
    let mut s = String::from(source);
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str("void main() {}\n");
    s
}

fn glsl_source_declares_main(source: &str) -> bool {
    source.lines().any(|line| {
        let t = line.trim_start();
        if t.starts_with("//") {
            return false;
        }
        t.split_whitespace().any(|tok| tok.starts_with("main("))
    })
}

fn parse_glsl(source: &str) -> Result<Module, CompileError> {
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(ShaderStage::Vertex);
    frontend
        .parse(&options, source)
        .map_err(|e| CompileError::Parse(e.emit_to_string(source)))
}

fn extract_functions(
    module: &Module,
) -> Result<Vec<(Handle<Function>, FunctionInfo)>, CompileError> {
    let mut out = Vec::new();
    for (handle, function) in module.functions.iter() {
        let Some(name) = function.name.clone() else {
            continue;
        };
        if name.starts_with("lpfx_") {
            continue;
        }
        // Skip the synthesized `void main() {}` entry point but keep user functions
        // named "main" that have parameters (e.g. `vec4 main(vec2, vec2, float)`).
        if name == "main" && function.arguments.is_empty() {
            continue;
        }
        let info = function_info(module, function, name)?;
        out.push((handle, info));
    }
    Ok(out)
}

fn function_info(
    module: &Module,
    function: &Function,
    name: String,
) -> Result<FunctionInfo, CompileError> {
    let params = function
        .arguments
        .iter()
        .map(|arg| {
            let inner = &module.types[arg.ty].inner;
            let pname = arg.name.clone().unwrap_or_else(|| String::from("_"));
            let (ty, qualifier) = match *inner {
                TypeInner::Pointer {
                    base,
                    space: AddressSpace::Function,
                } => (
                    naga_type_inner_to_glsl(module, &module.types[base].inner)?,
                    GlslParamQualifier::InOut,
                ),
                _ => (
                    naga_type_inner_to_glsl(module, inner)?,
                    GlslParamQualifier::In,
                ),
            };
            Ok(GlslParamMeta {
                name: pname,
                qualifier,
                ty,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let return_type = match &function.result {
        Some(res) => naga_type_inner_to_glsl(module, &module.types[res.ty].inner)?,
        None => GlslType::Void,
    };
    Ok(FunctionInfo {
        name,
        params,
        return_type,
    })
}

fn naga_type_inner_to_glsl(module: &Module, inner: &TypeInner) -> Result<GlslType, CompileError> {
    match *inner {
        TypeInner::Pointer { base, .. } => {
            naga_type_inner_to_glsl(module, &module.types[base].inner)
        }
        TypeInner::Scalar(scalar) => match scalar.kind {
            ScalarKind::Float if scalar.width == 4 => Ok(GlslType::Float),
            ScalarKind::Sint if scalar.width == 4 => Ok(GlslType::Int),
            ScalarKind::Uint if scalar.width == 4 => Ok(GlslType::UInt),
            ScalarKind::Bool => Ok(GlslType::Bool),
            _ => Err(CompileError::UnsupportedType(format!(
                "scalar kind {:?} width {}",
                scalar.kind, scalar.width
            ))),
        },
        TypeInner::Vector { size, scalar } => {
            let width_ok = match scalar.kind {
                ScalarKind::Bool => scalar.width == 1,
                _ => scalar.width == 4,
            };
            if !width_ok {
                return Err(CompileError::UnsupportedType(format!(
                    "vector width {}",
                    scalar.width
                )));
            }
            match (size, scalar.kind) {
                (VectorSize::Bi, ScalarKind::Float) => Ok(GlslType::Vec2),
                (VectorSize::Tri, ScalarKind::Float) => Ok(GlslType::Vec3),
                (VectorSize::Quad, ScalarKind::Float) => Ok(GlslType::Vec4),
                (VectorSize::Bi, ScalarKind::Sint) => Ok(GlslType::IVec2),
                (VectorSize::Tri, ScalarKind::Sint) => Ok(GlslType::IVec3),
                (VectorSize::Quad, ScalarKind::Sint) => Ok(GlslType::IVec4),
                (VectorSize::Bi, ScalarKind::Uint) => Ok(GlslType::UVec2),
                (VectorSize::Tri, ScalarKind::Uint) => Ok(GlslType::UVec3),
                (VectorSize::Quad, ScalarKind::Uint) => Ok(GlslType::UVec4),
                (VectorSize::Bi, ScalarKind::Bool) => Ok(GlslType::BVec2),
                (VectorSize::Tri, ScalarKind::Bool) => Ok(GlslType::BVec3),
                (VectorSize::Quad, ScalarKind::Bool) => Ok(GlslType::BVec4),
                _ => Err(CompileError::UnsupportedType(format!(
                    "vector {:?} {:?}",
                    size, scalar.kind
                ))),
            }
        }
        TypeInner::Matrix {
            columns,
            rows,
            scalar,
        } => {
            if scalar.kind != ScalarKind::Float || scalar.width != 4 {
                return Err(CompileError::UnsupportedType(format!(
                    "matrix scalar {:?} width {}",
                    scalar.kind, scalar.width
                )));
            }
            match (columns, rows) {
                (VectorSize::Bi, VectorSize::Bi) => Ok(GlslType::Mat2),
                (VectorSize::Tri, VectorSize::Tri) => Ok(GlslType::Mat3),
                (VectorSize::Quad, VectorSize::Quad) => Ok(GlslType::Mat4),
                _ => Err(CompileError::UnsupportedType(format!(
                    "matrix {columns:?}x{rows:?}"
                ))),
            }
        }
        TypeInner::Struct { .. } => Err(CompileError::UnsupportedType(String::from("struct"))),
        _ => Err(CompileError::UnsupportedType(format!("{inner:?}"))),
    }
}
