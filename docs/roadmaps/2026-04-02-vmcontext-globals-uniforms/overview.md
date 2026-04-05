# VMContext, Globals, and Uniforms — Roadmap Overview

## Motivation / Rationale

LightPlayer needs GLSL shaders to access `uniform` and `global` variables. Currently, the shader
execution environment is stateless—functions operate only on their explicit parameters. To support:

- **Uniforms**: Read-only data set by the host (time, resolution, user parameters)
- **Globals**: Mutable state within an invocation, shared across function calls (counters, RNG
  seeds). Reset to initial values before each invocation.

We need a runtime context that provides access to this data. The context must work on both
RISC-V32 (embedded) and WebAssembly (browser/emu) targets.

## Architecture / Design

### VMContext: Single Flat Struct

VMContext is a dynamically-created `GlslType` built at compile time. It contains:

1. **Well-known header** (fixed offsets):
    - `fuel: u64` — optional gas metering
    - `trap_handler: u32` — optional callback pointer
    - `globals_defaults_offset: u32` — offset to globals_defaults section (for memcpy reset)
    - (room for expansion)

2. **Shader-specific uniforms**: Collected from `uniform` declarations, accessed by name

3. **Shader-specific globals**: Collected from global variable declarations, mutable

4. **Global defaults**: Copy of initial global values, source for fast reset

```
VMContext (single flat allocation)
├── [0] fuel: u64
├── [8] trap_handler: u32
├── [12] globals_defaults_offset: u32  // points to offset [R] within this struct
├── [16] padding to alignment
├── [N] uniform_foo: SomeStruct
├── [M] uniform_bar: OtherStruct
├── [P] global_counter: i32          // mutable
├── [Q] global_position: vec3        // mutable
├── [R] defaults_global_counter: i32  // source for reset
└── [S] defaults_global_position: vec3 // source for reset
```

The host accesses well-known fields via a Rust struct (`VmContextHeader`) through unsafe cast.
Shader code accesses all fields via `vmctx + offset`.

### VMContext as First Parameter

Every shader function receives VMContext as an explicit first parameter:

- **Cranelift**: `pointer_type` added to all signatures
- **WASM**: `i32` added to all function signatures
- **LPIR**: First vreg implicitly holds VMContext; backends treat as native pointer

LPIR stays agnostic—no new opcodes. Lowering produces regular `Op::Load`/`Op::Store` with addresses
computed as `vmctx + offset`.

### Uniform Access: Name-Based

Uniforms are **not** accessed via binding IDs. Instead, the host looks up offsets by name using
`GlslType` metadata:

```rust
// Host side
let offset = vmcontext_type.path_offset( & ["uniform_time"]) ?;
let ptr = vmctx_ptr.add(offset);
```

This produces cleaner GLSL (no `layout(binding = N)` required) at the cost of name lookup (offset
can be cached after first access). A known design choice/limitation.

### Global Initialization and Reset

To avoid re-running shader code on every invocation:

1. VMContext layout includes a `globals_defaults` section after the mutable `globals` section
2. Run `_init(vmctx)` once per shader to initialize both globals and globals_defaults
3. On each invocation, `memcpy(vmctx.globals_defaults → vmctx.globals)` for fast reset

The `globals_defaults_offset` is stored in the VMContext header (at a fixed well-known offset). The
host reads this offset, computes `globals_size` from metadata, and performs
`memcpy(vmctx + defaults_offset → vmctx + globals_offset, globals_size)` for fast reset. This
assumes globals are initialized only from uniform/constant data—safe for shader patterns.

## File Tree

```
lp-shader/
├── lpvm/
│   └── src/
│       └── vmcontext.rs        # VmContextHeader, constants, builder
├── lp-glsl-naga/
│   └── src/
│       └── lower.rs              # Collect uniforms/globals, build VMContext type
├── lpir/
│   └── src/
│       └── module.rs             # No changes—VMContext is implicit first param
├── lpir-cranelift/
│   ├── src/
│   │   ├── emit/mod.rs           # Thread VMContext through signatures
│   │   └── jit_module.rs         # Store memcpy metadata (globals offset, defaults offset, size)
│   └── src/lib.rs                # DirectCall takes VMContext pointer
├── lp-glsl-wasm/
│   └── src/
│       ├── emit/mod.rs           # Add i32 param to all signatures
│       └── func.rs               # local.get 0 is VMContext
└── lp-glsl-filetests/
    └── src/
        └── test_run/             # Harness allocates VMContext, sets uniforms

docs/design/
└── uniforms-globals.md           # Design doc (created in Milestone I)
```

## Alternatives Considered

### 1. Reserved Register / Global Approach

Store VMContext pointer in a reserved register (RISC-V) or WASM global.

**Rejected**: Requires:

- Cranelift reserved register configuration (not well-tested on RISC-V)
- Custom assembly per architecture for setup/teardown
- LPIR language extensions (new opcodes)
- Breaks Rust interop (can't call shader functions directly)

The explicit parameter approach is simpler, more portable, and has negligible cost with inlining.

### 2. Separate Uniforms/Globals Pointers (Two-Layer)

VMContext contains pointers to separate allocations for uniforms and globals.

**Rejected**: Two allocations means two cache lines accessed. Single flat struct keeps everything
contiguous—better cache locality.

### 3. Binding-Based Uniform Access

Use `layout(binding = N)` to identify uniforms.

**Rejected**: Requires explicit bindings in GLSL source. Name-based access is cleaner and matches
the existing `GlslType` metadata we already collect.

### 4. Separate Defaults Buffer

Store global defaults in a separate allocation outside VMContext.

**Rejected**: Extra allocation and pointer. Keeping defaults in the same flat struct (globals
section followed by defaults section) allows a single memcpy with pre-computed offsets.

## Risks

1. **Performance of name lookup**: Offset lookup by name is slower than binding-based access.
   Mitigated by caching offsets after first access.

2. **Global initialization complexity**: The `_init()` + memcpy approach assumes globals are
   initialized from uniform/constant data only. If users need more complex patterns (e.g., global
   initialized from a previous global), we may need to extend.

3. **Embedded memory pressure**: VMContext is one more allocation. Size is
   `header + uniforms + globals + defaults`. Should be acceptable given typical shader sizes.

4. **Pointer width assumptions**: Currently assumes 32-bit targets. 64-bit support would need
   pointer width plumbing through frontend.

5. **LPIR changes breaking existing tests**: Adding implicit first param to all functions requires
   updating filetests, DirectCall, invoke paths. Mitigated by milestone structure (I focuses on
   threading, II on uniforms).
