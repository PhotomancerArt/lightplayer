# M5: Migrate Filetests to LPVM — Design

## Scope

Port `lps-filetests` from `lps_exec::GlslExecutable` to **`LpvmEngine` → `LpvmModule` → `LpvmInstance`**. All three host backends (Cranelift JIT, WASM wasmtime, RV32 emulator) use the same trait surface.

**Also in scope (not covered by the completed Q32 restructure plan):**

- **`LpvmInstance::call_q32`** — flat `i32` ABI words in / out, reusing **`lpvm_abi`** semantics (no duplicate calling convention).
- **`LpvmInstance::debug_state`** — optional emulator-style diagnostics for failed runs.
- **Lifecycle fix:** one **engine** and one **compiled module** per **test file**; fresh **instance** per **test case** (line), so GLSL is not recompiled for every expectation.

**Out of scope:** M7 removal of `GlslExecutable` from `lps-exec`; new filetests for shared memory; perf tuning.

**Dependency:** [`lps-value-q32-restructure`](../2026-04-07-lps-value-q32-restructure/00-design.md) is **done** (`LpsValueQ32`, `lpvm_abi`).

## File structure

```
lp-shader/lpvm/src/
├── instance.rs                 # UPDATE: call_q32 + debug_state on LpvmInstance
└── lpvm_abi.rs                 # EXISTING: flatten/decode (call_q32 uses this)

lp-shader/lpvm-cranelift/src/
├── lpvm_instance.rs           # UPDATE: call_q32 (exact), debug_state
└── ...

lp-shader/lpvm-emu/src/
├── instance.rs                # UPDATE: call_q32 (exact), debug_state (rich)
└── ...

lp-shader/lpvm-wasm/src/
├── rt_wasmtime/instance.rs    # UPDATE: call_q32 default, debug_state None
└── rt_browser/instance.rs     # UPDATE: same defaults (parity)

lp-shader/lps-filetests/src/test_run/
├── engine.rs                  # NEW: per-backend context, compile once per file
├── execution.rs               # UPDATE: LpvmInstance, call vs call_q32, debug_state
├── run_detail.rs              # UPDATE: wire engine/module lifecycle
├── compile.rs                 # UPDATE: produce IrModule + meta once per file
├── q32_exec_common.rs         # UPDATE/SHRINK: thin glue over call_q32 + decode
├── lpir_jit_executable.rs     # DELETE when redundant
├── lpir_rv32_executable.rs    # DELETE when redundant
├── wasm_runner.rs             # DELETE or shrink
└── wasm_link.rs               # DELETE or shrink
```

## Conceptual architecture

```
Per test FILE (.glsl)
  │
  ├─► FiletestEngine (Cranelift | Emu | Wasm)     ← created once
  │
  ├─► compile(ir, meta) → LpvmModule               ← once per file
  │
  └─► for each test CASE (line)
        instantiate() → LpvmInstance
        │
        ├─► .f32 target: instance.call(&[LpsValueF32]) → LpsValueF32
        │
        └─► .q32 target: instance.call_q32(&[i32]) → Vec<i32>
              (same word order as flatten_q32_arg chain)

On error: append instance.debug_state() if Some(_)
```

## Main components

| Piece | Role |
|-------|------|
| `LpvmInstance::call` | Existing F32-oriented API; unchanged contract. |
| `LpvmInstance::call_q32` | Q32 targets: args/ret as **flat `i32` words**; impl delegates to existing machine path + `decode_q32_return` flattened to words (or thin wrapper). |
| `LpvmInstance::debug_state` | RV32: registers / PC / trap info; others: `None`. |
| `FiletestEngine` (or equivalent) | Selects `CraneliftEngine` / `EmuEngine` / `WasmLpvmEngine` + options from target string. |
| `q32_exec_common` | After migration: helpers to build flat args from parsed expectations and compare decoded `LpsValueQ32` — not a second ABI. |

## Object safety note

`LpvmEngine` / `LpvmModule` use associated types; **`Box<dyn LpvmModule>` may not be viable**. Prefer a **backend enum** (`FiletestModule`) holding `CraneliftModule \| EmuModule \| WasmLpvmModule` with `match` dispatch to `instantiate` / `call` / `call_q32`, or a small **filetest-local trait** that erases to concrete modules. Phase 05 resolves this in code.

## Phases

**Consolidated write-up:** [`PHASES.md`](./PHASES.md).

Numbered detail files:

1. `01-phase-lpvm-instance-call-q32.md` — trait + default bodies in `lpvm`
2. `02-phase-cranelift-call-q32.md` — `CraneliftInstance`
3. `03-phase-emu-call-q32.md` — `EmuInstance` + `debug_state`
4. `04-phase-wasm-call-q32.md` — wasmtime + browser instances
5. `05-phase-filetests-engine-module.md` — engine + module per file, dispatch enum
6. `06-phase-filetests-execution.md` — `execution.rs`, `call` / `call_q32`, errors
7. `07-phase-remove-glsl-executable-wrappers.md` — delete redundant files, thin `q32_exec_common`
8. `08-phase-cleanup-validation.md` — full filetest matrix, `summary.md`, move to `plans-done`
