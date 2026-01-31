use crate::call::call_structreturn;
use crate::error::JitCallError;
use core::clone::Clone;
use core::default::Default;
use core::marker::{Copy, PhantomData};
use core::ops::Fn;
use core::result::Result;
use cranelift_codegen::ir::Type;
use cranelift_codegen::isa::CallConv;

use alloc::{boxed::Box, vec::Vec};

/// A wrapper for a StructReturn function that provides a Rust-friendly interface.
///
/// This wrapper handles buffer allocation and calling convention details,
/// presenting a simple `Fn() -> Vec<T>` interface.
///
/// # Note
/// This wrapper is provided as a convenience API for users who want a higher-level
/// interface to StructReturn functions. The main `lp-glsl-compiler` crate uses the lower-level
/// `call_structreturn` function directly for performance reasons, but this wrapper
/// can be useful for applications that prefer a simpler API.
pub struct StructReturnWrapper<T> {
    func_ptr: *const u8,
    buffer_size: usize,
    call_conv: CallConv,
    pointer_type: Type,
    _phantom: PhantomData<T>,
}

impl<T> StructReturnWrapper<T>
where
    T: Copy + Default,
{
    /// Create a new wrapper for a StructReturn function.
    ///
    /// # Safety
    /// - `func_ptr` must be a valid function pointer to a JIT-compiled function
    /// - The function signature must match: `fn(*mut T) -> ()` with StructReturn
    /// - The calling convention must match the one used when compiling the function
    pub unsafe fn new(
        func_ptr: *const u8,
        buffer_size: usize,
        call_conv: CallConv,
        pointer_type: Type,
    ) -> Result<Self, JitCallError> {
        // Validate inputs
        if func_ptr.is_null() {
            return Err(JitCallError::NullFunctionPointer);
        }
        if buffer_size == 0 {
            return Err(JitCallError::ZeroBufferSize);
        }

        Ok(Self {
            func_ptr,
            buffer_size,
            call_conv,
            pointer_type,
            _phantom: PhantomData,
        })
    }

    /// Call the wrapped function and return the result.
    pub fn call(&self) -> Vec<T> {
        let mut buffer = Vec::new();
        buffer.resize(self.buffer_size, T::default());

        unsafe {
            call_structreturn(
                self.func_ptr,
                buffer.as_mut_ptr(),
                self.buffer_size,
                self.call_conv,
                self.pointer_type,
            )
            .unwrap_or_else(|e| {
                panic!("StructReturn call failed in wrapper: {}", e);
            });
        }

        buffer
    }

    /// Get the buffer size for this wrapper.
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}

impl<T> Clone for StructReturnWrapper<T> {
    fn clone(&self) -> Self {
        Self {
            func_ptr: self.func_ptr,
            buffer_size: self.buffer_size,
            call_conv: self.call_conv,
            pointer_type: self.pointer_type,
            _phantom: PhantomData,
        }
    }
}

/// Convenience function to create a boxed closure from a StructReturn function.
///
/// This is the primary API for wrapping StructReturn functions.
///
/// # Safety
/// - `func_ptr` must be a valid function pointer to a JIT-compiled function
/// - The function signature must match: `fn(*mut T) -> ()` with StructReturn
/// - The calling convention must match the one used when compiling the function
/// - `buffer_size` must be correct for the return type `T`
///
/// # Note
/// This function is provided as a convenience API. The main `lp-glsl-compiler` crate uses
/// `call_structreturn` directly for performance, but this wrapper can be useful
/// for applications that prefer a simpler, higher-level interface.
pub unsafe fn wrap_structreturn_function<T>(
    func_ptr: *const u8,
    buffer_size: usize,
    call_conv: CallConv,
    pointer_type: Type,
) -> Result<Box<dyn Fn() -> Vec<T>>, JitCallError>
where
    T: Copy + Default + 'static,
{
    unsafe {
        let wrapper = StructReturnWrapper::new(func_ptr, buffer_size, call_conv, pointer_type)?;

        Ok(Box::new(move || wrapper.call()))
    }
}
