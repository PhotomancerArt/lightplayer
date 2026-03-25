//! `extern "C"` dispatch for small arities (native JIT pointers).

use alloc::vec::Vec;
use core::mem::transmute;

use crate::values::CallError;

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
        2 => {
            let r = match args.len() {
                0 => {
                    let f: extern "C" fn() -> (i32, i32) = unsafe { transmute(code) };
                    f()
                }
                1 => {
                    let f: extern "C" fn(i32) -> (i32, i32) = unsafe { transmute(code) };
                    f(args[0])
                }
                2 => {
                    let f: extern "C" fn(i32, i32) -> (i32, i32) = unsafe { transmute(code) };
                    f(args[0], args[1])
                }
                3 => {
                    let f: extern "C" fn(i32, i32, i32) -> (i32, i32) = unsafe { transmute(code) };
                    f(args[0], args[1], args[2])
                }
                4 => {
                    let f: extern "C" fn(i32, i32, i32, i32) -> (i32, i32) =
                        unsafe { transmute(code) };
                    f(args[0], args[1], args[2], args[3])
                }
                5 => {
                    let f: extern "C" fn(i32, i32, i32, i32, i32) -> (i32, i32) =
                        unsafe { transmute(code) };
                    f(args[0], args[1], args[2], args[3], args[4])
                }
                6 => {
                    let f: extern "C" fn(i32, i32, i32, i32, i32, i32) -> (i32, i32) =
                        unsafe { transmute(code) };
                    f(args[0], args[1], args[2], args[3], args[4], args[5])
                }
                7 => {
                    let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32) -> (i32, i32) =
                        unsafe { transmute(code) };
                    f(
                        args[0], args[1], args[2], args[3], args[4], args[5], args[6],
                    )
                }
                8 => {
                    let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32, i32) -> (i32, i32) =
                        unsafe { transmute(code) };
                    f(
                        args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
                    )
                }
                _ => unreachable!(),
            };
            Ok(alloc::vec![r.0, r.1])
        }
        3 => {
            let r = match args.len() {
                0 => {
                    let f: extern "C" fn() -> (i32, i32, i32) = unsafe { transmute(code) };
                    f()
                }
                1 => {
                    let f: extern "C" fn(i32) -> (i32, i32, i32) = unsafe { transmute(code) };
                    f(args[0])
                }
                2 => {
                    let f: extern "C" fn(i32, i32) -> (i32, i32, i32) = unsafe { transmute(code) };
                    f(args[0], args[1])
                }
                3 => {
                    let f: extern "C" fn(i32, i32, i32) -> (i32, i32, i32) =
                        unsafe { transmute(code) };
                    f(args[0], args[1], args[2])
                }
                4 => {
                    let f: extern "C" fn(i32, i32, i32, i32) -> (i32, i32, i32) =
                        unsafe { transmute(code) };
                    f(args[0], args[1], args[2], args[3])
                }
                _ => {
                    return Err(CallError::Unsupported(
                        "3-return call with more than 4 args not implemented".into(),
                    ));
                }
            };
            Ok(alloc::vec![r.0, r.1, r.2])
        }
        4 => {
            let r = match args.len() {
                0 => {
                    let f: extern "C" fn() -> (i32, i32, i32, i32) = unsafe { transmute(code) };
                    f()
                }
                1 => {
                    let f: extern "C" fn(i32) -> (i32, i32, i32, i32) = unsafe { transmute(code) };
                    f(args[0])
                }
                2 => {
                    let f: extern "C" fn(i32, i32) -> (i32, i32, i32, i32) =
                        unsafe { transmute(code) };
                    f(args[0], args[1])
                }
                3 => {
                    let f: extern "C" fn(i32, i32, i32) -> (i32, i32, i32, i32) =
                        unsafe { transmute(code) };
                    f(args[0], args[1], args[2])
                }
                4 => {
                    let f: extern "C" fn(i32, i32, i32, i32) -> (i32, i32, i32, i32) =
                        unsafe { transmute(code) };
                    f(args[0], args[1], args[2], args[3])
                }
                _ => {
                    return Err(CallError::Unsupported(
                        "4-return call with more than 4 args not implemented".into(),
                    ));
                }
            };
            Ok(alloc::vec![r.0, r.1, r.2, r.3])
        }
        _ => unreachable!(),
    }
}
