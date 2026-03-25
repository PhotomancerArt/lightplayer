//! `extern "C"` dispatch for small arities (native JIT pointers).

use alloc::vec::Vec;
use core::mem::transmute;

use crate::values::CallError;

/// C-layout multi-scalar returns for hosts where `extern "C"` struct returns match Cranelift’s
/// multi-`I32` layout (not Apple AArch64: read `x0`…`x3` after `blr` instead).
#[repr(C)]
struct CRet2 {
    v0: i32,
    v1: i32,
}
#[repr(C)]
struct CRet3 {
    v0: i32,
    v1: i32,
    v2: i32,
}
#[repr(C)]
struct CRet4 {
    v0: i32,
    v1: i32,
    v2: i32,
    v3: i32,
}

/// Call a finalized JIT function passing `i32` scalars and collecting `i32` return words.
///
/// # Safety
/// `code` must be a pointer from [`cranelift_jit::JITModule::get_finalized_function`] with a
/// matching ABI (SystemV-style `extern "C"` on the host).
pub(crate) unsafe fn invoke_i32_args_returns(
    code: *const u8,
    args: &[i32],
    n_ret: usize,
) -> Result<Vec<i32>, CallError> {
    if args.len() > 8 {
        return Err(CallError::Unsupported(
            "more than 8 scalar arguments not supported by invoke shim".into(),
        ));
    }
    if n_ret > 4 {
        return Err(CallError::Unsupported(
            "more than 4 scalar returns not supported by invoke shim".into(),
        ));
    }

    match n_ret {
        0 => Ok(match args.len() {
            0 => {
                let f: extern "C" fn() = unsafe { transmute(code) };
                f();
                Vec::new()
            }
            1 => {
                let f: extern "C" fn(i32) = unsafe { transmute(code) };
                f(args[0]);
                Vec::new()
            }
            2 => {
                let f: extern "C" fn(i32, i32) = unsafe { transmute(code) };
                f(args[0], args[1]);
                Vec::new()
            }
            3 => {
                let f: extern "C" fn(i32, i32, i32) = unsafe { transmute(code) };
                f(args[0], args[1], args[2]);
                Vec::new()
            }
            4 => {
                let f: extern "C" fn(i32, i32, i32, i32) = unsafe { transmute(code) };
                f(args[0], args[1], args[2], args[3]);
                Vec::new()
            }
            5 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32) = unsafe { transmute(code) };
                f(args[0], args[1], args[2], args[3], args[4]);
                Vec::new()
            }
            6 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32) = unsafe { transmute(code) };
                f(args[0], args[1], args[2], args[3], args[4], args[5]);
                Vec::new()
            }
            7 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32) =
                    unsafe { transmute(code) };
                f(
                    args[0], args[1], args[2], args[3], args[4], args[5], args[6],
                );
                Vec::new()
            }
            8 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32, i32) =
                    unsafe { transmute(code) };
                f(
                    args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
                );
                Vec::new()
            }
            _ => unreachable!(),
        }),
        1 => Ok(match args.len() {
            0 => {
                let f: extern "C" fn() -> i32 = unsafe { transmute(code) };
                alloc::vec![f()]
            }
            1 => {
                let f: extern "C" fn(i32) -> i32 = unsafe { transmute(code) };
                alloc::vec![f(args[0])]
            }
            2 => {
                let f: extern "C" fn(i32, i32) -> i32 = unsafe { transmute(code) };
                alloc::vec![f(args[0], args[1])]
            }
            3 => {
                let f: extern "C" fn(i32, i32, i32) -> i32 = unsafe { transmute(code) };
                alloc::vec![f(args[0], args[1], args[2])]
            }
            4 => {
                let f: extern "C" fn(i32, i32, i32, i32) -> i32 = unsafe { transmute(code) };
                alloc::vec![f(args[0], args[1], args[2], args[3])]
            }
            5 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32) -> i32 = unsafe { transmute(code) };
                alloc::vec![f(args[0], args[1], args[2], args[3], args[4])]
            }
            6 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32) -> i32 =
                    unsafe { transmute(code) };
                alloc::vec![f(args[0], args[1], args[2], args[3], args[4], args[5],)]
            }
            7 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32) -> i32 =
                    unsafe { transmute(code) };
                alloc::vec![f(
                    args[0], args[1], args[2], args[3], args[4], args[5], args[6],
                )]
            }
            8 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32, i32) -> i32 =
                    unsafe { transmute(code) };
                alloc::vec![f(
                    args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
                )]
            }
            _ => unreachable!(),
        }),
        2..=4 => {
            #[cfg(all(target_arch = "aarch64", not(target_os = "windows")))]
            {
                Ok(unsafe { aarch64_invoke_multi_ret(code, args, n_ret) })
            }
            #[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
            {
                Ok(match n_ret {
                    2 => {
                        let r = invoke_cret2(code, args);
                        alloc::vec![r.v0, r.v1]
                    }
                    3 => {
                        let r = invoke_cret3(code, args);
                        alloc::vec![r.v0, r.v1, r.v2]
                    }
                    4 => {
                        let r = invoke_cret4(code, args);
                        alloc::vec![r.v0, r.v1, r.v2, r.v3]
                    }
                    _ => unreachable!(),
                })
            }
        }
        _ => unreachable!(),
    }
}

#[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
fn invoke_cret2(code: *const u8, args: &[i32]) -> CRet2 {
    unsafe {
        match args.len() {
            0 => {
                let f: extern "C" fn() -> CRet2 = transmute(code);
                f()
            }
            1 => {
                let f: extern "C" fn(i32) -> CRet2 = transmute(code);
                f(args[0])
            }
            2 => {
                let f: extern "C" fn(i32, i32) -> CRet2 = transmute(code);
                f(args[0], args[1])
            }
            3 => {
                let f: extern "C" fn(i32, i32, i32) -> CRet2 = transmute(code);
                f(args[0], args[1], args[2])
            }
            4 => {
                let f: extern "C" fn(i32, i32, i32, i32) -> CRet2 = transmute(code);
                f(args[0], args[1], args[2], args[3])
            }
            5 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32) -> CRet2 = transmute(code);
                f(args[0], args[1], args[2], args[3], args[4])
            }
            6 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32) -> CRet2 = transmute(code);
                f(args[0], args[1], args[2], args[3], args[4], args[5])
            }
            7 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32) -> CRet2 = transmute(code);
                f(
                    args[0], args[1], args[2], args[3], args[4], args[5], args[6],
                )
            }
            8 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32, i32) -> CRet2 =
                    transmute(code);
                f(
                    args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
                )
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
fn invoke_cret3(code: *const u8, args: &[i32]) -> CRet3 {
    unsafe {
        match args.len() {
            0 => {
                let f: extern "C" fn() -> CRet3 = transmute(code);
                f()
            }
            1 => {
                let f: extern "C" fn(i32) -> CRet3 = transmute(code);
                f(args[0])
            }
            2 => {
                let f: extern "C" fn(i32, i32) -> CRet3 = transmute(code);
                f(args[0], args[1])
            }
            3 => {
                let f: extern "C" fn(i32, i32, i32) -> CRet3 = transmute(code);
                f(args[0], args[1], args[2])
            }
            4 => {
                let f: extern "C" fn(i32, i32, i32, i32) -> CRet3 = transmute(code);
                f(args[0], args[1], args[2], args[3])
            }
            5 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32) -> CRet3 = transmute(code);
                f(args[0], args[1], args[2], args[3], args[4])
            }
            6 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32) -> CRet3 = transmute(code);
                f(args[0], args[1], args[2], args[3], args[4], args[5])
            }
            7 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32) -> CRet3 = transmute(code);
                f(
                    args[0], args[1], args[2], args[3], args[4], args[5], args[6],
                )
            }
            8 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32, i32) -> CRet3 =
                    transmute(code);
                f(
                    args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
                )
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(not(all(target_arch = "aarch64", not(target_os = "windows"))))]
fn invoke_cret4(code: *const u8, args: &[i32]) -> CRet4 {
    unsafe {
        match args.len() {
            0 => {
                let f: extern "C" fn() -> CRet4 = transmute(code);
                f()
            }
            1 => {
                let f: extern "C" fn(i32) -> CRet4 = transmute(code);
                f(args[0])
            }
            2 => {
                let f: extern "C" fn(i32, i32) -> CRet4 = transmute(code);
                f(args[0], args[1])
            }
            3 => {
                let f: extern "C" fn(i32, i32, i32) -> CRet4 = transmute(code);
                f(args[0], args[1], args[2])
            }
            4 => {
                let f: extern "C" fn(i32, i32, i32, i32) -> CRet4 = transmute(code);
                f(args[0], args[1], args[2], args[3])
            }
            5 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32) -> CRet4 = transmute(code);
                f(args[0], args[1], args[2], args[3], args[4])
            }
            6 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32) -> CRet4 = transmute(code);
                f(args[0], args[1], args[2], args[3], args[4], args[5])
            }
            7 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32) -> CRet4 = transmute(code);
                f(
                    args[0], args[1], args[2], args[3], args[4], args[5], args[6],
                )
            }
            8 => {
                let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32, i32) -> CRet4 =
                    transmute(code);
                f(
                    args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
                )
            }
            _ => unreachable!(),
        }
    }
}

/// Cranelift places each `I32` return in its own GPR (`x0`…). Rust `extern "C"` `repr(C)` structs
/// pack small aggregates differently on Apple AArch64, so read registers explicitly after `blr`.
#[cfg(all(target_arch = "aarch64", not(target_os = "windows")))]
unsafe fn aarch64_invoke_multi_ret(code: *const u8, args: &[i32], n_ret: usize) -> Vec<i32> {
    use core::arch::asm;

    #[inline(always)]
    fn word64(w: u64) -> i32 {
        w as u32 as i32
    }

    let mut r0: u64;
    let mut r1: u64;
    let mut r2: u64;
    let mut r3: u64;

    match (args.len(), n_ret) {
        (0, 2) => {
            asm!(
                "blr {}",
                in(reg) code,
                lateout("x0") r0,
                lateout("x1") r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1)]
        }
        (0, 3) => {
            asm!(
                "blr {}",
                in(reg) code,
                lateout("x0") r0,
                lateout("x1") r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2)]
        }
        (0, 4) => {
            asm!(
                "blr {}",
                in(reg) code,
                lateout("x0") r0,
                lateout("x1") r1,
                lateout("x2") r2,
                lateout("x3") r3,
                clobber_abi("C"),
            );
            alloc::vec![word64(r0), word64(r1), word64(r2), word64(r3)]
        }
        (1, 2) => {
            let mut a0 = args[0] as i64 as u64;
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
        (1, 3) => {
            let mut a0 = args[0] as i64 as u64;
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
        (1, 4) => {
            let mut a0 = args[0] as i64 as u64;
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
        (2, 2) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
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
        (2, 3) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
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
        (2, 4) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
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
        (3, 2) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
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
        (3, 3) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
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
        (3, 4) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
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
        (4, 2) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
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
        (4, 3) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
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
        (4, 4) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
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
        (5, 2) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
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
        (5, 3) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
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
        (5, 4) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
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
        (6, 2) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
            let a5 = args[5] as i64 as u64;
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
        (6, 3) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
            let a5 = args[5] as i64 as u64;
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
        (6, 4) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
            let a5 = args[5] as i64 as u64;
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
        (7, 2) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
            let a5 = args[5] as i64 as u64;
            let a6 = args[6] as i64 as u64;
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
        (7, 3) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
            let a5 = args[5] as i64 as u64;
            let a6 = args[6] as i64 as u64;
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
        (7, 4) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
            let a5 = args[5] as i64 as u64;
            let a6 = args[6] as i64 as u64;
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
        (8, 2) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
            let a5 = args[5] as i64 as u64;
            let a6 = args[6] as i64 as u64;
            let a7 = args[7] as i64 as u64;
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
        (8, 3) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
            let a5 = args[5] as i64 as u64;
            let a6 = args[6] as i64 as u64;
            let a7 = args[7] as i64 as u64;
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
        (8, 4) => {
            let mut a0 = args[0] as i64 as u64;
            let mut a1 = args[1] as i64 as u64;
            let mut a2 = args[2] as i64 as u64;
            let mut a3 = args[3] as i64 as u64;
            let a4 = args[4] as i64 as u64;
            let a5 = args[5] as i64 as u64;
            let a6 = args[6] as i64 as u64;
            let a7 = args[7] as i64 as u64;
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
        _ => unreachable!(),
    }
}
