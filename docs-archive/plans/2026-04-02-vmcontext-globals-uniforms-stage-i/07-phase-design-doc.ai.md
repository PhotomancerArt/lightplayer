# Phase 7: Create Design Document

## Scope of Phase

Create `docs/design/uniforms-globals.md` documenting the full VMContext, uniforms, and globals architecture across all milestones.

## Code Organization Reminders

- Create file at `docs/design/uniforms-globals.md`
- Follow existing design doc style
- Include all milestones, clearly marked

## Implementation Details

### Content Outline

```markdown
# Uniforms and Globals in LightPlayer

## Overview

LightPlayer shaders can access:
- **Uniforms**: Read-only data set by the host (time, resolution, user parameters)
- **Globals**: Mutable state within an invocation, reset between invocations

## Architecture

### VMContext Layout

Single flat struct in memory:

```
[0]   VmContextHeader (16 bytes)
      - fuel: u64
      - trap_handler: u32
      - globals_defaults_offset: u32
[N]   Uniforms section (shader-specific)
[M]   Globals section (mutable, per-invocation)
[P]   GlobalsDefaults section (source for reset)
```

### Function ABI

All functions receive VMContext as first parameter:

```rust
fn shader(vmctx: *mut VMContext, arg0: i32, ...) -> i32
```

### Host Workflow

1. Compile shader → produces VMContext type metadata
2. Allocate VMContext memory (header + uniforms + globals + defaults)
3. Set header fields (fuel, trap_handler, defaults_offset)
4. Write uniform values
5. Call `_init(vmctx)` once to initialize globals and defaults
6. For each invocation:
   a. `memcpy(defaults → globals, globals_size)` for reset
   b. Call shader function
   c. Read outputs

## Milestones

### Milestone I: VMContext Foundation

- VMContextHeader struct with well-known fields
- All functions accept VMContext as first parameter
- Test harnesses allocate and pass VMContext
- **Status**: [ ] In Progress / [ ] Complete

### Milestone II: Uniforms and Readonly Globals

- Collect uniform declarations from GLSL
- Collect global declarations
- Build dynamic VMContext type with uniforms and globals
- Path-based name lookup for uniforms
- Load operations from VMContext
- **Status**: [ ] Not Started

### Milestone III: Mutability and Reset

- Store operations to globals
- Emit `_init()` function
- GlobalsDefaults section in VMContext
- Fast memcpy reset between invocations
- **Status**: [ ] Not Started

### Milestone IV: Validation

- Comprehensive filetests
- RISC-V32 and WASM validation
- Performance baseline
- **Status**: [ ] Not Started

## Design Decisions

### Why VMContext as explicit parameter?

Alternatives considered:
- Reserved register: Requires Cranelift config changes, custom assembly per arch
- WASM global: Divergent from RISC-V approach

Explicit parameter is portable, simple, and has negligible cost with inlining.

### Why name-based uniform access?

Requiring `layout(binding = N)` makes GLSL verbose. Name-based access is cleaner and offset lookup can be cached.

### Why single flat struct?

Better cache locality than separate allocations for uniforms/globals/defaults. One allocation, contiguous memory.

### Why globals_defaults in VMContext?

Enables fast memcpy reset. Alternative (re-running init code) is too expensive per-invocation.

## Future Work

- Compute mode where globals persist across invocations
- Fuel metering implementation
- Trap handlers for OOB/div-by-zero
- 64-bit pointer support
```

## Tests to Write

None—this is documentation only.

## Validate

```bash
# Just verify the file exists
cat docs/design/uniforms-globals.md | head -20
```

## Notes

- This doc captures the full roadmap, not just Milestone I
- Update status checkboxes as milestones complete
