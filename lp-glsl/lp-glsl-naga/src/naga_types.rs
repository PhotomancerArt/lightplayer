//! [`NagaModule`], function metadata, Naga → [`GlslType`] mapping, and [`CompileError`] for type extraction.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use naga::{AddressSpace, Function, Handle, Module, ScalarKind, TypeInner, VectorSize};

use lpir::{GlslParamMeta, GlslParamQualifier, GlslType};

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

/// Wrap a parsed [`Module`] the same way as [`crate::compile`] after parsing.
pub fn naga_module_from_parsed(module: Module) -> Result<NagaModule, CompileError> {
    let functions = extract_functions(&module)?;
    Ok(NagaModule { module, functions })
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
