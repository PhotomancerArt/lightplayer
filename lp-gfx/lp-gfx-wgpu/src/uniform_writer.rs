//! Serialize an engine `LpsValueF32` uniform tree into uniform-buffer bytes
//! at the offsets naga's layout records on the validated module.
//!
//! The traversal mirrors the CPU tier's convention
//! (`lp-shader/src/px_shader.rs::apply_uniform_fields`): the engine hands a
//! root `LpsValueF32::Struct` whose fields are keyed by top-level GLSL
//! uniform name; nested struct members resolve by name (dotted paths in
//! errors), and a shader uniform with no matching field is an error while
//! extra fields are ignored.

use lp_gfx::GfxError;
use lps_shared::LpsValueF32;
use naga::{Handle, Type, TypeInner};

use crate::uniform_layout::UniformTable;

/// Encode every reflected uniform global from the engine uniform tree.
/// Returns `(binding, bytes)` per global, in table (binding) order.
pub fn encode_uniforms(
    module: &naga::Module,
    table: &UniformTable,
    uniforms: &LpsValueF32,
) -> Result<Vec<(u32, Vec<u8>)>, GfxError> {
    let empty = Vec::new();
    let fields = match uniforms {
        LpsValueF32::Struct { fields, .. } => fields,
        // No-uniform shaders accept any value; with uniforms, require the
        // struct shape the engine produces.
        _ if table.globals.is_empty() => &empty,
        _ => {
            return Err(GfxError::Render(String::from(
                "expected uniforms as LpsValueF32::Struct",
            )));
        }
    };

    table
        .globals
        .iter()
        .map(|global| {
            let value = fields
                .iter()
                .find(|(name, _)| *name == global.name)
                .map(|(_, value)| value)
                .ok_or_else(|| {
                    GfxError::Render(format!("missing uniform field `{}`", global.name))
                })?;
            let mut bytes = vec![0u8; global.size as usize];
            write_value(module, global.ty, value, &global.name, &mut bytes, 0)?;
            Ok((global.binding, bytes))
        })
        .collect()
}

/// Write one value of naga type `ty` at `offset` into `out`.
fn write_value(
    module: &naga::Module,
    ty: Handle<Type>,
    value: &LpsValueF32,
    path: &str,
    out: &mut [u8],
    offset: usize,
) -> Result<(), GfxError> {
    match (&module.types[ty].inner, value) {
        (TypeInner::Scalar(scalar), _) => write_scalar(*scalar, value, path, out, offset),
        (TypeInner::Vector { size, scalar }, _) => {
            let lanes = lanes_of(value, path)?;
            if lanes.len() != *size as usize {
                return Err(type_mismatch(path, "vector lane count", value));
            }
            for (i, lane) in lanes.iter().enumerate() {
                write_scalar_lane(*scalar, *lane, path, out, offset + i * 4)?;
            }
            Ok(())
        }
        (
            TypeInner::Matrix {
                columns,
                rows,
                scalar,
            },
            _,
        ) => {
            if scalar.kind != naga::ScalarKind::Float || scalar.width != 4 {
                return Err(GfxError::Render(format!(
                    "uniform `{path}`: unsupported matrix scalar {scalar:?}"
                )));
            }
            let cols = matrix_columns(value, path)?;
            if cols.len() != *columns as usize || cols[0].len() != *rows as usize {
                return Err(type_mismatch(path, "matrix shape", value));
            }
            // Column stride per naga/WGSL rules: vec2 columns are 8-byte
            // aligned, vec3/vec4 columns 16 — i.e. the column vector's
            // alignment, which equals size / columns of the whole matrix.
            let column_stride = matrix_column_stride(*rows);
            for (c, column) in cols.iter().enumerate() {
                for (r, v) in column.iter().enumerate() {
                    write_bytes(out, offset + c * column_stride + r * 4, &v.to_le_bytes());
                }
            }
            Ok(())
        }
        (TypeInner::Struct { members, .. }, LpsValueF32::Struct { fields, .. }) => {
            for member in members {
                let name = member.name.as_deref().ok_or_else(|| {
                    GfxError::Render(format!("uniform `{path}`: struct member has no name"))
                })?;
                let member_path = format!("{path}.{name}");
                let value = fields
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, v)| v)
                    .ok_or_else(|| {
                        GfxError::Render(format!("missing uniform field `{member_path}`"))
                    })?;
                write_value(
                    module,
                    member.ty,
                    value,
                    &member_path,
                    out,
                    offset + member.offset as usize,
                )?;
            }
            Ok(())
        }
        (TypeInner::Struct { .. }, _) => Err(type_mismatch(path, "struct", value)),
        (
            TypeInner::Array {
                base,
                size: naga::ArraySize::Constant(len),
                stride,
            },
            LpsValueF32::Array(elements),
        ) => {
            if elements.len() != len.get() as usize {
                return Err(GfxError::Render(format!(
                    "uniform `{path}`: array length mismatch (shader wants {}, value has {})",
                    len.get(),
                    elements.len()
                )));
            }
            for (i, element) in elements.iter().enumerate() {
                write_value(
                    module,
                    *base,
                    element,
                    &format!("{path}[{i}]"),
                    out,
                    offset + i * *stride as usize,
                )?;
            }
            Ok(())
        }
        (TypeInner::Array { .. }, _) => Err(type_mismatch(path, "array", value)),
        // Texture uniforms never reach this writer: naga puts sampler2D
        // globals in the handle address space, so they are reflected as
        // texture bindings and their `LpsValueF32::Texture2D` values become
        // bind-group entries (`crate::render`), not uniform-buffer bytes.
        (other, _) => Err(GfxError::Render(format!(
            "uniform `{path}`: type {other:?} is not supported in a WGSL uniform block"
        ))),
    }
}

fn write_scalar(
    scalar: naga::Scalar,
    value: &LpsValueF32,
    path: &str,
    out: &mut [u8],
    offset: usize,
) -> Result<(), GfxError> {
    use naga::ScalarKind;
    match (scalar.kind, value) {
        (ScalarKind::Float, LpsValueF32::F32(v)) => {
            write_bytes(out, offset, &v.to_le_bytes());
            Ok(())
        }
        (ScalarKind::Sint, LpsValueF32::I32(v)) => {
            write_bytes(out, offset, &v.to_le_bytes());
            Ok(())
        }
        (ScalarKind::Uint, LpsValueF32::U32(v)) => {
            write_bytes(out, offset, &v.to_le_bytes());
            Ok(())
        }
        _ => Err(type_mismatch(path, "scalar", value)),
    }
}

/// One lane of an already-validated vector value.
#[derive(Clone, Copy)]
enum Lane {
    F32(f32),
    I32(i32),
    U32(u32),
}

fn write_scalar_lane(
    scalar: naga::Scalar,
    lane: Lane,
    path: &str,
    out: &mut [u8],
    offset: usize,
) -> Result<(), GfxError> {
    use naga::ScalarKind;
    match (scalar.kind, lane) {
        (ScalarKind::Float, Lane::F32(v)) => {
            write_bytes(out, offset, &v.to_le_bytes());
            Ok(())
        }
        (ScalarKind::Sint, Lane::I32(v)) => {
            write_bytes(out, offset, &v.to_le_bytes());
            Ok(())
        }
        (ScalarKind::Uint, Lane::U32(v)) => {
            write_bytes(out, offset, &v.to_le_bytes());
            Ok(())
        }
        _ => Err(GfxError::Render(format!(
            "uniform `{path}`: vector scalar kind mismatch"
        ))),
    }
}

fn lanes_of(value: &LpsValueF32, path: &str) -> Result<Vec<Lane>, GfxError> {
    Ok(match value {
        LpsValueF32::Vec2(v) => v.iter().map(|&x| Lane::F32(x)).collect(),
        LpsValueF32::Vec3(v) => v.iter().map(|&x| Lane::F32(x)).collect(),
        LpsValueF32::Vec4(v) => v.iter().map(|&x| Lane::F32(x)).collect(),
        LpsValueF32::IVec2(v) => v.iter().map(|&x| Lane::I32(x)).collect(),
        LpsValueF32::IVec3(v) => v.iter().map(|&x| Lane::I32(x)).collect(),
        LpsValueF32::IVec4(v) => v.iter().map(|&x| Lane::I32(x)).collect(),
        LpsValueF32::UVec2(v) => v.iter().map(|&x| Lane::U32(x)).collect(),
        LpsValueF32::UVec3(v) => v.iter().map(|&x| Lane::U32(x)).collect(),
        LpsValueF32::UVec4(v) => v.iter().map(|&x| Lane::U32(x)).collect(),
        other => return Err(type_mismatch(path, "vector", other)),
    })
}

fn matrix_columns(value: &LpsValueF32, path: &str) -> Result<Vec<Vec<f32>>, GfxError> {
    Ok(match value {
        LpsValueF32::Mat2x2(m) => m.iter().map(|c| c.to_vec()).collect(),
        LpsValueF32::Mat3x3(m) => m.iter().map(|c| c.to_vec()).collect(),
        LpsValueF32::Mat4x4(m) => m.iter().map(|c| c.to_vec()).collect(),
        other => return Err(type_mismatch(path, "matrix", other)),
    })
}

/// WGSL matrix column stride: columns are aligned like their vector type
/// (vec2 → 8, vec3/vec4 → 16).
fn matrix_column_stride(rows: naga::VectorSize) -> usize {
    match rows {
        naga::VectorSize::Bi => 8,
        naga::VectorSize::Tri | naga::VectorSize::Quad => 16,
    }
}

fn write_bytes(out: &mut [u8], offset: usize, bytes: &[u8]) {
    out[offset..offset + bytes.len()].copy_from_slice(bytes);
}

fn type_mismatch(path: &str, expected: &str, value: &LpsValueF32) -> GfxError {
    GfxError::Render(format!(
        "uniform `{path}`: shader expects {expected}, engine value is {value:?}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uniform_layout::reflect_uniforms;
    use crate::wgsl_compile::compile_wgsl;

    fn encode_shader(
        authored: &str,
        uniforms: LpsValueF32,
    ) -> Result<Vec<(u32, Vec<u8>)>, GfxError> {
        let shader = compile_wgsl(authored, &lp_shader::TextureBindingSpecs::new())
            .expect("shader compiles");
        let table = reflect_uniforms(&shader.module).expect("uniforms reflect");
        encode_uniforms(&shader.module, &table, &uniforms)
    }

    fn f32_at(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn root(fields: Vec<(&str, LpsValueF32)>) -> LpsValueF32 {
        LpsValueF32::Struct {
            name: None,
            fields: fields
                .into_iter()
                .map(|(n, v)| (String::from(n), v))
                .collect(),
        }
    }

    #[test]
    fn scalar_and_vec2_globals_round_trip() {
        let encoded = encode_shader(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             layout(binding = 1) uniform float time;\n\
             vec4 render(vec2 pos) { return vec4(pos / outputSize, time, 1.0); }\n",
            root(vec![
                ("outputSize", LpsValueF32::Vec2([128.0, 64.0])),
                ("time", LpsValueF32::F32(2.5)),
            ]),
        )
        .expect("encodes");
        assert_eq!(encoded.len(), 2);
        let (binding0, size_bytes) = &encoded[0];
        assert_eq!(*binding0, 0);
        assert_eq!(size_bytes.len(), 8);
        assert_eq!(f32_at(size_bytes, 0), 128.0);
        assert_eq!(f32_at(size_bytes, 4), 64.0);
        let (binding1, time_bytes) = &encoded[1];
        assert_eq!(*binding1, 1);
        assert_eq!(f32_at(time_bytes, 0), 2.5);
    }

    #[test]
    fn vec3_padding_follows_naga_layout() {
        // The classic hand-written-std140 mistake: vec3 is 16-aligned but
        // 12-sized. Assert the writer uses naga's member offsets, whatever
        // they are, by reading the reflected struct layout back.
        let shader = compile_wgsl(
            "layout(binding = 0) uniform Block { vec3 a; vec2 b; } blk;\n\
             vec4 render(vec2 pos) { return vec4(blk.a + vec3(blk.b, 0.0), 1.0); }\n",
            &lp_shader::TextureBindingSpecs::new(),
        )
        .expect("compiles");
        let table = reflect_uniforms(&shader.module).expect("reflects");
        assert_eq!(table.globals.len(), 1);
        let global = &table.globals[0];
        let naga::TypeInner::Struct { members, .. } = &shader.module.types[global.ty].inner else {
            panic!("uniform block reflects as struct");
        };
        assert_eq!(members[0].offset, 0, "vec3 a at 0");
        assert_eq!(members[1].offset, 16, "vec2 b after vec3 padding");

        let encoded = encode_uniforms(
            &shader.module,
            &table,
            &root(vec![(
                "blk",
                root(vec![
                    ("a", LpsValueF32::Vec3([1.0, 2.0, 3.0])),
                    ("b", LpsValueF32::Vec2([4.0, 5.0])),
                ]),
            )]),
        )
        .expect("encodes");
        let bytes = &encoded[0].1;
        assert_eq!(f32_at(bytes, 0), 1.0);
        assert_eq!(f32_at(bytes, 8), 3.0);
        assert_eq!(f32_at(bytes, 16), 4.0);
        assert_eq!(f32_at(bytes, 20), 5.0);
    }

    #[test]
    fn mat3_columns_are_16_byte_strided() {
        let shader = compile_wgsl(
            "layout(binding = 0) uniform Block { mat3 m; float tail; } blk;\n\
             vec4 render(vec2 pos) { return vec4(blk.m * vec3(pos, blk.tail), 1.0); }\n",
            &lp_shader::TextureBindingSpecs::new(),
        )
        .expect("compiles");
        let table = reflect_uniforms(&shader.module).expect("reflects");
        let global = &table.globals[0];
        let naga::TypeInner::Struct { members, .. } = &shader.module.types[global.ty].inner else {
            panic!("struct");
        };
        assert_eq!(members[1].offset, 48, "float lands after mat3 (3×16)");

        let m = LpsValueF32::Mat3x3([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        let encoded = encode_uniforms(
            &shader.module,
            &table,
            &root(vec![(
                "blk",
                root(vec![("m", m), ("tail", LpsValueF32::F32(42.0))]),
            )]),
        )
        .expect("encodes");
        let bytes = &encoded[0].1;
        assert_eq!(f32_at(bytes, 0), 1.0, "col0 row0");
        assert_eq!(f32_at(bytes, 8), 3.0, "col0 row2");
        assert_eq!(f32_at(bytes, 16), 4.0, "col1 starts at 16, not 12");
        assert_eq!(f32_at(bytes, 32), 7.0, "col2 at 32");
        assert_eq!(f32_at(bytes, 48), 42.0, "tail after matrix");
    }

    #[test]
    fn nested_structs_resolve_members_by_name() {
        let shader = compile_wgsl(
            "struct Inner { float x; vec3 v; };\n\
             layout(binding = 0) uniform Block { Inner inner; float after; } blk;\n\
             vec4 render(vec2 pos) { return vec4(blk.inner.v * blk.inner.x, blk.after); }\n",
            &lp_shader::TextureBindingSpecs::new(),
        )
        .expect("compiles");
        let table = reflect_uniforms(&shader.module).expect("reflects");
        let encoded = encode_uniforms(
            &shader.module,
            &table,
            &root(vec![(
                "blk",
                root(vec![
                    (
                        "inner",
                        root(vec![
                            ("x", LpsValueF32::F32(1.5)),
                            ("v", LpsValueF32::Vec3([7.0, 8.0, 9.0])),
                        ]),
                    ),
                    ("after", LpsValueF32::F32(-1.0)),
                ]),
            )]),
        )
        .expect("encodes");
        let bytes = &encoded[0].1;
        // Inner: x at 0; v is 16-aligned → 16; inner size 32 (struct align
        // 16); after at 32.
        assert_eq!(f32_at(bytes, 0), 1.5);
        assert_eq!(f32_at(bytes, 16), 7.0);
        assert_eq!(f32_at(bytes, 32), -1.0);
    }

    #[test]
    fn missing_uniform_field_errors_with_the_dotted_path() {
        let err = encode_shader(
            "layout(binding = 0) uniform float time;\n\
             vec4 render(vec2 pos) { return vec4(time); }\n",
            root(vec![("unrelated", LpsValueF32::F32(0.0))]),
        )
        .expect_err("must fail");
        match err {
            GfxError::Render(message) => {
                assert!(message.contains("time"), "names the uniform: {message}");
            }
            other => panic!("expected GfxError::Render, got {other:?}"),
        }
    }

    #[test]
    fn type_mismatch_errors_clearly() {
        let err = encode_shader(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render(vec2 pos) { return vec4(pos / outputSize, 0.0, 1.0); }\n",
            root(vec![("outputSize", LpsValueF32::F32(128.0))]),
        )
        .expect_err("must fail");
        match err {
            GfxError::Render(message) => {
                assert!(message.contains("outputSize"), "{message}");
            }
            other => panic!("expected GfxError::Render, got {other:?}"),
        }
    }
}
