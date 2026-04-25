//! [`NagaModule`], function metadata, Naga → [`LpsType`] mapping, and [`CompileError`] for type extraction.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use naga::{
    AddressSpace, ArraySize, Function, Handle, ImageClass, ImageDimension, Module, ScalarKind,
    StructMember as NagaStructMember, Type, TypeInner, VectorSize,
};

use lps_shared::{FnParam, LpsType, ParamQualifier, StructMember};

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
    pub params: Vec<FnParam>,
    pub return_type: LpsType,
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
        if name.starts_with("lpfn_") {
            continue;
        }
        if name.starts_with("__lp_") {
            continue;
        }
        // Skip Naga’s synthesized `void main() {}` vertex entry; user shaders use `render(...)`.
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
                    naga_type_handle_to_lps(module, base)?,
                    ParamQualifier::InOut,
                ),
                _ => (naga_type_handle_to_lps(module, arg.ty)?, ParamQualifier::In),
            };
            if matches!(ty, LpsType::Texture2D) {
                return Err(CompileError::UnsupportedType(String::from(
                    "sampler2D / Texture2D function parameters are not supported (no parameter binding metadata yet)",
                )));
            }
            Ok(FnParam {
                name: pname,
                ty,
                qualifier,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let return_type = match &function.result {
        Some(res) => naga_type_handle_to_lps(module, res.ty)?,
        None => LpsType::Void,
    };
    Ok(FunctionInfo {
        name,
        params,
        return_type,
    })
}

/// Map a Naga [`Type`] handle to [`LpsType`], preserving user `struct` names.
pub(crate) fn naga_type_handle_to_lps(
    module: &Module,
    ty_h: Handle<Type>,
) -> Result<LpsType, CompileError> {
    let inner = &module.types[ty_h].inner;
    match *inner {
        TypeInner::Pointer { base, .. } => naga_type_handle_to_lps(module, base),
        TypeInner::Struct { ref members, .. } => {
            if naga_combined_float_sampler2d_struct(module, members) {
                return Ok(LpsType::Texture2D);
            }
            let t = &module.types[ty_h];
            let mut out = Vec::with_capacity(members.len());
            for m in members {
                out.push(StructMember {
                    name: m.name.clone(),
                    ty: naga_type_handle_to_lps(module, m.ty)?,
                });
            }
            Ok(LpsType::Struct {
                name: t.name.clone(),
                members: out,
            })
        }
        _ => naga_type_inner_to_glsl(module, inner),
    }
}

pub(crate) fn naga_type_inner_to_glsl(
    module: &Module,
    inner: &TypeInner,
) -> Result<LpsType, CompileError> {
    match *inner {
        TypeInner::Pointer { base, .. } => naga_type_handle_to_lps(module, base),
        TypeInner::Scalar(scalar) => match scalar.kind {
            ScalarKind::Float if scalar.width == 4 => Ok(LpsType::Float),
            ScalarKind::Sint if scalar.width == 4 => Ok(LpsType::Int),
            ScalarKind::Uint if scalar.width == 4 => Ok(LpsType::UInt),
            ScalarKind::Bool => Ok(LpsType::Bool),
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
                (VectorSize::Bi, ScalarKind::Float) => Ok(LpsType::Vec2),
                (VectorSize::Tri, ScalarKind::Float) => Ok(LpsType::Vec3),
                (VectorSize::Quad, ScalarKind::Float) => Ok(LpsType::Vec4),
                (VectorSize::Bi, ScalarKind::Sint) => Ok(LpsType::IVec2),
                (VectorSize::Tri, ScalarKind::Sint) => Ok(LpsType::IVec3),
                (VectorSize::Quad, ScalarKind::Sint) => Ok(LpsType::IVec4),
                (VectorSize::Bi, ScalarKind::Uint) => Ok(LpsType::UVec2),
                (VectorSize::Tri, ScalarKind::Uint) => Ok(LpsType::UVec3),
                (VectorSize::Quad, ScalarKind::Uint) => Ok(LpsType::UVec4),
                (VectorSize::Bi, ScalarKind::Bool) => Ok(LpsType::BVec2),
                (VectorSize::Tri, ScalarKind::Bool) => Ok(LpsType::BVec3),
                (VectorSize::Quad, ScalarKind::Bool) => Ok(LpsType::BVec4),
                _ => Err(CompileError::UnsupportedType(format!(
                    "vector {:?} {:?}",
                    size, scalar.kind
                ))),
            }
        }
        TypeInner::Array { base, size, .. } => {
            let len = match size {
                ArraySize::Constant(n) => n.get(),
                ArraySize::Pending(_) | ArraySize::Dynamic => {
                    return Err(CompileError::UnsupportedType(String::from(
                        "array with non-constant size",
                    )));
                }
            };
            let elem = naga_type_handle_to_lps(module, base)?;
            Ok(LpsType::Array {
                element: Box::new(elem),
                len,
            })
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
                (VectorSize::Bi, VectorSize::Bi) => Ok(LpsType::Mat2),
                (VectorSize::Tri, VectorSize::Tri) => Ok(LpsType::Mat3),
                (VectorSize::Quad, VectorSize::Quad) => Ok(LpsType::Mat4),
                _ => Err(CompileError::UnsupportedType(format!(
                    "matrix {columns:?}x{rows:?}"
                ))),
            }
        }
        TypeInner::Image {
            dim,
            arrayed,
            class,
        } => naga_image_to_lps_texture2d(dim, arrayed, class),
        TypeInner::Sampler { comparison: true } => Err(CompileError::UnsupportedType(
            String::from("comparison / shadow sampler (sampler2DShadow, etc.)"),
        )),
        TypeInner::Sampler { comparison: false } => Err(CompileError::UnsupportedType(
            String::from("standalone sampler type (use sampler2D or texture2D)"),
        )),
        TypeInner::BindingArray { .. } => Err(CompileError::UnsupportedType(String::from(
            "binding_array of texture/sampler (texture arrays) not supported",
        ))),
        TypeInner::Struct { .. } => Err(CompileError::UnsupportedType(String::from(
            "struct type must be mapped with naga_type_handle_to_lps",
        ))),
        _ => Err(CompileError::UnsupportedType(format!("{inner:?}"))),
    }
}

/// Naga's GLSL `sampler2D` / combined sampler constructor uses a 2-tuple of (image, sampler).
fn naga_combined_float_sampler2d_struct(module: &Module, members: &[NagaStructMember]) -> bool {
    if members.len() != 2 {
        return false;
    }
    let t0 = &module.types[members[0].ty].inner;
    let t1 = &module.types[members[1].ty].inner;
    matches!(
        (t0, t1),
        (
            &TypeInner::Image {
                dim: ImageDimension::D2,
                arrayed: false,
                class: ImageClass::Sampled {
                    kind: ScalarKind::Float,
                    multi: false,
                },
            },
            &TypeInner::Sampler { comparison: false }
        )
    )
}

/// Map a sampled [`TypeInner::Image`] to [`LpsType::Texture2D`]; other image forms are rejected.
fn naga_image_to_lps_texture2d(
    dim: ImageDimension,
    arrayed: bool,
    class: ImageClass,
) -> Result<LpsType, CompileError> {
    match class {
        ImageClass::Storage { .. } => Err(CompileError::UnsupportedType(String::from(
            "storage image (image2D) not supported",
        ))),
        ImageClass::Depth { .. } => Err(CompileError::UnsupportedType(String::from(
            "depth / shadow image not supported",
        ))),
        ImageClass::External => Err(CompileError::UnsupportedType(String::from(
            "external texture not supported",
        ))),
        ImageClass::Sampled { kind, multi } => {
            if multi {
                return Err(CompileError::UnsupportedType(String::from(
                    "multisampled texture (sampler2DMS) not supported",
                )));
            }
            if kind != ScalarKind::Float {
                return Err(CompileError::UnsupportedType(String::from(
                    "only GLSL `sampler2D` / `texture2D` (float) is supported; integer/uint samplers are not supported",
                )));
            }
            match (dim, arrayed) {
                (ImageDimension::D2, false) => Ok(LpsType::Texture2D),
                (ImageDimension::D1, _) => Err(CompileError::UnsupportedType(String::from(
                    "1D texture (sampler1D) not supported",
                ))),
                (ImageDimension::D3, _) => Err(CompileError::UnsupportedType(String::from(
                    "3D texture (sampler3D) not supported",
                ))),
                (ImageDimension::Cube, _) => Err(CompileError::UnsupportedType(String::from(
                    "cube map texture (samplerCube) not supported",
                ))),
                (ImageDimension::D2, true) => Err(CompileError::UnsupportedType(String::from(
                    "2D array texture (sampler2DArray) not supported",
                ))),
            }
        }
    }
}

#[cfg(test)]
mod texture2d_param_tests {
    use alloc::string::String;
    use alloc::vec;

    use super::*;

    use naga::{
        Function, FunctionResult, ImageClass, ImageDimension, Module, Scalar, ScalarKind, Span,
        Type, TypeInner, VectorSize,
    };

    #[test]
    fn function_info_rejects_texture2d_parameter() {
        let mut module = Module::default();
        let vec4_ty = module.types.insert(
            Type {
                name: None,
                inner: TypeInner::Vector {
                    size: VectorSize::Quad,
                    scalar: Scalar {
                        kind: ScalarKind::Float,
                        width: 4,
                    },
                },
            },
            Span::UNDEFINED,
        );
        let image_ty = module.types.insert(
            Type {
                name: None,
                inner: TypeInner::Image {
                    dim: ImageDimension::D2,
                    arrayed: false,
                    class: ImageClass::Sampled {
                        kind: ScalarKind::Float,
                        multi: false,
                    },
                },
            },
            Span::UNDEFINED,
        );
        let sampler_ty = module.types.insert(
            Type {
                name: None,
                inner: TypeInner::Sampler { comparison: false },
            },
            Span::UNDEFINED,
        );
        let combined = module.types.insert(
            Type {
                name: None,
                inner: TypeInner::Struct {
                    members: vec![
                        naga::StructMember {
                            name: None,
                            ty: image_ty,
                            binding: None,
                            offset: 0,
                        },
                        naga::StructMember {
                            name: None,
                            ty: sampler_ty,
                            binding: None,
                            offset: 0,
                        },
                    ],
                    span: 0,
                },
            },
            Span::UNDEFINED,
        );

        let mut func = Function::default();
        func.name = Some(String::from("f"));
        func.arguments.push(naga::FunctionArgument {
            name: Some(String::from("tex")),
            ty: combined,
            binding: None,
        });
        func.result = Some(FunctionResult {
            ty: vec4_ty,
            binding: None,
        });
        let fh = module.functions.append(func, Span::UNDEFINED);
        let func_ref = &module.functions[fh];

        let err = super::function_info(&module, func_ref, String::from("f")).unwrap_err();
        let CompileError::UnsupportedType(msg) = err else {
            panic!("{err:?}");
        };
        assert!(
            msg.contains("function parameters") || msg.contains("parameter"),
            "{msg}"
        );
    }
}
