//! JIT smoke for [`LpvmInstance::call_render_texture`](lpvm::LpvmInstance::call_render_texture).

use alloc::string::String;

use lpir::builder::{FunctionBuilder, ModuleBuilder};
use lpir::{IrType, LpirOp};
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};

use crate::{CompileOptions, CraneliftEngine, FloatMode};

#[test]
fn render_texture_smoke_writes_pixel() {
    let mut fb = FunctionBuilder::new("__render_texture_smoke", &[]);
    let tex = fb.add_param(IrType::Pointer);
    let _w = fb.add_param(IrType::I32);
    let _h = fb.add_param(IrType::I32);
    let v = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: v,
        value: 0xFFFF,
    });
    fb.push(LpirOp::Store16 {
        base: tex,
        offset: 0,
        value: v,
    });
    fb.push_return(&[]);

    let mut mb = ModuleBuilder::new();
    mb.add_function(fb.finish());
    let ir = mb.finish();

    let meta = LpsModuleSig {
        functions: alloc::vec![LpsFnSig {
            name: String::from("__render_texture_smoke"),
            return_type: LpsType::Void,
            parameters: alloc::vec![
                FnParam {
                    name: String::from("tex"),
                    ty: LpsType::UInt,
                    qualifier: ParamQualifier::In,
                },
                FnParam {
                    name: String::from("w"),
                    ty: LpsType::Int,
                    qualifier: ParamQualifier::In,
                },
                FnParam {
                    name: String::from("h"),
                    ty: LpsType::Int,
                    qualifier: ParamQualifier::In,
                },
            ],
            kind: LpsFnKind::Synthetic,
        }],
        ..Default::default()
    };

    let engine = CraneliftEngine::new(CompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    });
    let module = engine.compile(&ir, &meta).expect("compile smoke module");
    let mut instance = module.instantiate().expect("instantiate");

    let mut buffer = engine.memory().alloc(16, 8).expect("alloc texture buffer");
    instance
        .call_render_texture("__render_texture_smoke", &mut buffer, 1, 1)
        .expect("call_render_texture");

    unsafe {
        let p = buffer.native_ptr();
        let got = core::slice::from_raw_parts(p, 2);
        assert_eq!(got, &[0xFF, 0xFF]);
    }
}
