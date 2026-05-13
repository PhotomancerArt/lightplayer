use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lps_shared::{
    FnParam, LayoutRules, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier, StructMember,
};

use crate::body::{
    AssignOp, BinaryOp, ParsedExpr, ParsedExprKind, ParsedFunctionBody, ParsedStmt, UnaryOp,
    parse_expr_tokens,
};
use crate::{Diagnostic, Span, Token, TopLevelIndex, TypeRef};

mod builtin;
mod coerce;
mod function;
mod place;
mod scalar;
mod types;
mod typing;

use function::{FunctionSig, GlobalConst, ImportRegistry};
pub use types::{
    BuiltinKind, HirAssignTarget, HirExpr, HirExprKind, HirFunction, HirFunctionBody, HirLocal,
    HirModule, HirOutArg, HirParam, HirStmt, HirUserCallWriteback, ImportKey, UniformInfo,
};
use types::{HirAccessRoot, StructTypes};
use typing::{
    access_lanes, builtin_kind, coerce_arithmetic_pair, coerce_comparison_pair,
    coerce_constructor_args, coerce_expr, glsl_param_token, is_comparison, is_glsl_import,
    is_logical, type_builtin_args, type_glsl_import_args, vector_dominant_type, zero_expr,
};
pub use typing::{scalar_base_type, scalar_ir_types, scalar_lane_count};

pub fn build_hir(
    source: &str,
    tokens: &[Token],
    index: &TopLevelIndex,
    bodies: Vec<(String, ParsedFunctionBody)>,
) -> Result<HirModule, Diagnostic> {
    let structs = build_struct_types(index)?;
    let (uniforms, uniforms_type) = build_uniforms(index, &structs)?;
    let functions_sigs = build_function_sigs(index, &structs)?;
    let globals = build_global_consts(source, tokens, index, &uniforms, &functions_sigs, &structs)?;
    let body_map = bodies.into_iter().collect::<BTreeMap<_, _>>();
    let mut imports = ImportRegistry::default();
    let mut functions = Vec::new();
    let mut function_meta = Vec::new();

    for (function_index, sig) in functions_sigs.iter().enumerate() {
        function_meta.push(LpsFnSig {
            name: sig.name.clone(),
            return_type: sig.return_ty.clone(),
            parameters: sig
                .params
                .iter()
                .map(|p| FnParam {
                    name: p.name.clone().unwrap_or_default(),
                    ty: p.ty.clone(),
                    qualifier: p.qualifier,
                })
                .collect(),
            kind: LpsFnKind::UserDefined,
        });

        let decl = &index.functions[function_index];
        let parsed_body = body_map
            .get(sig.name.as_str())
            .ok_or_else(|| Diagnostic::error(decl.body_span, "missing parsed function body"))?;
        let mut ctx = TypeCtx::new(
            sig,
            &functions_sigs,
            &uniforms,
            &globals,
            &structs,
            &mut imports,
        );
        let body = ctx.type_block(&parsed_body.statements, &sig.return_ty)?;
        functions.push(HirFunction {
            name: sig.name.clone(),
            return_ty: sig.return_ty.clone(),
            params: sig.params.clone(),
            body,
        });
    }

    Ok(HirModule {
        functions,
        meta: LpsModuleSig {
            functions: function_meta,
            uniforms_type,
            globals_type: None,
            ..Default::default()
        },
        uniforms,
        imports: imports.into_vec(),
    })
}

fn type_ref_to_lps_with_structs(
    ty: &TypeRef,
    structs: &StructTypes,
) -> Result<LpsType, Diagnostic> {
    type_name_to_lps_with_structs(&ty.name, ty.span, structs)
}

fn type_name_to_lps_with_structs(
    name: &str,
    span: Span,
    structs: &StructTypes,
) -> Result<LpsType, Diagnostic> {
    if let Some((element_name, len)) = parse_array_type_name(name) {
        let element = type_name_to_lps_with_structs(element_name, span, structs)?;
        return Ok(LpsType::Array {
            element: Box::new(element),
            len,
        });
    }
    if let Some(ty) = structs.get(name) {
        return Ok(ty.clone());
    }
    match name {
        "void" => Ok(LpsType::Void),
        "float" => Ok(LpsType::Float),
        "int" => Ok(LpsType::Int),
        "uint" => Ok(LpsType::UInt),
        "bool" => Ok(LpsType::Bool),
        "vec2" => Ok(LpsType::Vec2),
        "vec3" => Ok(LpsType::Vec3),
        "vec4" => Ok(LpsType::Vec4),
        "ivec2" => Ok(LpsType::IVec2),
        "ivec3" => Ok(LpsType::IVec3),
        "ivec4" => Ok(LpsType::IVec4),
        "uvec2" => Ok(LpsType::UVec2),
        "uvec3" => Ok(LpsType::UVec3),
        "uvec4" => Ok(LpsType::UVec4),
        "bvec2" => Ok(LpsType::BVec2),
        "bvec3" => Ok(LpsType::BVec3),
        "bvec4" => Ok(LpsType::BVec4),
        "mat2" => Ok(LpsType::Mat2),
        "mat3" => Ok(LpsType::Mat3),
        "mat4" => Ok(LpsType::Mat4),
        other => Err(Diagnostic::error(
            span,
            format!("M3 lps-glsl does not support type `{other}`"),
        )),
    }
}

fn parse_array_type_name(name: &str) -> Option<(&str, u32)> {
    let open = name.rfind('[')?;
    let close = name.strip_suffix(']')?;
    let len_text = close.get(open + 1..)?;
    let len = len_text.trim_end_matches(['u', 'U']).parse::<u32>().ok()?;
    Some((&name[..open], len))
}

fn build_struct_types(index: &TopLevelIndex) -> Result<StructTypes, Diagnostic> {
    let mut structs = BTreeMap::new();
    for decl in &index.structs {
        let mut members = Vec::new();
        for member in &decl.members {
            members.push(StructMember {
                name: Some(member.name.clone()),
                ty: type_ref_to_lps_with_structs(&member.ty, &structs)?,
            });
        }
        structs.insert(
            decl.name.clone(),
            LpsType::Struct {
                name: Some(decl.name.clone()),
                members,
            },
        );
    }
    Ok(structs)
}

fn build_function_sigs(
    index: &TopLevelIndex,
    structs: &StructTypes,
) -> Result<Vec<FunctionSig>, Diagnostic> {
    index
        .functions
        .iter()
        .map(|function| {
            Ok(FunctionSig {
                name: function.name.clone(),
                return_ty: type_ref_to_lps_with_structs(&function.return_ty, structs)?,
                params: function
                    .params
                    .iter()
                    .map(|p| {
                        Ok(HirParam {
                            name: p.name.clone(),
                            ty: type_ref_to_lps_with_structs(&p.ty, structs)?,
                            qualifier: p.qualifier,
                        })
                    })
                    .collect::<Result<Vec<_>, Diagnostic>>()?,
            })
        })
        .collect()
}

fn build_uniforms(
    index: &TopLevelIndex,
    structs: &StructTypes,
) -> Result<(BTreeMap<String, UniformInfo>, Option<LpsType>), Diagnostic> {
    let mut uniforms = BTreeMap::new();
    let mut members = Vec::new();
    let mut offset = lps_shared::VMCTX_HEADER_SIZE;
    for uniform in &index.uniforms {
        let ty = type_ref_to_lps_with_structs(&uniform.ty, structs)?;
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

fn build_global_consts(
    source: &str,
    tokens: &[Token],
    index: &TopLevelIndex,
    uniforms: &BTreeMap<String, UniformInfo>,
    functions: &[FunctionSig],
    structs: &StructTypes,
) -> Result<BTreeMap<String, GlobalConst>, Diagnostic> {
    let mut globals = BTreeMap::new();
    let mut imports = ImportRegistry::default();
    for konst in &index.consts {
        let ty = type_ref_to_lps_with_structs(&konst.ty, structs)?;
        let Some(init_span) = konst.init_span else {
            return Err(Diagnostic::error(
                konst.span,
                "const declaration requires initializer",
            ));
        };
        let parsed = parse_expr_tokens(source, tokens, init_span)?;
        let mut ctx = TypeCtx::global_const(functions, uniforms, &globals, structs, &mut imports);
        let expr = ctx.type_expr(&parsed)?;
        let expr = ctx.coerce_expr(expr, &ty)?;
        globals.insert(konst.name.clone(), GlobalConst { expr });
    }
    Ok(globals)
}

struct TypeCtx<'a> {
    params: &'a [HirParam],
    functions: &'a [FunctionSig],
    uniforms: &'a BTreeMap<String, UniformInfo>,
    globals: &'a BTreeMap<String, GlobalConst>,
    structs: &'a StructTypes,
    imports: &'a mut ImportRegistry,
    locals: Vec<HirLocal>,
    scopes: Vec<BTreeMap<String, usize>>,
    loop_depth: usize,
}

impl<'a> TypeCtx<'a> {
    fn new(
        function: &'a FunctionSig,
        functions: &'a [FunctionSig],
        uniforms: &'a BTreeMap<String, UniformInfo>,
        globals: &'a BTreeMap<String, GlobalConst>,
        structs: &'a StructTypes,
        imports: &'a mut ImportRegistry,
    ) -> Self {
        Self {
            params: &function.params,
            functions,
            uniforms,
            globals,
            structs,
            imports,
            locals: Vec::new(),
            scopes: alloc::vec![BTreeMap::new()],
            loop_depth: 0,
        }
    }

    fn global_const(
        functions: &'a [FunctionSig],
        uniforms: &'a BTreeMap<String, UniformInfo>,
        globals: &'a BTreeMap<String, GlobalConst>,
        structs: &'a StructTypes,
        imports: &'a mut ImportRegistry,
    ) -> Self {
        Self {
            params: &[],
            functions,
            uniforms,
            globals,
            structs,
            imports,
            locals: Vec::new(),
            scopes: alloc::vec![BTreeMap::new()],
            loop_depth: 0,
        }
    }

    fn type_block(
        &mut self,
        parsed: &[ParsedStmt],
        return_ty: &LpsType,
    ) -> Result<HirFunctionBody, Diagnostic> {
        let statements = self.type_statements(parsed, return_ty)?;
        Ok(HirFunctionBody {
            locals: core::mem::take(&mut self.locals),
            statements,
        })
    }

    fn type_statements(
        &mut self,
        parsed: &[ParsedStmt],
        return_ty: &LpsType,
    ) -> Result<Vec<HirStmt>, Diagnostic> {
        let mut statements = Vec::new();
        for stmt in parsed {
            statements.append(&mut self.type_stmt(stmt, return_ty)?);
        }
        Ok(statements)
    }

    fn type_stmt(
        &mut self,
        stmt: &ParsedStmt,
        return_ty: &LpsType,
    ) -> Result<Vec<HirStmt>, Diagnostic> {
        match stmt {
            ParsedStmt::Let {
                ty,
                name,
                init,
                span,
                ..
            } => {
                let ty = self.type_name_to_lps(ty, *span)?;
                let init = if let Some(init) = init {
                    let expr = self.type_expr(init)?;
                    self.coerce_expr(expr, &ty)?
                } else {
                    zero_expr(*span, &ty)?
                };
                let local = self.locals.len();
                self.locals.push(HirLocal {
                    name: name.clone(),
                    ty: ty.clone(),
                });
                self.scopes
                    .last_mut()
                    .expect("type scope")
                    .insert(name.clone(), local);
                Ok(alloc::vec![HirStmt::Let { local, init }])
            }
            ParsedStmt::LetGroup { declarations, .. } => {
                let mut statements = Vec::new();
                for declaration in declarations {
                    let ty = self.type_name_to_lps(&declaration.ty, declaration.span)?;
                    let init = if let Some(init) = &declaration.init {
                        let expr = self.type_expr(init)?;
                        self.coerce_expr(expr, &ty)?
                    } else {
                        zero_expr(declaration.span, &ty)?
                    };
                    let local = self.locals.len();
                    self.locals.push(HirLocal {
                        name: declaration.name.clone(),
                        ty: ty.clone(),
                    });
                    self.scopes
                        .last_mut()
                        .expect("type scope")
                        .insert(declaration.name.clone(), local);
                    statements.push(HirStmt::Let { local, init });
                }
                Ok(statements)
            }
            ParsedStmt::Assign {
                name,
                op,
                value,
                span,
            } => {
                let target = self.type_name_assign_target(*span, name)?;
                let value = self.type_assign_value(*span, &target, *op, value)?;
                Ok(alloc::vec![HirStmt::Expr(HirExpr {
                    span: *span,
                    ty: value.ty.clone(),
                    kind: HirExprKind::Assign {
                        target,
                        value: Box::new(value),
                    },
                })])
            }
            ParsedStmt::If {
                condition,
                accept,
                reject,
                ..
            } => {
                let condition = self.type_expr(condition)?;
                let condition = self.coerce_expr(condition, &LpsType::Bool)?;
                self.scopes.push(BTreeMap::new());
                let accept = self.type_statements(accept, return_ty)?;
                self.scopes.pop();
                self.scopes.push(BTreeMap::new());
                let reject = self.type_statements(reject, return_ty)?;
                self.scopes.pop();
                Ok(alloc::vec![HirStmt::If {
                    condition,
                    accept,
                    reject,
                }])
            }
            ParsedStmt::For {
                init,
                condition,
                continuing,
                body,
                span,
            } => {
                self.scopes.push(BTreeMap::new());
                let init = self.type_statements(init, return_ty)?;
                let condition = if let Some(condition) = condition {
                    let condition = self.type_expr(condition)?;
                    self.coerce_expr(condition, &LpsType::Bool)?
                } else {
                    HirExpr {
                        span: *span,
                        ty: LpsType::Bool,
                        kind: HirExprKind::BoolLiteral(true),
                    }
                };
                self.loop_depth += 1;
                self.scopes.push(BTreeMap::new());
                let body = self.type_statements(body, return_ty)?;
                self.scopes.pop();
                let continuing = self.type_statements(continuing, return_ty)?;
                self.loop_depth -= 1;
                self.scopes.pop();
                Ok(alloc::vec![HirStmt::For {
                    init,
                    condition,
                    continuing,
                    body,
                }])
            }
            ParsedStmt::While {
                condition,
                body,
                span: _,
            } => {
                let condition = self.type_expr(condition)?;
                let condition = self.coerce_expr(condition, &LpsType::Bool)?;
                self.loop_depth += 1;
                self.scopes.push(BTreeMap::new());
                let body = self.type_statements(body, return_ty)?;
                self.scopes.pop();
                self.loop_depth -= 1;
                Ok(alloc::vec![HirStmt::While { condition, body }])
            }
            ParsedStmt::DoWhile {
                body, condition, ..
            } => {
                self.loop_depth += 1;
                self.scopes.push(BTreeMap::new());
                let body = self.type_statements(body, return_ty)?;
                self.scopes.pop();
                self.loop_depth -= 1;
                let condition = self.type_expr(condition)?;
                let condition = self.coerce_expr(condition, &LpsType::Bool)?;
                Ok(alloc::vec![HirStmt::DoWhile { body, condition }])
            }
            ParsedStmt::Break { span } => {
                if self.loop_depth == 0 {
                    return Err(Diagnostic::error(*span, "break outside loop"));
                }
                Ok(alloc::vec![HirStmt::Break])
            }
            ParsedStmt::Continue { span } => {
                if self.loop_depth == 0 {
                    return Err(Diagnostic::error(*span, "continue outside loop"));
                }
                Ok(alloc::vec![HirStmt::Continue])
            }
            ParsedStmt::Block { statements, .. } => {
                self.scopes.push(BTreeMap::new());
                let statements = self.type_statements(statements, return_ty)?;
                self.scopes.pop();
                Ok(statements)
            }
            ParsedStmt::Empty { .. } => Ok(Vec::new()),
            ParsedStmt::Expr { expr, .. } => {
                let expr = self.type_expr(expr)?;
                Ok(alloc::vec![HirStmt::Expr(expr)])
            }
            ParsedStmt::Return { expr, span } => {
                let expr = match expr {
                    Some(expr) if *return_ty == LpsType::Void => {
                        return Err(Diagnostic::error(
                            *span,
                            "void function cannot return a value",
                        ));
                    }
                    Some(expr) => {
                        let expr = self.type_expr(expr)?;
                        Some(self.coerce_expr(expr, return_ty)?)
                    }
                    None if *return_ty == LpsType::Void => None,
                    None => {
                        return Err(Diagnostic::error(
                            *span,
                            "non-void function must return a value",
                        ));
                    }
                };
                Ok(alloc::vec![HirStmt::Return { expr, span: *span }])
            }
        }
    }

    fn type_expr(&mut self, expr: &ParsedExpr) -> Result<HirExpr, Diagnostic> {
        match &expr.kind {
            ParsedExprKind::BoolLiteral(v) => Ok(HirExpr {
                span: expr.span,
                ty: LpsType::Bool,
                kind: HirExprKind::BoolLiteral(*v),
            }),
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
            ParsedExprKind::Name(name) => self.type_name(expr.span, name),
            ParsedExprKind::Call { name, args } if self.is_constructor_name(name) => {
                self.type_constructor(expr.span, name, args)
            }
            ParsedExprKind::Call { name, args } => self.type_call(expr.span, name, args),
            ParsedExprKind::Swizzle { base, fields } => {
                let base = self.type_expr(base)?;
                let (lanes, ty) = access_lanes(expr.span, &base.ty, fields)?;
                Ok(HirExpr {
                    span: expr.span,
                    ty,
                    kind: HirExprKind::Swizzle {
                        base: Box::new(base),
                        lanes,
                    },
                })
            }
            ParsedExprKind::Index { base, index } => {
                let base = self.type_expr(base)?;
                let ty = if base.ty.is_matrix() {
                    base.ty
                        .matrix_column_type()
                        .ok_or_else(|| Diagnostic::error(expr.span, "index base must be matrix"))?
                } else if let Some(element) = base.ty.array_element_type() {
                    element
                } else {
                    scalar_base_type(&base.ty)
                        .ok_or_else(|| Diagnostic::error(expr.span, "index base must be vector"))?
                };
                let index = self.type_expr(index)?;
                let index = self.coerce_expr(index, &LpsType::Int)?;
                Ok(HirExpr {
                    span: expr.span,
                    ty,
                    kind: HirExprKind::Index {
                        base: Box::new(base),
                        index: Box::new(index),
                    },
                })
            }
            ParsedExprKind::Unary { op, expr: inner } => {
                let inner = self.type_expr(inner)?;
                let ty = match op {
                    UnaryOp::Neg if inner.ty == LpsType::Float || inner.ty == LpsType::Int => {
                        inner.ty.clone()
                    }
                    UnaryOp::Not => LpsType::Bool,
                    _ => {
                        return Err(Diagnostic::error(
                            expr.span,
                            "unsupported unary operand type",
                        ));
                    }
                };
                let inner = if *op == UnaryOp::Not {
                    self.coerce_expr(inner, &LpsType::Bool)?
                } else {
                    inner
                };
                Ok(HirExpr {
                    span: expr.span,
                    ty,
                    kind: HirExprKind::Unary {
                        op: *op,
                        expr: Box::new(inner),
                    },
                })
            }
            ParsedExprKind::Binary { op, lhs, rhs } if *op == BinaryOp::Comma => {
                let first = self.type_expr(lhs)?;
                let second = self.type_expr(rhs)?;
                Ok(HirExpr {
                    span: expr.span,
                    ty: second.ty.clone(),
                    kind: HirExprKind::Sequence {
                        first: Box::new(first),
                        second: Box::new(second),
                    },
                })
            }
            ParsedExprKind::Binary { op, lhs, rhs } => self.type_binary(expr.span, *op, lhs, rhs),
            ParsedExprKind::Conditional {
                condition,
                accept,
                reject,
            } => self.type_conditional(expr.span, condition, accept, reject),
            ParsedExprKind::Assign { target, value } => {
                let target = self.type_assign_target(target)?;
                let value = self.type_expr(value)?;
                let value = self.coerce_expr(value, target.ty())?;
                Ok(HirExpr {
                    span: expr.span,
                    ty: value.ty.clone(),
                    kind: HirExprKind::Assign {
                        target,
                        value: Box::new(value),
                    },
                })
            }
            ParsedExprKind::IncDec { target, op, prefix } => {
                let target = self.type_assign_target(target)?;
                let ty = target.ty().clone();
                if !matches!(
                    scalar_base_type(&ty),
                    Some(LpsType::Float | LpsType::Int | LpsType::UInt)
                ) {
                    return Err(Diagnostic::error(
                        expr.span,
                        "increment/decrement requires numeric local",
                    ));
                }
                Ok(HirExpr {
                    span: expr.span,
                    ty,
                    kind: HirExprKind::IncDec {
                        target,
                        op: *op,
                        prefix: *prefix,
                    },
                })
            }
        }
    }

    fn type_name(&self, span: Span, name: &str) -> Result<HirExpr, Diagnostic> {
        if let Some(local) = self.resolve_local(name) {
            return Ok(HirExpr {
                span,
                ty: self.locals[local].ty.clone(),
                kind: HirExprKind::Local { index: local },
            });
        }
        if let Some((index, param)) = self
            .params
            .iter()
            .enumerate()
            .find(|(_, p)| p.name.as_deref() == Some(name))
        {
            return Ok(HirExpr {
                span,
                ty: param.ty.clone(),
                kind: HirExprKind::Param { index },
            });
        }
        if let Some(global) = self.globals.get(name) {
            return Ok(global.expr.clone());
        }
        if let Some(uniform) = self.uniforms.get(name) {
            return Ok(HirExpr {
                span,
                ty: uniform.ty.clone(),
                kind: HirExprKind::Uniform {
                    name: name.to_string(),
                    byte_offset: uniform.byte_offset,
                },
            });
        }
        Err(Diagnostic::error(span, format!("unknown name `{name}`")))
    }

    fn type_constructor(
        &mut self,
        span: Span,
        name: &str,
        args: &[ParsedExpr],
    ) -> Result<HirExpr, Diagnostic> {
        let target_ty = self.type_name_to_lps(name, span)?;
        let args = args
            .iter()
            .map(|arg| self.type_expr(arg))
            .collect::<Result<Vec<_>, _>>()?;
        let args = coerce_constructor_args(span, &target_ty, args)?;
        Ok(HirExpr {
            span,
            ty: target_ty,
            kind: HirExprKind::Constructor { args },
        })
    }

    fn type_call(
        &mut self,
        span: Span,
        name: &str,
        args: &[ParsedExpr],
    ) -> Result<HirExpr, Diagnostic> {
        if let Some((function, sig)) = self
            .functions
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == name)
        {
            if sig.params.len() != args.len() {
                return Err(Diagnostic::error(
                    span,
                    format!("function `{name}` expects {} arguments", sig.params.len()),
                ));
            }
            let mut call_args = Vec::new();
            let mut writebacks = Vec::new();
            for (arg_index, (arg, param)) in args.iter().zip(sig.params.iter()).enumerate() {
                match param.qualifier {
                    ParamQualifier::In => {
                        let value = self.type_expr(arg)?;
                        call_args.push(self.coerce_expr(value, &param.ty)?);
                    }
                    ParamQualifier::Out => {
                        let target = self.type_assign_target(arg)?;
                        if target.ty() != &param.ty {
                            return Err(Diagnostic::error(
                                arg.span,
                                "out argument type must match parameter type",
                            ));
                        }
                        call_args.push(zero_expr(arg.span, &param.ty)?);
                        writebacks.push(HirUserCallWriteback {
                            arg_index,
                            target,
                            ty: param.ty.clone(),
                            copy_in: false,
                        });
                    }
                    ParamQualifier::InOut => {
                        let target = self.type_assign_target(arg)?;
                        if target.ty() != &param.ty {
                            return Err(Diagnostic::error(
                                arg.span,
                                "inout argument type must match parameter type",
                            ));
                        }
                        let value = self.type_expr(arg)?;
                        call_args.push(self.coerce_expr(value, &param.ty)?);
                        writebacks.push(HirUserCallWriteback {
                            arg_index,
                            target,
                            ty: param.ty.clone(),
                            copy_in: true,
                        });
                    }
                }
            }
            return Ok(HirExpr {
                span,
                ty: sig.return_ty.clone(),
                kind: HirExprKind::UserCall {
                    function,
                    args: call_args,
                    writebacks,
                },
            });
        }

        let args = args
            .iter()
            .map(|arg| self.type_expr(arg))
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(kind) = builtin_kind(name) {
            let (args, ty) = type_builtin_args(span, kind, args)?;
            return Ok(HirExpr {
                span,
                ty,
                kind: HirExprKind::Builtin { kind, args },
            });
        }

        if is_glsl_import(name) {
            let (args, ty) = type_glsl_import_args(span, name, args)?;
            let key = self.imports.glsl(name, args.len());
            return Ok(HirExpr {
                span,
                ty,
                kind: HirExprKind::ImportCall {
                    import: key,
                    args,
                    out: None,
                },
            });
        }

        if name.starts_with("lpfn_") {
            return self.type_lpfn_call(span, name, args);
        }

        Err(Diagnostic::error(
            span,
            format!("M3 lps-glsl does not support call `{name}`"),
        ))
    }

    fn type_lpfn_call(
        &mut self,
        span: Span,
        name: &str,
        args: Vec<HirExpr>,
    ) -> Result<HirExpr, Diagnostic> {
        let glsl_params = args
            .iter()
            .map(|arg| glsl_param_token(&arg.ty, span))
            .collect::<Result<Vec<_>, _>>()?;
        let glsl_params_csv = glsl_params.join(",");
        let mut out = None;
        let mut import_args = args.clone();
        let mut param_types = args
            .iter()
            .flat_map(|arg| scalar_ir_types(&arg.ty).unwrap_or_default())
            .collect::<Vec<_>>();
        let return_ty = if name == "lpfn_worley"
            && matches!(glsl_params.as_slice(), [a, b] if (a == "Vec2" || a == "Vec3") && b == "UInt")
        {
            LpsType::Float
        } else if name == "lpfn_fbm"
            && matches!(glsl_params.as_slice(), [a, b, c] if (a == "Vec2" || a == "Vec3") && b == "Int" && c == "UInt")
        {
            LpsType::Float
        } else if name == "lpfn_hsv2rgb" && matches!(glsl_params.as_slice(), [a] if a == "Vec3") {
            LpsType::Vec3
        } else if name == "lpfn_hsv2rgb" && matches!(glsl_params.as_slice(), [a] if a == "Vec4") {
            LpsType::Vec4
        } else if name == "lpfn_psrdnoise"
            && matches!(glsl_params.as_slice(), [a, b, c, d, e] if a == "Vec2" && b == "Vec2" && c == "Float" && d == "Vec2" && e == "UInt")
        {
            let HirExprKind::Local { index } = args[3].kind else {
                return Err(Diagnostic::error(
                    args[3].span,
                    "lpfn_psrdnoise gradient argument must be a local vec2",
                ));
            };
            out = Some(HirOutArg {
                local: index,
                ty: LpsType::Vec2,
                arg_index: 3,
            });
            import_args.remove(3);
            param_types = alloc::vec![
                lpir::IrType::F32,
                lpir::IrType::F32,
                lpir::IrType::F32,
                lpir::IrType::F32,
                lpir::IrType::F32,
                lpir::IrType::I32,
                lpir::IrType::I32,
            ];
            LpsType::Float
        } else {
            return Err(Diagnostic::error(
                span,
                format!("M3 lps-glsl does not support LPFN signature `{name}({glsl_params_csv})`"),
            ));
        };
        let key = self.imports.lpfn(
            name,
            glsl_params_csv,
            param_types,
            scalar_ir_types(&return_ty)?,
        );
        Ok(HirExpr {
            span,
            ty: return_ty,
            kind: HirExprKind::ImportCall {
                import: key,
                args: import_args,
                out,
            },
        })
    }

    fn type_binary(
        &mut self,
        span: Span,
        op: BinaryOp,
        lhs: &ParsedExpr,
        rhs: &ParsedExpr,
    ) -> Result<HirExpr, Diagnostic> {
        let lhs = self.type_expr(lhs)?;
        let rhs = self.type_expr(rhs)?;
        self.type_binary_values(span, op, lhs, rhs)
    }

    fn type_binary_values(
        &mut self,
        span: Span,
        op: BinaryOp,
        lhs: HirExpr,
        rhs: HirExpr,
    ) -> Result<HirExpr, Diagnostic> {
        if is_logical(op) {
            let lhs = self.coerce_expr(lhs, &LpsType::Bool)?;
            let rhs = self.coerce_expr(rhs, &LpsType::Bool)?;
            return Ok(HirExpr {
                span,
                ty: LpsType::Bool,
                kind: HirExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            });
        }
        if is_comparison(op) {
            let (lhs, rhs, ty) = coerce_comparison_pair(span, lhs, rhs)?;
            let ty = if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                LpsType::Bool
            } else {
                ty
            };
            return Ok(HirExpr {
                span,
                ty,
                kind: HirExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            });
        }
        let (lhs, rhs, ty) = coerce_arithmetic_pair(span, lhs, rhs)?;
        if op == BinaryOp::Mod && scalar_base_type(&ty) == Some(LpsType::Float) {
            return Err(Diagnostic::error(span, "modulo requires integer operands"));
        }
        Ok(HirExpr {
            span,
            ty,
            kind: HirExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        })
    }

    fn type_conditional(
        &mut self,
        span: Span,
        condition: &ParsedExpr,
        accept: &ParsedExpr,
        reject: &ParsedExpr,
    ) -> Result<HirExpr, Diagnostic> {
        let condition = self.type_expr(condition)?;
        let condition = self.coerce_expr(condition, &LpsType::Bool)?;
        let accept = self.type_expr(accept)?;
        let reject = self.type_expr(reject)?;
        let ty = if accept.ty == reject.ty {
            accept.ty.clone()
        } else {
            vector_dominant_type(&[&accept.ty, &reject.ty])
                .ok_or_else(|| Diagnostic::error(span, "incompatible ternary arm types"))?
        };
        let accept = self.coerce_expr(accept, &ty)?;
        let reject = self.coerce_expr(reject, &ty)?;
        Ok(HirExpr {
            span,
            ty,
            kind: HirExprKind::Conditional {
                condition: Box::new(condition),
                accept: Box::new(accept),
                reject: Box::new(reject),
            },
        })
    }

    fn type_assign_value(
        &mut self,
        span: Span,
        target: &HirAssignTarget,
        op: AssignOp,
        value: &ParsedExpr,
    ) -> Result<HirExpr, Diagnostic> {
        let ty = target.ty().clone();
        let value = self.type_expr(value)?;
        if op == AssignOp::Set {
            return self.coerce_expr(value, &ty);
        }
        let binary_op = match op {
            AssignOp::Set => unreachable!(),
            AssignOp::Add => BinaryOp::Add,
            AssignOp::Sub => BinaryOp::Sub,
            AssignOp::Mul => BinaryOp::Mul,
            AssignOp::Div => BinaryOp::Div,
            AssignOp::Mod => BinaryOp::Mod,
        };
        let lhs = HirExpr {
            span,
            ty: ty.clone(),
            kind: self.read_assign_target_kind(target),
        };
        let value = self.type_binary_values(span, binary_op, lhs, value)?;
        self.coerce_expr(value, &ty)
    }

    fn type_assign_target(&mut self, expr: &ParsedExpr) -> Result<HirAssignTarget, Diagnostic> {
        match &expr.kind {
            ParsedExprKind::Name(name) => self.type_name_assign_target(expr.span, name),
            ParsedExprKind::Swizzle { base, fields } => {
                let (root, base_lanes, base_ty) = self.type_access_root(base)?;
                let (relative_lanes, ty) = access_lanes(expr.span, &base_ty, fields)?;
                let lanes = relative_lanes
                    .into_iter()
                    .map(|lane| {
                        base_lanes
                            .get(lane)
                            .copied()
                            .ok_or_else(|| Diagnostic::error(expr.span, "field lane out of range"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                match root {
                    HirAccessRoot::Local(local) => {
                        Ok(HirAssignTarget::Swizzle { local, lanes, ty })
                    }
                    HirAccessRoot::Param(param) => {
                        Ok(HirAssignTarget::ParamSwizzle { param, lanes, ty })
                    }
                }
            }
            ParsedExprKind::Index { base, index } => match &base.kind {
                ParsedExprKind::Name(name) => {
                    if let Some(local) = self.resolve_local(name) {
                        let ty = if self.locals[local].ty.is_matrix() {
                            self.locals[local].ty.matrix_column_type().ok_or_else(|| {
                                Diagnostic::error(expr.span, "index base must be matrix")
                            })?
                        } else if let Some(element) = self.locals[local].ty.array_element_type() {
                            element
                        } else {
                            scalar_base_type(&self.locals[local].ty).ok_or_else(|| {
                                Diagnostic::error(expr.span, "index base must be vector")
                            })?
                        };
                        let index = self.type_expr(index)?;
                        let index = self.coerce_expr(index, &LpsType::Int)?;
                        return Ok(HirAssignTarget::Index {
                            local,
                            index: Box::new(index),
                            ty,
                        });
                    }
                    if let Some((param, p)) = self
                        .params
                        .iter()
                        .enumerate()
                        .find(|(_, p)| p.name.as_deref() == Some(name))
                    {
                        let ty = if p.ty.is_matrix() {
                            p.ty.matrix_column_type().ok_or_else(|| {
                                Diagnostic::error(expr.span, "index base must be matrix")
                            })?
                        } else if let Some(element) = p.ty.array_element_type() {
                            element
                        } else {
                            scalar_base_type(&p.ty).ok_or_else(|| {
                                Diagnostic::error(expr.span, "index base must be vector")
                            })?
                        };
                        let index = self.type_expr(index)?;
                        let index = self.coerce_expr(index, &LpsType::Int)?;
                        return Ok(HirAssignTarget::ParamIndex {
                            param,
                            index: Box::new(index),
                            ty,
                        });
                    }
                    Err(Diagnostic::error(
                        expr.span,
                        format!("unknown local `{name}`"),
                    ))
                }
                ParsedExprKind::Index {
                    base: matrix_base,
                    index: column,
                } => {
                    let ParsedExprKind::Name(name) = &matrix_base.kind else {
                        return Err(Diagnostic::error(
                            expr.span,
                            "unsupported matrix element assignment base",
                        ));
                    };
                    let local = self.resolve_local(name).ok_or_else(|| {
                        Diagnostic::error(expr.span, format!("unknown local `{name}`"))
                    })?;
                    if !self.locals[local].ty.is_matrix() {
                        return Err(Diagnostic::error(
                            expr.span,
                            "nested index assignment base must be matrix",
                        ));
                    }
                    let column = self.type_expr(column)?;
                    let column = self.coerce_expr(column, &LpsType::Int)?;
                    let row = self.type_expr(index)?;
                    let row = self.coerce_expr(row, &LpsType::Int)?;
                    Ok(HirAssignTarget::MatrixElement {
                        local,
                        column: Box::new(column),
                        row: Box::new(row),
                        ty: LpsType::Float,
                    })
                }
                _ => Err(Diagnostic::error(
                    expr.span,
                    "unsupported index assignment base",
                )),
            },
            _ => Err(Diagnostic::error(expr.span, "invalid assignment target")),
        }
    }

    fn type_name_assign_target(
        &self,
        span: Span,
        name: &str,
    ) -> Result<HirAssignTarget, Diagnostic> {
        if let Some(local) = self.resolve_local(name) {
            return Ok(HirAssignTarget::Local {
                local,
                ty: self.locals[local].ty.clone(),
            });
        }
        if let Some((param, p)) = self
            .params
            .iter()
            .enumerate()
            .find(|(_, p)| p.name.as_deref() == Some(name))
        {
            return Ok(HirAssignTarget::Param {
                param,
                ty: p.ty.clone(),
            });
        }
        Err(Diagnostic::error(span, format!("unknown local `{name}`")))
    }

    fn type_access_root(
        &self,
        expr: &ParsedExpr,
    ) -> Result<(HirAccessRoot, Vec<usize>, LpsType), Diagnostic> {
        match &expr.kind {
            ParsedExprKind::Name(name) => {
                if let Some(local) = self.resolve_local(name) {
                    let ty = self.locals[local].ty.clone();
                    return Ok((
                        HirAccessRoot::Local(local),
                        (0..scalar_lane_count(&ty)).collect(),
                        ty,
                    ));
                }
                if let Some((param, p)) = self
                    .params
                    .iter()
                    .enumerate()
                    .find(|(_, p)| p.name.as_deref() == Some(name))
                {
                    return Ok((
                        HirAccessRoot::Param(param),
                        (0..scalar_lane_count(&p.ty)).collect(),
                        p.ty.clone(),
                    ));
                }
                Err(Diagnostic::error(
                    expr.span,
                    format!("unknown local `{name}`"),
                ))
            }
            ParsedExprKind::Swizzle { base, fields } => {
                let (root, base_lanes, base_ty) = self.type_access_root(base)?;
                let (relative_lanes, ty) = access_lanes(expr.span, &base_ty, fields)?;
                let lanes = relative_lanes
                    .into_iter()
                    .map(|lane| {
                        base_lanes
                            .get(lane)
                            .copied()
                            .ok_or_else(|| Diagnostic::error(expr.span, "field lane out of range"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((root, lanes, ty))
            }
            _ => Err(Diagnostic::error(
                expr.span,
                "unsupported field assignment base",
            )),
        }
    }

    fn read_assign_target_kind(&self, target: &HirAssignTarget) -> HirExprKind {
        match target {
            HirAssignTarget::Param { param, .. } => HirExprKind::Param { index: *param },
            HirAssignTarget::Local { local, .. } => HirExprKind::Local { index: *local },
            HirAssignTarget::Swizzle { .. }
            | HirAssignTarget::ParamSwizzle { .. }
            | HirAssignTarget::ParamIndex { .. }
            | HirAssignTarget::Index { .. }
            | HirAssignTarget::MatrixElement { .. } => {
                unreachable!("compound assignment statement only has simple name targets")
            }
        }
    }

    fn coerce_expr(&mut self, expr: HirExpr, target: &LpsType) -> Result<HirExpr, Diagnostic> {
        coerce_expr(expr, target)
    }

    fn type_name_to_lps(&self, name: &str, span: Span) -> Result<LpsType, Diagnostic> {
        type_name_to_lps_with_structs(name, span, self.structs)
    }

    fn is_constructor_name(&self, name: &str) -> bool {
        self.type_name_to_lps(name, Span::new(0, 0)).is_ok()
    }

    fn resolve_local(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}
