# lps-builtins-gen-app

Scans **`lps-builtins`** (`src/builtins/glsl/`, `lpir/`, `lpfx/`) and emits the glue every
other crate expects: **`BuiltinId`**, Cranelift ABI glue, WASM import metadata, and `mod.rs`
stubs so the compiler and tests stay in sync.

## Generated outputs

| Output                                           | Purpose                                                          |
|--------------------------------------------------|------------------------------------------------------------------|
| `lps-builtin-ids/src/glsl_builtin_mapping.rs`    | GLSL / LPIR / LPFX name → `BuiltinId` (WASM Q32 overloads, etc.) |
| `lps-builtin-ids/src/lib.rs`                     | `BuiltinId` enum and helpers                                     |
| `lpvm-cranelift/src/generated_builtin_abi.rs`    | Cranelift lowering: symbol names and signatures                  |
| `lps-builtins-emu-app/src/builtin_refs.rs`       | Force-link all builtins for RV32 emu                             |
| `lps-builtins-wasm/src/builtin_refs.rs`          | Same for `wasm32` cdylib                                         |
| `lps-builtins/src/builtins/glsl/mod.rs`          | `mod` list for GLSL builtins                                     |
| `lps-builtins/src/builtins/lpir/mod.rs`          | `mod` list for LPIR builtins                                     |
| `lps-wasm/src/emit/builtin_wasm_import_types.rs` | WASM import typing for Q32 builtins                              |

Headers in generated files state that they are auto-generated and how to regenerate.

## Usage

From the **repository root** (workspace root):

```bash
cargo run -p lps-builtins-gen-app
```

The app resolves the `lp-shader/` directory (sibling layout under the workspace) and writes paths
relative to that layout.

`scripts/build-builtins.sh` runs this generator when builtin sources or the generator change (see
hash paths in that script), then builds `lps-builtins-emu-app` and `lps-builtins-wasm`.

## How discovery works

1. Walk `lps-builtins/src/builtins/{glsl,lpir,lpfx}/` for `#[unsafe(no_mangle)] pub extern "C"`
   functions (naming convention `__lp_q32_*` and related).
2. Parse with `syn`, build `BuiltinInfo` (symbol, module path, arity, etc.).
3. For LPFX, additional parsing under `builtins/lpfx/` feeds overload tables in
   `glsl_builtin_mapping.rs`.
4. Emit the files above, then run `cargo +nightly fmt` on the workspace root for the touched paths.

## Adding builtins

Implement in `lps-builtins`, then run `cargo run -p lps-builtins-gen-app` (or
`scripts/build-builtins.sh`). Do not hand-edit generated files.
