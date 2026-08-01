//! Two-class allocation: floats in FRs and integers in ARs, in one function.
//!
//! These are the end-to-end assertions for M7 P1. The per-operand class map is
//! unit-tested in [`crate::regalloc::classes`]; what is left to prove is that
//! the map, the ISA's float pool, and the ABI's float lanes agree well enough
//! that the allocator can actually place a float — and that
//! [`verify_operand_classes`](crate::regalloc::verify) rejects it when they do
//! not.
//!
//! Xtensa only. rv32's f32 path is soft float, which keeps every value in the
//! integer file, so its float pool is empty on purpose and a float vreg there
//! is meant to fail.

#![cfg(all(feature = "isa-xt", feature = "float-f32"))]

use alloc::vec::Vec;

use crate::abi::{PReg, RegClass};
use crate::isa::IsaTarget;
use crate::regalloc::test::builder::alloc_test;
use crate::regalloc::{Alloc, AllocOutput};
use crate::vinst::{VInst, VReg};

fn xt() -> crate::regalloc::test::builder::AllocTestBuilder {
    alloc_test().isa(IsaTarget::Xtensa)
}

/// Every physical register the allocation assigned, paired with the class the
/// operand required.
fn allocated_classes(vinsts: &[VInst], pool: &[VReg], out: &AllocOutput) -> Vec<(PReg, RegClass)> {
    use crate::regalloc::classes::{def_class, use_class};
    let mut found = Vec::new();
    for (idx, inst) in vinsts.iter().enumerate() {
        let base = out.inst_alloc_offsets[idx] as usize;
        let mut op = 0usize;
        let mut def_idx = 0usize;
        inst.for_each_def(pool, |_| {
            if let Alloc::Reg(p) = out.allocs[base + op] {
                found.push((p.get(), def_class(inst, def_idx)));
            }
            op += 1;
            def_idx += 1;
        });
        let mut use_idx = 0usize;
        inst.for_each_use(pool, |_| {
            if let Alloc::Reg(p) = out.allocs[base + op] {
                found.push((p.get(), use_class(inst, use_idx)));
            }
            op += 1;
            use_idx += 1;
        });
    }
    found
}

/// The headline: one function that allocates from **both** pools, and
/// `verify_alloc` — which includes `verify_operand_classes` — accepts it.
///
/// The shape is the calling convention in miniature (M7 D1/D2): a bit pattern
/// materialized in an address register, transferred in with `Wfr`, multiplied
/// in the float file, and transferred back out with `Rfr` before the return.
#[test]
fn floats_and_ints_allocate_from_their_own_pools_in_one_function() {
    let r = xt().run_vinst(
        "i0 = IConst32 1065353216
         i1 = Wfr i0
         i2 = FMul i1, i1
         i3 = Rfr i2
         Ret i3",
    );
    // `run_vinst` already ran `verify_alloc`; reaching here means the class
    // verifier accepted every operand. Now assert it was not vacuous — that a
    // float really did land in an FR.
    let (vinsts, _symbols, pool) = crate::debug::vinst::parse(
        "i0 = IConst32 1065353216\ni1 = Wfr i0\ni2 = FMul i1, i1\ni3 = Rfr i2\nRet i3",
    )
    .unwrap();
    let found = allocated_classes(&vinsts, &pool, &r.output);
    assert!(
        found.iter().any(|(p, _)| p.class == RegClass::Float),
        "no float register was allocated — the test would pass vacuously: {found:?}"
    );
    assert!(
        found.iter().any(|(p, _)| p.class == RegClass::Int),
        "no integer register was allocated: {found:?}"
    );
    for (preg, want) in &found {
        assert_eq!(preg.class, *want, "{preg:?} allocated for a {want:?} operand");
    }
    assert!(
        found
            .iter()
            .all(|(p, _)| p.class != RegClass::Float || p.hw < 16),
        "an FR index outside f0..f15: {found:?}"
    );
    r.expect_spill_slots(0);
}

/// Float pressure past the 16-register file spills — into the ordinary
/// class-tagged spill index space, with no new frame region (M7 D7).
#[test]
fn float_pressure_spills_without_a_new_frame_region() {
    let mut src = alloc::string::String::new();
    // 20 live floats, then one op that reads them all pairwise.
    for i in 0..20 {
        src.push_str(&alloc::format!("i{i} = IConst32 {i}\n"));
    }
    for i in 0..20 {
        src.push_str(&alloc::format!("i{} = Wfr i{i}\n", 100 + i));
    }
    // Sum them, keeping every one live until its turn.
    for i in 1..20 {
        src.push_str(&alloc::format!("i{} = FAdd i100, i{}\n", 200 + i, 100 + i));
    }
    src.push_str("i250 = Rfr i219\nRet i250");
    let r = xt().run_vinst(&src);
    r.expect_spill_slots_at_least(1);
}

/// The negative case: a deliberately wrong class is **rejected**.
///
/// This is the assertion that gives the positive test its meaning. A `Wfr`
/// reads an *address* register; handing it a float register does not crash and
/// does not produce a bad address — the emitter would read an FR, and the IEEE
/// pattern sitting in the AR would never enter the float file. The result is a
/// plausible wrong number, which is precisely why the verifier exists.
#[test]
#[should_panic(expected = "needs a Int register but was allocated to a Float one")]
fn a_float_register_on_wfrs_integer_source_is_rejected() {
    let src = "i0 = IConst32 1065353216\ni1 = Wfr i0\ni2 = Rfr i1\nRet i2";
    let (vinsts, _symbols, pool) = crate::debug::vinst::parse(src).unwrap();
    let r = xt().run_vinst(src);
    let mut output = r.output;

    // `Wfr` is instruction 1; its operands are [def, use].
    let wfr_use = output.inst_alloc_offsets[1] as usize + 1;
    assert!(
        matches!(output.allocs[wfr_use], Alloc::Reg(p) if p.get().class == RegClass::Int),
        "fixture drifted: Wfr's source was not allocated to an integer register"
    );
    output.allocs[wfr_use] = Alloc::reg(PReg::float(3));

    let func_abi = crate::isa::xt::abi::func_abi_xt(
        &lps_shared::LpsFnSig {
            name: alloc::string::String::from("test"),
            return_type: lps_shared::LpsType::Void,
            parameters: Vec::new(),
            kind: lps_shared::LpsFnKind::UserDefined,
        },
        None,
    );
    crate::regalloc::verify::verify_alloc(&vinsts, &pool, &output, &func_abi);
}

/// The mirror: an integer register where a float operand is required.
#[test]
#[should_panic(expected = "needs a Float register but was allocated to a Int one")]
fn an_integer_register_on_a_float_operand_is_rejected() {
    let src = "i0 = IConst32 1065353216\ni1 = Wfr i0\ni2 = Rfr i1\nRet i2";
    let (vinsts, _symbols, pool) = crate::debug::vinst::parse(src).unwrap();
    let r = xt().run_vinst(src);
    let mut output = r.output;

    // `Rfr` is instruction 2; its use must be a float register.
    let rfr_use = output.inst_alloc_offsets[2] as usize + 1;
    output.allocs[rfr_use] = Alloc::reg(PReg::int(5));

    let func_abi = crate::isa::xt::abi::func_abi_xt(
        &lps_shared::LpsFnSig {
            name: alloc::string::String::from("test"),
            return_type: lps_shared::LpsType::Void,
            parameters: Vec::new(),
            kind: lps_shared::LpsFnKind::UserDefined,
        },
        None,
    );
    crate::regalloc::verify::verify_alloc(&vinsts, &pool, &output, &func_abi);
}

/// A call evicts every live float, because no FR survives a `call8` (M6-P4).
/// Under-reporting the clobber set would leave the value in a register the
/// callee overwrites — wrong only for inputs that straddle a call.
#[test]
fn a_call_evicts_live_floats() {
    let r = xt().run_vinst(
        "i0 = IConst32 1065353216
         i1 = Wfr i0
         i2 = Call helper (i0)
         i3 = FMul i1, i1
         i4 = Rfr i3
         Ret i4",
    );
    // The float is live across the call and every FR is clobbered, so it must
    // have been spilled and reloaded.
    r.expect_spill_slots_at_least(1);
}
