use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lps_shared::{
    LpsType, ParamQualifier, TextureBindingSpec, TextureShapeHint, TextureStorageFormat,
};

use crate::body::{AssignOp, BinaryOp, ParsedExpr, ParsedExprKind, ParsedStmt, UnaryOp};
use crate::{Diagnostic, Span};

use super::array_size::ArraySizeConsts;
use super::const_fold::{fold_binary, fold_builtin_call, fold_glsl_import_call, fold_unary};
use super::function::{FunctionSig, GlobalConst, ImportRegistry};
use super::place::{AccessMode, HirPlace, PlaceRoot, PlaceSegment};
use super::types::StructTypes;
use super::types::{
    GlobalInfo, HirAssignTarget, HirExpr, HirExprKind, HirFunctionBody, HirLocal, HirOutArg,
    HirParam, HirStmt, HirTextureOperand, HirUserCallWriteback, UniformInfo,
};
use super::typing::builtin_has_out_args;
use super::typing::{
    access_lanes, builtin_kind, coerce_arithmetic_pair, coerce_comparison_pair,
    coerce_constructor_args, coerce_expr, glsl_param_token, is_comparison, is_glsl_import,
    is_logical, scalar_base_type, scalar_ir_types, scalar_lane_count, type_builtin_args,
    type_glsl_import_args, vector_dominant_type, zero_expr,
};
use super::{
    fixed_array_from_base, infer_array_constructor_type, infer_array_decl_type,
    parse_array_type_name, resolve_init_list_lens, scalar_or_struct_type_name_to_lps,
    type_name_to_lps_with_structs,
};

pub(super) struct TypeCtx<'a> {
    params: &'a [HirParam],
    functions: &'a [FunctionSig],
    uniforms: &'a BTreeMap<String, UniformInfo>,
    globals: &'a BTreeMap<String, GlobalConst>,
    global_vars: &'a BTreeMap<String, GlobalInfo>,
    structs: &'a StructTypes,
    array_size_consts: &'a ArraySizeConsts,
    imports: &'a mut ImportRegistry,
    texture_specs: &'a BTreeMap<String, TextureBindingSpec>,
    pub(super) locals: Vec<HirLocal>,
    scopes: Vec<BTreeMap<String, usize>>,
    loop_depth: usize,
}

impl<'a> TypeCtx<'a> {
    pub(super) fn new(
        function: &'a FunctionSig,
        functions: &'a [FunctionSig],
        uniforms: &'a BTreeMap<String, UniformInfo>,
        globals: &'a BTreeMap<String, GlobalConst>,
        global_vars: &'a BTreeMap<String, GlobalInfo>,
        structs: &'a StructTypes,
        array_size_consts: &'a ArraySizeConsts,
        imports: &'a mut ImportRegistry,
        texture_specs: &'a BTreeMap<String, TextureBindingSpec>,
    ) -> Self {
        Self {
            params: &function.params,
            functions,
            uniforms,
            globals,
            global_vars,
            structs,
            array_size_consts,
            imports,
            texture_specs,
            locals: Vec::new(),
            scopes: alloc::vec![BTreeMap::new()],
            loop_depth: 0,
        }
    }

    pub(super) fn global_const(
        functions: &'a [FunctionSig],
        uniforms: &'a BTreeMap<String, UniformInfo>,
        globals: &'a BTreeMap<String, GlobalConst>,
        global_vars: &'a BTreeMap<String, GlobalInfo>,
        structs: &'a StructTypes,
        array_size_consts: &'a ArraySizeConsts,
        imports: &'a mut ImportRegistry,
        texture_specs: &'a BTreeMap<String, TextureBindingSpec>,
    ) -> Self {
        Self {
            params: &[],
            functions,
            uniforms,
            globals,
            global_vars,
            structs,
            array_size_consts,
            imports,
            texture_specs,
            locals: Vec::new(),
            scopes: alloc::vec![BTreeMap::new()],
            loop_depth: 0,
        }
    }

    pub(super) fn type_block(
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
                let init = if let Some(init) = init {
                    self.type_decl_init(ty, *span, init)?
                } else {
                    let ty = self.type_decl_ty(ty, *span, None)?;
                    zero_expr(*span, &ty)?
                };
                let ty = init.ty.clone();
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
                    let init = if let Some(init) = &declaration.init {
                        self.type_decl_init(&declaration.ty, declaration.span, init)?
                    } else {
                        let ty = self.type_decl_ty(&declaration.ty, declaration.span, None)?;
                        zero_expr(declaration.span, &ty)?
                    };
                    let ty = init.ty.clone();
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
                let target = self.type_assign_target(&ParsedExpr {
                    span: *span,
                    kind: ParsedExprKind::Name(name.clone()),
                })?;
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

    pub(super) fn type_expr(&mut self, expr: &ParsedExpr) -> Result<HirExpr, Diagnostic> {
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
            ParsedExprKind::InitList { .. } => Err(Diagnostic::error(
                expr.span,
                "initializer list requires declaration type",
            )),
            ParsedExprKind::Swizzle { base, fields } => {
                if let Ok(place) = self.type_place(expr, AccessMode::Read) {
                    return Ok(HirExpr {
                        span: expr.span,
                        ty: place.ty.clone(),
                        kind: HirExprKind::PlaceRead {
                            target: HirAssignTarget { place },
                        },
                    });
                }
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
            ParsedExprKind::Length { base } => {
                let base = self.type_expr(base)?;
                let len = match &base.ty {
                    LpsType::Array { len, .. } => *len,
                    _ => {
                        return Err(Diagnostic::error(expr.span, "length() requires array base"));
                    }
                };
                Ok(HirExpr {
                    span: expr.span,
                    ty: LpsType::Int,
                    kind: HirExprKind::IntLiteral(len as i32),
                })
            }
            ParsedExprKind::Index { base, index } => {
                if let Ok(place) = self.type_place(expr, AccessMode::Read) {
                    return Ok(HirExpr {
                        span: expr.span,
                        ty: place.ty.clone(),
                        kind: HirExprKind::PlaceRead {
                            target: HirAssignTarget { place },
                        },
                    });
                }
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
                    UnaryOp::Neg
                        if matches!(
                            scalar_base_type(&inner.ty),
                            Some(LpsType::Float | LpsType::Int)
                        ) =>
                    {
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
                if let Some(folded) = fold_unary(expr.span, *op, &inner) {
                    return Ok(folded);
                }
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
            ParsedExprKind::Assign { target, op, value } => {
                let target = self.type_assign_target(target)?;
                let value = self.type_assign_value(expr.span, &target, *op, value)?;
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
        if let Some(global) = self.global_vars.get(name) {
            return Ok(HirExpr {
                span,
                ty: global.ty.clone(),
                kind: HirExprKind::Global {
                    name: name.to_string(),
                    byte_offset: global.byte_offset,
                },
            });
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
        let args = args
            .iter()
            .map(|arg| self.type_expr(arg))
            .collect::<Result<Vec<_>, _>>()?;
        let target_ty = self.type_constructor_target(name, span, &args)?;
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

        if let Some(kind) = builtin_kind(name) {
            if builtin_has_out_args(kind) {
                return self.type_builtin_out_call(span, kind, args);
            }
        }

        if name == "texelFetch" {
            return self.type_texel_fetch_call(span, args);
        }
        if name == "texture" {
            return self.type_texture_call(span, args);
        }

        let args = args
            .iter()
            .map(|arg| self.type_expr(arg))
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(kind) = builtin_kind(name) {
            let (args, ty) = type_builtin_args(span, kind, args)?;
            if let Some(folded) = fold_builtin_call(span, kind, &args, &ty) {
                return Ok(folded);
            }
            return Ok(HirExpr {
                span,
                ty,
                kind: HirExprKind::Builtin {
                    kind,
                    args,
                    writebacks: Vec::new(),
                },
            });
        }

        if is_glsl_import(name) {
            let (args, ty) = type_glsl_import_args(span, name, args)?;
            if let Some(folded) = fold_glsl_import_call(span, name, &args, &ty) {
                return Ok(folded);
            }
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

        if name == "__lp_get_fuel" && args.is_empty() {
            let key = self.imports.vm(name, 0);
            return Ok(HirExpr {
                span,
                ty: LpsType::UInt,
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
            format!("unsupported call `{name}`"),
        ))
    }

    fn type_texel_fetch_call(
        &mut self,
        span: Span,
        args: &[ParsedExpr],
    ) -> Result<HirExpr, Diagnostic> {
        if args.len() != 3 {
            return Err(Diagnostic::error(span, "texelFetch expects 3 arguments"));
        }
        let sampler = self.type_texture_operand(&args[0], "texelFetch")?;
        let coord = self.type_expr(&args[1])?;
        if coord.ty != LpsType::IVec2 {
            return Err(Diagnostic::error(
                args[1].span,
                "texelFetch coordinate must be ivec2",
            ));
        }
        let lod = self.type_expr(&args[2])?;
        let lod = self.coerce_expr(lod, &LpsType::Int)?;
        Ok(HirExpr {
            span,
            ty: LpsType::Vec4,
            kind: HirExprKind::TexelFetch {
                sampler,
                coord: Box::new(coord),
                lod: Box::new(lod),
            },
        })
    }

    fn type_texture_call(
        &mut self,
        span: Span,
        args: &[ParsedExpr],
    ) -> Result<HirExpr, Diagnostic> {
        if args.len() != 2 {
            return Err(Diagnostic::error(
                span,
                "texture expects sampler2D and vec2 arguments",
            ));
        }
        let sampler = self.type_texture_operand(&args[0], "texture")?;
        let coord = self.type_expr(&args[1])?;
        if coord.ty != LpsType::Vec2 {
            return Err(Diagnostic::error(
                args[1].span,
                "texture coordinate must be vec2",
            ));
        }
        let spec = self
            .texture_specs
            .get(sampler.path.as_str())
            .ok_or_else(|| {
                Diagnostic::error(
                    args[0].span,
                    format!(
                        "texture `{}`: no texture binding spec for sampler uniform `{}`",
                        sampler.path, sampler.path
                    ),
                )
            })?;
        let (func_name, argc) = match (spec.format, spec.shape_hint) {
            (TextureStorageFormat::Rgba16Unorm, TextureShapeHint::General2D) => {
                ("texture2d_rgba16_unorm", 10)
            }
            (TextureStorageFormat::Rgba16Unorm, TextureShapeHint::HeightOne) => {
                ("texture1d_rgba16_unorm", 7)
            }
            (TextureStorageFormat::R16Unorm, TextureShapeHint::General2D) => {
                ("texture2d_r16_unorm", 10)
            }
            (TextureStorageFormat::R16Unorm, TextureShapeHint::HeightOne) => {
                ("texture1d_r16_unorm", 7)
            }
            (TextureStorageFormat::Rgb16Unorm, _) => {
                return Err(Diagnostic::error(
                    span,
                    "texture does not support Rgb16Unorm filtered sampling",
                ));
            }
        };
        let import = self.imports.texture(func_name, argc);
        Ok(HirExpr {
            span,
            ty: LpsType::Vec4,
            kind: HirExprKind::Texture {
                sampler,
                coord: Box::new(coord),
                import,
            },
        })
    }

    fn type_texture_operand(
        &mut self,
        expr: &ParsedExpr,
        fn_name: &str,
    ) -> Result<HirTextureOperand, Diagnostic> {
        let place = self.type_place(expr, AccessMode::Read)?;
        if place.ty != LpsType::Texture2D {
            return Err(Diagnostic::error(
                expr.span,
                format!("{fn_name} expects sampler2D uniform"),
            ));
        }
        let PlaceRoot::Uniform {
            name,
            byte_offset,
            ty: _,
        } = place.root
        else {
            return Err(Diagnostic::error(
                expr.span,
                format!("{fn_name} sampler must be a uniform sampler2D"),
            ));
        };
        let mut path = name;
        let mut descriptor_byte_offset = byte_offset;
        for segment in place.segments {
            match segment {
                PlaceSegment::Field {
                    name, byte_offset, ..
                } => {
                    path.push('.');
                    path.push_str(&name);
                    descriptor_byte_offset =
                        descriptor_byte_offset.saturating_add(byte_offset as u32);
                }
                PlaceSegment::Index { .. } | PlaceSegment::Swizzle { .. } => {
                    return Err(Diagnostic::error(
                        expr.span,
                        format!(
                            "{fn_name}: texture arrays and swizzled texture operands are not supported"
                        ),
                    ));
                }
            }
        }
        Ok(HirTextureOperand {
            path,
            descriptor_byte_offset,
        })
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
        let return_ty = if let Some(gradient_ty) = lpfn_psrdnoise_gradient_type(&glsl_params) {
            let HirExprKind::Local { index } = args[3].kind else {
                return Err(Diagnostic::error(
                    args[3].span,
                    format!("lpfn_psrdnoise gradient argument must be a local {gradient_ty:?}"),
                ));
            };
            out = Some(HirOutArg {
                local: index,
                ty: gradient_ty,
                arg_index: 3,
            });
            import_args.remove(3);
            param_types = psrdnoise_param_types(&glsl_params);
            LpsType::Float
        } else if let Some(return_ty) = lpfn_return_type(name, &glsl_params) {
            return_ty
        } else {
            return Err(Diagnostic::error(
                span,
                format!("unsupported LPFN signature `{name}({glsl_params_csv})`"),
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
        let parsed_lhs = lhs;
        let parsed_rhs = rhs;
        let lhs = self.type_expr(parsed_lhs)?;
        let rhs = self.type_expr(parsed_rhs)?;
        if op == BinaryOp::Div
            && lhs.ty == rhs.ty
            && scalar_base_type(&lhs.ty) == Some(LpsType::Float)
            && same_nonzero_const_expr_tree(parsed_lhs, parsed_rhs)
        {
            return one_lanes_expr(span, &lhs.ty);
        }
        self.type_binary_values(span, op, lhs, rhs)
    }

    fn type_binary_values(
        &mut self,
        span: Span,
        op: BinaryOp,
        lhs: HirExpr,
        rhs: HirExpr,
    ) -> Result<HirExpr, Diagnostic> {
        if let Some(folded) = fold_binary(span, op, &lhs, &rhs) {
            return Ok(folded);
        }
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
            if matches!(op, BinaryOp::Eq | BinaryOp::Ne)
                && lhs.ty == rhs.ty
                && scalar_base_type(&lhs.ty).is_some()
                && scalar_lane_count(&lhs.ty) > 1
            {
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
        if op == BinaryOp::Mul
            && let Some(ty) = matrix_vector_multiply_type(&lhs.ty, &rhs.ty)
        {
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

    pub(super) fn type_assign_target(
        &mut self,
        expr: &ParsedExpr,
    ) -> Result<HirAssignTarget, Diagnostic> {
        let place = self.type_place(expr, AccessMode::Write)?;
        Ok(HirAssignTarget { place })
    }

    fn type_place(&mut self, expr: &ParsedExpr, mode: AccessMode) -> Result<HirPlace, Diagnostic> {
        let place = match &expr.kind {
            ParsedExprKind::Name(name) => self.type_name_place(expr.span, name)?,
            ParsedExprKind::Swizzle { base, fields } => {
                let mut base = self.type_place(base, mode)?;
                base.push_field(expr.span, fields)?;
                base
            }
            ParsedExprKind::Index { base, index } => {
                let mut base = self.type_place(base, mode)?;
                let index = self.type_expr(index)?;
                let index = self.coerce_expr(index, &LpsType::Int)?;
                base.push_index(index)?;
                base
            }
            _ => return Err(Diagnostic::error(expr.span, "invalid place expression")),
        };
        if mode != AccessMode::Read && !place.root.is_writable() {
            return Err(Diagnostic::error(
                expr.span,
                "cannot write to uniform variable",
            ));
        }
        Ok(place)
    }

    fn type_name_place(&self, span: Span, name: &str) -> Result<HirPlace, Diagnostic> {
        if let Some(local) = self.resolve_local(name) {
            let ty = self.locals[local].ty.clone();
            return Ok(HirPlace::local(local, ty));
        }
        if let Some((param, p)) = self
            .params
            .iter()
            .enumerate()
            .find(|(_, p)| p.name.as_deref() == Some(name))
        {
            let ty = p.ty.clone();
            return Ok(HirPlace::param(param, ty));
        }
        if let Some(uniform) = self.uniforms.get(name) {
            let ty = uniform.ty.clone();
            return Ok(HirPlace::uniform(
                String::from(name),
                uniform.byte_offset,
                ty,
            ));
        }
        if let Some(global) = self.global_vars.get(name) {
            let ty = global.ty.clone();
            return Ok(HirPlace::global(String::from(name), global.byte_offset, ty));
        }
        Err(Diagnostic::error(span, format!("unknown local `{name}`")))
    }

    fn read_assign_target_kind(&self, target: &HirAssignTarget) -> HirExprKind {
        HirExprKind::PlaceRead {
            target: target.clone(),
        }
    }

    pub(super) fn coerce_expr(
        &mut self,
        expr: HirExpr,
        target: &LpsType,
    ) -> Result<HirExpr, Diagnostic> {
        coerce_expr(expr, target)
    }

    fn type_name_to_lps(&self, name: &str, span: Span) -> Result<LpsType, Diagnostic> {
        type_name_to_lps_with_structs(name, span, self.structs, self.array_size_consts)
    }

    fn type_decl_init(
        &mut self,
        name: &str,
        span: Span,
        init: &ParsedExpr,
    ) -> Result<HirExpr, Diagnostic> {
        if matches!(init.kind, ParsedExprKind::InitList { .. }) {
            let ty = self.type_init_list_decl_ty(name, span, init)?;
            return self.type_init_list(init, &ty);
        }
        let expr = self.type_expr(init)?;
        let ty = self.type_decl_ty(name, span, Some(&expr))?;
        self.coerce_expr(expr, &ty)
    }

    fn type_init_list_decl_ty(
        &self,
        name: &str,
        span: Span,
        init: &ParsedExpr,
    ) -> Result<LpsType, Diagnostic> {
        if let Some((base_name, lens)) = parse_array_type_name(name, self.array_size_consts) {
            let base = scalar_or_struct_type_name_to_lps(base_name, span, self.structs)?;
            let lens = resolve_init_list_lens(span, &lens, init)?;
            return fixed_array_from_base(base, &lens, span);
        }
        self.type_name_to_lps(name, span)
    }

    fn type_init_list(
        &mut self,
        init: &ParsedExpr,
        target: &LpsType,
    ) -> Result<HirExpr, Diagnostic> {
        let ParsedExprKind::InitList { elements } = &init.kind else {
            let expr = self.type_expr(init)?;
            return self.coerce_expr(expr, target);
        };
        let LpsType::Array { element, len } = target else {
            return Err(Diagnostic::error(
                init.span,
                "initializer list target must be array",
            ));
        };
        if elements.len() > *len as usize {
            return Err(Diagnostic::error(
                init.span,
                "too many array initializer elements",
            ));
        }
        let mut args = Vec::new();
        for element_init in elements {
            args.push(self.type_init_list(element_init, element)?);
        }
        while args.len() < *len as usize {
            args.push(zero_expr(init.span, element)?);
        }
        Ok(HirExpr {
            span: init.span,
            ty: target.clone(),
            kind: HirExprKind::Constructor { args },
        })
    }

    fn type_decl_ty(
        &self,
        name: &str,
        span: Span,
        init: Option<&HirExpr>,
    ) -> Result<LpsType, Diagnostic> {
        if let Some((base_name, lens)) = parse_array_type_name(name, self.array_size_consts)
            && lens.iter().any(Option::is_none)
        {
            let Some(init) = init else {
                return Err(Diagnostic::error(
                    span,
                    "unsized array declaration requires initializer",
                ));
            };
            let base = scalar_or_struct_type_name_to_lps(base_name, span, self.structs)?;
            return infer_array_decl_type(span, &base, &lens, &init.ty);
        }
        self.type_name_to_lps(name, span)
    }

    fn type_constructor_target(
        &self,
        name: &str,
        span: Span,
        args: &[HirExpr],
    ) -> Result<LpsType, Diagnostic> {
        if let Some((base_name, lens)) = parse_array_type_name(name, self.array_size_consts)
            && lens.iter().any(Option::is_none)
        {
            if args.is_empty() {
                return Err(Diagnostic::error(
                    span,
                    "unsized array constructor requires at least one argument",
                ));
            }
            let base = scalar_or_struct_type_name_to_lps(base_name, span, self.structs)?;
            return infer_array_constructor_type(span, base, &lens, args);
        }
        self.type_name_to_lps(name, span)
    }

    fn is_constructor_name(&self, name: &str) -> bool {
        self.type_name_to_lps(name, Span::new(0, 0)).is_ok()
            || parse_array_type_name(name, self.array_size_consts)
                .is_some_and(|(_, lens)| lens.iter().any(Option::is_none))
    }

    fn resolve_local(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

fn one_lanes_expr(span: Span, ty: &LpsType) -> Result<HirExpr, Diagnostic> {
    let mut args = Vec::new();
    for _ in 0..scalar_lane_count(ty) {
        args.push(HirExpr {
            span,
            ty: LpsType::Float,
            kind: HirExprKind::FloatLiteral(1.0),
        });
    }
    Ok(HirExpr {
        span,
        ty: ty.clone(),
        kind: HirExprKind::Constructor { args },
    })
}

fn lpfn_return_type(name: &str, glsl_params: &[String]) -> Option<LpsType> {
    match name {
        "lpfn_hash" if matches!(glsl_params, [a, b] if (a == "UInt" || a == "UVec2" || a == "UVec3") && b == "UInt") => {
            Some(LpsType::UInt)
        }
        "lpfn_saturate" if matches!(glsl_params, [a] if a == "Float") => Some(LpsType::Float),
        "lpfn_saturate" if matches!(glsl_params, [a] if a == "Vec3") => Some(LpsType::Vec3),
        "lpfn_saturate" if matches!(glsl_params, [a] if a == "Vec4") => Some(LpsType::Vec4),
        "lpfn_hue2rgb" if matches!(glsl_params, [a] if a == "Float") => Some(LpsType::Vec3),
        "lpfn_hsv2rgb" if matches!(glsl_params, [a] if a == "Vec3") => Some(LpsType::Vec3),
        "lpfn_hsv2rgb" if matches!(glsl_params, [a] if a == "Vec4") => Some(LpsType::Vec4),
        "lpfn_rgb2hsv" if matches!(glsl_params, [a] if a == "Vec3") => Some(LpsType::Vec3),
        "lpfn_rgb2hsv" if matches!(glsl_params, [a] if a == "Vec4") => Some(LpsType::Vec4),
        "lpfn_fbm" if matches!(glsl_params, [a, b, c] if (a == "Vec2" || a == "Vec3") && b == "Int" && c == "UInt") => {
            Some(LpsType::Float)
        }
        "lpfn_fbm" if matches!(glsl_params, [a, b, c, d] if a == "Vec3" && b == "Float" && c == "Int" && d == "UInt") => {
            Some(LpsType::Float)
        }
        "lpfn_gnoise" if matches!(glsl_params, [a, b] if (a == "Float" || a == "Vec2" || a == "Vec3") && b == "UInt") => {
            Some(LpsType::Float)
        }
        "lpfn_gnoise" if matches!(glsl_params, [a, b, c] if a == "Vec3" && b == "Float" && c == "UInt") => {
            Some(LpsType::Float)
        }
        "lpfn_random" if matches!(glsl_params, [a, b] if (a == "Float" || a == "Vec2" || a == "Vec3") && b == "UInt") => {
            Some(LpsType::Float)
        }
        "lpfn_snoise" if matches!(glsl_params, [a, b] if (a == "Float" || a == "Vec2" || a == "Vec3") && b == "UInt") => {
            Some(LpsType::Float)
        }
        "lpfn_srandom" if matches!(glsl_params, [a, b] if (a == "Float" || a == "Vec2" || a == "Vec3") && b == "UInt") => {
            Some(LpsType::Float)
        }
        "lpfn_srandom3_tile" if matches!(glsl_params, [a, b, c] if a == "Vec3" && b == "Float" && c == "UInt") => {
            Some(LpsType::Vec3)
        }
        "lpfn_srandom3_vec" if matches!(glsl_params, [a, b] if a == "Vec3" && b == "UInt") => {
            Some(LpsType::Vec3)
        }
        "lpfn_worley" | "lpfn_worley_value" if matches!(glsl_params, [a, b] if (a == "Vec2" || a == "Vec3") && b == "UInt") => {
            Some(LpsType::Float)
        }
        _ => None,
    }
}

fn lpfn_psrdnoise_gradient_type(glsl_params: &[String]) -> Option<LpsType> {
    if matches!(glsl_params, [a, b, c, d, e] if a == "Vec2" && b == "Vec2" && c == "Float" && d == "Vec2" && e == "UInt")
    {
        Some(LpsType::Vec2)
    } else if matches!(glsl_params, [a, b, c, d, e] if a == "Vec3" && b == "Vec3" && c == "Float" && d == "Vec3" && e == "UInt")
    {
        Some(LpsType::Vec3)
    } else {
        None
    }
}

fn psrdnoise_param_types(glsl_params: &[String]) -> Vec<lpir::IrType> {
    let vector_lanes = if glsl_params.first().is_some_and(|p| p == "Vec3") {
        3
    } else {
        2
    };
    let mut param_types = alloc::vec![lpir::IrType::F32; vector_lanes * 2 + 1];
    param_types.push(lpir::IrType::I32);
    param_types.push(lpir::IrType::I32);
    param_types
}

fn same_nonzero_const_expr_tree(lhs: &ParsedExpr, rhs: &ParsedExpr) -> bool {
    const_expr_tree_nonzero(lhs) && const_expr_tree_eq(lhs, rhs)
}

fn const_expr_tree_eq(lhs: &ParsedExpr, rhs: &ParsedExpr) -> bool {
    match (&lhs.kind, &rhs.kind) {
        (ParsedExprKind::BoolLiteral(a), ParsedExprKind::BoolLiteral(b)) => a == b,
        (ParsedExprKind::FloatLiteral(a), ParsedExprKind::FloatLiteral(b)) => a == b,
        (ParsedExprKind::IntLiteral(a), ParsedExprKind::IntLiteral(b)) => a == b,
        (ParsedExprKind::UIntLiteral(a), ParsedExprKind::UIntLiteral(b)) => a == b,
        (
            ParsedExprKind::Call { name: a, args: aa },
            ParsedExprKind::Call { name: b, args: ba },
        ) => {
            a == b
                && aa.len() == ba.len()
                && aa
                    .iter()
                    .zip(ba.iter())
                    .all(|(a, b)| const_expr_tree_eq(a, b))
        }
        (ParsedExprKind::Unary { op: a, expr: ae }, ParsedExprKind::Unary { op: b, expr: be }) => {
            a == b && const_expr_tree_eq(ae, be)
        }
        (
            ParsedExprKind::Binary {
                op: a,
                lhs: al,
                rhs: ar,
            },
            ParsedExprKind::Binary {
                op: b,
                lhs: bl,
                rhs: br,
            },
        ) => a == b && const_expr_tree_eq(al, bl) && const_expr_tree_eq(ar, br),
        _ => false,
    }
}

fn const_expr_tree_nonzero(expr: &ParsedExpr) -> bool {
    match &expr.kind {
        ParsedExprKind::FloatLiteral(value) => *value != 0.0,
        ParsedExprKind::IntLiteral(value) => *value != 0,
        ParsedExprKind::UIntLiteral(value) => *value != 0,
        ParsedExprKind::BoolLiteral(_) => false,
        ParsedExprKind::Call { args, .. } => args.iter().all(const_expr_tree_nonzero),
        ParsedExprKind::Unary { expr, .. } => const_expr_tree_nonzero(expr),
        ParsedExprKind::Binary { lhs, rhs, .. } => {
            const_expr_tree_nonzero(lhs) && const_expr_tree_nonzero(rhs)
        }
        _ => false,
    }
}

fn matrix_vector_multiply_type(lhs: &LpsType, rhs: &LpsType) -> Option<LpsType> {
    if let Some((cols, rows)) = lhs.matrix_dims()
        && scalar_base_type(rhs) == Some(LpsType::Float)
        && scalar_lane_count(rhs) == cols
    {
        return LpsType::vector_type(&LpsType::Float, rows);
    }
    if let Some((cols, rows)) = rhs.matrix_dims()
        && scalar_base_type(lhs) == Some(LpsType::Float)
        && scalar_lane_count(lhs) == rows
    {
        return LpsType::vector_type(&LpsType::Float, cols);
    }
    None
}
