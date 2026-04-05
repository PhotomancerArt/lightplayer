//! Shared execution logic for filetests (`execute_function` / `execute_main`).

use anyhow::Result;

use lp_glsl_exec::GlslExecutable;
use lps_shared::LpsType;
use lpvm::LpsValue;

/// Execute a function by name with arguments and return the result as a [`LpsValue`].
pub fn execute_function(
    executable: &mut dyn GlslExecutable,
    name: &str,
    args: &[LpsValue],
) -> Result<LpsValue> {
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
        LpsType::Float => executable
            .call_f32(name, args)
            .map(LpsValue::F32)
            .map_err(|e| format_error(e, executable)),
        LpsType::Int => executable
            .call_i32(name, args)
            .map(LpsValue::I32)
            .map_err(|e| format_error(e, executable)),
        LpsType::UInt => executable
            .call_i32(name, args)
            .map(|i| LpsValue::U32(i as u32))
            .map_err(|e| format_error(e, executable)),
        LpsType::Bool => executable
            .call_bool(name, args)
            .map(LpsValue::Bool)
            .map_err(|e| format_error(e, executable)),
        LpsType::Vec2 => executable
            .call_vec(name, args, 2)
            .map(|v| LpsValue::Vec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Vec3 => executable
            .call_vec(name, args, 3)
            .map(|v| LpsValue::Vec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Vec4 => executable
            .call_vec(name, args, 4)
            .map(|v| LpsValue::Vec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Mat2 => executable
            .call_mat(name, args, 2, 2)
            .map(|v| LpsValue::Mat2x2([[v[0], v[1]], [v[2], v[3]]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Mat3 => executable
            .call_mat(name, args, 3, 3)
            .map(|v| LpsValue::Mat3x3([[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Mat4 => executable
            .call_mat(name, args, 4, 4)
            .map(|v| {
                LpsValue::Mat4x4([
                    [v[0], v[1], v[2], v[3]],
                    [v[4], v[5], v[6], v[7]],
                    [v[8], v[9], v[10], v[11]],
                    [v[12], v[13], v[14], v[15]],
                ])
            })
            .map_err(|e| format_error(e, executable)),
        LpsType::IVec2 => executable
            .call_ivec(name, args, 2)
            .map(|v| LpsValue::IVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::IVec3 => executable
            .call_ivec(name, args, 3)
            .map(|v| LpsValue::IVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::IVec4 => executable
            .call_ivec(name, args, 4)
            .map(|v| LpsValue::IVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::BVec2 => executable
            .call_bvec(name, args, 2)
            .map(|v| LpsValue::BVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::BVec3 => executable
            .call_bvec(name, args, 3)
            .map(|v| LpsValue::BVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::BVec4 => executable
            .call_bvec(name, args, 4)
            .map(|v| LpsValue::BVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::UVec2 => executable
            .call_uvec(name, args, 2)
            .map(|v| LpsValue::UVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::UVec3 => executable
            .call_uvec(name, args, 3)
            .map(|v| LpsValue::UVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::UVec4 => executable
            .call_uvec(name, args, 4)
            .map(|v| LpsValue::UVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Void => executable
            .call_void(name, args)
            .map(|_| LpsValue::F32(0.0))
            .map_err(|e| format_error(e, executable)),
        LpsType::Array { element, len } => executable
            .call_array(name, args, element.as_ref(), len as usize)
            .map(|elements| LpsValue::Array(elements.into_boxed_slice()))
            .map_err(|e| format_error(e, executable)),
        other => anyhow::bail!("unsupported return type: {:?}", other),
    }
}

/// Execute `main()` and return the result as a [`LpsValue`].
pub fn execute_main(executable: &mut dyn GlslExecutable) -> Result<LpsValue> {
    execute_function(executable, "main", &[])
}
