//! Naga module → LPIR [`lpir::LpirModule`] lowering entry point.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{
    CalleeRef, FuncId, FunctionBuilder, ImportDecl, IrFunction, IrType, LpirModule, LpirOp,
    ModuleBuilder, VMCTX_VREG, VReg,
};
use lps_shared::{
    LayoutRules, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, StructMember, TextureBindingSpec,
    VMCTX_HEADER_SIZE, type_alignment, type_size, validate_texture_binding_specs_against_module,
};
use naga::{AddressSpace, Expression, Function, GlobalVariable, Handle, Module};

use crate::NagaModule;
use crate::lower_ctx::{GlobalVarInfo, GlobalVarMap, LowerCtx};
use crate::lower_error::LowerError;
use crate::lower_lpfn;
use crate::naga_types::naga_type_handle_to_lps;

/// Options for Naga → LPIR lowering (e.g. compile-time texture binding metadata).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LowerOptions {
    /// When non-empty, must match every `Texture2D` uniform (validated before lowering; see
    /// [`lps_shared::validate_texture_binding_specs_against_module`]). An empty map skips that check
    /// so `lower()` stays usable when specs are applied later.
    pub texture_specs: BTreeMap<String, TextureBindingSpec>,
    /// Whether `texelFetch` lowering emits coordinate clamp ops (see [`lpir::TexelFetchBoundsMode`]).
    pub texel_fetch_bounds: lpir::TexelFetchBoundsMode,
}

impl Default for LowerOptions {
    fn default() -> Self {
        Self {
            texture_specs: BTreeMap::new(),
            texel_fetch_bounds: lpir::TexelFetchBoundsMode::default(),
        }
    }
}

/// Lower a parsed [`NagaModule`] to LPIR (scalarized vectors and matrices).
///
/// Same as [`lower_with_options`] with default options (no texture specs). Registers `@glsl::*`,
/// `@lpir::*`, and `@lpfn::*` imports as needed, then emits one [`lpir::IrFunction`] per
/// entry in [`NagaModule::functions`]. Fails with [`LowerError`] on unsupported Naga IR outside the
/// scalar subset.
pub fn lower(naga_module: &NagaModule) -> Result<(LpirModule, LpsModuleSig), LowerError> {
    lower_with_options(naga_module, &LowerOptions::default())
}

/// Lower with [`LowerOptions`]. When `options.texture_specs` is non-empty, runs binding validation, then
/// copies specs into [`LpsModuleSig::texture_specs`]. An empty spec map defers validation to match
/// texture-free [`lower`].
pub fn lower_with_options(
    naga_module: &NagaModule,
    options: &LowerOptions,
) -> Result<(LpirModule, LpsModuleSig), LowerError> {
    let mut mb = ModuleBuilder::new();
    let mut import_map = register_math_imports(&mut mb);
    import_map.extend(register_texture_imports(&mut mb));
    let lpfn_map = lower_lpfn::register_lpfn_imports(&mut mb, naga_module)?;
    let mut func_map: BTreeMap<Handle<Function>, CalleeRef> = BTreeMap::new();
    for (i, (handle, _)) in naga_module.functions.iter().enumerate() {
        func_map.insert(*handle, CalleeRef::Local(FuncId(i as u16)));
    }

    // Walk global variables and compute layout for uniforms and globals.
    let (global_map, uniforms_type, globals_type) = compute_global_layout(&naga_module.module)?;

    let mut glsl_meta = LpsModuleSig {
        uniforms_type,
        globals_type,
        texture_specs: options.texture_specs.clone(),
        ..Default::default()
    };
    // Non-empty `texture_specs` ⇒ run M1/M2-style binding validation before IR lowering. Empty map
    // defers validation (e.g. `lower()` without options, then `validate_texture_binding_specs_against_module`).
    if !options.texture_specs.is_empty() {
        validate_texture_binding_specs_against_module(&glsl_meta, &options.texture_specs)
            .map_err(LowerError::UnsupportedExpression)?;
    }

    // Lower user functions.
    for (handle, info) in &naga_module.functions {
        let func = &naga_module.module.functions[*handle];
        let ir = lower_function(
            &naga_module.module,
            func,
            info.name.as_str(),
            &func_map,
            &import_map,
            &lpfn_map,
            global_map.clone(),
            &options.texture_specs,
            options.texel_fetch_bounds,
            glsl_meta.uniforms_type.as_ref(),
        )
        .map_err(|e| LowerError::InFunction {
            name: info.name.clone(),
            inner: Box::new(e),
        })?;
        glsl_meta.functions.push(LpsFnSig {
            name: info.name.clone(),
            parameters: info.params.clone(),
            return_type: info.return_type.clone(),
            kind: LpsFnKind::UserDefined,
        });
        mb.add_function(ir);
    }

    // Synthesize __shader_init function if there are globals with initializers.
    if !global_map.is_empty() {
        if let Some(init_func) = synthesize_shader_init(&naga_module.module, &global_map) {
            glsl_meta.functions.push(LpsFnSig {
                name: String::from("__shader_init"),
                parameters: vec![],
                return_type: LpsType::Void,
                kind: LpsFnKind::Synthetic,
            });
            mb.add_function(init_func);
        }
    }

    Ok((mb.finish(), glsl_meta))
}

/// Naga-only companion sampler from `parse.rs` (`uniform sampler __lp_samp_*`); not part of the
/// runtime uniform ABI.
fn is_lp_synthetic_naga_sampler(gv: &GlobalVariable, lps_ty: &LpsType) -> bool {
    gv.space == AddressSpace::Handle
        && matches!(lps_ty, LpsType::UInt)
        && gv
            .name
            .as_deref()
            .is_some_and(|n| n.starts_with("__lp_samp_"))
}

/// Compute layout for global variables (uniforms and private globals).
/// Returns (global_map, uniforms_type, globals_type).
///
/// Naga's GLSL frontend may emit **multiple** [`GlobalVariable`] handles for the same logical
/// global (forward declaration plus a later redeclaration with an initializer). They share the
/// same name and address space and must alias to **one** VMContext region; otherwise loads can
/// target an uninitialized duplicate while [`synthesize_shader_init`] initializes another.
fn compute_global_layout(
    module: &Module,
) -> Result<(GlobalVarMap, Option<LpsType>, Option<LpsType>), LowerError> {
    type GlobalKey = (Option<String>, AddressSpace);

    let mut groups: BTreeMap<GlobalKey, Vec<Handle<GlobalVariable>>> = BTreeMap::new();
    let mut key_order: Vec<GlobalKey> = Vec::new();
    let mut seen_key: BTreeMap<GlobalKey, ()> = BTreeMap::new();

    for (h, gv) in module.global_variables.iter() {
        let key = (gv.name.clone(), gv.space);
        groups.entry(key.clone()).or_default().push(h);
        if seen_key.insert(key.clone(), ()).is_none() {
            key_order.push(key);
        }
    }

    let mut global_map: GlobalVarMap = BTreeMap::new();
    let mut uniforms_members: Vec<StructMember> = Vec::new();
    let mut globals_members: Vec<StructMember> = Vec::new();

    for key in &key_order {
        let handles = groups.get(key).ok_or_else(|| {
            LowerError::Internal(String::from("compute_global_layout: missing group"))
        })?;
        let ty0 = module.global_variables[handles[0]].ty;
        for &h in handles.iter().skip(1) {
            if module.global_variables[h].ty != ty0 {
                return Err(LowerError::UnsupportedExpression(format!(
                    "conflicting types for global {:?}",
                    key.0
                )));
            }
        }
        let canonical = handles
            .iter()
            .copied()
            .find(|h| module.global_variables[*h].init.is_some())
            .unwrap_or(handles[0]);
        let gv = &module.global_variables[canonical];

        // Map Naga type to LpsType first (needed for Handle / texture resources).
        let lps_ty = naga_type_handle_to_lps(module, gv.ty)
            .map_err(|e| LowerError::UnsupportedType(format!("{e:?}")))?;

        // Map AddressSpace to uniform/global (Naga uses `Handle` for bound textures/samplers).
        let (is_uniform, is_supported) = match gv.space {
            AddressSpace::Uniform => (true, true),
            AddressSpace::Private => (false, true),
            AddressSpace::Handle => {
                let texture2d = matches!(lps_ty, LpsType::Texture2D);
                let synth_sampler = matches!(lps_ty, LpsType::UInt)
                    && gv
                        .name
                        .as_deref()
                        .is_some_and(|n| n.starts_with("__lp_samp_"));
                if texture2d || synth_sampler {
                    (true, true)
                } else {
                    return Err(LowerError::UnsupportedExpression(format!(
                        "GlobalVariable address space Handle is only supported for Texture2D-like types, got {lps_ty:?}"
                    )));
                }
            }
            _ => (false, false),
        };

        if !is_supported {
            return Err(LowerError::UnsupportedExpression(format!(
                "GlobalVariable address space {:?} not supported",
                gv.space
            )));
        }

        // Determine component count for scalarization
        let component_count = lps_scalar_component_count(&lps_ty);

        if is_lp_synthetic_naga_sampler(gv, &lps_ty) {
            for &h in handles {
                global_map.insert(
                    h,
                    GlobalVarInfo {
                        byte_offset: 0,
                        ty: lps_ty.clone(),
                        component_count,
                        is_uniform: true,
                        vmctx_backed: false,
                    },
                );
            }
            continue;
        }

        let member = StructMember {
            name: gv.name.clone(),
            ty: lps_ty.clone(),
        };

        if is_uniform {
            uniforms_members.push(member);
        } else {
            globals_members.push(member);
        }

        let info = GlobalVarInfo {
            byte_offset: 0,
            ty: lps_ty,
            component_count,
            is_uniform,
            vmctx_backed: true,
        };
        for &h in handles {
            global_map.insert(h, info.clone());
        }
    }

    // Compute byte offsets using std430 layout
    let mut uniforms_offset = VMCTX_HEADER_SIZE as u32;
    for member in &uniforms_members {
        let align = type_alignment(&member.ty, LayoutRules::Std430) as u32;
        uniforms_offset = round_up_u32(uniforms_offset, align);
        uniforms_offset += type_size(&member.ty, LayoutRules::Std430) as u32;
    }
    let uniforms_size = uniforms_offset - VMCTX_HEADER_SIZE as u32;

    let mut globals_offset = uniforms_offset;
    for member in &globals_members {
        let align = type_alignment(&member.ty, LayoutRules::Std430) as u32;
        globals_offset = round_up_u32(globals_offset, align);
        globals_offset += type_size(&member.ty, LayoutRules::Std430) as u32;
    }

    // Assign the same byte offset to every Naga handle in a merged logical global group.
    let mut uniforms_offset = VMCTX_HEADER_SIZE as u32;
    let mut globals_offset = VMCTX_HEADER_SIZE as u32 + uniforms_size;

    for key in &key_order {
        let handles = groups.get(key).ok_or_else(|| {
            LowerError::Internal(String::from(
                "compute_global_layout: missing group (offsets)",
            ))
        })?;
        let h0 = handles[0];
        let info = global_map.get(&h0).ok_or_else(|| {
            LowerError::Internal(String::from("compute_global_layout: missing GlobalVarInfo"))
        })?;
        if !info.vmctx_backed {
            continue;
        }
        let align = type_alignment(&info.ty, LayoutRules::Std430) as u32;
        let size = type_size(&info.ty, LayoutRules::Std430) as u32;
        let off = if info.is_uniform {
            uniforms_offset = round_up_u32(uniforms_offset, align);
            let o = uniforms_offset;
            uniforms_offset += size;
            o
        } else {
            globals_offset = round_up_u32(globals_offset, align);
            let o = globals_offset;
            globals_offset += size;
            o
        };
        for &h in handles {
            global_map
                .get_mut(&h)
                .ok_or_else(|| {
                    LowerError::Internal(String::from("compute_global_layout: stale handle"))
                })?
                .byte_offset = off;
        }
    }

    let uniforms_type = if uniforms_members.is_empty() {
        None
    } else {
        // One struct entry per uniform global (including `uniform Params params`), so paths match
        // GLSL and dotted texture keys (e.g. `params.gradient`) align with `LpsTypePathExt`.
        Some(LpsType::Struct {
            name: Some(String::from("__uniforms")),
            members: uniforms_members,
        })
    };

    let globals_type = if globals_members.is_empty() {
        None
    } else {
        Some(LpsType::Struct {
            name: Some(String::from("__globals")),
            members: globals_members,
        })
    };

    Ok((global_map, uniforms_type, globals_type))
}

fn round_up_u32(size: u32, alignment: u32) -> u32 {
    ((size + alignment - 1) / alignment) * alignment
}

/// Scalar slots (each4 bytes in VMContext) for a value of `ty` when flattened for loads/stores.
fn lps_scalar_component_count(ty: &LpsType) -> u32 {
    match ty {
        LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => 1,
        LpsType::Vec2 | LpsType::IVec2 | LpsType::UVec2 | LpsType::BVec2 => 2,
        LpsType::Vec3 | LpsType::IVec3 | LpsType::UVec3 | LpsType::BVec3 => 3,
        LpsType::Vec4 | LpsType::IVec4 | LpsType::UVec4 | LpsType::BVec4 => 4,
        LpsType::Mat2 => 4,
        LpsType::Mat3 => 9,
        LpsType::Mat4 => 16,
        LpsType::Texture2D => 4,
        LpsType::Array { element, len } => lps_scalar_component_count(element).saturating_mul(*len),
        LpsType::Struct { members, .. } => members
            .iter()
            .map(|m| lps_scalar_component_count(&m.ty))
            .sum(),
        LpsType::Void => 0,
    }
}

/// Synthesize a __shader_init function that evaluates global initializers.
fn synthesize_shader_init(module: &Module, global_map: &GlobalVarMap) -> Option<IrFunction> {
    // Collect globals that have initializers
    let globals_with_init: Vec<(Handle<GlobalVariable>, &GlobalVarInfo, &naga::Expression)> =
        module
            .global_variables
            .iter()
            .filter_map(|(h, gv)| {
                global_map.get(&h).and_then(|info| {
                    gv.init
                        .map(|init_h| (h, info, &module.global_expressions[init_h]))
                })
            })
            .collect();

    if globals_with_init.is_empty() {
        return None;
    }

    let mut fb = FunctionBuilder::new("__shader_init", &[]);
    let mut emitted_any = false;

    // For each global with an initializer, evaluate it and store to VMContext.
    for (_gv_handle, info, init_expr) in globals_with_init {
        if info.is_uniform {
            // Uniforms shouldn't have initializers - skip or error
            continue;
        }
        if !info.vmctx_backed {
            continue;
        }

        let Some(init_vregs) = flatten_shader_init_vregs(module, &mut fb, init_expr) else {
            continue;
        };
        if init_vregs.is_empty() {
            continue;
        }

        emitted_any = true;
        // Store each component to the VMContext buffer
        for (i, vreg) in init_vregs.iter().enumerate() {
            let offset = info.byte_offset + (i as u32 * 4);
            fb.push(LpirOp::Store {
                base: VMCTX_VREG,
                offset,
                value: *vreg,
            });
        }
    }

    if !emitted_any {
        return None;
    }

    fb.push_return(&[]);
    Some(fb.finish())
}

/// Flatten constant global initializer expressions (literals and nested `Compose`, e.g. `mat2`).
fn flatten_shader_init_vregs(
    module: &Module,
    fb: &mut FunctionBuilder,
    expr: &naga::Expression,
) -> Option<Vec<VReg>> {
    match expr {
        Expression::Literal(lit) => Some(vec![push_literal_to_builder(fb, lit)?]),
        Expression::Compose { components, .. } => {
            let mut vregs = Vec::new();
            for comp in components.iter() {
                let sub = &module.global_expressions[*comp];
                let mut part = flatten_shader_init_vregs(module, fb, sub)?;
                vregs.append(&mut part);
            }
            Some(vregs)
        }
        _ => None,
    }
}

fn push_literal_to_builder(fb: &mut FunctionBuilder, lit: &naga::Literal) -> Option<VReg> {
    match *lit {
        naga::Literal::F32(v) => {
            let d = fb.alloc_vreg(IrType::F32);
            fb.push(LpirOp::FconstF32 { dst: d, value: v });
            Some(d)
        }
        naga::Literal::I32(v) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 { dst: d, value: v });
            Some(d)
        }
        naga::Literal::U32(v) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 {
                dst: d,
                value: v as i32,
            });
            Some(d)
        }
        naga::Literal::Bool(v) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 {
                dst: d,
                value: if v { 1 } else { 0 },
            });
            Some(d)
        }
        naga::Literal::F64(v) => {
            let d = fb.alloc_vreg(IrType::F32);
            fb.push(LpirOp::FconstF32 {
                dst: d,
                value: v as f32,
            });
            Some(d)
        }
        _ => None,
    }
}

fn register_math_imports(mb: &mut ModuleBuilder) -> BTreeMap<String, CalleeRef> {
    let mut m = BTreeMap::new();
    let mut reg =
        |module: &str, name: &str, params: &[IrType], rets: &[IrType], needs_vmctx: bool| {
            let r = mb.add_import(ImportDecl {
                module_name: String::from(module),
                func_name: String::from(name),
                param_types: params.to_vec(),
                return_types: rets.to_vec(),
                lpfn_glsl_params: None,
                needs_vmctx,
                sret: false,
            });
            m.insert(format!("{module}::{name}"), r);
        };
    let f1 = &[IrType::F32];
    let r1 = &[IrType::F32];
    let u1 = &[IrType::I32];
    reg("lpir", "sqrt", f1, r1, false);
    reg("glsl", "sin", f1, r1, false);
    reg("glsl", "cos", f1, r1, false);
    reg("glsl", "tan", f1, r1, false);
    reg("glsl", "asin", f1, r1, false);
    reg("glsl", "acos", f1, r1, false);
    reg("glsl", "atan", f1, r1, false);
    reg("glsl", "atan2", &[IrType::F32, IrType::F32], r1, false);
    reg("glsl", "sinh", f1, r1, false);
    reg("glsl", "cosh", f1, r1, false);
    reg("glsl", "tanh", f1, r1, false);
    reg("glsl", "asinh", f1, r1, false);
    reg("glsl", "acosh", f1, r1, false);
    reg("glsl", "atanh", f1, r1, false);
    reg("glsl", "exp", f1, r1, false);
    reg("glsl", "exp2", f1, r1, false);
    reg("glsl", "log", f1, r1, false);
    reg("glsl", "log2", f1, r1, false);
    reg("glsl", "pow", &[IrType::F32, IrType::F32], r1, false);
    reg("glsl", "ldexp", &[IrType::F32, IrType::I32], r1, false);
    reg("glsl", "round", f1, r1, false);
    reg("vm", "__lp_get_fuel", &[], u1, true);
    m
}

/// `@texture::*` sampler builtins (result pointer ABI; [`ImportDecl::sret`]).
fn register_texture_imports(mb: &mut ModuleBuilder) -> BTreeMap<String, CalleeRef> {
    let mut m = BTreeMap::new();
    let mut reg = |func_name: &str, user_param_count: usize| {
        let mut param_types = Vec::with_capacity(user_param_count);
        param_types.push(IrType::Pointer);
        for _ in 1..user_param_count {
            param_types.push(IrType::I32);
        }
        let r = mb.add_import(ImportDecl {
            module_name: String::from("texture"),
            func_name: String::from(func_name),
            param_types,
            return_types: Vec::new(),
            lpfn_glsl_params: None,
            needs_vmctx: false,
            sret: true,
        });
        m.insert(format!("texture::{func_name}"), r);
    };
    reg("texture2d_rgba16_unorm", 10);
    reg("texture1d_rgba16_unorm", 7);
    reg("texture2d_r16_unorm", 10);
    reg("texture1d_r16_unorm", 7);
    m
}

fn lower_function(
    module: &Module,
    func: &Function,
    name: &str,
    func_map: &BTreeMap<Handle<Function>, CalleeRef>,
    import_map: &BTreeMap<String, CalleeRef>,
    lpfn_map: &BTreeMap<Handle<Function>, CalleeRef>,
    global_map: GlobalVarMap,
    texture_specs: &BTreeMap<String, TextureBindingSpec>,
    texel_fetch_bounds: lpir::TexelFetchBoundsMode,
    uniforms_type: Option<&LpsType>,
) -> Result<IrFunction, LowerError> {
    let mut ctx = LowerCtx::new(
        module,
        func,
        name,
        func_map,
        import_map,
        lpfn_map,
        global_map,
        texture_specs,
        texel_fetch_bounds,
        uniforms_type,
    )?;
    crate::lower_stmt::lower_block(&mut ctx, &func.body)?;
    if func.result.is_none() && crate::lower_stmt::void_block_missing_return(&func.body) {
        ctx.fb.push_return(&[]);
    }
    Ok(ctx.finish())
}
