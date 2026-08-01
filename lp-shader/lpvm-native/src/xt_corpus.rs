//! The Xtensa hardware-risk corpus: one named case per thing that can be wrong
//! about running LightPlayer-compiled code on an ESP32-S3.
//!
//! # Why this lives in `lpvm-native` rather than in the firmware
//!
//! Both sides need the *same* module, built the same way:
//!
//! - the host golden test runs it on `lp-xt-emu` via
//!   [`crate::rt_emu::NativeEmuEngine::new_for_isa`] (`emu-xt`),
//! - the ESP32-S3 harness runs it on real silicon via
//!   [`crate::rt_jit::NativeJitEngine`].
//!
//! Both reach it through the identical `LpvmEngine` → `LpvmModule` →
//! `LpvmInstance::call_q32` path, so a mismatch is a genuine difference between
//! the emulator and the chip rather than a difference between two test harnesses.
//! Duplicating the corpus into the firmware would silently allow the two to
//! drift, which is exactly the failure this module exists to prevent.
//!
//! `lpir` is `no_std`, so these build unchanged on the device.
//!
//! # Goldens are NOT generated from device output
//!
//! Every [`XtCase::invocations`] golden is a committed constant, derived from
//! LPIR semantics and independently confirmed on `lp-xt-emu` by
//! `tests/xt_corpus_goldens.rs`. **Never** refresh a golden from what the chip
//! printed: that inverts the test into a tautology that passes forever. A
//! mismatch is a finding to triage — an emitter bug, an emulator bug, or a
//! hardware behaviour the ABI contract got wrong.
//!
//! # Deliberately NOT covered
//!
//! PR #194 runs the 851-file GLSL corpus on this backend and left known
//! failures: 12-argument calls, spilling around calls, `from-scalar` as a first
//! call argument, and divide-by-zero semantics. Cases here stay out of those
//! clusters on purpose. A case that fails for an already-known compiler reason
//! produces a red line that means nothing, and trains the reader to ignore red
//! lines.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use lp_collection::VecMap;
use lpir::builder::FunctionBuilder;
use lpir::{CalleeRef, FuncId, ImportDecl, IrType, LpirModule, LpirOp, VMCTX_VREG};
use lps_builtin_ids::BuiltinId;
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};

/// One call of a case: arguments in, expected words out.
pub struct XtInvocation {
    /// Arguments *excluding* the vmctx word — the engine prepends that.
    pub args: &'static [i32],
    /// Expected return words. Committed; never regenerated from a device.
    pub golden: &'static [i32],
}

/// A named corpus case, tied to the hardware risk it exists to catch.
pub struct XtCase {
    pub name: &'static str,
    /// Which risk-surface item this covers, for the transcript.
    pub risk: &'static str,
    /// Entry function name inside the built module.
    pub entry: &'static str,
    pub invocations: &'static [XtInvocation],
    /// Builds the module. A fn pointer so the table stays `const` in `no_std`.
    pub build: fn() -> (LpirModule, LpsModuleSig),
    /// True when the case calls a builtin, so it needs the Xtensa builtins
    /// image on the host side (absent → the golden test skips that case).
    pub needs_builtins: bool,
}

/// Signature helper: `name(x: int) -> int`.
fn int_sig(name: &str) -> LpsFnSig {
    LpsFnSig {
        name: name.to_string(),
        parameters: vec![FnParam {
            name: String::from("x"),
            ty: LpsType::Int,
            qualifier: ParamQualifier::In,
        }],
        return_type: LpsType::Int,
        kind: LpsFnKind::UserDefined,
    }
}

/// Signature helper: `name(x: int) -> vec4` (4 return words => the sret path).
fn vec4_ret_sig(name: &str) -> LpsFnSig {
    LpsFnSig {
        name: name.to_string(),
        parameters: vec![FnParam {
            name: String::from("x"),
            ty: LpsType::Int,
            qualifier: ParamQualifier::In,
        }],
        return_type: LpsType::Vec4,
        kind: LpsFnKind::UserDefined,
    }
}

fn sig_of(fns: Vec<LpsFnSig>) -> LpsModuleSig {
    LpsModuleSig {
        functions: fns,
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Risk 1 — intra-module calls. THE exec-alias case.
// ---------------------------------------------------------------------------

/// `g(x) = 3x`, `f(x) = g(x) + 1`.
///
/// The one case this whole milestone exists for. `link_jit` patches the
/// callee's address into an `l32r` literal slot; on the S3 that literal must
/// hold the **I-bus** alias, not the D-bus address the linker wrote through.
/// Wrong ⇒ `callx8` to a non-fetchable address ⇒ `EXCCAUSE=2` on the first
/// call. Two separate functions so the call cannot be inlined away — an
/// inlined call does not test the fix.
fn build_intra_module_call() -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("g", &[IrType::I32]);
    let gx = fb.add_param(IrType::I32);
    let three = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: three,
        value: 3,
    });
    fb.push(LpirOp::Imul {
        dst: gx,
        lhs: gx,
        rhs: three,
    });
    fb.push_return(&[gx]);
    let g = fb.finish();

    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let x = fb.add_param(IrType::I32);
    let out = fb.alloc_vreg(IrType::I32);
    fb.push_call(CalleeRef::Local(FuncId(0)), &[VMCTX_VREG, x], &[out]);
    fb.push(LpirOp::IaddImm {
        dst: out,
        src: out,
        imm: 1,
    });
    fb.push_return(&[out]);
    let f = fb.finish();

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), g), (FuncId(1), f)]),
        },
        sig_of(vec![int_sig("g"), int_sig("f")]),
    )
}

// ---------------------------------------------------------------------------
// Risk 3 — call depth > 16, forcing REAL register-window overflow.
// ---------------------------------------------------------------------------

/// A chain `f0 -> f1 -> ... -> f19`, each adding 1, deepest returning `x`.
///
/// Xtensa's register file holds a limited number of live windows (the S3's
/// physical file is 64 registers ⇒ 16 windows of 4). A chain 20 deep therefore
/// forces genuine window overflow/underflow through the spill area on hardware
/// frames. The emulator models the window machinery; silicon is the proof.
///
/// Each frame takes exactly one argument, keeping the case clear of #194's
/// known spilling-around-calls cluster.
pub const CHAIN_DEPTH: u32 = 20;

fn build_deep_call_chain() -> (LpirModule, LpsModuleSig) {
    let mut functions = VecMap::new();
    let mut sigs = Vec::new();

    // Deepest frame first: FuncId(0) = `f19`, the base case.
    let mut fb = FunctionBuilder::new("f19", &[IrType::I32]);
    let x = fb.add_param(IrType::I32);
    fb.push_return(&[x]);
    functions.insert(FuncId(0), fb.finish());
    sigs.push(int_sig("f19"));

    // Then f18 .. f0, each calling the previously-built (deeper) one.
    for i in (0..CHAIN_DEPTH - 1).rev() {
        let name = level_name(i);
        let callee = FuncId((CHAIN_DEPTH - 2 - i) as u16);
        let mut fb = FunctionBuilder::new(&name, &[IrType::I32]);
        let x = fb.add_param(IrType::I32);
        let out = fb.alloc_vreg(IrType::I32);
        fb.push_call(CalleeRef::Local(callee), &[VMCTX_VREG, x], &[out]);
        fb.push(LpirOp::IaddImm {
            dst: out,
            src: out,
            imm: 1,
        });
        fb.push_return(&[out]);
        functions.insert(FuncId((CHAIN_DEPTH - 1 - i) as u16), fb.finish());
        sigs.push(int_sig(&name));
    }

    (
        LpirModule {
            imports: vec![],
            functions,
        },
        sig_of(sigs),
    )
}

/// `f0`, `f1`, ... as owned names (the builder takes `&str`).
fn level_name(i: u32) -> String {
    let mut s = String::from("f");
    // no_std-friendly u32 → decimal without `format!`'s machinery.
    if i >= 10 {
        s.push((b'0' + (i / 10) as u8) as char);
    }
    s.push((b'0' + (i % 10) as u8) as char);
    s
}

// ---------------------------------------------------------------------------
// Risk 4 — branches taken in both directions.
// ---------------------------------------------------------------------------

/// `f(x) = if x >= 10 { 2 } else { 1 }`, exercised both ways by two
/// invocations. A branch only ever taken one direction proves half of nothing.
fn build_branch_both_directions() -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let x = fb.add_param(IrType::I32);
    let ten = fb.alloc_vreg(IrType::I32);
    let cond = fb.alloc_vreg(IrType::I32);
    let out = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: ten,
        value: 10,
    });
    fb.push(LpirOp::IgeS {
        dst: cond,
        lhs: x,
        rhs: ten,
    });
    fb.push_if(cond);
    fb.push(LpirOp::IconstI32 { dst: out, value: 2 });
    fb.push_else();
    fb.push(LpirOp::IconstI32 { dst: out, value: 1 });
    fb.end_if();
    fb.push_return(&[out]);

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), fb.finish())]),
        },
        sig_of(vec![int_sig("f")]),
    )
}

// ---------------------------------------------------------------------------
// Risk 5 — immediate materialisation (the no-ANDI path).
// ---------------------------------------------------------------------------

/// `f(x) = x & 0x0F0F_1234`.
///
/// Xtensa has **no** `andi`/`ori`/`xori`, so any bitwise constant must go
/// through materialisation (literal pool or `movi` pair). A silently truncated
/// immediate — the hazard called out on `isa/xt/imm.rs` — gives a wrong answer
/// here rather than a crash, which is precisely why it needs a golden.
const AND_MASK: i32 = 0x0F0F_1234u32 as i32;

fn build_imm_materialize() -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let x = fb.add_param(IrType::I32);
    let m = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: m,
        value: AND_MASK,
    });
    fb.push(LpirOp::Iand {
        dst: x,
        lhs: x,
        rhs: m,
    });
    fb.push_return(&[x]);

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), fb.finish())]),
        },
        sig_of(vec![int_sig("f")]),
    )
}

// ---------------------------------------------------------------------------
// Risk 6 — sret (aggregate) returns.
// ---------------------------------------------------------------------------

/// `f(x) -> vec4 { x, x+1, x+2, x+3 }`.
///
/// Four return words take the **sret** path: the caller passes a hidden
/// destination pointer (`add_sret_param`, always before user params) which the
/// callee stores through, and `return_types` is empty because nothing comes
/// back in registers. Vec4 and not Vec2 on purpose — two words return in
/// `r0`/`r1` and never touch sret at all.
///
/// Two real defects live behind this shape:
///
/// - #194: `RegPool::new` ignored `FuncAbi::allocatable`, handing out `a2` —
///   which must hold the sret pointer for the whole function — as a data
///   register.
/// - This session: `rt_jit` derived its cached `ret_count` from
///   `return_types.len()`, which is 0 here, so `invoke_flat` sized the sret
///   buffer at one word while the callee wrote four.
///
/// Both are invisible to a shader that returns a scalar, and vec3/vec4 returns
/// are ubiquitous in real GLSL.
fn build_sret_vec4() -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[]);
    let dst = fb.add_sret_param();
    let x = fb.add_param(IrType::I32);
    let a = fb.alloc_vreg(IrType::I32);
    let b = fb.alloc_vreg(IrType::I32);
    let c = fb.alloc_vreg(IrType::I32);
    for (dst_v, imm) in [(a, 1), (b, 2), (c, 3)] {
        fb.push(LpirOp::IaddImm {
            dst: dst_v,
            src: x,
            imm,
        });
    }
    for (i, v) in [x, a, b, c].into_iter().enumerate() {
        fb.push(LpirOp::Store {
            base: dst,
            offset: (i * 4) as u32,
            value: v,
        });
    }
    fb.push_return(&[]);

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), fb.finish())]),
        },
        sig_of(vec![vec4_ret_sig("f")]),
    )
}

// ---------------------------------------------------------------------------
// Risk 7 — Q32 arithmetic.
// ---------------------------------------------------------------------------

/// `f(x) = (x * x) >> 16` — a Q16.16 square, done with the integer ops the
/// device path actually uses. The device is integer-only; float executors are
/// future `fw-emu-xt` work.
///
/// ⚠️ Valid only for |x| < 1.0. This is a 32-bit multiply, so the intermediate
/// `x * x` overflows for x >= 1.0 (2.0 in Q16.16 is 131072; squared that is
/// 2^34, which wraps to 0). A general Q16.16 multiply needs a 64-bit
/// intermediate — that is what the `__lps_*_q32` builtins are for. The
/// invocations below stay under 1.0 deliberately: the point is exercising the
/// emitter's multiply and arithmetic-shift, not re-implementing fixed-point
/// math badly.
fn build_q32_square() -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let x = fb.add_param(IrType::I32);
    let sh = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::Imul {
        dst: x,
        lhs: x,
        rhs: x,
    });
    fb.push(LpirOp::IconstI32 { dst: sh, value: 16 });
    fb.push(LpirOp::IshrS {
        dst: x,
        lhs: x,
        rhs: sh,
    });
    fb.push_return(&[x]);

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), fb.finish())]),
        },
        sig_of(vec![int_sig("f")]),
    )
}

// ---------------------------------------------------------------------------
// Risk 2 — builtin calls through the windowed ABI (`callx8`).
// ---------------------------------------------------------------------------

/// `f(x) = __lps_sin_q32(x)`.
///
/// The builtin is an *import*, so its address is resolved at link time — on
/// device from `lps_builtins::jit_builtin_code_ptr` via `BuiltinTable`, on the
/// host from the cross-compiled Xtensa builtins image. That makes it the one
/// case whose golden cannot be hand-derived from LPIR semantics; it comes from
/// the emulator, which `tests/xt_builtins_image.rs` separately validates
/// against the host build of the same `lps-builtins` source.
fn build_builtin_sin() -> (LpirModule, LpsModuleSig) {
    let import = ImportDecl {
        module_name: String::from("lps"),
        func_name: BuiltinId::LpGlslSinQ32.name().to_string(),
        param_types: vec![IrType::I32],
        return_types: vec![IrType::I32],
        lpfn_glsl_params: None,
        // Pure math: no VM context, no aggregate return.
        needs_vmctx: false,
        sret: false,
    };

    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let x = fb.add_param(IrType::I32);
    let out = fb.alloc_vreg(IrType::I32);
    fb.push_call(CalleeRef::Import(lpir::ImportId(0)), &[x], &[out]);
    fb.push_return(&[out]);

    (
        LpirModule {
            imports: vec![import],
            functions: VecMap::from([(FuncId(0), fb.finish())]),
        },
        sig_of(vec![int_sig("f")]),
    )
}

/// Q16.16 one — `sin(1.0)` is the builtin invocation below.
const Q16_ONE: i32 = 1 << 16;

/// The corpus. One entry per risk-surface item in M1's milestone file.
pub const CASES: &[XtCase] = &[
    XtCase {
        name: "intra_module_call",
        risk: "1: intra-module call (exec-alias fix)",
        entry: "f",
        invocations: &[
            XtInvocation {
                args: &[10],
                golden: &[31],
            },
            XtInvocation {
                args: &[-4],
                golden: &[-11],
            },
        ],
        build: build_intra_module_call,
        needs_builtins: false,
    },
    XtCase {
        name: "builtin_call_sin_q32",
        risk: "2: builtin call via callx8",
        entry: "f",
        invocations: &[XtInvocation {
            args: &[Q16_ONE],
            // sin(1.0) in Q16.16 **as `__lps_sin_q32` computes it**, taken from
            // the HOST build of lps-builtins: 55192.
            //
            // Deliberately NOT `sin(1.0) * 65536` = 55146.6 — that was the
            // first value here and it was wrong. The oracle for a builtin is
            // the builtin's own fixed-point implementation, not real-number
            // math; the 45-unit gap is its approximation error, and using math
            // as the oracle would have reported a phantom emitter bug.
            golden: &[55192],
        }],
        build: build_builtin_sin,
        needs_builtins: true,
    },
    XtCase {
        name: "deep_call_chain_20",
        risk: "3: call depth > 16 (real window overflow)",
        entry: "f0",
        invocations: &[XtInvocation {
            args: &[100],
            // 19 frames each add 1 on the way back out.
            golden: &[119],
        }],
        build: build_deep_call_chain,
        needs_builtins: false,
    },
    XtCase {
        name: "branch_both_directions",
        risk: "4: branches taken both ways",
        entry: "f",
        invocations: &[
            XtInvocation {
                args: &[5],
                golden: &[1],
            },
            XtInvocation {
                args: &[20],
                golden: &[2],
            },
            XtInvocation {
                args: &[10],
                golden: &[2],
            },
        ],
        build: build_branch_both_directions,
        needs_builtins: false,
    },
    XtCase {
        name: "imm_materialize_and",
        risk: "5: immediate materialisation (no ANDI)",
        entry: "f",
        invocations: &[XtInvocation {
            args: &[0x1234_5678],
            // 0x12345678 & 0x0F0F1234, byte-wise:
            // 0x12&0x0F=0x02, 0x34&0x0F=0x04, 0x56&0x12=0x12, 0x78&0x34=0x30
            golden: &[0x0204_1230],
        }],
        build: build_imm_materialize,
        needs_builtins: false,
    },
    XtCase {
        name: "sret_vec4_return",
        risk: "6: sret (aggregate) return",
        entry: "f",
        invocations: &[
            XtInvocation {
                args: &[7],
                golden: &[7, 8, 9, 10],
            },
            XtInvocation {
                args: &[-1],
                golden: &[-1, 0, 1, 2],
            },
        ],
        build: build_sret_vec4,
        needs_builtins: false,
    },
    XtCase {
        name: "q32_square",
        risk: "7: Q32 arithmetic",
        entry: "f",
        invocations: &[
            XtInvocation {
                // 0.5 squared = 0.25
                args: &[Q16_ONE / 2],
                golden: &[Q16_ONE / 4],
            },
            XtInvocation {
                // 0.25 squared = 0.0625
                args: &[Q16_ONE / 4],
                golden: &[Q16_ONE / 16],
            },
            XtInvocation {
                // Negative input exercises the ARITHMETIC shift: -0.5 squared
                // is +0.25, and a logical shift would give a wildly wrong
                // positive value here.
                args: &[-(Q16_ONE / 2)],
                golden: &[Q16_ONE / 4],
            },
        ],
        build: build_q32_square,
        needs_builtins: false,
    },
];

// ===========================================================================
// The native-f32 half (M7 P5)
// ===========================================================================
//
// Same contract as the Q32 half above — committed goldens, confirmed on
// `lp-xt-emu` before anything is flashed, never regenerated from device output
// — with one difference that has to be structural rather than remembered: a
// word means something else here.
//
// In `FloatMode::F32` a word **is** an IEEE-754 bit pattern (M7 D1: floats
// cross every call boundary in address registers), so `XtF32Case` carries
// `u32`s and is invoked through `call_f32_words`, not `call_q32`. Reusing
// `XtCase` would have made `1.0f32` and `16257.0` in Q16.16 the same table
// entry, and a mode mix-up would surface as a wrong pixel instead of an error.
// The runtimes draw the same line: both `call_q32` and `call_f32_words` refuse
// the other mode outright.
//
// The goldens below are exact in binary32 by construction — every value is a
// dyadic rational and every intermediate is representable — so they are what
// the arithmetic *means*, not what any implementation rounded to. The two
// exceptions are marked where they occur: the builtin case (whose oracle is
// the builtin's own implementation, exactly as `builtin_call_sin_q32`'s is)
// and the infinity row (an IEEE Guaranteed row from `docs/design/float.md`).

/// One f32-mode call: argument bit patterns in, expected result words out.
#[cfg(feature = "float-f32")]
pub struct XtF32Invocation {
    /// Arguments *excluding* the vmctx word — the engine prepends that. Raw
    /// IEEE-754 bit patterns, one word per scalar component.
    pub args: &'static [u32],
    /// Expected return words. Committed; never regenerated from a device. For
    /// a float return these are bit patterns; for an int return, the integer.
    pub golden: &'static [u32],
}

/// A named f32-mode corpus case.
#[cfg(feature = "float-f32")]
pub struct XtF32Case {
    pub name: &'static str,
    /// Which f32 risk this covers, for the transcript.
    pub risk: &'static str,
    /// Entry function name inside the built module.
    pub entry: &'static str,
    pub invocations: &'static [XtF32Invocation],
    /// Builds the module. A fn pointer so the table stays `const` in `no_std`.
    pub build: fn() -> (LpirModule, LpsModuleSig),
    /// True when the case reaches an `__lp_lpir_*_f32` builtin, so it needs the
    /// Xtensa builtins image built with `float-f32` on the host side.
    pub needs_builtins: bool,
}

/// Signature helper: `name(a: float, b: float) -> float`.
#[cfg(feature = "float-f32")]
fn float2_sig(name: &str, ret: LpsType) -> LpsFnSig {
    LpsFnSig {
        name: name.to_string(),
        parameters: vec![
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
        return_type: ret,
        kind: LpsFnKind::UserDefined,
    }
}

/// Signature helper: `name(x: float) -> float`.
#[cfg(feature = "float-f32")]
fn float1_sig(name: &str) -> LpsFnSig {
    LpsFnSig {
        name: name.to_string(),
        parameters: vec![FnParam {
            name: String::from("x"),
            ty: LpsType::Float,
            qualifier: ParamQualifier::In,
        }],
        return_type: LpsType::Float,
        kind: LpsFnKind::UserDefined,
    }
}

/// Signature helper: `name(n: int, a: float, b: float) -> float`.
#[cfg(feature = "float-f32")]
fn recursion_sig(name: &str) -> LpsFnSig {
    LpsFnSig {
        name: name.to_string(),
        parameters: vec![
            FnParam {
                name: String::from("n"),
                ty: LpsType::Int,
                qualifier: ParamQualifier::In,
            },
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
        return_type: LpsType::Float,
        kind: LpsFnKind::UserDefined,
    }
}

// ---------------------------------------------------------------------------
// F1 — inline arithmetic and the wfr/rfr boundary.
// ---------------------------------------------------------------------------

/// `f(a, b) = ((a * b + a) - b) * 0.25`.
///
/// The inline family in one expression: `mul.s`, `add.s`, `sub.s`, and a float
/// *constant*, which on Xtensa is `IConst32` + `wfr` (M7 D11 — there is no
/// float literal pool). Both operands arrive in address registers and the
/// result leaves in one, so every `wfr`/`rfr` seam in the ABI is on the path;
/// a half-applied convention reads an FR nobody wrote and the answer is
/// garbage rather than a fault.
#[cfg(feature = "float-f32")]
fn build_f32_arith() -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::F32]);
    let a = fb.add_param(IrType::F32);
    let b = fb.add_param(IrType::F32);
    let t = fb.alloc_vreg(IrType::F32);
    let k = fb.alloc_vreg(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    fb.push(LpirOp::Fmul {
        dst: t,
        lhs: a,
        rhs: b,
    });
    fb.push(LpirOp::Fadd {
        dst: t,
        lhs: t,
        rhs: a,
    });
    fb.push(LpirOp::Fsub {
        dst: t,
        lhs: t,
        rhs: b,
    });
    fb.push(LpirOp::FconstF32 {
        dst: k,
        value: 0.25,
    });
    fb.push(LpirOp::Fmul {
        dst: out,
        lhs: t,
        rhs: k,
    });
    fb.push_return(&[out]);

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), fb.finish())]),
        },
        sig_of(vec![float2_sig("f", LpsType::Float)]),
    )
}

// ---------------------------------------------------------------------------
// F2 — all six compares, in one word, including the NaN row.
// ---------------------------------------------------------------------------

/// `f(a, b) -> int`, a bitmask of the six ordered/unordered compares:
/// `lt<<0 | le<<1 | gt<<2 | ge<<3 | eq<<4 | ne<<5`.
///
/// One entry point rather than six cases, because the interesting failure is a
/// *mapping* error rather than an arithmetic one and a mask makes it legible in
/// a serial transcript: `35` and `44` differ visibly, whereas six separate
/// PASS lines with `0`/`1` do not say which predicate moved.
///
/// Xtensa has three FP compare instructions (`oeq.s`, `olt.s`, `ole.s`) writing
/// a boolean register, so `gt`/`ge`/`ne` are built by swapping operands or
/// inverting — and M7 D5 fuses the compare into the consumer with `b0` as
/// implicit scratch. The NaN invocation is the row that caught D5's first
/// mapping table: it tabulated `ueq.s` + `movf` for `!=`, which computes
/// "ordered and unequal" and answers *false* on NaN, where `float.md` §3 makes
/// `!=` a Guaranteed *true*.
#[cfg(feature = "float-f32")]
fn build_f32_compare_mask() -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let a = fb.add_param(IrType::F32);
    let b = fb.add_param(IrType::F32);
    let acc = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: acc, value: 0 });

    let preds: [fn(lpir::VReg, lpir::VReg, lpir::VReg) -> LpirOp; 6] = [
        |dst, lhs, rhs| LpirOp::Flt { dst, lhs, rhs },
        |dst, lhs, rhs| LpirOp::Fle { dst, lhs, rhs },
        |dst, lhs, rhs| LpirOp::Fgt { dst, lhs, rhs },
        |dst, lhs, rhs| LpirOp::Fge { dst, lhs, rhs },
        |dst, lhs, rhs| LpirOp::Feq { dst, lhs, rhs },
        |dst, lhs, rhs| LpirOp::Fne { dst, lhs, rhs },
    ];
    for (i, make) in preds.into_iter().enumerate() {
        let bit = fb.alloc_vreg(IrType::I32);
        let sh = fb.alloc_vreg(IrType::I32);
        let shifted = fb.alloc_vreg(IrType::I32);
        fb.push(make(bit, a, b));
        fb.push(LpirOp::IconstI32 {
            dst: sh,
            value: i as i32,
        });
        fb.push(LpirOp::Ishl {
            dst: shifted,
            lhs: bit,
            rhs: sh,
        });
        fb.push(LpirOp::Ior {
            dst: acc,
            lhs: acc,
            rhs: shifted,
        });
    }
    fb.push_return(&[acc]);

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), fb.finish())]),
        },
        sig_of(vec![float2_sig("f", LpsType::Int)]),
    )
}

// ---------------------------------------------------------------------------
// F3 — floats across a real `callx8`.
// ---------------------------------------------------------------------------

/// `g(a, b) = a * b + a`; `f(a, b) = g(a, b) - b`.
///
/// The f32 sibling of `intra_module_call`, and the case that makes M7 D1/D2
/// observable on silicon: floats live in FRs *inside* a function but travel
/// between functions in **address** registers as raw bit patterns, because the
/// esp toolchain — which compiled M5's builtins — passes them in `a2..a7`. Two
/// separate functions so the call cannot be inlined away.
#[cfg(feature = "float-f32")]
fn build_f32_call_boundary() -> (LpirModule, LpsModuleSig) {
    let mut cb = FunctionBuilder::new("g", &[IrType::F32]);
    let ga = cb.add_param(IrType::F32);
    let gb = cb.add_param(IrType::F32);
    let prod = cb.alloc_vreg(IrType::F32);
    let gout = cb.alloc_vreg(IrType::F32);
    cb.push(LpirOp::Fmul {
        dst: prod,
        lhs: ga,
        rhs: gb,
    });
    cb.push(LpirOp::Fadd {
        dst: gout,
        lhs: prod,
        rhs: ga,
    });
    cb.push_return(&[gout]);
    let g = cb.finish();

    let mut fb = FunctionBuilder::new("f", &[IrType::F32]);
    let a = fb.add_param(IrType::F32);
    let b = fb.add_param(IrType::F32);
    let called = fb.alloc_vreg(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    fb.push_call(CalleeRef::Local(FuncId(0)), &[VMCTX_VREG, a, b], &[called]);
    fb.push(LpirOp::Fsub {
        dst: out,
        lhs: called,
        rhs: b,
    });
    fb.push_return(&[out]);
    let f = fb.finish();

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), g), (FuncId(1), f)]),
        },
        sig_of(vec![
            float2_sig("g", LpsType::Float),
            float2_sig("f", LpsType::Float),
        ]),
    )
}

// ---------------------------------------------------------------------------
// F4 — a builtin call returning a float.
// ---------------------------------------------------------------------------

/// `f(x) = floor(x)`.
///
/// `Ffloor` is not inlinable, so M7 D4 routes it to `__lp_lpir_ffloor_f32`
/// through a `sym_call` — resolved on device from
/// `lps_builtins::jit_builtin_code_ptr` via `BuiltinTable`, and on the host
/// from the cross-compiled Xtensa builtins image. It is written as the plain
/// LPIR op rather than a hand-written import precisely so the *lowering*
/// decision is on the path and not just the call.
///
/// `ffloor` over `fdiv`/`fsqrt` on purpose: it reaches no `div0.s`/`sqrt0.s`
/// estimate helper, so it does not additionally depend on M6-P6's
/// implementation-defined lookup ROMs. Same choice, same reason, as
/// `xt_pipeline_f32.rs::a_builtin_routed_float_op_resolves_and_runs`.
///
/// This is the one f32 case whose golden is not derivable from LPIR semantics
/// alone — the oracle for a builtin is the builtin's own implementation. The
/// values chosen are ones where that cannot diverge from the real-number
/// answer: exactly-representable inputs whose floor is exact.
#[cfg(feature = "float-f32")]
fn build_f32_builtin_floor() -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::F32]);
    let x = fb.add_param(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    fb.push(LpirOp::Ffloor { dst: out, src: x });
    fb.push_return(&[out]);

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), fb.finish())]),
        },
        sig_of(vec![float1_sig("f")]),
    )
}

// ---------------------------------------------------------------------------
// F5 — register pressure past the float pool.
// ---------------------------------------------------------------------------

/// `f(x) = sum(x + 1 .. x + 24)`, with all 24 intermediates live at once.
///
/// 15 of the 16 FRs are allocatable (`f15` is the emitter's scratch — M7 D8,
/// corrected during P3 because a *spilled def* still needs a register to write
/// to first), so 24 simultaneously-live floats force real spills and reloads
/// through `ssi`/`lsi`. Those encodings reach 1020 bytes and `lp-xt-inst`'s
/// encoder **truncates rather than failing**, which is a wrong answer and not a
/// crash — hence a golden.
///
/// Summed in reverse so every value stays live until it is used; a forward sum
/// would let the allocator retire each one immediately and the pool would never
/// saturate.
#[cfg(feature = "float-f32")]
const F32_PRESSURE_N: u32 = 24;

#[cfg(feature = "float-f32")]
fn build_f32_spill_pressure() -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::F32]);
    let x = fb.add_param(IrType::F32);
    let one = fb.alloc_vreg(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    fb.push(LpirOp::FconstF32 {
        dst: one,
        value: 1.0,
    });

    let mut vs = Vec::new();
    let mut prev = x;
    for _ in 0..F32_PRESSURE_N {
        let v = fb.alloc_vreg(IrType::F32);
        fb.push(LpirOp::Fadd {
            dst: v,
            lhs: prev,
            rhs: one,
        });
        vs.push(v);
        prev = v;
    }
    fb.push(LpirOp::FconstF32 {
        dst: out,
        value: 0.0,
    });
    for &v in vs.iter().rev() {
        fb.push(LpirOp::Fadd {
            dst: out,
            lhs: out,
            rhs: v,
        });
    }
    fb.push_return(&[out]);

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), fb.finish())]),
        },
        sig_of(vec![float1_sig("f")]),
    )
}

// ---------------------------------------------------------------------------
// F6 — live floats across real register-window overflow.
// ---------------------------------------------------------------------------

/// `rec(n, a, b) = if n == 0 { a + b } else { rec(n-1, a, b) + a*2 + b*3 }`,
/// entered at depth 20.
///
/// **The milestone's headline hazard, on silicon.** M7 D7 is a deliberate
/// *non*-change: no FR is callee-saved (measured, M6-P4), so there is no FP
/// callee-save region, float spills sit at the frame's *bottom*, and the
/// window-overflow handler writes the 32-byte reservation at the *top*. The
/// claim is that those cannot collide — and its failure mode is the worst kind,
/// silent corruption of an ancestor frame that surfaces long after the return.
///
/// Depth 20 is past the S3's 16-window ring, so the overflow/underflow handlers
/// really run; `a` and `b` are read *after* the recursive call returns, which
/// is what makes them live across it. `xt_pipeline_f32.rs` runs the same shape
/// at depth 100 on the emulator, where a serial session is not the budget.
///
/// `deep_call_chain_20` in the Q32 half is the integer control (M6-P2's
/// precedent): if both fail the finding is the window machinery, if only this
/// one fails the finding is floats specifically.
#[cfg(feature = "float-f32")]
pub const F32_RECURSION_DEPTH: i32 = 20;

#[cfg(feature = "float-f32")]
fn build_f32_recursion() -> (LpirModule, LpsModuleSig) {
    let mut cb = FunctionBuilder::new("rec", &[IrType::F32]);
    let n = cb.add_param(IrType::I32);
    let a = cb.add_param(IrType::F32);
    let b = cb.add_param(IrType::F32);
    let zero = cb.alloc_vreg(IrType::I32);
    let is_zero = cb.alloc_vreg(IrType::I32);
    let out = cb.alloc_vreg(IrType::F32);
    cb.push(LpirOp::IconstI32 {
        dst: zero,
        value: 0,
    });
    cb.push(LpirOp::Ieq {
        dst: is_zero,
        lhs: n,
        rhs: zero,
    });
    cb.push_if(is_zero);
    cb.push(LpirOp::Fadd {
        dst: out,
        lhs: a,
        rhs: b,
    });
    cb.push_else();
    {
        let n1 = cb.alloc_vreg(IrType::I32);
        cb.push(LpirOp::IaddImm {
            dst: n1,
            src: n,
            imm: -1,
        });
        let deeper = cb.alloc_vreg(IrType::F32);
        cb.push_call(
            CalleeRef::Local(FuncId(0)),
            &[VMCTX_VREG, n1, a, b],
            &[deeper],
        );
        let two = cb.alloc_vreg(IrType::F32);
        let three = cb.alloc_vreg(IrType::F32);
        let ta = cb.alloc_vreg(IrType::F32);
        let tb = cb.alloc_vreg(IrType::F32);
        let sum = cb.alloc_vreg(IrType::F32);
        cb.push(LpirOp::FconstF32 {
            dst: two,
            value: 2.0,
        });
        cb.push(LpirOp::FconstF32 {
            dst: three,
            value: 3.0,
        });
        cb.push(LpirOp::Fmul {
            dst: ta,
            lhs: a,
            rhs: two,
        });
        cb.push(LpirOp::Fmul {
            dst: tb,
            lhs: b,
            rhs: three,
        });
        cb.push(LpirOp::Fadd {
            dst: sum,
            lhs: ta,
            rhs: tb,
        });
        cb.push(LpirOp::Fadd {
            dst: out,
            lhs: deeper,
            rhs: sum,
        });
    }
    cb.end_if();
    cb.push_return(&[out]);
    let rec = cb.finish();

    let mut fb = FunctionBuilder::new("f", &[IrType::F32]);
    let fn_ = fb.add_param(IrType::I32);
    let fa = fb.add_param(IrType::F32);
    let fb_ = fb.add_param(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    fb.push_call(
        CalleeRef::Local(FuncId(0)),
        &[VMCTX_VREG, fn_, fa, fb_],
        &[out],
    );
    fb.push_return(&[out]);
    let f = fb.finish();

    (
        LpirModule {
            imports: vec![],
            functions: VecMap::from([(FuncId(0), rec), (FuncId(1), f)]),
        },
        sig_of(vec![recursion_sig("rec"), recursion_sig("f")]),
    )
}

// ---------------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------------

/// The f32 corpus. Separate from [`CASES`] because the *word* differs, not just
/// the arithmetic — see this section's header.
#[cfg(feature = "float-f32")]
pub const F32_CASES: &[XtF32Case] = &[
    XtF32Case {
        name: "f32_arith_and_const",
        risk: "F1: inline add/sub/mul and the wfr/rfr boundary",
        entry: "f",
        invocations: &[
            XtF32Invocation {
                // ((3 * 0.5 + 3) - 0.5) * 0.25 = 4.0 * 0.25 = 1.0
                args: &[0x4040_0000, 0x3F00_0000],
                golden: &[0x3F80_0000],
            },
            XtF32Invocation {
                // ((-2 * 4 + -2) - 4) * 0.25 = -3.5
                args: &[0xC000_0000, 0x4080_0000],
                golden: &[0xC060_0000],
            },
            XtF32Invocation {
                // inf * 1 + inf - 1, scaled: still inf. float.md §3
                // Guaranteed — infinity arithmetic is IEEE at RNE everywhere.
                args: &[0x7F80_0000, 0x3F80_0000],
                golden: &[0x7F80_0000],
            },
        ],
        build: build_f32_arith,
        needs_builtins: false,
    },
    XtF32Case {
        name: "f32_compare_mask",
        risk: "F2: all six float compares, incl. the NaN row",
        entry: "f",
        invocations: &[
            XtF32Invocation {
                // 1.0 vs 2.0: lt, le, ne  => 1 | 2 | 32 = 35
                args: &[0x3F80_0000, 0x4000_0000],
                golden: &[35],
            },
            XtF32Invocation {
                // 2.0 vs 1.0: gt, ge, ne  => 4 | 8 | 32 = 44
                args: &[0x4000_0000, 0x3F80_0000],
                golden: &[44],
            },
            XtF32Invocation {
                // 1.0 vs 1.0: le, ge, eq  => 2 | 8 | 16 = 26
                args: &[0x3F80_0000, 0x3F80_0000],
                golden: &[26],
            },
            XtF32Invocation {
                // NaN vs 1.0: every ordered compare false, `!=` true => 32.
                // float.md §3 Guaranteed. A quiet NaN, not a signalling one.
                args: &[0x7FC0_0000, 0x3F80_0000],
                golden: &[32],
            },
        ],
        build: build_f32_compare_mask,
        needs_builtins: false,
    },
    XtF32Case {
        name: "f32_call_boundary",
        risk: "F3: floats across callx8 in address registers (D1/D2)",
        entry: "f",
        invocations: &[
            XtF32Invocation {
                // g(3, 0.5) = 1.5 + 3 = 4.5; f = 4.5 - 0.5 = 4.0
                args: &[0x4040_0000, 0x3F00_0000],
                golden: &[0x4080_0000],
            },
            XtF32Invocation {
                // g(-1.5, 2) = -3 + -1.5 = -4.5; f = -4.5 - 2 = -6.5
                args: &[0xBFC0_0000, 0x4000_0000],
                golden: &[0xC0D0_0000],
            },
        ],
        build: build_f32_call_boundary,
        needs_builtins: false,
    },
    XtF32Case {
        name: "f32_builtin_floor",
        risk: "F4: builtin call returning a float, via sym_call (D4)",
        entry: "f",
        invocations: &[
            XtF32Invocation {
                // floor(3.75) = 3.0
                args: &[0x4070_0000],
                golden: &[0x4040_0000],
            },
            XtF32Invocation {
                // floor(-2.5) = -3.0 — floor, not trunc. The two disagree only
                // on negative non-integers, so a trunc-shaped implementation
                // passes every positive case and fails exactly here.
                args: &[0xC020_0000],
                golden: &[0xC040_0000],
            },
        ],
        build: build_f32_builtin_floor,
        needs_builtins: true,
    },
    XtF32Case {
        name: "f32_spill_pressure_24",
        risk: "F5: register pressure past the 15-FR pool (ssi/lsi spills)",
        entry: "f",
        invocations: &[
            XtF32Invocation {
                // sum(1..=24) = 300
                args: &[0x0000_0000],
                golden: &[0x4396_0000],
            },
            XtF32Invocation {
                // x = 0.5 shifts every term: 300 + 24*0.5 = 312
                args: &[0x3F00_0000],
                golden: &[0x439C_0000],
            },
        ],
        build: build_f32_spill_pressure,
        needs_builtins: false,
    },
    XtF32Case {
        name: "f32_recursion_depth_20",
        risk: "F6: live floats across real window overflow (D7)",
        entry: "f",
        invocations: &[XtF32Invocation {
            // rec(20, 1.25, 2.5) = 1.25 + 2.5 + 20 * (2.5 + 7.5) = 203.75
            args: &[20, 0x3FA0_0000, 0x4020_0000],
            golden: &[0x434B_C000],
        }],
        build: build_f32_recursion,
        needs_builtins: false,
    },
];
