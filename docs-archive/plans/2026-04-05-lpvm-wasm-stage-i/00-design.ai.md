# LPVM WASM Backend - Design

## Scope

Create `lpvm/lpvm-wasm/` crate implementing the LPVM traits for the WASM backend.
Validates the M1 trait design against wasmtime/browser constraints.

- **Emission** (always, `no_std` + alloc): LPIR → WASM bytes via `wasm-encoder`
- **Runtime** (`runtime` feature): wasmtime implementation of `LpvmEngine`, `LpvmModule`, `LpvmInstance`

## File Structure

```
lpvm/lpvm-wasm/
├── Cargo.toml
└── src/
    ├── lib.rs              # Crate root, feature-gated exports
    ├── emit.rs             # LPIR → WASM emission (entry point)
    ├── emit/               # Emission submodules
    │   ├── mod.rs          # Module emission orchestration
    │   ├── control.rs      # Control flow (if/else, loops, switch)
    │   ├── func.rs         # Function encoding, signatures
    │   ├── imports.rs      # Import filtering, builtin mapping
    │   ├── memory.rs       # Shadow stack, slots, memory ops
    │   ├── ops.rs          # LPIR ops → WASM instructions
    │   └── q32.rs          # Q32 fixed-point operations
    ├── module.rs           # WasmModule struct (bytes, exports, metadata)
    ├── options.rs          # WasmOptions (float mode, etc.)
    ├── error.rs            # WasmError unified enum
    └── runtime/            # wasmtime runtime (runtime feature)
        ├── mod.rs          # Runtime module root
        ├── engine.rs       # WasmEngine implements LpvmEngine
        └── instance.rs     # WasmInstance implements LpvmInstance
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  lpvm-wasm crate                                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────────────────────────────────────────┐  │
│  │ Emission (always, no_std + alloc)                     │  │
│  │                                                       │  │
│  │  emit::emit_module(ir, options) -> (bytes, shadow)    │  │
│  │  WasmModule { bytes, exports, shadow_stack_base }     │  │
│  │  ├─ bytes(&self) -> &[u8]  // browser use case        │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐  │
│  │ Runtime (runtime feature, wasmtime)                   │  │
│  │                                                       │  │
│  │  WasmEngine {                                         │  │
│  │    engine: wasmtime::Engine,                          │  │
│  │    builtins_bytes: Vec<u8>                            │  │
│  │  }                                                    │  │
│  │  implements LpvmEngine ──► compile(ir, meta)           │  │
│  │                              │                        │  │
│  │                              ▼                        │  │
│  │  WasmModule {                                         │  │
│  │    bytes, exports, meta, shadow_base                  │  │
│  │  }                                                    │  │
│  │  implements LpvmModule ──► signatures()               │  │
│  │                       └─► instantiate() ──►            │  │
│  │                              │                        │  │
│  │                              ▼                        │  │
│  │  WasmInstance { store, instance, memory }             │  │
│  │  implements LpvmInstance ──► call(name, args)        │  │
│  │                              resets shadow stack       │  │
│  │                              sets fuel, calls         │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Components

### Emission

`emit.rs` is the main entry. Takes `&IrModule` and `&WasmOptions`, produces:
- WASM bytes (valid module)
- `shadow_stack_base: Option<i32>` (if module uses shadow stack)

Emission submodules mirror `lps-wasm/src/emit/` structure, adapted for:
- Direct `IrModule` input (no GLSL frontend coupling)
- Clearer separation between emission and runtime concerns

### WasmModule

Opaque compiled artifact. Not a trait implementation — just the emission output.

```rust
pub struct WasmModule {
    pub bytes: Vec<u8>,
    pub exports: Vec<WasmExport>,
    pub shadow_stack_base: Option<i32>,
}

impl WasmModule {
    pub fn bytes(&self) -> &[u8] { &self.bytes }
    pub fn exports(&self) -> &[WasmExport] { &self.exports }
}
```

`WasmExport` carries both WASM-level types (`Vec<WasmValType>`) and logical
shader types (`LpsType`) for runtime marshaling.

### Runtime

**WasmEngine** (implements `LpvmEngine`):
- Holds `wasmtime::Engine` (expensive, shared)
- Holds builtins WASM bytes (loaded once, parsed per-compile)
- `compile()`: emit → parse → link builtins → return WasmModule + wasmtime Module

**WasmModule** (implements `LpvmModule`):
- Wraps wasmtime `Module` (parsed, ready to instantiate)
- Carries `LpsModuleSig` for signature queries
- `instantiate()`: create Store → link → return WasmInstance

**WasmInstance** (implements `LpvmInstance`):
- Holds `wasmtime::Store` (mutable execution state)
- `call()`: reset shadow stack → set fuel → flatten args → invoke → unmarshal result

## Key Design Decisions

1. **Parallel infrastructure**: Emission copied from `lps-wasm`, not moved.
   `lps-wasm` remains untouched until M7 migration. This lets us iterate safely.

2. **WasmModule is concrete, not trait**: The trait is `LpvmModule`. The WASM
   artifact also needs to expose raw bytes for the browser — that's a concrete
   method, not part of the trait.

3. **Error unification**: `WasmError` enum covers emission, instantiation, and
   call errors. Maps cleanly to the trait's single associated `Error` type.

4. **No Q32 conversion in emission**: Emission produces the same WASM regardless
   of float mode. The runtime (wasmtime) handles Q32 encoding/decoding when
   marshaling `LpsValue` ↔ WASM values.

5. **Instantiate pattern**: `WasmEngine::compile()` returns a type that implements
   both the concrete `WasmModule` (for bytes) and `LpvmModule` (for instantiation).
   This may be a wrapper or the same type — TBD in implementation.
