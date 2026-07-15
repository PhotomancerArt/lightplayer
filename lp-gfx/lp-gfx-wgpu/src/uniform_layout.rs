//! Reflection-driven uniform layout: the `name → (binding, type, size)`
//! table read from the validated naga module.
//!
//! Layout truth comes from **naga's own layout** (user decision Q2): struct
//! member offsets are the `offset` fields naga records on
//! `TypeInner::Struct`, array strides are `TypeInner::Array::stride`, and
//! sizes come from `naga::proc::Layouter` — the same numbers `wgsl-out`
//! emits, so offsets are correct by construction and there is no
//! hand-maintained std140 (the `lps-shared` `Std140` stub stays untouched).

use std::collections::BTreeSet;

use lp_gfx::GfxError;
use lp_shader::TextureBindingSpecs;
use lps_shared::TextureBindingSpec;
use naga::proc::Layouter;
use naga::{Handle, Type, TypeInner};

/// One reflected top-level uniform global.
#[derive(Debug, Clone)]
pub struct UniformGlobal {
    /// GLSL uniform name (matches the engine's `LpsValueF32::Struct` field).
    pub name: String,
    /// `@binding(N)` slot inside `@group(0)`.
    pub binding: u32,
    /// naga type handle (drives the byte writer).
    pub ty: Handle<Type>,
    /// Byte size per naga's layouter (uniform buffer / binding size).
    pub size: u32,
}

/// Uniform layout table for one compiled shader, sorted by binding.
#[derive(Debug, Clone, Default)]
pub struct UniformTable {
    pub globals: Vec<UniformGlobal>,
}

/// Reflect the module's `var<uniform>` globals. All uniforms must live in
/// descriptor set 0 (the spike's binding convention: `@group(0)`).
pub fn reflect_uniforms(module: &naga::Module) -> Result<UniformTable, GfxError> {
    let mut layouter = Layouter::default();
    layouter
        .update(module.to_ctx())
        .map_err(|e| GfxError::Compile(format!("uniform layout: {e}")))?;

    let mut globals = Vec::new();
    for (_, var) in module.global_variables.iter() {
        if var.space != naga::AddressSpace::Uniform {
            continue;
        }
        let name = var
            .name
            .clone()
            .ok_or_else(|| GfxError::Compile(String::from("unnamed uniform global")))?;
        let binding = var.binding.as_ref().ok_or_else(|| {
            GfxError::Compile(format!("uniform `{name}` has no resource binding"))
        })?;
        if binding.group != 0 {
            return Err(GfxError::Compile(format!(
                "uniform `{name}` uses descriptor set {} (only set 0 is supported)",
                binding.group
            )));
        }
        globals.push(UniformGlobal {
            name,
            binding: binding.binding,
            ty: var.ty,
            size: layouter[var.ty].size,
        });
    }
    globals.sort_by_key(|g| g.binding);
    Ok(UniformTable { globals })
}

/// One reflected `sampler2D` uniform: the naga texture global joined with
/// its compile-time [`TextureBindingSpec`].
#[derive(Debug, Clone)]
pub struct TextureGlobal {
    /// GLSL sampler uniform leaf path (matches the spec-map key and the
    /// engine's `LpsValueF32::Struct` field path).
    pub name: String,
    /// `@binding(N)` slot inside `@group(0)`.
    pub binding: u32,
    /// Compile-time binding contract for this sampler.
    pub spec: TextureBindingSpec,
}

/// Assign `@group(0)` bindings to handle-space texture globals that naga
/// `glsl-in` left unbound (GLSL `uniform sampler2D t;` carries no
/// `layout(binding = N)` in authored LightPlayer shaders). Slots start
/// after the highest binding already used in group 0. Must run before
/// validation so `wgsl-out` sees fully bound globals.
pub fn assign_texture_bindings(module: &mut naga::Module) -> Result<(), GfxError> {
    let mut used: BTreeSet<u32> = BTreeSet::new();
    for (_, var) in module.global_variables.iter() {
        if let Some(binding) = &var.binding
            && binding.group == 0
        {
            used.insert(binding.binding);
        }
    }
    let mut next = used.last().map_or(0, |&b| b + 1);
    let types = &module.types;
    for (_, var) in module.global_variables.iter_mut() {
        let is_handle = matches!(
            types[var.ty].inner,
            TypeInner::Image { .. } | TypeInner::Sampler { .. }
        );
        if !is_handle || var.binding.is_some() {
            continue;
        }
        var.binding = Some(naga::ResourceBinding {
            group: 0,
            binding: next,
        });
        next += 1;
    }
    Ok(())
}

/// Reflect the module's texture globals and join them with the compile-time
/// spec map by uniform name — the shared `TextureBindingSpec` contract.
/// Mismatches are compile errors, matching the CPU tier's validation:
/// a declared sampler without a spec, or a spec naming no declared sampler.
pub fn reflect_textures(
    module: &naga::Module,
    specs: &TextureBindingSpecs,
) -> Result<Vec<TextureGlobal>, GfxError> {
    let mut textures = Vec::new();
    for (_, var) in module.global_variables.iter() {
        let image = match &module.types[var.ty].inner {
            TypeInner::Image {
                dim,
                arrayed,
                class,
            } => Some((*dim, *arrayed, *class)),
            TypeInner::Sampler { .. } => {
                // The texture lowering rewrites every sampling call site to
                // `textureLoad` arithmetic; a comparison/other sampler global
                // means an unsupported sampling builtin survived.
                return Err(GfxError::Compile(format!(
                    "unsupported sampling operation left a sampler global `{}` in the module \
                     (only texture() and texelFetch(..., 0) are supported)",
                    var.name.as_deref().unwrap_or("<unnamed>")
                )));
            }
            _ => None,
        };
        let Some((dim, arrayed, class)) = image else {
            continue;
        };
        let name = var
            .name
            .clone()
            .ok_or_else(|| GfxError::Compile(String::from("texture global without a name")))?;
        if dim != naga::ImageDimension::D2 || arrayed {
            return Err(GfxError::Compile(format!(
                "texture `{name}`: only non-arrayed 2D textures are supported"
            )));
        }
        if !matches!(
            class,
            naga::ImageClass::Sampled {
                kind: naga::ScalarKind::Float,
                multi: false,
            }
        ) {
            return Err(GfxError::Compile(format!(
                "texture `{name}`: only single-sampled float sampler2D uniforms are supported"
            )));
        }
        let binding = var.binding.as_ref().ok_or_else(|| {
            GfxError::Compile(format!("texture `{name}` has no resource binding"))
        })?;
        if binding.group != 0 {
            return Err(GfxError::Compile(format!(
                "texture `{name}` uses descriptor set {} (only set 0 is supported)",
                binding.group
            )));
        }
        let spec = specs.get(&name).copied().ok_or_else(|| {
            GfxError::Compile(format!(
                "sampler2D uniform `{name}` has no TextureBindingSpec \
                 (every sampler leaf needs a compile-time spec)"
            ))
        })?;
        textures.push(TextureGlobal {
            name,
            binding: binding.binding,
            spec,
        });
    }
    for key in specs.keys() {
        if !textures.iter().any(|t| t.name == *key) {
            return Err(GfxError::Compile(format!(
                "TextureBindingSpec names sampler `{key}`, which the shader does not declare"
            )));
        }
    }
    textures.sort_by_key(|t| t.binding);
    Ok(textures)
}
