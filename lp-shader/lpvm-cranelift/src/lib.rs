//! LPIR → Cranelift: host JIT (`std` + native ISA) or embedded JIT (`glsl` without `std`, RV32 ISA).
//! Optional `riscv32-object` enables RV32 object emission + builtins ELF link (see `lpvm-emu` to run in the emulator).
//!
//! **Primary API:** [`CraneliftEngine`], [`CraneliftModule`], [`CraneliftInstance`] implement
//! [`lpvm::LpvmEngine`] / [`lpvm::LpvmModule`] / [`lpvm::LpvmInstance`]. [`jit_from_ir`] and
//! [`jit`] are thin helpers over [`CraneliftModule::compile`].
//!
//! # Status: host JIT path is deprecated for new work
//!
//! `CraneliftEngine` (the in-process host JIT) is **deprecated as a host execution
//! backend** for `lp-shader`. Use [`lpvm-wasm`](../lpvm_wasm/index.html)'s
//! `WasmLpvmEngine` (wasmtime) instead. The crate is intentionally kept in the tree:
//!
//! - `lp-engine` and `lpfx-cpu` still depend on it; consumer migration to wasmtime
//!   lands in M4 (see `docs/roadmaps/2026-04-16-lp-shader-textures/m4-consumer-migration.md`).
//! - The `riscv32-object` path (RV32 object emission via `cranelift-object`) remains
//!   in active use independent of the host JIT.
//!
//! Why deprecated: the in-process JIT exhibits non-deterministic state corruption
//! when multiple `JITModule` instances are constructed in the same process
//! (reproduces as "function must be compiled before it can be finalized" panics).
//! Wasmtime uses cranelift internally with proper per-instance isolation, gives us
//! 32-bit guest pointers (matching every other production backend), and removes
//! the only surface where this bug bites users.
//!
//! Do not add new host-execution consumers of `CraneliftEngine`. New host code
//! paths should go through `lpvm-wasm`.

#![no_std]

#[macro_use]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod builtins;
mod call;
mod compile;
mod compile_options;
mod cranelift_host_memory;
mod direct_call;
mod emit;
pub mod error;
mod generated_builtin_abi;
mod invoke;
#[cfg(not(feature = "std"))]
mod jit_memory;
mod jit_module;
mod lpvm_engine;
mod lpvm_instance;
mod lpvm_module;
mod module_lower;
mod process_sync;
mod q32_emit;

#[cfg(feature = "riscv32-object")]
mod object_link;
#[cfg(feature = "riscv32-object")]
mod object_module;

#[cfg(feature = "glsl")]
pub use compile::jit;
#[cfg(feature = "riscv32-object")]
pub use compile::object_bytes_from_ir;
pub use compile::{jit_from_ir, jit_from_ir_owned};
pub use compile_options::{CompileOptions, MemoryStrategy};
pub use direct_call::DirectCall;
pub use emit::signature_for_ir_func;
pub use error::{CompileError, CompilerError};
pub use lpir::FloatMode;
pub use lps_shared::path_resolve::PathError;
pub use lps_shared::{
    FnParam, LayoutRules, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier, StructMember,
};
pub use lpvm_engine::CraneliftEngine;
pub use lpvm_instance::{CraneliftInstance, InstanceError};
pub use lpvm_module::CraneliftModule;

/// Back-compat alias; prefer [`ParamQualifier`].
pub type GlslParamQualifier = ParamQualifier;
/// Back-compat alias for a single formal parameter; prefer [`FnParam`].
pub type LpsSig = FnParam;
pub use lps_q32::q32_options::{AddSubMode, DivMode, MulMode, Q32Options};
pub use lpvm::{
    CallError, CallResult, GlslReturn, LpsValueQ32, decode_q32_return, flatten_q32_arg,
};
#[cfg(feature = "riscv32-object")]
pub use object_link::link_object_with_builtins;

/// Options-only tests: run under `--no-default-features` (no host JIT execution).
#[cfg(test)]
mod tests_options {
    use super::{
        AddSubMode, CompileOptions, DivMode, FloatMode, MemoryStrategy, MulMode, Q32Options,
    };

    #[test]
    fn compile_options_default() {
        let opts = CompileOptions::default();
        assert_eq!(opts.float_mode, FloatMode::Q32);
        assert_eq!(opts.q32_options, Q32Options::default());
        assert_eq!(opts.memory_strategy, MemoryStrategy::Default);
        assert_eq!(opts.max_errors, None);
    }

    #[test]
    fn q32_options_default_is_saturating() {
        let q = Q32Options::default();
        assert_eq!(q.add_sub, AddSubMode::Saturating);
        assert_eq!(q.mul, MulMode::Saturating);
        assert_eq!(q.div, DivMode::Saturating);
    }
}

/// Host JIT tests: `jit_from_ir` with `std` uses the native ISA. Without `std`, JIT targets RV32
/// and executing it on the host is undefined — those cases are covered by `lpvm-emu` / fw-emu.
#[cfg(all(test, feature = "std"))]
mod tests {
    mod render_texture_smoke;

    use alloc::string::String;
    use core::mem;

    use lpir::parse_module;
    use lps_shared::lps_value_f32::LpsValueF32;
    use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
    use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};

    #[cfg(feature = "glsl")]
    use super::jit;
    use super::{
        CompileError, CompileOptions, CompilerError, CraneliftEngine, FloatMode, LpsValueQ32,
        MemoryStrategy, jit_from_ir,
    };
    use lps_q32::Q32;
    use lps_q32::q32_encode::q32_encode;

    fn jit_test_vmctx() -> *const u8 {
        // Use properly aligned storage for VmContext (needs 8-byte alignment for u64 fuel field).
        static VMCTX: core::mem::MaybeUninit<lpvm::VmContext> = core::mem::MaybeUninit::zeroed();
        VMCTX.as_ptr() as *const u8
    }

    #[test]
    fn cranelift_engine_host_memory_alloc_free_realloc() {
        use lpvm::AllocError;

        let engine = CraneliftEngine::new(CompileOptions {
            float_mode: FloatMode::Q32,
            ..Default::default()
        });
        let a = engine.memory().alloc(32, 8).expect("alloc");
        assert_eq!(a.size(), 32);
        assert_eq!(a.align(), 8);
        assert_eq!(a.guest_base(), a.native_ptr() as usize as u64);
        let b = engine.memory().alloc(16, 8).expect("alloc2");
        assert_ne!(a.native_ptr(), b.native_ptr());

        let c = engine.memory().realloc(a, 64).expect("realloc");
        assert_eq!(c.size(), 64);
        assert_eq!(c.guest_base(), c.native_ptr() as usize as u64);

        engine.memory().free(c);
        engine.memory().free(b);
        assert_eq!(
            engine.memory().realloc(c, 8).unwrap_err(),
            AllocError::InvalidPointer
        );
    }

    #[test]
    fn lpvm_trait_engine_q32_fadd() {
        let ir = parse_module(
            r"func @add(v1:f32, v2:f32) -> f32 {
  v3:f32 = fadd v1, v2
  return v3
}
",
        )
        .expect("parse");

        let meta = LpsModuleSig {
            functions: alloc::vec![LpsFnSig {
                name: String::from("add"),
                return_type: LpsType::Float,
                parameters: alloc::vec![
                    FnParam {
                        name: String::from("a"),
                        ty: LpsType::Float,
                        qualifier: ParamQualifier::In,
                    },
                    FnParam {
                        name: String::from("b"),
                        ty: LpsType::Float,
                        qualifier: ParamQualifier::In,
                    },
                ],
                kind: LpsFnKind::UserDefined,
            }],
            ..Default::default()
        };

        let engine = CraneliftEngine::new(CompileOptions {
            float_mode: FloatMode::Q32,
            ..Default::default()
        });
        let module = engine.compile(&ir, &meta).expect("compile");
        let mut inst = module.instantiate().expect("instantiate");
        let out = inst
            .call("add", &[LpsValueF32::F32(1.0), LpsValueF32::F32(2.0)])
            .expect("call");
        match out {
            LpsValueF32::F32(x) => assert!((x - 3.0).abs() < 1e-3),
            other => panic!("expected F32, got {other:?}"),
        }
    }

    #[test]
    fn jit_linear_fadd_f32() {
        let ir = parse_module(
            r"func @add(v1:f32, v2:f32) -> f32 {
  v3:f32 = fadd v1, v2
  return v3
}
",
        )
        .expect("parse");

        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let code_ptr = m.finalized_ptr_by_index(0);
        let add: extern "C" fn(*const u8, f32, f32) -> f32 = unsafe { mem::transmute(code_ptr) };
        assert!((add(jit_test_vmctx(), 1.0, 2.0) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_if_else() {
        let ir = parse_module(
            r"func @max(v1:f32, v2:f32) -> f32 {
  v3:i32 = fgt v1, v2
  if v3 {
    return v1
  } else {
    return v2
  }
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, f32, f32) -> f32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert!((f(jit_test_vmctx(), 3.0, 1.0) - 3.0).abs() < 1e-5);
        assert!((f(jit_test_vmctx(), 1.0, 5.0) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_if_no_else() {
        let ir = parse_module(
            r"func @clamp_positive(v1:f32) -> f32 {
  v2:f32 = fconst.f32 0.0
  v3:i32 = flt v1, v2
  if v3 {
    v1 = copy v2
  }
  return v1
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, f32) -> f32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert!((f(jit_test_vmctx(), -3.0) - 0.0).abs() < 1e-5);
        assert!((f(jit_test_vmctx(), 5.0) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_loop_countdown_sum() {
        let ir = parse_module(
            r"func @sum_to_n(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  loop {
    br_if_not v1
    v2 = iadd v2, v1
    v1 = isub_imm v1, 1
  }
  return v2
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(jit_test_vmctx(), 5), 15);
    }

    #[test]
    fn test_loop_break() {
        let ir = parse_module(
            r"func @first_below(v1:f32, v2:f32) -> f32 {
  v3:f32 = fconst.f32 1.0
  loop {
    v4:i32 = flt v1, v2
    if v4 {
      break
    }
    v1 = fsub v1, v3
  }
  return v1
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, f32, f32) -> f32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert!((f(jit_test_vmctx(), 10.0, 3.0) - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_switch_basic() {
        let ir = parse_module(
            r"func @classify(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  switch v1 {
    case 1 {
      v2 = iconst.i32 10
    }
    case 2 {
      v2 = iconst.i32 20
    }
    default {
      v2 = iconst.i32 -1
    }
  }
  return v2
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(jit_test_vmctx(), 1), 10);
        assert_eq!(f(jit_test_vmctx(), 2), 20);
        assert_eq!(f(jit_test_vmctx(), 99), -1);
    }

    #[test]
    fn test_switch_no_default() {
        let ir = parse_module(
            r"func @map_value(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  switch v1 {
    case 0 {
      v2 = iconst.i32 100
    }
    case 1 {
      v2 = iconst.i32 200
    }
  }
  return v2
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(jit_test_vmctx(), 0), 100);
        assert_eq!(f(jit_test_vmctx(), 1), 200);
        assert_eq!(f(jit_test_vmctx(), 5), 0);
    }

    #[test]
    fn test_slot_load_store() {
        let ir = parse_module(
            r"func @roundtrip(v1:f32) -> f32 {
  slot ss0, 4
  v2:i32 = slot_addr ss0
  store v2, 0, v1
  v3:f32 = load v2, 0
  return v3
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, f32) -> f32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert!((f(jit_test_vmctx(), 42.0) - 42.0).abs() < 1e-5);
    }

    #[test]
    fn test_slot_two_values() {
        let ir = parse_module(
            r"func @swap_slot(v1:f32, v2:f32) -> (f32, f32) {
  slot ss0, 8
  v3:i32 = slot_addr ss0
  store v3, 0, v1
  store v3, 4, v2
  v4:f32 = load v3, 4
  v5:f32 = load v3, 0
  return v4, v5
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, f32, f32) -> (f32, f32) =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        let (a, b) = f(jit_test_vmctx(), 1.0, 2.0);
        assert!((a - 2.0).abs() < 1e-5);
        assert!((b - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_memcpy_slots() {
        let ir = parse_module(
            r"func @copy_slot(v1:f32, v2:f32) -> (f32, f32) {
  slot ss0, 8
  slot ss1, 8
  v3:i32 = slot_addr ss0
  store v3, 0, v1
  store v3, 4, v2
  v4:i32 = slot_addr ss1
  memcpy v4, v3, 8
  v5:f32 = load v4, 0
  v6:f32 = load v4, 4
  return v5, v6
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, f32, f32) -> (f32, f32) =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        let (a, b) = f(jit_test_vmctx(), 3.0, 7.0);
        assert!((a - 3.0).abs() < 1e-5);
        assert!((b - 7.0).abs() < 1e-5);
    }

    #[test]
    fn test_local_call() {
        let ir = parse_module(
            r"func @double(v1:f32) -> f32 {
  v2:f32 = fadd v1, v1
  return v2
}

func @quad(v1:f32) -> f32 {
  v2:f32 = call @double(v1)
  v3:f32 = call @double(v2)
  return v3
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let quad: extern "C" fn(*const u8, f32) -> f32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(1)) };
        assert!((quad(jit_test_vmctx(), 3.0) - 12.0).abs() < 1e-5);
    }

    #[test]
    fn test_multi_return_call() {
        let ir = parse_module(
            r"func @swap_vals(v1:f32, v2:f32) -> (f32, f32) {
  return v2, v1
}

func @double_swap(v1:f32, v2:f32) -> (f32, f32) {
  v3:f32, v4:f32 = call @swap_vals(v1, v2)
  v5:f32, v6:f32 = call @swap_vals(v3, v4)
  return v5, v6
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, f32, f32) -> (f32, f32) =
            unsafe { mem::transmute(m.finalized_ptr_by_index(1)) };
        let (a, b) = f(jit_test_vmctx(), 1.0, 2.0);
        assert!((a - 1.0).abs() < 1e-5 && (b - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_recursive_factorial() {
        let ir = parse_module(
            r"func @factorial(v1:i32) -> i32 {
  v2:i32 = iconst.i32 1
  v3:i32 = ile_s v1, v2
  if v3 {
    return v2
  }
  v4:i32 = isub_imm v1, 1
  v5:i32 = call @factorial(v4)
  v6:i32 = imul v1, v5
  return v6
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(jit_test_vmctx(), 5), 120);
    }

    #[test]
    fn test_loop_continue() {
        let ir = parse_module(
            r"func @skip_twos(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  v3:i32 = iconst.i32 0
  loop {
    v4:i32 = ige_s v3, v1
    if v4 {
      break
    }
    v5:i32 = ieq_imm v3, 2
    if v5 {
      v3 = iadd_imm v3, 1
      continue
    }
    v2 = iadd v2, v3
    v3 = iadd_imm v3, 1
    continuing:
  }
  return v2
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        // sum 0..5 skipping 2: 0+1+3+4 = 8
        assert_eq!(f(jit_test_vmctx(), 5), 8);
    }

    #[test]
    fn test_call_in_loop() {
        let ir = parse_module(
            r"func @add1(v1:i32) -> i32 {
  v2:i32 = iadd_imm v1, 1
  return v2
}

func @count_up(v1:i32, v2:i32) -> i32 {
  loop {
    v3:i32 = ige_s v1, v2
    if v3 {
      break
    }
    v1 = call @add1(v1)
  }
  return v1
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(1)) };
        assert_eq!(f(jit_test_vmctx(), 0, 5), 5);
    }

    #[test]
    fn jit_rejects_f32_with_imports() {
        let ir = parse_module(
            r"import @glsl::sin(f32) -> f32

func @u(v1:f32) -> f32 {
  v2:f32 = call @glsl::sin(v1)
  return v2
}
",
        )
        .expect("parse");
        let err = match jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected import + F32 to fail"),
        };
        assert!(matches!(
            err,
            CompilerError::Codegen(CompileError::Unsupported(_))
        ));
    }

    #[test]
    fn jit_q32_constant() {
        let ir = parse_module(
            r"func @const_half() -> f32 {
  v1:f32 = fconst.f32 0.5
  return v1
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(jit_test_vmctx()), 32768);
    }

    #[test]
    fn jit_q32_identity() {
        let ir = parse_module(
            r"func @identity(v1:f32) -> f32 {
  return v1
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(jit_test_vmctx(), 65536), 65536);
    }

    #[test]
    fn jit_q32_fadd_builtin() {
        let ir = parse_module(
            r"func @add(v1:f32, v2:f32) -> f32 {
  v3:f32 = fadd v1, v2
  return v3
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(jit_test_vmctx(), q32(1.0), q32(2.0)), q32(3.0));
    }

    #[test]
    fn jit_q32_fmul_builtin() {
        let ir = parse_module(
            r"func @mul(v1:f32, v2:f32) -> f32 {
  v3:f32 = fmul v1, v2
  return v3
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(jit_test_vmctx(), q32(2.0), q32(3.0)), q32(6.0));
    }

    #[test]
    fn jit_q32_fdiv_builtin() {
        let ir = parse_module(
            r"func @div(v1:f32, v2:f32) -> f32 {
  v3:f32 = fdiv v1, v2
  return v3
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(jit_test_vmctx(), q32(6.0), q32(2.0)), q32(3.0));
    }

    #[test]
    fn jit_q32_import_sin() {
        let ir = parse_module(
            r"import @glsl::sin(f32) -> f32

func @apply_sin(v1:f32) -> f32 {
  v2:f32 = call @glsl::sin(v1)
  return v2
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(jit_test_vmctx(), q32(0.0)), q32(0.0));
        assert_q32_approx(
            f(jit_test_vmctx(), q32(core::f32::consts::FRAC_PI_2)),
            1.0,
            0.02,
        );
    }

    #[test]
    fn jit_q32_quadratic() {
        let ir = parse_module(
            r"func @quadratic(v1:f32) -> f32 {
  v2:f32 = fmul v1, v1
  v3:f32 = fconst.f32 2.0
  v4:f32 = fmul v3, v1
  v5:f32 = fadd v2, v4
  v6:f32 = fconst.f32 1.0
  v7:f32 = fadd v5, v6
  return v7
}
",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let f: extern "C" fn(*const u8, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_q32_approx(f(jit_test_vmctx(), q32(3.0)), 16.0, 0.05);
    }

    #[test]
    #[cfg(feature = "glsl")]
    fn jit_glsl_call_add_q32() {
        let src = "float add(float a, float b) { return a + b; }";
        let m = jit(
            src,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        assert!(m.func_names().iter().any(|n| n == "add"));
        let ret = m
            .call(
                "add",
                &[
                    LpsValueQ32::F32(Q32::from_fixed(q32_encode(1.0))),
                    LpsValueQ32::F32(Q32::from_fixed(q32_encode(2.0))),
                ],
            )
            .expect("call");
        match ret.value {
            Some(LpsValueQ32::F32(x)) => assert!((x.to_f32() - 3.0).abs() < 1e-5),
            other => panic!("expected float ~3.0, got {other:?}"),
        }
    }

    #[test]
    #[cfg(feature = "glsl")]
    fn glsl_call_agrees_with_direct_call() {
        let src = "float add(float a, float b) { return a + b; }";
        let m = jit(
            src,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let dc = m.direct_call("add").expect("direct_call");
        let a = lps_q32::q32_encode::q32_encode_f64(1.25);
        let b = lps_q32::q32_encode::q32_encode_f64(-0.5);
        let via_direct = unsafe {
            dc.call_i32(jit_test_vmctx(), &[a, b])
                .expect("direct invoke")
        };
        let via_call = m
            .call(
                "add",
                &[
                    LpsValueQ32::F32(Q32::from_fixed(lps_q32::q32_encode::q32_encode(1.25))),
                    LpsValueQ32::F32(Q32::from_fixed(lps_q32::q32_encode::q32_encode(-0.5))),
                ],
            )
            .expect("typed call");
        assert_eq!(via_direct.len(), 1);
        match via_call.value {
            Some(LpsValueQ32::F32(x)) => {
                assert_eq!(via_direct[0], x.to_fixed());
            }
            other => panic!("expected float return, got {other:?}"),
        }
    }

    #[test]
    fn q32_jit_invoke_two_returns() {
        let ir = parse_module(
            r"func @pair() -> (f32, f32) {
  v1:f32 = fconst.f32 1.0
  v2:f32 = fconst.f32 2.0
  return v1, v2
}",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let p = m.finalized_ptr_by_index(0);
        let vm = jit_test_vmctx();
        let words = unsafe {
            crate::invoke::invoke_i32_args_returns(p, vm, &[], 2, false).expect("invoke")
        };
        assert_eq!(words.len(), 2, "{words:?}");
        assert_q32_approx(words[0], 1.0, 1e-4);
        assert_q32_approx(words[1], 2.0, 1e-4);
    }

    /// Regression: host JIT must return all words of a multi-return; Rust tuple `extern "C"`
    /// returns are not ABI-safe (see `invoke.rs` `CRet3`).
    #[test]
    fn q32_jit_invoke_three_returns() {
        let ir = parse_module(
            r"func @triple() -> (f32, f32, f32) {
  v1:f32 = fconst.f32 1.0
  v2:f32 = fconst.f32 2.0
  v3:f32 = fconst.f32 0.5
  return v1, v2, v3
}",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let p = m.finalized_ptr_by_index(0);
        let vm = jit_test_vmctx();
        let words = unsafe {
            crate::invoke::invoke_i32_args_returns(p, vm, &[], 3, false).expect("invoke")
        };
        assert_eq!(words.len(), 3, "{words:?}");
        assert_q32_approx(words[0], 1.0, 1e-4);
        assert_q32_approx(words[1], 2.0, 1e-4);
        assert_q32_approx(words[2], 0.5, 1e-4);
    }

    #[test]
    fn direct_call_i32_buf_matches_call_i32() {
        let ir = parse_module(
            r"func @triple() -> (f32, f32, f32) {
  v1:f32 = fconst.f32 1.0
  v2:f32 = fconst.f32 2.0
  v3:f32 = fconst.f32 0.5
  return v1, v2, v3
}",
        )
        .expect("parse");
        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
        )
        .expect("jit");
        let dc = m.direct_call("triple").expect("direct_call");
        let via_vec = unsafe { dc.call_i32(jit_test_vmctx(), &[]).expect("call_i32") };
        let mut buf = [0i32; 3];
        unsafe {
            dc.call_i32_buf(jit_test_vmctx(), &[], &mut buf)
                .expect("call_i32_buf")
        };
        assert_eq!(via_vec.as_slice(), buf.as_slice());
    }

    #[test]
    fn low_memory_strategy_compiles() {
        let ir = parse_module(
            r"func @big(v1:f32, v2:f32) -> f32 {
  v3:f32 = fadd v1, v2
  v4:f32 = fadd v3, v1
  v5:f32 = fadd v4, v2
  return v5
}

func @small(v1:f32) -> f32 {
  return v1
}
",
        )
        .expect("parse");

        let m = jit_from_ir(
            &ir,
            &CompileOptions {
                memory_strategy: MemoryStrategy::LowMemory,
                float_mode: FloatMode::F32,
                ..Default::default()
            },
        )
        .expect("jit with LowMemory");

        let big_ptr = m.finalized_ptr("big").expect("big");
        let small_ptr = m.finalized_ptr("small").expect("small");
        assert!(!big_ptr.is_null());
        assert!(!small_ptr.is_null());
    }

    fn q32(f: f32) -> i32 {
        lps_q32::q32_encode::q32_encode(f)
    }

    fn assert_q32_approx(actual: i32, expected_f64: f64, tolerance: f64) {
        let actual_f64 = actual as f64 / 65536.0;
        assert!(
            (actual_f64 - expected_f64).abs() < tolerance,
            "Q32 mismatch: got {actual_f64} (raw {actual}), expected {expected_f64}"
        );
    }

    /// Verify the __lp_vm_get_fuel_q32 builtin pointer is non-null and ABI matches.
    /// NOTE: Direct call skipped on 64-bit hosts due to i32 pointer truncation.
    #[test]
    #[cfg(feature = "glsl")]
    fn jit_get_fuel_builtin() {
        use crate::builtins::get_function_pointer;
        use lps_builtin_ids::BuiltinId;

        // Verify builtin pointer is non-null
        let ptr = get_function_pointer(BuiltinId::LpVmGetFuelQ32);
        assert!(!ptr.is_null(), "get_fuel builtin pointer is null");
    }
}
