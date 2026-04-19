//! Q32Options-driven wasm lowering: wrapping / reciprocal paths vs defaults.

use lpir::CompilerConfig;
use lps_builtins::builtins::lpir::fdiv_recip_q32::__lp_lpir_fdiv_recip_q32;
use lps_frontend::{compile, lower};
use lps_q32::q32_options::{AddSubMode, DivMode, MulMode, Q32Options};
use lpvm::{LpsValueF32, LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;
use lpvm_wasm::{FloatMode, WasmOptions, compile_lpir};

const Q16: f32 = 65536.0;

fn opts_q32(q: Q32Options) -> WasmOptions {
    WasmOptions {
        float_mode: FloatMode::Q32,
        config: CompilerConfig {
            q32: q,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn f32_to_q32_bits(f: f32) -> i32 {
    (f * Q16) as i32
}

fn q32_result_as_i32(out: LpsValueF32) -> i32 {
    let LpsValueF32::F32(f) = out else {
        panic!("expected F32");
    };
    f32_to_q32_bits(f)
}

/// Reference wrapping multiply (matches design doc / native lowering).
fn wrapping_mul_q32(a: i32, b: i32) -> i32 {
    (((a as i64) * (b as i64)) >> 16) as i32
}

#[test]
fn fadd_q32_saturating_unchanged_bytes() {
    let src = "float add(float a, float b) { return a + b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");
    let a = compile_lpir(&ir, &meta, &WasmOptions::default())
        .expect("emit")
        .bytes()
        .to_vec();
    let b = compile_lpir(
        &ir,
        &meta,
        &opts_q32(Q32Options {
            add_sub: AddSubMode::Saturating,
            ..Default::default()
        }),
    )
    .expect("emit")
    .bytes()
    .to_vec();
    assert_eq!(
        a, b,
        "explicit saturating add_sub must match default wasm bytes"
    );
}

#[test]
fn fadd_q32_wrap_vs_sat_runtime() {
    let src = "float add(float a, float b) { return a + b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");

    let sat_engine = WasmLpvmEngine::new(WasmOptions::default()).expect("engine");
    let wrap_engine = WasmLpvmEngine::new(opts_q32(Q32Options {
        add_sub: AddSubMode::Wrapping,
        ..Default::default()
    }))
    .expect("engine");

    let sat_m = sat_engine.compile(&ir, &meta).expect("compile");
    let wrap_m = wrap_engine.compile(&ir, &meta).expect("compile");

    let mut sat_i = sat_m.instantiate().expect("instantiate");
    let mut wrap_i = wrap_m.instantiate().expect("instantiate");

    // 16384_q + 16384_q => i32 add wraps to 0x80000000; saturating clamps to MAX_FIXED.
    let out_sat = sat_i
        .call(
            "add",
            &[LpsValueF32::F32(16384.0), LpsValueF32::F32(16384.0)],
        )
        .expect("call");
    let out_wrap = wrap_i
        .call(
            "add",
            &[LpsValueF32::F32(16384.0), LpsValueF32::F32(16384.0)],
        )
        .expect("call");

    assert!(
        q32_result_as_i32(out_sat) > 0x7FFF_0000,
        "saturating add should clamp high"
    );
    assert_eq!(
        q32_result_as_i32(out_wrap),
        0x8000_0000u32 as i32,
        "wrapping add should use i32.add"
    );
}

#[test]
fn fsub_q32_wrap_vs_sat_runtime() {
    let src = "float sub(float a, float b) { return a - b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");

    let sat_engine = WasmLpvmEngine::new(WasmOptions::default()).expect("engine");
    let wrap_engine = WasmLpvmEngine::new(opts_q32(Q32Options {
        add_sub: AddSubMode::Wrapping,
        ..Default::default()
    }))
    .expect("engine");

    let mut sat_i = sat_engine
        .compile(&ir, &meta)
        .expect("compile")
        .instantiate()
        .expect("i");
    let mut wrap_i = wrap_engine
        .compile(&ir, &meta)
        .expect("compile")
        .instantiate()
        .expect("i");

    // Large positive minus large negative overflows i32 fixed range.
    let out_sat = sat_i
        .call(
            "sub",
            &[LpsValueF32::F32(16384.0), LpsValueF32::F32(-16384.0)],
        )
        .expect("call");
    let out_wrap = wrap_i
        .call(
            "sub",
            &[LpsValueF32::F32(16384.0), LpsValueF32::F32(-16384.0)],
        )
        .expect("call");

    assert!(
        q32_result_as_i32(out_sat) > 0x7FFF_0000,
        "saturating sub should clamp"
    );
    assert_eq!(
        q32_result_as_i32(out_wrap),
        0x8000_0000u32 as i32,
        "wrapping sub should use i32.sub (0x4000_0000 - (-0x4000_0000) wraps)"
    );
}

#[test]
fn fmul_q32_wrap_matches_reference_arith() {
    let src = "float mul(float a, float b) { return a * b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");

    let engine = WasmLpvmEngine::new(opts_q32(Q32Options {
        mul: MulMode::Wrapping,
        ..Default::default()
    }))
    .expect("engine");
    let mut inst = engine
        .compile(&ir, &meta)
        .expect("compile")
        .instantiate()
        .expect("i");

    let cases: &[(f32, f32)] = &[(3.0, 4.0), (-2.0, 5.5), (100.0, 0.125), (0.25, 0.5)];
    for &(af, bf) in cases {
        let ai = f32_to_q32_bits(af);
        let bi = f32_to_q32_bits(bf);
        let expected = wrapping_mul_q32(ai, bi);
        let got = q32_result_as_i32(
            inst.call("mul", &[LpsValueF32::F32(af), LpsValueF32::F32(bf)])
                .expect("call"),
        );
        assert_eq!(
            got, expected,
            "fmul wrap mismatch for ({af}, {bf}) fixed=({ai:#x},{bi:#x})"
        );
    }
}

#[test]
fn fdiv_recip_matches_native_helper_runtime() {
    let src = "float div(float a, float b) { return a / b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");

    let engine = WasmLpvmEngine::new(opts_q32(Q32Options {
        div: DivMode::Reciprocal,
        ..Default::default()
    }))
    .expect("engine");
    let mut inst = engine
        .compile(&ir, &meta)
        .expect("compile")
        .instantiate()
        .expect("i");

    let cases: &[(f32, f32)] = &[
        (10.0, 2.0),
        (-10.0, 2.0),
        (10.0, -2.0),
        (-10.0, -2.0),
        (1.5, 0.25),
        (0.0, 1.0),
    ];
    for &(af, bf) in cases {
        let ai = f32_to_q32_bits(af);
        let bi = f32_to_q32_bits(bf);
        let expected = __lp_lpir_fdiv_recip_q32(ai, bi);
        let got = q32_result_as_i32(
            inst.call("div", &[LpsValueF32::F32(af), LpsValueF32::F32(bf)])
                .expect("call"),
        );
        assert_eq!(
            got, expected,
            "fdiv recip wasm vs helper for ({af}, {bf}) fixed=({ai:#x},{bi:#x})"
        );
    }

    assert_eq!(
        q32_result_as_i32(
            inst.call("div", &[LpsValueF32::F32(1.0), LpsValueF32::F32(0.0)])
                .expect("call"),
        ),
        __lp_lpir_fdiv_recip_q32(f32_to_q32_bits(1.0), f32_to_q32_bits(0.0))
    );
    assert_eq!(
        q32_result_as_i32(
            inst.call("div", &[LpsValueF32::F32(-1.0), LpsValueF32::F32(0.0)])
                .expect("call"),
        ),
        __lp_lpir_fdiv_recip_q32(f32_to_q32_bits(-1.0), f32_to_q32_bits(0.0))
    );
    assert_eq!(
        q32_result_as_i32(
            inst.call("div", &[LpsValueF32::F32(0.0), LpsValueF32::F32(0.0)])
                .expect("call")
        ),
        __lp_lpir_fdiv_recip_q32(0, 0)
    );
}

#[test]
fn fsub_q32_saturating_unchanged_bytes() {
    let src = "float sub(float a, float b) { return a - b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");
    let a = compile_lpir(&ir, &meta, &WasmOptions::default())
        .expect("emit")
        .bytes()
        .to_vec();
    let b = compile_lpir(
        &ir,
        &meta,
        &opts_q32(Q32Options {
            add_sub: AddSubMode::Saturating,
            ..Default::default()
        }),
    )
    .expect("emit")
    .bytes()
    .to_vec();
    assert_eq!(a, b);
}

#[test]
fn fmul_q32_saturating_unchanged_bytes() {
    let src = "float mul(float a, float b) { return a * b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");
    let a = compile_lpir(&ir, &meta, &WasmOptions::default())
        .expect("emit")
        .bytes()
        .to_vec();
    let b = compile_lpir(
        &ir,
        &meta,
        &opts_q32(Q32Options {
            mul: MulMode::Saturating,
            ..Default::default()
        }),
    )
    .expect("emit")
    .bytes()
    .to_vec();
    assert_eq!(a, b);
}

#[test]
fn fdiv_q32_saturating_unchanged_bytes() {
    let src = "float div(float a, float b) { return a / b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");
    let a = compile_lpir(&ir, &meta, &WasmOptions::default())
        .expect("emit")
        .bytes()
        .to_vec();
    let b = compile_lpir(
        &ir,
        &meta,
        &opts_q32(Q32Options {
            div: DivMode::Saturating,
            ..Default::default()
        }),
    )
    .expect("emit")
    .bytes()
    .to_vec();
    assert_eq!(
        a, b,
        "explicit saturating div must match default wasm bytes"
    );
}
