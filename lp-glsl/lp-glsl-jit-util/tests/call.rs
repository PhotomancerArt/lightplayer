use cranelift_codegen::ir::types;
use cranelift_codegen::isa::CallConv;
use lp_glsl_jit_util::call::call_structreturn;

#[test]
fn test_error_handling_null_function_pointer() {
    let mut buffer = vec![0.0f32; 3];
    let result = unsafe {
        call_structreturn(
            std::ptr::null(),
            buffer.as_mut_ptr(),
            3,
            CallConv::AppleAarch64,
            types::I64,
        )
    };
    assert!(result.is_err());
}

#[test]
fn test_error_handling_null_buffer() {
    let result = unsafe {
        call_structreturn(
            0x1000 as *const u8,
            std::ptr::null_mut::<f32>(),
            3,
            CallConv::AppleAarch64,
            types::I64,
        )
    };
    assert!(result.is_err());
}

#[test]
fn test_error_handling_zero_buffer_size() {
    let mut buffer = vec![0.0f32; 3];
    let result = unsafe {
        call_structreturn(
            0x1000 as *const u8,
            buffer.as_mut_ptr(),
            0,
            CallConv::AppleAarch64,
            types::I64,
        )
    };
    assert!(result.is_err());
}

#[test]
fn test_error_handling_unsupported_calling_convention() {
    let mut buffer = vec![0.0f32; 3];
    let result = unsafe {
        call_structreturn(
            0x1000 as *const u8,
            buffer.as_mut_ptr(),
            3,
            CallConv::Fast, // Unsupported
            types::I64,
        )
    };
    assert!(result.is_err());
}
