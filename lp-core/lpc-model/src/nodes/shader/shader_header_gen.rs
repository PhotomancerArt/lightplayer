//! Deterministic GLSL header generation from authored shader slot defs.
//!
//! This is evidence machinery for the model: TOML owns the slot shape, and
//! tools can generate the shader-visible declarations from that shape. Runtime
//! ABI marshalling is intentionally outside this module.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::{
    ComputeShaderDef, LpType, ShaderSlotKind, ShaderSlotMappingKind, ShaderValueShapeRef,
    SlotShapeLookup, SlotShapeRegistry,
};

/// Generate GLSL header declarations for authored compute shader slots.
pub fn generate_compute_shader_header(
    def: &ComputeShaderDef,
    registry: &SlotShapeRegistry,
) -> Result<String, ShaderHeaderGenError> {
    let mut out = String::new();
    let mut emitted_structs = Vec::new();

    for slot in def.consumed_slots.entries.values() {
        emit_native_struct_if_needed(slot.value.value(), registry, &mut emitted_structs, &mut out)?;
    }
    for slot in def.produced_slots.entries.values() {
        emit_native_struct_if_needed(slot.value.value(), registry, &mut emitted_structs, &mut out)?;
    }

    for (binding, (name, slot)) in def.consumed_slots.entries.iter().enumerate() {
        match slot.kind.value() {
            // Timebase kinds declare exactly what a plain f32 value slot
            // declares: the config that makes them a phasor lives on the def,
            // never in GLSL (`uniform float phase;` is the whole contract).
            ShaderSlotKind::Value | ShaderSlotKind::Phasor | ShaderSlotKind::Seconds => {
                let ty = glsl_type_for_ref(slot.value.value(), registry)?;
                writeln!(&mut out, "// consumed: {name}").expect("write string");
                writeln!(&mut out, "layout(binding = {binding}) uniform {ty} {name};")
                    .expect("write string");
            }
            // A palette's GLSL spelling comes from the kind, not from a
            // resolvable `LpType`: `sampler2D` is not an `LpType` at all, and
            // the palette itself rides the def's `gradient` field. The engine
            // supplies the matching `TextureBindingSpec` at compile
            // (`docs/design/lp-shader-texture-access.md`).
            ShaderSlotKind::Palette => {
                writeln!(&mut out, "// consumed: {name}").expect("write string");
                writeln!(
                    &mut out,
                    "layout(binding = {binding}) uniform sampler2D {name};"
                )
                .expect("write string");
            }
            ShaderSlotKind::Map => {
                let ty = glsl_type_for_ref(slot.value.value(), registry)?;
                let mapping = slot
                    .mapping
                    .data
                    .as_ref()
                    .ok_or(ShaderHeaderGenError::MissingMapping)?;
                match mapping.kind.value() {
                    ShaderSlotMappingKind::Sentinel => {
                        validate_key_field(slot.value.value(), registry, mapping.key.value())?;
                        writeln!(&mut out, "// consumed: {name}").expect("write string");
                        writeln!(
                            &mut out,
                            "layout(binding = {binding}) uniform {ty} {name}[{}];",
                            mapping.len.value()
                        )
                        .expect("write string");
                    }
                }
            }
        }
    }

    for (name, slot) in &def.produced_slots.entries {
        match slot.kind.value() {
            // A produced timebase slot is meaningless, but its declaration is
            // still just `float name;` — refusing it here would only move the
            // diagnostic somewhere less useful.
            ShaderSlotKind::Value | ShaderSlotKind::Phasor | ShaderSlotKind::Seconds => {
                let ty = glsl_type_for_ref(slot.value.value(), registry)?;
                writeln!(&mut out, "// produced: {name}").expect("write string");
                writeln!(&mut out, "{ty} {name};").expect("write string");
            }
            // A shader writes pixels, not palettes: there is nothing for a
            // produced sampler to mean, and inventing a declaration would
            // hide the authoring mistake instead of naming it.
            ShaderSlotKind::Palette => {
                return Err(ShaderHeaderGenError::Unsupported(
                    "a palette slot cannot be produced",
                ));
            }
            ShaderSlotKind::Map => {
                let ty = glsl_type_for_ref(slot.value.value(), registry)?;
                let mapping = slot
                    .mapping
                    .data
                    .as_ref()
                    .ok_or(ShaderHeaderGenError::MissingMapping)?;
                match mapping.kind.value() {
                    ShaderSlotMappingKind::Sentinel => {
                        validate_key_field(slot.value.value(), registry, mapping.key.value())?;
                        writeln!(&mut out, "// produced: {name}").expect("write string");
                        writeln!(&mut out, "{ty} {name}[{}];", mapping.len.value())
                            .expect("write string");
                    }
                }
            }
        }
    }

    Ok(out)
}

/// Failure generating a shader header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShaderHeaderGenError {
    MissingMapping,
    UnknownNativeShape(String),
    Unsupported(&'static str),
    UnsupportedType(String),
    MissingKeyField { ty: String, field: String },
}

impl core::fmt::Display for ShaderHeaderGenError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingMapping => f.write_str("shader map slot is missing mapping"),
            Self::UnknownNativeShape(name) => write!(f, "unknown native shader shape {name:?}"),
            Self::Unsupported(message) => f.write_str(message),
            Self::UnsupportedType(ty) => write!(f, "unsupported shader value type {ty}"),
            Self::MissingKeyField { ty, field } => {
                write!(f, "shader mapping key field {field:?} is missing from {ty}")
            }
        }
    }
}

impl core::error::Error for ShaderHeaderGenError {}

fn emit_native_struct_if_needed(
    value_ref: &ShaderValueShapeRef,
    registry: &SlotShapeRegistry,
    emitted: &mut Vec<String>,
    out: &mut String,
) -> Result<(), ShaderHeaderGenError> {
    if !value_ref.is_native() || emitted.iter().any(|name| name == value_ref.as_str()) {
        return Ok(());
    }

    let ty = lp_type_for_ref(value_ref, registry)?;
    let LpType::Struct { name, fields } = ty else {
        return Err(ShaderHeaderGenError::Unsupported(
            "native shader header refs must be struct values",
        ));
    };
    let name = name.ok_or(ShaderHeaderGenError::Unsupported(
        "native shader header structs must have a name",
    ))?;

    writeln!(out, "struct {name} {{").expect("write string");
    for field in fields {
        let glsl_ty = glsl_type_for_lp_type(&field.ty)?;
        writeln!(out, "    {glsl_ty} {};", field.name).expect("write string");
    }
    writeln!(out, "}};").expect("write string");
    writeln!(out).expect("write string");

    emitted.push(String::from(value_ref.as_str()));
    Ok(())
}

fn validate_key_field(
    value_ref: &ShaderValueShapeRef,
    registry: &SlotShapeRegistry,
    key: &str,
) -> Result<(), ShaderHeaderGenError> {
    let ty = lp_type_for_ref(value_ref, registry)?;
    let LpType::Struct { name, fields } = ty else {
        return Err(ShaderHeaderGenError::Unsupported(
            "sentinel mappings require struct values",
        ));
    };
    if fields.iter().any(|field| field.name == key) {
        return Ok(());
    }
    Err(ShaderHeaderGenError::MissingKeyField {
        ty: name.unwrap_or_else(|| String::from("<anonymous>")),
        field: String::from(key),
    })
}

fn glsl_type_for_ref(
    value_ref: &ShaderValueShapeRef,
    registry: &SlotShapeRegistry,
) -> Result<String, ShaderHeaderGenError> {
    if let Some(ty) = value_ref.as_lp_type() {
        return glsl_type_for_lp_type(&ty);
    }
    let LpType::Struct {
        name: Some(name), ..
    } = lp_type_for_ref(value_ref, registry)?
    else {
        return Err(ShaderHeaderGenError::UnsupportedType(String::from(
            value_ref.as_str(),
        )));
    };
    Ok(name)
}

fn lp_type_for_ref(
    value_ref: &ShaderValueShapeRef,
    registry: &SlotShapeRegistry,
) -> Result<LpType, ShaderHeaderGenError> {
    if let Some(ty) = value_ref.as_lp_type() {
        return Ok(ty);
    }
    let id = crate::SlotShapeId::from_static_name(value_ref.as_str());
    let shape = registry.get_shape(id).ok_or_else(|| {
        ShaderHeaderGenError::UnknownNativeShape(String::from(value_ref.as_str()))
    })?;
    shape
        .value_shape()
        .map(|shape| shape.ty_owned())
        .ok_or(ShaderHeaderGenError::Unsupported(
            "native shader refs must resolve to value shapes",
        ))
}

/// The single [`LpType`] → GLSL type-name mapping: used by the generated
/// uniform header above and by the studio's editor completions, so the type
/// name a completion shows is exactly the one the header declares.
pub fn glsl_type_for_lp_type(ty: &LpType) -> Result<String, ShaderHeaderGenError> {
    Ok(match ty {
        LpType::F32 => String::from("float"),
        LpType::U32 => String::from("uint"),
        LpType::I32 => String::from("int"),
        LpType::Bool => String::from("bool"),
        LpType::Vec2 => String::from("vec2"),
        LpType::Vec3 => String::from("vec3"),
        LpType::Vec4 => String::from("vec4"),
        LpType::Struct {
            name: Some(name), ..
        } => name.clone(),
        other => return Err(ShaderHeaderGenError::UnsupportedType(format!("{other:?}"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetSlot, MapSlot, ShaderSlotDef, ShaderSlotMappingDef};
    use lp_collection::VecMap;

    #[test]
    fn shader_header_generates_fluid_emitter_output() {
        let registry = SlotShapeRegistry::default();

        let mut consumed = VecMap::new();
        consumed.insert(
            String::from("time"),
            ShaderSlotDef {
                default_bind: crate::OptionSlot::none(),
                panel: crate::OptionSlot::none(),
                kind: crate::ValueSlot::new(ShaderSlotKind::Value),
                value: crate::ValueSlot::new(ShaderValueShapeRef::builtin("f32")),
                key: crate::OptionSlot::none(),
                phasor: crate::OptionSlot::none(),
                gradient: crate::OptionSlot::none(),
                default: crate::OptionSlot::none(),
                min: crate::OptionSlot::none(),
                max: crate::OptionSlot::none(),
                step: crate::OptionSlot::none(),
                mapping: crate::OptionSlot::none(),
                label: crate::ValueSlot::default(),
                description: crate::ValueSlot::default(),
                unit: crate::OptionSlot::none(),
            },
        );

        let mut produced = VecMap::new();
        produced.insert(
            String::from("emitters"),
            ShaderSlotDef::map_u32_native(
                "lp::fluid::Emitter",
                ShaderSlotMappingDef::sentinel(4, "id", 0),
            ),
        );

        let def = ComputeShaderDef {
            source: AssetSlot::path("emitters.glsl"),
            bindings: crate::BindingDefs::default(),
            float_mode: crate::ValueSlot::default(),
            consumed_slots: MapSlot::new(consumed),
            produced_slots: MapSlot::new(produced),
        };

        let header = generate_compute_shader_header(&def, &registry).expect("header");

        assert!(header.contains("struct FluidEmitter"));
        assert!(header.contains("uint id;"));
        assert!(header.contains("layout(binding = 0) uniform float time;"));
        assert!(header.contains("// produced: emitters"));
        assert!(header.contains("FluidEmitter emitters[4];"));
        assert!(!header.contains("out FluidEmitter"));
    }

    /// The timebase kinds are invisible to GLSL: the header declares the same
    /// `float` uniform a value slot would, which is exactly why the shader
    /// compiler, the ABI and the filetests need no changes for them.
    #[test]
    fn timebase_kinds_declare_plain_float_uniforms() {
        let mut consumed = VecMap::new();
        consumed.insert(
            String::from("elapsed"),
            ShaderSlotDef::seconds("Elapsed", ""),
        );
        consumed.insert(
            String::from("wave"),
            ShaderSlotDef::phasor("Wave", "", crate::PhasorConfig::with_period(2.0)),
        );
        let def = ComputeShaderDef {
            source: AssetSlot::path("wave.glsl"),
            bindings: crate::BindingDefs::default(),
            float_mode: crate::ValueSlot::default(),
            consumed_slots: MapSlot::new(consumed),
            produced_slots: MapSlot::default(),
        };

        let header =
            generate_compute_shader_header(&def, &SlotShapeRegistry::default()).expect("header");

        assert!(
            header.contains("layout(binding = 0) uniform float elapsed;"),
            "{header}"
        );
        assert!(
            header.contains("layout(binding = 1) uniform float wave;"),
            "{header}"
        );
        assert!(!header.contains("PhasorConfig"), "{header}");
    }

    #[test]
    fn shader_header_rejects_unknown_native_shape() {
        let mut produced = VecMap::new();
        produced.insert(
            String::from("emitters"),
            ShaderSlotDef::map_u32_native(
                "lp::fluid::Missing",
                ShaderSlotMappingDef::sentinel(4, "id", 0),
            ),
        );
        let def = ComputeShaderDef {
            source: AssetSlot::path("emitters.glsl"),
            bindings: crate::BindingDefs::default(),
            float_mode: crate::ValueSlot::default(),
            consumed_slots: MapSlot::default(),
            produced_slots: MapSlot::new(produced),
        };

        assert!(matches!(
            generate_compute_shader_header(&def, &SlotShapeRegistry::default()),
            Err(ShaderHeaderGenError::UnknownNativeShape(_))
        ));
    }
}
