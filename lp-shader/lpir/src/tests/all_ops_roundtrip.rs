//! Module that exercises every [`crate::lpir_op::LpirOp`] variant for print/parse round-trip tests.

use crate::builder::{FunctionBuilder, ModuleBuilder};
use crate::lpir_module::{LpirModule, VMCTX_VREG};
use crate::lpir_op::LpirOp;
use crate::types::IrType;

pub(crate) fn module_all_ops() -> LpirModule {
    let mut mb = ModuleBuilder::new();
    let mut id_b = FunctionBuilder::new("id_i32", &[IrType::I32]);
    let id_arg = id_b.add_param(IrType::I32);
    id_b.push_return(&[id_arg]);
    let id_callee = mb.add_function(id_b.finish());

    let mut b = FunctionBuilder::new(
        "all_ops",
        &[
            IrType::F32,
            IrType::I32,
            IrType::I32,
            IrType::I32,
            IrType::I32,
        ],
    );

    let f1 = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FconstF32 {
        dst: f1,
        value: 1.0,
    });
    let f2 = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FconstF32 {
        dst: f2,
        value: 2.0,
    });

    let i0 = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: i0, value: 0 });
    let i1 = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: i1, value: 1 });
    let i2 = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: i2, value: 2 });
    let i3 = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: i3, value: 3 });
    let im1 = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 {
        dst: im1,
        value: -1,
    });

    macro_rules! fop {
        ($op:ident) => {{
            let d = b.alloc_vreg(IrType::F32);
            b.push(LpirOp::$op {
                dst: d,
                lhs: f1,
                rhs: f2,
            });
        }};
    }
    fop!(Fadd);
    fop!(Fsub);
    fop!(Fmul);
    fop!(Fdiv);
    let fneg_d = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::Fneg {
        dst: fneg_d,
        src: f1,
    });

    macro_rules! unary_f {
        ($op:ident, $src:expr) => {{
            let d = b.alloc_vreg(IrType::F32);
            b.push(LpirOp::$op { dst: d, src: $src });
        }};
    }
    unary_f!(Fabs, f1);
    unary_f!(Fsqrt, f1);
    fop!(Fmin);
    fop!(Fmax);
    unary_f!(Ffloor, f1);
    unary_f!(Fceil, f1);
    unary_f!(Ftrunc, f1);
    unary_f!(Fnearest, f1);

    macro_rules! fcmp {
        ($op:ident) => {{
            let d = b.alloc_vreg(IrType::I32);
            b.push(LpirOp::$op {
                dst: d,
                lhs: f1,
                rhs: f2,
            });
        }};
    }
    fcmp!(Feq);
    fcmp!(Fne);
    fcmp!(Flt);
    fcmp!(Fle);
    fcmp!(Fgt);
    fcmp!(Fge);

    macro_rules! iop {
        ($op:ident) => {{
            let d = b.alloc_vreg(IrType::I32);
            b.push(LpirOp::$op {
                dst: d,
                lhs: i2,
                rhs: i3,
            });
        }};
    }
    iop!(Iadd);
    iop!(Isub);
    iop!(Imul);
    iop!(IdivS);
    iop!(IdivU);
    iop!(IremS);
    iop!(IremU);
    let ineg_d = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::Ineg {
        dst: ineg_d,
        src: i1,
    });

    iop!(Ieq);
    iop!(Ine);
    iop!(IltS);
    iop!(IleS);
    iop!(IgtS);
    iop!(IgeS);
    iop!(IltU);
    iop!(IleU);
    iop!(IgtU);
    iop!(IgeU);

    iop!(Iand);
    iop!(Ior);
    iop!(Ixor);
    let ibnot_d = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::Ibnot {
        dst: ibnot_d,
        src: i1,
    });
    iop!(Ishl);
    iop!(IshrS);
    iop!(IshrU);

    macro_rules! imm {
        ($op:ident, $imm:expr) => {{
            let d = b.alloc_vreg(IrType::I32);
            b.push(LpirOp::$op {
                dst: d,
                src: i2,
                imm: $imm,
            });
        }};
    }
    imm!(IaddImm, 1);
    imm!(IsubImm, 1);
    imm!(ImulImm, 2);
    imm!(IshlImm, 1);
    imm!(IshrSImm, 1);
    imm!(IshrUImm, 1);
    imm!(IeqImm, 2);

    let fts = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::FtoiSatS { dst: fts, src: f1 });
    let ftu = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::FtoiSatU { dst: ftu, src: f2 });
    let itfs = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::ItofS { dst: itfs, src: i1 });
    let itfu = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::ItofU { dst: itfu, src: i2 });
    let fbits_d = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FfromI32Bits {
        dst: fbits_d,
        src: i1,
    });
    let fto16 = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::FtoUnorm16 {
        dst: fto16,
        src: f1,
    });
    let fto8 = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::FtoUnorm8 { dst: fto8, src: f2 });
    let u16f = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::Unorm16toF { dst: u16f, src: i2 });
    let u8f = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::Unorm8toF { dst: u8f, src: i2 });

    let sel_c = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 {
        dst: sel_c,
        value: 1,
    });
    let sel_t = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FconstF32 {
        dst: sel_t,
        value: 5.0,
    });
    let sel_f = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FconstF32 {
        dst: sel_f,
        value: 6.0,
    });
    let sel_d = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::Select {
        dst: sel_d,
        cond: sel_c,
        if_true: sel_t,
        if_false: sel_f,
    });

    let cpy_d = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::Copy {
        dst: cpy_d,
        src: f1,
    });

    let slot = b.alloc_slot(16);
    let base = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::SlotAddr { dst: base, slot });
    b.push(LpirOp::Store {
        base,
        offset: 0,
        value: f1,
    });

    // Narrow memory: distinct offsets in the 16-byte slot (see table in phase-5 plan).
    let v_8u = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 {
        dst: v_8u,
        value: 0xAB,
    });
    b.push(LpirOp::Store8 {
        base,
        offset: 4,
        value: v_8u,
    });
    let r_8u = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::Load8U {
        dst: r_8u,
        base,
        offset: 4,
    });

    let v_8s = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 {
        dst: v_8s,
        value: 0x80,
    });
    b.push(LpirOp::Store8 {
        base,
        offset: 5,
        value: v_8s,
    });
    let r_8s = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::Load8S {
        dst: r_8s,
        base,
        offset: 5,
    });

    let v_16u = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 {
        dst: v_16u,
        value: 0xABCD,
    });
    b.push(LpirOp::Store16 {
        base,
        offset: 8,
        value: v_16u,
    });
    let r_16u = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::Load16U {
        dst: r_16u,
        base,
        offset: 8,
    });

    let v_16s = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 {
        dst: v_16s,
        value: 0x8000,
    });
    b.push(LpirOp::Store16 {
        base,
        offset: 10,
        value: v_16s,
    });
    let r_16s = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::Load16S {
        dst: r_16s,
        base,
        offset: 10,
    });

    let loaded = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::Load {
        dst: loaded,
        base,
        offset: 0,
    });
    let base2 = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::SlotAddr { dst: base2, slot });
    b.push(LpirOp::Memcpy {
        dst_addr: base2,
        src_addr: base,
        size: 16,
    });

    b.push_if(sel_c);
    let ifv = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FconstF32 {
        dst: ifv,
        value: 7.0,
    });
    b.push_else();
    let elsv = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FconstF32 {
        dst: elsv,
        value: 8.0,
    });
    b.end_if();

    b.push_loop();
    b.push(LpirOp::BrIfNot { cond: i0 });
    b.push(LpirOp::Continue);
    b.end_loop();

    b.push_loop();
    b.push(LpirOp::Break);
    b.end_loop();

    b.push_switch(i1);
    b.push_case(0);
    let z0 = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: z0, value: 0 });
    b.end_switch_arm();
    b.push_case(1);
    let z1 = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: z1, value: 42 });
    b.end_switch_arm();
    b.push_default();
    let zd = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: zd, value: -1 });
    b.end_switch_arm();
    b.end_switch();

    b.push_block();
    b.push_exit_block();
    b.end_block();

    let call_out = b.alloc_vreg(IrType::I32);
    b.push_call(id_callee, &[VMCTX_VREG, i2], &[call_out]);

    b.push_return(&[f1, r_8u, r_8s, r_16u, r_16s]);

    mb.add_function(b.finish());
    mb.finish()
}

#[cfg(test)]
mod all_ops_exec {
    use alloc::vec::Vec;

    use super::module_all_ops;
    use crate::interp::{ImportHandler, InterpError, Value, interpret};
    use crate::validate::validate_module;

    struct NoImports;

    impl ImportHandler for NoImports {
        fn call(
            &mut self,
            _module_name: &str,
            _func_name: &str,
            _args: &[Value],
        ) -> Result<Vec<Value>, InterpError> {
            Err(InterpError::Import(alloc::string::String::from(
                "no imports in all_ops fixture",
            )))
        }
    }

    /// Interpreter reference for narrow mem return values (backends should match).
    #[test]
    fn interp_all_ops_narrow_memory_returns() {
        let m = module_all_ops();
        validate_module(&m).unwrap();
        let mut imp = NoImports;
        // `all_ops` has only implicit vmctx; `f1` comes from `fconst.f32 1.0` in the body.
        let out = interpret(&m, "all_ops", &[], &mut imp).unwrap();
        assert_eq!(out.len(), 5);
        assert!((out[0].as_f32().unwrap() - 1.0).abs() < 1e-6);
        assert_eq!(out[1], Value::I32(0xAB));
        assert_eq!(out[2], Value::I32(-128));
        assert_eq!(out[3], Value::I32(0xABCD));
        assert_eq!(out[4], Value::I32(-32768));
    }
}
