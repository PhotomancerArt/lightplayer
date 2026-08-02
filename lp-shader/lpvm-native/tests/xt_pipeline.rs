//! Full-pipeline tests for the Xtensa backend: LPIR → lower → regalloc →
//! `isa/xt` emit → manual link at the emulator's code base → windowed
//! execution on `lp-xt-emu` (the silicon-verified oracle).
//!
//! This is the M3 gate rig: every value asserted here went through the real
//! compiler pipeline with `IsaTarget::Xtensa`, not a hand-built VInst stream.

use lp_collection::VecMap;
use lpir::builder::FunctionBuilder;
use lpir::{FloatMode, FuncId, IrType, LpirModule, LpirOp};
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
use lpvm_native::compile::compile_module;
use lpvm_native::isa::IsaTarget;
use lpvm_native::native_options::NativeCompileOptions;

use lp_xt_emu::{Emulator, RunOutcome};

/// Compile `(ir, sig)` for Xtensa, link all functions at the emulator's
/// I-bus code base (patching literal-slot call relocations), and run
/// `entry_name` with `args` (arg 0 is the vmctx word — pass 0 when the
/// module needs none). Returns the emulator outcome.
fn compile_link_run(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    entry_name: &str,
    args: &[u32],
    fuel: bool,
) -> RunOutcome {
    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        fuel,
        ..Default::default()
    };
    let module = compile_module(ir, sig, FloatMode::Q32, opts, IsaTarget::Xtensa)
        .expect("xt compile should succeed");

    // Manual link: concatenate, then patch each literal-slot reloc with the
    // callee's absolute EXECUTE address at the emulator's I-bus base.
    let mut code = Vec::new();
    let mut entries = VecMap::<String, usize>::new();
    let mut func_offsets = Vec::new();
    for f in &module.functions {
        func_offsets.push(code.len());
        entries.insert(f.name.clone(), code.len());
        code.extend_from_slice(&f.code);
    }
    let mut emu = Emulator::new();
    let ibus_base = emu.profile.code_ibus_base();
    for (fi, f) in module.functions.iter().enumerate() {
        for reloc in &f.relocs {
            assert_eq!(
                reloc.r_type,
                IsaTarget::Xtensa.call_reloc_type(),
                "unexpected reloc type"
            );
            let target_off = *entries
                .get(&reloc.symbol)
                .unwrap_or_else(|| panic!("unresolved symbol {}", reloc.symbol));
            let target = ibus_base + target_off as u32;
            let slot = func_offsets[fi] + reloc.offset;
            code[slot..slot + 4].copy_from_slice(&target.to_le_bytes());
        }
    }

    let entry_off = *entries.get(entry_name).expect("entry function exists") as u32;
    emu.run_with_args(&code, entry_off, args)
}

fn expect_ok(out: RunOutcome) -> u32 {
    match out {
        RunOutcome::Ok(v) => v,
        RunOutcome::Trap(t) => panic!("unexpected trap: {t:?}"),
    }
}

/// One i32 → i32 function named `f`, body built by `build`.
fn unary_module(
    build: impl FnOnce(&mut FunctionBuilder, lpir::VReg),
) -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let x = fb.add_param(IrType::I32);
    build(&mut fb, x);
    let func = fb.finish();
    let module = LpirModule {
        imports: vec![],
        functions: VecMap::from([(FuncId(0), func)]),
    };
    let sig = LpsModuleSig {
        functions: vec![LpsFnSig {
            name: "f".to_string(),
            parameters: vec![FnParam {
                name: "x".to_string(),
                ty: LpsType::Int,
                qualifier: ParamQualifier::In,
            }],
            return_type: LpsType::Int,
            kind: LpsFnKind::UserDefined,
        }],
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    };
    (module, sig)
}

/// Run the unary function on the pipeline: args = [vmctx=0, x].
fn run_unary(build: impl FnOnce(&mut FunctionBuilder, lpir::VReg), x: i32) -> i32 {
    let (ir, sig) = unary_module(build);
    expect_ok(compile_link_run(&ir, &sig, "f", &[0, x as u32], false)) as i32
}

/// One `(i32, i32) -> i32` function named `f`, body built by `build`.
///
/// Both operands arrive as **runtime** arguments, which is the point: a
/// constant divisor could be folded, and the divide-by-zero guard has to hold
/// for divisors the compiler cannot see.
fn binary_module(
    build: impl FnOnce(&mut FunctionBuilder, lpir::VReg, lpir::VReg),
) -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let a = fb.add_param(IrType::I32);
    let b = fb.add_param(IrType::I32);
    build(&mut fb, a, b);
    let func = fb.finish();
    let module = LpirModule {
        imports: vec![],
        functions: VecMap::from([(FuncId(0), func)]),
    };
    let sig = LpsModuleSig {
        functions: vec![LpsFnSig {
            name: "f".to_string(),
            parameters: vec![
                FnParam {
                    name: "a".to_string(),
                    ty: LpsType::Int,
                    qualifier: ParamQualifier::In,
                },
                FnParam {
                    name: "b".to_string(),
                    ty: LpsType::Int,
                    qualifier: ParamQualifier::In,
                },
            ],
            return_type: LpsType::Int,
            kind: LpsFnKind::UserDefined,
        }],
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    };
    (module, sig)
}

/// Run the binary function on the pipeline: args = [vmctx=0, a, b].
fn run_binary(
    build: impl FnOnce(&mut FunctionBuilder, lpir::VReg, lpir::VReg),
    a: i32,
    b: i32,
) -> u32 {
    let (ir, sig) = binary_module(build);
    expect_ok(compile_link_run(
        &ir,
        &sig,
        "f",
        &[0, a as u32, b as u32],
        false,
    ))
}

/// `a <op> b` for one of the four LPIR integer divide/remainder ops, with both
/// operands opaque to the compiler.
fn run_div_op(make: fn(lpir::VReg, lpir::VReg, lpir::VReg) -> LpirOp, a: i32, b: i32) -> u32 {
    run_binary(
        |fb, x, y| {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(make(d, x, y));
            fb.push_return(&[d]);
        },
        a,
        b,
    )
}

fn idiv_s(dst: lpir::VReg, lhs: lpir::VReg, rhs: lpir::VReg) -> LpirOp {
    LpirOp::IdivS { dst, lhs, rhs }
}
fn idiv_u(dst: lpir::VReg, lhs: lpir::VReg, rhs: lpir::VReg) -> LpirOp {
    LpirOp::IdivU { dst, lhs, rhs }
}
fn irem_s(dst: lpir::VReg, lhs: lpir::VReg, rhs: lpir::VReg) -> LpirOp {
    LpirOp::IremS { dst, lhs, rhs }
}
fn irem_u(dst: lpir::VReg, lhs: lpir::VReg, rhs: lpir::VReg) -> LpirOp {
    LpirOp::IremU { dst, lhs, rhs }
}

#[test]
fn returns_a_constant_through_entry_retw() {
    let r = run_unary(
        |fb, x| {
            let c = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 { dst: c, value: 42 });
            fb.push_return(&[c]);
            let _ = x;
        },
        0,
    );
    assert_eq!(r, 42);
}

#[test]
fn addi_in_range_uses_the_imm_form_correctly() {
    let r = run_unary(
        |fb, x| {
            fb.push(LpirOp::IaddImm {
                dst: x,
                src: x,
                imm: 100,
            });
            fb.push_return(&[x]);
        },
        7,
    );
    assert_eq!(r, 107);
}

#[test]
fn addi_beyond_imm8_materializes() {
    // 70_000 fits neither addi(-128..=127) nor movi(-2048..=2047): the
    // constant must reach a literal pool + add path.
    let r = run_unary(
        |fb, x| {
            fb.push(LpirOp::IaddImm {
                dst: x,
                src: x,
                imm: 70_000,
            });
            fb.push_return(&[x]);
        },
        100,
    );
    assert_eq!(r, 70_100);
}

#[test]
fn bitwise_imm_has_no_imm_form_and_must_materialize() {
    // The key Xtensa fact: no andi/ori/xori. Any bitwise constant goes
    // through materialization; a silently-truncated immediate would give a
    // wrong answer here.
    let r = run_unary(
        |fb, x| {
            let m = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 {
                dst: m,
                value: 0x0F0F_1234u32 as i32,
            });
            fb.push(LpirOp::Iand {
                dst: x,
                lhs: x,
                rhs: m,
            });
            fb.push_return(&[x]);
        },
        0x1234_5678,
    );
    assert_eq!(r, 0x1234_5678 & 0x0F0F_1234);
}

#[test]
fn mul_div_rem_use_the_mul32_div32_options() {
    let r = run_unary(
        |fb, x| {
            let c = fb.alloc_vreg(IrType::I32);
            let m = fb.alloc_vreg(IrType::I32);
            let d = fb.alloc_vreg(IrType::I32);
            let rem = fb.alloc_vreg(IrType::I32);
            let s = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 { dst: c, value: 7 });
            fb.push(LpirOp::Imul {
                dst: m,
                lhs: x,
                rhs: x,
            }); // x*x
            fb.push(LpirOp::IdivS {
                dst: d,
                lhs: m,
                rhs: c,
            }); // (x*x)/7
            fb.push(LpirOp::IremS {
                dst: rem,
                lhs: m,
                rhs: c,
            }); // (x*x)%7
            fb.push(LpirOp::Iadd {
                dst: s,
                lhs: d,
                rhs: rem,
            });
            fb.push_return(&[s]);
        },
        123,
    );
    let m = 123i32 * 123;
    assert_eq!(r, m / 7 + m % 7);
}

// ── Integer divide/remainder never trap (docs/design/lpir/02-core-ops.md) ────
//
// Xtensa's QUOS/QUOU/REMS/REMU raise EXCCAUSE 6 on a zero divisor, so lowering
// guards them ([`IsaTarget::integer_div_traps_on_zero`]). Without the guard
// every zero-divisor case below is a `Trap { Exception, cause: 6 }` and
// `expect_ok` panics — which is exactly what happens when the guard is removed,
// and how these tests were negative-controlled.

#[test]
fn idiv_s_by_zero_is_all_ones_not_a_trap() {
    assert_eq!(run_div_op(idiv_s, 42, 0) as i32, -1);
    assert_eq!(run_div_op(idiv_s, -42, 0) as i32, -1);
    assert_eq!(run_div_op(idiv_s, 0, 0) as i32, -1);
    assert_eq!(run_div_op(idiv_s, i32::MIN, 0) as i32, -1);
}

#[test]
fn idiv_u_by_zero_is_all_ones_not_a_trap() {
    assert_eq!(run_div_op(idiv_u, 42, 0), u32::MAX);
    assert_eq!(run_div_op(idiv_u, 0, 0), u32::MAX);
    assert_eq!(run_div_op(idiv_u, -1, 0), u32::MAX); // 0xFFFF_FFFF / 0
}

#[test]
fn irem_s_by_zero_is_the_dividend_not_a_trap() {
    assert_eq!(run_div_op(irem_s, 42, 0) as i32, 42);
    assert_eq!(run_div_op(irem_s, -42, 0) as i32, -42);
    assert_eq!(run_div_op(irem_s, 0, 0) as i32, 0);
    assert_eq!(run_div_op(irem_s, i32::MIN, 0) as i32, i32::MIN);
}

#[test]
fn irem_u_by_zero_is_the_dividend_not_a_trap() {
    assert_eq!(run_div_op(irem_u, 42, 0), 42);
    assert_eq!(run_div_op(irem_u, 0, 0), 0);
    assert_eq!(run_div_op(irem_u, -1, 0), u32::MAX);
}

/// The control: `i32::MIN / -1` and `i32::MIN % -1` are RV32M edge cases that
/// Xtensa's divide **already** gets right, so lowering deliberately does not
/// guard them. These pass before and after the guard exists; if they ever start
/// failing, the guard grew a case it should not have.
#[test]
fn int_min_over_minus_one_needs_no_guard_on_xtensa() {
    assert_eq!(run_div_op(idiv_s, i32::MIN, -1) as i32, i32::MIN);
    assert_eq!(run_div_op(irem_s, i32::MIN, -1) as i32, 0);
    assert_eq!(run_div_op(idiv_s, 42, -1) as i32, -42);
    assert_eq!(run_div_op(irem_s, 42, -1) as i32, 0);
}

/// The guard must be invisible on the ordinary path — every non-zero divisor
/// still produces the plain quotient/remainder, including the signs GLSL
/// requires (truncation toward zero; remainder takes the dividend's sign).
#[test]
fn nonzero_divisors_are_unperturbed_by_the_guard() {
    assert_eq!(run_div_op(idiv_s, 42, 5) as i32, 8);
    assert_eq!(run_div_op(idiv_s, -42, 5) as i32, -8);
    assert_eq!(run_div_op(idiv_s, 42, -5) as i32, -8);
    assert_eq!(run_div_op(irem_s, 42, 5) as i32, 2);
    assert_eq!(run_div_op(irem_s, -42, 5) as i32, -2);
    assert_eq!(run_div_op(irem_s, 42, -5) as i32, 2);
    assert_eq!(run_div_op(idiv_u, 42, 5), 8);
    assert_eq!(run_div_op(irem_u, 42, 5), 2);
    // Unsigned reads the operands as u32: 0xFFFF_FFFF / 2 == 0x7FFF_FFFF,
    // which a signed divide would answer 0 for.
    assert_eq!(run_div_op(idiv_u, -1, 2), 0x7FFF_FFFF);
    assert_eq!(run_div_op(irem_u, -1, 2), 1);
}

/// `dst` aliasing an operand is the case an emitter-level expansion with
/// hand-managed scratch registers would get wrong: the guard reads `lhs` again
/// *after* the divide has been computed, so a lowering that clobbered `dst`
/// early would return the quotient instead of the dividend here.
#[test]
fn div_guard_is_correct_when_dst_aliases_an_operand() {
    // dst == lhs
    assert_eq!(
        run_binary(
            |fb, x, y| {
                fb.push(LpirOp::IremS {
                    dst: x,
                    lhs: x,
                    rhs: y,
                });
                fb.push_return(&[x]);
            },
            -42,
            0,
        ) as i32,
        -42,
    );
    // dst == rhs
    assert_eq!(
        run_binary(
            |fb, x, y| {
                fb.push(LpirOp::IdivS {
                    dst: y,
                    lhs: x,
                    rhs: y,
                });
                fb.push_return(&[y]);
            },
            42,
            0,
        ) as i32,
        -1,
    );
    // dst == lhs == rhs
    assert_eq!(
        run_binary(
            |fb, x, _y| {
                fb.push(LpirOp::IdivS {
                    dst: x,
                    lhs: x,
                    rhs: x,
                });
                fb.push_return(&[x]);
            },
            0,
            0,
        ) as i32,
        -1,
    );
}

/// The reason the contract exists in practice: GLSL authors write guarded
/// division, and eager `&&` evaluation runs the divide anyway. `x / i` with
/// `i == 0` must yield `-1` rather than taking the board down.
#[test]
fn guarded_division_idiom_does_not_trap_when_the_guard_is_false() {
    let build = |fb: &mut FunctionBuilder, x: lpir::VReg, i: lpir::VReg| {
        let zero = fb.alloc_vreg(IrType::I32);
        let ne = fb.alloc_vreg(IrType::I32);
        let q = fb.alloc_vreg(IrType::I32);
        let gt = fb.alloc_vreg(IrType::I32);
        let both = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 {
            dst: zero,
            value: 0,
        });
        fb.push(LpirOp::Ine {
            dst: ne,
            lhs: i,
            rhs: zero,
        });
        fb.push(LpirOp::IdivS {
            dst: q,
            lhs: x,
            rhs: i,
        });
        fb.push(LpirOp::IgtS {
            dst: gt,
            lhs: q,
            rhs: zero,
        });
        fb.push(LpirOp::Iand {
            dst: both,
            lhs: ne,
            rhs: gt,
        });
        fb.push_return(&[both]);
    };
    assert_eq!(run_binary(build, 10, 0), 0);
    assert_eq!(run_binary(build, 10, 2), 1);
    assert_eq!(run_binary(build, -10, 0), 0);
    assert_eq!(run_binary(build, 10, -2), 0);
}

/// A divisor that is a single-definition non-zero constant cannot be zero, so
/// the guard is elided entirely and the bare `QUOS`/`REMS` is emitted. The
/// values must be unchanged, and the function must be *smaller* than the same
/// division by a runtime divisor — the size assertion is what proves the
/// elision actually fired rather than the test merely re-checking arithmetic.
#[test]
fn constant_nonzero_divisor_elides_the_guard() {
    let const_div = |value: i32| {
        move |fb: &mut FunctionBuilder, x: lpir::VReg, _y: lpir::VReg| {
            let c = fb.alloc_vreg(IrType::I32);
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 { dst: c, value });
            fb.push(LpirOp::IdivS {
                dst: d,
                lhs: x,
                rhs: c,
            });
            fb.push_return(&[d]);
        }
    };
    assert_eq!(run_binary(const_div(7), 42, 0) as i32, 6);
    assert_eq!(run_binary(const_div(7), -42, 0) as i32, -6);
    assert_eq!(run_binary(const_div(-1), i32::MIN, 0) as i32, i32::MIN);

    // A zero constant divisor must still be guarded — it is exactly the case
    // the contract is about, and eliding it would reintroduce the trap.
    assert_eq!(run_binary(const_div(0), 42, 0) as i32, -1);

    let bytes = |b: fn(&mut FunctionBuilder, lpir::VReg, lpir::VReg)| {
        let (ir, sig) = binary_module(b);
        compile_module(
            &ir,
            &sig,
            FloatMode::Q32,
            NativeCompileOptions {
                float_mode: FloatMode::Q32,
                ..Default::default()
            },
            IsaTarget::Xtensa,
        )
        .expect("compile")
        .functions[0]
            .code
            .len()
    };
    let folded = bytes(|fb, x, _y| {
        let c = fb.alloc_vreg(IrType::I32);
        let d = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 { dst: c, value: 7 });
        fb.push(LpirOp::IdivS {
            dst: d,
            lhs: x,
            rhs: c,
        });
        fb.push_return(&[d]);
    });
    let guarded = bytes(|fb, x, y| {
        let d = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IdivS {
            dst: d,
            lhs: x,
            rhs: y,
        });
        fb.push_return(&[d]);
    });
    assert!(
        folded < guarded,
        "constant divisor should skip the guard: {folded} B folded vs {guarded} B guarded"
    );
}

/// The soundness boundary: a divisor with more than one definition is not
/// knowable from a linear walk, so the guard must stay even though one of the
/// definitions is a non-zero constant. Here the second definition makes the
/// divisor zero at run time — with an unsound elision this traps.
#[test]
fn redefined_divisor_keeps_the_guard() {
    assert_eq!(
        run_binary(
            |fb, x, y| {
                let c = fb.alloc_vreg(IrType::I32);
                let d = fb.alloc_vreg(IrType::I32);
                fb.push(LpirOp::IconstI32 { dst: c, value: 7 });
                // Second definition: c now holds whatever the caller passed.
                fb.push(LpirOp::Copy { dst: c, src: y });
                fb.push(LpirOp::IdivS {
                    dst: d,
                    lhs: x,
                    rhs: c,
                });
                fb.push_return(&[d]);
            },
            42,
            0,
        ) as i32,
        -1,
    );
}

#[test]
fn shifts_signed_and_unsigned() {
    let r = run_unary(
        |fb, x| {
            let a = fb.alloc_vreg(IrType::I32);
            let b = fb.alloc_vreg(IrType::I32);
            let s = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IshlImm {
                dst: a,
                src: x,
                imm: 4,
            });
            fb.push(LpirOp::IshrSImm {
                dst: b,
                src: x,
                imm: 3,
            });
            fb.push(LpirOp::Iadd {
                dst: s,
                lhs: a,
                rhs: b,
            });
            fb.push_return(&[s]);
        },
        -1024,
    );
    assert_eq!(r, (-1024i32 << 4) + (-1024i32 >> 3));
}

#[test]
fn srli_16_and_up_lowers_to_extui() {
    let r = run_unary(
        |fb, x| {
            fb.push(LpirOp::IshrUImm {
                dst: x,
                src: x,
                imm: 20,
            });
            fb.push_return(&[x]);
        },
        0xDEAD_BEEFu32 as i32,
    );
    assert_eq!(r as u32, 0xDEAD_BEEFu32 >> 20);
}

#[test]
fn select_and_compares() {
    // |x| via compare + select: (x < 0) ? -x : x.
    let r = run_unary(
        |fb, x| {
            let zero = fb.alloc_vreg(IrType::I32);
            let cond = fb.alloc_vreg(IrType::I32);
            let neg = fb.alloc_vreg(IrType::I32);
            let out = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 {
                dst: zero,
                value: 0,
            });
            fb.push(LpirOp::IltS {
                dst: cond,
                lhs: x,
                rhs: zero,
            });
            fb.push(LpirOp::Ineg { dst: neg, src: x });
            fb.push(LpirOp::Select {
                dst: out,
                cond,
                if_true: neg,
                if_false: x,
            });
            fb.push_return(&[out]);
        },
        -12345,
    );
    assert_eq!(r, 12345);
}

#[test]
fn if_else_branches_both_directions() {
    // if x >= 10 { 2 } else { 1 } — forward branch over the then-arm and the
    // join, matching the conformance corpus's branchdir case.
    for (x, want) in [(5, 1), (20, 2), (10, 2)] {
        let r = run_unary(
            |fb, x| {
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
            },
            x,
        );
        assert_eq!(r, want, "x={x}");
    }
}

#[test]
fn spill_pressure_beyond_the_12_reg_pool() {
    // 20 live values forces spills (pool = 12): v_i = x + i, all kept live,
    // then summed. Exercises slot stores/reloads under the windowed frame
    // (spill slots must not collide with the 32-byte top reservation).
    let n = 20i32;
    let r = run_unary(
        |fb, x| {
            let mut vs = Vec::new();
            for i in 0..n {
                let v = fb.alloc_vreg(IrType::I32);
                fb.push(LpirOp::IaddImm {
                    dst: v,
                    src: x,
                    imm: i,
                });
                vs.push(v);
            }
            // Sum in reverse so every value stays live until used.
            let acc = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 { dst: acc, value: 0 });
            for &v in vs.iter().rev() {
                fb.push(LpirOp::Iadd {
                    dst: acc,
                    lhs: acc,
                    rhs: v,
                });
            }
            fb.push_return(&[acc]);
        },
        1000,
    );
    let want: i32 = (0..n).map(|i| 1000 + i).sum();
    assert_eq!(r, want);
}

#[test]
fn cross_function_call_via_literal_slot_callx8() {
    // g(x) = 3x; f(x) = g(x) + 1 — the pooled-literal + l32r + callx8 path
    // with a real relocation patched by the harness, plus caller-saved
    // handling around the call.
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
    let call_out = fb.alloc_vreg(IrType::I32);
    fb.push_call(
        lpir::CalleeRef::Local(FuncId(0)),
        &[lpir::VMCTX_VREG, x],
        &[call_out],
    );
    fb.push(LpirOp::IaddImm {
        dst: call_out,
        src: call_out,
        imm: 1,
    });
    fb.push_return(&[call_out]);
    let f = fb.finish();

    let module = LpirModule {
        imports: vec![],
        functions: VecMap::from([(FuncId(0), g), (FuncId(1), f)]),
    };
    let int_sig = |name: &str| LpsFnSig {
        name: name.to_string(),
        parameters: vec![FnParam {
            name: "x".to_string(),
            ty: LpsType::Int,
            qualifier: ParamQualifier::In,
        }],
        return_type: LpsType::Int,
        kind: LpsFnKind::UserDefined,
    };
    let sig = LpsModuleSig {
        functions: vec![int_sig("g"), int_sig("f")],
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    };

    let out = expect_ok(compile_link_run(&module, &sig, "f", &[0, 14], false));
    assert_eq!(out as i32, 14 * 3 + 1);
}

/// Multi-argument guest→guest calls, one arity per case.
///
/// Regression for three allocator defects the Xtensa filetest corpus surfaced
/// (2026-07-30). All were in *shared* allocator code that rv32's register
/// layout happens to make safe, and Xtensa's does not:
///
/// 1. **Call-argument staging was not a parallel move.** Moves were emitted in
///    argument order, so an argument whose source register was another
///    argument's destination got clobbered and the callee received a duplicate.
///    Xtensa's staging bank `a10..a15` *is* the caller-saved half of its
///    allocatable pool, so sources land there constantly; rv32's argument
///    registers (10..17) and pool (18..31) are disjoint.
/// 2. **Entry parameter moves had the same hazard**, callee-side, plus an
///    ordering dependency with the incoming-stack-arg loads.
///
///    Cases 1..=11 cover both: the hazard fires at **three** user arguments,
///    well inside the six argument registers, so it was never about stack
///    arguments. M3's `cross_function_call_via_literal_slot_callx8` passed
///    throughout because it uses exactly one.
///
/// 3. **Stack-passed arguments were left out of that parallel move.** The
///    overflow arguments are stored to the outgoing area by the ISA emitter,
///    i.e. after every staging move has run, so a source register that a
///    staging move writes is read too late. First observable at **twelve**
///    arguments, where the values fill Xtensa's 12-register pool and the
///    overflow one is sitting in a staging target. Cases 12..=20 cover it;
///    see `docs/defects/2026-07-30-xtensa-stack-arg-staged-over.md`.
///
/// The arguments are distinct powers of two and the callee sums them, so any
/// dropped, duplicated or misplaced argument changes the total.
#[rstest::rstest]
#[case(1)]
#[case(2)]
#[case(3)]
#[case(4)]
#[case(5)]
#[case(6)]
#[case(7)]
#[case(8)]
#[case(9)]
#[case(10)]
#[case(11)]
#[case(12)]
#[case(13)]
#[case(14)]
#[case(15)]
#[case(16)]
#[case(17)]
#[case(18)]
#[case(19)]
#[case(20)]
fn multi_arg_call_passes_every_argument(#[case] n: usize) {
    let mut cb = FunctionBuilder::new("g", &[IrType::I32]);
    let ps: Vec<_> = (0..n).map(|_| cb.add_param(IrType::I32)).collect();
    let acc = cb.alloc_vreg(IrType::I32);
    cb.push(LpirOp::IaddImm {
        dst: acc,
        src: ps[0],
        imm: 0,
    });
    for p in &ps[1..] {
        cb.push(LpirOp::Iadd {
            dst: acc,
            lhs: acc,
            rhs: *p,
        });
    }
    cb.push_return(&[acc]);
    let g = cb.finish();

    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let mut call_args = vec![lpir::VMCTX_VREG];
    for i in 0..n {
        let v = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 {
            dst: v,
            value: 1 << i,
        });
        call_args.push(v);
    }
    let out = fb.alloc_vreg(IrType::I32);
    fb.push_call(lpir::CalleeRef::Local(FuncId(0)), &call_args, &[out]);
    fb.push_return(&[out]);
    let f = fb.finish();

    let module = LpirModule {
        imports: vec![],
        functions: VecMap::from([(FuncId(0), g), (FuncId(1), f)]),
    };
    let sig_of = |name: &str, np: usize| LpsFnSig {
        name: name.to_string(),
        parameters: (0..np)
            .map(|i| FnParam {
                name: alloc_name(i),
                ty: LpsType::Int,
                qualifier: ParamQualifier::In,
            })
            .collect(),
        return_type: LpsType::Int,
        kind: LpsFnKind::UserDefined,
    };
    let sig = LpsModuleSig {
        functions: vec![sig_of("g", n), sig_of("f", 0)],
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    };

    let expect: u32 = (0..n).map(|i| 1u32 << i).sum();
    let got = expect_ok(compile_link_run(&module, &sig, "f", &[0], false));
    assert_eq!(
        got, expect,
        "{n}-argument call: an argument was dropped, duplicated or misplaced"
    );
}

fn alloc_name(i: usize) -> String {
    format!("p{i}")
}
