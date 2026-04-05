# LPVM Core Traits

## Design

### Scope

Add `LpvmEngine`, `LpvmModule`, and `LpvmInstance` traits to the `lpvm` crate.
These are the trait abstractions that backend crates (`lpvm-cranelift`,
`lpvm-wasm`, `lpvm-rv32`) will implement in M2–M4.

Traits and types only — no implementations. `no_std + alloc` compatible.

### File structure

```
lp-shader/lpvm/
├── Cargo.toml              # UPDATE: add lpir dependency
└── src/
    ├── lib.rs              # UPDATE: add trait module + re-exports
    ├── engine.rs           # NEW: LpvmEngine trait
    ├── module.rs           # NEW: LpvmModule trait
    ├── instance.rs         # NEW: LpvmInstance trait
    ├── data.rs             # existing (unchanged)
    ├── data_error.rs       # existing (unchanged)
    └── vmcontext.rs        # existing (unchanged)
```

### Conceptual architecture

```
LpvmEngine                        -- shared config, cached resources
│  fn compile(&IrModule, &LpsModuleSig) -> Module
│
└─► LpvmModule                   -- compiled artifact (immutable)
    │  fn signatures() -> &LpsModuleSig
    │  fn instantiate(&self) -> Instance
    │
    └─► LpvmInstance              -- execution state (mutable)
         fn call(name, &[LpsValue]) -> Result<LpsValue, E>

Backends (M2–M4):
  lpvm-wasm:       WasmEngine  → WasmModule  → WasmInstance
  lpvm-cranelift:  CraneliftEngine → CraneliftModule → CraneliftInstance
  lpvm-rv32:       Rv32Engine  → Rv32Module  → Rv32Instance

Fast path (not in traits, backend-specific):
  CraneliftModule::direct_call(name) -> DirectCall
  lp-engine uses this directly for the hot render path
```

### Key decisions

- `lpvm` depends on `lpir` — LPVM is the runtime for LPIR, never used without
  it. `compile` takes `&IrModule` directly.
- `LpvmModule` exposes `fn signatures(&self) -> &LpsModuleSig` — all backends
  carry this metadata.
- Single semantic `call` method — `call(name, &[LpsValue]) -> Result<LpsValue, E>`.
  The fast path (`DirectCall`) is backend-specific, not in the trait. lp-engine
  uses concrete types / `#[cfg]` for the hot path. Acceptable — only 2 backends,
  few call sites.
- No `LpvmMemory` trait — memory is an implementation detail per backend.
  `LpvmEngine` implicitly owns shared memory. Textures will live there later.
- `out`/`inout` params: **not supported** by the semantic `call` API. Backends
  return a clear error if attempted. Filetests wrap these in plain functions.
- Associated `type Error` on each trait — backends bring their own error types.

## Phases

### Phase 1: Add traits

#### Scope

Add `lpir` dependency. Create `engine.rs`, `module.rs`, `instance.rs` with the
three traits. Wire into `lib.rs`.

#### Code Organization Reminders

- One trait per file.
- Traits at the top, helper types at the bottom.
- Keep related functionality grouped together.

#### Implementation Details

**Cargo.toml** — add `lpir` dependency:

```toml
[dependencies]
lps-shared = { path = "../lps-shared" }
lpir = { path = "../lpir" }
```

Update description to reflect the crate's expanded role.

**engine.rs:**

```rust
use lpir::IrModule;
use lps_shared::LpsModuleSig;

use crate::module::LpvmModule;

pub trait LpvmEngine {
    type Module: LpvmModule;
    type Error: core::fmt::Display;

    fn compile(
        &self,
        ir: &IrModule,
        meta: &LpsModuleSig,
    ) -> Result<Self::Module, Self::Error>;
}
```

**module.rs:**

```rust
use lps_shared::LpsModuleSig;

use crate::instance::LpvmInstance;

pub trait LpvmModule {
    type Instance: LpvmInstance;
    type Error: core::fmt::Display;

    fn signatures(&self) -> &LpsModuleSig;

    fn instantiate(&self) -> Result<Self::Instance, Self::Error>;
}
```

**instance.rs:**

```rust
use lps_shared::lps_value::LpsValue;

pub trait LpvmInstance {
    type Error: core::fmt::Display;

    fn call(
        &mut self,
        name: &str,
        args: &[LpsValue],
    ) -> Result<LpsValue, Self::Error>;
}
```

**lib.rs** — add modules and re-exports:

```rust
mod engine;
mod instance;
mod module;

pub use engine::LpvmEngine;
pub use instance::LpvmInstance;
pub use module::LpvmModule;
```

#### Validate

```bash
cargo check -p lpvm
cargo test -p lpvm
cargo check -p lps-shared -p lpir -p lps-frontend -p lps-filetests
```

### Phase 2: Cleanup & validation

#### Scope

Verify everything compiles, no warnings, no stale code. Update m1 doc. Move
plan to done.

#### Plan cleanup

Move remaining notes to the bottom of the plan file under `# Notes`.
Move the plan file to `docs/plans-done/`.

#### Commit

```
feat(lpvm): add LpvmEngine/LpvmModule/LpvmInstance traits

- LpvmEngine: compile(IrModule, LpsModuleSig) -> Module
- LpvmModule: signatures(), instantiate() -> Instance
- LpvmInstance: call(name, args) -> LpsValue
- Add lpir dependency to lpvm
- Semantic call API only; fast path is backend-specific
- out/inout params not supported via call()
```

#### Validate

```bash
cargo check -p lpvm
cargo test -p lpvm
cargo +nightly fmt -- --check
cargo check -p lps-filetests
```

## Notes

### Backend shapes to abstract over

**Cranelift JIT** (`lpir-cranelift`):
- Engine: `CompileOptions` (float mode, Q32 options, memory strategy)
- Module: `JitModule` (finalized code pointers, metadata, signatures)
- Instance: `DirectCall` (func ptr + arity) — essentially stateless
- Builtins: function pointers, statically linked at JIT time

**WASM** (`lps-wasm` / `lps-filetests::wasm_runner`):
- Engine: `wasmtime::Engine` + cached builtins `wasmtime::Module`
- Module: `WasmModule` (bytes + exports + shadow stack info)
- Instance: `wasmtime::Store` + `wasmtime::Instance` + shared `Memory`
- Builtins: `lps-builtins-wasm.wasm` loaded and instantiated, exports linked

**RV32 emulator** (`lpir-cranelift::emu_run`):
- Engine: compile options + builtins ELF binary path/bytes
- Module: ELF object bytes (compiled via Cranelift object mode)
- Instance: `Riscv32Emulator` with loaded ELF + memory
- Builtins: linked into ELF at link time

### Q&A record

**Q1: Should lpvm depend on lpir?** Yes. LPVM is the runtime for LPIR — never
used without it. `compile` takes `&IrModule`.

**Q2: Should LpvmModule own metadata?** Yes. `fn signatures(&self) -> &LpsModuleSig`.

**Q3: Call signature?** Single semantic `call(name, &[LpsValue]) -> Result<LpsValue, E>`.
Fast path is backend-specific, not in the trait. out/inout params return a
clear error ("out/inout parameters are not supported for direct calling").

**Q4: LpvmMemory trait?** Skip. Memory is per-backend. LpvmEngine implicitly
owns shared memory. Textures will live there later.

**Q5: Errors?** Associated `type Error` on each trait. Backends bring their own.
