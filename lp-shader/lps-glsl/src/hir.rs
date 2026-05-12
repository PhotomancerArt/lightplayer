use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lps_shared::{
    FnParam, LayoutRules, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier,
};

use crate::body::{BinaryOp, ParsedExpr, ParsedExprKind, ParsedFunctionBody, ParsedStmt, UnaryOp};
use crate::{Diagnostic, FunctionDecl, Span, TopLevelIndex, TypeRef};

#[derive(Debug, Clone)]
pub struct HirModule {
    pub functions: Vec<HirFunction>,
    pub meta: LpsModuleSig,
    pub uniforms: BTreeMap<String, UniformInfo>,
}

#[derive(Debug, Clone)]
pub struct UniformInfo {
    pub ty: LpsType,
    pub byte_offset: u32,
}

#[derive(Debug, Clone)]
pub struct HirFunction {
    pub name: String,
    pub return_ty: LpsType,
    pub params: Vec<HirParam>,
    pub body: HirFunctionBody,
}

#[derive(Debug, Clone)]
pub struct HirParam {
    pub name: Option<String>,
    pub ty: LpsType,
}

#[derive(Debug, Clone)]
pub struct HirFunctionBody {
    pub statements: Vec<HirStmt>,
}

#[derive(Debug, Clone)]
pub enum HirStmt {
    Return(HirExpr),
}

#[derive(Debug, Clone)]
pub struct HirExpr {
    pub span: Span,
    pub ty: LpsType,
    pub kind: HirExprKind,
}

#[derive(Debug, Clone)]
pub enum HirExprKind {
    FloatLiteral(f32),
    IntLiteral(i32),
    UIntLiteral(u32),
    Param {
        index: usize,
    },
    Uniform {
        name: String,
        byte_offset: u32,
    },
    Constructor {
        args: Vec<HirExpr>,
    },
    Mod {
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<HirExpr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
    },
}

pub fn build_hir(
    index: &TopLevelIndex,
    bodies: Vec<(String, ParsedFunctionBody)>,
) -> Result<HirModule, Diagnostic> {
    let (uniforms, uniforms_type) = build_uniforms(index)?;
    let body_map = bodies.into_iter().collect::<BTreeMap<_, _>>();
    let mut functions = Vec::new();
    let mut function_sigs = Vec::new();

    for function in &index.functions {
        let return_ty = type_ref_to_lps(&function.return_ty)?;
        let params = function
            .params
            .iter()
            .map(|p| {
                Ok(HirParam {
                    name: p.name.clone(),
                    ty: type_ref_to_lps(&p.ty)?,
                })
            })
            .collect::<Result<Vec<_>, Diagnostic>>()?;
        function_sigs.push(LpsFnSig {
            name: function.name.clone(),
            return_type: return_ty.clone(),
            parameters: params
                .iter()
                .map(|p| FnParam {
                    name: p.name.clone().unwrap_or_default(),
                    ty: p.ty.clone(),
                    qualifier: ParamQualifier::In,
                })
                .collect(),
            kind: LpsFnKind::UserDefined,
        });
        let parsed_body = body_map
            .get(function.name.as_str())
            .ok_or_else(|| Diagnostic::error(function.body_span, "missing parsed function body"))?;
        let body = type_function_body(function, &params, &return_ty, parsed_body, &uniforms)?;
        functions.push(HirFunction {
            name: function.name.clone(),
            return_ty,
            params,
            body,
        });
    }

    Ok(HirModule {
        functions,
        meta: LpsModuleSig {
            functions: function_sigs,
            uniforms_type,
            globals_type: None,
            ..Default::default()
        },
        uniforms,
    })
}

pub fn type_ref_to_lps(ty: &TypeRef) -> Result<LpsType, Diagnostic> {
    match ty.name.as_str() {
        "void" => Ok(LpsType::Void),
        "float" => Ok(LpsType::Float),
        "int" => Ok(LpsType::Int),
        "uint" => Ok(LpsType::UInt),
        "bool" => Ok(LpsType::Bool),
        "vec2" => Ok(LpsType::Vec2),
        "vec3" => Ok(LpsType::Vec3),
        "vec4" => Ok(LpsType::Vec4),
        other => Err(Diagnostic::error(
            ty.span,
            format!("M2 lps-glsl does not support type `{other}`"),
        )),
    }
}

fn build_uniforms(
    index: &TopLevelIndex,
) -> Result<(BTreeMap<String, UniformInfo>, Option<LpsType>), Diagnostic> {
    let mut uniforms = BTreeMap::new();
    let mut members = Vec::new();
    let mut offset = lps_shared::VMCTX_HEADER_SIZE;
    for uniform in &index.uniforms {
        let ty = type_ref_to_lps(&uniform.ty)?;
        let align = lps_shared::type_alignment(&ty, LayoutRules::Std430);
        offset = lps_shared::layout::round_up(offset, align);
        let byte_offset = offset as u32;
        offset += lps_shared::type_size(&ty, LayoutRules::Std430);
        members.push(lps_shared::StructMember {
            name: Some(uniform.name.clone()),
            ty: ty.clone(),
        });
        uniforms.insert(uniform.name.clone(), UniformInfo { ty, byte_offset });
    }
    let uniforms_type = if members.is_empty() {
        None
    } else {
        Some(LpsType::Struct {
            name: Some(String::from("__uniforms")),
            members,
        })
    };
    Ok((uniforms, uniforms_type))
}

fn type_function_body(
    function: &FunctionDecl,
    params: &[HirParam],
    return_ty: &LpsType,
    parsed: &ParsedFunctionBody,
    uniforms: &BTreeMap<String, UniformInfo>,
) -> Result<HirFunctionBody, Diagnostic> {
    let mut statements = Vec::new();
    for stmt in &parsed.statements {
        match stmt {
            ParsedStmt::Return(expr) => {
                let expr = type_expr(expr, params, uniforms)?;
                if expr.ty != *return_ty {
                    return Err(Diagnostic::error(
                        expr.span,
                        format!(
                            "return type mismatch in `{}`: expected {:?}, found {:?}",
                            function.name, return_ty, expr.ty
                        ),
                    ));
                }
                statements.push(HirStmt::Return(expr));
            }
        }
    }
    Ok(HirFunctionBody { statements })
}

fn type_expr(
    expr: &ParsedExpr,
    params: &[HirParam],
    uniforms: &BTreeMap<String, UniformInfo>,
) -> Result<HirExpr, Diagnostic> {
    match &expr.kind {
        ParsedExprKind::FloatLiteral(v) => Ok(HirExpr {
            span: expr.span,
            ty: LpsType::Float,
            kind: HirExprKind::FloatLiteral(*v),
        }),
        ParsedExprKind::IntLiteral(v) => Ok(HirExpr {
            span: expr.span,
            ty: LpsType::Int,
            kind: HirExprKind::IntLiteral(*v),
        }),
        ParsedExprKind::UIntLiteral(v) => Ok(HirExpr {
            span: expr.span,
            ty: LpsType::UInt,
            kind: HirExprKind::UIntLiteral(*v),
        }),
        ParsedExprKind::Name(name) => {
            if let Some((index, param)) = params
                .iter()
                .enumerate()
                .find(|(_, p)| p.name.as_deref() == Some(name.as_str()))
            {
                return Ok(HirExpr {
                    span: expr.span,
                    ty: param.ty.clone(),
                    kind: HirExprKind::Param { index },
                });
            }
            if let Some(uniform) = uniforms.get(name) {
                return Ok(HirExpr {
                    span: expr.span,
                    ty: uniform.ty.clone(),
                    kind: HirExprKind::Uniform {
                        name: name.clone(),
                        byte_offset: uniform.byte_offset,
                    },
                });
            }
            Err(Diagnostic::error(
                expr.span,
                format!("unknown name `{name}`"),
            ))
        }
        ParsedExprKind::Call { name, args } if is_constructor_name(name) => {
            let target_ty = constructor_type(name, expr.span)?;
            let args = args
                .iter()
                .map(|arg| type_expr(arg, params, uniforms))
                .collect::<Result<Vec<_>, _>>()?;
            ensure_constructor_args(expr.span, &target_ty, &args)?;
            Ok(HirExpr {
                span: expr.span,
                ty: target_ty,
                kind: HirExprKind::Constructor { args },
            })
        }
        ParsedExprKind::Call { name, args } if name == "mod" => {
            if args.len() != 2 {
                return Err(Diagnostic::error(expr.span, "mod expects two arguments"));
            }
            let lhs = type_expr(&args[0], params, uniforms)?;
            let rhs = type_expr(&args[1], params, uniforms)?;
            if lhs.ty != LpsType::Float || rhs.ty != LpsType::Float {
                return Err(Diagnostic::error(
                    expr.span,
                    "M2 lps-glsl supports only scalar float mod",
                ));
            }
            Ok(HirExpr {
                span: expr.span,
                ty: LpsType::Float,
                kind: HirExprKind::Mod {
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            })
        }
        ParsedExprKind::Call { name, .. } => Err(Diagnostic::error(
            expr.span,
            format!("M2 lps-glsl does not support call `{name}`"),
        )),
        ParsedExprKind::Unary { op, expr: inner } => {
            let inner = type_expr(inner, params, uniforms)?;
            if inner.ty != LpsType::Float && inner.ty != LpsType::Int {
                return Err(Diagnostic::error(
                    expr.span,
                    "unsupported unary operand type",
                ));
            }
            Ok(HirExpr {
                span: expr.span,
                ty: inner.ty.clone(),
                kind: HirExprKind::Unary {
                    op: *op,
                    expr: Box::new(inner),
                },
            })
        }
        ParsedExprKind::Binary { op, lhs, rhs } => {
            let lhs = type_expr(lhs, params, uniforms)?;
            let rhs = type_expr(rhs, params, uniforms)?;
            if lhs.ty != rhs.ty {
                return Err(Diagnostic::error(
                    expr.span,
                    "binary operands must have the same type",
                ));
            }
            if lhs.ty != LpsType::Float && lhs.ty != LpsType::Int {
                return Err(Diagnostic::error(
                    expr.span,
                    "M2 lps-glsl supports binary arithmetic on float and int",
                ));
            }
            Ok(HirExpr {
                span: expr.span,
                ty: lhs.ty.clone(),
                kind: HirExprKind::Binary {
                    op: *op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            })
        }
    }
}

fn is_constructor_name(name: &str) -> bool {
    matches!(
        name,
        "float" | "int" | "uint" | "bool" | "vec2" | "vec3" | "vec4"
    )
}

fn constructor_type(name: &str, span: Span) -> Result<LpsType, Diagnostic> {
    let ty_ref = TypeRef {
        name: name.to_string(),
        span,
    };
    type_ref_to_lps(&ty_ref)
}

fn ensure_constructor_args(
    span: Span,
    target_ty: &LpsType,
    args: &[HirExpr],
) -> Result<(), Diagnostic> {
    let expected_lanes = scalar_lane_count(target_ty);
    let actual_lanes = args
        .iter()
        .map(|arg| scalar_lane_count(&arg.ty))
        .sum::<usize>();
    if expected_lanes != actual_lanes {
        return Err(Diagnostic::error(
            span,
            format!(
                "constructor for {:?} expects {expected_lanes} scalar lanes, got {actual_lanes}",
                target_ty
            ),
        ));
    }
    let expected_scalar = scalar_base_type(target_ty).unwrap_or_else(|| target_ty.clone());
    if args
        .iter()
        .any(|arg| scalar_base_type(&arg.ty).unwrap_or_else(|| arg.ty.clone()) != expected_scalar)
    {
        return Err(Diagnostic::error(
            span,
            "constructor arguments must use the target scalar type",
        ));
    }
    Ok(())
}

pub fn scalar_lane_count(ty: &LpsType) -> usize {
    match ty {
        LpsType::Void => 0,
        LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => 1,
        _ => ty.component_count().unwrap_or(0),
    }
}

pub fn scalar_base_type(ty: &LpsType) -> Option<LpsType> {
    if ty.is_vector() {
        ty.vector_base_type()
    } else if ty.is_scalar() {
        Some(ty.clone())
    } else {
        None
    }
}
