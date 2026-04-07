//! Shared execution logic for filetests (`execute_function` / `execute_render`).

use anyhow::Result;

use lps_exec::GlslExecutable;
use lps_shared::LpsType;
use lpvm::LpsValueF32;

/// Execute a function by name with arguments and return the result as a [`LpsValueF32`].
pub fn execute_function(
    executable: &mut dyn GlslExecutable,
    name: &str,
    args: &[LpsValueF32],
) -> Result<LpsValueF32> {
    let return_ty = executable
        .get_function_signature(name)
        .ok_or_else(|| anyhow::anyhow!("function '{name}' not found"))?
        .return_type
        .clone();

    fn format_error(
        e: lps_diagnostics::GlslError,
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
            .map(LpsValueF32::F32)
            .map_err(|e| format_error(e, executable)),
        LpsType::Int => executable
            .call_i32(name, args)
            .map(LpsValueF32::I32)
            .map_err(|e| format_error(e, executable)),
        LpsType::UInt => executable
            .call_i32(name, args)
            .map(|i| LpsValueF32::U32(i as u32))
            .map_err(|e| format_error(e, executable)),
        LpsType::Bool => executable
            .call_bool(name, args)
            .map(LpsValueF32::Bool)
            .map_err(|e| format_error(e, executable)),
        LpsType::Vec2 => executable
            .call_vec(name, args, 2)
            .map(|v| LpsValueF32::Vec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Vec3 => executable
            .call_vec(name, args, 3)
            .map(|v| LpsValueF32::Vec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Vec4 => executable
            .call_vec(name, args, 4)
            .map(|v| LpsValueF32::Vec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Mat2 => executable
            .call_mat(name, args, 2, 2)
            .map(|v| LpsValueF32::Mat2x2([[v[0], v[1]], [v[2], v[3]]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Mat3 => executable
            .call_mat(name, args, 3, 3)
            .map(|v| {
                LpsValueF32::Mat3x3([[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]])
            })
            .map_err(|e| format_error(e, executable)),
        LpsType::Mat4 => executable
            .call_mat(name, args, 4, 4)
            .map(|v| {
                LpsValueF32::Mat4x4([
                    [v[0], v[1], v[2], v[3]],
                    [v[4], v[5], v[6], v[7]],
                    [v[8], v[9], v[10], v[11]],
                    [v[12], v[13], v[14], v[15]],
                ])
            })
            .map_err(|e| format_error(e, executable)),
        LpsType::IVec2 => executable
            .call_ivec(name, args, 2)
            .map(|v| LpsValueF32::IVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::IVec3 => executable
            .call_ivec(name, args, 3)
            .map(|v| LpsValueF32::IVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::IVec4 => executable
            .call_ivec(name, args, 4)
            .map(|v| LpsValueF32::IVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::BVec2 => executable
            .call_bvec(name, args, 2)
            .map(|v| LpsValueF32::BVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::BVec3 => executable
            .call_bvec(name, args, 3)
            .map(|v| LpsValueF32::BVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::BVec4 => executable
            .call_bvec(name, args, 4)
            .map(|v| LpsValueF32::BVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::UVec2 => executable
            .call_uvec(name, args, 2)
            .map(|v| LpsValueF32::UVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::UVec3 => executable
            .call_uvec(name, args, 3)
            .map(|v| LpsValueF32::UVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::UVec4 => executable
            .call_uvec(name, args, 4)
            .map(|v| LpsValueF32::UVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        LpsType::Void => executable
            .call_void(name, args)
            .map(|_| LpsValueF32::F32(0.0))
            .map_err(|e| format_error(e, executable)),
        LpsType::Array { element, len } => executable
            .call_array(name, args, element.as_ref(), len as usize)
            .map(|elements| LpsValueF32::Array(elements.into_boxed_slice()))
            .map_err(|e| format_error(e, executable)),
        other => anyhow::bail!("unsupported return type: {:?}", other),
    }
}

/// Execute `render()` and return the result as a [`LpsValueF32`].
pub fn execute_render(executable: &mut dyn GlslExecutable) -> Result<LpsValueF32> {
    execute_function(executable, "render", &[])
}
