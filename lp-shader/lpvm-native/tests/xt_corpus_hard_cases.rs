//! Proves the two corpus cases that could pass *without testing anything*
//! actually exercise what they claim.
//!
//! M1's review gate says a clean sweep is suspicious until the hard cases are
//! shown to have really happened, and names the two ways this corpus could be
//! quietly worthless:
//!
//! 1. **An inlined intra-module call does not test the exec-alias fix.** If the
//!    compiler folded `g` into `f`, there is no `l32r`/`callx8` pair, no
//!    relocation, and no literal holding an execute address — the case would
//!    pass on hardware while proving nothing about the D-bus/I-bus alias.
//! 2. **A call chain that stays under the window threshold does not test
//!    overflow.** Depth alone is not evidence; the frames have to actually
//!    wrap the physical register file.
//!
//! Both are asserted mechanically here rather than argued in prose, because a
//! transcript full of PASS lines looks identical either way.

#![cfg(feature = "emu-xt")]

use lp_collection::VecMap;
use lp_xt_emu::{Emulator, TraceEvent, Tracer};
use lpir::FloatMode;
use lpvm_native::compile::compile_module;
use lpvm_native::isa::IsaTarget;
use lpvm_native::native_options::NativeCompileOptions;
use lpvm_native::xt_corpus::CASES;

fn case(name: &str) -> &'static lpvm_native::xt_corpus::XtCase {
    CASES
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("corpus case `{name}` missing"))
}

/// Compile options for the low-level checks below.
///
/// `fuel: false` deliberately, unlike the corpus's real compile. Fuel
/// instrumentation reads and writes the vmctx header, and this test drives the
/// emulator directly with `vmctx = 0` rather than through `rt_emu`'s arena —
/// so fuel would fault at `vaddr: 0` (cause 3) before the chain ever got deep
/// enough to overflow anything. `xt_pipeline`'s own helper passes `fuel: false`
/// for the same reason.
///
/// This narrows what the test proves to a *structural* question — does the
/// chain overflow the register window — which is exactly what it is for. The
/// corpus's real behaviour, fuel included, is covered by
/// `xt_corpus_goldens.rs`.
fn opts() -> NativeCompileOptions {
    NativeCompileOptions {
        float_mode: FloatMode::Q32,
        fuel: false,
        ..Default::default()
    }
}

/// Risk 1's case must emit a REAL call — a relocation into a literal slot that
/// the linker fills with the callee's execute address. That relocation is the
/// exec-alias fix's entire surface: no reloc, no test.
#[test]
fn intra_module_call_really_emits_a_call_relocation() {
    let c = case("intra_module_call");
    let (ir, sig) = (c.build)();
    let module =
        compile_module(&ir, &sig, FloatMode::Q32, opts(), IsaTarget::Xtensa).expect("xt compile");

    let f = module
        .functions
        .iter()
        .find(|f| f.name == "f")
        .expect("entry `f` present");

    let call_relocs: Vec<_> = f
        .relocs
        .iter()
        .filter(|r| r.r_type == IsaTarget::Xtensa.call_reloc_type() && r.symbol == "g")
        .collect();

    assert!(
        !call_relocs.is_empty(),
        "`f` has no call relocation to `g` — the call was inlined or folded away, \
         so this case does NOT test the exec-alias fix. Make the callee opaque \
         to the optimiser rather than weakening this assertion."
    );

    // `g` must also survive as its own function; a reloc pointing at a symbol
    // that no longer exists would fail to link rather than test anything.
    assert!(
        module.functions.iter().any(|f| f.name == "g"),
        "callee `g` was eliminated from the module"
    );
}

/// Counts the window spills the emulator performs.
#[derive(Default)]
struct SpillCounter {
    spills: usize,
    reloads: usize,
    max_base: u8,
}

impl Tracer for SpillCounter {
    fn event(&mut self, event: TraceEvent<'_>) {
        match event {
            TraceEvent::WindowSpill { .. } => self.spills += 1,
            TraceEvent::WindowReload { .. } => self.reloads += 1,
            TraceEvent::WindowRotate { new_base, .. } => {
                self.max_base = self.max_base.max(new_base);
            }
            _ => {}
        }
    }
}

/// Risk 3's case must actually overflow the register window.
///
/// The emulator models overflow as a real spill to the frame's stack save area
/// (`TraceEvent::WindowSpill`), so this is observable rather than inferred.
/// Without it, "depth 20" would be an unverified claim — and the GLSL filetest
/// corpus cannot reach this depth at all (it forbids recursion, topping out
/// around 3-5 static frames), so nothing else in the tree covers it.
#[test]
fn deep_call_chain_really_overflows_a_register_window() {
    let c = case("deep_call_chain_20");
    let (ir, sig) = (c.build)();
    let module =
        compile_module(&ir, &sig, FloatMode::Q32, opts(), IsaTarget::Xtensa).expect("xt compile");

    // Link exactly as the JIT does: concatenate, then patch each literal slot
    // with the callee's absolute execute address at the emulator's I-bus base.
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
            let target_off = *entries.get(&reloc.symbol).expect("intra-module symbol");
            let target = ibus_base + target_off as u32;
            let slot = func_offsets[fi] + reloc.offset;
            code[slot..slot + 4].copy_from_slice(&target.to_le_bytes());
        }
    }

    let entry_off = *entries.get(c.entry).expect("entry present") as u32;
    let mut counter = SpillCounter::default();
    emu.mem.load_bytes(ibus_base, &code);
    let outcome = emu.run_loaded_with_args(ibus_base + entry_off, &[0, 100], &mut counter, None);

    assert!(
        counter.spills > 0,
        "the depth-{} chain caused ZERO window spills — it never overflowed the \
         register file, so it does not test what risk 3 claims. Outcome: {outcome:?}, \
         rotations reached base {}. Deepen the chain rather than deleting this assertion.",
        lpvm_native::xt_corpus::CHAIN_DEPTH,
        counter.max_base,
    );

    // Overflow without underflow would mean the frames never came back — the
    // return path through reload is half of what makes deep chains dangerous.
    assert!(
        counter.reloads > 0,
        "spills without reloads: frames were saved but never restored ({} spills)",
        counter.spills
    );

    eprintln!(
        "deep_call_chain_20: {} window spills, {} reloads (outcome {outcome:?})",
        counter.spills, counter.reloads
    );
}
