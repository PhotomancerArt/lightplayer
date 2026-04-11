# Phase 8: Update AGENTS.md Documentation

## Scope

Update the architecture diagram and key crates table in `AGENTS.md` to reflect
the new LPVM trait-based architecture.

## Current State

### Current Architecture Diagram (in AGENTS.md)

```
GLSL source (on-flash filesystem)
        │
        ▼
lps-frontend (no_std + alloc) ── parses GLSL via naga
        │
        ▼
LPIR (LightPlayer IR)
        │
        ▼
lpvm-cranelift (no_std + alloc) ── Cranelift codegen → RISC-V machine code
        │
        ▼
JIT buffer in RAM ── direct function call
        │
        ▼
LED output
```

### Current Key Crates Table


| Crate            | Role                                   | `no_std`         |
| ---------------- | -------------------------------------- | ---------------- |
| `lps-frontend`   | GLSL → LPIR (via naga)                 | yes              |
| `lpvm-cranelift` | LPIR → Cranelift → machine code        | yes              |
| `lp-engine`      | Shader runtime, node graph             | yes              |
| `lp-server`      | Project management, client connections | yes              |
| `fw-esp32`       | ESP32 firmware                         | yes (bare metal) |
| `fw-emu`         | RISC-V emulator firmware (CI)          | yes (bare metal) |


## Required Updates

### 1. New Architecture Diagram

Show the LPVM trait abstraction layer:

```
GLSL source (on-flash filesystem)
        │
        ▼
lps-frontend (no_std + alloc) ── parses GLSL via naga
        │
        ▼
LPIR (LightPlayer IR)
        │
        ▼
┌─────────────────────────────────────────────────────────┐
│ LPVM Traits: LpvmEngine → LpvmModule → LpvmInstance    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │
│  │lpvm-cranelift│  │  lpvm-emu   │  │  lpvm-wasm  │   │
│  │  (JIT host)  │  │ (RISC-V emu)│  │  (wasmtime) │   │
│  │  (JIT ESP32) │  │             │  │             │   │
│  └──────────────┘  └──────────────┘  └──────────────┘   │
└─────────────────────────────────────────────────────────┘
        │
        ▼
JIT buffer / Emulator / WASM runtime ── execute shader
        │
        ▼
LED output
```

### 2. Updated Key Crates Table


| Crate            | Role                                   | `no_std`         | Traits                               |
| ---------------- | -------------------------------------- | ---------------- | ------------------------------------ |
| `lps-frontend`   | GLSL → LPIR (via naga)                 | yes              | -                                    |
| `lpvm`           | Core traits (LpvmEngine, etc.)         | yes              | LpvmEngine, LpvmModule, LpvmInstance |
| `lpvm-cranelift` | LPIR → Cranelift → machine code        | yes              | implements LPVM traits               |
| `lpvm-emu`       | LPIR → RISC-V emulator                 | yes              | implements LPVM traits               |
| `lpvm-wasm`      | LPIR → WASM → wasmtime                 | no (std)         | implements LPVM traits               |
| `lp-engine`      | Shader runtime, node graph             | yes              | Uses LPVM via LpGraphics             |
| `lp-server`      | Project management, client connections | yes              | -                                    |
| `fw-esp32`       | ESP32 firmware                         | yes (bare metal) | Uses CraneliftGraphics               |
| `fw-emu`         | RISC-V emulator firmware (CI)          | yes (bare metal) | Uses CraneliftGraphics               |


### 3. Update Validation Commands

If any validation commands are listed, update them:

```bash
# New validation commands for LPVM architecture:
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
cargo test -p lps-filetests  # Uses all three LPVM backends
```

### 4. Add Trait-Based Architecture Explanation

Add a new section explaining the trait abstraction:

```markdown
## LPVM Trait Architecture

The shader compilation and execution pipeline is abstracted through three traits:

- **`LpvmEngine`**: Compiles LPIR modules. Implemented by `CraneliftEngine`,
  `EmuEngine`, and `WasmEngine`.
- **`LpvmModule`**: A compiled shader artifact. Provides `direct_call` for JIT
  targets and `instantiate` for runtime-interpreted targets.
- **`LpvmInstance`**: An instantiated shader with `call`/`call_q32` methods.

This allows `lps-filetests` to run identical GLSL tests against all three
backends (native JIT, emulated RISC-V, WASM) using the same test code.
```

## Code Changes

File: `/Users/yona/dev/photomancer/lp2025/AGENTS.md`

- Update architecture diagram (lines ~66-88)
- Update key crates table (lines ~90-100)
- Add trait architecture section
- Verify validation commands are current

## Code Organization Reminders

- Keep the core message: on-device JIT is the product
- Show that the trait abstraction enables testing without compromising embedded
- Don't make the diagram too complex - keep the core pipeline clear

## Validate

```bash
# Just read and verify
cat AGENTS.md | head -100
```

## Phase Notes

- This is documentation only - no code changes
- The diagram should show the multi-backend capability clearly
- Mention that `lp-engine` uses `LpGraphics` which abstracts over LPVM traits

