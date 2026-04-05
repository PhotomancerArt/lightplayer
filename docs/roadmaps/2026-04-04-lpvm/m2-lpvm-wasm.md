# M2: `lpvm-wasm`

## Goal

Build the WASM backend for LPVM. Done first because the WASM runtime model is
the strictest — wasmtime and browser APIs constrain the trait design.

## Naming / paths

- **Shader layer** uses **`lps-*`** (e.g. `lps-naga` for GLSL → LPIR).
- **Emission** today may still live under a transitional crate name
  (`lps-wasm`); **`lpvm-wasm`** is the target home for LPIR → `.wasm` +
  optional runtime.

If the repo already renamed WASM-related crates, align `Cargo.toml` paths with
reality; this doc uses **logical** crate names.

## Context for Agents

The current WASM stack typically:

- Emits `.wasm` from LPIR via **`wasm-encoder`** (`no_std` + alloc).
- Runs on desktop with **wasmtime** (often wired from **filetests**, not from
  the emission crate).
- **Browser**: `web-demo` (or similar) may call emission from WASM and hand
  bytes to JS.

`lpvm-wasm` adds **LPVM trait implementations** behind a **`runtime`** feature,
with native vs `wasm32` target selection (see roadmap overview).

### Typical current API (names may differ on disk)

- Top-level: GLSL → … → WASM bytes (may chain **`lps-naga`**).
- **`WasmModule`**: bytes + exports + shadow stack global info.
- **`WasmExport`**: WASM val types + **logical** param types (for harnesses) —
  those logical types should come from **`lps-shared`**, not from a “glsl” name.

### How wasmtime runner works (today: often in `lps-filetests`)

See `wasm_runner.rs` / `wasm_link.rs` (paths may be under `lps-filetests`
until renamed):

1. Obtain `WasmModule` (bytes + exports).
2. wasmtime `Engine` / `Store`, fuel.
3. Instantiate with builtins linked.
4. Per call: reset shadow stack global, set fuel, pass **`I32` VMContext** as
   first arg, flatten args, read results.

## What To Build

### Crate location

`lpvm/lpvm-wasm/`

### `Cargo.toml` (sketch)

Depend on **`lpvm`**, **`lpir`**, **`lps-shared`** (if export metadata needs
logical types), **`lps-builtin-ids`** (or transitional path), `wasm-encoder`,
optional `wasmtime` / `wasm-bindgen` / `web-sys` per target + `runtime` feature.

### Module structure

```
lpvm-wasm/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── emit.rs             # LPIR → WASM bytes
    ├── module.rs           # WasmModule, WasmExport
    ├── options.rs
    ├── error.rs
    ├── runtime_wasmtime.rs
    └── runtime_browser.rs  # optional / later
```

### Emission (always available)

Core entry: **`IrModule` → bytes**. The **shader frontend** (`lps-naga`) stays
out of this crate’s required path; optional feature can glue “source string →
WASM” for convenience.

### Runtime (`runtime` feature)

- **Native**: wasmtime — port logic from current filetests runner.
- **`wasm32`**: browser WebAssembly API — can trail wasmtime if needed.

### Unit tests

Emit minimal LPIR → instantiate (wasmtime) → call → assert. VMContext + shadow
stack behavior covered.

## WASM constraints vs traits

(Same as before — VMContext as i32 param, shadow stack, fuel, linear memory,
exports by name.) If traits from M1 cannot express these, **revise traits** in
M2 before M3.

## What NOT To Do

- Do NOT make GLSL source the **only** entry point; LPIR → WASM is the backend
  contract.
- Do NOT migrate **`lps-filetests`** yet (M5).
- Do NOT delete the old WASM emission crate until M7.

## Done When

- `lpvm-wasm` builds; emission + wasmtime runtime tests pass
- Traits validated against WASM; any M1 API gaps fixed
- Workspace builds
