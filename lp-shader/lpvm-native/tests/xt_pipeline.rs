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
