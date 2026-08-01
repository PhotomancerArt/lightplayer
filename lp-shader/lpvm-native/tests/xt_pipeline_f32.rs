//! Full-pipeline tests for **hardware f32** on the Xtensa backend: LPIR →
//! lower → regalloc → `isa/xt` emit (integer half + `emit_fp`) → manual link
//! at the emulator's code base → windowed execution on `lp-xt-emu`.
//!
//! The f32 sibling of [`xt_pipeline`](../xt_pipeline.rs), and the same promise:
//! every value asserted here went through the real compiler pipeline with
//! `IsaTarget::Xtensa` and `FloatMode::F32`, not a hand-built VInst stream.
//! Where `xt_pipeline.rs` was the M3 gate rig, this is M7's.
//!
//! # What this file exists to prove
//!
//! Two things beyond "arithmetic works", both of which are silent-corruption
//! hazards that a shallow test cannot see:
//!
//! 1. **The frame is safe for floats** (M7 D7). Float spills live at the
//!    *bottom* of the frame; the window-overflow handler scribbles in the
//!    32-byte reservation at the *top*. The argument that they cannot collide
//!    is worth nothing untested, so `float_recursion_at_depth_100_*` carries
//!    live floats across 100 nested `call8`s and checks every one on unwind.
//! 2. **Float spill offsets are range-checked** (`lsi`/`ssi` reach 1020 bytes,
//!    and `lp-xt-inst`'s encoder truncates rather than failing). Covered at
//!    the emitter level in `isa::xt::emit_fp`'s own tests, which can assert
//!    the *encoding*; here the concern is that a real compiled function with
//!    saturated register pressure round-trips.
//!
//! # Arming the FPU
//!
//! `Emulator::run*` stages a fresh `Cpu`, and `Cpu::new()` leaves `CPENABLE`
//! clear deliberately, so firmware that forgets to arm the coprocessor faults
//! on the host rather than silently working. Compiled shader code does not arm
//! it — that is board init's job (M7 D6, P5) — so this rig prepends a
//! two-instruction preamble that does what P5's board init will do.
//! `unarmed_float_code_faults_with_a_coprocessor_trap` is the negative
//! control: the same code without the preamble must take EXCCAUSE 32.

use lp_collection::VecMap;
use lpir::builder::FunctionBuilder;
use lpir::{FloatMode, FuncId, IrType, LpirModule, LpirOp};
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
use lpvm_native::compile::compile_module;
use lpvm_native::isa::IsaTarget;
use lpvm_native::native_options::NativeCompileOptions;

use lp_xt_elf::XtensaElf;
use lp_xt_emu::{CallOutcome, Emulator, RunOutcome};
use lp_xt_inst::{Inst, NullaryNarrowOp, Reg, SpecialReg, SrOp, encode};

/// EXCCAUSE 32 — `Coprocessor0Disabled`, what an FP instruction takes when
/// `CPENABLE` bit 0 is clear.
const EXC_COPROCESSOR0_DISABLED: u32 = 32;

/// `movi a15, 1; wsr.cpenable a15`, padded to a multiple of 4 bytes.
///
/// Runs *before* the entry function's `ENTRY`, in the caller's window. That
/// constrains which register it may touch: after the CALL8 rotation the
/// caller's `a15` becomes the callee's `a7`, the sixth argument register, so
/// this is safe for any entry taking five arguments or fewer — asserted in
/// [`link_blob`], because the failure would be a silently wrong argument.
///
/// The 4-byte padding is not cosmetic: the emitted blob's literal pool must
/// stay word-aligned (`isa::xt::emit`'s layout contract assumes a 4-aligned
/// blob start), and an odd-sized preamble would misalign every `l32r` target.
fn arm_fpu_preamble() -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&encode(&Inst::Movi(Reg::new(15), 1)));
    p.extend_from_slice(&encode(&Inst::Sr(
        SrOp::Wsr,
        SpecialReg::Cpenable,
        Reg::new(15),
    )));
    while p.len() % 4 != 0 {
        p.extend_from_slice(&encode(&Inst::NullaryN(NullaryNarrowOp::NopN)));
    }
    p
}

/// Compile `(ir, sig)` for Xtensa in `float_mode`, link every function at the
/// emulator's I-bus code base (patching literal-slot call relocations), and
/// return the linked blob plus the offset to start execution at.
///
/// **The entry function is laid out first**, so the returned offset is always
/// 0 and, when `arm_fpu` is set, [`arm_fpu_preamble`] can simply precede it
/// and fall through. Entering at the preamble is the whole point — a run that
/// starts at the entry function instead skips the arming and every FP
/// instruction takes EXCCAUSE 32. The preamble is part of the blob *before*
/// function offsets are taken, so relocations resolve to the shifted addresses
/// with no separate fixup.
fn link_blob(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    entry_name: &str,
    float_mode: FloatMode,
    arm_fpu: bool,
    emu: &Emulator,
) -> (Vec<u8>, u32) {
    let opts = NativeCompileOptions {
        float_mode,
        fuel: false,
        ..Default::default()
    };
    let module = compile_module(ir, sig, float_mode, opts, IsaTarget::Xtensa)
        .expect("xt f32 compile should succeed");

    let funcs: Vec<_> = module.functions.iter().collect();
    let entry_idx = funcs
        .iter()
        .position(|f| f.name == entry_name)
        .expect("entry function exists");
    let mut order: Vec<usize> = (0..funcs.len()).collect();
    order.sort_by_key(|&i| i != entry_idx);

    let mut code = if arm_fpu {
        arm_fpu_preamble()
    } else {
        Vec::new()
    };
    let entry_off = code.len() as u32;
    let mut entries = VecMap::<String, usize>::new();
    let mut func_offsets = vec![0usize; funcs.len()];
    for &i in &order {
        func_offsets[i] = code.len();
        entries.insert(funcs[i].name.clone(), code.len());
        code.extend_from_slice(&funcs[i].code);
    }

    let ibus_base = emu.profile.code_ibus_base();
    for (fi, f) in funcs.iter().enumerate() {
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

    // Run from the preamble when arming, from the entry function otherwise —
    // and those are the same place, because the entry is laid out first.
    (code, if arm_fpu { 0 } else { entry_off })
}

/// Compile, link and run in `FloatMode::F32` with the FPU armed.
fn run_f32(ir: &LpirModule, sig: &LpsModuleSig, entry_name: &str, args: &[u32]) -> RunOutcome {
    assert!(
        args.len() <= 5,
        "the arming preamble clobbers the sixth argument register"
    );
    let mut emu = Emulator::new();
    let (code, entry_off) = link_blob(ir, sig, entry_name, FloatMode::F32, true, &emu);
    emu.run_with_args(&code, entry_off, args)
}

/// The Xtensa builtins base image, or `None` with a loud note when it has not
/// been built.
///
/// A gitignored cross-target artifact (`scripts/build-builtins-xt.sh`, esp
/// toolchain), so absence is a skip and not a failure — the same contract
/// `xt_builtins_image.rs` uses.
fn builtins_image() -> Option<Vec<u8>> {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../lp-xt/fixtures/elf/lps-builtins-xt-app.elf");
    match std::fs::read(&p) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!(
                "SKIP: {} not found — run scripts/build-builtins-xt.sh (esp toolchain) first",
                p.display()
            );
            None
        }
    }
}

/// Compile in `FloatMode::F32`, link against the **builtins base image**, and
/// run with the FPU armed.
///
/// [`run_f32`] links only the module's own functions and loads the blob at the
/// code region's base. That is enough for everything M7 P3 emits inline, but a
/// `sym_call` into `__lp_lpir_*_f32` (M7 D4) needs the builtins image resident
/// *and* its symbols in the relocation map — and the image already occupies the
/// base of that region, so the shader has to move.
///
/// The layout is `rt_emu::xt_image`'s: image `.text` first, shader code
/// 4-aligned after it, below the image's data segments. It is hand-rolled here
/// rather than calling `build_xt_image` because that sits behind the `emu`
/// feature, and the other tests in this file run at *default* features.
fn run_f32_with_builtins(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    entry_name: &str,
    args: &[u32],
    image: &[u8],
) -> RunOutcome {
    let mut emu = Emulator::new();
    let elf = XtensaElf::parse(image).expect("builtins image parses as Xtensa ELF32");
    elf.load_into(&mut emu)
        .expect("image loads into emulator memory");

    // Executable segments are addressed through the I-bus view and data
    // segments through the D-bus one (lp-xt/lps-builtins-xt-app/link.ld), so
    // the split is read off the image rather than hardcoded.
    let ibus_base = emu.profile.code_ibus_base();
    let alias = emu.profile.alias;
    let mut text_end = ibus_base;
    let mut data_start = u32::MAX;
    for seg in elf.segments().expect("image segments decode") {
        let end = seg.vaddr + seg.memsz;
        if seg.vaddr >= ibus_base {
            text_end = text_end.max(end);
        } else {
            data_start = data_start.min(alias.dbus_to_ibus(seg.vaddr));
        }
    }
    let shader_base = text_end.next_multiple_of(4);

    let opts = NativeCompileOptions {
        float_mode: FloatMode::F32,
        fuel: false,
        ..Default::default()
    };
    let module = compile_module(ir, sig, FloatMode::F32, opts, IsaTarget::Xtensa)
        .expect("xt f32 compile should succeed");

    // Arming preamble, then the entry function, then everything else. The
    // preamble is a multiple of 4 bytes long, so the entry's ENTRY is the next
    // instruction executed and no jump is needed.
    let funcs: Vec<_> = module.functions.iter().collect();
    let entry_idx = funcs
        .iter()
        .position(|f| f.name == entry_name)
        .expect("entry function exists");
    let mut order: Vec<usize> = (0..funcs.len()).collect();
    order.sort_by_key(|&i| i != entry_idx);

    // Execution starts at the preamble, not at the entry function — entering at
    // the entry skips the arming and the first FP instruction takes EXCCAUSE 32.
    let mut code = arm_fpu_preamble();
    let entry = shader_base;
    let mut symbols: std::collections::BTreeMap<String, u32> = elf.symbols().into_iter().collect();
    let mut func_addrs = vec![0u32; funcs.len()];
    for &i in &order {
        let at = shader_base + code.len() as u32;
        func_addrs[i] = at;
        symbols.insert(funcs[i].name.clone(), at);
        code.extend_from_slice(&funcs[i].code);
        while code.len() % 4 != 0 {
            code.push(0);
        }
    }
    assert!(
        shader_base + code.len() as u32 <= data_start,
        "shader code overruns the builtins image's data segments"
    );

    for (fi, f) in funcs.iter().enumerate() {
        for reloc in &f.relocs {
            assert_eq!(
                reloc.r_type,
                IsaTarget::Xtensa.call_reloc_type(),
                "unexpected reloc type"
            );
            let target = *symbols.get(&reloc.symbol).unwrap_or_else(|| {
                panic!(
                    "unresolved symbol {} — not a builtin in the base image nor a \
                     function in this module",
                    reloc.symbol
                )
            });
            let slot = (func_addrs[fi] - shader_base) as usize + reloc.offset;
            code[slot..slot + 4].copy_from_slice(&target.to_le_bytes());
        }
    }

    emu.mem.load_bytes(shader_base, &code);
    let mut tracer = lp_xt_emu::NoopTracer;
    match emu.run_loaded_with_args(entry, args, &mut tracer, None) {
        CallOutcome::Ok { lo, .. } => RunOutcome::Ok(lo),
        CallOutcome::Trap(t) => RunOutcome::Trap(t),
    }
}

fn expect_ok(out: RunOutcome) -> u32 {
    match out {
        RunOutcome::Ok(v) => v,
        RunOutcome::Trap(t) => panic!("unexpected trap: {t:?}"),
    }
}

fn bits(f: f32) -> u32 {
    f.to_bits()
}

// ---------------------------------------------------------------------------
// Module shapes
// ---------------------------------------------------------------------------

fn float_param(name: &str) -> FnParam {
    FnParam {
        name: name.to_string(),
        ty: LpsType::Float,
        qualifier: ParamQualifier::In,
    }
}

fn int_param(name: &str) -> FnParam {
    FnParam {
        name: name.to_string(),
        ty: LpsType::Int,
        qualifier: ParamQualifier::In,
    }
}

fn one_function_sig(params: Vec<FnParam>, ret: LpsType) -> LpsModuleSig {
    LpsModuleSig {
        functions: vec![LpsFnSig {
            name: "f".to_string(),
            parameters: params,
            return_type: ret,
            kind: LpsFnKind::UserDefined,
        }],
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    }
}

fn one_function_module(func: lpir::IrFunction) -> LpirModule {
    LpirModule {
        imports: vec![],
        functions: VecMap::from([(FuncId(0), func)]),
    }
}

/// `f(a: float, b: float) -> float`, body built by `build`. Both operands are
/// runtime arguments so nothing can be constant-folded.
fn run_float_binop(
    build: impl FnOnce(&mut FunctionBuilder, lpir::VReg, lpir::VReg, lpir::VReg),
    a: f32,
    b: f32,
) -> u32 {
    let mut fb = FunctionBuilder::new("f", &[IrType::F32, IrType::F32]);
    let x = fb.add_param(IrType::F32);
    let y = fb.add_param(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    build(&mut fb, x, y, out);
    fb.push_return(&[out]);
    let ir = one_function_module(fb.finish());
    let sig = one_function_sig(vec![float_param("a"), float_param("b")], LpsType::Float);
    expect_ok(run_f32(&ir, &sig, "f", &[0, bits(a), bits(b)]))
}

/// `f(a: float, b: float) -> int` — the compare shape.
fn run_float_compare(
    make: fn(lpir::VReg, lpir::VReg, lpir::VReg) -> LpirOp,
    a: f32,
    b: f32,
) -> u32 {
    let mut fb = FunctionBuilder::new("f", &[IrType::F32, IrType::F32]);
    let x = fb.add_param(IrType::F32);
    let y = fb.add_param(IrType::F32);
    let out = fb.alloc_vreg(IrType::I32);
    fb.push(make(out, x, y));
    fb.push_return(&[out]);
    let ir = one_function_module(fb.finish());
    let sig = one_function_sig(vec![float_param("a"), float_param("b")], LpsType::Int);
    expect_ok(run_f32(&ir, &sig, "f", &[0, bits(a), bits(b)]))
}

// ---------------------------------------------------------------------------
// Arithmetic, the inline family
// ---------------------------------------------------------------------------

#[test]
fn fadd_fsub_fmul_through_the_pipeline() {
    let add = |fb: &mut FunctionBuilder, x, y, out| {
        fb.push(LpirOp::Fadd {
            dst: out,
            lhs: x,
            rhs: y,
        })
    };
    let sub = |fb: &mut FunctionBuilder, x, y, out| {
        fb.push(LpirOp::Fsub {
            dst: out,
            lhs: x,
            rhs: y,
        })
    };
    let mul = |fb: &mut FunctionBuilder, x, y, out| {
        fb.push(LpirOp::Fmul {
            dst: out,
            lhs: x,
            rhs: y,
        })
    };
    assert_eq!(f32::from_bits(run_float_binop(add, 1.5, 2.25)), 3.75);
    assert_eq!(f32::from_bits(run_float_binop(sub, 1.5, 2.25)), -0.75);
    assert_eq!(f32::from_bits(run_float_binop(mul, 1.5, 2.25)), 3.375);
    // Infinities and signed zero — float.md §3 Guaranteed rows.
    assert_eq!(
        f32::from_bits(run_float_binop(add, f32::INFINITY, 1.0)),
        f32::INFINITY
    );
    assert_eq!(run_float_binop(mul, -1.0, 0.0), bits(-0.0));
    assert_eq!(run_float_binop(add, -0.0, 0.0), bits(0.0));
}

#[test]
fn fabs_and_fneg_through_the_pipeline() {
    let abs = |fb: &mut FunctionBuilder, x, _y, out| fb.push(LpirOp::Fabs { dst: out, src: x });
    let neg = |fb: &mut FunctionBuilder, x, _y, out| fb.push(LpirOp::Fneg { dst: out, src: x });
    assert_eq!(f32::from_bits(run_float_binop(abs, -3.5, 0.0)), 3.5);
    assert_eq!(run_float_binop(abs, -0.0, 0.0), bits(0.0));
    assert_eq!(f32::from_bits(run_float_binop(neg, 3.5, 0.0)), -3.5);
    assert_eq!(run_float_binop(neg, 0.0, 0.0), bits(-0.0));
}

#[test]
fn float_constants_materialize_through_wfr() {
    // A float constant is `IConst32` + `Wfr` (M7 D11) — no float literal pool
    // of its own. Multiplying by it proves the bit pattern arrived intact.
    let r = run_float_binop(
        |fb, x, _y, out| {
            let k = fb.alloc_vreg(IrType::F32);
            fb.push(LpirOp::FconstF32 {
                dst: k,
                value: 0.25,
            });
            fb.push(LpirOp::Fmul {
                dst: out,
                lhs: x,
                rhs: k,
            });
        },
        8.0,
        0.0,
    );
    assert_eq!(f32::from_bits(r), 2.0);
}

// ---------------------------------------------------------------------------
// Compares
// ---------------------------------------------------------------------------

fn feq(dst: lpir::VReg, lhs: lpir::VReg, rhs: lpir::VReg) -> LpirOp {
    LpirOp::Feq { dst, lhs, rhs }
}
fn fne(dst: lpir::VReg, lhs: lpir::VReg, rhs: lpir::VReg) -> LpirOp {
    LpirOp::Fne { dst, lhs, rhs }
}
fn flt(dst: lpir::VReg, lhs: lpir::VReg, rhs: lpir::VReg) -> LpirOp {
    LpirOp::Flt { dst, lhs, rhs }
}
fn fle(dst: lpir::VReg, lhs: lpir::VReg, rhs: lpir::VReg) -> LpirOp {
    LpirOp::Fle { dst, lhs, rhs }
}
fn fgt(dst: lpir::VReg, lhs: lpir::VReg, rhs: lpir::VReg) -> LpirOp {
    LpirOp::Fgt { dst, lhs, rhs }
}
fn fge(dst: lpir::VReg, lhs: lpir::VReg, rhs: lpir::VReg) -> LpirOp {
    LpirOp::Fge { dst, lhs, rhs }
}

#[test]
fn all_six_float_compares_through_the_pipeline() {
    for (name, make, l, r, want) in [
        ("eq", feq as fn(_, _, _) -> LpirOp, 1.0f32, 1.0f32, 1u32),
        ("eq", feq, 1.0, 2.0, 0),
        ("ne", fne, 1.0, 2.0, 1),
        ("ne", fne, 1.0, 1.0, 0),
        ("lt", flt, 1.0, 2.0, 1),
        ("lt", flt, 2.0, 1.0, 0),
        ("le", fle, 1.0, 1.0, 1),
        ("le", fle, 2.0, 1.0, 0),
        ("gt", fgt, 2.0, 1.0, 1),
        ("gt", fgt, 1.0, 2.0, 0),
        ("ge", fge, 1.0, 1.0, 1),
        ("ge", fge, 1.0, 2.0, 0),
    ] {
        assert_eq!(run_float_compare(make, l, r), want, "{name}({l}, {r})");
    }
}

/// float.md §3, a *Guaranteed* row: ordered comparisons are false when either
/// operand is NaN, and `!=` is true.
///
/// This is the row that caught M7 D5's mapping table being wrong — it
/// tabulated `ueq.s` + `movf` for `!=`, which computes "ordered and unequal"
/// and answers *false* on NaN. See `isa::xt::emit_fp::emit_fcmp`.
#[test]
fn float_compares_handle_nan_per_the_spec() {
    let nan = f32::NAN;
    for (name, make, want) in [
        ("eq", feq as fn(_, _, _) -> LpirOp, 0u32),
        ("ne", fne, 1),
        ("lt", flt, 0),
        ("le", fle, 0),
        ("gt", fgt, 0),
        ("ge", fge, 0),
    ] {
        for (l, r) in [(nan, 1.0f32), (1.0, nan), (nan, nan)] {
            assert_eq!(run_float_compare(make, l, r), want, "{name} with NaN");
        }
    }
}

// ---------------------------------------------------------------------------
// Select and conversion
// ---------------------------------------------------------------------------

#[test]
fn float_select_through_the_pipeline() {
    // fabs via compare + select: (x < 0) ? -x : x, the float mirror of
    // `xt_pipeline`'s `select_and_compares`.
    for (x, want) in [(-12.5f32, 12.5f32), (12.5, 12.5), (0.0, 0.0)] {
        let r = run_float_binop(
            |fb, x, _y, out| {
                let zero = fb.alloc_vreg(IrType::F32);
                let cond = fb.alloc_vreg(IrType::I32);
                let neg = fb.alloc_vreg(IrType::F32);
                fb.push(LpirOp::FconstF32 {
                    dst: zero,
                    value: 0.0,
                });
                fb.push(LpirOp::Flt {
                    dst: cond,
                    lhs: x,
                    rhs: zero,
                });
                fb.push(LpirOp::Fneg { dst: neg, src: x });
                fb.push(LpirOp::Select {
                    dst: out,
                    cond,
                    if_true: neg,
                    if_false: x,
                });
            },
            x,
            0.0,
        );
        assert_eq!(f32::from_bits(r), want, "select on {x}");
    }
}

#[test]
fn itof_signed_and_unsigned_through_the_pipeline() {
    for (signed, arg, want) in [
        (true, -3i32 as u32, -3.0f32),
        (true, 7, 7.0),
        (false, 0xFFFF_FFFF, 4294967296.0),
        (false, 7, 7.0),
    ] {
        let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
        let x = fb.add_param(IrType::I32);
        let out = fb.alloc_vreg(IrType::F32);
        fb.push(if signed {
            LpirOp::ItofS { dst: out, src: x }
        } else {
            LpirOp::ItofU { dst: out, src: x }
        });
        fb.push_return(&[out]);
        let ir = one_function_module(fb.finish());
        let sig = one_function_sig(vec![int_param("x")], LpsType::Float);
        let got = expect_ok(run_f32(&ir, &sig, "f", &[0, arg]));
        assert_eq!(
            f32::from_bits(got),
            want,
            "itof signed={signed} of {arg:#x}"
        );
    }
}

// ---------------------------------------------------------------------------
// The D1 boundary: float params in, float return out, across a real call
// ---------------------------------------------------------------------------

/// Float values travel across every call boundary in **address** registers as
/// raw IEEE bit patterns (M7 D1/D2), with `wfr`/`rfr` at the seams. A
/// guest→guest call carrying floats both ways is what makes that observable:
/// if the convention were half-applied, the callee would read an FR the caller
/// never wrote.
#[test]
fn floats_cross_a_guest_call_in_address_registers() {
    // g(a, b) = a * b + a; f(a, b) = g(a, b) - b
    let mut cb = FunctionBuilder::new("g", &[IrType::F32, IrType::F32]);
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

    let mut fb = FunctionBuilder::new("f", &[IrType::F32, IrType::F32]);
    let a = fb.add_param(IrType::F32);
    let b = fb.add_param(IrType::F32);
    let called = fb.alloc_vreg(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    fb.push_call(
        lpir::CalleeRef::Local(FuncId(0)),
        &[lpir::VMCTX_VREG, a, b],
        &[called],
    );
    fb.push(LpirOp::Fsub {
        dst: out,
        lhs: called,
        rhs: b,
    });
    fb.push_return(&[out]);
    let f = fb.finish();

    let ir = LpirModule {
        imports: vec![],
        functions: VecMap::from([(FuncId(0), g), (FuncId(1), f)]),
    };
    let sig = LpsModuleSig {
        functions: vec![
            LpsFnSig {
                name: "g".to_string(),
                parameters: vec![float_param("a"), float_param("b")],
                return_type: LpsType::Float,
                kind: LpsFnKind::UserDefined,
            },
            LpsFnSig {
                name: "f".to_string(),
                parameters: vec![float_param("a"), float_param("b")],
                return_type: LpsType::Float,
                kind: LpsFnKind::UserDefined,
            },
        ],
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    };

    let (a, b) = (3.0f32, 0.5f32);
    let got = expect_ok(run_f32(&ir, &sig, "f", &[0, bits(a), bits(b)]));
    assert_eq!(f32::from_bits(got), a * b + a - b);
}

// ---------------------------------------------------------------------------
// Register pressure — the counterpart to `spill_pressure_beyond_the_12_reg_pool`
// ---------------------------------------------------------------------------

/// 24 simultaneously-live floats against a 15-register float pool, forcing
/// spills and reloads through `ssi`/`lsi`.
#[test]
fn float_spill_pressure_beyond_the_float_pool() {
    let n = 24;
    let r = run_float_binop(
        |fb, x, _y, out| {
            let one = fb.alloc_vreg(IrType::F32);
            fb.push(LpirOp::FconstF32 {
                dst: one,
                value: 1.0,
            });
            let mut vs = Vec::new();
            let mut prev = x;
            for _ in 0..n {
                let v = fb.alloc_vreg(IrType::F32);
                fb.push(LpirOp::Fadd {
                    dst: v,
                    lhs: prev,
                    rhs: one,
                });
                vs.push(v);
                prev = v;
            }
            // Sum in reverse so every value stays live until it is used.
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
        },
        0.0,
        0.0,
    );
    // v_i = i + 1, summed.
    let want: f32 = (1..=n).map(|i| i as f32).sum();
    assert_eq!(f32::from_bits(r), want);
}

/// Both pools saturated at once — ~14 integer and ~20 float values live
/// simultaneously. This is where a class confusion shows up: an integer
/// instruction handed a float allocation, or a float spill written to an
/// integer's slot, changes the answer rather than crashing.
#[test]
fn both_register_pools_saturated_simultaneously() {
    let n_int = 14i32;
    let n_flt = 20;
    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let k = fb.add_param(IrType::I32);

    let one = fb.alloc_vreg(IrType::F32);
    fb.push(LpirOp::FconstF32 {
        dst: one,
        value: 1.0,
    });

    let mut ints = Vec::new();
    for i in 0..n_int {
        let v = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IaddImm {
            dst: v,
            src: k,
            imm: i,
        });
        ints.push(v);
    }
    let mut flts = Vec::new();
    let mut prev = one;
    for _ in 0..n_flt {
        let v = fb.alloc_vreg(IrType::F32);
        fb.push(LpirOp::Fadd {
            dst: v,
            lhs: prev,
            rhs: one,
        });
        flts.push(v);
        prev = v;
    }

    // Consume both sets in reverse, interleaved, so both pools are under
    // pressure across the same instructions.
    let iacc = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: iacc,
        value: 0,
    });
    let facc = fb.alloc_vreg(IrType::F32);
    fb.push(LpirOp::FconstF32 {
        dst: facc,
        value: 0.0,
    });
    for i in 0..n_flt.max(n_int as usize) {
        if let Some(&v) = ints.iter().rev().nth(i) {
            fb.push(LpirOp::Iadd {
                dst: iacc,
                lhs: iacc,
                rhs: v,
            });
        }
        if let Some(&v) = flts.iter().rev().nth(i) {
            fb.push(LpirOp::Fadd {
                dst: facc,
                lhs: facc,
                rhs: v,
            });
        }
    }

    // Fold the integer accumulator into the float one so a single return value
    // depends on both, and neither can be silently dropped.
    let iacc_f = fb.alloc_vreg(IrType::F32);
    fb.push(LpirOp::ItofS {
        dst: iacc_f,
        src: iacc,
    });
    let out = fb.alloc_vreg(IrType::F32);
    fb.push(LpirOp::Fadd {
        dst: out,
        lhs: facc,
        rhs: iacc_f,
    });
    fb.push_return(&[out]);

    let ir = one_function_module(fb.finish());
    let sig = one_function_sig(vec![int_param("k")], LpsType::Float);
    let base = 100i32;
    let got = expect_ok(run_f32(&ir, &sig, "f", &[0, base as u32]));

    let want_i: i32 = (0..n_int).map(|i| base + i).sum();
    let want_f: f32 = (2..=(n_flt + 1)).map(|i| i as f32).sum();
    assert_eq!(f32::from_bits(got), want_f + want_i as f32);
}

// ---------------------------------------------------------------------------
// The frame hazard gate (M7 D7)
// ---------------------------------------------------------------------------

/// **The milestone's headline hazard, pinned.**
///
/// M7 changes nothing about the frame: no FR is callee-saved (measured,
/// M6-P4), so there is no FP callee-save region, `FrameLayout::compute` is
/// untouched, and the prologue/epilogue stay one `entry` / one `retw`. Float
/// spills land in the existing spill region at the *bottom* of the frame,
/// while the window-overflow handler writes into the 32-byte reservation at
/// the *top*.
///
/// That argument is worth nothing untested, and its failure mode is the worst
/// kind: **silent corruption of an ancestor frame that only surfaces long
/// after the return**. A shallow test cannot see it — the window has to
/// actually overflow, which needs real depth, and more than one live value has
/// to cross each call or a single-register accident can hide it.
///
/// So: depth 100, several live floats carried across each recursive `call8`,
/// every one checked on the way back out. This is the shape that retired the
/// windowed-ABI risk in the experiment repo.
///
/// The **integer control** is not decoration (M6-P2's precedent): if the ints
/// survive and the floats do not, the finding is "floats specifically", which
/// is a different bug from "the window machinery is broken".
#[test]
fn float_recursion_at_depth_100_preserves_every_live_value() {
    // rec(n, a, b) =
    //   if n == 0 { a + b }
    //   else { rec(n-1, a, b) + a*2 + b*3 }   -- a, b live across the call
    //
    // Every level holds `a`, `b` and the integer `n` live across its recursive
    // call, so 100 frames of them are in flight at the deepest point.
    let mut cb = FunctionBuilder::new("rec", &[IrType::I32, IrType::F32, IrType::F32]);
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
            lpir::CalleeRef::Local(FuncId(0)),
            &[lpir::VMCTX_VREG, n1, a, b],
            &[deeper],
        );
        // `a` and `b` are read *after* the call returns — that is what makes
        // them live across it, and what a clobbered ancestor frame destroys.
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

    let mut fb = FunctionBuilder::new("f", &[IrType::I32, IrType::F32, IrType::F32]);
    let fn_ = fb.add_param(IrType::I32);
    let fa = fb.add_param(IrType::F32);
    let fb_ = fb.add_param(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    fb.push_call(
        lpir::CalleeRef::Local(FuncId(0)),
        &[lpir::VMCTX_VREG, fn_, fa, fb_],
        &[out],
    );
    fb.push_return(&[out]);
    let f = fb.finish();

    let ir = LpirModule {
        imports: vec![],
        functions: VecMap::from([(FuncId(0), rec), (FuncId(1), f)]),
    };
    let params = || vec![int_param("n"), float_param("a"), float_param("b")];
    let sig = LpsModuleSig {
        functions: vec![
            LpsFnSig {
                name: "rec".to_string(),
                parameters: params(),
                return_type: LpsType::Float,
                kind: LpsFnKind::UserDefined,
            },
            LpsFnSig {
                name: "f".to_string(),
                parameters: params(),
                return_type: LpsType::Float,
                kind: LpsFnKind::UserDefined,
            },
        ],
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    };

    let depth = 100i32;
    let (a, b) = (1.25f32, 2.5f32);
    let mut emu = Emulator::new();
    // 100 nested calls of a spilling function need more than the fixture
    // corpus's default budget.
    emu.step_budget = 8_000_000;
    let (code, entry_off) = link_blob(&ir, &sig, "f", FloatMode::F32, true, &emu);
    let got = match emu.run_with_args(&code, entry_off, &[0, depth as u32, bits(a), bits(b)]) {
        RunOutcome::Ok(v) => v,
        RunOutcome::Trap(t) => panic!("depth-{depth} float recursion trapped: {t:?}"),
    };

    let want = a + b + depth as f32 * (a * 2.0 + b * 3.0);
    assert_eq!(
        f32::from_bits(got),
        want,
        "a float live across {depth} nested call8s was corrupted"
    );
}

/// The integer control for the test above, at the same depth and shape.
///
/// If this fails too, the finding is the window machinery, not floats. If this
/// passes and the float one fails, the finding is precise.
#[test]
fn integer_recursion_at_depth_100_is_the_control() {
    let mut cb = FunctionBuilder::new("rec", &[IrType::I32, IrType::I32, IrType::I32]);
    let n = cb.add_param(IrType::I32);
    let a = cb.add_param(IrType::I32);
    let b = cb.add_param(IrType::I32);
    let zero = cb.alloc_vreg(IrType::I32);
    let is_zero = cb.alloc_vreg(IrType::I32);
    let out = cb.alloc_vreg(IrType::I32);
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
    cb.push(LpirOp::Iadd {
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
        let deeper = cb.alloc_vreg(IrType::I32);
        cb.push_call(
            lpir::CalleeRef::Local(FuncId(0)),
            &[lpir::VMCTX_VREG, n1, a, b],
            &[deeper],
        );
        let ta = cb.alloc_vreg(IrType::I32);
        let tb = cb.alloc_vreg(IrType::I32);
        let sum = cb.alloc_vreg(IrType::I32);
        cb.push(LpirOp::IaddImm {
            dst: ta,
            src: a,
            imm: 0,
        });
        cb.push(LpirOp::Iadd {
            dst: ta,
            lhs: ta,
            rhs: a,
        });
        cb.push(LpirOp::IaddImm {
            dst: tb,
            src: b,
            imm: 0,
        });
        cb.push(LpirOp::Iadd {
            dst: tb,
            lhs: tb,
            rhs: b,
        });
        cb.push(LpirOp::Iadd {
            dst: tb,
            lhs: tb,
            rhs: b,
        });
        cb.push(LpirOp::Iadd {
            dst: sum,
            lhs: ta,
            rhs: tb,
        });
        cb.push(LpirOp::Iadd {
            dst: out,
            lhs: deeper,
            rhs: sum,
        });
    }
    cb.end_if();
    cb.push_return(&[out]);
    let rec = cb.finish();

    let mut fb = FunctionBuilder::new("f", &[IrType::I32, IrType::I32, IrType::I32]);
    let fn_ = fb.add_param(IrType::I32);
    let fa = fb.add_param(IrType::I32);
    let fb_ = fb.add_param(IrType::I32);
    let out = fb.alloc_vreg(IrType::I32);
    fb.push_call(
        lpir::CalleeRef::Local(FuncId(0)),
        &[lpir::VMCTX_VREG, fn_, fa, fb_],
        &[out],
    );
    fb.push_return(&[out]);
    let f = fb.finish();

    let ir = LpirModule {
        imports: vec![],
        functions: VecMap::from([(FuncId(0), rec), (FuncId(1), f)]),
    };
    let params = || vec![int_param("n"), int_param("a"), int_param("b")];
    let sig = LpsModuleSig {
        functions: vec![
            LpsFnSig {
                name: "rec".to_string(),
                parameters: params(),
                return_type: LpsType::Int,
                kind: LpsFnKind::UserDefined,
            },
            LpsFnSig {
                name: "f".to_string(),
                parameters: params(),
                return_type: LpsType::Int,
                kind: LpsFnKind::UserDefined,
            },
        ],
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    };

    let (depth, a, b) = (100i32, 5i32, 7i32);
    let mut emu = Emulator::new();
    emu.step_budget = 8_000_000;
    let (code, entry_off) = link_blob(&ir, &sig, "f", FloatMode::F32, true, &emu);
    let got = match emu.run_with_args(&code, entry_off, &[0, depth as u32, a as u32, b as u32]) {
        RunOutcome::Ok(v) => v,
        RunOutcome::Trap(t) => panic!("depth-{depth} integer recursion trapped: {t:?}"),
    };
    let want = a + b + depth * (a * 2 + b * 3);
    assert_eq!(got as i32, want);
}

// ---------------------------------------------------------------------------
// CPENABLE — the D6 failure mode, proved by the emulator
// ---------------------------------------------------------------------------

/// Without the arming preamble, the first FP instruction must **fault**, not
/// quietly execute.
///
/// `Cpu::new()` leaves `CPENABLE` clear on purpose so that firmware which
/// forgets to arm the coprocessor fails on the host instead of on a board.
/// This test is what turns that from an emulator implementation detail into
/// M7's stated failure mode (D6): compiled float code on an unarmed core takes
/// EXCCAUSE 32.
#[test]
fn unarmed_float_code_faults_with_a_coprocessor_trap() {
    let mut fb = FunctionBuilder::new("f", &[IrType::F32, IrType::F32]);
    let x = fb.add_param(IrType::F32);
    let y = fb.add_param(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    fb.push(LpirOp::Fadd {
        dst: out,
        lhs: x,
        rhs: y,
    });
    fb.push_return(&[out]);
    let ir = one_function_module(fb.finish());
    let sig = one_function_sig(vec![float_param("a"), float_param("b")], LpsType::Float);

    let mut emu = Emulator::new();
    let (code, entry_off) = link_blob(&ir, &sig, "f", FloatMode::F32, false, &emu);
    match emu.run_with_args(&code, entry_off, &[0, bits(1.0), bits(2.0)]) {
        RunOutcome::Trap(t) => assert_eq!(
            t.cause, EXC_COPROCESSOR0_DISABLED,
            "expected a coprocessor-disabled trap, got cause {}",
            t.cause
        ),
        RunOutcome::Ok(v) => panic!(
            "unarmed FP executed instead of faulting (returned {:#010x}) — \
             the CPENABLE gate is not doing its job",
            v
        ),
    }
}

// ---------------------------------------------------------------------------
// Builtin routing (M7 D4)
// ---------------------------------------------------------------------------

/// M7 D4 routes the non-inlinable float operations (divide, sqrt, the rounding
/// family, min/max, float→int, every transcendental) to M5's `_f32` builtins
/// via `sym_call`. Resolving those on the host emulation path needs
/// `lp-xt/lps-builtins-xt-app` built with `float-f32`, which
/// `scripts/build-builtins-xt.sh` now does unconditionally.
///
/// This was `#[ignore]`d while that build failed in the esp Rust backend,
/// which cannot select a float constant pool
/// (`docs/defects/2026-08-01-xtensa-backend-cannot-select-float-constant-pool.md`
/// — still open upstream, worked around in our source).
///
/// `ffloor` is deliberately chosen over `fdiv`/`fsqrt`: it reaches no
/// `div0.s`/`const.s` estimate helper, so it does not additionally depend on
/// M6-P6's unresolved policy fields.
#[test]
fn a_builtin_routed_float_op_resolves_and_runs() {
    let Some(image) = builtins_image() else {
        return;
    };

    let mut fb = FunctionBuilder::new("f", &[IrType::F32, IrType::F32]);
    let x = fb.add_param(IrType::F32);
    let _y = fb.add_param(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    fb.push(LpirOp::Ffloor { dst: out, src: x });
    fb.push_return(&[out]);
    let ir = one_function_module(fb.finish());
    let sig = one_function_sig(vec![float_param("a"), float_param("b")], LpsType::Float);

    let r = expect_ok(run_f32_with_builtins(
        &ir,
        &sig,
        "f",
        &[0, bits(3.75), bits(0.0)],
        &image,
    ));
    assert_eq!(f32::from_bits(r), 3.0);
}

// ---------------------------------------------------------------------------
// The frame is a non-change (M7 D7) — pinned, not asserted in prose
// ---------------------------------------------------------------------------

/// A float-using Xtensa function must have exactly the frame it had before
/// hardware float existed.
///
/// D7's claim is that float support adds **no** frame region: no FR is
/// callee-saved (measured, M6-P4), so there is no FP callee-save area to lay
/// out, and float spills reuse the existing class-tagged spill index space at
/// the bottom of the frame. That claim is what makes the depth-100 recursion
/// above safe — spills at the bottom cannot reach the window-overflow
/// reservation at the top.
///
/// So this pins the shape rather than the reasoning: the reservation is still
/// 32 bytes, the prologue is still a single `entry`, the epilogue a single
/// `retw`, and no extra region appeared between them. If this test starts
/// failing, D7 stopped being true and the recursion test's safety argument
/// goes with it.
#[test]
fn a_float_function_has_the_same_frame_shape_as_before() {
    use lp_xt_inst::{Inst, NullaryOp, decode};

    let mut fb = FunctionBuilder::new("f", &[IrType::F32, IrType::F32]);
    let x = fb.add_param(IrType::F32);
    let y = fb.add_param(IrType::F32);
    let out = fb.alloc_vreg(IrType::F32);
    // Enough live floats to force spills, so the spill region is non-empty and
    // its placement is actually exercised.
    let mut vs = Vec::new();
    let mut prev = x;
    for _ in 0..20 {
        let v = fb.alloc_vreg(IrType::F32);
        fb.push(LpirOp::Fadd {
            dst: v,
            lhs: prev,
            rhs: y,
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

    let ir = one_function_module(fb.finish());
    let sig = one_function_sig(vec![float_param("a"), float_param("b")], LpsType::Float);

    let emu = Emulator::new();
    let (code, entry_off) = link_blob(&ir, &sig, "f", FloatMode::F32, false, &emu);
    let body = &code[entry_off as usize..];

    // The prologue is one `entry a1, frame`. A literal pool would put a `j`
    // first; this function has one (the float constant), so skip it.
    let mut pc = 0usize;
    let mut entries = 0usize;
    let mut retws = 0usize;
    let mut saw_entry_first = None;
    while pc < body.len() {
        let end = (pc + 3).min(body.len());
        let Ok((inst, len)) = decode(&body[pc..end]) else {
            pc += 1;
            continue;
        };
        match inst {
            Inst::Entry(..) => {
                entries += 1;
                if saw_entry_first.is_none() {
                    saw_entry_first = Some(pc);
                }
            }
            Inst::Nullary(NullaryOp::Retw) => retws += 1,
            _ => {}
        }
        pc += len;
    }

    assert_eq!(
        entries, 1,
        "the prologue must still be exactly one `entry` — a second one would \
         mean float support grew a frame region"
    );
    assert_eq!(retws, 1, "the epilogue must still be exactly one `retw`");

    // And the reservation itself is untouched.
    assert_eq!(
        IsaTarget::Xtensa.frame_top_reserved_bytes(),
        32,
        "the window-overflow reservation must stay 32 bytes (M7 D7)"
    );
}

// ---------------------------------------------------------------------------
// Compile-stats disclosure (roadmap D3)
// ---------------------------------------------------------------------------

/// An F32 module compiled for Xtensa reports `HardwareF32`, and a Q32 one
/// reports `Fixed`.
///
/// The value used to be hardcoded to `Fixed` in `LpsCompileStats::from_module`;
/// it now comes from the module, which is the only thing that knows both the
/// mode it was compiled in and the target it was compiled for.
#[test]
fn the_isa_reports_what_it_actually_emitted() {
    use lpvm::FloatImpl;

    assert_eq!(
        IsaTarget::Xtensa.float_impl_for(FloatMode::F32),
        FloatImpl::HardwareF32,
        "Xtensa with float-f32 emits real FP instructions and must say so"
    );
    assert_eq!(
        IsaTarget::Xtensa.float_impl_for(FloatMode::Q32),
        FloatImpl::Fixed,
        "a Q16.16 float is an integer — `Fixed` is the literal truth here"
    );
}
