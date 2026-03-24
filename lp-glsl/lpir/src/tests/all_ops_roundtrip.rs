//! Module that exercises every [`crate::op::Op`] variant for print/parse round-trip tests.

use crate::builder::{FunctionBuilder, ModuleBuilder};
use crate::module::IrModule;
use crate::op::Op;
use crate::types::IrType;

pub(crate) fn module_all_ops() -> IrModule {
    let mut mb = ModuleBuilder::new();
    let mut id_b = FunctionBuilder::new("id_i32", &[IrType::I32]);
    let id_arg = id_b.add_param(IrType::I32);
    id_b.push_return(&[id_arg]);
    let id_callee = mb.add_function(id_b.finish());

    let mut b = FunctionBuilder::new("all_ops", &[IrType::F32]);

    let f1 = b.alloc_vreg(IrType::F32);
    b.push(Op::FconstF32 {
        dst: f1,
        value: 1.0,
    });
    let f2 = b.alloc_vreg(IrType::F32);
    b.push(Op::FconstF32 {
        dst: f2,
        value: 2.0,
    });

    let i0 = b.alloc_vreg(IrType::I32);
    b.push(Op::IconstI32 { dst: i0, value: 0 });
    let i1 = b.alloc_vreg(IrType::I32);
    b.push(Op::IconstI32 { dst: i1, value: 1 });
    let i2 = b.alloc_vreg(IrType::I32);
    b.push(Op::IconstI32 { dst: i2, value: 2 });
    let i3 = b.alloc_vreg(IrType::I32);
    b.push(Op::IconstI32 { dst: i3, value: 3 });
    let im1 = b.alloc_vreg(IrType::I32);
    b.push(Op::IconstI32 {
        dst: im1,
        value: -1,
    });

    macro_rules! fop {
        ($op:ident) => {{
            let d = b.alloc_vreg(IrType::F32);
            b.push(Op::$op {
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
    b.push(Op::Fneg {
        dst: fneg_d,
        src: f1,
    });

    macro_rules! icmp {
        ($op:ident) => {{
            let d = b.alloc_vreg(IrType::I32);
            b.push(Op::$op {
                dst: d,
                lhs: i1,
                rhs: i2,
            });
        }};
    }
    icmp!(Feq);
    icmp!(Fne);
    icmp!(Flt);
    icmp!(Fle);
    icmp!(Fgt);
    icmp!(Fge);

    macro_rules! iop {
        ($op:ident) => {{
            let d = b.alloc_vreg(IrType::I32);
            b.push(Op::$op {
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
    b.push(Op::Ineg {
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
    b.push(Op::Ibnot {
        dst: ibnot_d,
        src: i1,
    });
    iop!(Ishl);
    iop!(IshrS);
    iop!(IshrU);

    macro_rules! imm {
        ($op:ident, $imm:expr) => {{
            let d = b.alloc_vreg(IrType::I32);
            b.push(Op::$op {
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
    b.push(Op::FtoiSatS { dst: fts, src: f1 });
    let ftu = b.alloc_vreg(IrType::I32);
    b.push(Op::FtoiSatU { dst: ftu, src: f2 });
    let itfs = b.alloc_vreg(IrType::F32);
    b.push(Op::ItofS { dst: itfs, src: i1 });
    let itfu = b.alloc_vreg(IrType::F32);
    b.push(Op::ItofU { dst: itfu, src: i2 });

    let sel_c = b.alloc_vreg(IrType::I32);
    b.push(Op::IconstI32 {
        dst: sel_c,
        value: 1,
    });
    let sel_t = b.alloc_vreg(IrType::F32);
    b.push(Op::FconstF32 {
        dst: sel_t,
        value: 5.0,
    });
    let sel_f = b.alloc_vreg(IrType::F32);
    b.push(Op::FconstF32 {
        dst: sel_f,
        value: 6.0,
    });
    let sel_d = b.alloc_vreg(IrType::F32);
    b.push(Op::Select {
        dst: sel_d,
        cond: sel_c,
        if_true: sel_t,
        if_false: sel_f,
    });

    let cpy_d = b.alloc_vreg(IrType::F32);
    b.push(Op::Copy {
        dst: cpy_d,
        src: f1,
    });

    let slot = b.alloc_slot(16);
    let base = b.alloc_vreg(IrType::I32);
    b.push(Op::SlotAddr { dst: base, slot });
    b.push(Op::Store {
        base,
        offset: 0,
        value: f1,
    });
    let loaded = b.alloc_vreg(IrType::F32);
    b.push(Op::Load {
        dst: loaded,
        base,
        offset: 0,
    });
    let base2 = b.alloc_vreg(IrType::I32);
    b.push(Op::SlotAddr { dst: base2, slot });
    b.push(Op::Memcpy {
        dst_addr: base2,
        src_addr: base,
        size: 16,
    });

    b.push_if(sel_c);
    let ifv = b.alloc_vreg(IrType::F32);
    b.push(Op::FconstF32 {
        dst: ifv,
        value: 7.0,
    });
    b.push_else();
    let elsv = b.alloc_vreg(IrType::F32);
    b.push(Op::FconstF32 {
        dst: elsv,
        value: 8.0,
    });
    b.end_if();

    b.push_loop();
    b.push(Op::BrIfNot { cond: i0 });
    b.push(Op::Continue);
    b.end_loop();

    b.push_loop();
    b.push(Op::Break);
    b.end_loop();

    b.push_switch(i1);
    b.push_case(0);
    let z0 = b.alloc_vreg(IrType::I32);
    b.push(Op::IconstI32 { dst: z0, value: 0 });
    b.end_switch_arm();
    b.push_case(1);
    let z1 = b.alloc_vreg(IrType::I32);
    b.push(Op::IconstI32 { dst: z1, value: 42 });
    b.end_switch_arm();
    b.push_default();
    let zd = b.alloc_vreg(IrType::I32);
    b.push(Op::IconstI32 { dst: zd, value: -1 });
    b.end_switch_arm();
    b.end_switch();

    let call_out = b.alloc_vreg(IrType::I32);
    b.push_call(id_callee, &[i2], &[call_out]);

    b.push_return(&[f1]);

    mb.add_function(b.finish());
    mb.finish()
}
