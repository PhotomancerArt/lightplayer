//! Naga module → LPIR [`lpir::LpirModule`] lowering entry point.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{
    CalleeRef, FunctionBuilder, ImportDecl, IrFunction, IrType, LpirModule, LpirOp, ModuleBuilder,
    VMCTX_VREG, VReg,
};
use lps_shared::{
    LayoutRules, LpsFnSig, LpsModuleSig, LpsType, StructMember, VMCTX_HEADER_SIZE, type_alignment,
    type_size,
};
use naga::{AddressSpace, Expression, Function, GlobalVariable, Handle, Module};

use crate::NagaModule;
use crate::lower_ctx::{GlobalVarInfo, GlobalVarMap, LowerCtx};
use crate::lower_error::LowerError;
use crate::lower_lpfx;
use crate::naga_types::naga_type_inner_to_glsl;

/// Lower a parsed [`NagaModule`] to LPIR (scalarized vectors and matrices).
///
/// Registers `@glsl::*`, `@lpir::*`, and `@lpfx::*` imports as needed, then emits one [`lpir::IrFunction`] per
/// entry in [`NagaModule::functions`]. Fails with [`LowerError`] on unsupported Naga IR outside the
/// scalar subset.
pub fn lower(naga_module: &NagaModule) -> Result<(LpirModule, LpsModuleSig), LowerError> {
    let mut mb = ModuleBuilder::new();
    let import_map = register_math_imports(&mut mb);
    let lpfx_map = lower_lpfx::register_lpfx_imports(&mut mb, naga_module)?;
    let import_count = mb.import_count();

    let mut func_map: BTreeMap<Handle<Function>, CalleeRef> = BTreeMap::new();
    for (i, (handle, _)) in naga_module.functions.iter().enumerate() {
        func_map.insert(*handle, CalleeRef(import_count.saturating_add(i as u32)));
    }

    // Walk global variables and compute layout for uniforms and globals.
    let (global_map, uniforms_type, globals_type) = compute_global_layout(&naga_module.module)?;

    let mut glsl_meta = LpsModuleSig {
        uniforms_type,
        globals_type,
        ..Default::default()
    };

    // Lower user functions.
    for (handle, info) in &naga_module.functions {
        let func = &naga_module.module.functions[*handle];
        let ir = lower_function(
            &naga_module.module,
            func,
            info.name.as_str(),
            &func_map,
            &import_map,
            &lpfx_map,
            global_map.clone(),
        )
        .map_err(|e| LowerError::InFunction {
            name: info.name.clone(),
            inner: Box::new(e),
        })?;
        glsl_meta.functions.push(LpsFnSig {
            name: info.name.clone(),
            parameters: info.params.clone(),
            return_type: info.return_type.clone(),
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
            });
            mb.add_function(init_func);
        }
    }

    Ok((mb.finish(), glsl_meta))
}

/// Compute layout for global variables (uniforms and private globals).
/// Returns (global_map, uniforms_type, globals_type).
fn compute_global_layout(
    module: &Module,
) -> Result<(GlobalVarMap, Option<LpsType>, Option<LpsType>), LowerError> {
    let mut global_map: GlobalVarMap = BTreeMap::new();
    let mut uniforms_members: Vec<StructMember> = Vec::new();
    let mut globals_members: Vec<StructMember> = Vec::new();

    for (gv_handle, gv) in module.global_variables.iter() {
        // Map AddressSpace to uniform/global
        let (is_uniform, is_supported) = match gv.space {
            AddressSpace::Uniform => (true, true),
            AddressSpace::Private => (false, true),
            _ => (false, false),
        };

        if !is_supported {
            return Err(LowerError::UnsupportedExpression(format!(
                "GlobalVariable address space {:?} not supported",
                gv.space
            )));
        }

        // Map Naga type to LpsType
        let ty_inner = &module.types[gv.ty].inner;
        let lps_ty = naga_type_inner_to_glsl(module, ty_inner)
            .map_err(|e| LowerError::UnsupportedType(format!("{:?}", e)))?;

        // Determine component count for scalarization
        let component_count = match &lps_ty {
            LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => 1,
            LpsType::Vec2 | LpsType::IVec2 | LpsType::UVec2 | LpsType::BVec2 => 2,
            LpsType::Vec3 | LpsType::IVec3 | LpsType::UVec3 | LpsType::BVec3 => 3,
            LpsType::Vec4 | LpsType::IVec4 | LpsType::UVec4 | LpsType::BVec4 => 4,
            LpsType::Mat2 => 4,
            LpsType::Mat3 => 9,
            LpsType::Mat4 => 16,
            LpsType::Array { element, len } => {
                let elem_count = element.component_count().unwrap_or(1);
                ((*len) as usize * elem_count) as u32
            }
            _ => 1,
        };

        let member = StructMember {
            name: gv.name.clone(),
            ty: lps_ty.clone(),
        };

        if is_uniform {
            uniforms_members.push(member);
        } else {
            globals_members.push(member);
        }

        global_map.insert(
            gv_handle,
            GlobalVarInfo {
                byte_offset: 0, // Will be computed below
                ty: lps_ty,
                component_count,
                is_uniform,
            },
        );
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

    // Now update the global_map with actual byte offsets
    let mut uniforms_offset = VMCTX_HEADER_SIZE as u32;
    let mut globals_offset = VMCTX_HEADER_SIZE as u32 + uniforms_size;

    for (gv_handle, _gv) in module.global_variables.iter() {
        if let Some(info) = global_map.get_mut(&gv_handle) {
            let align = type_alignment(&info.ty, LayoutRules::Std430) as u32;
            if info.is_uniform {
                uniforms_offset = round_up_u32(uniforms_offset, align);
                info.byte_offset = uniforms_offset;
                uniforms_offset += type_size(&info.ty, LayoutRules::Std430) as u32;
            } else {
                globals_offset = round_up_u32(globals_offset, align);
                info.byte_offset = globals_offset;
                globals_offset += type_size(&info.ty, LayoutRules::Std430) as u32;
            }
        }
    }

    let uniforms_type = if uniforms_members.is_empty() {
        None
    } else {
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

    // For each global with an initializer, evaluate it and store to VMContext.
    for (_gv_handle, info, init_expr) in globals_with_init {
        if info.is_uniform {
            // Uniforms shouldn't have initializers - skip or error
            continue;
        }

        // Evaluate the initializer expression
        let init_vregs = match init_expr {
            Expression::Literal(lit) => {
                vec![push_literal_to_builder(&mut fb, lit)?]
            }
            Expression::Compose { components, .. } => {
                let mut vregs = Vec::new();
                for comp in components.iter() {
                    if let Expression::Literal(lit) = &module.global_expressions[*comp] {
                        vregs.push(push_literal_to_builder(&mut fb, lit)?);
                    } else {
                        // Non-constant component - not supported yet
                        return None;
                    }
                }
                vregs
            }
            _ => {
                // Non-constant initializer - not supported yet
                return None;
            }
        };

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

    fb.push_return(&[]);
    Some(fb.finish())
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
        naga::Literal::Bool(b) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 {
                dst: d,
                value: b as i32,
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
                lpfx_glsl_params: None,
                needs_vmctx,
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

fn lower_function(
    module: &Module,
    func: &Function,
    name: &str,
    func_map: &BTreeMap<Handle<Function>, CalleeRef>,
    import_map: &BTreeMap<String, CalleeRef>,
    lpfx_map: &BTreeMap<Handle<Function>, CalleeRef>,
    global_map: GlobalVarMap,
) -> Result<IrFunction, LowerError> {
    let mut ctx = LowerCtx::new(
        module, func, name, func_map, import_map, lpfx_map, global_map,
    )?;
    crate::lower_stmt::lower_block(&mut ctx, &func.body)?;
    if func.result.is_none() && crate::lower_stmt::void_block_missing_return(&func.body) {
        ctx.fb.push_return(&[]);
    }
    Ok(ctx.finish())
}
