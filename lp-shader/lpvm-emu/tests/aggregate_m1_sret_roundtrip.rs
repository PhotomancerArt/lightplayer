//! M1 aggregate `in` + sret return through [`lpvm_emu::EmuInstance`].

use lps_frontend::{compile, lower};
use lps_shared::lps_value_f32::LpsValueF32;
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_emu::{CompileOptions, EmuEngine};

#[test]
fn emu_mul2_float4_array_in_and_sret_return() {
    let glsl = r#"
float[4] mul2_float4(in float[4] a) {
    float[4] o;
    o[0] = a[0] * 2.0;
    o[1] = a[1] * 2.0;
    o[2] = a[2] * 2.0;
    o[3] = a[3] * 2.0;
    return o;
}
"#;
    let naga = compile(glsl).expect("parse glsl");
    let (ir, meta) = lower(&naga).expect("lower");

    let engine = EmuEngine::new(CompileOptions::default());
    let module = engine.compile(&ir, &meta).expect("compile+link");
    let mut inst = module.instantiate().expect("instantiate");

    let arg = LpsValueF32::Array(Box::new([
        LpsValueF32::F32(1.0),
        LpsValueF32::F32(2.0),
        LpsValueF32::F32(3.0),
        LpsValueF32::F32(4.0),
    ]));
    let out = inst.call("mul2_float4", &[arg]).expect("call mul2_float4");

    let LpsValueF32::Array(elems) = out else {
        panic!("expected float[4] array return, got {out:?}");
    };
    assert_eq!(elems.len(), 4);
    for (i, expect) in [(0, 2.0f32), (1, 4.0), (2, 6.0), (3, 8.0)] {
        let LpsValueF32::F32(x) = &elems[i] else {
            panic!("expected f32 at [{i}]");
        };
        assert!(
            (*x - expect).abs() < 1e-3,
            "elem[{i}] want {expect} got {x}"
        );
    }
}
