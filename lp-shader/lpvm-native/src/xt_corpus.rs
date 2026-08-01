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
