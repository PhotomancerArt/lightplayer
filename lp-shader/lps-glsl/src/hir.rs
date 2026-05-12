use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lps_shared::{
    FnParam, LayoutRules, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier,
};

use crate::body::{
    AssignOp, BinaryOp, IncDecOp, ParsedExpr, ParsedExprKind, ParsedFunctionBody, ParsedStmt,
    UnaryOp, parse_expr_tokens,
};
use crate::{Diagnostic, Span, Token, TopLevelIndex, TypeRef};

#[derive(Debug, Clone)]
pub struct HirModule {
    pub functions: Vec<HirFunction>,
    pub meta: LpsModuleSig,
    pub uniforms: BTreeMap<String, UniformInfo>,
    pub imports: Vec<ImportInfo>,
}

#[derive(Debug, Clone)]
pub struct UniformInfo {
    pub ty: LpsType,
    pub byte_offset: u32,
}

#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub key: ImportKey,
    pub module_name: String,
    pub func_name: String,
    pub param_types: Vec<lpir::IrType>,
    pub return_types: Vec<lpir::IrType>,
    pub lpfn_glsl_params: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportKey {
    Glsl { name: String, argc: usize },
    Lpfn { name: String, glsl_params: String },
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
    pub qualifier: ParamQualifier,
}

#[derive(Debug, Clone)]
pub struct HirFunctionBody {
    pub locals: Vec<HirLocal>,
    pub statements: Vec<HirStmt>,
}

#[derive(Debug, Clone)]
pub struct HirLocal {
    pub name: String,
    pub ty: LpsType,
}

#[derive(Debug, Clone)]
pub enum HirStmt {
    Let {
        local: usize,
        init: HirExpr,
    },
    Assign {
        local: usize,
        value: HirExpr,
    },
    If {
        condition: HirExpr,
        accept: Vec<HirStmt>,
        reject: Vec<HirStmt>,
    },
    For {
        init: Vec<HirStmt>,
        condition: HirExpr,
        continuing: Vec<HirStmt>,
        body: Vec<HirStmt>,
    },
    While {
        condition: HirExpr,
        body: Vec<HirStmt>,
    },
    DoWhile {
        body: Vec<HirStmt>,
        condition: HirExpr,
    },
    Break,
    Continue,
    Expr(HirExpr),
    Return {
        expr: Option<HirExpr>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct HirExpr {
    pub span: Span,
    pub ty: LpsType,
    pub kind: HirExprKind,
}

#[derive(Debug, Clone)]
pub enum HirExprKind {
    BoolLiteral(bool),
    FloatLiteral(f32),
    IntLiteral(i32),
    UIntLiteral(u32),
    Param {
        index: usize,
    },
    Local {
        index: usize,
    },
    Uniform {
        name: String,
        byte_offset: u32,
    },
    Constructor {
        args: Vec<HirExpr>,
    },
    Cast {
        expr: Box<HirExpr>,
    },
    Swizzle {
        base: Box<HirExpr>,
        lanes: Vec<usize>,
    },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    Builtin {
        kind: BuiltinKind,
        args: Vec<HirExpr>,
    },
    UserCall {
        function: usize,
        args: Vec<HirExpr>,
    },
    ImportCall {
        import: ImportKey,
        args: Vec<HirExpr>,
        out: Option<HirOutArg>,
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
    Sequence {
        first: Box<HirExpr>,
        second: Box<HirExpr>,
    },
    Conditional {
        condition: Box<HirExpr>,
        accept: Box<HirExpr>,
        reject: Box<HirExpr>,
    },
    Assign {
        target: HirAssignTarget,
        value: Box<HirExpr>,
    },
    IncDec {
        target: HirAssignTarget,
        op: IncDecOp,
        prefix: bool,
    },
}

#[derive(Debug, Clone)]
pub struct HirOutArg {
    pub local: usize,
    pub ty: LpsType,
    pub arg_index: usize,
}

#[derive(Debug, Clone)]
pub enum HirAssignTarget {
    Param {
        param: usize,
        ty: LpsType,
    },
    Local {
        local: usize,
        ty: LpsType,
    },
    Swizzle {
        local: usize,
        lanes: Vec<usize>,
        ty: LpsType,
    },
    ParamSwizzle {
        param: usize,
        lanes: Vec<usize>,
        ty: LpsType,
    },
    Index {
        local: usize,
        index: Box<HirExpr>,
        ty: LpsType,
    },
    MatrixElement {
        local: usize,
        column: Box<HirExpr>,
        row: Box<HirExpr>,
        ty: LpsType,
    },
}

impl HirAssignTarget {
    fn ty(&self) -> &LpsType {
        match self {
            HirAssignTarget::Param { ty, .. }
            | HirAssignTarget::Local { ty, .. }
            | HirAssignTarget::Swizzle { ty, .. }
            | HirAssignTarget::ParamSwizzle { ty, .. }
            | HirAssignTarget::Index { ty, .. }
            | HirAssignTarget::MatrixElement { ty, .. } => ty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinKind {
    Abs,
    All,
    Any,
    Clamp,
    Equal,
    Floor,
    Fract,
    GreaterThan,
    GreaterThanEqual,
    Length,
    LessThan,
    LessThanEqual,
    Max,
    Min,
    Mix,
    Mod,
    Not,
    NotEqual,
    Smoothstep,
}

#[derive(Debug, Clone)]
struct FunctionSig {
    name: String,
    return_ty: LpsType,
    params: Vec<HirParam>,
}

#[derive(Debug, Clone)]
struct GlobalConst {
    expr: HirExpr,
}

#[derive(Debug, Default)]
struct ImportRegistry {
    imports: BTreeMap<ImportKey, ImportInfo>,
}

impl ImportRegistry {
    fn glsl(&mut self, name: &str, argc: usize) -> ImportKey {
        let key = ImportKey::Glsl {
            name: String::from(name),
            argc,
        };
        self.imports
            .entry(key.clone())
            .or_insert_with(|| ImportInfo {
                key: key.clone(),
                module_name: String::from("glsl"),
                func_name: String::from(name),
                param_types: alloc::vec![lpir::IrType::F32; argc],
                return_types: alloc::vec![lpir::IrType::F32],
                lpfn_glsl_params: None,
            });
        key
    }

    fn lpfn(
        &mut self,
        name: &str,
        glsl_params: String,
        param_types: Vec<lpir::IrType>,
        return_types: Vec<lpir::IrType>,
    ) -> ImportKey {
        let key = ImportKey::Lpfn {
            name: String::from(name),
            glsl_params: glsl_params.clone(),
        };
        self.imports
            .entry(key.clone())
            .or_insert_with(|| ImportInfo {
                key: key.clone(),
                module_name: String::from("lpfn"),
                func_name: format!("{name}_0"),
                param_types,
                return_types,
                lpfn_glsl_params: Some(glsl_params),
            });
        key
    }

    fn into_vec(self) -> Vec<ImportInfo> {
        self.imports.into_values().collect()
    }
}

pub fn build_hir(
    source: &str,
    tokens: &[Token],
    index: &TopLevelIndex,
    bodies: Vec<(String, ParsedFunctionBody)>,
) -> Result<HirModule, Diagnostic> {
    let (uniforms, uniforms_type) = build_uniforms(index)?;
    let functions_sigs = build_function_sigs(index)?;
    let globals = build_global_consts(source, tokens, index, &uniforms, &functions_sigs)?;
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
        let mut ctx = TypeCtx::new(sig, &functions_sigs, &uniforms, &globals, &mut imports);
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

pub fn type_ref_to_lps(ty: &TypeRef) -> Result<LpsType, Diagnostic> {
    type_name_to_lps(&ty.name, ty.span)
}

fn type_name_to_lps(name: &str, span: Span) -> Result<LpsType, Diagnostic> {
    if let Some((element_name, len)) = parse_array_type_name(name) {
        let element = type_name_to_lps(element_name, span)?;
        return Ok(LpsType::Array {
            element: Box::new(element),
            len,
        });
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

fn build_function_sigs(index: &TopLevelIndex) -> Result<Vec<FunctionSig>, Diagnostic> {
    index
        .functions
        .iter()
        .map(|function| {
            Ok(FunctionSig {
                name: function.name.clone(),
                return_ty: type_ref_to_lps(&function.return_ty)?,
                params: function
                    .params
                    .iter()
                    .map(|p| {
                        Ok(HirParam {
                            name: p.name.clone(),
                            ty: type_ref_to_lps(&p.ty)?,
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

fn build_global_consts(
    source: &str,
    tokens: &[Token],
    index: &TopLevelIndex,
    uniforms: &BTreeMap<String, UniformInfo>,
    functions: &[FunctionSig],
) -> Result<BTreeMap<String, GlobalConst>, Diagnostic> {
    let mut globals = BTreeMap::new();
    let mut imports = ImportRegistry::default();
    for konst in &index.consts {
        let ty = type_ref_to_lps(&konst.ty)?;
        let Some(init_span) = konst.init_span else {
            return Err(Diagnostic::error(
                konst.span,
                "const declaration requires initializer",
            ));
        };
        let parsed = parse_expr_tokens(source, tokens, init_span)?;
        let mut ctx = TypeCtx::global_const(functions, uniforms, &globals, &mut imports);
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
        imports: &'a mut ImportRegistry,
    ) -> Self {
        Self {
            params: &function.params,
            functions,
            uniforms,
            globals,
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
        imports: &'a mut ImportRegistry,
    ) -> Self {
        Self {
            params: &[],
            functions,
            uniforms,
            globals,
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
                let ty = type_name_to_lps(ty, *span)?;
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
                    let ty = type_name_to_lps(&declaration.ty, declaration.span)?;
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
            ParsedExprKind::Call { name, args } if is_constructor_name(name) => {
                self.type_constructor(expr.span, name, args)
            }
            ParsedExprKind::Call { name, args } => self.type_call(expr.span, name, args),
            ParsedExprKind::Swizzle { base, fields } => {
                let base = self.type_expr(base)?;
                let (lanes, ty) = swizzle_lanes(expr.span, &base.ty, fields)?;
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
        let target_ty = type_name_to_lps(name, span)?;
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
            let args = args
                .into_iter()
                .zip(sig.params.iter())
                .map(|(arg, param)| self.coerce_expr(arg, &param.ty))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(HirExpr {
                span,
                ty: sig.return_ty.clone(),
                kind: HirExprKind::UserCall { function, args },
            });
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
                let ParsedExprKind::Name(name) = &base.kind else {
                    return Err(Diagnostic::error(
                        expr.span,
                        "unsupported swizzle assignment base",
                    ));
                };
                if let Some(local) = self.resolve_local(name) {
                    let (lanes, ty) = swizzle_lanes(expr.span, &self.locals[local].ty, fields)?;
                    return Ok(HirAssignTarget::Swizzle { local, lanes, ty });
                }
                if let Some((param, p)) = self
                    .params
                    .iter()
                    .enumerate()
                    .find(|(_, p)| p.name.as_deref() == Some(name))
                {
                    let (lanes, ty) = swizzle_lanes(expr.span, &p.ty, fields)?;
                    return Ok(HirAssignTarget::ParamSwizzle { param, lanes, ty });
                }
                Err(Diagnostic::error(
                    expr.span,
                    format!("unknown local `{name}`"),
                ))
            }
            ParsedExprKind::Index { base, index } => match &base.kind {
                ParsedExprKind::Name(name) => {
                    let local = self.resolve_local(name).ok_or_else(|| {
                        Diagnostic::error(expr.span, format!("unknown local `{name}`"))
                    })?;
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
                    Ok(HirAssignTarget::Index {
                        local,
                        index: Box::new(index),
                        ty,
                    })
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

    fn read_assign_target_kind(&self, target: &HirAssignTarget) -> HirExprKind {
        match target {
            HirAssignTarget::Param { param, .. } => HirExprKind::Param { index: *param },
            HirAssignTarget::Local { local, .. } => HirExprKind::Local { index: *local },
            HirAssignTarget::Swizzle { .. }
            | HirAssignTarget::ParamSwizzle { .. }
            | HirAssignTarget::Index { .. }
            | HirAssignTarget::MatrixElement { .. } => {
                unreachable!("compound assignment statement only has simple name targets")
            }
        }
    }

    fn coerce_expr(&mut self, expr: HirExpr, target: &LpsType) -> Result<HirExpr, Diagnostic> {
        coerce_expr(expr, target)
    }

    fn resolve_local(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

fn is_constructor_name(name: &str) -> bool {
    type_name_to_lps(name, Span::new(0, 0)).is_ok()
}

fn builtin_kind(name: &str) -> Option<BuiltinKind> {
    Some(match name {
        "abs" => BuiltinKind::Abs,
        "all" => BuiltinKind::All,
        "any" => BuiltinKind::Any,
        "clamp" => BuiltinKind::Clamp,
        "equal" => BuiltinKind::Equal,
        "floor" => BuiltinKind::Floor,
        "fract" => BuiltinKind::Fract,
        "greaterThan" => BuiltinKind::GreaterThan,
        "greaterThanEqual" => BuiltinKind::GreaterThanEqual,
        "length" => BuiltinKind::Length,
        "lessThan" => BuiltinKind::LessThan,
        "lessThanEqual" => BuiltinKind::LessThanEqual,
        "max" => BuiltinKind::Max,
        "min" => BuiltinKind::Min,
        "mix" => BuiltinKind::Mix,
        "mod" => BuiltinKind::Mod,
        "not" => BuiltinKind::Not,
        "notEqual" => BuiltinKind::NotEqual,
        "smoothstep" => BuiltinKind::Smoothstep,
        _ => return None,
    })
}

fn is_glsl_import(name: &str) -> bool {
    matches!(name, "sin" | "cos" | "exp" | "atan")
}

fn type_glsl_import_args(
    span: Span,
    name: &str,
    args: Vec<HirExpr>,
) -> Result<(Vec<HirExpr>, LpsType), Diagnostic> {
    if matches!(name, "sin" | "cos" | "exp") && args.len() == 1 {
        let arg = args[0].clone();
        let arg_base = scalar_base_type(&arg.ty).unwrap_or_else(|| arg.ty.clone());
        if arg_base == LpsType::Float {
            return Ok((args, arg.ty));
        }
        let arg = coerce_expr(arg, &LpsType::Float)?;
        return Ok((alloc::vec![arg], LpsType::Float));
    }

    let args = args
        .into_iter()
        .map(|arg| coerce_expr(arg, &LpsType::Float))
        .collect::<Result<Vec<_>, _>>()?;
    let ty = match name {
        "atan" if args.len() == 1 || args.len() == 2 => LpsType::Float,
        _ => {
            return Err(Diagnostic::error(
                span,
                format!("unsupported GLSL import signature `{name}`"),
            ));
        }
    };
    Ok((args, ty))
}

fn type_builtin_args(
    span: Span,
    kind: BuiltinKind,
    args: Vec<HirExpr>,
) -> Result<(Vec<HirExpr>, LpsType), Diagnostic> {
    let arity = match kind {
        BuiltinKind::Abs
        | BuiltinKind::All
        | BuiltinKind::Any
        | BuiltinKind::Floor
        | BuiltinKind::Fract
        | BuiltinKind::Length
        | BuiltinKind::Not => 1,
        BuiltinKind::Equal
        | BuiltinKind::GreaterThan
        | BuiltinKind::GreaterThanEqual
        | BuiltinKind::LessThan
        | BuiltinKind::LessThanEqual
        | BuiltinKind::Max
        | BuiltinKind::Min
        | BuiltinKind::Mod
        | BuiltinKind::NotEqual => 2,
        BuiltinKind::Clamp | BuiltinKind::Mix | BuiltinKind::Smoothstep => 3,
    };
    if args.len() != arity {
        return Err(Diagnostic::error(
            span,
            format!("builtin expects {arity} arguments"),
        ));
    }
    match kind {
        BuiltinKind::Abs | BuiltinKind::Floor | BuiltinKind::Fract => {
            let ty = args[0].ty.clone();
            Ok((args, ty))
        }
        BuiltinKind::Length => {
            if scalar_base_type(&args[0].ty) != Some(LpsType::Float) {
                return Err(Diagnostic::error(span, "length expects float lanes"));
            }
            Ok((args, LpsType::Float))
        }
        BuiltinKind::All | BuiltinKind::Any => {
            let arg = coerce_expr(args[0].clone(), &args[0].ty)?;
            if scalar_base_type(&arg.ty) != Some(LpsType::Bool) {
                return Err(Diagnostic::error(span, "all/any expects bool lanes"));
            }
            Ok((alloc::vec![arg], LpsType::Bool))
        }
        BuiltinKind::Not => {
            let arg = args[0].clone();
            let ty = arg.ty.clone();
            if scalar_base_type(&ty) != Some(LpsType::Bool) {
                return Err(Diagnostic::error(span, "not expects bool lanes"));
            }
            Ok((alloc::vec![arg], ty))
        }
        BuiltinKind::Max | BuiltinKind::Min | BuiltinKind::Mod => {
            let (a, b, ty) = coerce_arithmetic_pair(span, args[0].clone(), args[1].clone())?;
            Ok((alloc::vec![a, b], ty))
        }
        BuiltinKind::Equal
        | BuiltinKind::GreaterThan
        | BuiltinKind::GreaterThanEqual
        | BuiltinKind::LessThan
        | BuiltinKind::LessThanEqual
        | BuiltinKind::NotEqual => {
            let (a, b, ty) = coerce_comparison_pair(span, args[0].clone(), args[1].clone())?;
            Ok((alloc::vec![a, b], ty))
        }
        BuiltinKind::Clamp | BuiltinKind::Smoothstep => {
            let (a, b, ty_ab) = coerce_arithmetic_pair(span, args[0].clone(), args[1].clone())?;
            let c = coerce_expr(args[2].clone(), &ty_ab).or_else(|_| {
                let (_, c, _) = coerce_arithmetic_pair(span, a.clone(), args[2].clone())?;
                Ok::<_, Diagnostic>(c)
            })?;
            let ty = vector_dominant_type(&[&a.ty, &b.ty, &c.ty])
                .ok_or_else(|| Diagnostic::error(span, "unsupported builtin argument types"))?;
            Ok((
                alloc::vec![
                    coerce_expr(a, &ty)?,
                    coerce_expr(b, &ty)?,
                    coerce_expr(c, &ty)?
                ],
                ty,
            ))
        }
        BuiltinKind::Mix => {
            let (x, y, ty) = coerce_arithmetic_pair(span, args[0].clone(), args[1].clone())?;
            let a = if scalar_lane_count(&args[2].ty) == 1 {
                coerce_expr(args[2].clone(), &LpsType::Float)?
            } else {
                coerce_expr(args[2].clone(), &ty)?
            };
            Ok((alloc::vec![x, y, a], ty))
        }
    }
}

fn coerce_constructor_args(
    span: Span,
    target_ty: &LpsType,
    args: Vec<HirExpr>,
) -> Result<Vec<HirExpr>, Diagnostic> {
    let expected_lanes = scalar_lane_count(target_ty);
    let actual_lanes = args
        .iter()
        .map(|arg| scalar_lane_count(&arg.ty))
        .sum::<usize>();
    if actual_lanes >= expected_lanes {
        let expected_scalar = scalar_base_type(target_ty).unwrap_or_else(|| target_ty.clone());
        return args
            .into_iter()
            .map(|arg| {
                let arg_scalar = scalar_base_type(&arg.ty).unwrap_or_else(|| arg.ty.clone());
                if arg_scalar == expected_scalar {
                    Ok(arg)
                } else {
                    let target = if scalar_lane_count(&arg.ty) > 1 {
                        LpsType::vector_type(&expected_scalar, scalar_lane_count(&arg.ty))
                            .unwrap_or_else(|| expected_scalar.clone())
                    } else {
                        expected_scalar.clone()
                    };
                    coerce_expr(arg, &target)
                }
            })
            .collect();
    }
    if args.len() == 1 && expected_lanes > 1 && scalar_lane_count(&args[0].ty) == 1 {
        return Ok(args);
    }
    Err(Diagnostic::error(
        span,
        format!(
            "constructor for {:?} expects {expected_lanes} scalar lanes, got {actual_lanes}",
            target_ty
        ),
    ))
}

fn coerce_arithmetic_pair(
    span: Span,
    lhs: HirExpr,
    rhs: HirExpr,
) -> Result<(HirExpr, HirExpr, LpsType), Diagnostic> {
    let ty = vector_dominant_type(&[&lhs.ty, &rhs.ty])
        .ok_or_else(|| Diagnostic::error(span, "unsupported arithmetic operand types"))?;
    Ok((coerce_expr(lhs, &ty)?, coerce_expr(rhs, &ty)?, ty))
}

fn coerce_comparison_pair(
    span: Span,
    lhs: HirExpr,
    rhs: HirExpr,
) -> Result<(HirExpr, HirExpr, LpsType), Diagnostic> {
    let ty = vector_dominant_type(&[&lhs.ty, &rhs.ty])
        .ok_or_else(|| Diagnostic::error(span, "unsupported comparison operand types"))?;
    let result_ty = comparison_result_type(&ty)
        .ok_or_else(|| Diagnostic::error(span, "unsupported comparison result type"))?;
    Ok((coerce_expr(lhs, &ty)?, coerce_expr(rhs, &ty)?, result_ty))
}

fn coerce_expr(expr: HirExpr, target: &LpsType) -> Result<HirExpr, Diagnostic> {
    if expr.ty == *target {
        return Ok(expr);
    }
    if scalar_lane_count(&expr.ty) == 1 && scalar_lane_count(target) > 1 {
        let scalar = scalar_base_type(target).unwrap_or_else(|| target.clone());
        let expr = coerce_expr(expr, &scalar)?;
        return Ok(HirExpr {
            span: expr.span,
            ty: target.clone(),
            kind: HirExprKind::Constructor {
                args: alloc::vec![expr],
            },
        });
    }
    if scalar_lane_count(&expr.ty) == scalar_lane_count(target)
        && scalar_base_type(&expr.ty).is_some()
        && scalar_base_type(target).is_some()
    {
        return Ok(HirExpr {
            span: expr.span,
            ty: target.clone(),
            kind: HirExprKind::Cast {
                expr: Box::new(expr),
            },
        });
    }
    match (&expr.ty, target) {
        (LpsType::Int, LpsType::Float)
        | (LpsType::UInt, LpsType::Float)
        | (LpsType::Float, LpsType::Int)
        | (LpsType::Float, LpsType::UInt)
        | (LpsType::Int, LpsType::UInt)
        | (LpsType::UInt, LpsType::Int) => Ok(HirExpr {
            span: expr.span,
            ty: target.clone(),
            kind: HirExprKind::Cast {
                expr: Box::new(expr),
            },
        }),
        (LpsType::Bool, LpsType::Float)
        | (LpsType::Bool, LpsType::Int)
        | (LpsType::Bool, LpsType::UInt)
        | (LpsType::Float, LpsType::Bool)
        | (LpsType::Int, LpsType::Bool)
        | (LpsType::UInt, LpsType::Bool) => Ok(HirExpr {
            span: expr.span,
            ty: target.clone(),
            kind: HirExprKind::Cast {
                expr: Box::new(expr),
            },
        }),
        (LpsType::Bool, LpsType::Bool) => Ok(expr),
        _ => Err(Diagnostic::error(
            expr.span,
            format!("cannot coerce {:?} to {:?}", expr.ty, target),
        )),
    }
}

fn vector_dominant_type(types: &[&LpsType]) -> Option<LpsType> {
    if let Some(matrix) = types.iter().find(|ty| ty.is_matrix()) {
        if types
            .iter()
            .all(|ty| **ty == **matrix || **ty == LpsType::Float)
        {
            return Some((*matrix).clone());
        }
        return None;
    }
    let mut lanes = 1usize;
    let mut base = LpsType::Bool;
    for ty in types {
        let ty_base = scalar_base_type(ty)?;
        if ty_base == LpsType::Float {
            base = LpsType::Float;
        } else if ty_base == LpsType::UInt && base != LpsType::Float {
            base = LpsType::UInt;
        } else if ty_base == LpsType::Int && base == LpsType::Bool {
            base = LpsType::Int;
        } else if ty_base != LpsType::Int && ty_base != LpsType::Bool {
            return None;
        }
        lanes = lanes.max(scalar_lane_count(ty));
    }
    if lanes == 1 {
        Some(base)
    } else {
        LpsType::vector_type(&base, lanes)
    }
}

fn comparison_result_type(operand_ty: &LpsType) -> Option<LpsType> {
    match scalar_lane_count(operand_ty) {
        1 => Some(LpsType::Bool),
        lanes => LpsType::vector_type(&LpsType::Bool, lanes),
    }
}

fn zero_expr(span: Span, ty: &LpsType) -> Result<HirExpr, Diagnostic> {
    let scalar = match scalar_base_type(ty).unwrap_or_else(|| ty.clone()) {
        LpsType::Float => HirExpr {
            span,
            ty: LpsType::Float,
            kind: HirExprKind::FloatLiteral(0.0),
        },
        LpsType::Int => HirExpr {
            span,
            ty: LpsType::Int,
            kind: HirExprKind::IntLiteral(0),
        },
        LpsType::UInt => HirExpr {
            span,
            ty: LpsType::UInt,
            kind: HirExprKind::UIntLiteral(0),
        },
        LpsType::Bool => HirExpr {
            span,
            ty: LpsType::Bool,
            kind: HirExprKind::BoolLiteral(false),
        },
        _ => return Err(Diagnostic::error(span, "unsupported zero initializer type")),
    };
    coerce_expr(scalar, ty)
}

fn swizzle_lanes(
    span: Span,
    ty: &LpsType,
    fields: &str,
) -> Result<(Vec<usize>, LpsType), Diagnostic> {
    let count = scalar_lane_count(ty);
    if count < 2 {
        return Err(Diagnostic::error(span, "swizzle requires vector base"));
    }
    let mut lanes = Vec::new();
    for ch in fields.chars() {
        let lane = match ch {
            'x' | 'r' | 's' => 0,
            'y' | 'g' | 't' => 1,
            'z' | 'b' | 'p' => 2,
            'w' | 'a' | 'q' => 3,
            _ => return Err(Diagnostic::error(span, "unsupported swizzle field")),
        };
        if lane >= count {
            return Err(Diagnostic::error(span, "swizzle lane out of range"));
        }
        lanes.push(lane);
    }
    let base = scalar_base_type(ty).ok_or_else(|| Diagnostic::error(span, "swizzle base type"))?;
    let out_ty = if lanes.len() == 1 {
        base
    } else {
        LpsType::vector_type(&base, lanes.len())
            .ok_or_else(|| Diagnostic::error(span, "unsupported swizzle width"))?
    };
    Ok((lanes, out_ty))
}

fn is_comparison(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
    )
}

fn is_logical(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::LogicalXor
    )
}

fn glsl_param_token(ty: &LpsType, span: Span) -> Result<String, Diagnostic> {
    Ok(match ty {
        LpsType::Float => String::from("Float"),
        LpsType::Int => String::from("Int"),
        LpsType::UInt => String::from("UInt"),
        LpsType::Vec2 => String::from("Vec2"),
        LpsType::Vec3 => String::from("Vec3"),
        LpsType::Vec4 => String::from("Vec4"),
        other => {
            return Err(Diagnostic::error(
                span,
                format!("unsupported LPFN parameter type {other:?}"),
            ));
        }
    })
}

pub fn scalar_lane_count(ty: &LpsType) -> usize {
    match ty {
        LpsType::Void => 0,
        LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => 1,
        LpsType::Array { element, len } => scalar_lane_count(element).saturating_mul(*len as usize),
        _ => ty
            .component_count()
            .or_else(|| ty.matrix_element_count())
            .unwrap_or(0),
    }
}

pub fn scalar_base_type(ty: &LpsType) -> Option<LpsType> {
    if let LpsType::Array { element, .. } = ty {
        scalar_base_type(element)
    } else if ty.is_matrix() {
        Some(LpsType::Float)
    } else if ty.is_vector() {
        ty.vector_base_type()
    } else if ty.is_scalar() {
        Some(ty.clone())
    } else {
        None
    }
}

pub fn scalar_ir_types(ty: &LpsType) -> Result<Vec<lpir::IrType>, Diagnostic> {
    if *ty == LpsType::Void {
        return Ok(Vec::new());
    }
    let Some(base) = scalar_base_type(ty) else {
        return Err(Diagnostic::error(
            Span::new(0, 0),
            format!("M3 lps-glsl cannot scalarize type {ty:?}"),
        ));
    };
    let lane = match base {
        LpsType::Float => lpir::IrType::F32,
        LpsType::Int | LpsType::UInt | LpsType::Bool => lpir::IrType::I32,
        _ => {
            return Err(Diagnostic::error(
                Span::new(0, 0),
                format!("M3 lps-glsl cannot scalarize type {ty:?}"),
            ));
        }
    };
    Ok(alloc::vec![lane; scalar_lane_count(ty)])
}
