# Milestone III: Mutability, _init(), and Reset

## Goal

Enable mutable globals, implement the `_init()` function for initialization, and provide fast global reset via defaults buffer.

## Suggested Plan Name

`vmcontext-globals-uniforms-milestone-3`

## Scope

**In scope:**
- Global writes via `Op::Store`
- Detect mutable vs readonly globals
- Emit `_init(vmctx)` function that initializes globals from uniforms/constants
- Run `_init()` once per shader to produce "defaults" buffer
- Fast global reset: `memcpy(defaults, globals)` per invocation
- Host API: allocate VMContext, run init, reset, set uniforms by name

**Out of scope:**
- Complex global initialization (globals depending on other globals)
- Memory protection for uniforms
- Fuel metering implementation
- Trap handlers

## Key Decisions

1. **Globals defaults in VMContext**: `globals_defaults` section lives in the same flat VMContext struct (after mutable `globals` section)
2. **Memcpy reset**: Faster than re-running shader code on every invocation
3. **Offset in header**: `globals_defaults_offset` stored in well-known header (fixed offset 12), set by host during VMContext setup
4. **Assumption**: Globals initialized only from uniforms/constant data

## Deliverables

- `Op::Store` emission for global writes
- `_init()` function generation in naga
- VMContext layout with `globals` + `globals_defaults` sections
- `globals_defaults_offset: u32` field in VMContext header
- `memcpy`-based reset path (host reads offset from header, computes size from metadata)
- Filetests demonstrating:
  - Mutable global across invocations
  - Reset restores initial values
  - Uniforms affect initialization

## Dependencies

- Milestone II: Uniforms and Readonly Globals

## Estimated Scope

~500 lines: store emission (100), init function (200), reset logic (100), host API (100)
