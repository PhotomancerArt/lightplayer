# Milestone I: VMContext Foundation

## Goal

Establish the VMContext type definition, header struct, and signature changes. Thread an empty
VMContext through the entire system (Cranelift and WASM) with no uniforms or globals yet—just the
plumbing.

## Suggested Plan Name

`vmcontext-globals-uniforms-milestone-1`

## Scope

**In scope:**

- `VmContextHeader` struct definition in `lpvm`
- Dynamic `GlslType` builder for shader-specific VMContext
- Update Cranelift signature generation to add VMContext as first param
- Update WASM emission to add i32 VMContext param
- Update `DirectCall` and `invoke` APIs to accept VMContext pointer
- Empty `_init()` function emission (stub)
- Test: verify VMContext pointer arrives correctly in shader functions

**Out of scope:**

- Actual uniform or global collection
- Load/store from VMContext
- Defaults buffer
- Host-side VMContext allocation helpers

## Key Decisions

1. **VMContext is dynamic**: Created per-shader by naga, not a fixed GlslType
2. **Explicit first parameter**: No reserved registers or WASM globals
3. **Well-known header at offset 0**: fuel (u64), trap_handler (u32), room for expansion
4. **LPIR stays agnostic**: No new opcodes; backends add the param

## Deliverables

- `lpvm/src/vmcontext.rs` with header and builder
- `docs/design/uniforms-globals.md` (design doc for entire feature)
- Updated `lpir-cranelift` signatures
- Updated `lps-wasm` signatures
- Filetests passing with empty VMContext

## Dependencies

- None (foundational milestone)

## Estimated Scope

~400 lines: type definitions (100), signature updates (150), test updates (150)
