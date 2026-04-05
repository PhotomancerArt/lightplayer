# Milestone I: VMContext Foundation — Plan Notes

## Scope of Work

Establish the VMContext type definition, header struct, and signature changes. Thread an empty
VMContext (no uniforms or globals yet) through the entire system:

- Define `VmContextHeader` in `lpvm` with well-known fields at fixed offsets
- Create VMContext builder for dynamic type construction
- Update Cranelift signature generation to add VMContext as first param
- Update WASM emission to add i32 VMContext param
- Update `DirectCall` and `invoke` APIs to accept VMContext pointer
- Create design doc `docs/design/uniforms-globals.md`
- Test: verify VMContext pointer arrives correctly in shader functions

## Current State

- **lpvm**: Has `GlslType`, layout computation (`std430`), and path resolution. Can describe struct
  types and compute byte offsets. No VMContext-specific code yet.
- **lpir**: Pure IR without GLSL metadata. `IrFunction` has `param_count` and `vreg_types`. First
  vreg is currently the first user param.
- **lpir-cranelift**: `signature_for_ir_func` builds signatures from `IrFunction` param/return
  counts. `DirectCall` and `invoke` assume user params only.
- **lp-glsl-wasm**: Emits functions with user params only. `local.get 0` is first user param.
- **lp-glsl-filetests**: Test harness creates and calls shaders without any context pointer.

## Questions

### Q1: VmContextHeader layout

**Context**: The well-known header needs fixed offsets for host access. Proposed:

- `[0] fuel: u64`
- `[8] trap_handler: u32`
- `[12] globals_defaults_offset: u32`
- `[16] ... room for expansion`

**Question**: Is this header layout correct? Should we add any other fields now (e.g., a flags word,
version field for future-proofing even if unused)?

**Answer**: Layout confirmed. No need for explicit expansion room—fields after header are
dynamically computed based on actual header size, so we can add fields naturally later.

### Q2: How to represent VMContext in LPIR

**Context**: Every function needs VMContext as arg 0. Options:

1. Add implicit param in backends only (Cranelift/WASM add +1 to signatures, LPIR stays unchanged)
2. Add explicit VMContext vreg in LPIR (new convention: vreg 0 is always VMContext)

**Question**: Should LPIR know about VMContext explicitly, or should backends handle it
transparently?

**Answer**: Explicit in LPIR. Add `IrFunction.vmctx_vreg: VReg` (always 0), shift user params to
start at vreg 1. Keeps LPIR small and simple, backends handle it directly.

### Q3: Cranelift signature changes

**Context**: `signature_for_ir_func` currently builds signatures from `IrFunction` directly. With
VMContext as first param, we need to add `pointer_type` as the first `AbiParam`.

**Question**: Should we change `signature_for_ir_func` signature to take a
`vmctx_type: Option<types::Type>` parameter, or just always add it (with `None` meaning skip)?

**Answer**: Always add VMContext to all function signatures. If a function doesn't use
globals/uniforms, caller can pass nullptr.

### Q4: WASM local indexing

**Context**: Currently `local.get 0` is first user param. With VMContext added, `local.get 0`
becomes VMContext, and user params start at `local.get 1`.

**Question**: This is a breaking change to all WASM emission. Should we introduce a
`FuncEmitCtx.vmctx_local: u32` field to make this explicit, or just hardcode "user local = index +
1"?

**Answer**: Add `FuncEmitCtx.vmctx_local: Option<u32>` field. Set to `Some(0)` when VMContext
enabled. Makes the code self-documenting.

### Q5: DirectCall API changes

**Context**: `DirectCall::call_i32(&[a, b])` calls a shader with args. With VMContext, this becomes
`call_i32(vmctx, &[a, b])` or similar.

**Question**: Should DirectCall take VMContext as a separate parameter or as part of the args slice?

**Answer**: Separate parameter: `call_i32(vmctx: *const u8, args: &[i32])`. Clearer intent.

### Q6: Empty VMContext testing

**Context**: For this milestone, VMContext has no uniforms or globals—just the header.

**Question**: How do we test that VMContext arrives correctly? Options:

1. Store a "magic number" in header via host, shader reads it back via a builtin
2. Add a test-only uniform that reads from a known header offset
3. Just verify the pointer is non-null in shader (no actual read)

**Suggested approach**: Option 1—store magic number at header offset (e.g., fuel = 0xDEADBEEF),
shader returns it via a new test builtin or by writing to a slot. Proves VMContext is reachable.

### Q7: Design doc scope

**Context**: We need `docs/design/uniforms-globals.md`.

**Question**: Should this doc cover the full roadmap (all 4 milestones) or just Milestone I?

**Suggested approach**: Full roadmap. Write the complete design now while it's fresh, with sections
marked "Milestone I", "Milestone II", etc. Easier than extending it later.

### Q8: VMContext creation and backward compatibility

**Context**: All existing code (filetests, wasm runner, JIT tests) creates and calls shaders without
any VMContext. We need to either:

1. Break everything and update all call sites in this milestone
2. Provide a default/null VMContext that works for shaders not using globals/uniforms
3. Create VMContext automatically in the test harnesses

**Question**: How should we handle VMContext creation? And what's the migration strategy for
existing code?

**Answer**: Rip the band-aid off in this milestone. Update all call sites (filetests, wasm runner,
JIT tests) to create and pass VMContext. Provide `VmContext::minimal()` helper for tests that just
need a placeholder. This milestone's job is to do the painful work so later milestones can focus on
features.
