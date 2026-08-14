//! Regression: loop-carried vregs numbered >= 256 must not lose their
//! store-after-def / spill-slot preassignment.
//!
//! `RegSet` was a fixed 256-bit bitset that SILENTLY IGNORED inserts and
//! queries of vregs >= `MAX_VREGS`(256), while nothing in the pipeline caps
//! vreg ids (`TempVRegs::mint` and `FunctionBuilder::alloc_vreg` both mint
//! past 256; `NativeError::TooManyVRegs` has no producer). The loop-carried
//! preassignment in `regalloc/walk.rs` (`Region::Loop` arm) intersects
//! RegSet-based `live_in` with `defs_in_region` — a loop-carried vreg
//! numbered >= 256 silently vanished from both sets, so its def inside the
//! loop body got `Alloc::None` (value discarded) and every iteration after
//! the first read a stale value from the boundary reload.
//!
//! Both tests run the REAL pipeline (LPIR -> lower -> regalloc -> isa/xt
//! emit) and execute on `lp-xt-emu`, the silicon-verified oracle that is a
//! default-features dev-dependency (same rig as `xt_pipeline.rs`; the
//! regalloc walk under test is ISA-independent). The `low_vreg` control
//! proves the rig itself computes the right answer when ids stay below 256;
//! `high_vreg` is the regression case with the accumulator minted past 256.

use lp_collection::VecMap;
use lpir::builder::FunctionBuilder;
use lpir::{FloatMode, FuncId, IrType, LpirModule, LpirOp};
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
use lpvm_native::compile::compile_module;
use lpvm_native::isa::IsaTarget;
use lpvm_native::native_options::NativeCompileOptions;

use lp_xt_emu::{Emulator, RunOutcome};

/// Compile `(ir, sig)` for Xtensa, link at the emulator's I-bus code base,
/// and run `f(x)`. Mirrors `xt_pipeline.rs::compile_link_run` (fuel off: the
/// emulator is driven with `vmctx = 0`, so fuel would fault on the vmctx
/// header before the loop ever ran).
fn compile_and_run(ir: &LpirModule, sig: &LpsModuleSig, x: i32) -> i32 {
    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        fuel: false,
        ..Default::default()
    };
    let module = compile_module(ir, sig, FloatMode::Q32, opts, IsaTarget::Xtensa)
        .expect("xt compile should succeed");

    let mut code = Vec::new();
    let mut entries = VecMap::<String, usize>::new();
    for f in &module.functions {
        entries.insert(f.name.clone(), code.len());
        code.extend_from_slice(&f.code);
    }
    assert!(
        module.functions.iter().all(|f| f.relocs.is_empty()),
        "single-function module should need no relocs"
    );

    let mut emu = Emulator::new();
    let entry_off = *entries.get("f").expect("entry function exists") as u32;
    match emu.run_with_args(&code, entry_off, &[0, x as u32]) {
        RunOutcome::Ok(v) => v as i32,
        RunOutcome::Trap(t) => panic!("unexpected trap: {t:?}"),
    }
}

/// Build `f(x) = pad_chain(x) + sum` where `pad` extra vregs are minted
/// BEFORE the loop's accumulator, and the loop runs `iters` times adding `x`
/// to the accumulator each iteration:
///
/// ```text
/// cur = x + 1 + 1 + ... (pad times)      // pushes vreg ids upward
/// acc = 0; i = 0;
/// loop { if !(i < iters) break; acc = acc + x; i = i + 1; }
/// return acc + cur;                       // = iters*x + x + pad
/// ```
///
/// With `pad` large enough, `acc` and `i` (both loop-carried: live into the
/// body AND redefined inside it) get vreg ids >= 256.
fn loop_module(pad: usize, iters: i32) -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let x = fb.add_param(IrType::I32);

    let one = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: one, value: 1 });

    // Serial dependency chain on the runtime parameter: not const-foldable,
    // and every link stays live until the next, so the ids really are minted.
    let mut cur = x;
    for _ in 0..pad {
        let next = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::Iadd {
            dst: next,
            lhs: cur,
            rhs: one,
        });
        cur = next;
    }

    let acc = fb.alloc_vreg(IrType::I32);
    let i = fb.alloc_vreg(IrType::I32);
    let n = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: acc, value: 0 });
    fb.push(LpirOp::IconstI32 { dst: i, value: 0 });
    fb.push(LpirOp::IconstI32 {
        dst: n,
        value: iters,
    });

    fb.push_loop();
    let cond = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IltS {
        dst: cond,
        lhs: i,
        rhs: n,
    });
    fb.push(LpirOp::BrIfNot { cond });
    fb.push(LpirOp::Iadd {
        dst: acc,
        lhs: acc,
        rhs: x,
    });
    fb.push(LpirOp::Iadd {
        dst: i,
        lhs: i,
        rhs: one,
    });
    fb.end_loop();

    let ret = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::Iadd {
        dst: ret,
        lhs: acc,
        rhs: cur,
    });
    fb.push_return(&[ret]);

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

/// Highest vreg id the module actually carries — the test's premise check.
fn max_vreg_id(ir: &LpirModule) -> u32 {
    ir.functions
        .values()
        .map(|f| f.vreg_types.len() as u32)
        .max()
        .unwrap_or(0)
}

/// Control: identical program shape with every vreg id below 256. Proves the
/// rig and the expected-value formula before the regression case leans on it.
#[test]
fn loop_carried_accumulator_below_256_vregs() {
    let (ir, sig) = loop_module(100, 4);
    assert!(
        max_vreg_id(&ir) < 256,
        "control case must stay below 256 vregs, got {}",
        max_vreg_id(&ir)
    );
    let x = 5;
    assert_eq!(compile_and_run(&ir, &sig, x), 4 * x + x + 100);
}

/// Regression: the same loop with its carried accumulator minted past 256.
/// Before the RegSet fix this HUNG (emulator timeout trap): the loop counter
/// `i`, numbered >= 256, was dropped from the loop-carried preassignment, its
/// `i = i + 1` def was assigned `Alloc::None`, the increment was discarded,
/// and `i < iters` stayed true forever. On a device this is a wedged render
/// task, not a wrong pixel.
#[test]
fn loop_carried_accumulator_above_256_vregs() {
    let (ir, sig) = loop_module(300, 4);
    assert!(
        max_vreg_id(&ir) > 256,
        "regression case must exceed 256 vregs, got {}",
        max_vreg_id(&ir)
    );
    let x = 5;
    assert_eq!(compile_and_run(&ir, &sig, x), 4 * x + x + 300);
}
