# M2: `lpvm-wasm`

## Goal

Build the WASM backend for LPVM. This is done first because the WASM runtime
model is the strictest — we don't control wasmtime or browser WebAssembly APIs,
so the trait design must accommodate their constraints. Issues found here inform
trait adjustments before building other backends.

## Context for Agents

The current WASM implementation lives in `lp-glsl-wasm`. It emits `.wasm` bytes
from LPIR using `wasm-encoder`. It does NOT run WASM — that's done by wasmtime
in `lp-glsl-filetests` (desktop) or by the browser in `web-demo`.

`lpvm-wasm` will contain both emission AND runtime (behind a `runtime` feature).

### Current `lp-glsl-wasm` API

- `glsl_wasm(source, options) -> Result<WasmModule, GlslWasmError>` — top-level
  entry: GLSL → naga → LPIR → WASM bytes.
- `WasmModule` — holds `bytes: Vec<u8>`, `exports: Vec<WasmExport>`,
  `shadow_stack_base: Option<i32>`.
- `WasmExport` — name, WASM param/result types, GLSL param types, return type.
- `WasmOptions` — `float_mode` (default Q32).
- Also re-exports `CompileError`, `FloatMode`, `GlslType` from naga.

### How the wasmtime runner works (in filetests)

`WasmExecutable` in `lp-glsl-filetests/src/test_run/wasm_runner.rs`:

1. Calls `glsl_wasm()` to get `WasmModule`
2. Creates wasmtime `Engine` with `consume_fuel` enabled
3. Creates `Store` with fuel config
4. Calls `wasm_link::instantiate_wasm_module` to link builtins + instantiate
5. Before each call: resets shadow stack global, sets fuel
6. Calls exported function with `Val::I32(0)` as VMContext pointer + flattened args
7. Reads return values from WASM call results

Key details:
- First param to every exported function is `I32` for VMContext pointer
- Shadow stack global (`__lp_shadow_sp`) must be reset before each call
- Fuel is used for execution limits

## What To Build

### Crate location

`lpvm/lpvm-wasm/`

### Cargo.toml structure

```toml
[package]
name = "lpvm-wasm"
version = "0.1.0"
edition = "2024"

[dependencies]
lpvm = { path = "../lpvm", default-features = false }
lpir = { path = "../../lp-glsl/lpir", default-features = false }
lp-glsl-builtin-ids = { path = "../../lp-glsl/lp-glsl-builtin-ids" }
wasm-encoder = "..."
log = "..."

# Runtime deps (only with runtime feature)
wasmtime = { version = "42", optional = true }
wasm-bindgen = { version = "...", optional = true }
js-sys = { version = "...", optional = true }
web-sys = { version = "...", features = ["WebAssembly", ...], optional = true }

[features]
default = ["std"]
std = ["lpvm/std"]
runtime = []  # Enables Module/Instance implementations

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
# wasmtime is only available on native targets
# Wire this up so runtime feature on native pulls in wasmtime

[target.'cfg(target_arch = "wasm32")'.dependencies]
# wasm-bindgen/js-sys/web-sys for browser runtime
```

Note: the exact dependency wiring for target-conditional runtime deps needs
care. The `runtime` feature enables the trait implementations; the target arch
determines which backing implementation is compiled.

### Module structure

```
lpvm-wasm/
├── Cargo.toml
└── src/
    ├── lib.rs              # Re-exports, feature gates
    ├── emit.rs             # LPIR → WASM bytes (extracted from lp-glsl-wasm)
    ├── module.rs           # WasmModule, WasmExport types
    ├── options.rs          # Emission options
    ├── error.rs            # Error types
    ├── runtime_wasmtime.rs # LpvmModule/Instance impl via wasmtime (cfg native + runtime)
    └── runtime_browser.rs  # LpvmModule/Instance impl via browser API (cfg wasm32 + runtime)
```

### Emission (always available)

Extract from `lp-glsl-wasm`. The emission code is `no_std + alloc` and produces
`.wasm` bytes from an `IrModule`.

Important: the current `glsl_wasm()` entry point takes GLSL source and calls
naga internally. For `lpvm-wasm`, the entry point should take `IrModule` (LPIR),
not GLSL source. The GLSL → LPIR step is the frontend's job, not the backend's.

Provide both:
- `emit_wasm(ir: &IrModule, options: &WasmOptions) -> Result<WasmModule, ...>` — takes LPIR
- Optionally re-export or provide a convenience that chains with naga (behind a
  feature flag), but the core API is LPIR → WASM.

### Runtime — wasmtime (native, behind `runtime` feature)

Implement `LpvmModule` and `LpvmInstance` using wasmtime. Extract logic from
`lp-glsl-filetests/src/test_run/wasm_runner.rs` and `wasm_link.rs`.

Key behaviors to preserve:
- VMContext as first parameter (i32) to all exported functions
- Shadow stack global reset before each call
- Fuel-based execution limits
- Builtin function linking

### Runtime — browser (wasm32, behind `runtime` feature)

Implement `LpvmModule` and `LpvmInstance` using `wasm-bindgen` + browser
`WebAssembly` API. This is new code — `web-demo` currently does this from JS.

This can be deferred to a later pass if the browser path is not immediately
needed. The wasmtime path is sufficient for validation.

### Unit tests

Basic sanity tests using wasmtime (dev-dependency):
- Emit a simple LPIR module to WASM bytes
- Instantiate via the runtime
- Call a function and verify the result
- Test VMContext passing
- Test shadow stack reset between calls

## WASM-Specific Constraints That Affect Trait Design

These are things the agent should flag if the `LpvmModule`/`LpvmInstance` traits
from M1 don't accommodate:

1. **VMContext is a parameter, not a register**: In WASM, VMContext is passed as
   the first i32 parameter to every function. In JIT, it's in a dedicated
   register. The trait API should not assume either model.

2. **Shadow stack reset**: WASM uses a global for the shadow stack pointer that
   must be reset between calls. This is an instance-level reset, analogous to
   resetting global state in JIT. The trait should have a "reset" or "prepare
   for call" mechanism.

3. **Fuel model**: WASM uses wasmtime's built-in fuel. JIT uses VMContext fuel
   field. The trait should abstract fuel/execution limits.

4. **Memory model**: WASM linear memory is managed by the WASM runtime, not by
   us directly. `LpvmMemory` for WASM wraps the WASM memory, which has
   different growth semantics than a raw buffer.

5. **No direct function pointers**: In WASM, you call exports by name. In JIT,
   you call function pointers. The trait should use names or handles, not
   raw pointers.

## What NOT To Do

- Do NOT keep the `glsl_wasm(source)` API as the primary entry point. The
  backend takes LPIR, not GLSL source.
- Do NOT implement the browser runtime if it would delay the milestone
  significantly. Wasmtime path first, browser path can follow.
- Do NOT modify `lp-glsl-filetests` to use this yet. That's M5.
- Do NOT delete `lp-glsl-wasm` yet. It coexists until M5/M7.

## Done When

- `lpvm-wasm` crate exists at `lpvm/lpvm-wasm/`
- Emission works: LPIR → .wasm bytes
- `LpvmModule`/`LpvmInstance` implemented for wasmtime (native, behind
  `runtime` feature)
- Unit tests pass: emit, instantiate, call, verify results
- Any trait design issues surfaced and resolved (possibly requiring changes
  back in `lpvm`)
- Workspace builds pass
