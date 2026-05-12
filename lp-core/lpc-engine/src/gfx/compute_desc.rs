//! Build shader runtime compute descriptors from authored model definitions.

use alloc::format;
use alloc::string::String;

use lp_shader::CompileComputeDesc;
use lpc_model::{
    ComputeShaderDef, ShaderSlotDef, ShaderSlotKind, ShaderSlotMappingKind, ShaderValueShapeRef,
    SlotShape, SlotShapeRegistry,
};
use lpir::CompilerConfig;
use lps_shared::LpsType;

use crate::gfx::model_type_to_lps_type;

/// Convert an authored compute shader definition into a shader runtime descriptor.
///
/// The model owns the user-facing slot contract. The runtime descriptor owns
/// the lowered compiler ABI: consumed slots become uniforms and produced slots
/// become private globals.
pub fn compute_desc_from_model_def<'a>(
    glsl: &'a str,
    def: &ComputeShaderDef,
    registry: &SlotShapeRegistry,
    compiler_config: CompilerConfig,
) -> Result<CompileComputeDesc<'a>, ComputeDescError> {
    let mut desc = CompileComputeDesc::new(glsl, compiler_config);

    for (name, slot) in &def.consumed_slots.entries {
        let ty = lps_type_for_slot_value(slot.value.value(), registry)?;
        match slot.kind.value() {
            ShaderSlotKind::Value => {
                desc = desc.with_consumed(name.clone(), ty);
            }
            ShaderSlotKind::Map => {
                return Err(ComputeDescError::Unsupported(String::from(
                    "consumed map slots do not have a shader ABI projection yet",
                )));
            }
        }
    }

    for (name, slot) in &def.produced_slots.entries {
        let ty = lps_type_for_slot_value(slot.value.value(), registry)?;
        match slot.kind.value() {
            ShaderSlotKind::Value => {
                desc = desc.with_produced(name.clone(), ty);
            }
            ShaderSlotKind::Map => {
                ensure_u32_map_key(slot)?;
                let mapping = slot
                    .mapping
                    .data
                    .as_ref()
                    .ok_or(ComputeDescError::MissingMapping { slot: name.clone() })?;
                match mapping.kind.value() {
                    ShaderSlotMappingKind::Sentinel => {
                        desc = desc.with_sentinel_array_output(
                            name.clone(),
                            ty,
                            *mapping.len.value(),
                            mapping.key.value().clone(),
                        );
                    }
                }
            }
        }
    }

    Ok(desc)
}

/// Failure building a serial compute descriptor from an authored model def.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputeDescError {
    UnknownNativeShape(String),
    NativeShapeIsNotValue(String),
    MissingMapping { slot: String },
    Unsupported(String),
}

impl core::fmt::Display for ComputeDescError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownNativeShape(name) => write!(f, "unknown native shader shape {name:?}"),
            Self::NativeShapeIsNotValue(name) => {
                write!(f, "native shader shape {name:?} is not a value shape")
            }
            Self::MissingMapping { slot } => {
                write!(f, "shader map slot {slot:?} is missing mapping")
            }
            Self::Unsupported(message) => f.write_str(message),
        }
    }
}

impl core::error::Error for ComputeDescError {}

fn lps_type_for_slot_value(
    value_ref: &ShaderValueShapeRef,
    registry: &SlotShapeRegistry,
) -> Result<LpsType, ComputeDescError> {
    if let Some(ty) = value_ref.as_lp_type() {
        return Ok(model_type_to_lps_type(&ty));
    }

    let shape = registry
        .get_by_name(value_ref.as_str())
        .ok_or_else(|| ComputeDescError::UnknownNativeShape(String::from(value_ref.as_str())))?;
    match shape {
        SlotShape::Value { shape } => Ok(model_type_to_lps_type(&shape.ty)),
        _ => Err(ComputeDescError::NativeShapeIsNotValue(String::from(
            value_ref.as_str(),
        ))),
    }
}

fn ensure_u32_map_key(slot: &ShaderSlotDef) -> Result<(), ComputeDescError> {
    let Some(key) = slot.key.data.as_ref() else {
        return Err(ComputeDescError::Unsupported(String::from(
            "shader map slot is missing key type",
        )));
    };
    if key.value() == &lpc_model::ShaderMapKeyDef::U32 {
        Ok(())
    } else {
        Err(ComputeDescError::Unsupported(format!(
            "unsupported shader map key {:?}",
            key.value()
        )))
    }
}

#[cfg(all(test, not(any(target_arch = "riscv32", target_arch = "wasm32"))))]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::format;

    use lpc_model::{
        BindingDefs, FluidEmitter, MapSlot, ShaderSlotMappingDef, SourcePathSlot, StaticSlotShape,
        generate_compute_shader_header,
    };
    use lps_shared::LpsValueF32;
    use lpvm_wasm::WasmOptions;
    use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

    #[test]
    fn compute_def_header_and_runtime_descriptor_execute() {
        let mut registry = SlotShapeRegistry::default();
        FluidEmitter::ensure_registered(&mut registry).expect("fluid emitter");

        let mut consumed = BTreeMap::new();
        consumed.insert(
            String::from("time"),
            ShaderSlotDef::value_f32("Time", "Seconds", 0.0, None),
        );

        let mut produced = BTreeMap::new();
        produced.insert(
            String::from("emitters"),
            ShaderSlotDef::map_u32_native(
                "lp::fluid::Emitter",
                ShaderSlotMappingDef::sentinel(4, "id", 0),
            ),
        );

        let def = ComputeShaderDef {
            glsl_path: SourcePathSlot::new(String::from("emitters.glsl")),
            bindings: BindingDefs::default(),
            glsl_opts: lpc_model::GlslOpts::default(),
            consumed_slots: MapSlot::new(consumed),
            produced_slots: MapSlot::new(produced),
        };

        let header = generate_compute_shader_header(&def, &registry).expect("header");
        let glsl = format!(
            r#"{header}
void tick() {{
    emitters[0].id = 1u;
    emitters[0].pos = vec2(time, 0.75);
    emitters[0].dir = vec2(1.0, 0.0);
    emitters[0].radius = 0.125;
    emitters[0].color = vec3(1.0, 0.5, 0.25);
    emitters[0].velocity = 0.2;
    emitters[0].intensity = 0.8;
}}
"#
        );

        let desc =
            compute_desc_from_model_def(&glsl, &def, &registry, lpir::CompilerConfig::default())
                .expect("compute desc");
        let engine = lp_shader::LpsEngine::new(
            WasmLpvmEngine::new(WasmOptions::default()).expect("wasm engine"),
        );
        let shader = engine.compile_compute_desc(desc).expect("compile compute");

        shader
            .tick(&[("time", LpsValueF32::F32(0.25))])
            .expect("tick");
        let LpsValueF32::Array(items) = shader.get_output("emitters").expect("emitters") else {
            panic!("expected emitter array");
        };
        let LpsValueF32::Struct { fields, .. } = &items[0] else {
            panic!("expected emitter struct");
        };
        assert!(
            matches!(field(fields, "id"), Some(LpsValueF32::U32(1))),
            "fields: {fields:?}"
        );
        assert!(
            field(fields, "pos")
                .expect("pos")
                .approx_eq_default(&LpsValueF32::Vec2([0.25, 0.75]))
        );
    }

    fn field<'a>(fields: &'a [(String, LpsValueF32)], name: &str) -> Option<&'a LpsValueF32> {
        fields
            .iter()
            .find_map(|(field_name, value)| (field_name == name).then_some(value))
    }
}
