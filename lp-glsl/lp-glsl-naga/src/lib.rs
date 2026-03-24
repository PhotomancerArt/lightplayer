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
use core::fmt::Write as _;

pub use naga;

use naga::{Function, Handle, Module, ScalarKind, ShaderStage, TypeInner, VectorSize};

mod expr_scalar;
pub mod lower;
mod lower_ctx;
mod lower_error;
mod lower_expr;
mod lower_lpfx;
mod lower_math;
mod lower_stmt;
pub mod std_math_handler;

pub use lower::lower;
pub use lower_error::LowerError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_float_add() {
        let src = "float add(float a, float b) { return a + b; }";
        let result = compile(src).unwrap();
        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.functions[0].1.name, "add");
        assert_eq!(result.functions[0].1.return_type, GlslType::Float);
        assert_eq!(result.functions[0].1.params.len(), 2);
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
    fn naga_module_accessible() {
        let src = "float f() { return 1.0; }";
        let result = compile(src).unwrap();
        assert!(result.module.functions.len() >= 1);
    }

    #[test]
    fn lower_produces_ir_functions() {
        let src = "float add(float a, float b) { return a + b; }";
        let naga = compile(src).unwrap();
        let ir = super::lower(&naga).expect("lower");
        assert_eq!(ir.functions.len(), 1);
        assert_eq!(ir.functions[0].name, "add");
        assert_eq!(ir.functions[0].param_count, 2);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatMode {
    Q32,
    Float,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlslType {
    Void,
    Float,
    Int,
    UInt,
    Bool,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    IVec3,
    IVec4,
    UVec2,
    UVec3,
    UVec4,
    BVec2,
    BVec3,
    BVec4,
}

#[derive(Clone, Debug)]
pub struct FunctionInfo {
    pub name: String,
    pub params: Vec<(String, GlslType)>,
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

fn prepend_lpfx_prototypes(source: &str) -> String {
    const PREAMBLE: &str = "#version 450 core\n";
    let mut s = String::from(PREAMBLE);
    s.push_str(include_str!("lpfx_prologue.glsl"));
    s.push_str("\n#line 1\n");
    s.push_str(source);
    s
}

/// Parse GLSL and collect named function metadata.
pub fn compile(source: &str) -> Result<NagaModule, CompileError> {
    let source = prepend_lpfx_prototypes(source);
    let source = ensure_vertex_entry_point(&source);
    let module = parse_glsl(&source)?;
    let functions = extract_functions(&module)?;
    Ok(NagaModule { module, functions })
}

/// Naga's GLSL frontend expects a shader entry point. Filetests and snippets only define helpers;
/// append an empty `main` when missing.
fn ensure_vertex_entry_point(source: &str) -> String {
    if source.contains("void main") {
        return String::from(source);
    }
    let mut s = String::from(source);
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str("void main() {}\n");
    s
}

fn parse_glsl(source: &str) -> Result<Module, CompileError> {
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(ShaderStage::Vertex);
    frontend.parse(&options, source).map_err(|e| {
        let mut msg = String::new();
        let _ = write!(&mut msg, "{e}");
        CompileError::Parse(msg)
    })
}

fn extract_functions(
    module: &Module,
) -> Result<Vec<(Handle<Function>, FunctionInfo)>, CompileError> {
    let mut out = Vec::new();
    for (handle, function) in module.functions.iter() {
        let Some(name) = function.name.clone() else {
            continue;
        };
        if name == "main" || name.starts_with("lpfx_") {
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
            let ty = naga_type_inner_to_glsl(&module.types[arg.ty].inner)?;
            let pname = arg.name.clone().unwrap_or_else(|| String::from("_"));
            Ok((pname, ty))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let return_type = match &function.result {
        Some(res) => naga_type_inner_to_glsl(&module.types[res.ty].inner)?,
        None => GlslType::Void,
    };
    Ok(FunctionInfo {
        name,
        params,
        return_type,
    })
}

fn naga_type_inner_to_glsl(inner: &TypeInner) -> Result<GlslType, CompileError> {
    match *inner {
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
        TypeInner::Struct { .. } => Err(CompileError::UnsupportedType(String::from("struct"))),
        _ => Err(CompileError::UnsupportedType(format!("{inner:?}"))),
    }
}
