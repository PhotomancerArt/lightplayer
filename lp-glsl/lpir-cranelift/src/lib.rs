//! LPIR → Cranelift: host JIT by default; optional `riscv32-emu` for RV32 object emission,
//! linking with `lp-glsl-builtins-emu-app`, and `lp-riscv-emu` execution helpers.

#![no_std]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod builtins;
mod call;
mod compile;
mod compile_options;
mod direct_call;
mod emit;
pub mod error;
mod invoke;
#[cfg(not(feature = "std"))]
mod jit_memory;
mod jit_module;
mod module_lower;
mod process_sync;
mod q32;
mod q32_options;
mod values;

#[cfg(feature = "riscv32-emu")]
mod emu_run;
#[cfg(feature = "riscv32-emu")]
mod object_link;
#[cfg(feature = "riscv32-emu")]
mod object_module;

#[cfg(feature = "std")]
pub use compile::jit;
pub use compile::{jit_from_ir, jit_from_ir_owned};
#[cfg(feature = "riscv32-emu")]
pub use compile::{object_bytes_from_ir, run_lpir_function_i32};
pub use compile_options::{CompileOptions, MemoryStrategy};
pub use direct_call::DirectCall;
pub use emit::signature_for_ir_func;
#[cfg(feature = "riscv32-emu")]
pub use emu_run::glsl_q32_call_emulated;
pub use error::{CompileError, CompilerError};
pub use jit_module::JitModule;
pub use lpir::FloatMode;
#[cfg(feature = "riscv32-emu")]
pub use object_link::link_object_with_builtins;
pub use q32_options::{AddSubMode, DivMode, MulMode, Q32Options};
pub use values::{CallError, CallResult, GlslQ32, GlslReturn};

#[cfg(test)]
mod tests {
    use core::mem;

    use lpir::parse_module;

    #[cfg(feature = "std")]
    use super::jit;
    use super::{
        AddSubMode, CompileError, CompileOptions, CompilerError, DivMode, FloatMode, GlslQ32,
        MemoryStrategy, MulMode, Q32Options, jit_from_ir,
    };

    #[test]
    fn jit_linear_fadd_f32() {
        let ir = parse_module(
            r"func @add(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
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
        let code_ptr = m.finalized_ptr_by_index(0);
        let add: extern "C" fn(f32, f32) -> f32 = unsafe { mem::transmute(code_ptr) };
        assert!((add(1.0, 2.0) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_if_else() {
        let ir = parse_module(
            r"func @max(v0:f32, v1:f32) -> f32 {
  v2:i32 = fgt v0, v1
  if v2 {
    return v0
  } else {
    return v1
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
        let f: extern "C" fn(f32, f32) -> f32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert!((f(3.0, 1.0) - 3.0).abs() < 1e-5);
        assert!((f(1.0, 5.0) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_if_no_else() {
        let ir = parse_module(
            r"func @clamp_positive(v0:f32) -> f32 {
  v1:f32 = fconst.f32 0.0
  v2:i32 = flt v0, v1
  if v2 {
    v0 = copy v1
  }
  return v0
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
        let f: extern "C" fn(f32) -> f32 = unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert!((f(-3.0) - 0.0).abs() < 1e-5);
        assert!((f(5.0) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_loop_countdown_sum() {
        let ir = parse_module(
            r"func @sum_to_n(v0:i32) -> i32 {
  v1:i32 = iconst.i32 0
  loop {
    br_if_not v0
    v1 = iadd v1, v0
    v0 = isub_imm v0, 1
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
        let f: extern "C" fn(i32) -> i32 = unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(5), 15);
    }

    #[test]
    fn test_loop_break() {
        let ir = parse_module(
            r"func @first_below(v0:f32, v1:f32) -> f32 {
  v2:f32 = fconst.f32 1.0
  loop {
    v3:i32 = flt v0, v1
    if v3 {
      break
    }
    v0 = fsub v0, v2
  }
  return v0
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
        let f: extern "C" fn(f32, f32) -> f32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert!((f(10.0, 3.0) - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_switch_basic() {
        let ir = parse_module(
            r"func @classify(v0:i32) -> i32 {
  v1:i32 = iconst.i32 0
  switch v0 {
    case 1 {
      v1 = iconst.i32 10
    }
    case 2 {
      v1 = iconst.i32 20
    }
    default {
      v1 = iconst.i32 -1
    }
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
        let f: extern "C" fn(i32) -> i32 = unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(1), 10);
        assert_eq!(f(2), 20);
        assert_eq!(f(99), -1);
    }

    #[test]
    fn test_switch_no_default() {
        let ir = parse_module(
            r"func @map_value(v0:i32) -> i32 {
  v1:i32 = iconst.i32 0
  switch v0 {
    case 0 {
      v1 = iconst.i32 100
    }
    case 1 {
      v1 = iconst.i32 200
    }
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
        let f: extern "C" fn(i32) -> i32 = unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(0), 100);
        assert_eq!(f(1), 200);
        assert_eq!(f(5), 0);
    }

    #[test]
    fn test_slot_load_store() {
        let ir = parse_module(
            r"func @roundtrip(v0:f32) -> f32 {
  slot ss0, 4
  v1:i32 = slot_addr ss0
  store v1, 0, v0
  v2:f32 = load v1, 0
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
        let f: extern "C" fn(f32) -> f32 = unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert!((f(42.0) - 42.0).abs() < 1e-5);
    }

    #[test]
    fn test_slot_two_values() {
        let ir = parse_module(
            r"func @swap_slot(v0:f32, v1:f32) -> (f32, f32) {
  slot ss0, 8
  v2:i32 = slot_addr ss0
  store v2, 0, v0
  store v2, 4, v1
  v3:f32 = load v2, 4
  v4:f32 = load v2, 0
  return v3, v4
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
        let f: extern "C" fn(f32, f32) -> (f32, f32) =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        let (a, b) = f(1.0, 2.0);
        assert!((a - 2.0).abs() < 1e-5);
        assert!((b - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_memcpy_slots() {
        let ir = parse_module(
            r"func @copy_slot(v0:f32, v1:f32) -> (f32, f32) {
  slot ss0, 8
  slot ss1, 8
  v2:i32 = slot_addr ss0
  store v2, 0, v0
  store v2, 4, v1
  v3:i32 = slot_addr ss1
  memcpy v3, v2, 8
  v4:f32 = load v3, 0
  v5:f32 = load v3, 4
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
        let f: extern "C" fn(f32, f32) -> (f32, f32) =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        let (a, b) = f(3.0, 7.0);
        assert!((a - 3.0).abs() < 1e-5);
        assert!((b - 7.0).abs() < 1e-5);
    }

    #[test]
    fn test_local_call() {
        let ir = parse_module(
            r"func @double(v0:f32) -> f32 {
  v1:f32 = fadd v0, v0
  return v1
}

func @quad(v0:f32) -> f32 {
  v1:f32 = call @double(v0)
  v2:f32 = call @double(v1)
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
        let quad: extern "C" fn(f32) -> f32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(1)) };
        assert!((quad(3.0) - 12.0).abs() < 1e-5);
    }

    #[test]
    fn test_multi_return_call() {
        let ir = parse_module(
            r"func @swap_vals(v0:f32, v1:f32) -> (f32, f32) {
  return v1, v0
}

func @double_swap(v0:f32, v1:f32) -> (f32, f32) {
  v2:f32, v3:f32 = call @swap_vals(v0, v1)
  v4:f32, v5:f32 = call @swap_vals(v2, v3)
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
        let f: extern "C" fn(f32, f32) -> (f32, f32) =
            unsafe { mem::transmute(m.finalized_ptr_by_index(1)) };
        let (a, b) = f(1.0, 2.0);
        assert!((a - 1.0).abs() < 1e-5 && (b - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_recursive_factorial() {
        let ir = parse_module(
            r"func @factorial(v0:i32) -> i32 {
  v1:i32 = iconst.i32 1
  v2:i32 = ile_s v0, v1
  if v2 {
    return v1
  }
  v3:i32 = isub_imm v0, 1
  v4:i32 = call @factorial(v3)
  v5:i32 = imul v0, v4
  return v5
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
        let f: extern "C" fn(i32) -> i32 = unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(5), 120);
    }

    #[test]
    fn test_call_in_loop() {
        let ir = parse_module(
            r"func @add1(v0:i32) -> i32 {
  v1:i32 = iadd_imm v0, 1
  return v1
}

func @count_up(v0:i32, v1:i32) -> i32 {
  loop {
    v2:i32 = ige_s v0, v1
    if v2 {
      break
    }
    v0 = call @add1(v0)
  }
  return v0
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
        let f: extern "C" fn(i32, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(1)) };
        assert_eq!(f(0, 5), 5);
    }

    #[test]
    fn jit_rejects_f32_with_imports() {
        let ir = parse_module(
            r"import @glsl::sin(f32) -> f32

func @u(v0:f32) -> f32 {
  v1:f32 = call @glsl::sin(v0)
  return v1
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
  v0:f32 = fconst.f32 0.5
  return v0
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
        let f: extern "C" fn() -> i32 = unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(), 32768);
    }

    #[test]
    fn jit_q32_identity() {
        let ir = parse_module(
            r"func @identity(v0:f32) -> f32 {
  return v0
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
        let f: extern "C" fn(i32) -> i32 = unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(65536), 65536);
    }

    #[test]
    fn jit_q32_fadd_builtin() {
        let ir = parse_module(
            r"func @add(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
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
        let f: extern "C" fn(i32, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(q32(1.0), q32(2.0)), q32(3.0));
    }

    #[test]
    fn jit_q32_fmul_builtin() {
        let ir = parse_module(
            r"func @mul(v0:f32, v1:f32) -> f32 {
  v2:f32 = fmul v0, v1
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
        let f: extern "C" fn(i32, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(q32(2.0), q32(3.0)), q32(6.0));
    }

    #[test]
    fn jit_q32_fdiv_builtin() {
        let ir = parse_module(
            r"func @div(v0:f32, v1:f32) -> f32 {
  v2:f32 = fdiv v0, v1
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
        let f: extern "C" fn(i32, i32) -> i32 =
            unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(q32(6.0), q32(2.0)), q32(3.0));
    }

    #[test]
    fn jit_q32_import_sin() {
        let ir = parse_module(
            r"import @glsl::sin(f32) -> f32

func @apply_sin(v0:f32) -> f32 {
  v1:f32 = call @glsl::sin(v0)
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
        let f: extern "C" fn(i32) -> i32 = unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_eq!(f(q32(0.0)), q32(0.0));
        assert_q32_approx(f(q32(core::f32::consts::FRAC_PI_2)), 1.0, 0.02);
    }

    #[test]
    fn jit_q32_quadratic() {
        let ir = parse_module(
            r"func @quadratic(v0:f32) -> f32 {
  v1:f32 = fmul v0, v0
  v2:f32 = fconst.f32 2.0
  v3:f32 = fmul v2, v0
  v4:f32 = fadd v1, v3
  v5:f32 = fconst.f32 1.0
  v6:f32 = fadd v4, v5
  return v6
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
        let f: extern "C" fn(i32) -> i32 = unsafe { mem::transmute(m.finalized_ptr_by_index(0)) };
        assert_q32_approx(f(q32(3.0)), 16.0, 0.05);
    }

    #[test]
    #[cfg(feature = "std")]
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
            .call("add", &[GlslQ32::Float(1.0), GlslQ32::Float(2.0)])
            .expect("call");
        match ret.value {
            Some(GlslQ32::Float(x)) => assert!((x - 3.0).abs() < 1e-5),
            other => panic!("expected float ~3.0, got {other:?}"),
        }
    }

    #[test]
    #[cfg(feature = "std")]
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
        let a = crate::q32::q32_encode_f64(1.25);
        let b = crate::q32::q32_encode_f64(-0.5);
        let via_direct = unsafe { dc.call_i32(&[a, b]).expect("direct invoke") };
        let via_call = m
            .call("add", &[GlslQ32::Float(1.25), GlslQ32::Float(-0.5)])
            .expect("typed call");
        assert_eq!(via_direct.len(), 1);
        match via_call.value {
            Some(GlslQ32::Float(x)) => {
                assert_eq!(via_direct[0], crate::q32::q32_encode_f64(x));
            }
            other => panic!("expected float return, got {other:?}"),
        }
    }

    #[test]
    fn q32_jit_invoke_two_returns() {
        let ir = parse_module(
            r"func @pair() -> (f32, f32) {
  v0:f32 = fconst.f32 1.0
  v1:f32 = fconst.f32 2.0
  return v0, v1
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
        let words = unsafe { crate::invoke::invoke_i32_args_returns(p, &[], 2).expect("invoke") };
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
  v0:f32 = fconst.f32 1.0
  v1:f32 = fconst.f32 2.0
  v2:f32 = fconst.f32 0.5
  return v0, v1, v2
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
        let words = unsafe { crate::invoke::invoke_i32_args_returns(p, &[], 3).expect("invoke") };
        assert_eq!(words.len(), 3, "{words:?}");
        assert_q32_approx(words[0], 1.0, 1e-4);
        assert_q32_approx(words[1], 2.0, 1e-4);
        assert_q32_approx(words[2], 0.5, 1e-4);
    }

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

    #[test]
    fn low_memory_strategy_compiles() {
        let ir = parse_module(
            r"func @big(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
  v3:f32 = fadd v2, v0
  v4:f32 = fadd v3, v1
  return v4
}

func @small(v0:f32) -> f32 {
  return v0
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
        crate::q32::q32_encode(f)
    }

    fn assert_q32_approx(actual: i32, expected_f64: f64, tolerance: f64) {
        let actual_f64 = actual as f64 / 65536.0;
        assert!(
            (actual_f64 - expected_f64).abs() < tolerance,
            "Q32 mismatch: got {actual_f64} (raw {actual}), expected {expected_f64}"
        );
    }
}
