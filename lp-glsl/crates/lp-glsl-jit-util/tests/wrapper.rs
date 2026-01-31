use cranelift_codegen::ir::types;
use cranelift_codegen::isa::CallConv;
use lp_glsl_jit_util::{StructReturnWrapper, wrap_structreturn_function};

#[test]
fn test_wrapper_creation() {
    let func_ptr = 0x1000 as *const u8;
    let wrapper =
        unsafe { StructReturnWrapper::<f32>::new(func_ptr, 3, CallConv::AppleAarch64, types::I64) };
    assert!(wrapper.is_ok());
}

#[test]
fn test_wrapper_error_null_pointer() {
    let wrapper = unsafe {
        StructReturnWrapper::<f32>::new(std::ptr::null(), 3, CallConv::AppleAarch64, types::I64)
    };
    assert!(wrapper.is_err());
}

#[test]
fn test_wrapper_error_zero_size() {
    let func_ptr = 0x1000 as *const u8;
    let wrapper =
        unsafe { StructReturnWrapper::<f32>::new(func_ptr, 0, CallConv::AppleAarch64, types::I64) };
    assert!(wrapper.is_err());
}

#[test]
fn test_wrapper_clone() {
    let func_ptr = 0x1000 as *const u8;
    let wrapper = unsafe {
        StructReturnWrapper::<f32>::new(func_ptr, 3, CallConv::AppleAarch64, types::I64).unwrap()
    };
    let cloned = wrapper.clone();
    assert_eq!(wrapper.buffer_size(), cloned.buffer_size());
}

#[test]
fn test_wrap_function() {
    let func_ptr = 0x1000 as *const u8;
    let wrapped = unsafe {
        wrap_structreturn_function::<f32>(func_ptr, 3, CallConv::AppleAarch64, types::I64)
    };
    assert!(wrapped.is_ok());
}
