//! Classic-ESP32 (LX6) memory-model regressions, on the host.
//!
//! The Xtensa *ISA* needs no classic-specific coverage — LX6 ≡ LX7 in the
//! executed instruction set, proven on silicon (multi-board P6: zero
//! divergences, full corpus). What classic changes is the **memory system**:
//! the heap has no I-bus view, and JIT code must be *installed* into a fixed
//! SRAM1 region through a word-mirrored D-bus walk
//! ([`lpvm_native::codemem_esp32`]).
//!
//! These tests run the real compiler pipeline (LPIR → regalloc → `isa/xt`
//! emit → [`link_jit_at`]) and execute the result on `lp-xt-emu` under
//! `BoardProfile::esp32()`, whose alias model is hardware-measured. The
//! flagship test installs the image through the descending mirrored write
//! walk — the exact discipline the device uses — and executes it at the
//! I-bus base, which is the whole classic JIT story minus silicon.
//!
//! Note on the roadmap's Q6 ("run the xtn.q32 corpus under the esp32
//! profile"): the corpus's execution engine loads a builtins base image
//! *linked for the S3 memory map*, so a full classic corpus lane needs a
//! second cross-linked builtins image — real work, not registration (the
//! same premise the 2026-07-29 roadmap's M4 got wrong). Since the ISA is
//! proven identical and only the memory model differs, THIS file is the
//! classic guard instead: emitted-code execution + the install walk under
//! the measured classic alias. Recorded as a deviation in the plan.

use lp_collection::VecMap;
use lpir::builder::FunctionBuilder;
use lpir::{FloatMode, FuncId, IrType, LpirModule, LpirOp};
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
use lpvm_native::codemem_esp32::{CodeArena, CodeRegion, CodeSink, install};
use lpvm_native::compile::compile_module;
use lpvm_native::isa::IsaTarget;
use lpvm_native::link::link_jit_at;
use lpvm_native::native_options::NativeCompileOptions;

use lp_xt_emu::board::BoardProfile;
use lp_xt_emu::{CallOutcome, Emulator, RunOutcome};

/// The pure codemem math and the emulator's silicon-measured board model
/// must describe the SAME region — a drift here would let host tests pass
/// against a map the device does not have.
#[test]
fn code_region_matches_the_emulator_profile() {
    let region = CodeRegion::ESP32_DEFAULT;
    let profile = BoardProfile::esp32();
    assert_eq!(region.dbus_base, profile.code_dbus_base);
    assert_eq!(region.len_bytes as usize, profile.code_region_len);
    assert_eq!(region.ibus_base(), profile.code_ibus_base());

    // The inverse write map agrees with the emulator's alias rule at every
    // word of the region (both directions of the mirror, full range).
    let mut ibus = region.ibus_base();
    while ibus < region.ibus_end() {
        assert_eq!(
            profile.alias.dbus_to_ibus(region.dbus_write_addr(ibus)),
            ibus,
            "alias mismatch at ibus {ibus:#x}"
        );
        ibus += 4;
    }
}

/// g(x) = 3x; f(x) = g(x) + 1 — two functions so the image contains a real
/// intra-module call, whose literal slot [`link_jit_at`] must patch with the
/// callee's absolute I-bus address.
fn call_module() -> (LpirModule, LpsModuleSig) {
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
    (module, sig)
}

/// Compile [`call_module`] for Xtensa and link it at `exec_base` via the
/// real seam ([`link_jit_at`], no manual patching). Returns the linked image
/// and the entry offset of `f`.
fn compile_and_link_at(exec_base: u32) -> (Vec<u8>, u32) {
    let (ir, sig) = call_module();
    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        fuel: false,
        ..Default::default()
    };
    let compiled = compile_module(&ir, &sig, FloatMode::Q32, opts, IsaTarget::Xtensa)
        .expect("xt compile should succeed");
    let linked = link_jit_at(&compiled, IsaTarget::Xtensa, exec_base, |_| None)
        .expect("link_jit_at should succeed");
    let entry = *linked.entries.get("f").expect("entry f exists") as u32;
    (linked.code, entry)
}

/// The pipeline's output executes under the classic profile — emitted code,
/// windowed calls, and the classic code region's fetch rules all agree.
#[test]
fn pipeline_executes_under_the_classic_profile() {
    let mut emu = Emulator::with_profile(BoardProfile::esp32());
    let ibus_base = emu.profile.code_ibus_base();
    let (code, entry_off) = compile_and_link_at(ibus_base);
    match emu.run_with_args(&code, entry_off, &[0, 14]) {
        RunOutcome::Ok(v) => assert_eq!(v as i32, 14 * 3 + 1),
        RunOutcome::Trap(t) => panic!("unexpected trap under classic profile: {t:?}"),
    }
}

/// A sink that performs the install through the emulator's **D-bus** view,
/// so every write goes through the mirrored alias exactly as the device's
/// volatile-write walk does.
struct EmuDbusSink<'a> {
    emu: &'a mut Emulator,
}

impl CodeSink for EmuDbusSink<'_> {
    fn write_word(&mut self, dbus_addr: u32, word: u32) {
        self.emu
            .mem
            .write_u32(dbus_addr, word)
            .unwrap_or_else(|t| panic!("D-bus store at {dbus_addr:#x} trapped: {t:?}"));
    }
}

/// The flagship classic-memory-model test: link at the arena's span base,
/// install through the DESCENDING mirrored D-bus walk, execute at the I-bus
/// base. If the codemem math and the silicon-measured alias model disagree
/// anywhere — direction, word granularity, endianness within the word, span
/// bounds — the emitted code is scrambled and this cannot pass.
#[test]
fn mirrored_install_walk_executes_real_emitted_code() {
    let mut arena = CodeArena::new(CodeRegion::ESP32_DEFAULT);
    let mut emu = Emulator::with_profile(BoardProfile::esp32());

    // Reserve a span exactly as the placed pipeline would; first-fit puts it
    // at the region base, matching the profile's code_ibus_base.
    let probe = compile_and_link_at(0); // link once to learn the image size
    let span = arena.alloc(probe.0.len() as u32).expect("span fits");
    let (code, entry_off) = compile_and_link_at(span);

    install(
        arena.region(),
        span,
        &code,
        &mut EmuDbusSink { emu: &mut emu },
    )
    .expect("install within the reserved span");

    match emu.run_loaded_with_args(span + entry_off, &[0, 14], &mut lp_xt_emu::NoopTracer, None) {
        CallOutcome::Ok { lo, .. } => assert_eq!(lo as i32, 14 * 3 + 1),
        CallOutcome::Trap(t) => panic!("installed code trapped: {t:?}"),
    }
}

/// The classic capacity edge stays a clean error at real-region scale: an
/// image larger than the region must be a `TooLarge`, never a wild write.
/// (Sized from the region itself, so the 2026-08-02 shrink to 32 KiB needed
/// no edit here.)
#[test]
fn oversized_image_is_a_clean_toolarge() {
    let mut arena = CodeArena::new(CodeRegion::ESP32_DEFAULT);
    let cap = arena.capacity();
    let err = arena.alloc(cap + 4).unwrap_err();
    let msg = alloc_error_to_string(err);
    assert!(msg.contains("does not fit"), "unexpected error: {msg}");
}

fn alloc_error_to_string(e: lpvm_native::codemem_esp32::CodeMemError) -> String {
    format!("{e}")
}
