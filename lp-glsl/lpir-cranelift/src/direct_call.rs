//! Level-3 flat `i32` buffer calling convention.

use cranelift_codegen::ir::Type;
use cranelift_codegen::isa::CallConv;

use crate::jit_module::JitModule;
use crate::values::CallError;

/// Raw JIT function pointer plus arity for [`DirectCall::call_i32`].
#[derive(Clone, Copy, Debug)]
pub struct DirectCall {
    pub func_ptr: *const u8,
    pub call_conv: CallConv,
    pub pointer_type: Type,
    pub arg_i32_count: usize,
    pub ret_i32_count: usize,
}

// SAFETY: Points at finalized JIT code; not used across threads concurrently for mutation.
unsafe impl Send for DirectCall {}
unsafe impl Sync for DirectCall {}

impl JitModule {
    /// Level-3 handle: [`DirectCall::call_i32`] uses the same `extern "C"` layout as [`crate::invoke`].
    pub fn direct_call(&self, name: &str) -> Option<DirectCall> {
        let sig = self.signatures.get(name)?;
        Some(DirectCall {
            func_ptr: self.finalized_ptr(name)?,
            call_conv: self.call_conv,
            pointer_type: self.pointer_type,
            arg_i32_count: sig.params.len(),
            ret_i32_count: sig.returns.len(),
        })
    }
}

impl DirectCall {
    /// # Safety
    /// `func_ptr` must match the compiled signature (`arg_i32_count` / `ret_i32_count`).
    pub unsafe fn call_i32(&self, args: &[i32]) -> Result<alloc::vec::Vec<i32>, CallError> {
        if args.len() != self.arg_i32_count {
            return Err(CallError::Arity {
                expected: self.arg_i32_count,
                got: args.len(),
            });
        }
        unsafe { crate::invoke::invoke_i32_args_returns(self.func_ptr, args, self.ret_i32_count) }
    }

    /// Like [`Self::call_i32`] but writes return words into `out` (no heap allocation).
    ///
    /// # Safety
    /// Same as [`Self::call_i32`]. `out.len()` must equal `ret_i32_count`.
    pub unsafe fn call_i32_buf(&self, args: &[i32], out: &mut [i32]) -> Result<(), CallError> {
        if args.len() != self.arg_i32_count {
            return Err(CallError::Arity {
                expected: self.arg_i32_count,
                got: args.len(),
            });
        }
        if out.len() != self.ret_i32_count {
            return Err(CallError::TypeMismatch(alloc::format!(
                "return buffer length {} does not match {} return word(s)",
                out.len(),
                self.ret_i32_count
            )));
        }
        unsafe {
            crate::invoke::invoke_i32_args_returns_buf(self.func_ptr, args, self.ret_i32_count, out)
        }
    }
}
