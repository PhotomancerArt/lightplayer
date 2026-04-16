# M7 Cleanup: Design Overview

## Scope

Final cleanup milestone for LPVM2. Delete obsolete code, remove dead traits and
legacy crates, consolidate duplicate code paths, and verify everything builds
and passes across all targets.

## File Structure

```
lp-core/lp-engine/src/
├── gfx/
│   └── cranelift.rs          # UPDATE: use CraneliftEngine trait, not jit()

lp-shader/lpvm-cranelift/src/
├── lib.rs                    # UPDATE: remove JitModule/jit() exports
├── jit_module.rs             # DELETE (after lp-engine migrated)
├── compile.rs                # UPDATE: remove jit()/jit_from_ir() fns
├── lpvm_engine.rs            # KEEP (CraneliftEngine impl already here)
└── lpvm_module.rs            # KEEP (CraneliftModule impl already here)

lp-shader/lpvm-emu/src/
├── emu_run.rs                # DELETE (consolidate into EmuInstance)
├── lib.rs                    # UPDATE: remove emu_run exports
├── lpvm_engine.rs            # KEEP (EmuEngine)
├── lpvm_module.rs            # UPDATE: EmuModule with call/call_q32
└── instance.rs               # KEEP (EmuInstance)

lp-shader/lps-filetests/src/test_run/
├── mod.rs                    # UPDATE: remove wasm_link module
├── wasm_link.rs              # DELETE (migrate test to lpvm-wasm)
└── wasm_runner.rs            # UPDATE: use lpvm-wasm instead

lp-shader/lps-filetests/tests/
└── lpfx_builtins_memory.rs   # UPDATE: use lpvm-wasm instantiate/link

lp-shader/legacy/
├── lps-exec/                 # DELETE entire crate
├── lps-wasm/                 # DELETE entire crate
└── lps-builtins-wasm/        # DELETE entire crate

AGENTS.md                     # UPDATE: architecture diagram and key crates table
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         LightPlayer Stack                           │
├─────────────────────────────────────────────────────────────────────┤
│  Firmware (fw-esp32, fw-emu, future: fw-wasm)                       │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  LpServer with Arc<dyn LpGraphics>                           │   │
│  │  ├── ProjectRuntime                                          │   │
│  │  │   └── ShaderRuntime with Box<dyn LpShader>               │   │
│  │  │       └── CraneliftShader ──► DirectCall (pixel loop)      │   │
│  │  └── (future) WasmShader ───────► WasmInstance::call_q32     │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│  LPVM Traits (lp-shader/lpvm/src/lib.rs)                            │
│  ├── LpvmEngine::compile() ──► LpvmModule                           │
│  ├── LpvmModule ──► LpvmInstance + DirectCall (for JIT targets)    │
│  └── LpvmInstance::call()/call_q32() ──► shader execution          │
└─────────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│ lpvm-cranelift│    │  lpvm-emu     │    │  lpvm-wasm    │
│ (host/ESP32)  │    │  (RISC-V emu) │    │  (wasmtime)   │
├───────────────┤    ├───────────────┤    ├───────────────┤
│CraneliftEngine│    │  EmuEngine    │    │  WasmEngine   │
│CraneliftModule│    │  EmuModule    │    │  WasmModule   │
│CraneliftInst. │    │  EmuInstance  │    │  WasmInstance │
│  DirectCall    │    │               │    │               │
└───────────────┘    └───────────────┘    └───────────────┘
        │                     │                     │
        └─────────────────────┴─────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│  lps-filetests ──► Compiles GLSL, runs against ALL three backends    │
│  (uses LpvmEngine trait, not old GlslExecutable/JitModule APIs)    │
└─────────────────────────────────────────────────────────────────────┘
```

## Key Components

### LpvmEngine / LpvmModule / LpvmInstance Traits

- **Location:** `lp-shader/lpvm/src/lib.rs`
- **Purpose:** Uniform interface for shader compilation and execution across all backends
- **Cranelift:** `CraneliftEngine` → `CraneliftModule` (with `DirectCall` for JIT)
- **Emu:** `EmuEngine` → `EmuModule` → `EmuInstance`
- **WASM:** `WasmEngine` → `WasmModule` → `WasmInstance`

### LpGraphics / LpShader Traits (lp-engine)

- **Location:** `lp-core/lp-engine/src/gfx/`
- **Purpose:** Abstraction layer between engine and LPVM backends
- **CraneliftGraphics:** Implements `LpGraphics` using `CraneliftEngine`
- **CraneliftShader:** Implements `LpShader` using `DirectCall` for pixel loop

### Filetest Runner

- **Location:** `lp-shader/lps-filetests/src/`
- **Purpose:** Test GLSL shaders across all three backends
- **Uses:** `LpvmEngine` trait, no legacy `GlslExecutable` or `JitModule`

## Deletions

### Legacy Crates (lp-shader/legacy/)

| Crate               | Reason for Deletion                                                           |
| ------------------- | ----------------------------------------------------------------------------- |
| `lps-exec`          | `GlslExecutable` trait superseded by `LpvmEngine`/`LpvmModule`/`LpvmInstance` |
| `lps-wasm`          | Old WASM emitter superseded by `lpvm-wasm`                                    |
| `lps-builtins-wasm` | Old builtins WASM build, not used by new `lpvm-wasm`                          |

### Code to Delete

| File/Function                 | Replacement                       |
| ----------------------------- | --------------------------------- |
| `lpvm-cranelift::jit()`       | `CraneliftEngine::compile()`      |
| `lpvm-cranelift::JitModule`   | `CraneliftModule`                 |
| `lpvm-emu::emu_run.rs`        | `EmuInstance::call`/`call_q32`    |
| `lps-filetests::wasm_link.rs` | `lpvm-wasm` instantiate/link path |
| `lps-exec::GlslExecutable`    | `LpvmEngine` trait                |

## Main Interactions

1. **Firmware startup:** Creates `CraneliftGraphics` → `LpServer` → `ProjectRuntime`
2. **Shader compile:** `ShaderRuntime` calls `LpGraphics::compile_shader()` → backend-specific shader
3. **Shader render:** `LpShader::render()` contains pixel loop, calls `DirectCall` or `call_q32`
4. **Filetests:** Uses `LpvmEngine` implementations directly, no intermediate traits

## Verification

After cleanup:

- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server` ✓
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu` ✓
- `cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu` ✓
- `./scripts/glsl-filetests.sh --target rv32.q32c --target wasm.q32` ✓
- `cargo +nightly fmt --check` ✓
- No warnings in affected crates
