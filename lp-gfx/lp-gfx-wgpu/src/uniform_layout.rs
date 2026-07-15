//! Reflection-driven uniform layout: the `name → (binding, type, size)`
//! table read from the validated naga module.
//!
//! Layout truth comes from **naga's own layout** (user decision Q2): struct
//! member offsets are the `offset` fields naga records on
//! `TypeInner::Struct`, array strides are `TypeInner::Array::stride`, and
//! sizes come from `naga::proc::Layouter` — the same numbers `wgsl-out`
//! emits, so offsets are correct by construction and there is no
//! hand-maintained std140 (the `lps-shared` `Std140` stub stays untouched).

use lp_gfx::GfxError;
use naga::proc::Layouter;
use naga::{Handle, Type};

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
