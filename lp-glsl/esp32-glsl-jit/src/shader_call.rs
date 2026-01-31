/// Direct shader function calling without abstraction overhead
use lp_glsl_jit_util::call_structreturn_with_args;

/// Call a vec4 shader function directly using q32 format
///
/// # Arguments
/// - `func_ptr`: Function pointer to the compiled shader
/// - `frag_coord`: Fragment coordinates [x, y] as q32 (i32)
/// - `output_size`: Output size [width, height] as q32 (i32)
/// - `time`: Time value as q32 (i32)
/// - `isa`: ISA for calling convention and pointer type
///
/// # Returns
/// Returns [r, g, b, a] as q32 (i32) values
pub unsafe fn call_vec4_shader(
    func_ptr: *const u8,
    frag_coord: [i32; 2],
    output_size: [i32; 2],
    time: i32,
    isa: &cranelift_codegen::isa::OwnedTargetIsa,
) -> Result<[i32; 4], lp_glsl_jit_util::JitCallError> {
    // Prepare JIT call arguments (i32 values as u64)
    // vec2 expands to 2 i32s each, so we have 5 i32 parameters total
    let jit_args = alloc::vec![
        frag_coord[0] as u64,  // fragCoord.x
        frag_coord[1] as u64,  // fragCoord.y
        output_size[0] as u64, // outputSize.x
        output_size[1] as u64, // outputSize.y
        time as u64,           // time
    ];

    // vec4 return value buffer (4 i32s = 16 bytes)
    let mut result_buffer = [0u8; 16];

    // Call the shader function with StructReturn
    unsafe {
        call_structreturn_with_args(
            func_ptr,
            result_buffer.as_mut_ptr(),
            16, // 4 i32s = 16 bytes
            &jit_args,
            isa.default_call_conv(),
            isa.pointer_type(),
        )?;
    }

    // Extract vec4 result (r, g, b, a) as i32 q32 values from buffer
    let r = i32::from_le_bytes([
        result_buffer[0],
        result_buffer[1],
        result_buffer[2],
        result_buffer[3],
    ]);
    let g = i32::from_le_bytes([
        result_buffer[4],
        result_buffer[5],
        result_buffer[6],
        result_buffer[7],
    ]);
    let b = i32::from_le_bytes([
        result_buffer[8],
        result_buffer[9],
        result_buffer[10],
        result_buffer[11],
    ]);
    let a = i32::from_le_bytes([
        result_buffer[12],
        result_buffer[13],
        result_buffer[14],
        result_buffer[15],
    ]);

    Ok([r, g, b, a])
}
