# Phase 3: `JitModule`, `CompileOptions`, unified errors, emission function names

## Scope

- Introduce **`CompileOptions { float_mode: FloatMode }`** (extend later as needed).
- Unify errors: **`CompilerError`** (or extended **`CompileError`**) with variants
  **`Parse(String)`**, **`Lower(LowerError)`**, **`Codegen(CompileError)`** — implement
  **`Display`** / **`std::error::Error`**.
- Define **`JitModule`** struct holding:
  - inner **`JITModule`**
  - **`GlslModuleMeta`**
  - **`HashMap<String, FuncId>`** or parallel **names + ids**
  - per-function **Cranelift `Signature`**
  - **`call_conv`**, **`pointer_type`**
- Refactor **`jit_from_ir`** to either:
  - return **`Result<JitModule, CompilerError>`**, or
  - keep internal **`build_jit_inner`** used by both borrowed and owned paths.
- Audit **`emit` / `jit_module`**: every **`CompileError`** from per-function work
  must include **`&f.name`** in the message string.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Public API sketch

```rust
pub fn jit_from_ir(ir: &IrModule, options: &CompileOptions) -> Result<JitModule, CompilerError>;
```

Existing tests can migrate to **`JitModule::func_id("add")`** or keep indexing
by order — document stable ordering (**same as `IrModule::functions`**).

### `JitModule` methods (minimal for this phase)

- **`fn get_finalized_function(&self, name: &str) -> *const u8`** (or by `FuncId`)
- accessors for **`call_conv`**, **`pointer_type`** for **`direct_call`** later

### `GlslModuleMeta` for hand-written IR

If **`jit_from_ir`** is called without metadata (tests with parsed LPIR only),
pass **`GlslModuleMeta::default()`** or **`Option<GlslModuleMeta>`** inside
**`JitModule`** — **`call()`** returns error if metadata missing for that name.

## Validate

```
cargo check -p lpir-cranelift
cargo test -p lpir-cranelift
```
