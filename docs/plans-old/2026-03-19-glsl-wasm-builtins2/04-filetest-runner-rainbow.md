# Phase 4: Filetest runner + Rainbow end-to-end

## Scope of phase

Update the filetest WASM runner to link `builtins.wasm` + shared memory. Compile and run
`rainbow.shader/main.glsl` under wasmtime. Remove `@unimplemented(backend=wasm)` from filetests that
now pass.

## Code organization reminders

- Shared linking helper in `lps-filetests/src/test_run/wasm_link.rs`.
- Isolate builtins.wasm file path resolution in one place with a clear error message.
- Tests first in test modules; helpers at bottom.

## Implementation details

### 1. `wasm_link.rs`

File: `lp-shader/lps-filetests/src/test_run/wasm_link.rs`

Shared helper that encapsulates the builtins.wasm + memory + linker pattern. Reference:
`lps-wasm/tests/q32_builtin_link.rs`.

```rust
pub struct WasmLinkedInstance {
    pub store: Store<()>,
    pub instance: Instance,
}

/// Load builtins.wasm, create shared memory, link, instantiate shader.
pub fn instantiate_with_builtins(
    engine: &Engine,
    store: &mut Store<()>,
    shader_module: &Module,
) -> Result<Instance, ...> {
    // 1. Locate builtins.wasm (lps_BUILTINS_WASM env var or CARGO_MANIFEST_DIR-relative)
    // 2. Load and instantiate builtins module
    // 3. Create shared Memory (1 page min)
    // 4. Build Linker:
    //    - "env" "memory" → shared memory
    //    - "builtins" "<name>" → each export from builtins instance
    // 5. Instantiate shader module via linker
}
```

The helper should detect whether the shader module actually has imports. If the import section is
empty (no builtins, no memory), fall back to `Instance::new(&mut store, &module, &[])` for backwards
compatibility with simple shaders that don't use builtins.

### 2. Update `wasm_runner.rs`

File: `lp-shader/lps-filetests/src/test_run/wasm_runner.rs`

Replace the `Instance::new(&mut store, &wasm_module, &[])` call in `WasmExecutable::from_source`
with a call to `wasm_link::instantiate_with_builtins`. The rest of the `GlslExecutable`
implementation stays the same.

### 3. Rainbow compilation test

Add a test (in `lps-wasm/tests/` or as a filetest) that:

1. Compiles `examples/basic/src/rainbow.shader/main.glsl` with `glsl_wasm` Q32
2. Links with builtins.wasm + shared memory
3. Calls `main(vec2(100.0, 100.0), vec2(200.0, 200.0), 1.0)` (or similar)
4. Verifies it returns a vec4 (4× i32 on stack) without trapping

Potential issues to watch for:

- `const bool CYCLE_PALETTE = true` — verify const bool declaration and usage in `if` compiles
- `vec4` return from `main` — verify vec4 constructor + multi-value return
- Multiple user functions calling each other (`paletteRainbow` → called from `applyPalette` → called
  from `main`)
- Vector operations in palette functions (vec3 arithmetic, `abs(mod(...))`)
- Scalar/vector `mix` overload (scalar `mix` in `main`, vec3 `mix` at the end)

### 4. Filetests

Run the full filetest suite for WASM and check which builtins filetests now pass:

```bash
./scripts/filetests.sh --target wasm.q32
```

For filetests that pass, remove `@unimplemented(backend=wasm)`. Only remove the annotation for tests
that actually pass — don't speculatively remove. Focus on:

- `builtins/trig-sin.glsl`, `trig-cos.glsl` — should pass (import path works)
- `builtins/common-floor.glsl`, `common-fract.glsl` — should pass after phase 2
- Any that only use inline builtins (abs, clamp, etc.) + features already supported (scalars,
  vectors)

Check that Cranelift filetests are not regressed:

```bash
./scripts/filetests.sh --target cranelift.q32
```

### 5. `test_q32_float_mul` ignore removal

Check whether `test_q32_float_mul` in `lps-wasm/tests/basic.rs` still has `#[ignore]`. The Q32
mul/div bug was reportedly fixed (per the predecessor plan notes). If the test passes, remove the
`#[ignore]`.

## Validate

```bash
cd lps && cargo test -p lps-wasm
cd lps && cargo test -p lps-filetests
cargo build
cargo +nightly fmt --check
./scripts/filetests.sh --target wasm.q32
./scripts/filetests.sh --target cranelift.q32
```
