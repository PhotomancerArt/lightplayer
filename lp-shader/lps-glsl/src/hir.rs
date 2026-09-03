use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lp_collection::VecMap;

use lps_shared::{FnParam, LayoutRules, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, StructMember};

use crate::body::{
    BinaryOp, ParsedExpr, ParsedExprKind, ParsedFunctionBody, UnaryOp, parse_expr_tokens,
};
use crate::{CompileOptions, Diagnostic, Span, Token, TopLevelIndex, TypeRef};

mod arena;
mod array_size;
mod builtin;
mod builtin_out;
mod coerce;
mod const_fold;
mod function;
mod place;
mod scalar;
mod shape;
mod symbols;
mod typeck;
mod types;
mod typing;

pub(crate) use arena::{ExprId, ExprList, HirArena, PlaceId, TypeId};
use array_size::{ArraySizeConsts, eval_array_size_expr};
// The builtin checks double as the test-only ground truth for the
// editor-facing [`crate::builtin_inventory`] (its drift tests tie the
// inventory to them).
#[cfg(test)]
pub(crate) use builtin::{builtin_has_out_args, builtin_kind, check_builtin_arity, is_glsl_import};
use function::{FunctionSig, GlobalConst, ImportRegistry};
pub(crate) use place::{
    HirPlace, PlaceRoot, PlaceSegment, field_type, index_element_type, swizzle_type,
};
pub use symbols::{
    FnParamSymbol, FnSymbol, StructSymbol, SymbolAnalysis, VarSymbol, analyze_symbols,
};
use typeck::TypeCtx;
use types::StructTypes;
pub use types::{
    BuiltinKind, GlobalInfo, HirExprKind, HirExprRef, HirFunction, HirFunctionBody,
    HirFunctionParam, HirModule, HirOutArg, HirParam, HirStmt, HirTextureOperand,
    HirUserCallWriteback, ImportId, ImportInfo, ImportKey, UniformInfo,
};
pub use typing::{scalar_base_type, scalar_ir_types, scalar_lane_count};

#[derive(Debug)]
pub struct HirBuildJob<'src> {
    source: &'src str,
    index: TopLevelIndex,
    options: CompileOptions,
    state: HirBuildState<'src>,
}

#[derive(Debug)]
enum HirBuildState<'src> {
    /// The token tape lives here, not on the job: only the header step
    /// reads it (array-size consts, global initialisers), and it is the
    /// largest thing the build holds — 24 B per token, ~16 KB for meteor's
    /// sim shader — so it is dropped with this state instead of staying
    /// resident under every function's typing.
    Header {
        tokens: Vec<Token>,
        bodies: Vec<(String, ParsedFunctionBody<'src>)>,
    },
    Functions(Box<HirBuildFunctionState<'src>>),
    ShaderInit(Box<HirBuildFunctionState<'src>>),
    Done,
}

#[derive(Debug)]
struct HirBuildFunctionState<'src> {
    array_size_consts: ArraySizeConsts,
    structs: StructTypes,
    uniforms: VecMap<String, UniformInfo>,
    uniforms_type: Option<LpsType>,
    global_vars: VecMap<String, GlobalInfo>,
    globals_type: Option<LpsType>,
    global_inits: Vec<GlobalInit<'src>>,
    functions_sigs: Vec<FunctionSig>,
    imports: ImportRegistry,
    globals: VecMap<String, GlobalConst>,
    body_map: VecMap<String, ParsedFunctionBody<'src>>,
    functions: Vec<HirFunction>,
    /// Set when [`synthesize_shader_init`] produced the trailing synthetic
    /// function; the module signature is built from the sig table in
    /// [`HirBuildFunctionState::finish`], so this is all the extra
    /// bookkeeping it needs.
    has_shader_init: bool,
    next_function: usize,
}

pub enum HirBuildStepResult {
    Pending,
    Finished(HirModule),
}

impl<'src> HirBuildJob<'src> {
    pub fn new(
        source: &'src str,
        tokens: Vec<Token>,
        index: TopLevelIndex,
        bodies: Vec<(String, ParsedFunctionBody<'src>)>,
        options: CompileOptions,
    ) -> Self {
        Self {
            source,
            index,
            options,
            state: HirBuildState::Header { tokens, bodies },
        }
    }

    pub fn step(&mut self) -> Result<HirBuildStepResult, Diagnostic> {
        let state = core::mem::replace(&mut self.state, HirBuildState::Done);
        match state {
            HirBuildState::Header { tokens, bodies } => {
                let (array_size_consts, const_init_cache) =
                    build_array_size_consts(self.source, &tokens, &self.index)?;
                let structs = build_struct_types(&self.index, &array_size_consts)?;
                let (uniforms, uniforms_type, uniforms_size) =
                    build_uniforms(&self.index, &structs, &array_size_consts)?;
                let (global_vars, globals_type, global_inits) = build_global_vars(
                    self.source,
                    &tokens,
                    &self.index,
                    &structs,
                    &array_size_consts,
                    uniforms_size,
                )?;
                let functions_sigs =
                    build_function_sigs(&self.index, &structs, &array_size_consts)?;
                let mut imports = ImportRegistry::default();
                let globals = build_global_consts(
                    self.source,
                    &tokens,
                    &self.index,
                    &uniforms,
                    &global_vars,
                    &functions_sigs,
                    &structs,
                    &array_size_consts,
                    &mut imports,
                    &self.options.texture_specs,
                    const_init_cache,
                )?;
                self.state = HirBuildState::Functions(Box::new(HirBuildFunctionState {
                    array_size_consts,
                    structs,
                    uniforms,
                    uniforms_type,
                    global_vars,
                    globals_type,
                    global_inits,
                    functions_sigs,
                    imports,
                    globals,
                    body_map: function_body_map(bodies),
                    functions: Vec::new(),
                    has_shader_init: false,
                    next_function: 0,
                }));
                Ok(HirBuildStepResult::Pending)
            }
            HirBuildState::Functions(mut state) => {
                if state.next_function < state.functions_sigs.len() {
                    self.type_next_function(&mut state)?;
                    state.next_function += 1;
                    self.state = HirBuildState::Functions(state);
                } else {
                    self.state = HirBuildState::ShaderInit(state);
                }
                Ok(HirBuildStepResult::Pending)
            }
            HirBuildState::ShaderInit(mut state) => {
                if let Some(init) = synthesize_shader_init(
                    &state.global_inits,
                    &state.functions_sigs,
                    &state.uniforms,
                    &state.globals,
                    &state.global_vars,
                    &state.structs,
                    &state.array_size_consts,
                    &mut state.imports,
                    &self.options.texture_specs,
                )? {
                    state.has_shader_init = true;
                    state.functions.push(init);
                }
                self.state = HirBuildState::Done;
                Ok(HirBuildStepResult::Finished(state.finish(&self.options)))
            }
            HirBuildState::Done => Err(Diagnostic::error(
                Span::new(0, 0),
                "HIR build job already finished",
            )),
        }
    }

    fn type_next_function(
        &self,
        state: &mut HirBuildFunctionState<'src>,
    ) -> Result<(), Diagnostic> {
        let function_index = state.next_function;
        let sig = &state.functions_sigs[function_index];
        let decl = &self.index.functions[function_index];
        let parsed_body = state
            .body_map
            .remove(sig.name.as_str())
            .ok_or_else(|| Diagnostic::error(decl.body_span, "missing parsed function body"))?;
        let mut ctx = TypeCtx::new(
            sig,
            &state.functions_sigs,
            &state.uniforms,
            &state.globals,
            &state.global_vars,
            &state.structs,
            &state.array_size_consts,
            &mut state.imports,
            &self.options.texture_specs,
        );
        let mut body = ctx.type_block(&parsed_body.statements, &sig.return_ty)?;
        let (return_ty, params) = intern_signature(&mut body.arena, sig);
        state.functions.push(HirFunction {
            name: sig.name.clone(),
            return_ty,
            params,
            body,
        });
        Ok(())
    }
}

impl HirBuildFunctionState<'_> {
    fn finish(self, options: &CompileOptions) -> HirModule {
        HirModule {
            functions: self.functions,
            meta: LpsModuleSig {
                functions: module_function_sigs(self.functions_sigs, self.has_shader_init),
                uniforms_type: self.uniforms_type,
                globals_type: self.globals_type,
                texture_specs: options.texture_specs.clone(),
                ..Default::default()
            },
            uniforms: self.uniforms,
            globals: self.global_vars,
            imports: self.imports.into_vec(),
            texture_specs: options.texture_specs.clone(),
            texel_fetch_bounds: options.texel_fetch_bounds,
        }
    }
}

/// The module signature list, built by consuming the sig table.
///
/// [`FunctionSig`] and [`LpsFnSig`] carry the same name, return type and
/// parameters. Nothing reads the sig table once every function is typed, so
/// the signature list moves that data out instead of cloning it per function
/// and per parameter — the [`HirFunction`] copy is the one that has to own
/// its own.
fn module_function_sigs(sigs: Vec<FunctionSig>, has_shader_init: bool) -> Vec<LpsFnSig> {
    let mut meta = sigs
        .into_iter()
        .map(|sig| LpsFnSig {
            name: sig.name,
            return_type: sig.return_ty,
            parameters: sig
                .params
                .into_iter()
                .map(|p| FnParam {
                    name: p.name.unwrap_or_default(),
                    ty: p.ty,
                    qualifier: p.qualifier,
                })
                .collect(),
            kind: LpsFnKind::UserDefined,
        })
        .collect::<Vec<_>>();
    if has_shader_init {
        meta.push(LpsFnSig {
            name: String::from("__shader_init"),
            return_type: LpsType::Void,
            parameters: Vec::new(),
            kind: LpsFnKind::Synthetic,
        });
    }
    meta
}

/// A function's signature types as ids into its own arena's type table.
fn intern_signature(arena: &mut HirArena, sig: &FunctionSig) -> (TypeId, Vec<HirFunctionParam>) {
    let return_ty = arena.intern(sig.return_ty.clone());
    let params = sig
        .params
        .iter()
        .map(|p| HirFunctionParam {
            ty: arena.intern(p.ty.clone()),
            qualifier: p.qualifier,
        })
        .collect();
    (return_ty, params)
}

#[allow(
    dead_code,
    reason = "kept as the synchronous HIR builder for tests and future callers"
)]
pub fn build_hir<'src>(
    source: &'src str,
    tokens: &[Token],
    index: &TopLevelIndex,
    bodies: Vec<(String, ParsedFunctionBody<'src>)>,
    options: &CompileOptions,
) -> Result<HirModule, Diagnostic> {
    let (array_size_consts, const_init_cache) = build_array_size_consts(source, tokens, index)?;
    let structs = build_struct_types(index, &array_size_consts)?;
    let (uniforms, uniforms_type, uniforms_size) =
        build_uniforms(index, &structs, &array_size_consts)?;
    let (global_vars, globals_type, global_inits) = build_global_vars(
        source,
        tokens,
        index,
        &structs,
        &array_size_consts,
        uniforms_size,
    )?;
    let functions_sigs = build_function_sigs(index, &structs, &array_size_consts)?;
    let mut imports = ImportRegistry::default();
    let globals = build_global_consts(
        source,
        tokens,
        index,
        &uniforms,
        &global_vars,
        &functions_sigs,
        &structs,
        &array_size_consts,
        &mut imports,
        &options.texture_specs,
        const_init_cache,
    )?;
    let body_map = function_body_map(bodies);
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
            &global_vars,
            &structs,
            &array_size_consts,
            &mut imports,
            &options.texture_specs,
        );
        let mut body = ctx.type_block(&parsed_body.statements, &sig.return_ty)?;
        let (return_ty, params) = intern_signature(&mut body.arena, sig);
        functions.push(HirFunction {
            name: sig.name.clone(),
            return_ty,
            params,
            body,
        });
    }

    if let Some(init) = synthesize_shader_init(
        &global_inits,
        &functions_sigs,
        &uniforms,
        &globals,
        &global_vars,
        &structs,
        &array_size_consts,
        &mut imports,
        &options.texture_specs,
    )? {
        function_meta.push(LpsFnSig {
            name: String::from("__shader_init"),
            return_type: LpsType::Void,
            parameters: Vec::new(),
            kind: LpsFnKind::Synthetic,
        });
        functions.push(init);
    }

    Ok(HirModule {
        functions,
        meta: LpsModuleSig {
            functions: function_meta,
            uniforms_type,
            globals_type,
            texture_specs: options.texture_specs.clone(),
            ..Default::default()
        },
        uniforms,
        globals: global_vars,
        imports: imports.into_vec(),
        texture_specs: options.texture_specs.clone(),
        texel_fetch_bounds: options.texel_fetch_bounds,
    })
}

fn type_ref_to_lps_with_structs(
    ty: &TypeRef,
    structs: &StructTypes,
    array_size_consts: &ArraySizeConsts,
) -> Result<LpsType, Diagnostic> {
    type_name_to_lps_with_structs(&ty.name, ty.span, structs, array_size_consts)
}

fn type_name_to_lps_with_structs(
    name: &str,
    span: Span,
    structs: &StructTypes,
    array_size_consts: &ArraySizeConsts,
) -> Result<LpsType, Diagnostic> {
    if let Some((base_name, lens)) = parse_array_type_name(name, array_size_consts) {
        if lens.iter().any(Option::is_none) {
            return Err(Diagnostic::error(
                span,
                "array length must be specified here",
            ));
        }
        return fixed_array_type(base_name, &lens, span, structs);
    }
    scalar_or_struct_type_name_to_lps(name, span, structs)
}

fn scalar_or_struct_type_name_to_lps(
    name: &str,
    span: Span,
    structs: &StructTypes,
) -> Result<LpsType, Diagnostic> {
    scalar_or_struct_type_name_lookup(name, structs)
        .ok_or_else(|| Diagnostic::error(span, format!("unsupported type `{name}`")))
}

/// Type-name lookup that never builds a [`Diagnostic`].
///
/// The failure message of [`scalar_or_struct_type_name_to_lps`] costs a
/// `format!` allocation, and the probe-style caller
/// (`TypeCtx::is_constructor_name`, via [`is_scalar_or_struct_type_name`])
/// discards it for every non-type callee — which is every builtin and every
/// user function call in the shader.
fn scalar_or_struct_type_name_lookup(name: &str, structs: &StructTypes) -> Option<LpsType> {
    if let Some(ty) = structs.get(name) {
        return Some(ty.clone());
    }
    scalar_type_name_to_lps(name)
}

fn scalar_type_name_to_lps(name: &str) -> Option<LpsType> {
    match name {
        "void" => Some(LpsType::Void),
        "float" => Some(LpsType::Float),
        "int" => Some(LpsType::Int),
        "uint" => Some(LpsType::UInt),
        "bool" => Some(LpsType::Bool),
        "vec2" => Some(LpsType::Vec2),
        "vec3" => Some(LpsType::Vec3),
        "vec4" => Some(LpsType::Vec4),
        "ivec2" => Some(LpsType::IVec2),
        "ivec3" => Some(LpsType::IVec3),
        "ivec4" => Some(LpsType::IVec4),
        "uvec2" => Some(LpsType::UVec2),
        "uvec3" => Some(LpsType::UVec3),
        "uvec4" => Some(LpsType::UVec4),
        "bvec2" => Some(LpsType::BVec2),
        "bvec3" => Some(LpsType::BVec3),
        "bvec4" => Some(LpsType::BVec4),
        "mat2" => Some(LpsType::Mat2),
        "mat3" => Some(LpsType::Mat3),
        "mat4" => Some(LpsType::Mat4),
        "sampler2D" | "texture2D" => Some(LpsType::Texture2D),
        _ => None,
    }
}

/// Whether `name` names a scalar or struct type, allocation-free.
fn is_scalar_or_struct_type_name(name: &str, structs: &StructTypes) -> bool {
    structs.contains_key(name) || scalar_type_name_to_lps(name).is_some()
}

fn fixed_array_type(
    base_name: &str,
    lens: &[Option<u32>],
    span: Span,
    structs: &StructTypes,
) -> Result<LpsType, Diagnostic> {
    let mut ty = scalar_or_struct_type_name_to_lps(base_name, span, structs)?;
    for len in lens.iter().rev() {
        let Some(len) = len else {
            return Err(Diagnostic::error(
                span,
                "array length must be specified here",
            ));
        };
        ty = LpsType::Array {
            element: Box::new(ty),
            len: *len,
        };
    }
    Ok(ty)
}

fn parse_array_type_name<'a>(
    name: &'a str,
    array_size_consts: &ArraySizeConsts,
) -> Option<(&'a str, Vec<Option<u32>>)> {
    let open = name.find('[')?;
    let base = &name[..open];
    let mut rest = &name[open..];
    let mut lens = Vec::new();
    while let Some(after_open) = rest.strip_prefix('[') {
        let close = after_open.find(']')?;
        let len_text = &after_open[..close];
        let len = if len_text.is_empty() {
            None
        } else {
            Some(eval_array_size_expr(len_text, array_size_consts)?)
        };
        lens.push(len);
        rest = &after_open[close + 1..];
    }
    if rest.is_empty() {
        Some((base, lens))
    } else {
        None
    }
}

fn infer_array_decl_type(
    span: Span,
    base: &LpsType,
    lens: &[Option<u32>],
    init_ty: &LpsType,
) -> Result<LpsType, Diagnostic> {
    let (init_base, init_lens) = array_base_and_lens(init_ty)
        .ok_or_else(|| Diagnostic::error(span, "unsized array initializer must have array type"))?;
    if *base != init_base || lens.len() != init_lens.len() {
        return Err(Diagnostic::error(
            span,
            "unsized array initializer type mismatch",
        ));
    }
    let mut resolved = Vec::new();
    for (decl_len, init_len) in lens.iter().zip(init_lens.iter()) {
        if let Some(decl_len) = decl_len
            && decl_len != init_len
        {
            return Err(Diagnostic::error(span, "array initializer length mismatch"));
        }
        resolved.push(Some(*init_len));
    }
    fixed_array_from_base(base.clone(), &resolved, span)
}

fn resolve_init_list_lens(
    span: Span,
    lens: &[Option<u32>],
    init: &ParsedExpr<'_>,
) -> Result<Vec<Option<u32>>, Diagnostic> {
    let Some((first_len, rest_lens)) = lens.split_first() else {
        return Ok(Vec::new());
    };
    let ParsedExprKind::InitList { elements } = &init.kind else {
        return fixed_init_lens(span, lens);
    };
    let resolved_len = first_len.unwrap_or(elements.len() as u32);
    let mut resolved = alloc::vec![Some(resolved_len)];
    if !rest_lens.is_empty() {
        if let Some(first) = elements.first() {
            resolved.extend(resolve_init_list_lens(span, rest_lens, first)?);
        } else {
            resolved.extend(fixed_init_lens(span, rest_lens)?);
        }
    }
    Ok(resolved)
}

fn fixed_init_lens(span: Span, lens: &[Option<u32>]) -> Result<Vec<Option<u32>>, Diagnostic> {
    if lens.iter().any(Option::is_none) {
        return Err(Diagnostic::error(
            span,
            "unsized array initializer requires elements",
        ));
    }
    Ok(lens.to_vec())
}

fn infer_array_constructor_type(
    span: Span,
    base: LpsType,
    lens: &[Option<u32>],
    arena: &HirArena,
    args: &[ExprId],
) -> Result<LpsType, Diagnostic> {
    let Some((first_len, rest_lens)) = lens.split_first() else {
        return Ok(base);
    };
    let len = first_len.unwrap_or(args.len() as u32);
    let element = if rest_lens.is_empty() {
        base
    } else if rest_lens.iter().all(Option::is_some) {
        fixed_array_from_base(base, rest_lens, span)?
    } else {
        let first_arg = args.first().ok_or_else(|| {
            Diagnostic::error(
                span,
                "unsized array constructor requires at least one argument",
            )
        })?;
        infer_array_decl_type(span, &base, rest_lens, arena.expr_ty(*first_arg))?
    };
    Ok(LpsType::Array {
        element: Box::new(element),
        len,
    })
}

fn fixed_array_from_base(
    base: LpsType,
    lens: &[Option<u32>],
    span: Span,
) -> Result<LpsType, Diagnostic> {
    let mut ty = base;
    for len in lens.iter().rev() {
        let Some(len) = len else {
            return Err(Diagnostic::error(
                span,
                "array length must be specified here",
            ));
        };
        ty = LpsType::Array {
            element: Box::new(ty),
            len: *len,
        };
    }
    Ok(ty)
}

fn array_base_and_lens(ty: &LpsType) -> Option<(LpsType, Vec<u32>)> {
    let mut lens = Vec::new();
    let mut current = ty;
    while let LpsType::Array { element, len } = current {
        lens.push(*len);
        current = element;
    }
    if lens.is_empty() {
        None
    } else {
        Some((current.clone(), lens))
    }
}

/// Parses each `int`/`uint` const initializer once and returns both the
/// folded array-size table and the parsed expressions themselves (aligned
/// index-for-index with `index.consts`, `None` for entries this pass didn't
/// touch). [`build_global_consts`] consumes the cache instead of re-parsing
/// the same spans.
fn build_array_size_consts<'src>(
    source: &'src str,
    tokens: &[Token],
    index: &TopLevelIndex,
) -> Result<(ArraySizeConsts, Vec<Option<ParsedExpr<'src>>>), Diagnostic> {
    let mut consts = VecMap::new();
    let mut parsed_cache = Vec::with_capacity(index.consts.len());
    for konst in &index.consts {
        if !matches!(konst.ty.name.as_str(), "int" | "uint") {
            parsed_cache.push(None);
            continue;
        }
        let Some(init_span) = konst.init_span else {
            parsed_cache.push(None);
            continue;
        };
        let parsed = parse_expr_tokens(source, tokens, init_span)?;
        if let Some(value) = eval_parsed_array_size_expr(&parsed, &consts) {
            consts.insert(konst.name.clone(), value);
        }
        parsed_cache.push(Some(parsed));
    }
    Ok((consts, parsed_cache))
}

fn eval_parsed_array_size_expr(expr: &ParsedExpr<'_>, consts: &ArraySizeConsts) -> Option<u32> {
    let value = eval_parsed_const_int(expr, consts)?;
    if value < 0 {
        return None;
    }
    u32::try_from(value).ok()
}

fn eval_parsed_const_int(expr: &ParsedExpr<'_>, consts: &ArraySizeConsts) -> Option<i64> {
    match &expr.kind {
        ParsedExprKind::IntLiteral(value) => Some(i64::from(*value)),
        ParsedExprKind::UIntLiteral(value) => Some(i64::from(*value)),
        ParsedExprKind::Name(name) => consts.get(*name).copied().map(i64::from),
        ParsedExprKind::Unary {
            op: UnaryOp::Neg,
            expr,
        } => eval_parsed_const_int(expr, consts)?.checked_neg(),
        ParsedExprKind::Unary { .. } => None,
        ParsedExprKind::Binary { op, lhs, rhs } => {
            let lhs = eval_parsed_const_int(lhs, consts)?;
            let rhs = eval_parsed_const_int(rhs, consts)?;
            match op {
                BinaryOp::Add => lhs.checked_add(rhs),
                BinaryOp::Sub => lhs.checked_sub(rhs),
                BinaryOp::Mul => lhs.checked_mul(rhs),
                BinaryOp::Div => lhs.checked_div(rhs),
                BinaryOp::Mod => lhs.checked_rem(rhs),
                _ => None,
            }
        }
        _ => None,
    }
}

fn build_struct_types(
    index: &TopLevelIndex,
    array_size_consts: &ArraySizeConsts,
) -> Result<StructTypes, Diagnostic> {
    let mut structs = VecMap::new();
    for decl in &index.structs {
        let mut members = Vec::new();
        for member in &decl.members {
            members.push(StructMember {
                name: Some(member.name.clone()),
                ty: type_ref_to_lps_with_structs(&member.ty, &structs, array_size_consts)?,
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
    array_size_consts: &ArraySizeConsts,
) -> Result<Vec<FunctionSig>, Diagnostic> {
    index
        .functions
        .iter()
        .map(|function| {
            Ok(FunctionSig {
                name: function.name.clone(),
                return_ty: type_ref_to_lps_with_structs(
                    &function.return_ty,
                    structs,
                    array_size_consts,
                )?,
                params: function
                    .params
                    .iter()
                    .map(|p| {
                        Ok(HirParam {
                            name: p.name.clone(),
                            ty: type_ref_to_lps_with_structs(&p.ty, structs, array_size_consts)?,
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
    array_size_consts: &ArraySizeConsts,
) -> Result<(VecMap<String, UniformInfo>, Option<LpsType>, usize), Diagnostic> {
    let mut uniforms = VecMap::new();
    let mut members = Vec::new();
    let mut offset = lps_shared::VMCTX_HEADER_SIZE;
    for uniform in &index.uniforms {
        let ty = type_ref_to_lps_with_structs(&uniform.ty, structs, array_size_consts)?;
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
    Ok((
        uniforms,
        uniforms_type,
        offset - lps_shared::VMCTX_HEADER_SIZE,
    ))
}

#[derive(Debug, Clone)]
struct GlobalInit<'src> {
    name: String,
    ty: LpsType,
    byte_offset: u32,
    init_span: Span,
    /// Parsed once in [`build_global_vars`] (validation pass); carried here
    /// so [`synthesize_shader_init`] doesn't parse the same span again.
    init: ParsedExpr<'src>,
}

fn function_body_map<'src>(
    bodies: Vec<(String, ParsedFunctionBody<'src>)>,
) -> VecMap<String, ParsedFunctionBody<'src>> {
    let mut body_map = VecMap::new();
    for (name, body) in bodies {
        body_map.insert(name, body);
    }
    body_map
}

fn build_global_vars<'src>(
    source: &'src str,
    tokens: &[Token],
    index: &TopLevelIndex,
    structs: &StructTypes,
    array_size_consts: &ArraySizeConsts,
    uniforms_size: usize,
) -> Result<
    (
        VecMap<String, GlobalInfo>,
        Option<LpsType>,
        Vec<GlobalInit<'src>>,
    ),
    Diagnostic,
> {
    let mut order = Vec::<String>::new();
    let mut by_name = VecMap::<String, (LpsType, Span, Option<Span>)>::new();
    for global in &index.globals {
        let ty = type_ref_to_lps_with_structs(&global.ty, structs, array_size_consts)?;
        if let Some((existing_ty, _span, init_span)) = by_name.get_mut(&global.name) {
            if *existing_ty != ty {
                return Err(Diagnostic::error(
                    global.span,
                    format!("conflicting global declaration `{}`", global.name),
                ));
            }
            if global.init_span.is_some() {
                *init_span = global.init_span;
            }
            continue;
        }
        order.push(global.name.clone());
        by_name.insert(global.name.clone(), (ty, global.span, global.init_span));
    }

    let mut globals = VecMap::new();
    let mut members = Vec::new();
    let mut inits = Vec::new();
    let mut offset = lps_shared::VMCTX_HEADER_SIZE + uniforms_size;
    for name in order {
        let (ty, _span, init_span) = by_name
            .remove(&name)
            .ok_or_else(|| Diagnostic::error(Span::new(0, 0), "internal global map mismatch"))?;
        let align = lps_shared::type_alignment(&ty, LayoutRules::Std430);
        offset = lps_shared::layout::round_up(offset, align);
        let byte_offset = offset as u32;
        offset += lps_shared::type_size(&ty, LayoutRules::Std430);
        members.push(lps_shared::StructMember {
            name: Some(name.clone()),
            ty: ty.clone(),
        });
        if let Some(init_span) = init_span {
            let init = parse_expr_tokens(source, tokens, init_span)?;
            inits.push(GlobalInit {
                name: name.clone(),
                ty: ty.clone(),
                byte_offset,
                init_span,
                init,
            });
        }
        globals.insert(name, GlobalInfo { ty, byte_offset });
    }

    let globals_type = if members.is_empty() {
        None
    } else {
        Some(LpsType::Struct {
            name: Some(String::from("__globals")),
            members,
        })
    };
    Ok((globals, globals_type, inits))
}

fn build_global_consts<'src>(
    source: &'src str,
    tokens: &[Token],
    index: &TopLevelIndex,
    uniforms: &VecMap<String, UniformInfo>,
    global_vars: &VecMap<String, GlobalInfo>,
    functions: &[FunctionSig],
    structs: &StructTypes,
    array_size_consts: &ArraySizeConsts,
    imports: &mut ImportRegistry,
    texture_specs: &VecMap<String, lps_shared::TextureBindingSpec>,
    mut const_init_cache: Vec<Option<ParsedExpr<'src>>>,
) -> Result<VecMap<String, GlobalConst>, Diagnostic> {
    let mut globals = VecMap::new();
    for (const_index, konst) in index.consts.iter().enumerate() {
        let ty = type_ref_to_lps_with_structs(&konst.ty, structs, array_size_consts)?;
        let Some(init_span) = konst.init_span else {
            return Err(Diagnostic::error(
                konst.span,
                "const declaration requires initializer",
            ));
        };
        // `build_array_size_consts` already parsed this span for int/uint
        // consts; parsing is a pure function of (source, tokens, span), so
        // reusing that result is identical to re-parsing.
        let parsed = match const_init_cache.get_mut(const_index).and_then(Option::take) {
            Some(cached) => cached,
            None => parse_expr_tokens(source, tokens, init_span)?,
        };
        let mut ctx = TypeCtx::global_const(
            functions,
            uniforms,
            &globals,
            global_vars,
            structs,
            array_size_consts,
            imports,
            texture_specs,
        );
        let expr = ctx.type_expr(&parsed)?;
        let expr = ctx.coerce_expr(expr, &ty)?;
        globals.insert(
            konst.name.clone(),
            GlobalConst {
                arena: core::mem::take(&mut ctx.arena),
                expr,
            },
        );
    }
    Ok(globals)
}

#[allow(
    clippy::too_many_arguments,
    reason = "synthetic init needs the same typing context as functions"
)]
fn synthesize_shader_init(
    inits: &[GlobalInit<'_>],
    functions: &[FunctionSig],
    uniforms: &VecMap<String, UniformInfo>,
    global_consts: &VecMap<String, GlobalConst>,
    global_vars: &VecMap<String, GlobalInfo>,
    structs: &StructTypes,
    array_size_consts: &ArraySizeConsts,
    imports: &mut ImportRegistry,
    texture_specs: &VecMap<String, lps_shared::TextureBindingSpec>,
) -> Result<Option<HirFunction>, Diagnostic> {
    if inits.is_empty() {
        return Ok(None);
    }
    let sig = FunctionSig {
        name: String::from("__shader_init"),
        return_ty: LpsType::Void,
        params: Vec::new(),
    };
    let mut ctx = TypeCtx::new(
        &sig,
        functions,
        uniforms,
        global_consts,
        global_vars,
        structs,
        array_size_consts,
        imports,
        texture_specs,
    );
    let mut statements = Vec::new();
    for init in inits {
        // Already parsed once in `build_global_vars`; parsing is a pure
        // function of (source, tokens, span), so this is behaviorally
        // identical to re-parsing `init.init_span` here.
        let expr = ctx.type_expr(&init.init)?;
        let value = ctx.coerce_expr(expr, &init.ty)?;
        let target = ctx.arena.push_place(HirPlace::global(
            init.name.clone(),
            init.byte_offset,
            init.ty.clone(),
        ));
        let ty = ctx.arena.expr_ty(value).clone();
        let assign = ctx
            .arena
            .push_expr(init.init_span, ty, HirExprKind::Assign { target, value });
        statements.push(HirStmt::Expr(assign));
    }
    let locals = core::mem::take(&mut ctx.locals);
    let mut arena = core::mem::take(&mut ctx.arena);
    let (return_ty, params) = intern_signature(&mut arena, &sig);
    Ok(Some(HirFunction {
        name: sig.name.clone(),
        return_ty,
        params,
        body: HirFunctionBody {
            locals,
            statements,
            arena,
        },
    }))
}
