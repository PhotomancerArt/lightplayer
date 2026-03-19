# Phase 5: Wasmtime linking — `builtins.wasm` + shared memory

## Scope of phase

- **`WasmExecutable` / `wasm_runner`:** Stop using `Instance::new(..., &[])` for real shaders; load **`builtins.wasm`** from a known path (env var, `CARGO_MANIFEST_DIR`-relative, or `include_bytes!` for tests).
- **Host `Memory`:** Create `wasmtime::Memory` in the `Store` with sufficient min pages; pass the same memory into **both** module instantiations.
- **Linker:** Instantiate `builtins.wasm` with `env` memory import; obtain `Extern` values for each builtin export; define them on a `Linker` under `"builtins"` / `BuiltinId::name()`; instantiate **shader** module with `env.memory` + linked builtins.
- **API:** `glsl_wasm` may need to return metadata about required imports, or the runner always provides the full builtins set — align with “register all exports from builtins instance” approach.

## Code organization reminders

- Isolate filesystem path to `builtins.wasm` in one helper with clear error message if artifact missing (hint: run `build-builtins.sh`).

## Implementation details

- **Filetests:** `compile_for_target(Wasm)` must use the new instantiation path so `wasm.q32` matches production linking.
- **Performance:** Instantiating builtins once per process vs per test — prefer once if tests allow (static `OnceLock` or shared `Engine`/`Module` for builtins).

## Validate

```bash
cd lp-glsl && cargo test -p lp-glsl-filetests -- --ignored   # if filetests use ignored flag; else normal
./scripts/glsl-filetests.sh --target wasm.q32 builtins/
```

Smoke: `cargo test -p lp-glsl-wasm --test q32_builtin_link` (loads `lp_glsl_builtins_wasm.wasm`, shared `env.memory`, `Linker` + builtin re-exports, runs `sin(1.0)`). Set `LP_GLSL_BUILTINS_WASM` to override artifact path.

## Validate

- No `unwrap()` without message in linking path.
