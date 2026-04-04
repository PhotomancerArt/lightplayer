//! `extern "C"` dispatch for small arities (native JIT pointers).

use alloc::vec::Vec;
use core::mem::transmute;

use crate::values::CallError;

/// VMContext as a full machine word for `extern "C"` JIT entrypoints.
type VmCtxWord = usize;

/// C-layout multi-scalar returns for hosts where `extern "C"` struct returns match Cranelift’s
/// multi-`I32` layout (not Apple AArch64: read `x0`…`x3` after `blr` instead).
#[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
#[repr(C)]
struct CRet2 {
    v0: i32,
    v1: i32,
}
#[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
#[repr(C)]
struct CRet3 {
    v0: i32,
    v1: i32,
    v2: i32,
}
#[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
#[repr(C)]
struct CRet4 {
    v0: i32,
    v1: i32,
    v2: i32,
    v3: i32,
}

/// System V / RISC-V32: hidden StructReturn pointer as the first `extern "C"` argument (`a0` /
/// `rdi`) plus user `i32` args; callee writes `out.len()` words.
#[cfg(any(
    target_arch = "riscv32",
    all(
        target_arch = "x86_64",
        not(all(target_os = "windows", target_env = "msvc"))
    )
))]
unsafe fn invoke_sysv_struct_return_buf(
    code: *const u8,
    vmctx: VmCtxWord,
    user: &[i32],
    out: &mut [i32],
) -> Result<(), CallError> {
    let buf = out.as_mut_ptr();
    unsafe {
        match user.len() {
            0 => {
                let f: extern "C" fn(VmCtxWord, *mut i32) = transmute(code);
                f(vmctx, buf);
            }
            1 => {
                let f: extern "C" fn(VmCtxWord, *mut i32, i32) = transmute(code);
                f(vmctx, buf, user[0]);
            }
            2 => {
                let f: extern "C" fn(VmCtxWord, *mut i32, i32, i32) = transmute(code);
                f(vmctx, buf, user[0], user[1]);
            }
            3 => {
                let f: extern "C" fn(VmCtxWord, *mut i32, i32, i32, i32) = transmute(code);
                f(vmctx, buf, user[0], user[1], user[2]);
            }
            4 => {
                let f: extern "C" fn(VmCtxWord, *mut i32, i32, i32, i32, i32) = transmute(code);
                f(vmctx, buf, user[0], user[1], user[2], user[3]);
            }
            5 => {
                let f: extern "C" fn(VmCtxWord, *mut i32, i32, i32, i32, i32, i32) = transmute(code);
                f(vmctx, buf, user[0], user[1], user[2], user[3], user[4]);
            }
            6 => {
                let f: extern "C" fn(VmCtxWord, *mut i32, i32, i32, i32, i32, i32, i32) = transmute(code);
                f(vmctx, buf, user[0], user[1], user[2], user[3], user[4], user[5]);
            }
            7 => {
                let f: extern "C" fn(VmCtxWord, *mut i32, i32, i32, i32, i32, i32, i32, i32) = transmute(code);
                f(vmctx, buf, user[0], user[1], user[2], user[3], user[4], user[5], user[6]);
            }
            8 => {
                let f: extern "C" fn(VmCtxWord, *mut i32, i32, i32, i32, i32, i32, i32, i32, i32) = transmute(code);
                f(vmctx, buf, user[0], user[1], user[2], user[3], user[4], user[5], user[6], user[7]);
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}


/// AArch64: VMContext and user args in `x0`…`x7`; StructReturn pointer in `x8`.
#[cfg(all(target_arch = "aarch64", not(target_os = "windows")))]
unsafe fn aarch64_invoke_struct_return_buf(
    code: *const u8,
    vmctx: VmCtxWord,
    user: &[i32],
    out: &mut [i32],
) -> Result<(), CallError> {
    use core::arch::asm;
    let buf = out.as_mut_ptr() as u64;
    match user.len() {
        0 => {
            let a0 = vmctx as u64;
            unsafe {
                asm!(
                    "blr {}",
                    in(reg) code,
                    in("x8") buf,
                    in("x0") a0,
                    clobber_abi("C"),
                );
            }
        }
        1 => {
            let a0 = vmctx as u64;
            let a1 = user[0] as i64 as u64;
            unsafe {
                asm!(
                    "blr {}",
                    in(reg) code,
                    in("x8") buf,
                    in("x0") a0,
                    in("x1") a1,
                    clobber_abi("C"),
                );
            }
        }
        2 => {
            let a0 = vmctx as u64;
            let a1 = user[0] as i64 as u64;
            let a2 = user[1] as i64 as u64;
            unsafe {
                asm!(
                    "blr {}",
                    in(reg) code,
                    in("x8") buf,
                    in("x0") a0,
                    in("x1") a1,
                    in("x2") a2,
                    clobber_abi("C"),
                );
            }
        }
        3 => {
            let a0 = vmctx as u64;
            let a1 = user[0] as i64 as u64;
            let a2 = user[1] as i64 as u64;
            let a3 = user[2] as i64 as u64;
            unsafe {
                asm!(
                    "blr {}",
                    in(reg) code,
                    in("x8") buf,
                    in("x0") a0,
                    in("x1") a1,
                    in("x2") a2,
                    in("x3") a3,
                    clobber_abi("C"),
                );
            }
        }
        4 => {
            let a0 = vmctx as u64;
            let a1 = user[0] as i64 as u64;
            let a2 = user[1] as i64 as u64;
            let a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            unsafe {
                asm!(
                    "blr {}",
                    in(reg) code,
                    in("x8") buf,
                    in("x0") a0,
                    in("x1") a1,
                    in("x2") a2,
                    in("x3") a3,
                    in("x4") a4,
                    clobber_abi("C"),
                );
            }
        }
        5 => {
            let a0 = vmctx as u64;
            let a1 = user[0] as i64 as u64;
            let a2 = user[1] as i64 as u64;
            let a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            unsafe {
                asm!(
                    "blr {}",
                    in(reg) code,
                    in("x8") buf,
                    in("x0") a0,
                    in("x1") a1,
                    in("x2") a2,
                    in("x3") a3,
                    in("x4") a4,
                    in("x5") a5,
                    clobber_abi("C"),
                );
            }
        }
        6 => {
            let a0 = vmctx as u64;
            let a1 = user[0] as i64 as u64;
            let a2 = user[1] as i64 as u64;
            let a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            unsafe {
                asm!(
                    "blr {}",
                    in(reg) code,
                    in("x8") buf,
                    in("x0") a0,
                    in("x1") a1,
                    in("x2") a2,
                    in("x3") a3,
                    in("x4") a4,
                    in("x5") a5,
                    in("x6") a6,
                    clobber_abi("C"),
                );
            }
        }
        7 => {
            let a0 = vmctx as u64;
            let a1 = user[0] as i64 as u64;
            let a2 = user[1] as i64 as u64;
            let a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            unsafe {
                asm!(
                    "blr {}",
                    in(reg) code,
                    in("x8") buf,
                    in("x0") a0,
                    in("x1") a1,
                    in("x2") a2,
                    in("x3") a3,
                    in("x4") a4,
                    in("x5") a5,
                    in("x6") a6,
                    in("x7") a7,
                    clobber_abi("C"),
                );
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}


unsafe fn invoke_struct_return_dispatch(
    code: *const u8,
    vmctx: VmCtxWord,
    user: &[i32],
    out: &mut [i32],
) -> Result<(), CallError> {
    #[cfg(target_arch = "riscv32")]
    {
        return unsafe { invoke_sysv_struct_return_buf(code, vmctx, user, out) };
    }
    #[cfg(all(
        target_arch = "x86_64",
        not(all(target_os = "windows", target_env = "msvc"))
    ))]
    {
        return unsafe { invoke_sysv_struct_return_buf(code, vmctx, user, out) };
    }
    #[cfg(all(target_arch = "aarch64", not(target_os = "windows")))]
    {
        return unsafe { aarch64_invoke_struct_return_buf(code, vmctx, user, out) };
    }
    #[cfg(not(any(
        target_arch = "riscv32",
        all(
            target_arch = "x86_64",
            not(all(target_os = "windows", target_env = "msvc"))
        ),
        all(target_arch = "aarch64", not(target_os = "windows")),
    )))]
    {
        let _ = (code, vmctx, user, out);
        Err(CallError::Unsupported(
            "StructReturn JIT invoke is not implemented for this host target".into(),
        ))
    }
}

/// Call a finalized JIT function passing `i32` scalars and collecting `i32` return words.
///
/// # Safety
/// `code` must be a pointer from [`cranelift_jit::JITModule::get_finalized_function`] with a
/// matching ABI (SystemV-style `extern "C"` on the host; RISC-V may use StructReturn).
pub(crate) unsafe fn invoke_i32_args_returns(
    code: *const u8,
    vmctx: *const u8,
    user: &[i32],
    n_ret: usize,
    uses_struct_return: bool,
) -> Result<Vec<i32>, CallError> {
    let mut out = alloc::vec![0i32; n_ret];
    unsafe {
        invoke_i32_args_returns_buf(code, vmctx, user, n_ret, &mut out, uses_struct_return)?
    };
    Ok(out)
}

/// Like [`invoke_i32_args_returns`] but writes each return scalar into `out` (no heap allocation).
///
/// # Safety
/// Same as [`invoke_i32_args_returns`]. Caller must ensure `out.len() == n_ret`.
pub(crate) unsafe fn invoke_i32_args_returns_buf(
    code: *const u8,
    vmctx: *const u8,
    user: &[i32],
    n_ret: usize,
    out: &mut [i32],
    uses_struct_return: bool,
) -> Result<(), CallError> {
    if out.len() != n_ret {
        return Err(CallError::TypeMismatch(alloc::format!(
            "return buffer length {} does not match {} return word(s)",
            out.len(),
            n_ret
        )));
    }

    if user.len() > 8 {
        return Err(CallError::Unsupported(
            "more than 8 user scalar arguments (vmctx passed separately) not supported by invoke shim".into(),
        ));
    }

    let vm = vmctx as VmCtxWord;

    if uses_struct_return {
        return unsafe { invoke_struct_return_dispatch(code, vm, user, out) };
    }

    if n_ret > 4 {
        return Err(CallError::Unsupported(
            "more than 4 scalar returns not supported by invoke shim".into(),
        ));
    }

    match n_ret {
        0 => {
            match user.len() {
                0 => {
                    let f: extern "C" fn(VmCtxWord) = unsafe { transmute(code) };
                    f(vm);
                }
                1 => {
                    let f: extern "C" fn(VmCtxWord, i32) = unsafe { transmute(code) };
                    f(vm, user[0]);
                }
                2 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32) = unsafe { transmute(code) };
                    f(vm, user[0], user[1]);
                }
                3 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32) = unsafe { transmute(code) };
                    f(vm, user[0], user[1], user[2]);
                }
                4 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32) = unsafe { transmute(code) };
                    f(vm, user[0], user[1], user[2], user[3]);
                }
                5 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32) = unsafe { transmute(code) };
                    f(vm, user[0], user[1], user[2], user[3], user[4]);
                }
                6 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32) = unsafe { transmute(code) };
                    f(vm, user[0], user[1], user[2], user[3], user[4], user[5]);
                }
                7 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32, i32) = unsafe { transmute(code) };
                    f(vm, user[0], user[1], user[2], user[3], user[4], user[5], user[6]);
                }
                8 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32, i32, i32) = unsafe { transmute(code) };
                    f(vm, user[0], user[1], user[2], user[3], user[4], user[5], user[6], user[7]);
                }
                _ => unreachable!(),
            }
            Ok(())
        }
        1 => {
            match user.len() {
                0 => {
                    let f: extern "C" fn(VmCtxWord) -> i32 = unsafe { transmute(code) };
                    out[0] = f(vm);
                }
                1 => {
                    let f: extern "C" fn(VmCtxWord, i32) -> i32 = unsafe { transmute(code) };
                    out[0] = f(vm, user[0]);
                }
                2 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32) -> i32 = unsafe { transmute(code) };
                    out[0] = f(vm, user[0], user[1]);
                }
                3 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32) -> i32 = unsafe { transmute(code) };
                    out[0] = f(vm, user[0], user[1], user[2]);
                }
                4 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32) -> i32 = unsafe { transmute(code) };
                    out[0] = f(vm, user[0], user[1], user[2], user[3]);
                }
                5 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32) -> i32 = unsafe { transmute(code) };
                    out[0] = f(vm, user[0], user[1], user[2], user[3], user[4]);
                }
                6 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32) -> i32 = unsafe { transmute(code) };
                    out[0] = f(vm, user[0], user[1], user[2], user[3], user[4], user[5]);
                }
                7 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32, i32) -> i32 = unsafe { transmute(code) };
                    out[0] = f(vm, user[0], user[1], user[2], user[3], user[4], user[5], user[6]);
                }
                8 => {
                    let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 = unsafe { transmute(code) };
                    out[0] = f(vm, user[0], user[1], user[2], user[3], user[4], user[5], user[6], user[7]);
                }
                _ => unreachable!(),
            }
            Ok(())
        }
        2..=4 => {
            #[cfg(all(target_arch = "aarch64", not(target_os = "windows")))]
            {
                unsafe { aarch64_invoke_multi_ret_buf(code, vm, user, n_ret, out) };
                Ok(())
            }
            #[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
            {
                match n_ret {
                    2 => {
                        let r = invoke_cret2(code, vm, user);
                        out[0] = r.v0;
                        out[1] = r.v1;
                    }
                    3 => {
                        let r = invoke_cret3(code, vm, user);
                        out[0] = r.v0;
                        out[1] = r.v1;
                        out[2] = r.v2;
                    }
                    4 => {
                        let r = invoke_cret4(code, vm, user);
                        out[0] = r.v0;
                        out[1] = r.v1;
                        out[2] = r.v2;
                        out[3] = r.v3;
                    }
                    _ => unreachable!(),
                }
                Ok(())
            }
        }
        _ => unreachable!(),
    }
}

#[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
fn invoke_cret2(code: *const u8, vm: VmCtxWord, user: &[i32]) -> CRet2 {
    unsafe {
        match user.len() {
            0 => {
                let f: extern "C" fn(VmCtxWord) -> CRet2 = transmute(code);
                f(vm)
            }
            1 => {
                let f: extern "C" fn(VmCtxWord, i32) -> CRet2 = transmute(code);
                f(vm, user[0])
            }
            2 => {
                let f: extern "C" fn(VmCtxWord, i32, i32) -> CRet2 = transmute(code);
                f(vm, user[0], user[1])
            }
            3 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32) -> CRet2 = transmute(code);
                f(vm, user[0], user[1], user[2])
            }
            4 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32) -> CRet2 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3])
            }
            5 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32) -> CRet2 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4])
            }
            6 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32) -> CRet2 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4], user[5])
            }
            7 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32, i32) -> CRet2 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4], user[5], user[6])
            }
            8 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32, i32, i32) -> CRet2 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4], user[5], user[6], user[7])
            }
            _ => unreachable!(),
        }
    }
}


#[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
fn invoke_cret3(code: *const u8, vm: VmCtxWord, user: &[i32]) -> CRet3 {
    unsafe {
        match user.len() {
            0 => {
                let f: extern "C" fn(VmCtxWord) -> CRet3 = transmute(code);
                f(vm)
            }
            1 => {
                let f: extern "C" fn(VmCtxWord, i32) -> CRet3 = transmute(code);
                f(vm, user[0])
            }
            2 => {
                let f: extern "C" fn(VmCtxWord, i32, i32) -> CRet3 = transmute(code);
                f(vm, user[0], user[1])
            }
            3 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32) -> CRet3 = transmute(code);
                f(vm, user[0], user[1], user[2])
            }
            4 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32) -> CRet3 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3])
            }
            5 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32) -> CRet3 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4])
            }
            6 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32) -> CRet3 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4], user[5])
            }
            7 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32, i32) -> CRet3 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4], user[5], user[6])
            }
            8 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32, i32, i32) -> CRet3 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4], user[5], user[6], user[7])
            }
            _ => unreachable!(),
        }
    }
}


#[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
fn invoke_cret4(code: *const u8, vm: VmCtxWord, user: &[i32]) -> CRet4 {
    unsafe {
        match user.len() {
            0 => {
                let f: extern "C" fn(VmCtxWord) -> CRet4 = transmute(code);
                f(vm)
            }
            1 => {
                let f: extern "C" fn(VmCtxWord, i32) -> CRet4 = transmute(code);
                f(vm, user[0])
            }
            2 => {
                let f: extern "C" fn(VmCtxWord, i32, i32) -> CRet4 = transmute(code);
                f(vm, user[0], user[1])
            }
            3 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32) -> CRet4 = transmute(code);
                f(vm, user[0], user[1], user[2])
            }
            4 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32) -> CRet4 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3])
            }
            5 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32) -> CRet4 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4])
            }
            6 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32) -> CRet4 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4], user[5])
            }
            7 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32, i32) -> CRet4 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4], user[5], user[6])
            }
            8 => {
                let f: extern "C" fn(VmCtxWord, i32, i32, i32, i32, i32, i32, i32, i32) -> CRet4 = transmute(code);
                f(vm, user[0], user[1], user[2], user[3], user[4], user[5], user[6], user[7])
            }
            _ => unreachable!(),
        }
    }
}


/// Cranelift places each `I32` return in its own GPR (`x0`…). Rust `extern "C"` `repr(C)` structs
/// pack small aggregates differently on Apple AArch64, so read registers explicitly after `blr`.
#[cfg(all(target_arch = "aarch64", not(target_os = "windows")))]
// Inline asm: Rust 2024 `unsafe_op_in_unsafe_fn`; not all x2/x3 outputs used when n_ret < 4.
#[allow(
    dead_code,
    unsafe_op_in_unsafe_fn,
    unused_assignments,
    unused_mut,
    reason = "vec-return variant kept alongside aarch64_invoke_multi_ret_buf; invoke delegates to buf"
)]
unsafe fn aarch64_invoke_multi_ret(code: *const u8, vm: VmCtxWord, user: &[i32], n_ret: usize) -> Vec<i32> {
    use core::arch::asm;

    #[inline(always)]
    fn word64(w: u64) -> i32 {
        w as u32 as i32
    }

    let mut r0: u64;
    let mut r1: u64;
    let mut r2: u64;
    let mut r3: u64;

    match (user.len(), n_ret) {
        (0, 2) => {
            let mut a0 = vm as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                lateout("x1") r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1)]
        }
        (0, 3) => {
            let mut a0 = vm as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                lateout("x1") r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2)]
        }
        (0, 4) => {
            let mut a0 = vm as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                lateout("x1") r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2), word64(r3)]
        }
        (1, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1)]
        }
        (1, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2)]
        }
        (1, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2), word64(r3)]
        }
        (2, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1)]
        }
        (2, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2)]
        }
        (2, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2), word64(r3)]
        }
        (3, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1)]
        }
        (3, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2)]
        }
        (3, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2), word64(r3)]
        }
        (4, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1)]
        }
        (4, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2)]
        }
        (4, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2), word64(r3)]
        }
        (5, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1)]
        }
        (5, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2)]
        }
        (5, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2), word64(r3)]
        }
        (6, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1)]
        }
        (6, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2)]
        }
        (6, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2), word64(r3)]
        }
        (7, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1)]
        }
        (7, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2)]
        }
        (7, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2), word64(r3)]
        }
        (8, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            let a8 = user[7] as i64 as u64;
            asm!(
                "sub sp, sp, #16",
                "str {a8}, [sp]",
                "blr {code}",
                "add sp, sp, #16",
                code = in(reg) code,
                a8 = in(reg) a8,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1)]
        }
        (8, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            let a8 = user[7] as i64 as u64;
            asm!(
                "sub sp, sp, #16",
                "str {a8}, [sp]",
                "blr {code}",
                "add sp, sp, #16",
                code = in(reg) code,
                a8 = in(reg) a8,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2)]
        }
        (8, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            let a8 = user[7] as i64 as u64;
            asm!(
                "sub sp, sp, #16",
                "str {a8}, [sp]",
                "blr {code}",
                "add sp, sp, #16",
                code = in(reg) code,
                a8 = in(reg) a8,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2), word64(r3)]
        }
        _ => unreachable!(),
    }
}

/// Like [`aarch64_invoke_multi_ret`] but writes return words into `out` (no allocation).
#[cfg(all(target_arch = "aarch64", not(target_os = "windows")))]
#[allow(
    unsafe_op_in_unsafe_fn,
    unused_assignments,
    unused_mut,
    reason = "AArch64 `asm!(blr)` multi-return shim; register lateouts match callee ABI"
)]
unsafe fn aarch64_invoke_multi_ret_buf(
    code: *const u8,
    vm: VmCtxWord,
    user: &[i32],
    n_ret: usize,
    out: &mut [i32],
) {
    use core::arch::asm;

    #[inline(always)]
    fn word64(w: u64) -> i32 {
        w as u32 as i32
    }

    let mut r0: u64;
    let mut r1: u64;
    let mut r2: u64;
    let mut r3: u64;

    match (user.len(), n_ret) {
        (0, 2) => {
            let mut a0 = vm as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                lateout("x1") r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
        }
        (0, 3) => {
            let mut a0 = vm as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                lateout("x1") r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
        }
        (0, 4) => {
            let mut a0 = vm as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                lateout("x1") r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
            out[3] = word64(r3);
        }
        (1, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
        }
        (1, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
        }
        (1, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
            out[3] = word64(r3);
        }
        (2, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
        }
        (2, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
        }
        (2, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
            out[3] = word64(r3);
        }
        (3, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
        }
        (3, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
        }
        (3, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
            out[3] = word64(r3);
        }
        (4, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
        }
        (4, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
        }
        (4, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
            out[3] = word64(r3);
        }
        (5, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
        }
        (5, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
        }
        (5, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
            out[3] = word64(r3);
        }
        (6, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
        }
        (6, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
        }
        (6, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
            out[3] = word64(r3);
        }
        (7, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
        }
        (7, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
        }
        (7, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            asm!(
                "blr {}",
                in(reg) code,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
            out[3] = word64(r3);
        }
        (8, 2) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            let a8 = user[7] as i64 as u64;
            asm!(
                "sub sp, sp, #16",
                "str {a8}, [sp]",
                "blr {code}",
                "add sp, sp, #16",
                code = in(reg) code,
                a8 = in(reg) a8,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
        }
        (8, 3) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            let a8 = user[7] as i64 as u64;
            asm!(
                "sub sp, sp, #16",
                "str {a8}, [sp]",
                "blr {code}",
                "add sp, sp, #16",
                code = in(reg) code,
                a8 = in(reg) a8,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
        }
        (8, 4) => {
            let mut a0 = vm as u64;
            let mut a1 = user[0] as i64 as u64;
            let mut a2 = user[1] as i64 as u64;
            let mut a3 = user[2] as i64 as u64;
            let a4 = user[3] as i64 as u64;
            let a5 = user[4] as i64 as u64;
            let a6 = user[5] as i64 as u64;
            let a7 = user[6] as i64 as u64;
            let a8 = user[7] as i64 as u64;
            asm!(
                "sub sp, sp, #16",
                "str {a8}, [sp]",
                "blr {code}",
                "add sp, sp, #16",
                code = in(reg) code,
                a8 = in(reg) a8,
                inlateout("x0") a0 => r0,
                inlateout("x1") a1 => r1,
                inlateout("x2") a2 => r2,
                inlateout("x3") a3 => r3,
                in("x4") a4,
                in("x5") a5,
                in("x6") a6,
                in("x7") a7,
                clobber_abi("C"),
            );
            out[0] = word64(r0);
            out[1] = word64(r1);
            out[2] = word64(r2);
            out[3] = word64(r3);
        }
        _ => unreachable!(),
    }
}
