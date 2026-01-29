//! Low-level StructReturn calling functions.
//!
//! These functions handle platform-specific calling conventions for
//! calling JIT-compiled functions that use StructReturn.
use crate::error::JitCallError;
use core::marker::Copy;
use core::result::Result;
use cranelift_codegen::ir::{Type, types};
use cranelift_codegen::isa::CallConv;

use alloc::string::String;

/// Call a JIT-compiled function that uses StructReturn.
///
/// # Safety
/// - `func_ptr` must be a valid function pointer to a JIT-compiled function
/// - `buffer` must point to valid, writable memory of at least `buffer_size` bytes
/// - The function signature must match: `fn(*mut T) -> ()` where T is the element type
/// - The calling convention must match the one used when compiling the function
pub unsafe fn call_structreturn<T>(
    func_ptr: *const u8,
    buffer: *mut T,
    buffer_size: usize,
    call_conv: CallConv,
    pointer_type: Type,
) -> Result<(), JitCallError>
where
    T: Copy,
{
    // Validate inputs
    validate_call_args(func_ptr, buffer as *mut u8, buffer_size, pointer_type)?;

    // Dispatch to platform-specific implementation
    #[cfg(target_arch = "aarch64")]
    {
        match (call_conv, pointer_type) {
            (CallConv::AppleAarch64, types::I64) => unsafe {
                call_structreturn_arm64_apple(func_ptr, buffer as *mut u8, buffer_size)
            },
            (CallConv::SystemV, types::I64) => unsafe {
                call_structreturn_arm64_systemv(func_ptr, buffer as *mut u8, buffer_size)
            },
            _ => Err(JitCallError::UnsupportedCallingConvention {
                call_conv,
                pointer_type,
            }),
        }
    }

    #[cfg(target_arch = "riscv32")]
    {
        return match (call_conv, pointer_type) {
            (CallConv::SystemV, types::I32) => unsafe {
                call_structreturn_riscv32(func_ptr, buffer as *mut u8, buffer_size)
            },
            _ => Err(JitCallError::UnsupportedCallingConvention {
                call_conv,
                pointer_type,
            }),
        };
    }

    #[cfg(not(any(target_arch = "aarch64", target_arch = "riscv32")))]
    {
        let _ = (func_ptr, buffer, buffer_size);
        return Err(JitCallError::UnsupportedCallingConvention {
            call_conv,
            pointer_type,
        });
    }
}

#[cfg(target_arch = "aarch64")]
unsafe fn call_structreturn_arm64_apple(
    func_ptr: *const u8,
    buffer: *mut u8,
    _buffer_size: usize,
) -> Result<(), JitCallError> {
    use core::arch::asm;

    // On AppleAarch64, StructReturn uses x8 register
    // We need to use inline assembly to pass the parameter in x8
    // blr expects the function address in a register (we'll use x9 as temp)
    unsafe {
        asm!(
            "mov x8, {buffer}",
            "mov x9, {func}",
            "blr x9",
            buffer = in(reg) buffer as u64,
            func = in(reg) func_ptr as u64,
            out("x8") _,
            out("x9") _,
            clobber_abi("C"),
        );
    }

    Ok(())
}

#[cfg(target_arch = "aarch64")]
unsafe fn call_structreturn_arm64_systemv(
    func_ptr: *const u8,
    buffer: *mut u8,
    _buffer_size: usize,
) -> Result<(), JitCallError> {
    // SystemV on ARM64 also uses x8 for StructReturn
    unsafe { call_structreturn_arm64_apple(func_ptr, buffer, _buffer_size) }
}

#[cfg(target_arch = "riscv32")]
unsafe fn call_structreturn_riscv32(
    func_ptr: *const u8,
    buffer: *mut u8,
    _buffer_size: usize,
) -> Result<(), JitCallError> {
    // RISC-V32 SystemV uses first argument register (a0) for StructReturn
    // This matches Rust's extern "C" calling convention
    unsafe {
        let func: extern "C" fn(*mut u8) = core::mem::transmute(func_ptr);
        func(buffer);
    }
    Ok(())
}

/// Call a JIT-compiled function that uses StructReturn with arguments.
///
/// # Safety
/// - `func_ptr` must be a valid function pointer to a JIT-compiled function
/// - `buffer` must point to valid, writable memory of at least `buffer_size` bytes
/// - `args` contains the arguments as u64 values (will be cast appropriately per calling convention)
/// - The function signature must match the expected signature with StructReturn
/// - The calling convention must match the one used when compiling the function
pub unsafe fn call_structreturn_with_args<T>(
    func_ptr: *const u8,
    buffer: *mut T,
    buffer_size: usize,
    args: &[u64],
    call_conv: CallConv,
    pointer_type: Type,
) -> Result<(), JitCallError>
where
    T: Copy,
{
    // Validate inputs
    validate_call_args(func_ptr, buffer as *mut u8, buffer_size, pointer_type)?;

    // Dispatch to platform-specific implementation
    #[cfg(target_arch = "aarch64")]
    {
        match (call_conv, pointer_type) {
            (CallConv::AppleAarch64, types::I64) => unsafe {
                call_structreturn_arm64_apple_with_args(
                    func_ptr,
                    buffer as *mut u8,
                    buffer_size,
                    args,
                )
            },
            (CallConv::SystemV, types::I64) => unsafe {
                call_structreturn_arm64_systemv_with_args(
                    func_ptr,
                    buffer as *mut u8,
                    buffer_size,
                    args,
                )
            },
            _ => Err(JitCallError::UnsupportedCallingConvention {
                call_conv,
                pointer_type,
            }),
        }
    }

    #[cfg(target_arch = "riscv32")]
    {
        return match (call_conv, pointer_type) {
            (CallConv::SystemV, types::I32) => unsafe {
                call_structreturn_riscv32_with_args(func_ptr, buffer as *mut u8, buffer_size, args)
            },
            _ => Err(JitCallError::UnsupportedCallingConvention {
                call_conv,
                pointer_type,
            }),
        };
    }

    #[cfg(not(any(target_arch = "aarch64", target_arch = "riscv32")))]
    {
        let _ = (func_ptr, buffer, buffer_size, args);
        return Err(JitCallError::UnsupportedCallingConvention {
            call_conv,
            pointer_type,
        });
    }
}

#[cfg(target_arch = "aarch64")]
unsafe fn call_structreturn_arm64_apple_with_args(
    func_ptr: *const u8,
    buffer: *mut u8,
    _buffer_size: usize,
    args: &[u64],
) -> Result<(), JitCallError> {
    use core::arch::asm;

    // On AppleAarch64, StructReturn uses x8 register
    // Regular arguments go in x0-x7, then stack
    // We'll use inline assembly to set up the call

    // Limit to reasonable number of arguments (8 register + some stack)
    if args.len() > 16 {
        return Err(JitCallError::UnsupportedCallingConvention {
            call_conv: cranelift_codegen::isa::CallConv::AppleAarch64,
            pointer_type: cranelift_codegen::ir::types::I64,
        });
    }

    unsafe {
        // Prepare arguments: first 8 go in x0-x7, rest on stack
        // StructReturn pointer goes in x8
        // Function pointer goes in x9 (temp)

        // For simplicity, we'll use a macro to generate the call based on argument count
        // This is a bit verbose but ensures correct calling convention

        match args.len() {
            0 => {
                asm!(
                    "mov x8, {buffer}",
                    "mov x9, {func}",
                    "blr x9",
                    buffer = in(reg) buffer as u64,
                    func = in(reg) func_ptr as u64,
                    out("x8") _,
                    out("x9") _,
                    clobber_abi("C"),
                );
            }
            1 => {
                asm!(
                    "mov x0, {arg0}",
                    "mov x8, {buffer}",
                    "mov x9, {func}",
                    "blr x9",
                    arg0 = in(reg) args[0],
                    buffer = in(reg) buffer as u64,
                    func = in(reg) func_ptr as u64,
                    out("x0") _,
                    out("x8") _,
                    out("x9") _,
                    clobber_abi("C"),
                );
            }
            2 => {
                asm!(
                    "mov x0, {arg0}",
                    "mov x1, {arg1}",
                    "mov x8, {buffer}",
                    "mov x9, {func}",
                    "blr x9",
                    arg0 = in(reg) args[0],
                    arg1 = in(reg) args[1],
                    buffer = in(reg) buffer as u64,
                    func = in(reg) func_ptr as u64,
                    out("x0") _,
                    out("x1") _,
                    out("x8") _,
                    out("x9") _,
                    clobber_abi("C"),
                );
            }
            3 => {
                asm!(
                    "mov x0, {arg0}",
                    "mov x1, {arg1}",
                    "mov x2, {arg2}",
                    "mov x8, {buffer}",
                    "mov x9, {func}",
                    "blr x9",
                    arg0 = in(reg) args[0],
                    arg1 = in(reg) args[1],
                    arg2 = in(reg) args[2],
                    buffer = in(reg) buffer as u64,
                    func = in(reg) func_ptr as u64,
                    out("x0") _,
                    out("x1") _,
                    out("x2") _,
                    out("x8") _,
                    out("x9") _,
                    clobber_abi("C"),
                );
            }
            4 => {
                asm!(
                    "mov x0, {arg0}",
                    "mov x1, {arg1}",
                    "mov x2, {arg2}",
                    "mov x3, {arg3}",
                    "mov x8, {buffer}",
                    "mov x9, {func}",
                    "blr x9",
                    arg0 = in(reg) args[0],
                    arg1 = in(reg) args[1],
                    arg2 = in(reg) args[2],
                    arg3 = in(reg) args[3],
                    buffer = in(reg) buffer as u64,
                    func = in(reg) func_ptr as u64,
                    out("x0") _,
                    out("x1") _,
                    out("x2") _,
                    out("x3") _,
                    out("x8") _,
                    out("x9") _,
                    clobber_abi("C"),
                );
            }
            5 => {
                asm!(
                    "mov x0, {arg0}",
                    "mov x1, {arg1}",
                    "mov x2, {arg2}",
                    "mov x3, {arg3}",
                    "mov x4, {arg4}",
                    "mov x8, {buffer}",
                    "mov x9, {func}",
                    "blr x9",
                    arg0 = in(reg) args[0],
                    arg1 = in(reg) args[1],
                    arg2 = in(reg) args[2],
                    arg3 = in(reg) args[3],
                    arg4 = in(reg) args[4],
                    buffer = in(reg) buffer as u64,
                    func = in(reg) func_ptr as u64,
                    out("x0") _,
                    out("x1") _,
                    out("x2") _,
                    out("x3") _,
                    out("x4") _,
                    out("x8") _,
                    out("x9") _,
                    clobber_abi("C"),
                );
            }
            6 => {
                asm!(
                    "mov x0, {arg0}",
                    "mov x1, {arg1}",
                    "mov x2, {arg2}",
                    "mov x3, {arg3}",
                    "mov x4, {arg4}",
                    "mov x5, {arg5}",
                    "mov x8, {buffer}",
                    "mov x9, {func}",
                    "blr x9",
                    arg0 = in(reg) args[0],
                    arg1 = in(reg) args[1],
                    arg2 = in(reg) args[2],
                    arg3 = in(reg) args[3],
                    arg4 = in(reg) args[4],
                    arg5 = in(reg) args[5],
                    buffer = in(reg) buffer as u64,
                    func = in(reg) func_ptr as u64,
                    out("x0") _,
                    out("x1") _,
                    out("x2") _,
                    out("x3") _,
                    out("x4") _,
                    out("x5") _,
                    out("x8") _,
                    out("x9") _,
                    clobber_abi("C"),
                );
            }
            7 => {
                asm!(
                    "mov x0, {arg0}",
                    "mov x1, {arg1}",
                    "mov x2, {arg2}",
                    "mov x3, {arg3}",
                    "mov x4, {arg4}",
                    "mov x5, {arg5}",
                    "mov x6, {arg6}",
                    "mov x8, {buffer}",
                    "mov x9, {func}",
                    "blr x9",
                    arg0 = in(reg) args[0],
                    arg1 = in(reg) args[1],
                    arg2 = in(reg) args[2],
                    arg3 = in(reg) args[3],
                    arg4 = in(reg) args[4],
                    arg5 = in(reg) args[5],
                    arg6 = in(reg) args[6],
                    buffer = in(reg) buffer as u64,
                    func = in(reg) func_ptr as u64,
                    out("x0") _,
                    out("x1") _,
                    out("x2") _,
                    out("x3") _,
                    out("x4") _,
                    out("x5") _,
                    out("x6") _,
                    out("x8") _,
                    out("x9") _,
                    clobber_abi("C"),
                );
            }
            _ => {
                // 8 or more arguments: first 7 in registers, rest need stack
                // For now, limit to 8 arguments (7 in x0-x6, x7 for 8th)
                if args.len() > 8 {
                    return Err(JitCallError::UnsupportedCallingConvention {
                        call_conv: cranelift_codegen::isa::CallConv::AppleAarch64,
                        pointer_type: cranelift_codegen::ir::types::I64,
                    });
                }
                asm!(
                    "mov x0, {arg0}",
                    "mov x1, {arg1}",
                    "mov x2, {arg2}",
                    "mov x3, {arg3}",
                    "mov x4, {arg4}",
                    "mov x5, {arg5}",
                    "mov x6, {arg6}",
                    "mov x7, {arg7}",
                    "mov x8, {buffer}",
                    "mov x9, {func}",
                    "blr x9",
                    arg0 = in(reg) args[0],
                    arg1 = in(reg) args[1],
                    arg2 = in(reg) args[2],
                    arg3 = in(reg) args[3],
                    arg4 = in(reg) args[4],
                    arg5 = in(reg) args[5],
                    arg6 = in(reg) args[6],
                    arg7 = in(reg) args[7],
                    buffer = in(reg) buffer as u64,
                    func = in(reg) func_ptr as u64,
                    out("x0") _,
                    out("x1") _,
                    out("x2") _,
                    out("x3") _,
                    out("x4") _,
                    out("x5") _,
                    out("x6") _,
                    out("x7") _,
                    out("x8") _,
                    out("x9") _,
                    clobber_abi("C"),
                );
            }
        }
    }

    Ok(())
}

#[cfg(target_arch = "aarch64")]
unsafe fn call_structreturn_arm64_systemv_with_args(
    func_ptr: *const u8,
    buffer: *mut u8,
    buffer_size: usize,
    args: &[u64],
) -> Result<(), JitCallError> {
    // SystemV on ARM64 also uses x8 for StructReturn
    unsafe { call_structreturn_arm64_apple_with_args(func_ptr, buffer, buffer_size, args) }
}

#[cfg(target_arch = "riscv32")]
unsafe fn call_structreturn_riscv32_with_args(
    func_ptr: *const u8,
    buffer: *mut u8,
    _buffer_size: usize,
    args: &[u64],
) -> Result<(), JitCallError> {
    // RISC-V32 SystemV: StructReturn pointer is first argument (a0)
    // Regular arguments follow in a1-a7, then stack
    // We need to construct a function pointer with the right signature

    // Limit to reasonable number of arguments
    if args.len() > 8 {
        return Err(JitCallError::UnsupportedCallingConvention {
            call_conv: cranelift_codegen::isa::CallConv::SystemV,
            pointer_type: cranelift_codegen::ir::types::I32,
        });
    }

    // For RISC-V32, we'll use a match to handle different argument counts
    // StructReturn pointer goes as first argument, then regular args
    unsafe {
        match args.len() {
            0 => {
                let func: extern "C" fn(*mut u8) = core::mem::transmute(func_ptr);
                func(buffer);
            }
            1 => {
                let func: extern "C" fn(*mut u8, u32) = core::mem::transmute(func_ptr);
                func(buffer, args[0] as u32);
            }
            2 => {
                let func: extern "C" fn(*mut u8, u32, u32) = core::mem::transmute(func_ptr);
                func(buffer, args[0] as u32, args[1] as u32);
            }
            3 => {
                let func: extern "C" fn(*mut u8, u32, u32, u32) = core::mem::transmute(func_ptr);
                func(buffer, args[0] as u32, args[1] as u32, args[2] as u32);
            }
            4 => {
                let func: extern "C" fn(*mut u8, u32, u32, u32, u32) =
                    core::mem::transmute(func_ptr);
                func(
                    buffer,
                    args[0] as u32,
                    args[1] as u32,
                    args[2] as u32,
                    args[3] as u32,
                );
            }
            5 => {
                let func: extern "C" fn(*mut u8, u32, u32, u32, u32, u32) =
                    core::mem::transmute(func_ptr);
                func(
                    buffer,
                    args[0] as u32,
                    args[1] as u32,
                    args[2] as u32,
                    args[3] as u32,
                    args[4] as u32,
                );
            }
            6 => {
                let func: extern "C" fn(*mut u8, u32, u32, u32, u32, u32, u32) =
                    core::mem::transmute(func_ptr);
                func(
                    buffer,
                    args[0] as u32,
                    args[1] as u32,
                    args[2] as u32,
                    args[3] as u32,
                    args[4] as u32,
                    args[5] as u32,
                );
            }
            7 => {
                let func: extern "C" fn(*mut u8, u32, u32, u32, u32, u32, u32, u32) =
                    core::mem::transmute(func_ptr);
                func(
                    buffer,
                    args[0] as u32,
                    args[1] as u32,
                    args[2] as u32,
                    args[3] as u32,
                    args[4] as u32,
                    args[5] as u32,
                    args[6] as u32,
                );
            }
            _ => {
                // 8 arguments: first 7 in a1-a7, 8th on stack (but we'll pass all 8)
                let func: extern "C" fn(*mut u8, u32, u32, u32, u32, u32, u32, u32, u32) =
                    core::mem::transmute(func_ptr);
                func(
                    buffer,
                    args[0] as u32,
                    args[1] as u32,
                    args[2] as u32,
                    args[3] as u32,
                    args[4] as u32,
                    args[5] as u32,
                    args[6] as u32,
                    args[7] as u32,
                );
            }
        }
    }

    Ok(())
}

fn validate_call_args(
    func_ptr: *const u8,
    buffer: *mut u8,
    buffer_size: usize,
    pointer_type: Type,
) -> Result<(), JitCallError> {
    if func_ptr.is_null() {
        return Err(JitCallError::NullFunctionPointer);
    }

    if buffer.is_null() {
        return Err(JitCallError::NullBuffer);
    }

    if buffer_size == 0 {
        return Err(JitCallError::ZeroBufferSize);
    }

    // Validate pointer type matches platform
    // Use size_of to determine pointer width at runtime
    let pointer_size = core::mem::size_of::<*const ()>();
    let actual_width = if pointer_size == 4 {
        "32"
    } else if pointer_size == 8 {
        "64"
    } else {
        "unknown"
    };

    match pointer_type {
        types::I32 if pointer_size == 4 => Ok(()),
        types::I64 if pointer_size == 8 => Ok(()),
        _ => Err(JitCallError::PointerTypeMismatch {
            expected: pointer_type,
            actual_pointer_width: String::from(actual_width),
        }),
    }
}
