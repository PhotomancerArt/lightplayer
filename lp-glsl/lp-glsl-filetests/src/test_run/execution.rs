//! Shared execution logic for filetests (`execute_function` / `execute_main`).

use anyhow::Result;

use lp_glsl_core::Type;
use lp_glsl_exec::GlslExecutable;
use lp_glsl_values::GlslValue;

/// Execute a function by name with arguments and return the result as a [`GlslValue`].
pub fn execute_function(
    executable: &mut dyn GlslExecutable,
    name: &str,
    args: &[GlslValue],
) -> Result<GlslValue> {
    let return_ty = executable
        .get_function_signature(name)
        .ok_or_else(|| anyhow::anyhow!("function '{name}' not found"))?
        .return_type
        .clone();

    fn format_error(
        e: lp_glsl_diagnostics::GlslError,
        executable: &dyn GlslExecutable,
    ) -> anyhow::Error {
        let error_msg = format!("{e:#}");
        if let Some(state) = executable.format_emulator_state() {
            anyhow::anyhow!("{error_msg}{state}")
        } else {
            anyhow::anyhow!("{error_msg}")
        }
    }

    match return_ty {
        Type::Float => executable
            .call_f32(name, args)
            .map(GlslValue::F32)
            .map_err(|e| format_error(e, executable)),
        Type::Int => executable
            .call_i32(name, args)
            .map(GlslValue::I32)
            .map_err(|e| format_error(e, executable)),
        Type::UInt => executable
            .call_i32(name, args)
            .map(|i| GlslValue::U32(i as u32))
            .map_err(|e| format_error(e, executable)),
        Type::Bool => executable
            .call_bool(name, args)
            .map(GlslValue::Bool)
            .map_err(|e| format_error(e, executable)),
        Type::Vec2 => executable
            .call_vec(name, args, 2)
            .map(|v| GlslValue::Vec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        Type::Vec3 => executable
            .call_vec(name, args, 3)
            .map(|v| GlslValue::Vec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        Type::Vec4 => executable
            .call_vec(name, args, 4)
            .map(|v| GlslValue::Vec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        Type::Mat2 => executable
            .call_mat(name, args, 2, 2)
            .map(|v| GlslValue::Mat2x2([[v[0], v[1]], [v[2], v[3]]]))
            .map_err(|e| format_error(e, executable)),
        Type::Mat3 => executable
            .call_mat(name, args, 3, 3)
            .map(|v| {
                GlslValue::Mat3x3([[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]])
            })
            .map_err(|e| format_error(e, executable)),
        Type::Mat4 => executable
            .call_mat(name, args, 4, 4)
            .map(|v| {
                GlslValue::Mat4x4([
                    [v[0], v[1], v[2], v[3]],
                    [v[4], v[5], v[6], v[7]],
                    [v[8], v[9], v[10], v[11]],
                    [v[12], v[13], v[14], v[15]],
                ])
            })
            .map_err(|e| format_error(e, executable)),
        Type::IVec2 => executable
            .call_ivec(name, args, 2)
            .map(|v| GlslValue::IVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        Type::IVec3 => executable
            .call_ivec(name, args, 3)
            .map(|v| GlslValue::IVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        Type::IVec4 => executable
            .call_ivec(name, args, 4)
            .map(|v| GlslValue::IVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        Type::BVec2 => executable
            .call_bvec(name, args, 2)
            .map(|v| GlslValue::BVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        Type::BVec3 => executable
            .call_bvec(name, args, 3)
            .map(|v| GlslValue::BVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        Type::BVec4 => executable
            .call_bvec(name, args, 4)
            .map(|v| GlslValue::BVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        Type::UVec2 => executable
            .call_uvec(name, args, 2)
            .map(|v| GlslValue::UVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        Type::UVec3 => executable
            .call_uvec(name, args, 3)
            .map(|v| GlslValue::UVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        Type::UVec4 => executable
            .call_uvec(name, args, 4)
            .map(|v| GlslValue::UVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        Type::Void => executable
            .call_void(name, args)
            .map(|_| GlslValue::F32(0.0))
            .map_err(|e| format_error(e, executable)),
        Type::Array(elem_ty, len) => executable
            .call_array(name, args, elem_ty.as_ref(), len)
            .map(|elements| GlslValue::Array(elements.into_boxed_slice()))
            .map_err(|e| format_error(e, executable)),
        other => anyhow::bail!("unsupported return type: {:?}", other),
    }
}

/// Execute `main()` and return the result as a [`GlslValue`].
pub fn execute_main(executable: &mut dyn GlslExecutable) -> Result<GlslValue> {
    execute_function(executable, "main", &[])
}
