# Phase 1: `builtins.wasm` artifact and shared memory import

## Scope of phase

- Add **`lps-builtins-wasm`**: a thin crate that depends on `lps-builtins`, targets
  `wasm32-unknown-unknown`, and exports every `__lp_q32_*` / `__lpfn_*` symbol the Rust side already
  exposes (same pattern as `lps-builtins-emu-app` referencing symbols).
- Configure the wasm build so the module **imports** linear memory (`env.memory`) instead of
  defining its own — implemented via `lps-builtins-wasm/build.rs` (`--import-memory` when
  targeting `wasm32-unknown-unknown`).
- Extend **`scripts/build-builtins.sh`** (or adjacent script) to produce `builtins.wasm`
  deterministically beside other builtin artifacts.
- Document output path and how tests/runtime load the file.

## Code organization reminders

- Prefer one concept per file; entry/binary crate stays small.
- Helpers at bottom of files.
- TODO only for genuinely temporary hooks; prefer a single documented output path constant.

## Implementation details

- **Crate layout:** `src/lib.rs` or `main.rs` that forces linkage of all builtins (mirror emu-app
  `builtin_refs` pattern if needed to avoid LTO dropping symbols).
- **Memory:** Target must emit `(import "env" "memory" (memory …))` with no conflicting data
  segments that assume owned memory until we validate sret paths.
- **CI / dev:** `cargo build -p lps-builtins-wasm --target wasm32-unknown-unknown` from
  `lp-shader/` workspace; ensure target is installed in docs (
  `rustup target add wasm32-unknown-unknown`).

## Validate

```bash
cd lps && cargo build -p lps-builtins-wasm --target wasm32-unknown-unknown
```

Inspect `builtins.wasm` with `wasm-objdump -x` or `wasm2wat` — confirm memory is imported, named
exports match `BuiltinId::name()` values.

- Build succeeds with no new warnings (fix or allow-list with comment).
